//! Automation runs on its own thread: Lua states are not `Send`, and macros may block on dialogs.

use asscore::Script;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use yuzu_lua::*;

pub struct ScriptInfo {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub macros: Vec<MacroInfo>,
    pub error: Option<String>,
}

pub enum Cmd {
    Load { files: Vec<PathBuf>, include: Vec<PathBuf> },
    Run { script: usize, macro_idx: usize, doc: Box<Script>, sel: Vec<usize>, active: Option<usize>, project: ProjectInfo },
    /// Validation / is_active of every macro for this document and selection.
    Validate { doc: Box<Script>, sel: Vec<usize>, active: Option<usize>, project: ProjectInfo, token: u64 },
}

pub enum Msg {
    Loaded(Vec<ScriptInfo>),
    Log(i32, String),
    Dialog(DialogSpec, Sender<DialogResult>),
    FileDialog { save: bool, title: String, default: String, reply: Sender<Option<String>> },
    Progress { kind: String, value: f64, text: String },
    Done(Result<(Box<Script>, Outcome), String>),
    /// (script, states of its macros) for the request with this token.
    States(u64, Vec<(usize, Vec<MacroState>)>),
    /// The macro's validation said no for this selection; nothing was run.
    Unavailable(String),
}

struct UiHost {
    tx: Sender<Msg>,
    ctx: eframe::egui::Context,
    project: Arc<Mutex<ProjectInfo>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    clip: Arc<Mutex<String>>,
}

impl UiHost {
    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
        self.ctx.request_repaint();
    }
}

impl Host for UiHost {
    fn log(&self, level: i32, msg: &str) {
        self.send(Msg::Log(level, msg.to_string()));
    }
    fn dialog(&self, spec: &DialogSpec) -> DialogResult {
        let (rtx, rrx) = channel();
        self.send(Msg::Dialog(spec.clone(), rtx));
        rrx.recv().unwrap_or(DialogResult::Cancelled)
    }
    fn progress(&self, kind: &str, value: f64, text: &str) {
        self.send(Msg::Progress { kind: kind.to_string(), value, text: text.to_string() });
    }
    fn is_cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn project(&self) -> ProjectInfo {
        self.project.lock().unwrap().clone()
    }
    fn file_dialog(&self, save: bool, title: &str, default: &str) -> Option<String> {
        let (rtx, rrx) = channel();
        self.send(Msg::FileDialog { save, title: title.into(), default: default.into(), reply: rtx });
        rrx.recv().ok().flatten()
    }
    fn clipboard_get(&self) -> Option<String> {
        Some(self.clip.lock().unwrap().clone())
    }
    fn clipboard_set(&self, s: &str) {
        *self.clip.lock().unwrap() = s.to_string();
    }
}

pub struct Worker {
    pub tx: Sender<Cmd>,
    pub rx: Receiver<Msg>,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

pub fn spawn(ctx: eframe::egui::Context) -> Worker {
    let (ctx_tx, ctx_rx) = channel::<Cmd>();
    let (msg_tx, msg_rx) = channel::<Msg>();
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let c2 = cancel.clone();
    std::thread::Builder::new()
        .name("automation".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let project = Arc::new(Mutex::new(ProjectInfo::default()));
            let host: Rc<UiHost> = Rc::new(UiHost { tx: msg_tx.clone(), ctx: ctx.clone(), project: project.clone(), cancel: c2, clip: Arc::new(Mutex::new(String::new())) });
            let mut scripts: Vec<Option<LuaScript>> = Vec::new();
            while let Ok(cmd) = ctx_rx.recv() {
                match cmd {
                    Cmd::Load { files, include } => {
                        scripts.clear();
                        let mut infos = Vec::new();
                        for f in files {
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| LuaScript::load(&f, host.clone(), &include))) {
                                Ok(Ok(s)) => {
                                    infos.push(ScriptInfo { path: f.clone(), name: s.script_name.clone(), description: s.script_description.clone(), author: s.script_author.clone(), version: s.script_version.clone(), macros: s.macros(), error: None });
                                    scripts.push(Some(s));
                                }
                                Ok(Err(e)) => {
                                    infos.push(ScriptInfo { path: f.clone(), name: file_label(&f), description: String::new(), author: String::new(), version: String::new(), macros: vec![], error: Some(e) });
                                    scripts.push(None);
                                }
                                Err(_) => {
                                    infos.push(ScriptInfo { path: f.clone(), name: file_label(&f), description: String::new(), author: String::new(), version: String::new(), macros: vec![], error: Some("script panicked while loading".into()) });
                                    scripts.push(None);
                                }
                            }
                        }
                        let _ = msg_tx.send(Msg::Loaded(infos));
                        ctx.request_repaint();
                    }
                    Cmd::Run { script, macro_idx, mut doc, sel, active, project: pi } => {
                        *project.lock().unwrap() = pi;
                        host.cancel.store(false, std::sync::atomic::Ordering::Relaxed);
                        let s = scripts.get(script).and_then(|s| s.as_ref());
                        // as in Aegisub, a macro whose validation says no is not run (hotkeys, palette)
                        let info = s.and_then(|s| s.macros().into_iter().nth(macro_idx));
                        if let (Some(s), Some(m)) = (s, info.as_ref()) {
                            if m.has_validate && !s.validate(macro_idx, &doc, &sel, active) {
                                let _ = msg_tx.send(Msg::Unavailable(m.name.clone()));
                                ctx.request_repaint();
                                continue;
                            }
                        }
                        let res = match s {
                            Some(s) => s.run_macro(macro_idx, &mut doc, &sel, active).map(|o| (doc, o)),
                            None => Err("script not loaded".into()),
                        };
                        let _ = msg_tx.send(Msg::Done(res));
                        ctx.request_repaint();
                    }
                    Cmd::Validate { doc, sel, active, project: pi, token } => {
                        *project.lock().unwrap() = pi;
                        let states: Vec<(usize, Vec<MacroState>)> = scripts.iter().enumerate()
                            .filter_map(|(i, s)| s.as_ref().filter(|s| s.macros().iter().any(|m| m.has_validate || m.has_active)).map(|s| (i, s.macro_states(&doc, &sel, active))))
                            .collect();
                        let _ = msg_tx.send(Msg::States(token, states));
                        ctx.request_repaint();
                    }
                }
            }
        })
        .expect("spawn automation thread");
    Worker { tx: ctx_tx, rx: msg_rx, cancel }
}

fn file_label(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Find automation scripts in standard locations: Yuzu's own dir plus existing Aegisub dirs.
pub fn default_script_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    // Bundled scripts next to the executable (release zip) and in the source tree (development).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("automation/autoload"));
            v.push(d.join("../../automation/autoload"));
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(h) = &home {
        v.push(h.join(".config/yuzu/automation/autoload"));
        v.push(h.join(".aegisub/automation/autoload"));
    }
    if let Some(a) = std::env::var_os("APPDATA").map(PathBuf::from) {
        v.push(a.join("Yuzu/automation/autoload"));
        v.push(a.join("Aegisub/automation/autoload"));
    }
    v
}

pub fn scan_scripts(dirs: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut include = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut es: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            es.sort();
            for p in es {
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if ext == "lua" || ext == "moon" { files.push(p); }
            }
        }
        if let Some(parent) = d.parent() {
            include.push(parent.join("include"));
            include.push(parent.to_path_buf());
        }
        include.push(d.clone());
    }
    (files, include)
}
