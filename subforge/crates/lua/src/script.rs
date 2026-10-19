//! Loading and running Automation 4 Lua scripts.
use crate::convert::table_to_style;
use crate::doc::Doc;
use crate::host::{Dialog, DialogField, DialogValue, Host};
use crate::libs;
use crate::metrics;
use crate::subs::make_subs;
use mlua::{Function, Lua, MultiValue, RegistryKey, Result as LuaResult, Table, Value, Variadic};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use subforge_core::time::Fps;

const CANCEL_MARK: &str = "\u{1}SUBFORGE_CANCEL\u{1}";

const AEGISUB_UTIL_LUA: &str = include_str!("../lua/aegisub_util.lua");
const UTILS_AUTO4_LUA: &str = include_str!("../lua/vendor/utils-auto4.lua");
const KARASKEL_AUTO4_LUA: &str = include_str!("../lua/vendor/karaskel-auto4.lua");
const CLEANTAGS_LUA: &str = include_str!("../lua/vendor/cleantags.lua");
const MOON_SEARCHER_LUA: &str = include_str!("../lua/moon_searcher.lua");
const AEGISUB_RE_LUA: &str = include_str!("../lua/aegisub_re.lua");
const PRELUDE_LUA: &str = include_str!("../lua/prelude.lua");

#[derive(Debug)]
pub enum ScriptError {
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::Cancelled => write!(f, "cancelled"),
            ScriptError::Failed(s) => write!(f, "{s}"),
        }
    }
}

impl From<mlua::Error> for ScriptError {
    fn from(e: mlua::Error) -> Self {
        let s = e.to_string();
        if s.contains(CANCEL_MARK) {
            ScriptError::Cancelled
        } else {
            ScriptError::Failed(s)
        }
    }
}

struct MacroEntry {
    name: String,
    description: String,
    process: RegistryKey,
    validate: Option<RegistryKey>,
}

struct FilterEntry {
    name: String,
    description: String,
    priority: i64,
    process: RegistryKey,
    config: Option<RegistryKey>,
}

#[derive(Default)]
struct Registry {
    macros: Vec<MacroEntry>,
    filters: Vec<FilterEntry>,
}

#[derive(Debug, Clone)]
pub struct MacroInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct FilterInfo {
    pub name: String,
    pub description: String,
    pub priority: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// New selection (1-based flat indices) if the script returned one.
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
}

pub type SharedHost = Rc<RefCell<dyn Host>>;

pub struct LuaScript {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub macros: Vec<MacroInfo>,
    pub filters: Vec<FilterInfo>,
    /// Set when the script failed to load (syntax error, missing include...).
    pub load_error: Option<String>,
    lua: Lua,
    registry: Rc<RefCell<Registry>>,
}

fn wrap<T>(r: LuaResult<T>) -> LuaResult<T> {
    r
}

fn parse_dialog(t: &Table, buttons: Option<Table>, ids: Option<Table>) -> LuaResult<Dialog> {
    let mut d = Dialog::default();
    for pair in t.clone().sequence_values::<Table>() {
        let f = pair?;
        let s = |k: &str| -> LuaResult<String> { Ok(f.get::<Option<String>>(k)?.unwrap_or_default()) };
        let n = |k: &str| -> LuaResult<Option<f64>> { f.get::<Option<f64>>(k) };
        let class = s("class")?.to_ascii_lowercase();
        let mut field = DialogField {
            class: class.clone(),
            name: s("name")?,
            label: s("label")?,
            x: n("x")?.unwrap_or(0.0) as i32,
            y: n("y")?.unwrap_or(0.0) as i32,
            width: n("width")?.unwrap_or(1.0) as i32,
            height: n("height")?.unwrap_or(1.0) as i32,
            hint: s("hint")?,
            min: n("min")?,
            max: n("max")?,
            step: n("step")?,
            ..Default::default()
        };
        if let Some(items) = f.get::<Option<Table>>("items")? {
            field.items = items.sequence_values::<String>().filter_map(|x| x.ok()).collect();
        }
        let val = match class.as_str() {
            "edit" | "textbox" | "dropdown" | "color" | "coloralpha" | "alpha" => {
                let v: Value = f.get("value")?;
                let txt: Option<String> = f.get("text")?;
                match (v, txt) {
                    (Value::String(s), _) => Some(DialogValue::Str(s.to_str()?.to_string())),
                    (Value::Integer(i), _) => Some(DialogValue::Str(i.to_string())),
                    (Value::Number(n), _) => Some(DialogValue::Str(n.to_string())),
                    (_, Some(t)) => Some(DialogValue::Str(t)),
                    _ => None,
                }
            }
            "intedit" | "floatedit" | "slider" => n("value")?.map(DialogValue::Num),
            "checkbox" => f.get::<Option<bool>>("value")?.map(DialogValue::Bool),
            _ => None,
        };
        field.value = val;
        d.fields.push(field);
    }
    if let Some(b) = buttons {
        d.buttons = b.sequence_values::<String>().filter_map(|x| x.ok()).collect();
    }
    if let Some(ids) = ids {
        for kv in ids.pairs::<String, String>() {
            let (k, v) = kv?;
            d.button_ids.insert(k, v);
        }
    }
    Ok(d)
}

fn install_aegisub(lua: &Lua, host: &SharedHost, registry: &Rc<RefCell<Registry>>, script_dir: PathBuf) -> LuaResult<()> {
    let aeg = lua.create_table()?;

    // ---- logging / progress
    {
        let host = host.clone();
        aeg.set("__log", lua.create_function(move |_, (lvl, msg): (i32, String)| {
            host.borrow_mut().log(lvl, &msg);
            Ok(())
        })?)?;
    }
    let progress = lua.create_table()?;
    {
        let h = host.clone();
        progress.set("set", lua.create_function(move |_, p: f64| { h.borrow_mut().progress_set(p); Ok(()) })?)?;
        let h = host.clone();
        progress.set("task", lua.create_function(move |_, s: Variadic<Value>| {
            let msg = s.into_iter().next().map(|v| v.to_string().unwrap_or_default()).unwrap_or_default();
            h.borrow_mut().progress_task(&msg);
            Ok(())
        })?)?;
        let h = host.clone();
        progress.set("title", lua.create_function(move |_, s: String| { h.borrow_mut().progress_title(&s); Ok(()) })?)?;
        let h = host.clone();
        progress.set("is_cancelled", lua.create_function(move |_, ()| Ok(h.borrow().is_cancelled()))?)?;
    }
    aeg.set("progress", progress)?;

    aeg.set("cancel", lua.create_function(|_, ()| -> LuaResult<()> { Err(mlua::Error::runtime(CANCEL_MARK)) })?)?;

    // ---- registration
    {
        let reg = registry.clone();
        aeg.set(
            "register_macro",
            lua.create_function(move |lua, (name, desc, process, validate, _active): (String, Option<String>, Function, Option<Function>, Option<Function>)| {
                let validate = validate.map(|f| lua.create_registry_value(f)).transpose()?;
                reg.borrow_mut().macros.push(MacroEntry {
                    name,
                    description: desc.unwrap_or_default(),
                    process: lua.create_registry_value(process)?,
                    validate,
                });
                Ok(())
            })?,
        )?;
        let reg = registry.clone();
        aeg.set(
            "register_filter",
            lua.create_function(move |lua, (name, desc, priority, process, config): (String, Option<String>, Option<i64>, Function, Option<Function>)| {
                let config = config.map(|f| lua.create_registry_value(f)).transpose()?;
                reg.borrow_mut().filters.push(FilterEntry {
                    name,
                    description: desc.unwrap_or_default(),
                    priority: priority.unwrap_or(0),
                    process: lua.create_registry_value(process)?,
                    config,
                });
                Ok(())
            })?,
        )?;
    }

    // ---- dialogs
    {
        let h = host.clone();
        aeg.set(
            "__dialog",
            lua.create_function(move |lua, (dlg, buttons, ids): (Table, Option<Table>, Option<Table>)| {
                let d = parse_dialog(&dlg, buttons, ids)?;
                let res = h.borrow_mut().dialog(&d);
                let out = lua.create_table()?;
                match res {
                    None => Ok(MultiValue::from_iter([Value::Boolean(false), Value::Table(out)])),
                    Some(r) => {
                        for (k, v) in r.values {
                            match v {
                                DialogValue::Str(s) => out.set(k, s)?,
                                DialogValue::Num(n) => out.set(k, n)?,
                                DialogValue::Bool(b) => out.set(k, b)?,
                            }
                        }
                        let first = match (d.buttons.is_empty(), r.button) {
                            (true, _) => Value::Boolean(true),
                            (false, Some(b)) => Value::String(lua.create_string(b)?),
                            (false, None) => Value::String(lua.create_string(d.buttons[0].as_str())?),
                        };
                        Ok(MultiValue::from_iter([first, Value::Table(out)]))
                    }
                }
            })?,
        )?;
        let h = host.clone();
        aeg.set("__open_dialog", lua.create_function(move |lua, (t, d, f, w, multiple): (String, String, String, String, bool)| {
            match h.borrow_mut().open_dialog(&t, &d, &f, &w, multiple) {
                None => Ok(Value::Nil),
                Some(v) if multiple => Ok(Value::Table(lua.create_sequence_from(v)?)),
                Some(v) => Ok(v.into_iter().next().map(|s| lua.create_string(s).map(Value::String)).transpose()?.unwrap_or(Value::Nil)),
            }
        })?)?;
        let h = host.clone();
        aeg.set("__save_dialog", lua.create_function(move |_, (t, d, f, w): (String, String, String, String)| {
            Ok(h.borrow_mut().save_dialog(&t, &d, &f, &w))
        })?)?;
    }

    // ---- video / project info
    {
        let h = host.clone();
        aeg.set("__frame_from_ms", lua.create_function(move |_, ms: f64| {
            Ok(h.borrow().fps().map(|f| Fps::from_f64(f).frame_at(ms.round() as i64)))
        })?)?;
        let h = host.clone();
        aeg.set("__ms_from_frame", lua.create_function(move |_, fr: f64| {
            Ok(h.borrow().fps().map(|f| Fps::from_f64(f).start_ms(fr.round() as i64)))
        })?)?;
        let h = host.clone();
        aeg.set("video_size", lua.create_function(move |_, ()| {
            Ok(match h.borrow().video_size() {
                Some((x, y)) => MultiValue::from_iter([Value::Integer(x as _), Value::Integer(y as _), Value::Number(x as f64 / y as f64), Value::Integer(0)]),
                None => MultiValue::new(),
            })
        })?)?;
        let h = host.clone();
        aeg.set("keyframes", lua.create_function(move |lua, ()| {
            let hb = h.borrow();
            match (hb.keyframes(), hb.fps()) {
                (Some(k), Some(f)) => {
                    let fps = Fps::from_f64(f);
                    let frames: Vec<i64> = k.into_iter().map(|ms| fps.frame_at(ms)).collect();
                    Ok(Value::Table(lua.create_sequence_from(frames)?))
                }
                _ => Ok(Value::Nil),
            }
        })?)?;
        let h = host.clone();
        aeg.set("__clip_get", lua.create_function(move |_, ()| Ok(h.borrow().clipboard_get()))?)?;
        let h = host.clone();
        aeg.set("__clip_set", lua.create_function(move |_, s: mlua::LuaString| Ok(h.borrow_mut().clipboard_set(&String::from_utf8_lossy(&s.as_bytes()))))?)?;
        let h = host.clone();
        aeg.set("get_audio_selection", lua.create_function(move |_, ()| {
            Ok(match h.borrow().audio_selection() {
                Some((a, b)) => MultiValue::from_iter([Value::Integer(a as _), Value::Integer(b as _)]),
                None => MultiValue::new(),
            })
        })?)?;
        let h = host.clone();
        aeg.set("project_properties", lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            let hb = h.borrow();
            if let Some(p) = hb.video_path() { t.set("video_file", p.to_string_lossy().to_string())?; }
            if let Some(p) = hb.audio_path() { t.set("audio_file", p.to_string_lossy().to_string())?; }
            t.set("ar_mode", 0)?;
            t.set("ar_value", 0)?;
            t.set("video_zoom", 1)?;
            t.set("scroll_position", 0)?;
            t.set("active_row", 0)?;
            t.set("video_position", 0)?;
            Ok(t)
        })?)?;
        let h = host.clone();
        aeg.set("file_name", lua.create_function(move |_, ()| {
            Ok(h.borrow().script_path().and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string())))
        })?)?;
        let h = host.clone();
        let sdir = script_dir.clone();
        aeg.set("decode_path", lua.create_function(move |_, p: String| {
            let hb = h.borrow();
            let dir_of = |p: Option<PathBuf>| p.and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()));
            let mut out = p.clone();
            let script_dir = dir_of(hb.script_path()).unwrap_or_else(|| sdir.to_string_lossy().to_string());
            let table: [(&str, String); 6] = [
                ("?script", script_dir),
                ("?video", dir_of(hb.video_path()).unwrap_or_default()),
                ("?audio", dir_of(hb.audio_path()).unwrap_or_default()),
                ("?temp", std::env::temp_dir().to_string_lossy().to_string()),
                ("?user", std::env::var("HOME").map(|h| format!("{h}/.subforge")).unwrap_or_default()),
                ("?data", std::env::var("SUBFORGE_DATA").unwrap_or_default()),
            ];
            for (k, v) in table {
                if out.starts_with(k) { out = format!("{v}{}", &out[k.len()..]); }
            }
            Ok(out)
        })?)?;
    }

    // ---- text metrics
    aeg.set(
        "text_extents",
        lua.create_function(|_, (style, text): (Table, String)| {
            let st = table_to_style(&style)?;
            Ok(metrics::text_extents(&st, &text))
        })?,
    )?;

    lua.globals().set("aegisub", aeg)?;
    lua.load(PRELUDE_LUA).set_name("=prelude").exec()?;
    Ok(())
}

/// Translate MoonScript source to Lua with the embedded compiler.
fn compile_moon(lua: &Lua, src: &str) -> LuaResult<String> {
    let require: Function = lua.globals().get("require")?;
    let base: Table = require.call("moonscript.base")?;
    let to_lua: Function = base.get("to_lua")?;
    // returns (code, line_table) on success, (nil, message) on failure
    let (code, extra): (Option<String>, Value) = to_lua.call(src)?;
    code.ok_or_else(|| mlua::Error::runtime(format!("MoonScript: {}", extra.to_string().unwrap_or_else(|_| "compile failed".into()))))
}

/// Register the embedded MoonScript runtime (lazily loaded `require` targets) and a `.moon` searcher.
fn install_moonscript(lua: &Lua) -> LuaResult<()> {
    let package: Table = lua.globals().get("package")?;
    let preload: Table = package.get("preload")?;
    for &(name, src) in crate::moon_modules::MODULES {
        preload.set(
            name,
            lua.create_function(move |lua, ()| lua.load(src).set_name(format!("=moon:{name}")).call::<Value>(name))?,
        )?;
    }
    preload.set("lpeg", lua.create_function(|lua, ()| lua.globals().get::<Function>("require")?.call::<Value>("lulpeg"))?)?;
    preload.set("moonscript", lua.create_function(|lua, ()| lua.globals().get::<Function>("require")?.call::<Value>("moonscript.base"))?)?;
    lua.load(MOON_SEARCHER_LUA).set_name("=moon_searcher").exec()
}

fn find_include(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() && p.exists() {
        return Some(p.to_path_buf());
    }
    dirs.iter().map(|d| d.join(name)).find(|c| c.exists())
}

impl LuaScript {
    /// Load a script into a fresh Lua state. Never panics: failures land in `load_error`.
    pub fn load(path: &Path, host: SharedHost, include_dirs: &[PathBuf]) -> LuaScript {
        let registry = Rc::new(RefCell::new(Registry::default()));
        // SAFETY: scripts are user-installed and trusted (same trust level as Aegisub);
        // unsafe mode is required for `ffi` and C module loading.
        let lua = unsafe { Lua::unsafe_new() };
        let mut script = LuaScript {
            path: path.to_path_buf(),
            name: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            description: String::new(),
            author: String::new(),
            version: String::new(),
            macros: vec![],
            filters: vec![],
            load_error: None,
            lua,
            registry,
        };
        if let Err(e) = script.init(path, host, include_dirs) {
            script.load_error = Some(e.to_string());
        }
        script
    }

    fn init(&mut self, path: &Path, host: SharedHost, include_dirs: &[PathBuf]) -> LuaResult<()> {
        let lua = &self.lua;
        let script_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut dirs = vec![script_dir.clone()];
        dirs.extend(include_dirs.iter().cloned());
        if let Ok(extra) = std::env::var("SUBFORGE_INCLUDE") {
            dirs.extend(std::env::split_paths(&extra));
        }

        // package.path
        let package: Table = lua.globals().get("package")?;
        let mut pp = package.get::<String>("path").unwrap_or_default();
        for d in &dirs {
            pp = format!("{}/?.lua;{}/?/init.lua;{}", d.display(), d.display(), pp);
        }
        package.set("path", pp)?;

        for d in &dirs {
            crate::fonts::add_font_dir(d.join("fonts"));
        }
        install_aegisub(lua, &host, &self.registry, script_dir)?;

        // native modules
        let unicode = libs::install_unicode(lua)?;
        lua.globals().set("unicode", unicode.clone())?;
        libs::preload(lua, "unicode", unicode.clone())?;
        libs::preload(lua, "aegisub.unicode", unicode)?;
        let lfs = libs::install_lfs(lua)?;
        libs::preload(lua, "lfs", lfs.clone())?;
        libs::preload(lua, "aegisub.lfs", lfs)?;
        let re_impl = libs::install_re_impl(lua)?;
        let re: Table = lua.load(AEGISUB_RE_LUA).set_name("=aegisub.re").call(re_impl)?;
        lua.globals().set("re", re.clone())?;
        libs::preload(lua, "re", re.clone())?;
        libs::preload(lua, "aegisub.re", re)?;

        install_moonscript(lua)?;

        // include(): script-dir/include-dir files first, then bundled libs
        let dirs_c = dirs.clone();
        let include = lua.create_function(move |lua, name: String| -> LuaResult<MultiValue> {
            let base = name.trim_start_matches("./");
            let loaded_key = format!("__included:{base}");
            let globals = lua.globals();
            if globals.get::<Option<bool>>(loaded_key.as_str())?.unwrap_or(false) {
                return Ok(MultiValue::new());
            }
            let src: Option<(String, String)> = if let Some(p) = find_include(base, &dirs_c) {
                Some((std::fs::read_to_string(&p).map_err(mlua::Error::external)?, p.to_string_lossy().to_string()))
            } else {
                match base {
                    "utils.lua" | "utils-auto4.lua" => Some((UTILS_AUTO4_LUA.to_string(), "utils-auto4.lua".into())),
                    "karaskel.lua" | "karaskel-auto4.lua" => Some((KARASKEL_AUTO4_LUA.to_string(), "karaskel-auto4.lua".into())),
                    "cleantags.lua" => Some((CLEANTAGS_LUA.to_string(), "cleantags.lua".into())),
                    "unicode.lua" | "re.lua" => return Ok(MultiValue::new()), // built in
                    _ => None,
                }
            };
            let Some((code, chunk)) = src else {
                return Err(mlua::Error::runtime(format!("Lua include not found: {name}")));
            };
            let code = if chunk.ends_with(".moon") { compile_moon(lua, &code)? } else { code };
            globals.set(loaded_key.as_str(), true)?;
            lua.load(&code).set_name(format!("@{chunk}")).call::<MultiValue>(())
        })?;
        lua.globals().set("include", include)?;

        // require("aegisub.util"), require("karaskel"), require("utils"), require("aegisub.argcheck")
        let preload: Table = package.get("preload")?;
        preload.set(
            "aegisub.util",
            lua.create_function(|lua, ()| lua.load(AEGISUB_UTIL_LUA).set_name("=aegisub.util").call::<Value>(()))?,
        )?;
        preload.set(
            "aegisub.argcheck",
            lua.create_function(|lua, ()| {
                lua.load("return function(...) return function(f) return f end end").set_name("=aegisub.argcheck").call::<Value>(())
            })?,
        )?;
        preload.set(
            "aegisub.ffi",
            lua.create_function(|lua, ()| {
                lua.load("local ffi = require('ffi'); return { string = function(c) if c == nil then return nil end return ffi.string(c) end }")
                    .set_name("=aegisub.ffi")
                    .call::<Value>(())
            })?,
        )?;
        preload.set(
            "aegisub.clipboard",
            lua.create_function(|lua, ()| lua.globals().get::<Table>("aegisub")?.get::<Value>("clipboard"))?,
        )?;
        for (modname, file, global) in [("utils", "utils.lua", None), ("karaskel", "karaskel.lua", Some("karaskel")), ("cleantags", "cleantags.lua", None)] {
            preload.set(
                modname,
                lua.create_function(move |lua, ()| -> LuaResult<Value> {
                    let inc: Function = lua.globals().get("include")?;
                    inc.call::<()>(file)?;
                    match global {
                        Some(g) => lua.globals().get(g),
                        None => Ok(Value::Boolean(true)),
                    }
                })?,
            )?;
        }

        let code = std::fs::read_to_string(path).map_err(mlua::Error::external)?;
        // Aegisub scripts are UTF-8; strip BOM.
        let code = code.strip_prefix('\u{feff}').unwrap_or(&code);
        let code = if path.extension().is_some_and(|e| e == "moon") { compile_moon(lua, code)? } else { code.to_string() };
        lua.load(&code).set_name(format!("@{}", path.display())).exec()?;

        let g = lua.globals();
        let gs = |k: &str| g.get::<Option<String>>(k).ok().flatten();
        if let Some(n) = gs("script_name") { self.name = n; }
        self.description = gs("script_description").unwrap_or_default();
        self.author = gs("script_author").unwrap_or_default();
        self.version = gs("script_version").unwrap_or_default();

        // Legacy-style scripts: top-level process_lines(subs, config)
        let reg = self.registry.borrow();
        self.macros = reg.macros.iter().map(|m| MacroInfo { name: m.name.clone(), description: m.description.clone() }).collect();
        self.filters = reg.filters.iter().map(|f| FilterInfo { name: f.name.clone(), description: f.description.clone(), priority: f.priority }).collect();
        Ok(())
    }

    fn parse_outcome(ret: MultiValue) -> Outcome {
        let mut out = Outcome::default();
        let mut it = ret.into_iter();
        if let Some(Value::Table(t)) = it.next() {
            let sel: Vec<usize> = t.sequence_values::<f64>().filter_map(|v| v.ok()).filter(|v| *v >= 1.0).map(|v| v as usize).collect();
            out.selection = Some(sel);
        }
        if let Some(v) = it.next() {
            match v {
                Value::Integer(i) if i >= 1 => out.active = Some(i as usize),
                Value::Number(n) if n >= 1.0 => out.active = Some(n as usize),
                _ => {}
            }
        }
        out
    }

    fn sel_table(&self, sel: &[usize]) -> LuaResult<Table> {
        self.lua.create_sequence_from(sel.iter().map(|&i| i as i64))
    }

    /// Run `validate` of a macro (true when absent).
    pub fn validate_macro(&self, idx: usize, doc: Rc<RefCell<Doc>>, sel: &[usize], active: usize) -> bool {
        let key = {
            let reg = self.registry.borrow();
            match reg.macros.get(idx).and_then(|m| m.validate.as_ref()) {
                Some(k) => self.lua.registry_value::<Function>(k),
                None => return true,
            }
        };
        let Ok(f) = key else { return true };
        let Ok(subs) = make_subs(&self.lua, doc) else { return false };
        let Ok(sel_t) = self.sel_table(sel) else { return false };
        match f.call::<MultiValue>((subs, sel_t, active)) {
            Ok(v) => !matches!(v.into_iter().next(), Some(Value::Boolean(false)) | Some(Value::Nil) | None),
            Err(_) => false,
        }
    }

    pub fn run_macro(&self, idx: usize, doc: Rc<RefCell<Doc>>, sel: &[usize], active: usize) -> Result<Outcome, ScriptError> {
        let f: Function = {
            let reg = self.registry.borrow();
            let m = reg.macros.get(idx).ok_or_else(|| ScriptError::Failed(format!("no macro #{idx}")))?;
            self.lua.registry_value(&m.process)?
        };
        let subs = make_subs(&self.lua, doc)?;
        let ret: MultiValue = f.call((subs, self.sel_table(sel)?, active))?;
        Ok(Self::parse_outcome(ret))
    }

    /// Run an export filter with the given dialog results as config.
    pub fn run_filter(&self, idx: usize, doc: Rc<RefCell<Doc>>, config: HashMap<String, DialogValue>) -> Result<(), ScriptError> {
        let f: Function = {
            let reg = self.registry.borrow();
            let m = reg.filters.get(idx).ok_or_else(|| ScriptError::Failed(format!("no filter #{idx}")))?;
            self.lua.registry_value(&m.process)?
        };
        let cfg = self.lua.create_table()?;
        for (k, v) in config {
            match v {
                DialogValue::Str(s) => cfg.set(k, s)?,
                DialogValue::Num(n) => cfg.set(k, n)?,
                DialogValue::Bool(b) => cfg.set(k, b)?,
            }
        }
        let subs = make_subs(&self.lua, doc)?;
        f.call::<()>((subs, cfg))?;
        Ok(())
    }

    /// Dialog a filter wants shown before it runs (via its `config` function).
    pub fn filter_dialog(&self, idx: usize, doc: Rc<RefCell<Doc>>) -> Result<Option<Dialog>, ScriptError> {
        let f: Function = {
            let reg = self.registry.borrow();
            match reg.filters.get(idx).and_then(|m| m.config.as_ref()) {
                Some(k) => self.lua.registry_value(k)?,
                None => return Ok(None),
            }
        };
        let subs = make_subs(&self.lua, doc)?;
        let cfg = self.lua.create_table()?;
        let ret: Value = f.call((subs, cfg))?;
        match ret {
            Value::Table(t) => Ok(Some(parse_dialog(&t, None, None)?)),
            _ => Ok(None),
        }
    }

    /// Evaluate arbitrary Lua in the script's environment (used by tests / REPL).
    pub fn eval<T: mlua::FromLuaMulti>(&self, code: &str) -> LuaResult<T> {
        wrap(self.lua.load(code).eval())
    }
}
