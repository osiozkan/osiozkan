//! Aegisub-compatible Lua automation runtime for Kaku.
//!
//! Existing Aegisub macros/filters (`aegisub.register_macro`, `karaskel`, `util`, `re`, `unicode`,
//! `aegisub.dialog.display`, `subs[i]`, …) run unmodified on LuaJIT.

mod convert;
mod natives;
mod subs;

use kaku_core::ass::Script;
use mlua::{Function, Lua, MultiValue, Result as LResult, Table, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
pub use subs::Shared;

const UTIL_LUA: &str = include_str!("../lua/util.lua");
const KARASKEL_LUA: &str = include_str!("../lua/karaskel.lua");
const PRELUDE_LUA: &str = include_str!("../lua/prelude.lua");
const CANCEL_MARK: &str = "__kaku_cancel__";
const NEED_DIALOG_MARK: &str = "__kaku_need_dialog__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Macro,
    Filter,
}

#[derive(Debug, Clone)]
pub struct Registered {
    pub kind: Kind,
    pub name: String,
    pub description: String,
    pub index: usize,
}

/// One control of an `aegisub.dialog.display` config.
#[derive(Debug, Clone, Default)]
pub struct Control {
    pub class: String,
    pub name: String,
    pub label: String,
    /// Default value rendered as text (checkbox: "true"/"false").
    pub value: String,
    pub items: Vec<String>,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub hint: String,
}

#[derive(Debug, Clone, Default)]
pub struct DialogSpec {
    pub controls: Vec<Control>,
    pub buttons: Vec<String>,
}

/// A user's answer to the n-th dialog shown during a run (`button == None` means cancelled).
#[derive(Debug, Clone, Default)]
pub struct DialogAnswer {
    pub button: Option<String>,
    pub values: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct RunOutput {
    /// Set when the macro needs user input that was not supplied (probe mode).
    pub pending_dialog: Option<DialogSpec>,
    pub log: Vec<String>,
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
    pub cancelled: bool,
}

#[derive(Debug)]
pub enum Error {
    Lua(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Lua(s) => write!(f, "lua: {s}"),
            Error::Other(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for Error {}

impl From<mlua::Error> for Error {
    fn from(e: mlua::Error) -> Self {
        Error::Lua(e.to_string())
    }
}

/// Frame-rate / video info a host may provide so frame functions work.
#[derive(Debug, Clone, Default)]
pub struct VideoInfo {
    pub fps: Option<(u32, u32)>,
    pub size: Option<(u32, u32)>,
}

pub struct Runtime {
    lua: Lua,
    log: Rc<RefCell<Vec<String>>>,
    /// Overrides for `aegisub.dialog.display` controls by name (headless/CI use).
    pub dialog_values: Rc<RefCell<HashMap<String, String>>>,
    /// Button to "press" in headless dialogs (default: first).
    pub dialog_button: Rc<RefCell<Option<String>>>,
    video: Rc<RefCell<VideoInfo>>,
    script_path: Rc<RefCell<Option<PathBuf>>>,
    include_dirs: Rc<RefCell<Vec<PathBuf>>>,
    /// Pre-supplied answers for dialogs, consumed in order of appearance.
    pub answers: Rc<RefCell<Vec<DialogAnswer>>>,
    /// When true, a dialog without a supplied answer aborts the run and reports its spec.
    pub probe_dialogs: Rc<std::cell::Cell<bool>>,
    dialog_counter: Rc<std::cell::Cell<usize>>,
    pending: Rc<RefCell<Option<DialogSpec>>>,
}

impl Runtime {
    pub fn new() -> Result<Runtime, Error> {
        let rt = Runtime {
            lua: Lua::new(),
            log: Rc::new(RefCell::new(Vec::new())),
            dialog_values: Rc::new(RefCell::new(HashMap::new())),
            dialog_button: Rc::new(RefCell::new(None)),
            video: Rc::new(RefCell::new(VideoInfo::default())),
            script_path: Rc::new(RefCell::new(None)),
            include_dirs: Rc::new(RefCell::new(Vec::new())),
            answers: Rc::new(RefCell::new(Vec::new())),
            probe_dialogs: Rc::new(std::cell::Cell::new(false)),
            dialog_counter: Rc::new(std::cell::Cell::new(0)),
            pending: Rc::new(RefCell::new(None)),
        };
        rt.setup()?;
        Ok(rt)
    }

    pub fn add_include_dir(&self, p: impl Into<PathBuf>) {
        self.include_dirs.borrow_mut().push(p.into());
    }

    pub fn set_video(&self, v: VideoInfo) {
        *self.video.borrow_mut() = v;
    }

    pub fn take_log(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.borrow_mut())
    }

    fn setup(&self) -> LResult<()> {
        let lua = &self.lua;
        let g = lua.globals();

        // package.preload for bundled libraries.
        let package: Table = g.get("package")?;
        let preload: Table = package.get("preload")?;
        let util_src = UTIL_LUA;
        preload.set("util", lua.create_function(move |lua, ()| lua.load(util_src).set_name("=util.lua").eval::<Value>())?)?;
        preload.set("aegisub.util", lua.create_function(|lua, ()| lua.globals().get::<Function>("require")?.call::<Value>("util"))?)?;
        preload.set("karaskel", lua.create_function(|lua, ()| lua.load(KARASKEL_LUA).set_name("=karaskel.lua").eval::<Value>())?)?;
        let re = natives::install_re(lua)?;
        preload.set("re", lua.create_function(move |_, ()| Ok(re.clone()))?)?;
        let uni = natives::install_unicode(lua)?;
        preload.set("unicode", lua.create_function(move |_, ()| Ok(uni.clone()))?)?;
        preload.set("clipboard", lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            t.set("get", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
            t.set("set", lua.create_function(|_, _s: Value| Ok(false))?)?;
            Ok(t)
        })?)?;

        // include()
        let dirs = self.include_dirs.clone();
        let sp = self.script_path.clone();
        g.set("include", lua.create_function(move |lua, name: String| {
            let stem = name.trim_end_matches(".lua");
            if matches!(stem, "util" | "karaskel" | "utils") {
                let req: Function = lua.globals().get("require")?;
                let module = if stem == "utils" { "util" } else { stem };
                let m: Value = req.call(module)?;
                if stem == "util" || stem == "utils" {
                    lua.globals().set("util", m)?;
                } else {
                    lua.globals().set("karaskel", m)?;
                }
                return Ok(());
            }
            let mut cands: Vec<PathBuf> = Vec::new();
            if let Some(p) = sp.borrow().as_ref().and_then(|p| p.parent()) {
                cands.push(p.join(&name));
            }
            for d in dirs.borrow().iter() {
                cands.push(d.join(&name));
            }
            cands.push(PathBuf::from(&name));
            for c in cands {
                if c.is_file() {
                    let src = std::fs::read(&c).map_err(|e| mlua::Error::runtime(e.to_string()))?;
                    return lua.load(&src[..]).set_name(format!("@{}", c.display())).exec();
                }
            }
            Err(mlua::Error::runtime(format!("Lua include not found: {}", name)))
        })?)?;

        // Extend package.path with include dirs lazily via a searcher-friendly string.
        self.refresh_package_path()?;

        // Registry for macros/filters.
        let reg = lua.create_table()?;
        g.set("__kaku_registry", reg)?;

        let aegisub = lua.create_table()?;
        aegisub.set("lua_automation_version", 4)?;
        aegisub.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
        aegisub.set("register_macro", lua.create_function(|lua, (name, desc, process, validate, is_active): (String, Option<String>, Function, Option<Function>, Option<Function>)| {
            let reg: Table = lua.globals().get("__kaku_registry")?;
            let e = lua.create_table()?;
            e.set("kind", "macro")?;
            e.set("name", name)?;
            e.set("description", desc.unwrap_or_default())?;
            e.set("process", process)?;
            e.set("validate", validate)?;
            e.set("is_active", is_active)?;
            reg.push(e)?;
            Ok(())
        })?)?;
        aegisub.set("register_filter", lua.create_function(|lua, (name, desc, prio, process, cfg): (String, Option<String>, Option<i64>, Function, Option<Function>)| {
            let reg: Table = lua.globals().get("__kaku_registry")?;
            let e = lua.create_table()?;
            e.set("kind", "filter")?;
            e.set("name", name)?;
            e.set("description", desc.unwrap_or_default())?;
            e.set("priority", prio.unwrap_or(0))?;
            e.set("process", process)?;
            e.set("config", cfg)?;
            reg.push(e)?;
            Ok(())
        })?)?;

        // Logging
        let log = self.log.clone();
        let debug_out = lua.create_function(move |lua, args: MultiValue| {
            let mut v: Vec<Value> = args.into_iter().collect();
            if matches!(v.first(), Some(Value::Integer(_)) | Some(Value::Number(_))) && v.len() >= 2 {
                v.remove(0);
            }
            if v.is_empty() {
                return Ok(());
            }
            let fmt: Function = lua.globals().get::<Table>("string")?.get("format")?;
            let msg: String = if v.len() > 1 {
                match fmt.call::<String>(MultiValue::from_vec(v.clone())) {
                    Ok(m) => m,
                    Err(_) => lua_tostring(lua, &v[0])?,
                }
            } else {
                lua_tostring(lua, &v[0])?
            };
            log.borrow_mut().push(msg);
            Ok(())
        })?;
        let debug = lua.create_table()?;
        debug.set("out", debug_out.clone())?;
        aegisub.set("debug", debug)?;
        aegisub.set("log", debug_out)?;

        // Progress & cancel
        let progress = lua.create_table()?;
        progress.set("set", lua.create_function(|_, _p: Value| Ok(()))?)?;
        progress.set("task", lua.create_function(|_, _p: MultiValue| Ok(()))?)?;
        progress.set("title", lua.create_function(|_, _p: MultiValue| Ok(()))?)?;
        progress.set("is_cancelled", lua.create_function(|_, ()| Ok(false))?)?;
        aegisub.set("progress", progress)?;
        aegisub.set("cancel", lua.create_function(|_, ()| -> LResult<()> { Err(mlua::Error::runtime(CANCEL_MARK)) })?)?;

        // Video/project info
        let v = self.video.clone();
        aegisub.set("frame_from_ms", lua.create_function({
            let v = v.clone();
            move |_, ms: f64| Ok(v.borrow().fps.map(|(n, d)| (ms * n as f64 / (1000.0 * d as f64)).floor() as i64))
        })?)?;
        aegisub.set("ms_from_frame", lua.create_function({
            let v = v.clone();
            move |_, f: f64| Ok(v.borrow().fps.map(|(n, d)| (f * 1000.0 * d as f64 / n as f64).ceil() as i64))
        })?)?;
        aegisub.set("video_size", lua.create_function({
            let v = v.clone();
            move |_, ()| Ok(match v.borrow().size { Some((w, h)) => MultiValue::from_vec(vec![Value::Integer(w as i64), Value::Integer(h as i64), Value::Integer(0), Value::Integer(0)]), None => MultiValue::new() })
        })?)?;
        aegisub.set("keyframes", lua.create_function(|lua, ()| lua.create_table())?)?;
        aegisub.set("get_audio_selection", lua.create_function(|_, ()| Ok(MultiValue::new()))?)?;
        aegisub.set("project_properties", lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            for k in ["audio_file", "video_file", "timecodes_file", "keyframes_file"] {
                t.set(k, "")?;
            }
            t.set("ar_mode", 0)?;
            t.set("ar_value", 0.0)?;
            t.set("video_position", 0)?;
            t.set("scroll_position", 0)?;
            t.set("active_row", 0)?;
            Ok(t)
        })?)?;
        aegisub.set("decode_path", lua.create_function(|_, p: String| {
            let p = p.replace("?user", "~/.kaku").replace("?data", ".").replace("?temp", &std::env::temp_dir().to_string_lossy()).replace("?script", ".");
            Ok(p)
        })?)?;
        aegisub.set("file_name", lua.create_function(|_, ()| Ok("untitled.ass"))?)?;

        // text_extents(style, text)
        aegisub.set("text_extents", lua.create_function(|_, (style, text): (Table, String)| {
            let text = text.replace("\\N", "").replace("\\n", "");
            let size = style.get::<Option<f64>>("fontsize")?.unwrap_or(48.0);
            let sx = style.get::<Option<f64>>("scale_x")?.unwrap_or(100.0);
            let sy = style.get::<Option<f64>>("scale_y")?.unwrap_or(100.0);
            let sp = style.get::<Option<f64>>("spacing")?.unwrap_or(0.0);
            let bold = style.get::<Option<bool>>("bold")?.unwrap_or(false);
            let family = style.get::<Option<String>>("fontname")?.unwrap_or_default();
            let italic = style.get::<Option<bool>>("italic")?.unwrap_or(false);
            if let Some(e) = kaku_core::fonts::measure(&family, bold, italic, size, sx, sy, sp, &text) {
                return Ok((e.width, e.height, e.descent, e.extlead));
            }
            Ok(natives::approx_extents(size, sx, sy, sp, bold, &text))
        })?)?;

        // dialogs
        let dialog = lua.create_table()?;
        let vals = self.dialog_values.clone();
        let btn = self.dialog_button.clone();
        let answers = self.answers.clone();
        let probe = self.probe_dialogs.clone();
        let counter = self.dialog_counter.clone();
        let pending = self.pending.clone();
        dialog.set("display", lua.create_function(move |lua, (controls, buttons, _ids): (Table, Option<Table>, Option<Table>)| {
            let n = counter.get();
            counter.set(n + 1);
            let supplied: Option<DialogAnswer> = answers.borrow().get(n).cloned();
            if supplied.is_none() && probe.get() {
                *pending.borrow_mut() = Some(read_spec(&controls, buttons.as_ref())?);
                return Err(mlua::Error::runtime(NEED_DIALOG_MARK));
            }
            if let Some(a) = &supplied {
                match &a.button {
                    None => {
                        // Cancelled: emulate pressing Cancel -> false.
                        return Ok(MultiValue::from_vec(vec![Value::Boolean(false), Value::Table(lua.create_table()?)]));
                    }
                    Some(_) => {}
                }
            }
            let result = lua.create_table()?;
            for c in controls.sequence_values::<Table>() {
                let c = c?;
                let Some(name) = c.get::<Option<String>>("name")? else { continue };
                let class: String = c.get::<Option<String>>("class")?.unwrap_or_default();
                let default: Value = c.get("value")?;
                let over = supplied.as_ref().and_then(|a| a.values.get(&name).cloned()).or_else(|| vals.borrow().get(&name).cloned());
                let v: Value = match (class.as_str(), over) {
                    ("checkbox", Some(o)) => Value::Boolean(matches!(o.as_str(), "true" | "1" | "yes" | "on")),
                    ("intedit", Some(o)) => Value::Integer(o.parse::<f64>().map(|f| f as i64).unwrap_or(0)),
                    ("floatedit", Some(o)) => Value::Number(o.parse().unwrap_or(0.0)),
                    (_, Some(o)) => Value::String(lua.create_string(&o)?),
                    ("dropdown", None) => {
                        if matches!(default, Value::Nil) {
                            let items: Option<Table> = c.get("items")?;
                            items.and_then(|t| t.get::<Value>(1).ok()).unwrap_or(Value::Nil)
                        } else { default }
                    }
                    ("checkbox", None) => if matches!(default, Value::Nil) { Value::Boolean(false) } else { default },
                    (_, None) => default,
                };
                if class != "label" {
                    result.set(name, v)?;
                }
            }
            match buttons {
                Some(b) => {
                    let labels: Vec<String> = b.sequence_values::<String>().filter_map(|x| x.ok()).collect();
                    let chosen = supplied.as_ref().and_then(|a| a.button.clone()).or_else(|| btn.borrow().clone()).or_else(|| labels.first().cloned());
                    match chosen {
                        Some(c) if c.eq_ignore_ascii_case("cancel") => Ok(MultiValue::from_vec(vec![Value::Boolean(false), Value::Table(result)])),
                        Some(c) => Ok(MultiValue::from_vec(vec![Value::String(lua.create_string(&c)?), Value::Table(result)])),
                        None => Ok(MultiValue::from_vec(vec![Value::Boolean(true), Value::Table(result)])),
                    }
                }
                None => Ok(MultiValue::from_vec(vec![Value::Boolean(true), Value::Table(result)])),
            }
        })?)?;
        dialog.set("open", lua.create_function(|_, _a: MultiValue| Ok(Value::Nil))?)?;
        dialog.set("save", lua.create_function(|_, _a: MultiValue| Ok(Value::Nil))?)?;
        aegisub.set("dialog", dialog)?;

        g.set("aegisub", aegisub)?;
        lua.load(PRELUDE_LUA).set_name("=prelude").exec()?;
        // Common 5.1 compat shims already provided by LuaJIT (unpack, loadstring, …).
        Ok(())
    }

    fn refresh_package_path(&self) -> LResult<()> {
        let package: Table = self.lua.globals().get("package")?;
        let mut path: String = package.get("path")?;
        let mut add = |d: &Path| {
            path = format!("{}/?.lua;{}", d.display(), path);
        };
        if let Some(d) = self.script_path.borrow().as_ref().and_then(|p| p.parent()) {
            add(d);
        }
        for d in self.include_dirs.borrow().iter() {
            add(d);
        }
        package.set("path", path)
    }

    /// Load an automation script; returns the macros/filters it registered.
    pub fn load_script(&self, path: &Path) -> Result<Vec<Registered>, Error> {
        *self.script_path.borrow_mut() = Some(path.to_path_buf());
        self.refresh_package_path()?;
        let src = std::fs::read(path).map_err(|e| Error::Other(format!("{}: {}", path.display(), e)))?;
        let src = strip_bom(&src);
        self.load_source(src, &path.display().to_string())
    }

    pub fn load_source(&self, src: &[u8], chunk_name: &str) -> Result<Vec<Registered>, Error> {
        let g = self.lua.globals();
        let reg: Table = g.get("__kaku_registry")?;
        let before = reg.raw_len();
        self.lua.load(src).set_name(format!("@{}", chunk_name)).exec()?;
        let mut out = Vec::new();
        for i in (before + 1)..=reg.raw_len() {
            let e: Table = reg.get(i)?;
            out.push(Registered {
                kind: if e.get::<String>("kind")? == "filter" { Kind::Filter } else { Kind::Macro },
                name: e.get("name")?,
                description: e.get("description")?,
                index: i,
            });
        }
        Ok(out)
    }

    pub fn script_meta(&self) -> HashMap<String, String> {
        let g = self.lua.globals();
        ["script_name", "script_description", "script_author", "script_version", "script_namespace"]
            .iter()
            .filter_map(|k| g.get::<Option<String>>(*k).ok().flatten().map(|v| (k.to_string(), v)))
            .collect()
    }

    /// Run macro/filter `reg_index` (from `Registered::index`) against `script`.
    /// `selection` and `active` are 1-based indices into the flat entry list, like Aegisub.
    pub fn run(&self, reg_index: usize, script: &Shared, selection: &[usize], active: usize) -> Result<RunOutput, Error> {
        let reg: Table = self.lua.globals().get("__kaku_registry")?;
        let e: Table = reg.get(reg_index).map_err(|_| Error::Other("no such macro".into()))?;
        let f: Function = e.get("process")?;
        let subs = self.lua.create_userdata(subs::Subs(script.clone()))?;
        let sel = self.lua.create_sequence_from(selection.iter().map(|&i| i as i64))?;
        let kind: String = e.get("kind")?;
        self.dialog_counter.set(0);
        let res = if kind == "filter" {
            let cfg = self.lua.create_table()?;
            f.call::<MultiValue>((subs, cfg))
        } else {
            f.call::<MultiValue>((subs, sel, active as i64))
        };
        let mut out = RunOutput::default();
        self.dialog_counter.set(0);
        match res {
            Ok(rv) => {
                let mut it = rv.into_iter();
                if let Some(Value::Table(t)) = it.next() {
                    out.selection = Some(t.sequence_values::<i64>().filter_map(|x| x.ok()).map(|x| x as usize).collect());
                }
                if let Some(v) = it.next() {
                    match v {
                        Value::Integer(i) => out.active = Some(i as usize),
                        Value::Number(f) => out.active = Some(f as usize),
                        _ => {}
                    }
                }
            }
            Err(err) => {
                if err.to_string().contains(NEED_DIALOG_MARK) {
                    out.pending_dialog = self.pending.borrow_mut().take();
                } else if err.to_string().contains(CANCEL_MARK) {
                    out.cancelled = true;
                } else {
                    out.log = self.take_log();
                    out.log.push(format!("error: {}", err));
                    return Err(Error::Lua(out.log.join("\n")));
                }
            }
        }
        out.log = self.take_log();
        Ok(out)
    }

    /// Evaluate a macro's validate function (true if no validator).
    pub fn validate(&self, reg_index: usize, script: &Shared, selection: &[usize], active: usize) -> Result<bool, Error> {
        let reg: Table = self.lua.globals().get("__kaku_registry")?;
        let e: Table = reg.get(reg_index).map_err(|_| Error::Other("no such macro".into()))?;
        let Some(v): Option<Function> = e.get("validate")? else { return Ok(true) };
        let subs = self.lua.create_userdata(subs::Subs(script.clone()))?;
        let sel = self.lua.create_sequence_from(selection.iter().map(|&i| i as i64))?;
        let r: Value = v.call((subs, sel, active as i64))?;
        Ok(!matches!(r, Value::Nil | Value::Boolean(false)))
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)
}

fn lua_tostring(lua: &Lua, v: &Value) -> LResult<String> {
    let f: Function = lua.globals().get("tostring")?;
    f.call(v.clone())
}

/// Convenience: run a macro by name on an owned script and return the modified script.
pub fn run_by_name(rt: &Runtime, regs: &[Registered], name: &str, script: Script, selection: Vec<usize>) -> Result<(Script, RunOutput), Error> {
    let r = regs.iter().find(|r| r.name == name).ok_or_else(|| Error::Other(format!("macro '{}' not found", name)))?;
    let shared: Shared = Rc::new(RefCell::new(script));
    let active = selection.first().copied().unwrap_or(0);
    let out = rt.run(r.index, &shared, &selection, active)?;
    let s = shared.borrow().clone();
    Ok((s, out))
}

fn read_spec(controls: &Table, buttons: Option<&Table>) -> LResult<DialogSpec> {
    let mut spec = DialogSpec::default();
    for c in controls.clone().sequence_values::<Table>() {
        let c = c?;
        let val = |k: &str| -> LResult<String> {
            Ok(match c.get::<Value>(k)? {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(i) => i.to_string(),
                Value::Number(f) => kaku_core::ass::fmt_num(f),
                Value::Boolean(b) => b.to_string(),
                _ => String::new(),
            })
        };
        let num = |k: &str| c.get::<Option<f64>>(k).ok().flatten();
        let mut ctl = Control {
            class: val("class")?,
            name: val("name")?,
            label: val("label")?,
            value: val("value")?,
            x: num("x").unwrap_or(0.0) as i64,
            y: num("y").unwrap_or(0.0) as i64,
            width: num("width").unwrap_or(1.0) as i64,
            height: num("height").unwrap_or(1.0) as i64,
            min: num("min"),
            max: num("max"),
            hint: val("hint")?,
            ..Default::default()
        };
        if let Some(items) = c.get::<Option<Table>>("items")? {
            ctl.items = items.sequence_values::<String>().filter_map(|x| x.ok()).collect();
        }
        if ctl.class == "checkbox" && ctl.value.is_empty() {
            ctl.value = "false".into();
        }
        spec.controls.push(ctl);
    }
    spec.buttons = match buttons {
        Some(b) => b.clone().sequence_values::<String>().filter_map(|x| x.ok()).collect(),
        None => vec!["OK".into(), "Cancel".into()],
    };
    Ok(spec)
}
