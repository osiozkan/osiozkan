//! UI-independent application state: document, undo/redo, selection, macros, QC.
//! Everything here is unit-tested; `main.rs` only renders it.

use nsub_core::formats::{self, Format};
use nsub_core::{ops, qc, Event, Script};
use crate::worker::{Cmd, Msg, Waker, Worker};
use nsub_lua::dialog::{DialogRequest, DialogResponse, Progress};
use nsub_lua::MacroInfo;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const UNDO_LIMIT: usize = 200;

pub struct Doc {
    pub script: Script,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    undo: Vec<Script>,
    redo: Vec<Script>,
    last_key: String,
    /// Bumped on every change; lets previews know when to re-render.
    pub rev: u64,
}

impl Doc {
    pub fn new() -> Doc {
        Doc { script: Script::new(), path: None, dirty: false, undo: vec![], redo: vec![], last_key: String::new(), rev: 0 }
    }

    pub fn open(path: &Path) -> Result<Doc, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let script = formats::load(&bytes, path.to_str())?;
        let is_ass = matches!(Format::from_extension(&path.to_string_lossy()), Some(Format::Ass | Format::Ssa));
        Ok(Doc { script, path: if is_ass { Some(path.to_path_buf()) } else { None }, dirty: !is_ass, undo: vec![], redo: vec![], last_key: String::new(), rev: 0 })
    }

    /// Snapshot before a structural change.
    pub fn checkpoint(&mut self) {
        self.rev += 1;
        self.undo.push(self.script.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.last_key.clear();
    }

    /// Snapshot once per continuous edit session (typing in a field, dragging a value).
    pub fn touch(&mut self, key: &str) {
        if self.last_key != key {
            self.checkpoint();
            self.last_key = key.to_string();
        }
        self.rev += 1;
        self.dirty = true;
    }

    pub fn end_edit_session(&mut self) {
        self.last_key.clear();
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.rev += 1;
                self.redo.push(std::mem::replace(&mut self.script, prev));
                self.dirty = true;
                self.last_key.clear();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.rev += 1;
                self.undo.push(std::mem::replace(&mut self.script, next));
                self.dirty = true;
                self.last_key.clear();
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let fmt = Format::from_extension(&path.to_string_lossy()).unwrap_or(Format::Ass);
        if path.exists() {
            let _ = std::fs::copy(path, path.with_extension(format!("{}.bak", fmt.extension())));
        }
        std::fs::write(path, formats::save(&self.script, fmt)).map_err(|e| format!("{}: {e}", path.display()))?;
        if matches!(fmt, Format::Ass | Format::Ssa) {
            self.path = Some(path.to_path_buf());
            self.dirty = false;
        }
        Ok(())
    }

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
        format!("{}{} — nightsub", name, if self.dirty { " *" } else { "" })
    }
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

pub struct App {
    pub doc: Doc,
    pub sel: BTreeSet<usize>,
    pub active: Option<usize>,
    pub issues: Vec<qc::Issue>,
    pub qc_cfg: qc::QcConfig,
    pub status: String,
    pub log: String,
    pub scripts: Vec<PathBuf>,
    pub macros: Vec<MacroInfo>,
    pub fps: f64,
    worker: Option<Worker>,
    pub waker: Option<Waker>,
    /// Dialog the running macro is waiting on.
    pub pending_dialog: Option<DialogRequest>,
    pub running: Option<RunState>,
}

#[derive(Default, Clone)]
pub struct RunState {
    pub name: String,
    pub fraction: f64,
    pub task: String,
    pub title: String,
}

impl App {
    pub fn new() -> App {
        App {
            doc: Doc::new(),
            sel: BTreeSet::new(),
            active: None,
            issues: vec![],
            qc_cfg: qc::QcConfig::default(),
            status: String::new(),
            log: String::new(),
            scripts: vec![],
            macros: vec![],
            fps: 23.976,
            worker: None,
            waker: None,
            pending_dialog: None,
            running: None,
        }
    }

    pub fn open(&mut self, path: &Path) {
        match Doc::open(path) {
            Ok(d) => {
                self.doc = d;
                self.sel.clear();
                self.active = if self.doc.script.events.is_empty() { None } else { Some(0) };
                if let Some(a) = self.active {
                    self.sel.insert(a);
                }
                self.status = format!("Opened {}", path.display());
                self.refresh_qc();
                self.reload_automation();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    pub fn refresh_qc(&mut self) {
        self.issues = qc::check(&self.doc.script, &self.qc_cfg);
    }

    pub fn select_only(&mut self, i: usize) {
        self.sel.clear();
        self.sel.insert(i);
        self.active = Some(i);
    }

    pub fn select_range_to(&mut self, i: usize) {
        let from = self.active.unwrap_or(i);
        let (a, b) = (from.min(i), from.max(i));
        self.sel = (a..=b).collect();
        self.active = Some(i);
    }

    pub fn toggle_select(&mut self, i: usize) {
        if !self.sel.remove(&i) {
            self.sel.insert(i);
        }
        self.active = Some(i);
    }

    pub fn select_all(&mut self) {
        self.sel = (0..self.doc.script.events.len()).collect();
    }

    // ------------------------------------------------------------ structural edits

    pub fn insert_after_active(&mut self) {
        self.doc.checkpoint();
        let at = self.active.map_or(self.doc.script.events.len(), |a| a + 1);
        let (start, style) = match self.active.and_then(|a| self.doc.script.events.get(a)) {
            Some(e) => (e.end, e.style.clone()),
            None => (0, "Default".to_string()),
        };
        let ev = Event { start, end: start + 2000, style, ..Event::default() };
        self.doc.script.events.insert(at, ev);
        self.select_only(at);
    }

    pub fn delete_selected(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.doc.checkpoint();
        let first = *self.sel.iter().next().unwrap();
        let mut idx: Vec<usize> = self.sel.iter().copied().collect();
        idx.sort_unstable_by(|a, b| b.cmp(a));
        for i in idx {
            if i < self.doc.script.events.len() {
                self.doc.script.events.remove(i);
            }
        }
        self.sel.clear();
        let n = self.doc.script.events.len();
        self.active = if n == 0 { None } else { Some(first.min(n - 1)) };
        if let Some(a) = self.active {
            self.sel.insert(a);
        }
    }

    pub fn duplicate_selected(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.doc.checkpoint();
        let idx: Vec<usize> = self.sel.iter().copied().collect();
        let last = *idx.last().unwrap();
        let copies: Vec<Event> = idx.iter().filter_map(|&i| self.doc.script.events.get(i).cloned()).collect();
        let n = copies.len();
        for (k, c) in copies.into_iter().enumerate() {
            self.doc.script.events.insert(last + 1 + k, c);
        }
        self.sel = (last + 1..last + 1 + n).collect();
        self.active = Some(last + n);
    }

    pub fn merge_selected(&mut self) {
        let idx: Vec<usize> = self.sel.iter().copied().collect();
        if idx.len() < 2 {
            return;
        }
        self.doc.checkpoint();
        let first = idx[0];
        for _ in 1..idx.len() {
            ops::merge_with_next(&mut self.doc.script, first);
        }
        self.select_only(first);
    }

    pub fn shift_selected(&mut self, delta: i64) {
        if self.sel.is_empty() {
            return;
        }
        self.doc.checkpoint();
        for &i in &self.sel {
            if let Some(e) = self.doc.script.events.get_mut(i) {
                e.start = (e.start + delta).max(0);
                e.end = (e.end + delta).max(0);
            }
        }
    }

    pub fn shift_all(&mut self, delta: i64) {
        self.doc.checkpoint();
        ops::shift(&mut self.doc.script, delta, None);
    }

    pub fn sort_by_time(&mut self) {
        self.doc.checkpoint();
        self.doc.script.sort_by_start();
    }

    pub fn fix_all(&mut self) -> usize {
        self.doc.checkpoint();
        let n = qc::fix_overlaps(&mut self.doc.script, self.qc_cfg.min_gap_ms)
            + qc::fix_min_duration(&mut self.doc.script, self.qc_cfg.min_duration_ms)
            + qc::fix_double_spaces(&mut self.doc.script);
        self.refresh_qc();
        n
    }

    pub fn undo(&mut self) {
        if self.doc.undo() {
            self.clamp_selection();
        }
    }
    pub fn redo(&mut self) {
        if self.doc.redo() {
            self.clamp_selection();
        }
    }
    fn clamp_selection(&mut self) {
        let n = self.doc.script.events.len();
        self.sel.retain(|&i| i < n);
        if self.active.map_or(false, |a| a >= n) {
            self.active = n.checked_sub(1);
        }
    }

    // ------------------------------------------------------------ automation

    fn worker(&mut self) -> &Worker {
        if self.worker.is_none() {
            self.worker = Some(Worker::spawn(self.waker.clone()));
        }
        self.worker.as_ref().unwrap()
    }

    fn reload_automation(&mut self) {
        self.macros.clear();
        let cmd = Cmd::Load { scripts: self.scripts.clone(), doc_path: self.doc.path.clone(), fps: self.fps };
        self.worker().send(cmd);
    }

    pub fn add_script(&mut self, path: PathBuf) {
        if !self.scripts.contains(&path) {
            self.scripts.push(path);
        }
        self.reload_automation();
    }

    pub fn reload_scripts(&mut self) {
        self.reload_automation();
    }

    pub fn run_macro(&mut self, id: usize) {
        if self.running.is_some() {
            self.status = "A macro is already running".into();
            return;
        }
        let info = self.doc.script.info.len();
        let styles = self.doc.script.styles.len();
        let to_flat = |i: usize| info + styles + i + 1;
        let sel: Vec<usize> = self.sel.iter().map(|&i| to_flat(i)).collect();
        let active = self.active.map(to_flat).unwrap_or(info + styles + 1);
        let name = self.macros.iter().find(|m| m.id == id).map(|m| m.name.clone()).unwrap_or_default();
        self.running = Some(RunState { name, ..Default::default() });
        let script = Box::new(self.doc.script.clone());
        self.worker().send(Cmd::Run { id, script, sel, active });
    }

    pub fn cancel_macro(&mut self) {
        if let Some(w) = &self.worker {
            w.request_cancel();
        }
        // a blocked dialog also has to be released
        if self.pending_dialog.is_some() {
            self.answer_dialog(DialogResponse::default());
        }
    }

    pub fn answer_dialog(&mut self, resp: DialogResponse) {
        self.pending_dialog = None;
        if let Some(w) = &self.worker {
            w.send(Cmd::DialogAnswer(resp));
        }
    }

    /// Handle messages from the automation worker. Call every frame.
    pub fn pump(&mut self) {
        let mut msgs = vec![];
        if let Some(w) = &self.worker {
            while let Ok(m) = w.rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            match m {
                Msg::Loaded { macros, errors } => {
                    self.macros = macros;
                    if !errors.is_empty() {
                        self.log = errors.join("\n");
                        self.status = format!("{} automation script(s) failed to load (see Log)", errors.len());
                    }
                }
                Msg::Dialog(req) => self.pending_dialog = Some(req),
                Msg::Progress(Progress { fraction, task, title }) => {
                    if let Some(r) = &mut self.running {
                        if let Some(f) = fraction {
                            r.fraction = (f / 100.0).clamp(0.0, 1.0);
                        }
                        if let Some(t) = task {
                            r.task = t;
                        }
                        if let Some(t) = title {
                            r.title = t;
                        }
                    }
                }
                Msg::Finished(result) => {
                    let name = self.running.take().map(|r| r.name).unwrap_or_default();
                    self.pending_dialog = None;
                    match result {
                        Ok((script, res)) => {
                            self.log = res.log.clone();
                            if res.canceled {
                                self.status = format!("'{name}' cancelled");
                                continue;
                            }
                            self.doc.checkpoint();
                            self.doc.script = *script;
                            let (ni, ns) = (self.doc.script.info.len(), self.doc.script.styles.len());
                            let n = self.doc.script.events.len();
                            if !res.selection.is_empty() {
                                self.sel = res.selection.iter().filter_map(|&f| f.checked_sub(ni + ns + 1)).filter(|&i| i < n).collect();
                            }
                            self.clamp_selection();
                            self.status = format!("'{name}' done");
                            self.refresh_qc();
                        }
                        Err(e) => {
                            self.log = e;
                            self.status = format!("'{name}' failed (see Log)");
                        }
                    }
                }
            }
        }
    }

    pub fn is_busy(&self) -> bool {
        self.running.is_some()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(a: &mut App, done: impl Fn(&App) -> bool) {
        for _ in 0..500 {
            a.pump();
            if done(a) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("timed out waiting for worker");
    }

    fn app_with(n: usize) -> App {
        let mut a = App::new();
        for i in 0..n {
            a.doc.script.events.push(Event { start: i as i64 * 3000, end: i as i64 * 3000 + 2000, text: format!("line {i}"), ..Event::default() });
        }
        a.select_only(0);
        a
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut a = app_with(3);
        a.insert_after_active();
        assert_eq!(a.doc.script.events.len(), 4);
        a.undo();
        assert_eq!(a.doc.script.events.len(), 3);
        a.redo();
        assert_eq!(a.doc.script.events.len(), 4);
        assert!(a.doc.dirty);
    }

    #[test]
    fn edit_sessions_coalesce() {
        let mut d = Doc::new();
        d.script.events.push(Event::default());
        d.touch("0:text");
        d.script.events[0].text = "a".into();
        d.touch("0:text");
        d.script.events[0].text = "ab".into();
        assert!(d.undo());
        assert_eq!(d.script.events[0].text, "");
        assert!(!d.undo());
    }

    #[test]
    fn delete_duplicate_merge_shift() {
        let mut a = app_with(4);
        a.select_range_to(1);
        a.duplicate_selected();
        assert_eq!(a.doc.script.events.len(), 6);
        assert_eq!(a.sel.len(), 2);
        a.merge_selected();
        assert_eq!(a.doc.script.events.len(), 5);
        a.shift_selected(500);
        let e = &a.doc.script.events[a.active.unwrap()];
        assert!(e.start >= 500);
        a.delete_selected();
        assert_eq!(a.doc.script.events.len(), 4);
        assert!(a.active.is_some());
    }

    #[test]
    fn save_and_reopen() {
        let dir = std::env::temp_dir().join(format!("nsub-gui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.ass");
        let mut a = app_with(2);
        a.doc.save_to(&p).unwrap();
        assert!(!a.doc.dirty);
        let mut b = App::new();
        b.open(&p);
        assert_eq!(b.doc.script.events.len(), 2);
        assert_eq!(b.doc.script.events[1].text, "line 1");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn macros_run_with_selection() {
        let dir = std::env::temp_dir().join(format!("nsub-gui-macro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lua = dir.join("m.lua");
        std::fs::write(&lua, "script_name='x'\naegisub.register_macro('Mark','d',function(subs,sel) for _,i in ipairs(sel) do local l=subs[i]; l.actor='M'; subs[i]=l end return sel end)\n").unwrap();
        let mut a = app_with(3);
        a.add_script(lua);
        wait(&mut a, |a| !a.macros.is_empty());
        assert_eq!(a.macros.len(), 1);
        a.select_range_to(1);
        a.run_macro(a.macros[0].id);
        wait(&mut a, |a| !a.is_busy());
        assert_eq!(a.doc.script.events[0].actor, "M");
        assert_eq!(a.doc.script.events[1].actor, "M");
        assert_eq!(a.doc.script.events[2].actor, "");
        assert_eq!(a.sel.len(), 2);
        a.undo();
        assert_eq!(a.doc.script.events[0].actor, "");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn macro_dialog_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nsub-gui-dlg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lua = dir.join("d.lua");
        std::fs::write(&lua, "aegisub.register_macro('Ask','d',function(subs,sel) local b,r=aegisub.dialog.display({{class='edit',name='t',value='x'}},{'Apply','Cancel'},{ok='Apply',cancel='Cancel'}); if b ~= 'Apply' then aegisub.cancel() end; local l=subs[sel[1]]; l.text=r.t; subs[sel[1]]=l end)\n").unwrap();
        let mut a = app_with(2);
        a.add_script(lua);
        wait(&mut a, |a| !a.macros.is_empty());
        a.run_macro(a.macros[0].id);
        wait(&mut a, |a| a.pending_dialog.is_some());
        let mut resp = DialogResponse { button: Some("Apply".into()), ..Default::default() };
        resp.values.insert("t".into(), nsub_lua::dialog::Val::Str("typed".into()));
        a.answer_dialog(resp);
        wait(&mut a, |a| !a.is_busy());
        assert_eq!(a.doc.script.events[0].text, "typed");
        // cancel path
        a.run_macro(a.macros[0].id);
        wait(&mut a, |a| a.pending_dialog.is_some());
        a.cancel_macro();
        wait(&mut a, |a| !a.is_busy());
        assert_eq!(a.doc.script.events[0].text, "typed");
        assert!(a.status.contains("cancelled"));
        std::fs::remove_dir_all(dir).ok();
    }
}
