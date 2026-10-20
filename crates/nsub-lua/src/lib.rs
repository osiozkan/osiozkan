//! Aegisub-compatible Lua automation runtime (LuaJIT, Automation 4 API).

mod api;
mod convert;
pub mod dialog;
pub use nsub_core::metrics;
mod relib;

use mlua::{Function, Lua, Table, Value};
use nsub_core::ass::Script;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub use nsub_core::metrics::{Extents, HeuristicMetrics, TextMetrics};

pub(crate) const CANCEL_MARK: &str = "__nsub_cancel__";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroKind {
    Macro,
    Filter,
}

#[derive(Clone, Debug)]
pub struct MacroInfo {
    pub id: usize,
    pub kind: MacroKind,
    pub name: String,
    pub description: String,
    pub script_file: String,
    pub has_validate: bool,
    pub has_is_active: bool,
}

#[derive(Debug, Default)]
pub struct RunResult {
    pub selection: Vec<usize>,
    pub active: Option<usize>,
    pub canceled: bool,
    pub log: String,
}

pub struct Options {
    /// Shows dialogs requested by `aegisub.dialog.display`; headless defaults are used when unset.
    pub dialog_handler: Option<dialog::DialogHandler>,
    pub progress_handler: Option<dialog::ProgressHandler>,
    /// Set by the host to ask a running macro to stop (`aegisub.progress.is_cancelled`).
    pub cancel_flag: Arc<AtomicBool>,
    pub fps: f64,
    pub script_path: Option<PathBuf>,
    pub include_dirs: Vec<PathBuf>,
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    /// Pre-seeded answers for `aegisub.dialog.display`, keyed by control name.
    pub dialog_answers: HashMap<String, String>,
    /// Button label chosen in headless dialogs (default: the first button, or `true`).
    pub dialog_button: Option<String>,
    /// Echo log output to stderr as it is produced.
    pub echo_log: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            dialog_handler: None,
            progress_handler: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            fps: 23.976,
            script_path: None,
            include_dirs: vec![],
            video_path: None,
            audio_path: None,
            dialog_answers: HashMap::new(),
            dialog_button: None,
            echo_log: true,
        }
    }
}

pub(crate) struct Shared {
    pub opts: Options,
    pub entries: Vec<convert::Entry>,
    pub log: String,
    pub progress: f64,
    pub cancelled: bool,
    pub current_script_file: String,
    pub clipboard: String,
    pub metrics: Box<dyn TextMetrics>,
    pub res: (i32, i32),
    pub ycbcr: String,
}

pub struct Automation {
    lua: Lua,
    shared: Rc<RefCell<Shared>>,
    registry: Table,
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

impl Automation {
    pub fn new(opts: Options) -> Result<Automation, String> {
        let lua = Lua::new();
        let shared = Rc::new(RefCell::new(Shared {
            opts,
            entries: vec![],
            log: String::new(),
            progress: 0.0,
            cancelled: false,
            current_script_file: String::new(),
            clipboard: String::new(),
            metrics: Box::new(HeuristicMetrics),
            res: (1920, 1080),
            ycbcr: "None".into(),
        }));
        let registry = lua.create_table().map_err(err)?;
        lua.set_named_registry_value("nsub_macros", registry.clone()).map_err(err)?;
        api::install(&lua, &shared).map_err(err)?;
        relib::install(&lua).map_err(err)?;
        api::install_prelude(&lua, &shared).map_err(err)?;
        Ok(Automation { lua, shared, registry })
    }

    pub fn set_metrics(&self, m: Box<dyn TextMetrics>) {
        self.shared.borrow_mut().metrics = m;
    }

    pub fn log(&self) -> String {
        self.shared.borrow().log.clone()
    }

    /// Load an automation script, returning the macros/filters it registered.
    pub fn load_script(&mut self, path: &Path) -> Result<Vec<MacroInfo>, String> {
        let src = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let before = self.registry.raw_len();
        let file = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        {
            let mut sh = self.shared.borrow_mut();
            sh.current_script_file = file.clone();
            if sh.opts.script_path.is_none() {
                sh.opts.script_path = Some(path.to_path_buf());
            }
        }
        // Each script gets the script's directory on package.path, like Aegisub.
        if let Some(dir) = path.parent() {
            let d = dir.to_string_lossy().replace('\\', "/");
            let pkg: Table = self.lua.globals().get("package").map_err(err)?;
            let cur: String = pkg.get("path").unwrap_or_default();
            pkg.set("path", format!("{d}/?.lua;{d}/?/init.lua;{cur}")).map_err(err)?;
        }
        let g = self.lua.globals();
        for k in ["script_name", "script_description", "script_author", "script_version"] {
            g.set(k, Value::Nil).map_err(err)?;
        }
        self.lua
            .load(&src[..])
            .set_name(format!("@{}", path.display()))
            .exec()
            .map_err(|e| format!("{}: {}", path.display(), clean_err(&e.to_string())))?;
        // Scripts without register_macro/filter that define script_name+process_macro are not supported by Aegisub either.
        Ok(self.macros().into_iter().skip(before).collect())
    }

    pub fn macros(&self) -> Vec<MacroInfo> {
        let mut out = Vec::new();
        for i in 1..=self.registry.raw_len() {
            if let Ok(t) = self.registry.get::<Table>(i) {
                out.push(MacroInfo {
                    id: i - 1,
                    kind: if t.get::<String>("kind").unwrap_or_default() == "filter" { MacroKind::Filter } else { MacroKind::Macro },
                    name: t.get("name").unwrap_or_default(),
                    description: t.get("description").unwrap_or_default(),
                    script_file: t.get("script_file").unwrap_or_default(),
                    has_validate: t.get::<Function>("validate").is_ok(),
                    has_is_active: t.get::<Function>("is_active").is_ok(),
                });
            }
        }
        out
    }

    fn entry(&self, id: usize) -> Result<Table, String> {
        self.registry.get::<Table>(id + 1).map_err(|_| format!("no macro with id {id}"))
    }

    /// 1-based selection indices refer to the flat Aegisub line list (info, styles, events).
    pub fn flat_index_of_event(script: &Script, event_idx: usize) -> usize {
        script.info.len() + script.styles.len() + event_idx + 1
    }

    pub fn validate(&self, id: usize, script: &Script, selection: &[usize], active: usize) -> Result<bool, String> {
        let t = self.entry(id)?;
        let Ok(f) = t.get::<Function>("validate") else { return Ok(true) };
        self.shared.borrow_mut().entries = convert::flatten(script);
        let subs = api::make_subs(&self.lua, &self.shared).map_err(err)?;
        let sel = self.lua.create_sequence_from(selection.iter().copied()).map_err(err)?;
        let r: Value = f.call((subs, sel, active)).map_err(|e| clean_err(&e.to_string()))?;
        Ok(!matches!(r, Value::Nil | Value::Boolean(false)))
    }

    /// Run a macro (or filter) against `script`. On error or cancel the script is left untouched.
    pub fn run(&self, id: usize, script: &mut Script, selection: &[usize], active: usize) -> Result<RunResult, String> {
        let t = self.entry(id)?;
        let process: Function = t.get("process").map_err(err)?;
        let is_filter = t.get::<String>("kind").unwrap_or_default() == "filter";
        {
            let mut sh = self.shared.borrow_mut();
            sh.entries = convert::flatten(script);
            sh.log.clear();
            sh.cancelled = false;
            sh.progress = 0.0;
            sh.opts.script_path.get_or_insert_with(PathBuf::new);
        }
        api::set_script_context(&self.shared, script);
        let subs = api::make_subs(&self.lua, &self.shared).map_err(err)?;
        let sel = self.lua.create_sequence_from(selection.iter().copied()).map_err(err)?;
        let ret: Result<mlua::MultiValue, _> = if is_filter {
            let cfg = self.lua.create_table().map_err(err)?;
            process.call((subs, cfg))
        } else {
            process.call((subs, sel, active))
        };
        let log = self.shared.borrow().log.clone();
        match ret {
            Ok(vals) => {
                let entries = std::mem::take(&mut self.shared.borrow_mut().entries);
                convert::unflatten(entries, script);
                let mut res = RunResult { log, ..Default::default() };
                let mut it = vals.into_iter();
                if let Some(Value::Table(t)) = it.next() {
                    res.selection = t.sequence_values::<usize>().filter_map(|v| v.ok()).collect();
                }
                if let Some(v) = it.next() {
                    res.active = match v {
                        Value::Integer(i) if i > 0 => Some(i as usize),
                        Value::Number(n) if n > 0.0 => Some(n as usize),
                        _ => None,
                    };
                }
                Ok(res)
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains(CANCEL_MARK) {
                    Ok(RunResult { canceled: true, log, ..Default::default() })
                } else {
                    Err(format!("{}\n{}", clean_err(&msg), log))
                }
            }
        }
    }

    /// Execute arbitrary Lua (for tests and `nightsub lua`).
    pub fn eval(&self, code: &str) -> Result<String, String> {
        let v: Value = self.lua.load(code).eval().map_err(|e| clean_err(&e.to_string()))?;
        Ok(match v {
            Value::Nil => String::new(),
            Value::String(s) => s.to_string_lossy().to_string(),
            other => format!("{:?}", other),
        })
    }
}

fn clean_err(s: &str) -> String {
    s.lines().filter(|l| !l.trim_start().starts_with("stack traceback")).take(8).collect::<Vec<_>>().join("\n")
}
