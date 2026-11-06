//! Aegisub uyumlu Lua (Automation 4) çalışma zamanı.
//!
//! `subs` nesnesi gerçek Aegisub'daki gibi bir userdata'dır: `#subs`, `subs[i]`,
//! `subs[i] = line`, `subs.insert/append/delete/deleterange` desteklenir.
use kitsune_core::ass::{Document, Event, EventKind, Style};
use mlua::{Function, Lua, MultiValue, RegistryKey, Table, UserData, UserDataMethods, Value, Variadic};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const UTIL_LUA: &str = include_str!("../lua/util.lua");
const UTILS_LUA: &str = include_str!("../lua/utils.lua");
const KARASKEL_LUA: &str = include_str!("../lua/karaskel.lua");
const CANCEL_MARK: &str = "__kitsune_cancel__";

#[derive(Debug, Clone)]
pub enum Line {
    Info(String, String),
    Style(Style),
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroKind {
    Macro,
    Filter,
}

#[derive(Debug, Clone)]
pub struct MacroInfo {
    pub name: String,
    pub description: String,
    pub script: String,
    pub kind: MacroKind,
}

#[derive(Debug, Default)]
pub struct RunOutput {
    pub selection: Vec<usize>,
    pub active: Option<usize>,
    pub logs: Vec<String>,
    pub cancelled: bool,
}

#[derive(Default, Clone)]
struct Ctx {
    fps: Option<f64>,
    video_size: Option<(i32, i32)>,
    script_dir: PathBuf,
    include_dirs: Vec<PathBuf>,
    current_script: String,
    project: Vec<(String, String)>,
}

struct MacroEntry {
    info: MacroInfo,
    process: RegistryKey,
    validate: Option<RegistryKey>,
}

pub struct Engine {
    lua: Lua,
    macros: Rc<RefCell<Vec<MacroEntry>>>,
    logs: Rc<RefCell<Vec<String>>>,
    ctx: Rc<RefCell<Ctx>>,
}

fn lerr(e: mlua::Error) -> String {
    let s = e.to_string();
    s.replace(CANCEL_MARK, "iptal")
}

// ---------- Satır <-> Lua tablosu ----------

fn line_to_table(lua: &Lua, l: &Line) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("extra", lua.create_table()?)?;
    match l {
        Line::Info(k, v) => {
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", k.as_str())?;
            t.set("value", v.as_str())?;
            t.set("raw", if k == ";" { format!(";{}", v) } else { format!("{}: {}", k, v) })?;
        }
        Line::Style(s) => {
            t.set("class", "style")?;
            t.set("section", "V4+ Styles")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", s.primary.as_str())?;
            t.set("color2", s.secondary.as_str())?;
            t.set("color3", s.outline_color.as_str())?;
            t.set("color4", s.back.as_str())?;
            t.set("bold", s.bold)?;
            t.set("italic", s.italic)?;
            t.set("underline", s.underline)?;
            t.set("strikeout", s.strikeout)?;
            t.set("scale_x", s.scale_x)?;
            t.set("scale_y", s.scale_y)?;
            t.set("spacing", s.spacing)?;
            t.set("angle", s.angle)?;
            t.set("borderstyle", s.border_style)?;
            t.set("outline", s.outline)?;
            t.set("shadow", s.shadow)?;
            t.set("align", s.alignment)?;
            t.set("margin_l", s.margin_l)?;
            t.set("margin_r", s.margin_r)?;
            t.set("margin_t", s.margin_v)?;
            t.set("margin_b", s.margin_v)?;
            t.set("encoding", s.encoding)?;
        }
        Line::Event(e) => {
            t.set("class", "dialogue")?;
            t.set("section", "Events")?;
            t.set("comment", e.is_comment())?;
            t.set("layer", e.layer)?;
            t.set("start_time", e.start)?;
            t.set("end_time", e.end)?;
            t.set("style", e.style.as_str())?;
            t.set("actor", e.actor.as_str())?;
            t.set("margin_l", e.margin_l)?;
            t.set("margin_r", e.margin_r)?;
            t.set("margin_t", e.margin_v)?;
            t.set("margin_b", e.margin_v)?;
            t.set("effect", e.effect.as_str())?;
            t.set("text", e.text.as_str())?;
            t.set("raw", e.to_line())?;
        }
    }
    Ok(t)
}

fn get_str(t: &Table, k: &str, d: &str) -> mlua::Result<String> {
    match t.get::<Value>(k)? {
        Value::Nil => Ok(d.to_string()),
        Value::String(s) => Ok(s.to_string_lossy().to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(mlua::Error::runtime(format!("alan '{}' metin olmalı", k))),
    }
}
fn get_f(t: &Table, k: &str, d: f64) -> mlua::Result<f64> {
    match t.get::<Value>(k)? {
        Value::Nil => Ok(d),
        Value::Integer(i) => Ok(i as f64),
        Value::Number(n) => Ok(n),
        Value::String(s) => Ok(s.to_string_lossy().parse().unwrap_or(d)),
        _ => Err(mlua::Error::runtime(format!("alan '{}' sayı olmalı", k))),
    }
}
fn get_b(t: &Table, k: &str, d: bool) -> mlua::Result<bool> {
    match t.get::<Value>(k)? {
        Value::Nil => Ok(d),
        Value::Boolean(b) => Ok(b),
        Value::Integer(i) => Ok(i != 0),
        Value::Number(n) => Ok(n != 0.0),
        _ => Ok(d),
    }
}

fn table_to_line(t: &Table) -> mlua::Result<Line> {
    let class = get_str(t, "class", "")?;
    match class.as_str() {
        "info" => Ok(Line::Info(get_str(t, "key", "")?, get_str(t, "value", "")?)),
        "style" => {
            let d = Style::default();
            let mv = if t.contains_key("margin_t")? { get_f(t, "margin_t", 10.0)? } else { get_f(t, "margin_v", 10.0)? };
            Ok(Line::Style(Style {
                name: get_str(t, "name", "Default")?,
                fontname: get_str(t, "fontname", &d.fontname)?,
                fontsize: get_f(t, "fontsize", d.fontsize)?,
                primary: get_str(t, "color1", &d.primary)?,
                secondary: get_str(t, "color2", &d.secondary)?,
                outline_color: get_str(t, "color3", &d.outline_color)?,
                back: get_str(t, "color4", &d.back)?,
                bold: get_b(t, "bold", false)?,
                italic: get_b(t, "italic", false)?,
                underline: get_b(t, "underline", false)?,
                strikeout: get_b(t, "strikeout", false)?,
                scale_x: get_f(t, "scale_x", 100.0)?,
                scale_y: get_f(t, "scale_y", 100.0)?,
                spacing: get_f(t, "spacing", 0.0)?,
                angle: get_f(t, "angle", 0.0)?,
                border_style: get_f(t, "borderstyle", 1.0)? as i32,
                outline: get_f(t, "outline", 2.0)?,
                shadow: get_f(t, "shadow", 2.0)?,
                alignment: get_f(t, "align", 2.0)? as i32,
                margin_l: get_f(t, "margin_l", 10.0)? as i32,
                margin_r: get_f(t, "margin_r", 10.0)? as i32,
                margin_v: mv as i32,
                encoding: get_f(t, "encoding", 1.0)? as i32,
            }))
        }
        "dialogue" => {
            let mv = if t.contains_key("margin_t")? { get_f(t, "margin_t", 0.0)? } else { get_f(t, "margin_v", 0.0)? };
            Ok(Line::Event(Event {
                kind: if get_b(t, "comment", false)? { EventKind::Comment } else { EventKind::Dialogue },
                layer: get_f(t, "layer", 0.0)? as i32,
                start: get_f(t, "start_time", 0.0)?.round() as i64,
                end: get_f(t, "end_time", 0.0)?.round() as i64,
                style: get_str(t, "style", "Default")?,
                actor: if t.contains_key("actor")? { get_str(t, "actor", "")? } else { get_str(t, "name", "")? },
                margin_l: get_f(t, "margin_l", 0.0)? as i32,
                margin_r: get_f(t, "margin_r", 0.0)? as i32,
                margin_v: mv as i32,
                effect: get_str(t, "effect", "")?,
                text: get_str(t, "text", "")?,
            }))
        }
        other => Err(mlua::Error::runtime(format!("bilinmeyen satır sınıfı: '{}'", other))),
    }
}

pub fn doc_to_lines(doc: &Document) -> Vec<Line> {
    let mut v = vec![];
    for (k, val) in &doc.info {
        v.push(Line::Info(k.clone(), val.clone()));
    }
    for s in &doc.styles {
        v.push(Line::Style(s.clone()));
    }
    for e in &doc.events {
        v.push(Line::Event(e.clone()));
    }
    v
}

pub fn lines_into_doc(lines: Vec<Line>, doc: &mut Document) {
    doc.info.clear();
    doc.styles.clear();
    doc.events.clear();
    for l in lines {
        match l {
            Line::Info(k, v) => doc.info.push((k, v)),
            Line::Style(s) => doc.styles.push(s),
            Line::Event(e) => doc.events.push(e),
        }
    }
}

// ---------- subs userdata ----------

#[derive(Clone)]
struct Subs(Rc<RefCell<Vec<Line>>>);

fn idx_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i as i64),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn collect_lines(args: &[Value]) -> mlua::Result<Vec<Line>> {
    let mut out = vec![];
    for a in args {
        match a {
            Value::Table(t) => out.push(table_to_line(t)?),
            _ => return Err(mlua::Error::runtime("satır tablosu bekleniyordu")),
        }
    }
    Ok(out)
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(mlua::MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().len()));
        m.add_meta_method(mlua::MetaMethod::Index, |lua, this, key: Value| {
            if let Some(i) = idx_of(&key) {
                let lines = this.0.borrow();
                if i < 1 || i as usize > lines.len() {
                    return Err(mlua::Error::runtime(format!("satır indeksi sınır dışı: {}", i)));
                }
                return Ok(Value::Table(line_to_table(lua, &lines[i as usize - 1])?));
            }
            let Value::String(k) = &key else { return Ok(Value::Nil) };
            let st = this.0.clone();
            match &*k.to_str()? {
                "n" => Ok(Value::Integer(st.borrow().len() as _)),
                "insert" => Ok(Value::Function(lua.create_function(move |_, args: Variadic<Value>| {
                    let i = args.first().and_then(idx_of).ok_or_else(|| mlua::Error::runtime("insert: indeks gerekli"))?;
                    let new = collect_lines(&args[1..])?;
                    let mut l = st.borrow_mut();
                    let at = (i.max(1) as usize - 1).min(l.len());
                    for (o, n) in new.into_iter().enumerate() {
                        l.insert(at + o, n);
                    }
                    Ok(())
                })?)),
                "append" => Ok(Value::Function(lua.create_function(move |_, args: Variadic<Value>| {
                    let new = collect_lines(&args)?;
                    st.borrow_mut().extend(new);
                    Ok(())
                })?)),
                "delete" => Ok(Value::Function(lua.create_function(move |_, args: Variadic<Value>| {
                    let mut idx: Vec<usize> = vec![];
                    for a in args.iter() {
                        match a {
                            Value::Table(t) => {
                                for v in t.sequence_values::<i64>() {
                                    idx.push(v? as usize);
                                }
                            }
                            other => idx.push(idx_of(other).ok_or_else(|| mlua::Error::runtime("delete: indeks bekleniyordu"))? as usize),
                        }
                    }
                    idx.sort_unstable();
                    idx.dedup();
                    let mut l = st.borrow_mut();
                    for i in idx.into_iter().rev() {
                        if i >= 1 && i <= l.len() {
                            l.remove(i - 1);
                        }
                    }
                    Ok(())
                })?)),
                "deleterange" => Ok(Value::Function(lua.create_function(move |_, (a, b): (i64, i64)| {
                    let mut l = st.borrow_mut();
                    let a = a.max(1) as usize;
                    let b = (b.max(0) as usize).min(l.len());
                    if a <= b {
                        l.drain(a - 1..b);
                    }
                    Ok(())
                })?)),
                _ => Ok(Value::Nil),
            }
        });
        m.add_meta_method(mlua::MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let i = idx_of(&key).ok_or_else(|| mlua::Error::runtime("subs yalnızca sayısal indekse atanabilir"))?;
            let mut l = this.0.borrow_mut();
            match val {
                Value::Nil => {
                    if i >= 1 && (i as usize) <= l.len() {
                        l.remove(i as usize - 1);
                    }
                }
                Value::Table(t) => {
                    let line = table_to_line(&t)?;
                    if i == 0 || i as usize == l.len() + 1 {
                        l.push(line);
                    } else if i >= 1 && (i as usize) <= l.len() {
                        l[i as usize - 1] = line;
                    } else {
                        return Err(mlua::Error::runtime(format!("satır indeksi sınır dışı: {}", i)));
                    }
                }
                _ => return Err(mlua::Error::runtime("satır tablosu bekleniyordu")),
            }
            Ok(())
        });
    }
}

// ---------- Engine ----------

impl Engine {
    pub fn new() -> Result<Engine, String> {
        let e = Engine {
            lua: Lua::new(),
            macros: Rc::new(RefCell::new(vec![])),
            logs: Rc::new(RefCell::new(vec![])),
            ctx: Rc::new(RefCell::new(Ctx::default())),
        };
        e.setup().map_err(lerr)?;
        Ok(e)
    }

    pub fn set_video(&self, fps: Option<f64>, size: Option<(i32, i32)>) {
        let mut c = self.ctx.borrow_mut();
        c.fps = fps;
        c.video_size = size;
    }
    pub fn add_include_dir(&self, p: impl Into<PathBuf>) {
        self.ctx.borrow_mut().include_dirs.push(p.into());
    }
    pub fn set_project_property(&self, k: &str, v: &str) {
        let mut c = self.ctx.borrow_mut();
        c.project.retain(|(a, _)| a != k);
        c.project.push((k.into(), v.into()));
    }
    pub fn macros(&self) -> Vec<MacroInfo> {
        self.macros.borrow().iter().map(|m| m.info.clone()).collect()
    }
    pub fn take_logs(&self) -> Vec<String> {
        std::mem::take(&mut *self.logs.borrow_mut())
    }

    fn setup(&self) -> mlua::Result<()> {
        let lua = &self.lua;
        let g = lua.globals();
        let aeg = lua.create_table()?;

        // register_macro / register_filter
        let macros = self.macros.clone();
        let ctx = self.ctx.clone();
        aeg.set("register_macro", lua.create_function(move |lua, (name, desc, process, validate): (String, Option<String>, Function, Option<Function>)| {
            let script = ctx.borrow().current_script.clone();
            macros.borrow_mut().push(MacroEntry {
                info: MacroInfo { name, description: desc.unwrap_or_default(), script, kind: MacroKind::Macro },
                process: lua.create_registry_value(process)?,
                validate: match validate { Some(f) => Some(lua.create_registry_value(f)?), None => None },
            });
            Ok(())
        })?)?;
        let macros = self.macros.clone();
        let ctx = self.ctx.clone();
        aeg.set("register_filter", lua.create_function(move |lua, (name, desc, _prio, process, _cfg): (String, Option<String>, Option<i64>, Function, Value)| {
            let script = ctx.borrow().current_script.clone();
            macros.borrow_mut().push(MacroEntry {
                info: MacroInfo { name, description: desc.unwrap_or_default(), script, kind: MacroKind::Filter },
                process: lua.create_registry_value(process)?,
                validate: None,
            });
            Ok(())
        })?)?;
        aeg.set("cancel", lua.create_function(|_, ()| -> mlua::Result<()> { Err(mlua::Error::runtime(CANCEL_MARK)) })?)?;

        // progress
        let progress = lua.create_table()?;
        let logs = self.logs.clone();
        progress.set("set", lua.create_function(|_, _: Value| Ok(()))?)?;
        progress.set("task", lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
        progress.set("title", lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
        progress.set("is_cancelled", lua.create_function(|_, ()| Ok(false))?)?;
        aeg.set("progress", progress)?;

        // debug
        let dbg = lua.create_table()?;
        let out_fn = {
            let logs = logs.clone();
            lua.create_function(move |lua, args: MultiValue| {
                let mut v: Vec<Value> = args.into_iter().collect();
                if matches!(v.first(), Some(Value::Integer(_)) | Some(Value::Number(_))) && v.len() > 1 {
                    v.remove(0);
                }
                let fmt: Function = lua.globals().get::<Table>("string")?.get("format")?;
                let s = if v.len() > 1 {
                    fmt.call::<String>(MultiValue::from_iter(v))?
                } else {
                    match v.first() {
                        Some(Value::String(s)) => s.to_string_lossy().to_string(),
                        Some(Value::Nil) | None => String::new(),
                        Some(x) => lua.globals().get::<Function>("tostring")?.call::<String>(x.clone())?,
                    }
                };
                logs.borrow_mut().push(s);
                Ok(())
            })?
        };
        dbg.set("out", out_fn.clone())?;
        aeg.set("log", out_fn.clone())?;
        g.set("print", lua.create_function({
            let logs = logs.clone();
            move |lua, args: Variadic<Value>| {
                let ts: Function = lua.globals().get("tostring")?;
                let mut parts = vec![];
                for a in args.iter() { parts.push(ts.call::<String>(a.clone())?); }
                logs.borrow_mut().push(parts.join("\t"));
                Ok(())
            }
        })?)?;
        aeg.set("debug", dbg)?;

        // zaman / kare
        let ctx = self.ctx.clone();
        aeg.set("frame_from_ms", lua.create_function(move |_, ms: f64| Ok(ctx.borrow().fps.map(|f| (ms * f / 1000.0).floor() as i64)))?)?;
        let ctx = self.ctx.clone();
        aeg.set("ms_from_frame", lua.create_function(move |_, fr: f64| Ok(ctx.borrow().fps.map(|f| (fr * 1000.0 / f).round() as i64)))?)?;
        let ctx = self.ctx.clone();
        aeg.set("video_size", lua.create_function(move |_, ()| Ok(match ctx.borrow().video_size { Some((w, h)) => (Some(w), Some(h), Some(1.0), Some(0)), None => (None, None, None, None) }))?)?;
        aeg.set("keyframes", lua.create_function(|lua, ()| lua.create_table())?)?;
        aeg.set("get_audio_selection", lua.create_function(|_, ()| Ok(Value::Nil))?)?;
        aeg.set("gettext", lua.create_function(|_, s: String| Ok(s))?)?;
        aeg.set("lua_supports", lua.create_function(|_, _: Value| Ok(true))?)?;
        let ctx = self.ctx.clone();
        aeg.set("project_properties", lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for (k, v) in ctx.borrow().project.iter() { t.set(k.as_str(), v.as_str())?; }
            Ok(t)
        })?)?;
        let ctx = self.ctx.clone();
        aeg.set("decode_path", lua.create_function(move |_, p: String| {
            let c = ctx.borrow();
            let dir = c.script_dir.to_string_lossy().to_string();
            Ok(p.replace("?script", &dir).replace("?user", &dir).replace("?data", &dir).replace("?temp", &std::env::temp_dir().to_string_lossy()))
        })?)?;

        // yaklaşık metin ölçümü
        aeg.set("text_extents", lua.create_function(|_, (style, text): (Table, String)| {
            let fs = get_f(&style, "fontsize", 48.0)?;
            let sx = get_f(&style, "scale_x", 100.0)? / 100.0;
            let sy = get_f(&style, "scale_y", 100.0)? / 100.0;
            let sp = get_f(&style, "spacing", 0.0)?;
            let bold = get_b(&style, "bold", false)?;
            let w: f64 = text.chars().map(|c| {
                let base = if c.is_whitespace() { 0.28 } else if "iljI.,;:!'|".contains(c) { 0.28 } else if "mwMW@".contains(c) { 0.85 } else if c.is_ascii_uppercase() { 0.68 } else if (c as u32) > 0x2E80 { 1.0 } else { 0.52 };
                (base * fs * if bold { 1.05 } else { 1.0 } + sp) * sx
            }).sum();
            Ok((w, fs * 1.15 * sy, fs * 0.22 * sy, 0.0f64))
        })?)?;

        // dialog (başsız): varsayılan değerleri döndürür
        let dialog = lua.create_table()?;
        dialog.set("display", lua.create_function(|lua, (cfg, buttons, _ids): (Table, Option<Table>, Value)| {
            let res = lua.create_table()?;
            for c in cfg.sequence_values::<Table>() {
                let c = c?;
                let name: Option<String> = c.get("name")?;
                let Some(name) = name else { continue };
                let class: String = c.get("class")?;
                let v: Value = match class.as_str() {
                    "label" | "image" => continue,
                    "checkbox" => Value::Boolean(c.get::<Option<bool>>("value")?.unwrap_or(false)),
                    "dropdown" => { let v: Value = c.get("value")?; if let Value::Nil = v { let items: Option<Table> = c.get("items")?; match items { Some(t) => t.get::<Value>(1)?, None => Value::Nil } } else { v } }
                    "intedit" | "floatedit" | "slider" | "scrollbar" => c.get::<Option<f64>>("value")?.map(Value::Number).unwrap_or(Value::Number(0.0)),
                    _ => { let v: Value = c.get("value")?; if let Value::Nil = v { c.get("text")? } else { v } }
                };
                res.set(name, v)?;
            }
            let btn: Value = match buttons {
                Some(b) => b.get::<Value>(1)?,
                None => Value::Boolean(true),
            };
            Ok((btn, res))
        })?)?;
        dialog.set("open", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        dialog.set("save", lua.create_function(|_, _: Variadic<Value>| Ok(Value::Nil))?)?;
        aeg.set("dialog", dialog)?;
        g.set("aegisub", aeg)?;

        // unicode
        let uni = lua.create_table()?;
        uni.set("len", lua.create_function(|_, s: mlua::LuaString| Ok(s.to_string_lossy().chars().count()))?)?;
        uni.set("codepoint", lua.create_function(|_, s: mlua::LuaString| Ok(s.to_string_lossy().chars().next().map(|c| c as u32).unwrap_or(0)))?)?;
        uni.set("to_upper", lua.create_function(|_, s: mlua::LuaString| Ok(s.to_string_lossy().to_uppercase()))?)?;
        uni.set("to_lower", lua.create_function(|_, s: mlua::LuaString| Ok(s.to_string_lossy().to_lowercase()))?)?;
        uni.set("to_fold_case", lua.create_function(|_, s: mlua::LuaString| Ok(s.to_string_lossy().to_lowercase()))?)?;
        uni.set("chars", lua.create_function(|lua, s: mlua::LuaString| {
            let chars: Vec<String> = s.to_string_lossy().chars().map(|c| c.to_string()).collect();
            let mut i = 0usize;
            lua.create_function_mut(move |_, ()| { if i < chars.len() { i += 1; Ok(Some(chars[i - 1].clone())) } else { Ok(None) } })
        })?)?;
        g.set("unicode", uni.clone())?;

        // yerleşik modüller
        let preload: Table = lua.globals().get::<Table>("package")?.get("preload")?;
        let util_src = UTIL_LUA;
        preload.set("aegisub.util", lua.create_function(move |lua, _: MultiValue| lua.load(util_src).set_name("=aegisub.util").eval::<Value>())?)?;
        preload.set("unicode", lua.create_function(move |_, _: MultiValue| Ok(uni.clone()))?)?;
        Ok(())
    }

    /// Bir Lua betiği yükler; kaydettiği makroların sayısını döndürür.
    pub fn load_script(&self, path: &Path) -> Result<usize, String> {
        let src = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let src = String::from_utf8_lossy(&src).trim_start_matches('\u{feff}').to_string();
        {
            let mut c = self.ctx.borrow_mut();
            c.script_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            c.current_script = path.to_string_lossy().to_string();
        }
        let before = self.macros.borrow().len();
        self.load_in_env(&src, &path.to_string_lossy()).map_err(lerr)?;
        Ok(self.macros.borrow().len() - before)
    }

    fn load_in_env(&self, src: &str, chunk_name: &str) -> mlua::Result<()> {
        let lua = &self.lua;
        let env = lua.create_table()?;
        let mt = lua.create_table()?;
        mt.set("__index", lua.globals())?;
        env.set_metatable(Some(mt))?;
        // include(): aynı ortamda yükler
        let me_ctx = self.ctx.clone();
        let env2 = env.clone();
        let this = IncludeHelper { ctx: me_ctx };
        env.set("include", lua.create_function(move |lua, name: String| {
            let Some((cn, src)) = this.find(&name) else {
                return Err(mlua::Error::runtime(format!("include: '{}' bulunamadı", name)));
            };
            lua.load(&src).set_name(format!("@{}", cn)).set_environment(env2.clone()).exec()
        })?)?;
        let chunk = lua.load(src).set_name(format!("@{}", chunk_name)).set_environment(env.clone());
        chunk.exec()?;
        Ok(())
    }

    /// Makro doğrulaması (varsa `validate` fonksiyonu).
    pub fn validate(&self, idx: usize, doc: &Document, selection: &[usize], active: Option<usize>) -> bool {
        let macros = self.macros.borrow();
        let Some(m) = macros.get(idx) else { return false };
        let Some(v) = &m.validate else { return true };
        let r = (|| -> mlua::Result<bool> {
            let f: Function = self.lua.registry_value(v)?;
            let subs = self.lua.create_userdata(Subs(Rc::new(RefCell::new(doc_to_lines(doc)))))?;
            let off = doc.info.len() + doc.styles.len();
            let sel = self.lua.create_sequence_from(selection.iter().map(|i| (i + off + 1) as i64))?;
            let ret: Value = f.call((subs, sel, active.map(|a| (a + off + 1) as i64)))?;
            Ok(!matches!(ret, Value::Boolean(false) | Value::Nil))
        })();
        r.unwrap_or(false)
    }

    /// Makroyu çalıştırır. `selection`/`active` doc.events indeksleridir (0 tabanlı).
    /// Hata ya da iptalde belge değişmez.
    pub fn run(&self, idx: usize, doc: &mut Document, selection: &[usize], active: Option<usize>) -> Result<RunOutput, String> {
        let macros = self.macros.borrow();
        let m = macros.get(idx).ok_or("makro bulunamadı")?;
        let off = doc.info.len() + doc.styles.len();
        let shared = Rc::new(RefCell::new(doc_to_lines(doc)));
        self.logs.borrow_mut().clear();
        let call = || -> mlua::Result<(Vec<i64>, Option<i64>)> {
            let f: Function = self.lua.registry_value(&m.process)?;
            let subs = self.lua.create_userdata(Subs(shared.clone()))?;
            let sel = self.lua.create_sequence_from(selection.iter().map(|i| (i + off + 1) as i64))?;
            let ret: MultiValue = f.call((subs, sel, active.map(|a| (a + off + 1) as i64)))?;
            let mut it = ret.into_iter();
            let mut new_sel = vec![];
            if let Some(Value::Table(t)) = it.next() {
                for v in t.sequence_values::<i64>() { new_sel.push(v?); }
            } else {
                new_sel = selection.iter().map(|i| (i + off + 1) as i64).collect();
            }
            let new_active = it.next().and_then(|v| idx_of(&v));
            Ok((new_sel, new_active))
        };
        match call() {
            Ok((sel, act)) => {
                let lines = shared.borrow().clone();
                let new_off = lines.iter().filter(|l| !matches!(l, Line::Event(_))).count();
                lines_into_doc(lines, doc);
                let conv = |i: i64| (i as usize).checked_sub(new_off + 1);
                Ok(RunOutput {
                    selection: sel.into_iter().filter_map(conv).collect(),
                    active: act.and_then(conv).or(active),
                    logs: self.take_logs(),
                    cancelled: false,
                })
            }
            Err(e) => {
                if e.to_string().contains(CANCEL_MARK) {
                    Ok(RunOutput { selection: selection.to_vec(), active, logs: self.take_logs(), cancelled: true })
                } else {
                    let logs = self.take_logs().join("\n");
                    Err(format!("{}\n{}", lerr(e), logs).trim().to_string())
                }
            }
        }
    }
}

struct IncludeHelper {
    ctx: Rc<RefCell<Ctx>>,
}
impl IncludeHelper {
    fn find(&self, name: &str) -> Option<(String, String)> {
        let c = self.ctx.borrow();
        let mut dirs = vec![c.script_dir.clone()];
        dirs.extend(c.include_dirs.iter().cloned());
        for d in dirs {
            let p = d.join(name);
            if let Ok(src) = std::fs::read_to_string(&p) {
                return Some((p.to_string_lossy().to_string(), src));
            }
        }
        match name {
            "utils.lua" => Some(("utils.lua".into(), UTILS_LUA.into())),
            "karaskel.lua" => Some(("karaskel.lua".into(), KARASKEL_LUA.into())),
            "unicode.lua" | "re.lua" | "clipboard.lua" => Some((name.into(), String::new())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(dir: &Path, name: &str, src: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, src).unwrap();
        p
    }
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kitsune-lua-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn doc() -> Document {
        Document::parse("[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,merhaba\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\k50}ka{\\k50}ra\n")
    }

    #[test]
    fn macro_edits_lines() {
        let d = tmp("edit");
        let p = script(&d, "a.lua", r#"
script_name = "Büyük harf"
include("utils.lua")
function go(subs, sel, act)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then
      l.text = unicode.to_upper(l.text)
      l.start_time = l.start_time + 100
      subs[i] = l
    end
  end
  subs.append({class="dialogue", comment=true, layer=0, start_time=0, end_time=10, style="Default", actor="", margin_l=0, margin_r=0, margin_t=0, effect="", text="x", extra={}})
  aegisub.debug.out("n=%d", #subs)
  return {sel[1]}
end
aegisub.register_macro("Büyüt", "aciklama", go)
"#);
        let e = Engine::new().unwrap();
        assert_eq!(e.load_script(&p).unwrap(), 1);
        let mut doc = doc();
        let out = e.run(0, &mut doc, &[0], Some(0)).unwrap();
        assert_eq!(doc.events[0].text, "MERHABA");
        assert_eq!(doc.events[0].start, 1100);
        assert_eq!(doc.events.len(), 3);
        assert!(doc.events[2].is_comment());
        assert_eq!(out.selection, vec![0]);
        assert!(out.logs[0].starts_with("n="));
    }

    #[test]
    fn cancel_leaves_doc_untouched() {
        let d = tmp("cancel");
        let p = script(&d, "c.lua", "aegisub.register_macro('c','',function(s) local l=s[#s]; l.text='zzz'; s[#s]=l; aegisub.cancel() end)");
        let e = Engine::new().unwrap();
        e.load_script(&p).unwrap();
        let mut doc = doc();
        let before = doc.events.clone();
        let out = e.run(0, &mut doc, &[], None).unwrap();
        assert!(out.cancelled);
        assert_eq!(doc.events, before);
    }

    #[test]
    fn karaskel_works() {
        let d = tmp("kara");
        let p = script(&d, "k.lua", r#"
include("karaskel.lua")
aegisub.register_macro('k','',function(subs, sel)
  local meta, styles = karaskel.collect_head(subs, false)
  local l = subs[#subs]
  karaskel.preproc_line(subs, meta, styles, l)
  aegisub.debug.out("%d %d %d", meta.res_x, l.kara.n, l.kdur)
  assert(l.kara[2].start_time == 500)
  assert(styles["Default"].fontsize == 40)
end)"#);
        let e = Engine::new().unwrap();
        e.load_script(&p).unwrap();
        let mut doc = doc();
        let out = e.run(0, &mut doc, &[1], Some(1)).unwrap();
        assert_eq!(out.logs[0], "1280 2 1000");
    }

    #[test]
    fn error_reports_message() {
        let d = tmp("err");
        let p = script(&d, "e.lua", "aegisub.register_macro('e','',function() error('patladi') end)");
        let e = Engine::new().unwrap();
        e.load_script(&p).unwrap();
        let mut doc = doc();
        assert!(e.run(0, &mut doc, &[], None).unwrap_err().contains("patladi"));
    }

    #[test]
    fn util_module() {
        let d = tmp("util");
        let p = script(&d, "u.lua", r#"
local util = require("aegisub.util")
aegisub.register_macro('u','',function()
  aegisub.debug.out(util.ass_color(255,0,16))
  aegisub.debug.out(util.interpolate_color(0.5, "&H000000&", "&HFFFFFF&"))
  local h, t = util.headtail("a b c")
  aegisub.debug.out(h .. "|" .. t)
  aegisub.debug.out(tostring(unicode.len("çğş")))
end)"#);
        let e = Engine::new().unwrap();
        e.load_script(&p).unwrap();
        let mut doc = doc();
        let out = e.run(0, &mut doc, &[], None).unwrap();
        assert_eq!(out.logs, vec!["&H1000FF&", "&H808080&", "a|b c", "3"]);
    }
}
