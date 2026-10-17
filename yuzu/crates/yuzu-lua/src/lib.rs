//! yuzu-lua: Aegisub-compatible Lua automation (macros, export filters, karaskel, `re`, `unicode`, …).

mod doc;
mod metrics;
mod re;

pub use metrics::add_font_dir;

use asscore::{time::Fps, Script};
use doc::{Subs, SubsRc, SubsUd};
use mlua::{Function, Lua, MultiValue, RegistryKey, Result as LResult, Table, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ------------------------------------------------------------------ host interface

#[derive(Clone, Debug, PartialEq)]
pub enum DValue {
    None,
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Clone, Debug, Default)]
pub struct DialogControl {
    pub class: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub label: String,
    pub text: String,
    pub hint: String,
    pub value: Option<DValue>,
    pub items: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct DialogSpec {
    pub controls: Vec<DialogControl>,
    pub buttons: Vec<String>,
    /// True when the script supplied no custom buttons (OK/Cancel, boolean result).
    pub default_buttons: bool,
    pub ok: Option<String>,
    pub cancel: Option<String>,
}

pub enum DialogResult {
    Cancelled,
    Ok { button: String, values: HashMap<String, DValue> },
}

#[derive(Clone, Debug, Default)]
pub struct ProjectInfo {
    pub script_path: Option<PathBuf>,
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub fps: Option<(u32, u32)>,
    pub video_size: Option<(i32, i32)>,
    pub keyframes: Vec<i64>,
    pub video_position: i64,
    pub active_row: i64,
    pub audio_selection: Option<(i64, i64)>,
    pub data_dir: Option<PathBuf>,
    pub user_dir: Option<PathBuf>,
}

pub trait Host {
    /// level: 0 fatal, 1 error, 2 warning, 3 hint, 4+ debug/trace
    fn log(&self, level: i32, msg: &str);
    fn dialog(&self, spec: &DialogSpec) -> DialogResult;
    fn progress(&self, _kind: &str, _value: f64, _text: &str) {}
    fn is_cancelled(&self) -> bool {
        false
    }
    fn project(&self) -> ProjectInfo {
        ProjectInfo::default()
    }
    fn file_dialog(&self, _save: bool, _title: &str, _default: &str) -> Option<String> {
        None
    }
    fn clipboard_get(&self) -> Option<String> {
        None
    }
    fn clipboard_set(&self, _s: &str) {}
}

// ------------------------------------------------------------------ script instance

struct MacroEntry {
    name: String,
    description: String,
    func: RegistryKey,
    validate: Option<RegistryKey>,
    is_active: Option<RegistryKey>,
}

struct FilterEntry {
    name: String,
    description: String,
    priority: i64,
    func: RegistryKey,
    config: Option<RegistryKey>,
}

#[derive(Clone, Debug)]
pub struct MacroInfo {
    pub index: usize,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Default)]
pub struct Outcome {
    /// New selection as 0-based event indices, if the macro returned one.
    pub selection: Option<Vec<usize>>,
    pub active: Option<usize>,
    pub cancelled: bool,
}

pub struct LuaScript {
    lua: Lua,
    pub path: PathBuf,
    pub script_name: String,
    pub script_description: String,
    pub script_author: String,
    pub script_version: String,
    macros: Rc<RefCell<Vec<MacroEntry>>>,
    filters: Rc<RefCell<Vec<FilterEntry>>>,
    subs: SubsRc,
    #[allow(dead_code)]
    host: Rc<dyn Host>,
}

const CANCEL: &str = "__yuzu_cancel__";

fn lerr(e: mlua::Error) -> String {
    let s = e.to_string();
    s
}

fn is_cancel(e: &mlua::Error) -> bool {
    e.to_string().contains(CANCEL)
}

fn to_i(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn dvalue_to_lua(lua: &Lua, v: &DValue) -> LResult<Value> {
    Ok(match v {
        DValue::None => Value::Nil,
        DValue::Str(s) => Value::String(lua.create_string(s)?),
        DValue::Num(n) => Value::Number(*n),
        DValue::Bool(b) => Value::Boolean(*b),
    })
}

fn lua_to_dvalue(v: &Value) -> Option<DValue> {
    match v {
        Value::Nil => None,
        Value::Boolean(b) => Some(DValue::Bool(*b)),
        Value::Integer(i) => Some(DValue::Num(*i as f64)),
        Value::Number(n) => Some(DValue::Num(*n)),
        Value::String(s) => Some(DValue::Str(s.to_str().map(|s| s.to_string()).unwrap_or_default())),
        _ => None,
    }
}

fn s_of(t: &Table, k: &str) -> String {
    match t.get::<Value>(k) {
        Ok(Value::String(s)) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
        Ok(Value::Number(n)) => asscore::fmt_num(n),
        Ok(Value::Integer(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn i_of(t: &Table, k: &str) -> i32 {
    t.get::<Value>(k).ok().and_then(|v| to_i(&v)).unwrap_or(0) as i32
}
fn f_of(t: &Table, k: &str) -> Option<f64> {
    match t.get::<Value>(k) {
        Ok(Value::Number(n)) => Some(n),
        Ok(Value::Integer(n)) => Some(n as f64),
        _ => None,
    }
}

fn parse_dialog(dlg: &Table, buttons: Option<Table>, ids: Option<Table>) -> LResult<DialogSpec> {
    let mut spec = DialogSpec::default();
    for pair in dlg.clone().sequence_values::<Table>() {
        let c = pair?;
        let mut ctl = DialogControl {
            class: s_of(&c, "class"),
            name: s_of(&c, "name"),
            x: i_of(&c, "x"),
            y: i_of(&c, "y"),
            width: i_of(&c, "width").max(1),
            height: i_of(&c, "height").max(1),
            label: s_of(&c, "label"),
            text: s_of(&c, "text"),
            hint: s_of(&c, "hint"),
            min: f_of(&c, "min"),
            max: f_of(&c, "max"),
            step: f_of(&c, "step"),
            ..Default::default()
        };
        ctl.value = c.get::<Value>("value").ok().and_then(|v| lua_to_dvalue(&v));
        if let Ok(Value::Table(it)) = c.get::<Value>("items") {
            for s in it.sequence_values::<Value>().flatten() {
                if let Value::String(s) = s {
                    ctl.items.push(s.to_str()?.to_string());
                }
            }
        }
        spec.controls.push(ctl);
    }
    match buttons {
        Some(b) => {
            for s in b.sequence_values::<String>().flatten() {
                spec.buttons.push(s);
            }
        }
        None => {
            spec.default_buttons = true;
            spec.buttons = vec!["OK".into(), "Cancel".into()];
        }
    }
    if let Some(ids) = ids {
        let g = |k| match ids.get::<Value>(k) {
            Ok(Value::String(s)) => s.to_str().ok().map(|s| s.to_string()),
            _ => None,
        };
        spec.ok = g("ok");
        spec.cancel = g("cancel");
    } else if spec.default_buttons {
        spec.ok = Some("OK".into());
        spec.cancel = Some("Cancel".into());
    }
    Ok(spec)
}

pub fn default_dialog_values(spec: &DialogSpec) -> HashMap<String, DValue> {
    let mut m = HashMap::new();
    for c in &spec.controls {
        if c.name.is_empty() {
            continue;
        }
        let v = match c.class.as_str() {
            "label" | "image" | "hyperlink" => continue,
            "checkbox" => DValue::Bool(matches!(c.value, Some(DValue::Bool(true)))),
            "intedit" | "floatedit" | "slider" => DValue::Num(match &c.value { Some(DValue::Num(n)) => *n, _ => c.min.unwrap_or(0.0) }),
            "dropdown" => match &c.value { Some(DValue::Str(s)) => DValue::Str(s.clone()), _ => DValue::Str(c.items.first().cloned().unwrap_or_default()) },
            _ => match &c.value { Some(DValue::Str(s)) => DValue::Str(s.clone()), _ => DValue::Str(c.text.clone()) },
        };
        m.insert(c.name.clone(), v);
    }
    m
}

impl LuaScript {
    /// Load and execute a script file; collects registered macros/filters.
    pub fn load(path: &Path, host: Rc<dyn Host>, include_dirs: &[PathBuf]) -> Result<LuaScript, String> {
        let src = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let lua = unsafe { Lua::unsafe_new() };
        let macros: Rc<RefCell<Vec<MacroEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let filters: Rc<RefCell<Vec<FilterEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let subs: SubsRc = Rc::new(RefCell::new(Subs::default()));
        let sc = LuaScript {
            lua,
            path: path.to_path_buf(),
            script_name: String::new(),
            script_description: String::new(),
            script_author: String::new(),
            script_version: String::new(),
            macros,
            filters,
            subs,
            host: host.clone(),
        };
        sc.setup(&host, include_dirs).map_err(lerr)?;
        let chunk = sc.lua.load(&src[..]).set_name(format!("@{}", path.display()));
        chunk.exec().map_err(lerr)?;
        let g = sc.lua.globals();
        let gs = |k: &str| match g.get::<Value>(k) {
            Ok(Value::String(s)) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            Ok(Value::Number(n)) => asscore::fmt_num(n),
            _ => String::new(),
        };
        let mut sc = sc;
        sc.script_name = gs("script_name");
        sc.script_description = gs("script_description");
        sc.script_author = gs("script_author");
        sc.script_version = gs("script_version");
        if sc.script_name.is_empty() {
            sc.script_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        }
        Ok(sc)
    }

    fn setup(&self, host: &Rc<dyn Host>, include_dirs: &[PathBuf]) -> LResult<()> {
        let lua = &self.lua;
        // package.path
        let sdir = self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut dirs: Vec<PathBuf> = vec![sdir.clone(), sdir.join("include"), sdir.join("..").join("include")];
        dirs.extend(include_dirs.iter().cloned());
        let mut pp = String::new();
        for d in &dirs {
            let d = d.to_string_lossy();
            pp.push_str(&format!("{d}/?.lua;{d}/?/init.lua;"));
        }
        let package: Table = lua.globals().get("package")?;
        let old: String = package.get("path")?;
        package.set("path", format!("{pp}{old}"))?;

        // libraries
        let preload: Table = package.get("preload")?;
        let libs: [(&str, &str); 9] = [
            ("aegisub.util", include_str!("../lua/util.lua")),
            ("utils", include_str!("../lua/utils.lua")),
            ("unicode", include_str!("../lua/unicode.lua")),
            ("karaskel", include_str!("../lua/karaskel.lua")),
            ("cleantags", include_str!("../lua/cleantags.lua")),
            ("clipboard", include_str!("../lua/clipboard.lua")),
            ("lfs", include_str!("../lua/lfs.lua")),
            ("l0.DependencyControl", include_str!("../lua/depctrl.lua")),
            ("DependencyControl", include_str!("../lua/depctrl.lua")),
        ];
        for (name, src) in libs {
            let f = lua.load(src).set_name(format!("={name}")).into_function()?;
            preload.set(name, f)?;
        }
        re::install(lua)?;

        // primitives
        let y = lua.create_table()?;
        {
            let h = host.clone();
            y.set("log", lua.create_function(move |_, (lvl, msg): (i64, String)| { h.log(lvl as i32, &msg); Ok(()) })?)?;
        }
        {
            let h = host.clone();
            y.set("dialog", lua.create_function(move |lua, (dlg, buttons, ids): (Table, Option<Table>, Option<Table>)| {
                let spec = parse_dialog(&dlg, buttons, ids)?;
                match h.dialog(&spec) {
                    DialogResult::Cancelled => {
                        if spec.default_buttons {
                            Ok(MultiValue::from_iter([Value::Boolean(false)]))
                        } else if let Some(c) = &spec.cancel {
                            Ok(MultiValue::from_iter([Value::String(lua.create_string(c)?)]))
                        } else {
                            Err(mlua::Error::runtime(CANCEL))
                        }
                    }
                    DialogResult::Ok { button, values } => {
                        let t = lua.create_table()?;
                        for (k, v) in &values {
                            t.set(k.as_str(), dvalue_to_lua(lua, v)?)?;
                        }
                        let b = if spec.default_buttons {
                            Value::Boolean(spec.ok.as_deref() == Some(button.as_str()))
                        } else {
                            Value::String(lua.create_string(&button)?)
                        };
                        Ok(MultiValue::from_iter([b, Value::Table(t)]))
                    }
                }
            })?)?;
        }
        {
            let h = host.clone();
            y.set("progress", lua.create_function(move |_, (kind, v): (String, Value)| {
                match v {
                    Value::String(s) => h.progress(&kind, 0.0, &s.to_str()?),
                    other => h.progress(&kind, to_i(&other).map(|i| i as f64).or(if let Value::Number(n) = other { Some(n) } else { None }).unwrap_or(0.0), ""),
                }
                Ok(())
            })?)?;
        }
        {
            let h = host.clone();
            y.set("is_cancelled", lua.create_function(move |_, ()| Ok(h.is_cancelled()))?)?;
        }
        y.set("text_extents", lua.create_function(|_, (st, text): (Table, String)| {
            let fam = s_of(&st, "fontname");
            let req = metrics::FontReq {
                family: &fam,
                size: f_of(&st, "fontsize").unwrap_or(40.0),
                bold: matches!(st.get::<Value>("bold"), Ok(Value::Boolean(true))) || f_of(&st, "bold").map(|v| v != 0.0).unwrap_or(false),
                italic: matches!(st.get::<Value>("italic"), Ok(Value::Boolean(true))) || f_of(&st, "italic").map(|v| v != 0.0).unwrap_or(false),
                scale_x: f_of(&st, "scale_x").unwrap_or(100.0),
                scale_y: f_of(&st, "scale_y").unwrap_or(100.0),
                spacing: f_of(&st, "spacing").unwrap_or(0.0),
            };
            let (w, hh, d, e) = metrics::text_extents(&req, &text);
            Ok((w, hh, d, e))
        })?)?;
        {
            let h = host.clone();
            y.set("frame_from_ms", lua.create_function(move |_, ms: f64| {
                Ok(h.project().fps.map(|(n, d)| Fps { num: n, den: d }.frame_from_ms(ms as i64)))
            })?)?;
            let h = host.clone();
            y.set("ms_from_frame", lua.create_function(move |_, f: f64| {
                Ok(h.project().fps.map(|(n, d)| Fps { num: n, den: d }.ms_from_frame(f as i64)))
            })?)?;
            let h = host.clone();
            y.set("video_size", lua.create_function(move |_, ()| {
                Ok(match h.project().video_size {
                    Some((w, hh)) => MultiValue::from_iter([Value::Integer(w as i64), Value::Integer(hh as i64), Value::Number(w as f64 / hh as f64), Value::Integer(0)]),
                    None => MultiValue::new(),
                })
            })?)?;
            let h = host.clone();
            y.set("keyframes", lua.create_function(move |lua, ()| {
                let p = h.project();
                let t = lua.create_table()?;
                if p.fps.is_some() {
                    let f = p.fps.map(|(n, d)| Fps { num: n, den: d }).unwrap();
                    for (i, k) in p.keyframes.iter().enumerate() {
                        t.set(i + 1, f.frame_from_ms(*k))?;
                    }
                }
                Ok(t)
            })?)?;
            let h = host.clone();
            y.set("project_properties", lua.create_function(move |lua, ()| {
                let p = h.project();
                let t = lua.create_table()?;
                let ps = |o: &Option<PathBuf>| o.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                t.set("video_file", ps(&p.video_path))?;
                t.set("audio_file", ps(&p.audio_path))?;
                t.set("video_position", p.video_position)?;
                t.set("active_row", p.active_row)?;
                t.set("ar_mode", 0)?;
                t.set("ar_value", 0)?;
                t.set("video_zoom", 1.0)?;
                t.set("scroll_position", 0)?;
                t.set("keyframes_file", "")?;
                t.set("timecodes_file", "")?;
                t.set("export_encoding", "UTF-8")?;
                t.set("project_path", ps(&p.script_path))?;
                Ok(t)
            })?)?;
            let h = host.clone();
            y.set("decode_path", lua.create_function(move |_, path: String| {
                let p = h.project();
                let dir = |o: &Option<PathBuf>| o.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().to_string());
                let mut out = path.clone();
                let rep = |out: &mut String, key: &str, val: Option<String>| {
                    if out.starts_with(key) {
                        if let Some(v) = val {
                            *out = format!("{}{}", v, &out[key.len()..]);
                        }
                    }
                };
                rep(&mut out, "?script", dir(&p.script_path));
                rep(&mut out, "?video", dir(&p.video_path));
                rep(&mut out, "?audio", dir(&p.audio_path));
                rep(&mut out, "?data", p.data_dir.as_ref().map(|d| d.to_string_lossy().to_string()));
                rep(&mut out, "?user", p.user_dir.as_ref().map(|d| d.to_string_lossy().to_string()));
                rep(&mut out, "?local", p.user_dir.as_ref().map(|d| d.to_string_lossy().to_string()));
                rep(&mut out, "?temp", Some(std::env::temp_dir().to_string_lossy().to_string()));
                rep(&mut out, "?dictionary", None);
                Ok(out)
            })?)?;
            let h = host.clone();
            y.set("audio_selection", lua.create_function(move |_, ()| {
                Ok(match h.project().audio_selection {
                    Some((a, b)) => MultiValue::from_iter([Value::Integer(a), Value::Integer(b)]),
                    None => MultiValue::new(),
                })
            })?)?;
            let h = host.clone();
            y.set("file_dialog", lua.create_function(move |_, (kind, title, def): (String, Option<String>, Option<String>)| {
                Ok(h.file_dialog(kind == "save", &title.unwrap_or_default(), &def.unwrap_or_default()))
            })?)?;
            let h = host.clone();
            y.set("clip_get", lua.create_function(move |_, ()| Ok(h.clipboard_get()))?)?;
            let h = host.clone();
            y.set("clip_set", lua.create_function(move |_, s: String| { h.clipboard_set(&s); Ok(true) })?)?;
        }
        y.set("case", lua.create_function(|_, (s, up): (String, bool)| {
            Ok(if up { turkish_aware_upper(&s) } else { turkish_aware_lower(&s) })
        })?)?;
        y.set("fs", lua.create_function(|lua, (op, a): (String, Option<String>)| {
            fs_op(lua, &op, a.as_deref())
        })?)?;

        // registration
        {
            let m = self.macros.clone();
            y.set("register_macro", lua.create_function(move |lua, (name, desc, f, validate, active): (String, Option<String>, Function, Option<Function>, Option<Function>)| {
                m.borrow_mut().push(MacroEntry {
                    name,
                    description: desc.unwrap_or_default(),
                    func: lua.create_registry_value(f)?,
                    validate: match validate { Some(v) => Some(lua.create_registry_value(v)?), None => None },
                    is_active: match active { Some(v) => Some(lua.create_registry_value(v)?), None => None },
                });
                Ok(())
            })?)?;
            let fl = self.filters.clone();
            y.set("register_filter", lua.create_function(move |lua, (name, desc, prio, f, cfg): (String, Option<String>, Option<i64>, Function, Option<Function>)| {
                fl.borrow_mut().push(FilterEntry {
                    name,
                    description: desc.unwrap_or_default(),
                    priority: prio.unwrap_or(0),
                    func: lua.create_registry_value(f)?,
                    config: match cfg { Some(v) => Some(lua.create_registry_value(v)?), None => None },
                });
                Ok(())
            })?)?;
        }
        lua.globals().set("__y", y)?;
        let prelude: Function = lua.load(include_str!("../lua/prelude.lua")).set_name("=prelude").into_function()?;
        prelude.call::<()>(())?;
        lua.globals().set("__y", Value::Nil)?;
        Ok(())
    }

    pub fn macros(&self) -> Vec<MacroInfo> {
        self.macros.borrow().iter().enumerate().map(|(i, m)| MacroInfo { index: i, name: m.name.clone(), description: m.description.clone() }).collect()
    }

    pub fn filters(&self) -> Vec<(usize, String, String, i64)> {
        self.filters.borrow().iter().enumerate().map(|(i, f)| (i, f.name.clone(), f.description.clone(), f.priority)).collect()
    }

    fn install_subs(&self, doc: &Script) -> LResult<mlua::AnyUserData> {
        *self.subs.borrow_mut() = Subs::from_script(doc);
        self.lua.create_userdata(SubsUd(self.subs.clone()))
    }

    fn selection_table(&self, sel: &[usize]) -> LResult<Table> {
        let t = self.lua.create_table()?;
        let s = self.subs.borrow();
        let mut n = 0;
        for &e in sel {
            if let Some(i) = s.lua_index_of_event(e) {
                n += 1;
                t.set(n, i as i64 + 1)?;
            }
        }
        Ok(t)
    }

    /// Whether the macro can be run for the given state.
    pub fn validate(&self, macro_idx: usize, doc: &Script, sel: &[usize], active: Option<usize>) -> bool {
        let m = self.macros.borrow();
        let Some(me) = m.get(macro_idx) else { return false };
        let Some(vk) = &me.validate else { return true };
        let run = || -> LResult<bool> {
            let ud = self.install_subs(doc)?;
            let f: Function = self.lua.registry_value(vk)?;
            let st = self.selection_table(sel)?;
            let act = active.and_then(|a| self.subs.borrow().lua_index_of_event(a)).map(|i| i as i64 + 1).unwrap_or(-1);
            let r: Value = f.call((ud, st, act))?;
            Ok(!matches!(r, Value::Nil | Value::Boolean(false)))
        };
        run().unwrap_or(false)
    }

    /// Run a macro against `doc`, replacing its contents on success. On error/cancel `doc` is untouched.
    pub fn run_macro(&self, macro_idx: usize, doc: &mut Script, sel: &[usize], active: Option<usize>) -> Result<Outcome, String> {
        let (fk_name, f): (String, Function) = {
            let m = self.macros.borrow();
            let me = m.get(macro_idx).ok_or("no such macro")?;
            (me.name.clone(), self.lua.registry_value(&me.func).map_err(lerr)?)
        };
        let _ = fk_name;
        let ud = self.install_subs(doc).map_err(lerr)?;
        let st = self.selection_table(sel).map_err(lerr)?;
        let act = active.and_then(|a| self.subs.borrow().lua_index_of_event(a)).map(|i| i as i64 + 1).unwrap_or(-1);
        let ret = f.call::<MultiValue>((ud, st, act));
        match ret {
            Err(e) if is_cancel(&e) => Ok(Outcome { cancelled: true, ..Default::default() }),
            Err(e) => Err(lerr(e)),
            Ok(vals) => {
                let vals: Vec<Value> = vals.into_iter().collect();
                let taken = std::mem::take(&mut *self.subs.borrow_mut());
                let (new_sel, new_active) = {
                    let map = |li: i64| -> Option<usize> {
                        if li < 1 { return None; }
                        taken.event_index_of(li as usize - 1)
                    };
                    let sel_out = match vals.first() {
                        Some(Value::Table(t)) => {
                            let mut v = Vec::new();
                            for x in t.clone().sequence_values::<Value>().flatten() {
                                if let Some(i) = to_i(&x) { if let Some(e) = map(i) { v.push(e); } }
                            }
                            Some(v)
                        }
                        _ => None,
                    };
                    let act_out = vals.get(1).and_then(to_i).and_then(map).or_else(|| match vals.first() {
                        Some(x) => to_i(x).and_then(map),
                        None => None,
                    });
                    (sel_out, act_out)
                };
                *doc = taken.into_script();
                Ok(Outcome { selection: new_sel, active: new_active, cancelled: false })
            }
        }
    }

    pub fn run_filter(&self, idx: usize, doc: &mut Script, config: Option<HashMap<String, DValue>>) -> Result<(), String> {
        let f: Function = {
            let m = self.filters.borrow();
            let fe = m.get(idx).ok_or("no such filter")?;
            self.lua.registry_value(&fe.func).map_err(lerr)?
        };
        let ud = self.install_subs(doc).map_err(lerr)?;
        let cfg = self.lua.create_table().map_err(lerr)?;
        if let Some(c) = config {
            for (k, v) in c { cfg.set(k, dvalue_to_lua(&self.lua, &v).map_err(lerr)?).map_err(lerr)?; }
        }
        match f.call::<()>((ud, cfg)) {
            Err(e) if is_cancel(&e) => Ok(()),
            Err(e) => Err(lerr(e)),
            Ok(()) => {
                let taken = std::mem::take(&mut *self.subs.borrow_mut());
                *doc = taken.into_script();
                Ok(())
            }
        }
    }

    /// Evaluate a Lua expression/statement in this script's state (for the console / tests).
    pub fn eval(&self, code: &str) -> Result<String, String> {
        let v: Value = self.lua.load(code).eval().map_err(lerr)?;
        Ok(match v {
            Value::Nil => "nil".into(),
            Value::String(s) => s.to_str().map_err(lerr)?.to_string(),
            Value::Number(n) => asscore::fmt_num(n),
            Value::Integer(i) => i.to_string(),
            Value::Boolean(b) => b.to_string(),
            other => format!("{:?}", other),
        })
    }
}

fn turkish_aware_upper(s: &str) -> String {
    s.chars().map(|c| match c { 'i' if false => 'İ', _ => c }).collect::<String>().to_uppercase()
}
fn turkish_aware_lower(s: &str) -> String {
    s.to_lowercase()
}

fn fs_op(lua: &Lua, op: &str, a: Option<&str>) -> LResult<Value> {
    match op {
        "cwd" => Ok(Value::String(lua.create_string(std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())?)),
        "stat" => {
            let Some(p) = a else { return Ok(Value::Nil) };
            match std::fs::metadata(p) {
                Ok(m) => {
                    let t = lua.create_table()?;
                    t.set("mode", if m.is_dir() { "directory" } else { "file" })?;
                    t.set("size", m.len())?;
                    if let Ok(mt) = m.modified() {
                        if let Ok(d) = mt.duration_since(std::time::UNIX_EPOCH) { t.set("modification", d.as_secs())?; }
                    }
                    Ok(Value::Table(t))
                }
                Err(_) => Ok(Value::Nil),
            }
        }
        "mkdir" => Ok(Value::Boolean(a.map(|p| std::fs::create_dir_all(p).is_ok()).unwrap_or(false))),
        "rmdir" => Ok(Value::Boolean(a.map(|p| std::fs::remove_dir(p).is_ok()).unwrap_or(false))),
        "dir" => {
            let t = lua.create_table()?;
            let mut n = 0;
            if let Some(p) = a {
                n += 1; t.set(n, ".")?;
                n += 1; t.set(n, "..")?;
                if let Ok(rd) = std::fs::read_dir(p) {
                    for e in rd.flatten() { n += 1; t.set(n, e.file_name().to_string_lossy().to_string())?; }
                }
            }
            Ok(Value::Table(t))
        }
        _ => Ok(Value::Nil),
    }
}

#[cfg(test)]
mod tests;
