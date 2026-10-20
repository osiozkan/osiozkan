//! Automation in the editor: macro availability (validation / is_active, as Aegisub greys out
//! and checks menu items), reloading scripts when they change on disk, and the script manager.

use super::*;
use std::collections::HashMap;
use std::time::{Instant, SystemTime};
use yuzu_lua::MacroState;

pub(super) struct AutoState {
    /// (script, macro) → state for the request `token_got`.
    pub states: HashMap<(usize, usize), MacroState>,
    pub token_sent: u64,
    /// Files the loaded scripts depend on, with their modification times and sizes.
    pub watch: Vec<Watched>,
    pub last_check: Instant,
    /// Script picked in the manager.
    pub picked: Option<usize>,
}

impl Default for AutoState {
    fn default() -> Self { AutoState { states: HashMap::new(), token_sent: 0, watch: vec![], last_check: Instant::now(), picked: None } }
}

pub(super) type Watched = (PathBuf, Option<SystemTime>, u64);

fn stamp(p: PathBuf) -> Watched {
    let m = std::fs::metadata(&p).ok();
    let (t, n) = (m.as_ref().and_then(|m| m.modified().ok()), m.map(|m| m.len()).unwrap_or(0));
    (p, t, n)
}

/// Lua files of the script folders and of their include folders (a few levels deep).
pub(super) fn watched_files(dirs: &[PathBuf]) -> Vec<Watched> {
    fn walk(d: &std::path::Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            if out.len() > 4000 { return; }
            let p = e.path();
            if p.is_dir() { if depth > 0 { walk(&p, depth - 1, out); } }
            else if p.extension().map(|x| x.eq_ignore_ascii_case("lua") || x.eq_ignore_ascii_case("moon")).unwrap_or(false) { out.push(p); }
        }
    }
    let mut files = Vec::new();
    for d in dirs {
        walk(d, 0, &mut files);
        if let Some(parent) = d.parent() { walk(&parent.join("include"), 3, &mut files); }
    }
    files.sort();
    files.dedup();
    files.into_iter().map(stamp).collect()
}

impl App {
    fn states_token(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.doc_gen, &self.selection, self.active, self.scripts.len()).hash(&mut h);
        h.finish()
    }

    /// Ask the automation thread which macros are available (when a script has validators).
    pub(super) fn request_macro_states(&mut self) {
        if self.busy.is_some() || !self.scripts.iter().any(|s| s.macros.iter().any(|m| m.has_validate || m.has_active)) { return; }
        let token = self.states_token();
        if token == self.auto.token_sent { return; }
        self.auto.token_sent = token;
        let _ = self.worker.tx.send(Cmd::Validate { doc: Box::new(self.doc.clone()), sel: self.sel_vec(), active: Some(self.active), project: self.project_info(), token });
    }

    pub(super) fn got_macro_states(&mut self, token: u64, states: Vec<(usize, Vec<MacroState>)>) {
        if token != self.auto.token_sent { return; }
        self.auto.states.clear();
        for (si, v) in states { for (mi, st) in v.into_iter().enumerate() { self.auto.states.insert((si, mi), st); } }
    }

    /// Known state of a macro for the current selection (None: not asked, or no validator).
    pub(super) fn macro_state(&self, si: usize, mi: usize) -> Option<MacroState> {
        (self.auto.token_sent == self.states_token()).then(|| self.auto.states.get(&(si, mi)).copied()).flatten()
    }

    /// Remember what the loaded scripts depend on (called when scripts were loaded).
    pub(super) fn reset_script_watch(&mut self) {
        self.auto.watch = watched_files(&self.settings.script_dirs);
        self.auto.last_check = Instant::now();
        self.auto.token_sent = 0;
        self.auto.states.clear();
    }

    /// Every 1.5 s: reload the scripts when a script or library changed, appeared or went away.
    pub(super) fn watch_scripts(&mut self) {
        if !self.settings.auto_reload || self.busy.is_some() || self.auto.last_check.elapsed().as_millis() < 1500 { return; }
        self.auto.last_check = Instant::now();
        let now = watched_files(&self.settings.script_dirs);
        if now == self.auto.watch { return; }
        let changed: Vec<String> = now.iter().filter(|x| !self.auto.watch.contains(x)).chain(self.auto.watch.iter().filter(|x| !now.contains(x)))
            .filter_map(|(p, _, _)| p.file_name().map(|n| n.to_string_lossy().to_string())).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        self.auto.watch = now;
        self.reload_scripts();
        self.last_status = tf!("Scripts changed ({}), reloading", changed.join(", "));
    }

    pub(super) fn automation_manager_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(t("Reload all")).clicked() { self.reload_scripts(); }
            if ui.button(t("Add script folder…")).clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() { self.settings.script_dirs.push(d); self.settings.save(); self.reload_scripts(); }
            }
            if ui.checkbox(&mut self.settings.auto_reload, t("Reload when changed")).changed() { self.settings.save(); }
            let errs = self.scripts.iter().filter(|s| s.error.is_some()).count();
            ui.weak(tf!("{} scripts · {} macros · {} failed", self.scripts.len(), self.scripts.iter().map(|s| s.macros.len()).sum::<usize>(), errs));
        });
        let mut pick = None;
        ui.push_id("auto_scripts", |ui| {
            // cell texts must not be selectable, or they swallow the click meant for the row
            ui.style_mut().interaction.selectable_labels = false;
            TableBuilder::new(ui).striped(true).max_scroll_height(260.0).auto_shrink([false, true]).sense(egui::Sense::click())
                .column(Column::initial(200.0).clip(true)).column(Column::exact(70.0)).column(Column::initial(120.0).clip(true)).column(Column::exact(60.0)).column(Column::remainder().clip(true))
                .header(20.0, |mut h| { for n in [t("Script"), t("Version"), t("Author"), t("Macros"), t("File")] { h.col(|ui| { ui.strong(n); }); } })
                .body(|body| body.rows(20.0, self.scripts.len(), |mut row| {
                    let k = row.index();
                    let s = &self.scripts[k];
                    row.set_selected(self.auto.picked == Some(k));
                    row.col(|ui| {
                        if s.error.is_some() { ui.colored_label(Color32::from_rgb(255, 110, 100), format!("⚠ {}", s.name)); } else { ui.label(&s.name); }
                    });
                    row.col(|ui| { ui.label(&s.version); });
                    row.col(|ui| { ui.label(&s.author); });
                    row.col(|ui| { ui.label(s.macros.len().to_string()); });
                    row.col(|ui| { ui.weak(s.path.display().to_string()); });
                    if row.response().clicked() { pick = Some(k); }
                }));
        });
        if pick.is_some() { self.auto.picked = pick; }
        let Some(s) = self.auto.picked.and_then(|k| self.scripts.get(k)) else { ui.weak(t("Pick a script to see its macros or its error.")); return };
        ui.separator();
        ui.strong(&s.name);
        if !s.description.is_empty() { ui.label(&s.description); }
        match &s.error {
            Some(e) => {
                egui::ScrollArea::vertical().max_height(220.0).id_salt("auto_err").show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(e).monospace().color(Color32::from_rgb(255, 140, 120))).wrap());
                });
            }
            None => {
                for m in &s.macros {
                    let mut flags = Vec::new();
                    if m.has_validate { flags.push(t("validates")); }
                    if m.has_active { flags.push(t("checkable")); }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&m.name).strong());
                        if !m.description.is_empty() { ui.weak(&m.description); }
                        if !flags.is_empty() { ui.weak(format!("({})", flags.join(", "))); }
                    });
                }
            }
        }
    }
}
