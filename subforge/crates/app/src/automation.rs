//! Discover Automation scripts and run macros on a worker thread so the UI
//! stays responsive while scripts show dialogs, log and report progress.
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use subforge_core::ass::Script;
use subforge_lua::{Dialog, DialogResult, Doc, Host, LuaScript, Outcome, ScriptError, SharedHost};

#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub macros: Vec<(String, String)>,
    pub filters: Vec<(String, String)>,
    pub filter_prio: Vec<i64>,
    pub error: Option<String>,
}

pub struct SilentHost;
impl Host for SilentHost {
    fn log(&mut self, _: i32, _: &str) {}
    fn dialog(&mut self, _: &Dialog) -> Option<DialogResult> {
        None
    }
}

/// Load every `*.lua` (non-recursive) in the given dirs just to read metadata.
pub fn scan(dirs: &[PathBuf]) -> Vec<ScriptEntry> {
    scan_with(dirs, &[])
}

pub fn scan_with(dirs: &[PathBuf], include: &[PathBuf]) -> Vec<ScriptEntry> {
    let mut out = vec![];
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "lua" || e == "moon").unwrap_or(false)).collect();
        files.sort();
        for p in files {
            if p.extension().map(|e| e == "moon").unwrap_or(false) {
                continue; // MoonScript needs a compiler; planned
            }
            out.push(inspect_with(&p, include));
        }
    }
    out
}

pub fn inspect(path: &Path) -> ScriptEntry {
    inspect_with(path, &[])
}

pub fn inspect_with(path: &Path, include: &[PathBuf]) -> ScriptEntry {
    let host: SharedHost = Rc::new(RefCell::new(SilentHost));
    let s = LuaScript::load(path, host, include);
    ScriptEntry {
        path: path.to_path_buf(),
        name: s.name.clone(),
        description: s.description.clone(),
        macros: s.macros.iter().map(|m| (m.name.clone(), m.description.clone())).collect(),
        filters: s.filters.iter().map(|f| (f.name.clone(), f.description.clone())).collect(),
        filter_prio: s.filters.iter().map(|f| f.priority).collect(),
        error: s.load_error.clone(),
    }
}

pub enum RunMsg {
    Log(i32, String),
    Progress(f64),
    Task(String),
    Dialog(Dialog, Sender<Option<DialogResult>>),
    Done(Result<RunResult, ScriptError>),
}

pub struct RunResult {
    pub script: Script,
    pub outcome: Outcome,
    /// 1-based flat→event mapping helper: number of non-event lines before events.
    pub event_offset: usize,
}

struct ChannelHost {
    tx: Sender<RunMsg>,
    cancel: Arc<AtomicBool>,
    fps: Option<f64>,
    video_size: Option<(i32, i32)>,
    script_path: Option<PathBuf>,
    video_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    keyframes: Vec<i64>,
}

impl Host for ChannelHost {
    fn log(&mut self, level: i32, msg: &str) {
        let _ = self.tx.send(RunMsg::Log(level, msg.to_string()));
    }
    fn progress_set(&mut self, p: f64) {
        let _ = self.tx.send(RunMsg::Progress(p));
    }
    fn progress_task(&mut self, m: &str) {
        let _ = self.tx.send(RunMsg::Task(m.to_string()));
    }
    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn dialog(&mut self, d: &Dialog) -> Option<DialogResult> {
        let (rtx, rrx) = channel();
        self.tx.send(RunMsg::Dialog(d.clone(), rtx)).ok()?;
        rrx.recv().ok().flatten()
    }
    fn fps(&self) -> Option<f64> {
        self.fps
    }
    fn video_size(&self) -> Option<(i32, i32)> {
        self.video_size
    }
    fn keyframes(&self) -> Option<Vec<i64>> {
        (!self.keyframes.is_empty()).then(|| self.keyframes.clone())
    }
    fn script_path(&self) -> Option<PathBuf> {
        self.script_path.clone()
    }
    fn video_path(&self) -> Option<PathBuf> {
        self.video_path.clone()
    }
    fn audio_path(&self) -> Option<PathBuf> {
        self.audio_path.clone()
    }
}

pub struct RunContext {
    pub fps: Option<f64>,
    pub video_size: Option<(i32, i32)>,
    pub script_path: Option<PathBuf>,
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub keyframes: Vec<i64>,
    pub include_dirs: Vec<PathBuf>,
}

pub struct RunHandle {
    pub rx: Receiver<RunMsg>,
    pub cancel: Arc<AtomicBool>,
}

/// Start `macro_idx` of `script_path` on a copy of `script`.
/// `sel_events` are 0-based event indices.
pub fn spawn_macro(script_path: PathBuf, macro_idx: usize, script: Script, sel_events: Vec<usize>, active_event: usize, ctx: RunContext) -> RunHandle {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    thread::spawn(move || {
        let host: SharedHost = Rc::new(RefCell::new(ChannelHost {
            tx: tx.clone(),
            cancel: c2,
            fps: ctx.fps,
            video_size: ctx.video_size,
            script_path: ctx.script_path,
            video_path: ctx.video_path,
            audio_path: ctx.audio_path,
            keyframes: ctx.keyframes,
        }));
        let result = (|| {
            let s = LuaScript::load(&script_path, host, &ctx.include_dirs);
            if let Some(e) = &s.load_error {
                return Err(ScriptError::Failed(format!("failed to load script: {e}")));
            }
            let doc = Rc::new(RefCell::new(Doc::from_script(&script)));
            let (ni, ns, _) = doc.borrow().counts();
            let sel: Vec<usize> = sel_events.iter().map(|&e| doc.borrow().event_to_flat(e)).collect();
            let active = doc.borrow().event_to_flat(active_event);
            if !s.validate_macro(macro_idx, doc.clone(), &sel, active) {
                return Err(ScriptError::Failed("this macro is not available for the current selection".into()));
            }
            let outcome = s.run_macro(macro_idx, doc.clone(), &sel, active)?;
            let out = doc.borrow().to_script();
            let _ = (ni, ns);
            let event_offset = {
                let d = doc.borrow();
                let (a, b, _) = d.counts();
                a + b
            };
            Ok(RunResult { script: out, outcome, event_offset })
        })();
        let _ = tx.send(RunMsg::Done(result));
    });
    RunHandle { rx, cancel }
}

/// Apply export filters in order (each `(script path, filter index)`), showing any config
/// dialogs through the host. Result is the filtered copy of `script`.
pub fn spawn_filters(items: Vec<(PathBuf, usize)>, script: Script, ctx: RunContext) -> RunHandle {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    thread::spawn(move || {
        let host: SharedHost = Rc::new(RefCell::new(ChannelHost {
            tx: tx.clone(),
            cancel: c2,
            fps: ctx.fps,
            video_size: ctx.video_size,
            script_path: ctx.script_path,
            video_path: ctx.video_path,
            audio_path: ctx.audio_path,
            keyframes: ctx.keyframes,
        }));
        let result = (|| {
            let doc = Rc::new(RefCell::new(Doc::from_script(&script)));
            for (path, idx) in &items {
                let s = LuaScript::load(path, host.clone(), &ctx.include_dirs);
                if let Some(e) = &s.load_error {
                    return Err(ScriptError::Failed(format!("failed to load {}: {e}", path.display())));
                }
                let mut cfg = std::collections::HashMap::new();
                if let Some(d) = s.filter_dialog(*idx, doc.clone())? {
                    match host.borrow_mut().dialog(&d) {
                        Some(r) => cfg = r.values,
                        None => return Err(ScriptError::Cancelled),
                    }
                }
                s.run_filter(*idx, doc.clone(), cfg)?;
            }
            let out = doc.borrow().to_script();
            Ok(RunResult { script: out, outcome: Outcome::default(), event_offset: 0 })
        })();
        let _ = tx.send(RunMsg::Done(result));
    });
    RunHandle { rx, cancel }
}

/// Convert a macro's returned flat selection into 0-based event indices.
pub fn selection_to_events(r: &RunResult) -> Option<Vec<usize>> {
    r.outcome.selection.as_ref().map(|sel| sel.iter().filter_map(|&f| f.checked_sub(1)?.checked_sub(r.event_offset)).filter(|&e| e < r.script.events.len()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use subforge_core::ass::Event;

    #[test]
    fn export_filter_with_config_dialog() {
        let dir = std::env::temp_dir().join(format!("sf-app-f-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("f.lua");
        std::fs::write(
            &p,
            r#"script_name="F"
aegisub.register_filter("Suffix", "adds suffix", 5, function(subs, cfg)
  for i = 1, #subs do local l = subs[i]; if l.class == "dialogue" then l.text = l.text .. cfg.suffix; subs[i] = l end end
end, function(subs, cfg) return {{class="edit", name="suffix", value="!"}} end)"#,
        )
        .unwrap();
        let e = inspect(&p);
        assert_eq!(e.filters.len(), 1);
        assert_eq!(e.filter_prio, vec![5]);
        let mut s = Script::new();
        s.events.push(Event::new(0, 1000, "Default", "x"));
        let h = spawn_filters(vec![(p, 0)], s, RunContext { fps: None, video_size: None, script_path: None, video_path: None, audio_path: None, keyframes: vec![], include_dirs: vec![] });
        let mut out = None;
        for m in h.rx.iter() {
            match m {
                RunMsg::Dialog(_, tx) => {
                    let mut vals = std::collections::HashMap::new();
                    vals.insert("suffix".to_string(), subforge_lua::DialogValue::Str("?!".into()));
                    tx.send(Some(DialogResult { button: None, values: vals })).unwrap();
                }
                RunMsg::Done(r) => {
                    out = Some(r);
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(out.unwrap().unwrap().script.events[0].text, "x?!");
    }

    #[test]
    fn run_on_thread_with_dialog() {
        let dir = std::env::temp_dir().join(format!("sf-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.lua");
        std::fs::write(
            &p,
            r#"script_name="T"
aegisub.register_macro("go","d",function(subs, sel)
  local b, r = aegisub.dialog.display({{class="intedit", name="n", value=5}}, {"OK","Cancel"})
  if not b then aegisub.cancel() end
  for _, i in ipairs(sel) do local l = subs[i]; l.text = "n=" .. r.n; subs[i] = l end
  aegisub.progress.set(100)
  return sel
end)"#,
        )
        .unwrap();
        let entries = scan(&[dir.clone()]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].macros[0].0, "go");
        let mut s = Script::new();
        s.events.push(Event::new(0, 1000, "Default", "x"));
        let h = spawn_macro(p, 0, s, vec![0], 0, RunContext { fps: None, video_size: None, script_path: None, video_path: None, audio_path: None, keyframes: vec![], include_dirs: vec![] });
        let mut done = None;
        for m in h.rx.iter() {
            match m {
                RunMsg::Dialog(d, tx) => {
                    assert_eq!(d.fields.len(), 1);
                    let mut vals = std::collections::HashMap::new();
                    vals.insert("n".to_string(), subforge_lua::DialogValue::Num(42.0));
                    tx.send(Some(DialogResult { button: Some("OK".into()), values: vals })).unwrap();
                }
                RunMsg::Done(r) => {
                    done = Some(r);
                    break;
                }
                _ => {}
            }
        }
        let r = done.unwrap().unwrap();
        assert_eq!(r.script.events[0].text, "n=42");
        assert_eq!(selection_to_events(&r), Some(vec![0]));
    }
}
