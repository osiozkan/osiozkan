//! Betik motoru: yükleme, makro kaydı, çalıştırma.
use crate::api;
use crate::dialog::{DialogResult, DialogSpec};
use crate::subs::{SharedScript, Subs};
use mlua::{Function, Lua, Table, Value};
use osisub_core::ass::Script;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const CANCEL_MARKER: &str = "__AEGISUB_CANCEL__";

#[derive(Clone, Debug, Default)]
pub struct StyleMetrics {
    pub fontname: String,
    pub fontsize: f64,
    pub bold: bool,
    pub italic: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
}

/// Arayüz/ortam köprüsü. Varsayılanlar başsız (CLI) çalışma içindir.
pub trait Host {
    fn dialog(&mut self, spec: &DialogSpec) -> DialogResult {
        spec.defaults()
    }
    /// (genişlik, yükseklik, descent, extlead). `None` = yerleşik yazı tipi ölçümünü kullan.
    fn text_extents(&mut self, _st: &StyleMetrics, _text: &str) -> Option<(f64, f64, f64, f64)> {
        None
    }
    fn progress(&mut self, _fraction: f64) {}
    fn progress_task(&mut self, _task: &str) {}
    fn progress_title(&mut self, _title: &str) {}
    fn is_cancelled(&mut self) -> bool {
        false
    }
    fn log(&mut self, _level: i64, _msg: &str) {}
    fn fps(&self) -> Option<f64> {
        None
    }
    fn video_size(&self) -> Option<(i32, i32)> {
        None
    }
    /// Anahtar kare numaraları.
    fn keyframes(&self) -> Vec<i64> {
        vec![]
    }
    /// Ses seçimi (başlangıç, bitiş) ms.
    fn audio_selection(&self) -> Option<(i64, i64)> {
        None
    }
    fn open_dialog(&mut self, _title: &str, _dir: &str, _file: &str, _wildcard: &str, _multiple: bool) -> Option<Vec<String>> {
        None
    }
    fn save_dialog(&mut self, _title: &str, _dir: &str, _file: &str, _wildcard: &str) -> Option<String> {
        None
    }
    fn clipboard_get(&mut self) -> Option<String> {
        None
    }
    fn clipboard_set(&mut self, _text: &str) -> bool {
        false
    }
}

/// CLI için: `--param name=value` ile iletişim kutusu değerleri ezilebilir.
#[derive(Default)]
pub struct HeadlessHost {
    pub params: HashMap<String, String>,
    pub fps: Option<f64>,
    pub video_size: Option<(i32, i32)>,
    pub quiet: bool,
}

impl Host for HeadlessHost {
    fn dialog(&mut self, spec: &DialogSpec) -> DialogResult {
        use crate::dialog::DV;
        let mut r = spec.defaults();
        for (k, v) in &self.params {
            if let Some(cur) = r.values.get(k).cloned() {
                let nv = match cur {
                    DV::Bool(_) => DV::Bool(matches!(v.as_str(), "1" | "true" | "yes" | "on")),
                    DV::Num(_) => DV::Num(v.parse().unwrap_or(0.0)),
                    DV::Str(_) => DV::Str(v.clone()),
                };
                r.values.insert(k.clone(), nv);
            }
        }
        r
    }
    fn log(&mut self, _level: i64, msg: &str) {
        if !self.quiet {
            eprint!("{}", msg);
        }
    }
    fn fps(&self) -> Option<f64> {
        self.fps
    }
    fn video_size(&self) -> Option<(i32, i32)> {
        self.video_size
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroKind {
    Macro,
    Filter,
}

#[derive(Clone, Debug)]
pub struct MacroInfo {
    pub id: usize,
    pub name: String,
    pub description: String,
    pub kind: MacroKind,
    pub priority: i64,
    pub script_file: String,
}

#[derive(Clone, Debug, Default)]
pub struct ScriptInfo {
    pub file: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub macros: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct RunOutcome {
    /// Makronun döndürdüğü yeni seçim (1 tabanlı satır indeksleri).
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
    pub undo_desc: Option<String>,
    pub cancelled: bool,
}

#[derive(Clone, Debug)]
pub struct EngineError {
    pub message: String,
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EngineError {}

impl From<mlua::Error> for EngineError {
    fn from(e: mlua::Error) -> Self {
        EngineError { message: e.to_string() }
    }
}

pub(crate) struct MacroDef {
    pub info: MacroInfo,
    pub process: Function,
    pub validate: Option<Function>,
    pub is_active: Option<Function>,
    /// Filtreler için yapılandırma iletişim kutusu işlevi.
    pub config: Option<Function>,
}

pub(crate) struct Inner {
    pub host: Box<dyn Host>,
    pub macros: Vec<MacroDef>,
    pub current_script: String,
    pub log: String,
    pub include_dirs: Vec<PathBuf>,
    pub props: HashMap<String, String>,
    pub undo_desc: Option<String>,
    pub clipboard: String,
    pub fonts: crate::fonts::FontDb,
}

pub struct Engine {
    lua: Lua,
    script: SharedScript,
    inner: Rc<RefCell<Inner>>,
    scripts: Vec<ScriptInfo>,
}

const PRELUDE: &str = include_str!("../lua/prelude.lua");

impl Engine {
    pub fn new(host: Box<dyn Host>) -> Result<Engine, EngineError> {
        // ffi/bit/debug dahil tüm kütüphaneler: kullanıcı betikleri `require "ffi"` yapabilir.
        let lua = unsafe { Lua::unsafe_new() };
        let inner = Rc::new(RefCell::new(Inner {
            host,
            macros: vec![],
            current_script: String::new(),
            log: String::new(),
            include_dirs: vec![],
            props: HashMap::new(),
            undo_desc: None,
            clipboard: String::new(),
            fonts: Default::default(),
        }));
        let script: SharedScript = Rc::new(RefCell::new(Script::new_default()));
        api::install(&lua, &inner, &script)?;
        lua.load(PRELUDE).set_name("=prelude").exec()?;
        Ok(Engine { lua, script, inner, scripts: vec![] })
    }

    pub fn script(&self) -> std::cell::Ref<'_, Script> {
        self.script.borrow()
    }

    pub fn set_script(&mut self, s: Script) {
        *self.script.borrow_mut() = s;
    }

    pub fn take_script(&self) -> Script {
        self.script.borrow().clone()
    }

    pub fn add_font_dir(&mut self, p: impl Into<PathBuf>) {
        self.inner.borrow_mut().fonts.add_dir(p);
    }

    pub fn add_include_dir(&mut self, p: impl Into<PathBuf>) {
        self.inner.borrow_mut().include_dirs.push(p.into());
    }

    /// `aegisub.project_properties()` ve `decode_path` için değerler (script_dir, video_file, audio_file ...).
    pub fn set_property(&mut self, k: &str, v: &str) {
        self.inner.borrow_mut().props.insert(k.to_string(), v.to_string());
    }

    /// Biriken günlüğü alır ve temizler.
    pub fn take_log(&self) -> String {
        std::mem::take(&mut self.inner.borrow_mut().log)
    }

    pub fn macros(&self) -> Vec<MacroInfo> {
        self.inner.borrow().macros.iter().map(|m| m.info.clone()).collect()
    }

    pub fn scripts(&self) -> &[ScriptInfo] {
        &self.scripts
    }

    pub fn load_script(&mut self, path: &Path) -> Result<ScriptInfo, EngineError> {
        let bytes = std::fs::read(path).map_err(|e| EngineError { message: format!("{}: {}", path.display(), e) })?;
        let mut src = osisub_core::ass::decode_bytes(&bytes);
        if src.starts_with('\u{feff}') {
            src.remove(0);
        }
        if src.starts_with("#!") {
            src.insert_str(0, "--");
        }
        let file = path.to_string_lossy().to_string();
        let dir = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let before = self.inner.borrow().macros.len();
        self.inner.borrow_mut().current_script = file.clone();

        let mk: Function = self.lua.globals().get("__new_script_env")?;
        let env: Table = mk.call(dir)?;
        let chunk = self.lua.load(&src).set_name(format!("@{}", file)).set_environment(env.clone());
        chunk.exec()?;

        let g = |k: &str| -> String {
            match env.get::<Value>(k) {
                Ok(Value::String(s)) => s.to_str().map(|x| x.to_string()).unwrap_or_default(),
                Ok(Value::Number(n)) => n.to_string(),
                _ => String::new(),
            }
        };
        let after = self.inner.borrow().macros.len();
        let info = ScriptInfo {
            file,
            name: if g("script_name").is_empty() { path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() } else { g("script_name") },
            description: g("script_description"),
            author: g("script_author"),
            version: g("script_version"),
            macros: (before..after).collect(),
        };
        self.scripts.push(info.clone());
        Ok(info)
    }

    fn sel_table(&self, sel: &[usize]) -> mlua::Result<Table> {
        let t = self.lua.create_table()?;
        for (i, v) in sel.iter().enumerate() {
            t.set(i + 1, *v as i64)?;
        }
        Ok(t)
    }

    pub fn validate_macro(&mut self, id: usize, selection: &[usize], active: usize) -> bool {
        let f = self.inner.borrow().macros.get(id).and_then(|m| m.validate.clone());
        let Some(f) = f else { return true };
        let Ok(sel) = self.sel_table(selection) else { return false };
        let subs = match self.lua.create_userdata(Subs(self.script.clone())) {
            Ok(s) => s,
            Err(_) => return false,
        };
        match f.call::<Value>((subs, sel, active as i64)) {
            Ok(Value::Boolean(b)) => b,
            Ok(Value::Nil) => false,
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn is_macro_active(&mut self, id: usize, selection: &[usize], active: usize) -> bool {
        let f = self.inner.borrow().macros.get(id).and_then(|m| m.is_active.clone());
        let Some(f) = f else { return false };
        let Ok(sel) = self.sel_table(selection) else { return false };
        let Ok(subs) = self.lua.create_userdata(Subs(self.script.clone())) else { return false };
        matches!(f.call::<Value>((subs, sel, active as i64)), Ok(Value::Boolean(true)))
    }

    pub fn run_macro(&mut self, id: usize, selection: &[usize], active: usize) -> Result<RunOutcome, EngineError> {
        let (process, config, kind) = {
            let inner = self.inner.borrow();
            let m = inner.macros.get(id).ok_or_else(|| EngineError { message: format!("makro {} yok", id) })?;
            (m.process.clone(), m.config.clone(), m.info.kind.clone())
        };
        self.inner.borrow_mut().undo_desc = None;
        let subs = self.lua.create_userdata(Subs(self.script.clone()))?;
        let res: mlua::Result<mlua::MultiValue> = match kind {
            MacroKind::Macro => process.call((subs, self.sel_table(selection)?, active as i64)),
            MacroKind::Filter => {
                let cfg = self.lua.create_table()?;
                if let Some(c) = config {
                    let d: Value = c.call((0,))?;
                    if let Value::Table(d) = d {
                        let spec = DialogSpec::from_lua(&d, None, None)?;
                        let r = self.inner.borrow_mut().host.dialog(&spec);
                        if r.button.is_none() {
                            return Ok(RunOutcome { cancelled: true, ..Default::default() });
                        }
                        for (k, v) in r.values {
                            cfg.set(k, api::dv_to_lua(&self.lua, &v)?)?;
                        }
                    }
                }
                process.call((subs, cfg))
            }
        };
        let undo_desc = self.inner.borrow_mut().undo_desc.take();
        match res {
            Ok(vals) => {
                let mut out = RunOutcome { undo_desc, ..Default::default() };
                let mut it = vals.into_iter();
                if let Some(Value::Table(t)) = it.next() {
                    out.selection = Some(t.sequence_values::<i64>().filter_map(|v| v.ok()).map(|v| v as usize).collect());
                }
                match it.next() {
                    Some(Value::Integer(i)) => out.active = Some(i as usize),
                    Some(Value::Number(n)) => out.active = Some(n as usize),
                    _ => {}
                }
                Ok(out)
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains(CANCEL_MARKER) {
                    Ok(RunOutcome { cancelled: true, undo_desc, ..Default::default() })
                } else {
                    Err(EngineError { message: msg })
                }
            }
        }
    }
}
