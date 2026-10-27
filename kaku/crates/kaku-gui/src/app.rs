use crate::config::{self, Config};
use crate::highlight;
use crate::i18n::t;
use crate::media_pane::{Event as MEvent, LineSpan, MediaPane};
use egui::{Color32, Key, Modifiers, RichText};
use egui_extras::{Column, TableBuilder};
use kaku_core::ass::{Entry, Kind, Line, Script};
use kaku_core::history::History;
use kaku_core::search::{self, Field, Query};
use kaku_core::time::{format_ass_time, parse_ass_time};
use kaku_core::{formats, karaoke, ops, qc, tags};
use kaku_lua::{DialogAnswer, DialogSpec, Kind as LKind, Registered, Runtime};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Default)]
struct FindState {
    open: bool,
    pattern: String,
    replace: String,
    regex: bool,
    case: bool,
    word: bool,
    field: usize, // 0 text, 1 text w/o tags, 2 style, 3 actor, 4 effect
}

struct PendingMacro {
    reg: Registered,
    answers: Vec<DialogAnswer>,
    spec: DialogSpec,
    values: HashMap<String, String>,
}

pub struct KakuApp {
    script: Script,
    path: Option<PathBuf>,
    dirty: bool,
    history: History,
    /// entry index of every event line
    ev: Vec<usize>,
    sel: BTreeSet<usize>,
    active: Option<usize>,
    anchor: usize,
    filter: String,
    issues: Vec<qc::Issue>,
    qc_cfg: qc::QcConfig,
    qc_dirty: bool,
    status: String,
    find: FindState,
    shift_open: bool,
    shift_ms: i64,
    resample_open: bool,
    resample_xy: (i32, i32),
    tpp_open: bool,
    tpp: (i64, i64, i64),
    balance_max: usize,
    style_rename: String,
    fonts_open: bool,
    pick_pos: bool,
    cfg: Config,
    last_autosave: f64,
    fonts_cache: Vec<kaku_core::fonts::FontStatus>,
    turkish: bool,
    side_tab: usize,
    rt: Option<Runtime>,
    macros: Vec<Registered>,
    lua_paths: Vec<PathBuf>,
    pending: Option<PendingMacro>,
    log_window: Option<String>,
    scroll_to: Option<usize>,
    style_edit: usize,
    time_buf: (usize, String, String),
    pub media: MediaPane,
    rev: u64,
    frame_key: (Option<usize>, u64),
    frame_key_time: f64,
    frame_requested: (Option<usize>, u64),
    last_active: Option<usize>,
}

impl KakuApp {
    pub fn new() -> Self {
        let mut a = KakuApp {
            script: Script::new_default(),
            path: None,
            dirty: false,
            history: History::new(200),
            ev: vec![],
            sel: BTreeSet::new(),
            active: None,
            anchor: 0,
            filter: String::new(),
            issues: vec![],
            qc_cfg: {
                let c = Config::load();
                qc::QcConfig { max_cps: c.max_cps, max_line_chars: c.max_chars, ..Default::default() }
            },
            qc_dirty: true,
            status: "Welcome to Kaku — File ▸ Open (Ctrl+O)".into(),
            find: FindState::default(),
            shift_open: false,
            shift_ms: 0,
            resample_open: false,
            resample_xy: (1920, 1080),
            tpp_open: false,
            tpp: (120, 400, 300),
            balance_max: 42,
            style_rename: String::new(),
            fonts_open: false,
            pick_pos: false,
            cfg: Config::load(),
            last_autosave: 0.0,
            fonts_cache: vec![],
            turkish: true,
            side_tab: 0,
            rt: None,
            macros: vec![],
            lua_paths: vec![],
            pending: None,
            log_window: None,
            scroll_to: None,
            style_edit: 0,
            time_buf: (usize::MAX, String::new(), String::new()),
            media: MediaPane::new(),
            rev: 0,
            frame_key: (None, 0),
            frame_key_time: 0.0,
            frame_requested: (None, u64::MAX),
            last_active: None,
        };
        a.reindex();
        a.ensure_runtime();
        a.autoload_scripts();
        a
    }

    fn autoload_scripts(&mut self) {
        // ~/.kaku/automation/*.lua and ./automation/*.lua
        let mut dirs = vec![PathBuf::from("automation")];
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(h).join(".kaku/automation"));
        }
        for d in dirs {
            if let Ok(rd) = std::fs::read_dir(&d) {
                let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e == "lua")).collect();
                files.sort();
                for f in files {
                    self.load_lua(&f);
                }
            }
        }
    }

    fn ensure_runtime(&mut self) {
        if self.rt.is_none() {
            match Runtime::new() {
                Ok(r) => {
                    if let Some(h) = std::env::var_os("HOME") {
                        r.add_include_dir(PathBuf::from(h).join(".kaku/automation/include"));
                    }
                    r.add_include_dir("automation/include");
                    if let Ok(b) = r.load_builtins() {
                        self.macros.extend(b);
                    }
                    self.rt = Some(r);
                }
                Err(e) => {
                    self.status = format!("Lua init failed: {e}");
                }
            }
        }
    }

    fn load_lua(&mut self, path: &Path) {
        self.ensure_runtime();
        let Some(rt) = self.rt.as_ref() else { return };
        match rt.load_script(path) {
            Ok(r) => {
                self.status = format!("Loaded {} ({} macros)", path.display(), r.len());
                self.macros.extend(r);
                self.lua_paths.push(path.to_path_buf());
            }
            Err(e) => self.log_window = Some(format!("Failed to load {}:\n{e}", path.display())),
        }
    }

    pub fn open_path(&mut self, p: &Path) {
        match std::fs::read(p) {
            Ok(b) => {
                self.script = formats::load(&p.to_string_lossy(), &b);
                self.path = Some(p.to_path_buf());
                self.dirty = false;
                self.history = History::new(200);
                self.sel.clear();
                self.active = None;
                self.reindex();
                if !self.ev.is_empty() {
                    self.select_only(0);
                }
                self.cfg.max_cps = self.qc_cfg.max_cps;
                self.cfg.max_chars = self.qc_cfg.max_line_chars;
                self.cfg.push_recent(p);
                self.status = format!("Opened {} — {} lines", p.display(), self.ev.len());
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }

    fn autosave_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into())
    }

    fn autosave_tick(&mut self, now: f64) {
        if !self.dirty || self.cfg.autosave_secs == 0 || now - self.last_autosave < self.cfg.autosave_secs as f64 {
            return;
        }
        self.last_autosave = now;
        if let Some(p) = config::autosave_path(&self.autosave_name()) {
            let _ = std::fs::create_dir_all(p.parent().unwrap());
            if std::fs::write(&p, self.script.to_string()).is_ok() {
                self.status = format!("Autosaved {}", p.display());
            }
        }
    }

    pub fn goto_line(&mut self, i: usize) {
        if i < self.ev.len() {
            self.select_only(i);
            self.scroll_to = Some(i);
        }
    }

    fn save_to(&mut self, p: &Path) {
        let data = formats::save(&p.to_string_lossy(), &self.script);
        match std::fs::write(p, data) {
            Ok(_) => {
                self.path = Some(p.to_path_buf());
                self.dirty = false;
                self.cfg.max_cps = self.qc_cfg.max_cps;
                self.cfg.max_chars = self.qc_cfg.max_line_chars;
                self.cfg.push_recent(p);
                if let Some(a) = config::autosave_path(&self.autosave_name()) {
                    let _ = std::fs::remove_file(a);
                }
                self.status = format!("Saved {}", p.display());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    fn reindex(&mut self) {
        self.ev = self.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Line(_))).map(|(i, _)| i).collect();
        self.qc_dirty = true;
        let n = self.ev.len();
        self.sel.retain(|&i| i < n);
        if self.active.map_or(false, |a| a >= n) {
            self.active = n.checked_sub(1);
        }
    }

    /// Run a mutation with undo support.
    fn edit(&mut self, label: &str, f: impl FnOnce(&mut Script)) {
        self.history.record(label, &self.script);
        f(&mut self.script);
        self.dirty = true;
        self.reindex();
    }

    fn line(&self, i: usize) -> Option<&Line> {
        match self.script.entries.get(*self.ev.get(i)?) {
            Some(Entry::Line(l)) => Some(l),
            _ => None,
        }
    }
    fn line_mut(&mut self, i: usize) -> Option<&mut Line> {
        let ei = *self.ev.get(i)?;
        match self.script.entries.get_mut(ei) {
            Some(Entry::Line(l)) => Some(l),
            _ => None,
        }
    }

    fn select_only(&mut self, i: usize) {
        self.sel.clear();
        self.sel.insert(i);
        self.active = Some(i);
        self.anchor = i;
    }

    fn selected_vec(&self) -> Vec<usize> {
        self.sel.iter().copied().collect()
    }

    // ---------- operations ----------

    fn undo(&mut self) {
        if let Some(l) = self.history.undo(&mut self.script) {
            self.status = format!("Undo: {l}");
            self.dirty = true;
            self.reindex();
        }
    }
    fn redo(&mut self) {
        if let Some(l) = self.history.redo(&mut self.script) {
            self.status = format!("Redo: {l}");
            self.dirty = true;
            self.reindex();
        }
    }

    fn insert_line_after_active(&mut self) {
        let at = self.active.map(|a| a + 1).unwrap_or(self.ev.len());
        let (start, style) = match self.active.and_then(|a| self.line(a)) {
            Some(l) => (l.end, l.style.clone()),
            None => (0, "Default".into()),
        };
        let entry_pos = self.ev.get(at).copied().or_else(|| self.ev.last().map(|l| l + 1)).unwrap_or(self.script.entries.len());
        self.edit("Insert line", |s| {
            let mut l = Line::new(start, start + 2000, "");
            l.style = style;
            s.entries.insert(entry_pos, Entry::Line(l));
        });
        self.select_only(at);
        self.scroll_to = Some(at);
    }

    fn delete_selected(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        let idx: BTreeSet<usize> = self.sel.iter().filter_map(|&i| self.ev.get(i).copied()).collect();
        let first = *self.sel.iter().next().unwrap();
        self.edit("Delete lines", |s| {
            let mut i = 0;
            s.entries.retain(|_| {
                let keep = !idx.contains(&i);
                i += 1;
                keep
            });
        });
        self.sel.clear();
        if !self.ev.is_empty() {
            self.select_only(first.min(self.ev.len() - 1));
        } else {
            self.active = None;
        }
    }

    fn duplicate_selected(&mut self) {
        let sel = self.selected_vec();
        if sel.is_empty() {
            return;
        }
        let last_entry = self.ev[*sel.last().unwrap()];
        let copies: Vec<Entry> = sel.iter().filter_map(|&i| self.line(i).cloned()).map(Entry::Line).collect();
        self.edit("Duplicate", |s| {
            for (k, c) in copies.into_iter().enumerate() {
                s.entries.insert(last_entry + 1 + k, c);
            }
        });
    }

    fn split_active(&mut self) {
        let Some(a) = self.active else { return };
        let Some(l) = self.line(a).cloned() else { return };
        let mid = (l.start + l.end) / 2;
        // Split at middle of visible text on a space if possible.
        let t = &l.text;
        let target = t.len() / 2;
        let idx = t.char_indices().map(|(i, _)| i).filter(|&i| t[i..].starts_with(' ')).min_by_key(|&i| (i as i64 - target as i64).abs()).unwrap_or(target.min(t.len()));
        let idx = (0..=idx).rev().find(|&i| t.is_char_boundary(i)).unwrap_or(0);
        if let Some((x, y)) = ops::split_line(&l, mid, idx) {
            let ei = self.ev[a];
            self.edit("Split line", |s| {
                s.entries[ei] = Entry::Line(x);
                s.entries.insert(ei + 1, Entry::Line(y));
            });
        }
    }

    fn merge_selected(&mut self) {
        let sel = self.selected_vec();
        if sel.len() < 2 || sel.windows(2).any(|w| w[1] != w[0] + 1) {
            self.status = "Select consecutive lines to merge".into();
            return;
        }
        let (first, n) = (sel[0], sel.len());
        self.edit("Merge lines", |s| ops::merge_lines(s, first, n, "\\N"));
        self.select_only(first);
    }

    fn run_find(&mut self, replace: bool, all: bool) {
        let field = [Field::Text, Field::TextNoTags, Field::Style, Field::Actor, Field::Effect][self.find.field.min(4)];
        let q = match Query::new(&self.find.pattern, self.find.regex, self.find.case, self.find.word, field) {
            Ok(q) => q,
            Err(e) => {
                self.status = format!("Bad pattern: {e}");
                return;
            }
        };
        if replace && all {
            let with = self.find.replace.clone();
            self.history.record("Replace all", &self.script);
            let n = search::replace(&mut self.script, &q, &with);
            self.dirty = n > 0;
            self.reindex();
            self.status = format!("Replaced in {n} line(s)");
            return;
        }
        let hits = search::find(&self.script, &q);
        if hits.is_empty() {
            self.status = "Not found".into();
            return;
        }
        let from = self.active.map(|a| a + 1).unwrap_or(0);
        let h = hits.iter().copied().find(|&h| h >= from).unwrap_or(hits[0]);
        if replace {
            let with = self.find.replace.clone();
            self.history.record("Replace", &self.script);
            // replace only in the hit line
            let ei = self.ev[h];
            let mut single = Script { entries: vec![self.script.entries[ei].clone()] };
            search::replace(&mut single, &q, &with);
            self.script.entries[ei] = single.entries.remove(0);
            self.dirty = true;
            self.qc_dirty = true;
        }
        self.select_only(h);
        self.scroll_to = Some(h);
        self.status = format!("{} match(es)", hits.len());
    }

    fn run_macro_step(&mut self, mut pm: PendingMacro) {
        let Some(rt) = self.rt.as_ref() else { return };
        rt.probe_dialogs.set(true);
        *rt.answers.borrow_mut() = pm.answers.clone();
        // Lua selection: 1-based entry indices
        let sel: Vec<usize> = self.sel.iter().filter_map(|&i| self.ev.get(i)).map(|e| e + 1).collect();
        let active = self.active.and_then(|a| self.ev.get(a)).map(|e| e + 1).unwrap_or(1);
        let shared = Rc::new(RefCell::new(self.script.clone()));
        let res = rt.run(pm.reg.index, &shared, &sel, active);
        let log_text;
        match res {
            Ok(out) => {
                log_text = out.log.join("\n");
                if let Some(spec) = out.pending_dialog {
                    pm.values = spec.controls.iter().filter(|c| !c.name.is_empty()).map(|c| (c.name.clone(), c.value.clone())).collect();
                    pm.spec = spec;
                    self.pending = Some(pm);
                    return;
                }
                if out.cancelled {
                    self.status = format!("{} cancelled", pm.reg.name);
                } else {
                    let new = shared.borrow().clone();
                    if new != self.script {
                        self.history.record(&pm.reg.name, &self.script);
                        self.script = new;
                        self.dirty = true;
                        self.reindex();
                    }
                    if let Some(s) = out.selection {
                        self.sel = s.iter().filter_map(|&e| self.ev.iter().position(|&x| x + 1 == e)).collect();
                        self.active = self.sel.iter().next().copied();
                    }
                    self.status = format!("{} done", pm.reg.name);
                }
            }
            Err(e) => {
                log_text = e.to_string();
                self.status = format!("{} failed", pm.reg.name);
            }
        }
        if !log_text.is_empty() {
            self.log_window = Some(log_text);
        }
    }

    // ---------- UI ----------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (open, save, undo, redo, find, del, newl) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::O),
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::Y),
                i.consume_key(Modifiers::COMMAND, Key::F),
                false,
                i.consume_key(Modifiers::COMMAND, Key::I),
            )
        });
        let _ = del;
        if open {
            self.do_open();
        }
        if save {
            self.do_save();
        }
        if undo && !ctx.egui_wants_keyboard_input() {
            self.undo();
        }
        if redo && !ctx.egui_wants_keyboard_input() {
            self.redo();
        }
        if find {
            self.find.open = true;
        }
        if newl {
            self.insert_line_after_active();
        }
    }

    fn do_open(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Subtitles", &["ass", "ssa", "srt", "vtt"]).pick_file() {
            self.open_path(&p);
        }
    }
    fn do_save(&mut self) {
        match self.path.clone() {
            Some(p) if matches!(formats::Format::from_path(&p.to_string_lossy()), Some(formats::Format::Ass)) => self.save_to(&p),
            _ => self.do_save_as(),
        }
    }
    fn do_save_as(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).save_file() {
            self.save_to(&p);
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t("File"), |ui| {
                if ui.button(t("Open…   Ctrl+O")).clicked() {
                    ui.close();
                    self.do_open();
                }
                if ui.button(t("Save   Ctrl+S")).clicked() {
                    ui.close();
                    self.do_save();
                }
                ui.menu_button("Recent", |ui| {
                    let recent = self.cfg.recent.clone();
                    if recent.is_empty() {
                        ui.label("—");
                    }
                    for r in recent {
                        if ui.button(r.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()).on_hover_text(r.display().to_string()).clicked() {
                            ui.close();
                            self.open_path(&r);
                        }
                    }
                });
                if ui.button(t("Save as…")).clicked() {
                    ui.close();
                    self.do_save_as();
                }
                if ui.button(t("New")).clicked() {
                    ui.close();
                    self.script = Script::new_default();
                    self.path = None;
                    self.history = History::new(200);
                    self.reindex();
                }
            });
            ui.menu_button("Dil / Language", |ui| {
                if ui.selectable_label(crate::i18n::is_turkish(), "Türkçe").clicked() {
                    crate::i18n::set_turkish(true);
                    crate::i18n::save_pref(true);
                    ui.close();
                }
                if ui.selectable_label(!crate::i18n::is_turkish(), "English").clicked() {
                    crate::i18n::set_turkish(false);
                    crate::i18n::save_pref(false);
                    ui.close();
                }
            });
            ui.menu_button(t("Edit"), |ui| {
                if ui.add_enabled(self.history.can_undo(), egui::Button::new(t("Undo   Ctrl+Z"))).clicked() {
                    ui.close();
                    self.undo();
                }
                if ui.add_enabled(self.history.can_redo(), egui::Button::new(t("Redo   Ctrl+Y"))).clicked() {
                    ui.close();
                    self.redo();
                }
                ui.separator();
                if ui.button(t("Find / Replace…   Ctrl+F")).clicked() {
                    ui.close();
                    self.find.open = true;
                }
                if ui.button(t("Select all")).clicked() {
                    ui.close();
                    self.sel = (0..self.ev.len()).collect();
                }
            });
            ui.menu_button(t("Subtitles"), |ui| {
                if ui.button(t("Insert line   Ctrl+I")).clicked() {
                    ui.close();
                    self.insert_line_after_active();
                }
                if ui.button(t("Duplicate")).clicked() {
                    ui.close();
                    self.duplicate_selected();
                }
                if ui.button(t("Delete")).clicked() {
                    ui.close();
                    self.delete_selected();
                }
                if ui.button(t("Split at middle")).clicked() {
                    ui.close();
                    self.split_active();
                }
                if ui.button(t("Merge selected")).clicked() {
                    ui.close();
                    self.merge_selected();
                }
                ui.separator();
                if ui.button(t("Sort by start time")).clicked() {
                    ui.close();
                    self.edit("Sort", |s| s.sort_by_start());
                }
                if ui.button(t("Fix common errors")).clicked() {
                    ui.close();
                    let mut n = 0;
                    self.edit("Fix common errors", |s| n = ops::fix_common_errors(s));
                    self.status = format!("Fixed {n} line(s)");
                }
                if ui.button(t("Remove empty lines")).clicked() {
                    ui.close();
                    let mut n = 0;
                    self.edit("Remove empty", |s| n = ops::remove_empty(s));
                    self.status = format!("Removed {n} line(s)");
                }
                if ui.button(t("Balance long lines")).clicked() {
                    ui.close();
                    let m = self.balance_max;
                    let mut n = 0;
                    self.edit("Balance lines", |s| n = kaku_core::textfix::balance_script(s, m));
                    self.status = format!("Rebalanced {n} line(s)");
                }
                if ui.button(t("Remove hearing-impaired text")).clicked() {
                    ui.close();
                    let mut n = 0;
                    self.edit("Remove HI", |s| {
                        n = kaku_core::textfix::remove_hearing_impaired(s);
                        ops::remove_empty(s);
                    });
                    self.status = format!("Changed {n} line(s)");
                }
                ui.menu_button("Tags", |ui| {
                    let apply = |me: &mut Self, label: &str, f: &dyn Fn(&str) -> String| {
                        let sel = me.selected_vec();
                        me.history.record(label, &me.script);
                        for i in sel {
                            if let Some(l) = me.line_mut(i) {
                                l.text = f(&l.text);
                            }
                        }
                        me.dirty = true;
                        me.qc_dirty = true;
                    };
                    if ui.button("Clean tags (selected)").clicked() {
                        ui.close();
                        apply(self, "Clean tags", &|t| tags::clean(t));
                    }
                    if ui.button("Strip all tags (selected)").clicked() {
                        ui.close();
                        apply(self, "Strip tags", &|t| tags::strip_tags(t));
                    }
                    if ui.button("Add \\blur0.6 (selected)").clicked() {
                        ui.close();
                        apply(self, "Add blur", &|t| tags::prepend_tags(t, "\\blur0.6"));
                    }
                });
                if ui.button("Select overlapping lines").clicked() {
                    ui.close();
                    let o = ops::find_overlaps(&self.script);
                    self.status = format!("{} overlapping line(s)", o.len());
                    self.sel = o.into_iter().collect();
                    if let Some(&f) = self.sel.iter().next() {
                        self.active = Some(f);
                        self.scroll_to = Some(f);
                    }
                }
                ui.menu_button(t("Change case (selected)"), |ui| {
                    use kaku_core::textfix::Casing;
                    ui.checkbox(&mut self.turkish, t("Turkish i/İ rules"));
                    for (label, mode) in [("UPPER", Casing::Upper), ("lower", Casing::Lower), ("Sentence case", Casing::Sentence), ("Title Case", Casing::Title)] {
                        if ui.button(label).clicked() {
                            ui.close();
                            let sel = self.selected_vec();
                            let tr = self.turkish;
                            self.history.record("Change case", &self.script);
                            for i in sel {
                                if let Some(l) = self.line_mut(i) {
                                    l.text = kaku_core::textfix::change_case(&l.text, mode, tr);
                                }
                            }
                            self.dirty = true;
                            self.qc_dirty = true;
                        }
                    }
                });
                if ui.button(t("Check / collect fonts…")).clicked() {
                    ui.close();
                    self.fonts_cache = kaku_core::fonts::check_fonts(&self.script);
                    self.fonts_open = true;
                }
                if ui.button(t("Resample resolution…")).clicked() {
                    ui.close();
                    self.resample_xy = self.script.play_res();
                    self.resample_open = true;
                }
                if ui.button(t("Remove duplicates")).clicked() {
                    ui.close();
                    let mut n = 0;
                    self.edit("Remove duplicates", |s| n = ops::remove_duplicates(s));
                    self.status = format!("Removed {n} line(s)");
                }
            });
            ui.menu_button(t("Timing"), |ui| {
                if ui.button(t("Shift times…")).clicked() {
                    ui.close();
                    self.shift_open = true;
                }
                if ui.button(t("Timing post-processor…")).clicked() {
                    ui.close();
                    self.tpp_open = true;
                }
                if ui.button(t("Close small gaps (≤300 ms)")).clicked() {
                    ui.close();
                    let mut n = 0;
                    self.edit("Close gaps", |s| n = ops::close_gaps(s, 300));
                    self.status = format!("Closed {n} gap(s)");
                }
                if ui.button(t("Enforce 2-frame gap (84 ms)")).clicked() {
                    ui.close();
                    let mut n = 0;
                    self.edit("Min gap", |s| n = ops::enforce_min_gap(s, 84));
                    self.status = format!("Adjusted {n} line(s)");
                }
                if ui.button(t("Extend to reading speed")).clicked() {
                    ui.close();
                    let cps = self.qc_cfg.max_cps;
                    let max = self.qc_cfg.max_duration;
                    let mut n = 0;
                    self.edit("Extend to CPS", |s| n = ops::extend_to_cps(s, cps, max));
                    self.status = format!("Extended {n} line(s)");
                }
            });
            ui.menu_button(t("Media"), |ui| {
                if ui.button(t("Open video / audio…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Media", &["mkv", "mp4", "avi", "webm", "mov", "m4v", "flac", "wav", "mp3", "m4a", "aac", "ogg", "opus", "mka"]).pick_file() {
                        let ctx = ui.ctx().clone();
                        self.media.load(p, &ctx);
                    }
                }
                let has = self.media.path.is_some();
                if ui.add_enabled(has, egui::Button::new(t("▶ Play active line audio"))).clicked() {
                    ui.close();
                    if let Some(l) = self.active.and_then(|a| self.line(a)).cloned() {
                        self.media.play(l.start, l.end);
                    }
                }
                if ui.add_enabled(has, egui::Button::new(t("■ Stop"))).clicked() {
                    ui.close();
                    self.media.stop();
                }
                ui.separator();
                if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new(t("Snap all lines to keyframes (±250 ms)"))).clicked() {
                    ui.close();
                    let kf = self.media.keyframes.clone();
                    let mut n = 0;
                    self.edit("Snap to keyframes", |s| n = kaku_media::snap_to_keyframes(s, &kf, 250, 250));
                    self.status = format!("Snapped {n} line(s)");
                }
                ui.checkbox(&mut self.media.snap_keyframes, t("Snap dragging to keyframes (Alt = off)"));
                ui.checkbox(&mut self.media.follow, t("Waveform follows active line"));
            });
            ui.menu_button(t("Karaoke"), |ui| {
                if ui.button(t("Auto-split selected by word")).clicked() {
                    ui.close();
                    self.kara_auto(karaoke::Split::Word);
                }
                if ui.button(t("Auto-split selected by character")).clicked() {
                    ui.close();
                    self.kara_auto(karaoke::Split::Grapheme);
                }
                if ui.button(t("Remove karaoke tags")).clicked() {
                    ui.close();
                    let sel = self.selected_vec();
                    self.history.record("Strip karaoke", &self.script);
                    for i in sel {
                        if let Some(l) = self.line_mut(i) {
                            l.text = karaoke::strip_k(&l.text);
                        }
                    }
                    self.dirty = true;
                }
            });
            ui.menu_button(t("Automation"), |ui| {
                if ui.button(t("Load Lua script…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Lua", &["lua"]).pick_file() {
                        self.load_lua(&p);
                    }
                }
                ui.separator();
                if self.macros.is_empty() {
                    ui.label(RichText::new("No macros loaded.\nPut .lua files in ./automation or ~/.kaku/automation").weak());
                }
                let list = self.macros.clone();
                for r in list {
                    let icon = if r.kind == LKind::Macro { "▶" } else { "⚙" };
                    if ui.button(format!("{icon} {}", r.name)).on_hover_text(&r.description).clicked() {
                        ui.close();
                        self.run_macro_step(PendingMacro { reg: r, answers: vec![], spec: DialogSpec::default(), values: HashMap::new() });
                    }
                }
            });
        });
    }

    fn kara_auto(&mut self, mode: karaoke::Split) {
        let sel = self.selected_vec();
        self.history.record("Auto karaoke", &self.script);
        let by_word = matches!(mode, karaoke::Split::Word);
        for i in sel {
            if let Some(l) = self.line_mut(i) {
                let total = (l.duration() / 10).max(1);
                l.text = karaoke::auto_split(&l.text, total, if by_word { karaoke::Split::Word } else { karaoke::Split::Grapheme });
            }
        }
        self.dirty = true;
        self.qc_dirty = true;
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("🔎");
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("filter text / style / actor").desired_width(260.0));
            if !self.filter.is_empty() && ui.small_button("✖").clicked() {
                self.filter.clear();
            }
        });
        let filt = self.filter.to_lowercase();
        let rows: Vec<usize> = (0..self.ev.len())
            .filter(|&i| {
                filt.is_empty() || self.line(i).map_or(false, |l| l.text.to_lowercase().contains(&filt) || l.style.to_lowercase().contains(&filt) || l.actor.to_lowercase().contains(&filt))
            })
            .collect();
        let issue_lines: BTreeSet<usize> = self.issues.iter().map(|i| i.line).collect();
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut tb = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .column(Column::exact(48.0))
            .column(Column::exact(34.0))
            .column(Column::exact(86.0))
            .column(Column::exact(86.0))
            .column(Column::exact(52.0))
            .column(Column::exact(44.0))
            .column(Column::exact(110.0))
            .column(Column::exact(90.0))
            .column(Column::remainder().clip(true));
        if let Some(t) = self.scroll_to.take() {
            if let Some(pos) = rows.iter().position(|&r| r == t) {
                tb = tb.scroll_to_row(pos, Some(egui::Align::Center));
            }
        }
        tb.header(20.0, |mut h| {
            let heads_en = ["#", "L", "Start", "End", "Dur", "CPS", "Style", "Actor", "Text"];
            let heads_tr = ["#", "K", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Karakter", "Metin"];
            for k in 0..9 {
                h.col(|ui| {
                    ui.strong(if crate::i18n::is_turkish() { heads_tr[k] } else { heads_en[k] });
                });
            }
        })
        .body(|body| {
            body.rows(22.0, rows.len(), |mut row| {
                let i = rows[row.index()];
                let Some(l) = self.line(i) else { return };
                let l = l.clone();
                row.set_selected(self.sel.contains(&i));
                let dim = l.is_comment();
                let bad = issue_lines.contains(&i);
                let col = if dim { Color32::GRAY } else { ui_text_color(bad) };
                let cps = if l.duration() > 0 && !tags::has_drawing(&l.text) { tags::visible_len(&l.text) as f64 / (l.duration() as f64 / 1000.0) } else { 0.0 };
                row.col(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).color(col));
                });
                row.col(|ui| {
                    ui.label(RichText::new(l.layer.to_string()).color(col));
                });
                row.col(|ui| {
                    ui.label(RichText::new(format_ass_time(l.start)).color(col).monospace());
                });
                row.col(|ui| {
                    ui.label(RichText::new(format_ass_time(l.end)).color(col).monospace());
                });
                row.col(|ui| {
                    ui.label(RichText::new(format!("{:.2}", l.duration() as f64 / 1000.0)).color(col).monospace());
                });
                row.col(|ui| {
                    let c = if cps > self.qc_cfg.max_cps { Color32::from_rgb(255, 110, 110) } else { col };
                    ui.label(RichText::new(format!("{:.0}", cps)).color(c).monospace());
                });
                row.col(|ui| {
                    ui.label(RichText::new(&l.style).color(col));
                });
                row.col(|ui| {
                    ui.label(RichText::new(&l.actor).color(col));
                });
                row.col(|ui| {
                    let job = highlight::layout(&l.text.replace('\n', " "), egui::FontId::proportional(14.0), col, f32::INFINITY);
                    ui.add(egui::Label::new(job).truncate());
                });
                if row.response().clicked() {
                    clicked = Some((i, row.response().ctx.input(|inp| inp.modifiers)));
                }
            });
        });
        if let Some((i, m)) = clicked {
            if m.shift {
                let (a, b) = (self.anchor.min(i), self.anchor.max(i));
                self.sel = (a..=b).collect();
                self.active = Some(i);
            } else if m.command {
                if !self.sel.remove(&i) {
                    self.sel.insert(i);
                }
                self.active = Some(i);
                self.anchor = i;
            } else {
                self.select_only(i);
            }
        }
        // arrow key navigation
        if !ui.ctx().egui_wants_keyboard_input() {
            let (up, down) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
            if let Some(a) = self.active {
                if up && a > 0 {
                    self.select_only(a - 1);
                    self.scroll_to = Some(a - 1);
                }
                if down && a + 1 < self.ev.len() {
                    self.select_only(a + 1);
                    self.scroll_to = Some(a + 1);
                }
            }
            if ui.input(|i| i.key_pressed(Key::Delete)) {
                self.delete_selected();
            }
        }
    }

    fn current_ass_text(&self) -> String {
        self.script.to_string()
    }

    fn frame_logic(&mut self, ctx: &egui::Context) {
        if self.media.path.is_none() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let key = (self.active, self.rev);
        if key != self.frame_key {
            self.frame_key = key;
            self.frame_key_time = now;
        }
        if key != self.frame_requested {
            if now - self.frame_key_time > 0.3 {
                if let Some(l) = self.active.and_then(|a| self.line(a)) {
                    let t = (l.start + l.end) / 2;
                    let text = self.current_ass_text();
                    self.media.request_frame(t, text);
                    self.frame_requested = key;
                    self.media.playhead = t;
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        // follow active line in waveform
        if self.media.follow && self.active != self.last_active {
            self.last_active = self.active;
            if let Some(l) = self.active.and_then(|a| self.line(a)) {
                let span = (self.media.ms_per_px * 900.0) as i64;
                self.media.view_start = (l.start - span / 4).max(0);
            }
        }
    }

    fn set_pos_from_click(&mut self, fx: f32, fy: f32) {
        let (rx, ry) = self.script.play_res();
        let (x, y) = ((fx * rx as f32).round() as i64, (fy * ry as f32).round() as i64);
        let sel = self.selected_vec();
        self.history.record("Set \\pos", &self.script);
        for i in sel {
            if let Some(l) = self.line_mut(i) {
                let mut segs = tags::parse(&l.text);
                let pos = tags::Tag { name: "pos".into(), arg: format!("({x},{y})") };
                match segs.first_mut() {
                    Some(tags::Seg::Block(ts)) => {
                        ts.retain(|t| t.name != "pos" && t.name != "move");
                        ts.insert(0, pos);
                    }
                    _ => segs.insert(0, tags::Seg::Block(vec![pos])),
                }
                l.text = tags::render(&segs);
            }
        }
        self.dirty = true;
        self.qc_dirty = true;
        self.status = format!("\\pos({x},{y})");
    }

    fn wave_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.small_button(if self.media.playing() { "■" } else { "▶" }).clicked() {
                if self.media.playing() {
                    self.media.stop();
                } else if let Some(l) = self.active.and_then(|a| self.line(a)).cloned() {
                    self.media.play(l.start, l.end);
                }
            }
            ui.label(RichText::new(format!("{}  ·  Ctrl+scroll: zoom  ·  drag yellow edges to retime", format_ass_time(self.media.playhead))).weak().small());
        });
        // spans: only lines near the view for speed
        let view_a = self.media.view_start - 5000;
        let view_b = self.media.view_start + (self.media.ms_per_px * 4000.0) as i64;
        let spans: Vec<LineSpan> = (0..self.ev.len())
            .filter_map(|i| {
                let l = self.line(i)?;
                if l.end < view_a || l.start > view_b || l.is_comment() {
                    return None;
                }
                Some(LineSpan { idx: i, start: l.start, end: l.end, active: self.active == Some(i), selected: self.sel.contains(&i) })
            })
            .collect();
        // karaoke syllable boundaries of the active line
        let marks: Vec<(i64, String)> = match self.active.and_then(|a| self.line(a)) {
            Some(l) if l.text.contains("\\k") || l.text.contains("\\K") => {
                let mut t = l.start;
                karaoke::syllables(&l.text).iter().map(|s| {
                    let m = (t, s.text.chars().take(8).collect::<String>());
                    t += s.dur_cs * 10;
                    m
                }).collect()
            }
            _ => vec![],
        };
        let events = self.media.waveform_ui(ui, &spans, &marks);
        for e in events {
            match e {
                MEvent::DragBegin => self.history.record("Drag timing", &self.script),
                MEvent::SetStart(i, t) => {
                    if let Some(l) = self.line_mut(i) {
                        l.start = t.min(l.end - 10);
                    }
                    self.dirty = true;
                    self.qc_dirty = true;
                    self.time_buf.0 = usize::MAX;
                }
                MEvent::SetEnd(i, t) => {
                    if let Some(l) = self.line_mut(i) {
                        l.end = t.max(l.start + 10);
                    }
                    self.dirty = true;
                    self.qc_dirty = true;
                    self.time_buf.0 = usize::MAX;
                }
                MEvent::SylBoundary(i, t) => {
                    if let Some(a) = self.active {
                        if let Some(l) = self.line_mut(a) {
                            let mut sy = karaoke::syllables(&l.text);
                            if i >= 1 && i < sy.len() {
                                let start0 = l.start + sy[..i - 1].iter().map(|s| s.dur_cs * 10).sum::<i64>();
                                let pair_end = start0 + (sy[i - 1].dur_cs + sy[i].dur_cs) * 10;
                                let t = t.clamp(start0, pair_end);
                                let d0 = ((t - start0 + 5) / 10).max(0);
                                let total = sy[i - 1].dur_cs + sy[i].dur_cs;
                                sy[i - 1].dur_cs = d0.min(total);
                                sy[i].dur_cs = total - sy[i - 1].dur_cs;
                                l.text = karaoke::build(&sy);
                                self.dirty = true;
                            }
                        }
                    }
                }
                MEvent::Select(i) => {
                    self.select_only(i);
                    self.scroll_to = Some(i);
                }
                MEvent::Seek(t) => {
                    let text = self.current_ass_text();
                    self.media.request_frame(t, text);
                }
            }
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.active else {
            ui.label(t("No line selected"));
            return;
        };
        let Some(mut l) = self.line(a).cloned() else { return };
        let orig = l.clone();
        let styles: Vec<String> = self.script.styles().map(|s| s.name.clone()).collect();
        ui.horizontal(|ui| {
            let mut comment = l.is_comment();
            if ui.checkbox(&mut comment, t("Comment")).changed() {
                l.kind = if comment { Kind::Comment } else { Kind::Dialogue };
            }
            ui.label(t("Layer"));
            ui.add(egui::DragValue::new(&mut l.layer));
            if self.time_buf.0 != a {
                self.time_buf = (a, format_ass_time(l.start), format_ass_time(l.end));
            }
            ui.label(t("Start"));
            let r1 = ui.add(egui::TextEdit::singleline(&mut self.time_buf.1).desired_width(100.0).font(egui::TextStyle::Monospace));
            if r1.lost_focus() {
                if let Some(t) = parse_ass_time(&self.time_buf.1) {
                    l.start = t;
                }
                self.time_buf.0 = usize::MAX;
            }
            ui.label(t("End"));
            let r2 = ui.add(egui::TextEdit::singleline(&mut self.time_buf.2).desired_width(100.0).font(egui::TextStyle::Monospace));
            if r2.lost_focus() {
                if let Some(t) = parse_ass_time(&self.time_buf.2) {
                    l.end = t;
                }
                self.time_buf.0 = usize::MAX;
            }
            ui.label(format!("Dur {:.2}s", l.duration() as f64 / 1000.0));
            egui::ComboBox::from_id_salt("style_cb").selected_text(&l.style).show_ui(ui, |ui| {
                for s in &styles {
                    ui.selectable_value(&mut l.style, s.clone(), s);
                }
            });
            ui.label(t("Actor"));
            ui.add(egui::TextEdit::singleline(&mut l.actor).desired_width(90.0));
            ui.label(t("Effect"));
            ui.add(egui::TextEdit::singleline(&mut l.effect).desired_width(80.0));
        });
        let font = egui::FontId::proportional(16.0);
        let base = ui.visuals().text_color();
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
            let job = highlight::layout(text.as_str(), font.clone(), base, wrap);
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let te = egui::TextEdit::multiline(&mut l.text).desired_rows(3).desired_width(f32::INFINITY).layouter(&mut layouter);
        let resp = ui.add(te);
        if resp.gained_focus() {
            self.history.record("Edit text", &self.script);
        }
        let vis = tags::visible_text(&l.text);
        let chars = tags::visible_len(&l.text);
        let cps = if l.duration() > 0 { chars as f64 / (l.duration() as f64 / 1000.0) } else { 0.0 };
        let widest = vis.lines().map(|x| x.chars().count()).max().unwrap_or(0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{chars} chars   widest row {widest}   {cps:.1} cps")).weak());
            if ui.small_button("\\N").on_hover_text("Insert line break").clicked() {
                l.text.push_str("\\N");
            }
        });
        if l != orig {
            let changed_text_only = l.text != orig.text && l.start == orig.start && l.end == orig.end && l.style == orig.style;
            if !changed_text_only && !resp.has_focus() {
                self.history.record("Edit line", &self.script);
            }
            if let Some(dst) = self.line_mut(a) {
                *dst = l;
            }
            self.dirty = true;
            self.qc_dirty = true;
        }
    }

    fn side(&mut self, ui: &mut egui::Ui) {
        if self.media.path.is_some() {
            ui.checkbox(&mut self.pick_pos, t("Click video to set \\pos"));
            if let Some((fx, fy)) = self.media.video_ui(ui, self.pick_pos) {
                self.set_pos_from_click(fx, fy);
            }
            ui.label(RichText::new(&self.media.status).weak().small());
            ui.separator();
        }
        ui.horizontal(|ui| {
            for (i, t) in ["QC", "Styles", "Info"].iter().enumerate() {
                ui.selectable_value(&mut self.side_tab, i, *t);
            }
        });
        ui.separator();
        match self.side_tab {
            0 => {
                ui.horizontal(|ui| {
                    ui.label("CPS");
                    ui.add(egui::DragValue::new(&mut self.qc_cfg.max_cps).range(5.0..=60.0));
                    ui.label(t("Chars"));
                    ui.add(egui::DragValue::new(&mut self.qc_cfg.max_line_chars).range(10..=120));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Min ms"));
                    ui.add(egui::DragValue::new(&mut self.qc_cfg.min_duration).range(0..=3000));
                    ui.label(t("Max ms"));
                    ui.add(egui::DragValue::new(&mut self.qc_cfg.max_duration).range(1000..=30000));
                });
                if ui.button(t("Re-check")).clicked() {
                    self.qc_dirty = true;
                }
                ui.label(format!("{} issue(s)", self.issues.len()));
                let mut go = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for it in &self.issues {
                        let c = match it.severity {
                            qc::Severity::Error => Color32::from_rgb(255, 100, 100),
                            qc::Severity::Warning => Color32::from_rgb(255, 200, 90),
                            qc::Severity::Info => Color32::GRAY,
                        };
                        if ui.add(egui::Button::new(RichText::new(format!("#{} {} — {}", it.line + 1, it.code, it.message)).color(c)).frame(false)).clicked() {
                            go = Some(it.line);
                        }
                    }
                });
                if let Some(g) = go {
                    self.select_only(g);
                    self.scroll_to = Some(g);
                }
            }
            1 => self.styles_tab(ui),
            _ => {
                ui.label(RichText::new("Script properties").strong());
                let mut changed = false;
                let mut remove: Option<usize> = None;
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    egui::Grid::new("info_grid").num_columns(3).show(ui, |ui| {
                        for (i, e) in self.script.entries.iter_mut().enumerate() {
                            if let Entry::Info { key, value } = e {
                                ui.label(key.as_str());
                                changed |= ui.add(egui::TextEdit::singleline(value).desired_width(150.0)).changed();
                                if ui.small_button("✖").clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        }
                    });
                });
                if let Some(i) = remove {
                    self.history.record("Remove property", &self.script);
                    self.script.entries.remove(i);
                    self.dirty = true;
                }
                if changed {
                    self.dirty = true;
                }
                ui.separator();
                let (x, y) = self.script.play_res();
                ui.label(format!("Lines: {}", self.ev.len()));
                ui.label(format!("Resolution: {x}×{y}"));
                ui.label(format!("Styles: {}", self.script.styles().count()));
                let total: i64 = self.script.lines().map(|l| l.duration().max(0)).sum();
                ui.label(format!("Total on-screen time: {}", format_ass_time(total)));
                if let Some(p) = &self.path {
                    ui.label(p.display().to_string());
                }
            }
        }
    }

    fn styles_tab(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = self.script.styles().map(|s| s.name.clone()).collect();
        if names.is_empty() {
            ui.label(t("No styles"));
            return;
        }
        self.style_edit = self.style_edit.min(names.len() - 1);
        let cur = names[self.style_edit].clone();
        ui.horizontal_wrapped(|ui| {
            if ui.button(t("Duplicate")).clicked() {
                let mut new = None;
                self.edit("Duplicate style", |s| new = kaku_core::styles::duplicate(s, &cur));
                if let Some(n) = new {
                    self.style_edit = self.script.styles().position(|x| x.name == n).unwrap_or(0);
                }
            }
            if ui.button(t("Delete")).clicked() && names.len() > 1 {
                let repl = names.iter().find(|n| **n != cur).cloned();
                self.edit("Delete style", |s| kaku_core::styles::delete(s, &cur, repl.as_deref()));
            }
            if ui.button(t("Import…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass", "ssa"]).pick_file() {
                    if let Ok(b) = std::fs::read(&p) {
                        let src = formats::load(&p.to_string_lossy(), &b);
                        let mut n = 0;
                        self.edit("Import styles", |s| n = kaku_core::styles::import(s, &src, None, false));
                        self.status = format!("Imported {n} style(s)");
                    }
                }
            }
            let miss = kaku_core::styles::missing(&self.script);
            if !miss.is_empty() && ui.button(format!("Create {} missing", miss.len())).clicked() {
                self.edit("Create missing styles", |s| {
                    kaku_core::styles::create_missing(s);
                });
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.style_rename).hint_text("new name").desired_width(140.0));
            if ui.button(t("Rename")).clicked() && !self.style_rename.trim().is_empty() {
                let to = self.style_rename.trim().to_string();
                let mut ok = false;
                self.edit("Rename style", |s| ok = kaku_core::styles::rename(s, &cur, &to));
                if ok {
                    self.style_edit = self.script.styles().position(|x| x.name == to).unwrap_or(0);
                    self.style_rename.clear();
                } else {
                    self.status = "Rename failed (name exists?)".into();
                }
            }
        });
        let names: Vec<String> = self.script.styles().map(|s| s.name.clone()).collect();
        self.style_edit = self.style_edit.min(names.len().saturating_sub(1));
        egui::ComboBox::from_id_salt("style_pick").selected_text(&names[self.style_edit]).show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(&mut self.style_edit, i, n);
            }
        });
        let target = names[self.style_edit].clone();
        let idx = self.script.entries.iter().position(|e| matches!(e, Entry::Style(s) if s.name == target));
        let Some(idx) = idx else { return };
        let Entry::Style(mut st) = self.script.entries[idx].clone() else { return };
        let orig = st.clone();
        egui::Grid::new("style_grid").num_columns(2).show(ui, |ui| {
            ui.label(t("Font"));
            ui.text_edit_singleline(&mut st.fontname);
            ui.end_row();
            ui.label(t("Size"));
            ui.add(egui::DragValue::new(&mut st.fontsize).range(1.0..=500.0));
            ui.end_row();
            ui.label(t("Bold / Italic"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.bold, "B");
                ui.checkbox(&mut st.italic, "I");
                ui.checkbox(&mut st.underline, "U");
            });
            ui.end_row();
            for (label, c) in [("Primary", &mut st.color1), ("Secondary", &mut st.color2), ("Outline", &mut st.color3), ("Shadow", &mut st.color4)] {
                ui.label(label);
                let mut rgba = [c.r, c.g, c.b, 255 - c.a];
                if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                    c.r = rgba[0];
                    c.g = rgba[1];
                    c.b = rgba[2];
                    c.a = 255 - rgba[3];
                }
                ui.end_row();
            }
            ui.label(t("Outline / Shadow"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut st.outline).speed(0.1).range(0.0..=50.0));
                ui.add(egui::DragValue::new(&mut st.shadow).speed(0.1).range(0.0..=50.0));
            });
            ui.end_row();
            ui.label(t("Alignment"));
            ui.add(egui::DragValue::new(&mut st.align).range(1..=9));
            ui.end_row();
            ui.label(t("Margins L R V"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut st.margin_l));
                ui.add(egui::DragValue::new(&mut st.margin_r));
                ui.add(egui::DragValue::new(&mut st.margin_t));
            });
            ui.end_row();
        });
        if st != orig {
            self.history.record("Edit style", &self.script);
            self.script.entries[idx] = Entry::Style(st);
            self.dirty = true;
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if self.find.open {
            let mut open = true;
            egui::Window::new(t("Find / Replace")).open(&mut open).default_width(360.0).show(ctx, |ui| {
                egui::Grid::new("fr").num_columns(2).show(ui, |ui| {
                    ui.label(t("Find"));
                    ui.text_edit_singleline(&mut self.find.pattern);
                    ui.end_row();
                    ui.label(t("Replace"));
                    ui.text_edit_singleline(&mut self.find.replace);
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.find.regex, t("Regex"));
                    ui.checkbox(&mut self.find.case, t("Match case"));
                    ui.checkbox(&mut self.find.word, t("Whole word"));
                });
                egui::ComboBox::from_label("in").selected_text(["Text (raw)", "Text (no tags)", "Style", "Actor", "Effect"][self.find.field.min(4)]).show_ui(ui, |ui| {
                    for (i, n) in ["Text (raw)", "Text (no tags)", "Style", "Actor", "Effect"].iter().enumerate() {
                        ui.selectable_value(&mut self.find.field, i, *n);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(t("Find next")).clicked() {
                        self.run_find(false, false);
                    }
                    if ui.button(t("Replace")).clicked() {
                        self.run_find(true, false);
                    }
                    if ui.button(t("Replace all")).clicked() {
                        self.run_find(true, true);
                    }
                });
            });
            self.find.open = open;
        }
        if self.shift_open {
            let mut open = true;
            egui::Window::new(t("Shift times")).open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Milliseconds"));
                    ui.add(egui::DragValue::new(&mut self.shift_ms).speed(10));
                });
                ui.horizontal(|ui| {
                    if ui.button(t("Selected")).clicked() {
                        let sel = self.selected_vec();
                        let d = self.shift_ms;
                        self.edit("Shift selected", |s| ops::shift(s, d, Some(&sel)));
                    }
                    if ui.button(t("All")).clicked() {
                        let d = self.shift_ms;
                        self.edit("Shift all", |s| ops::shift(s, d, None));
                    }
                    if ui.button(t("From selected to end")).clicked() {
                        let first = self.sel.iter().next().copied().unwrap_or(0);
                        let sel: Vec<usize> = (first..self.ev.len()).collect();
                        let d = self.shift_ms;
                        self.edit("Shift tail", |s| ops::shift(s, d, Some(&sel)));
                    }
                });
            });
            self.shift_open = open;
        }
        if self.fonts_open {
            let mut open = true;
            egui::Window::new(t("Fonts")).open(&mut open).default_width(520.0).show(ctx, |ui| {
                if ui.button(t("Re-scan")).clicked() {
                    self.fonts_cache = kaku_core::fonts::check_fonts(&self.script);
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("fonts_grid").striped(true).num_columns(4).show(ui, |ui| {
                        ui.strong(t("Font"));
                        ui.strong("B/I");
                        ui.strong(t("Lines"));
                        ui.strong(t("Status"));
                        ui.end_row();
                        for f in &self.fonts_cache {
                            ui.label(&f.used.family);
                            ui.label(format!("{}{}", if f.used.bold { "B" } else { "" }, if f.used.italic { "I" } else { "" }));
                            ui.label(f.used.lines.to_string());
                            if !f.installed {
                                ui.colored_label(Color32::from_rgb(255, 100, 100), "missing");
                            } else if !f.missing_glyphs.is_empty() {
                                ui.colored_label(Color32::from_rgb(255, 200, 90), format!("no glyph: {}", f.missing_glyphs.iter().take(12).collect::<String>()));
                            } else {
                                ui.colored_label(Color32::from_rgb(120, 220, 140), "ok");
                            }
                            ui.end_row();
                        }
                    });
                });
                if ui.button(t("Collect font files to folder…")).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        match kaku_core::fonts::collect_fonts(&self.script, &d) {
                            Ok((c, m)) => self.status = format!("Copied {} font file(s); {} not found", c.len(), m.len()),
                            Err(e) => self.status = format!("Collect failed: {e}"),
                        }
                    }
                }
            });
            if !open {
                self.fonts_open = false;
            }
        }
        if self.resample_open {
            let mut open = true;
            egui::Window::new(t("Resample resolution")).open(&mut open).show(ctx, |ui| {
                let (ox, oy) = self.script.play_res();
                ui.label(format!("Current: {ox}×{oy}"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.resample_xy.0).range(16..=16000));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut self.resample_xy.1).range(16..=16000));
                    for (w, h) in [(1280, 720), (1920, 1080), (3840, 2160)] {
                        if ui.small_button(format!("{h}p")).clicked() {
                            self.resample_xy = (w, h);
                        }
                    }
                });
                if ui.button(t("Resample")).clicked() {
                    let (x, y) = self.resample_xy;
                    let mut ok = false;
                    self.edit("Resample", |s| ok = kaku_core::resample::resample(s, x, y, Default::default()));
                    self.status = if ok { format!("Resampled to {x}×{y}") } else { "Nothing to do".into() };
                    self.resample_open = false;
                }
            });
            if !open {
                self.resample_open = false;
            }
        }
        if self.tpp_open {
            let mut open = true;
            egui::Window::new(t("Timing post-processor")).open(&mut open).show(ctx, |ui| {
                egui::Grid::new("tpp").num_columns(2).show(ui, |ui| {
                    ui.label(t("Lead-in (ms)"));
                    ui.add(egui::DragValue::new(&mut self.tpp.0).range(0..=2000));
                    ui.end_row();
                    ui.label(t("Lead-out (ms)"));
                    ui.add(egui::DragValue::new(&mut self.tpp.1).range(0..=2000));
                    ui.end_row();
                    ui.label(t("Join gaps up to (ms)"));
                    ui.add(egui::DragValue::new(&mut self.tpp.2).range(0..=2000));
                    ui.end_row();
                });
                if ui.button(t("Apply")).clicked() {
                    let t = kaku_core::textfix::Tpp { lead_in: self.tpp.0, lead_out: self.tpp.1, join_gap: self.tpp.2 };
                    let mut n = 0;
                    self.edit("Timing post-process", |s| n = kaku_core::textfix::timing_post_process(s, t));
                    self.status = format!("{n} adjustment(s)");
                    self.tpp_open = false;
                }
            });
            if !open {
                self.tpp_open = false;
            }
        }
        if let Some(mut pm) = self.pending.take() {
            let mut result: Option<Option<String>> = None; // Some(Some(button)) / Some(None)=cancel
            egui::Window::new(&pm.reg.name).collapsible(false).resizable(true).show(ctx, |ui| {
                let mut ctls: Vec<_> = pm.spec.controls.clone();
                ctls.sort_by_key(|c| (c.y, c.x));
                let mut last_y = i64::MIN;
                let mut row_ui: Option<()> = None;
                let _ = &mut row_ui;
                egui::Grid::new("lua_dlg").num_columns(2).show(ui, |ui| {
                    for c in &ctls {
                        if c.y != last_y && last_y != i64::MIN {
                            ui.end_row();
                        }
                        last_y = c.y;
                        let v = pm.values.entry(c.name.clone()).or_insert_with(|| c.value.clone());
                        match c.class.as_str() {
                            "label" => {
                                ui.label(&c.label);
                            }
                            "checkbox" => {
                                let mut b = v == "true";
                                if ui.checkbox(&mut b, &c.label).changed() {
                                    *v = b.to_string();
                                }
                            }
                            "dropdown" => {
                                egui::ComboBox::from_id_salt(&c.name).selected_text(v.clone()).show_ui(ui, |ui| {
                                    for it in &c.items {
                                        ui.selectable_value(v, it.clone(), it);
                                    }
                                });
                            }
                            "textbox" => {
                                ui.add(egui::TextEdit::multiline(v).desired_rows((c.height as usize).max(3)));
                            }
                            "intedit" | "floatedit" => {
                                ui.add(egui::TextEdit::singleline(v).desired_width(90.0));
                            }
                            _ => {
                                ui.add(egui::TextEdit::singleline(v).desired_width(180.0));
                            }
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    for b in &pm.spec.buttons {
                        if ui.button(b).clicked() {
                            result = Some(if b.eq_ignore_ascii_case("cancel") { None } else { Some(b.clone()) });
                        }
                    }
                });
            });
            match result {
                None => self.pending = Some(pm),
                Some(None) => self.status = format!("{} cancelled", pm.reg.name),
                Some(Some(button)) => {
                    pm.answers.push(DialogAnswer { button: Some(button), values: pm.values.clone() });
                    self.run_macro_step(pm);
                }
            }
        }
        if let Some(text) = self.log_window.clone() {
            let mut open = true;
            egui::Window::new(t("Automation log")).open(&mut open).show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| ui.monospace(&text));
            });
            if !open {
                self.log_window = None;
            }
        }
    }
}

fn ui_text_color(bad: bool) -> Color32 {
    if bad { Color32::from_rgb(255, 190, 120) } else { Color32::from_gray(220) }
}

impl eframe::App for KakuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.qc_dirty {
            self.issues = qc::check(&self.script, &self.qc_cfg);
            self.qc_dirty = false;
            self.rev += 1;
        }
        self.shortcuts(&ctx);
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            match ext.as_str() {
                "ass" | "ssa" | "srt" | "vtt" | "lrc" | "sbv" | "sub" => self.open_path(&p),
                "lua" => self.load_lua(&p),
                _ => self.media.load(p, &ctx),
            }
        }
        let now = ctx.input(|i| i.time);
        self.autosave_tick(now);
        if self.dirty {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        self.media.poll(&ctx);
        self.frame_logic(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let name = self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
                ui.label(format!("{}{}", name, if self.dirty { " ●" } else { "" }));
                ui.separator();
                ui.label(&self.status);
            });
        });
        egui::Panel::bottom("editor").resizable(true).default_size(170.0).show(ui, |ui| self.editor(ui));
        if self.media.path.is_some() {
            egui::Panel::bottom("wave").resizable(true).default_size(150.0).show(ui, |ui| self.wave_panel(ui));
        }
        egui::Panel::right("side").resizable(true).default_size(320.0).show(ui, |ui| self.side(ui));
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui));
        self.windows(&ctx);
    }
}
