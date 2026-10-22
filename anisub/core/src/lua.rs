//! Aegisub-compatible Lua automation host (LuaJIT).
use crate::ass::{Color, Event, Line, LineKind, Script, Style};
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use unicode_width::UnicodeWidthChar;

const PRELUDE: &str = include_str!("../lua/prelude.lua");
const UTILS: &str = include_str!("../lua/utils.lua");
const UNICODE: &str = include_str!("../lua/unicode.lua");
const KARASKEL: &str = include_str!("../lua/karaskel.lua");

#[derive(Default, Clone)]
pub struct HostConfig {
    pub fps: Option<f64>,
    pub video_size: Option<(i32, i32)>,
    pub include_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct MacroInfo {
    pub name: String,
    pub description: String,
    pub is_filter: bool,
}

struct SubsProxy {
    index: Function,
    newindex: Function,
    len: Function,
}

impl mlua::UserData for SubsProxy {
    fn add_methods<M: mlua::UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(mlua::MetaMethod::Index, |_, this, k: Value| this.index.call::<Value>(k));
        m.add_meta_method(mlua::MetaMethod::NewIndex, |_, this, (k, v): (Value, Value)| this.newindex.call::<()>((k, v)));
        m.add_meta_method(mlua::MetaMethod::Len, |_, this, ()| this.len.call::<Value>(()));
    }
}

pub struct LuaScript {
    lua: Lua,
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub log: Rc<RefCell<String>>,
}

pub struct RunResult {
    pub selection: Option<Vec<usize>>,
    pub log: String,
}

fn style_from_table(t: &Table) -> mlua::Result<Style> {
    let mut s = Style::default();
    let get_f = |k: &str, d: f64| -> f64 { t.get::<Option<f64>>(k).ok().flatten().unwrap_or(d) };
    let get_s = |k: &str| -> Option<String> { t.get::<Option<String>>(k).ok().flatten() };
    let get_b = |k: &str| -> bool { match t.get::<Value>(k) { Ok(Value::Boolean(b)) => b, Ok(Value::Number(n)) => n != 0.0, Ok(Value::Integer(n)) => n != 0, _ => false } };
    if let Some(v) = get_s("name") { s.name = v; }
    if let Some(v) = get_s("fontname") { s.fontname = v; }
    s.fontsize = get_f("fontsize", 20.0);
    for (k, slot) in [("color1", &mut s.color1), ("color2", &mut s.color2), ("color3", &mut s.color3), ("color4", &mut s.color4)] {
        if let Some(c) = get_s(k).and_then(|v| Color::parse(&v)) { *slot = c; }
    }
    s.bold = get_b("bold");
    s.italic = get_b("italic");
    s.underline = get_b("underline");
    s.strikeout = get_b("strikeout");
    s.scale_x = get_f("scale_x", 100.0);
    s.scale_y = get_f("scale_y", 100.0);
    s.spacing = get_f("spacing", 0.0);
    s.angle = get_f("angle", 0.0);
    s.borderstyle = get_f("borderstyle", 1.0) as i32;
    s.outline = get_f("outline", 0.0);
    s.shadow = get_f("shadow", 0.0);
    s.align = get_f("align", 2.0) as i32;
    s.margin_l = get_f("margin_l", 0.0) as i32;
    s.margin_r = get_f("margin_r", 0.0) as i32;
    s.margin_t = get_f("margin_t", get_f("margin_v", 0.0)) as i32;
    s.margin_b = get_f("margin_b", s.margin_t as f64) as i32;
    s.encoding = get_f("encoding", 1.0) as i32;
    Ok(s)
}

pub fn script_to_lua(lua: &Lua, script: &Script) -> mlua::Result<Table> {
    let data = lua.create_table()?;
    for (i, l) in script.lines.iter().enumerate() {
        let t = lua.create_table()?;
        t.set("section", l.section.as_str())?;
        t.set("extra", lua.create_table()?)?;
        match &l.kind {
            LineKind::Info { key, value } => {
                t.set("class", "info")?;
                t.set("key", key.as_str())?;
                t.set("value", value.as_str())?;
                t.set("raw", format!("{key}: {value}"))?;
            }
            LineKind::Style(s) => {
                t.set("class", "style")?;
                t.set("name", s.name.as_str())?;
                t.set("fontname", s.fontname.as_str())?;
                t.set("fontsize", s.fontsize)?;
                t.set("color1", s.color1.to_style_string())?;
                t.set("color2", s.color2.to_style_string())?;
                t.set("color3", s.color3.to_style_string())?;
                t.set("color4", s.color4.to_style_string())?;
                t.set("bold", s.bold)?;
                t.set("italic", s.italic)?;
                t.set("underline", s.underline)?;
                t.set("strikeout", s.strikeout)?;
                t.set("scale_x", s.scale_x)?;
                t.set("scale_y", s.scale_y)?;
                t.set("spacing", s.spacing)?;
                t.set("angle", s.angle)?;
                t.set("borderstyle", s.borderstyle)?;
                t.set("outline", s.outline)?;
                t.set("shadow", s.shadow)?;
                t.set("align", s.align)?;
                t.set("margin_l", s.margin_l)?;
                t.set("margin_r", s.margin_r)?;
                t.set("margin_t", s.margin_t)?;
                t.set("margin_b", s.margin_b)?;
                t.set("margin_v", s.margin_t)?;
                t.set("encoding", s.encoding)?;
                t.set("relative_to", 2)?;
                t.set("raw", format!("Style: {}", s.name))?;
            }
            LineKind::Dialogue(e) => {
                t.set("class", "dialogue")?;
                t.set("comment", e.comment)?;
                t.set("layer", e.layer)?;
                t.set("start_time", e.start_time)?;
                t.set("end_time", e.end_time)?;
                t.set("style", e.style.as_str())?;
                t.set("actor", e.actor.as_str())?;
                t.set("margin_l", e.margin_l)?;
                t.set("margin_r", e.margin_r)?;
                t.set("margin_t", e.margin_t)?;
                t.set("margin_b", e.margin_b)?;
                t.set("margin_v", e.margin_t)?;
                t.set("effect", e.effect.as_str())?;
                t.set("text", e.text.as_str())?;
                t.set("raw", "")?;
            }
            LineKind::Raw(r) => {
                t.set("class", "unknown")?;
                t.set("raw", r.as_str())?;
            }
        }
        data.set(i + 1, t)?;
    }
    Ok(data)
}

pub fn lua_to_script(data: &Table) -> mlua::Result<Script> {
    let mut script = Script::default();
    for pair in data.sequence_values::<Table>() {
        let t = pair?;
        let class: String = t.get("class")?;
        let section: Option<String> = t.get("section")?;
        let gs = |k: &str| -> String { t.get::<Option<String>>(k).ok().flatten().unwrap_or_default() };
        let gn = |k: &str| -> f64 { t.get::<Option<f64>>(k).ok().flatten().unwrap_or(0.0) };
        let (sec, kind) = match class.as_str() {
            "info" => (section.unwrap_or("[Script Info]".into()), LineKind::Info { key: gs("key"), value: match t.get::<Value>("value")? { Value::Number(n) => crate::ass::fmt_num(n), Value::Integer(i) => i.to_string(), Value::String(s) => s.to_string_lossy().to_string(), _ => String::new() } }),
            "style" => (section.unwrap_or("[V4+ Styles]".into()), LineKind::Style(style_from_table(&t)?)),
            "dialogue" => {
                let comment = matches!(t.get::<Value>("comment")?, Value::Boolean(true));
                let mt = t.get::<Option<f64>>("margin_t")?.or(t.get::<Option<f64>>("margin_v")?).unwrap_or(0.0);
                let mb = t.get::<Option<f64>>("margin_b")?.unwrap_or(mt);
                (
                    section.unwrap_or("[Events]".into()),
                    LineKind::Dialogue(Event {
                        comment,
                        layer: gn("layer") as i32,
                        start_time: gn("start_time").round() as i64,
                        end_time: gn("end_time").round() as i64,
                        style: gs("style"),
                        actor: gs("actor"),
                        margin_l: gn("margin_l") as i32,
                        margin_r: gn("margin_r") as i32,
                        margin_t: mt as i32,
                        margin_b: mb as i32,
                        effect: gs("effect"),
                        text: gs("text"),
                    }),
                )
            }
            _ => (section.unwrap_or_default(), LineKind::Raw(gs("raw"))),
        };
        script.lines.push(Line { section: sec, kind });
    }
    Ok(script)
}

fn char_width(c: char, em: f64, bold: bool) -> f64 {
    let w = match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.26,
        'm' | 'w' | 'M' | 'W' => 0.85,
        c if c.is_ascii_uppercase() => 0.68,
        c if c.is_ascii() => 0.52,
        c => match c.width() { Some(2) => 1.0, Some(0) | None => 0.0, _ => 0.6 },
    };
    em * w * if bold { 1.07 } else { 1.0 }
}

/// Approximate text extents (real font metrics are planned; this keeps layouts stable meanwhile).
pub fn approx_text_extents(style: &Style, text: &str) -> (f64, f64, f64, f64) {
    let sx = style.scale_x / 100.0;
    let sy = style.scale_y / 100.0;
    let mut width = 0.0;
    let mut n = 0;
    for c in text.chars() {
        width += (char_width(c, style.fontsize, style.bold) + style.spacing) * sx;
        n += 1;
    }
    let _ = n;
    (width, style.fontsize * 1.117 * sy, style.fontsize * 0.212 * sy, 0.0)
}

impl LuaScript {
    pub fn load(path: &Path, cfg: &HostConfig) -> Result<LuaScript, String> {
        let lua = unsafe { Lua::unsafe_new() };
        let log = Rc::new(RefCell::new(String::new()));
        let script_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let setup = || -> mlua::Result<()> {
            let host = lua.create_table()?;
            let l2 = log.clone();
            host.set("log", lua.create_function(move |_, (lvl, msg): (i64, String)| {
                let _ = lvl;
                l2.borrow_mut().push_str(&msg);
                Ok(())
            })?)?;
            host.set("text_extents", lua.create_function(|_, (st, text): (Table, String)| {
                let s = style_from_table(&st)?;
                Ok(approx_text_extents(&s, &text))
            })?)?;
            let fps = cfg.fps;
            host.set("frame_from_ms", lua.create_function(move |_, ms: f64| Ok(fps.map(|f| (ms * f / 1000.0).floor() as i64)))?)?;
            host.set("ms_from_frame", lua.create_function(move |_, fr: f64| Ok(fps.map(|f| (fr * 1000.0 / f).round() as i64)))?)?;
            let vs = cfg.video_size;
            host.set("video_size", lua.create_function(move |_, ()| Ok(match vs { Some((w, h)) => MultiValue::from_vec(vec![Value::Integer(w as _), Value::Integer(h as _), Value::Number(1.0), Value::Integer(0)]), None => MultiValue::new() }))?)?;
            host.set("keyframes", lua.create_function(|lua, ()| lua.create_table())?)?;
            let sd = script_dir.clone();
            host.set("decode_path", lua.create_function(move |_, p: String| {
                let r = if let Some(rest) = p.strip_prefix("?script") { format!("{}{}", sd.display(), rest) } else if let Some(rest) = p.strip_prefix("?temp") { format!("{}{}", std::env::temp_dir().display(), rest) } else if let Some(rest) = p.strip_prefix("?user").or(p.strip_prefix("?data")) { format!("{}{}", sd.display(), rest) } else { p };
                Ok(r)
            })?)?;
            host.set("project_properties", lua.create_function(|lua, ()| lua.create_table())?)?;
            host.set("dialog_open", lua.create_function(|_, _a: MultiValue| Ok(Value::Nil))?)?;
            host.set("dialog_save", lua.create_function(|_, _a: MultiValue| Ok(Value::Nil))?)?;
            host.set("dialog_display", lua.create_function(|lua, (cfg, buttons, ids): (Table, Option<Table>, Option<Table>)| {
                // Headless: accept defaults, press the first (OK-like) button.
                let res = lua.create_table()?;
                for pair in cfg.sequence_values::<Table>() {
                    let c = pair?;
                    let Some(name) = c.get::<Option<String>>("name")? else { continue };
                    let class: String = c.get::<Option<String>>("class")?.unwrap_or_default();
                    let v: Value = c.get("value")?;
                    let val = match (class.as_str(), v) {
                        ("checkbox", Value::Boolean(b)) => Value::Boolean(b),
                        ("checkbox", _) => Value::Boolean(false),
                        ("dropdown", Value::Nil) => {
                            match c.get::<Option<Table>>("items")? { Some(items) => items.get::<Value>(1)?, None => Value::Nil }
                        }
                        (_, Value::Nil) => match class.as_str() { "intedit" | "floatedit" | "scrollfloat" | "scrollint" => Value::Integer(0), "edit" | "textbox" | "label" => Value::String(lua.create_string("")?), _ => Value::Nil },
                        (_, v) => v,
                    };
                    res.set(name, val)?;
                }
                let btn: Value = match &buttons { Some(b) => b.get(1)?, None => Value::Boolean(true) };
                let _ = ids;
                Ok((btn, res))
            })?)?;
            let dirs = cfg.include_dirs.clone();
            let sd2 = script_dir.clone();
            host.set("find_include", lua.create_function(move |_, name: String| {
                match name.as_str() {
                    "utils.lua" => return Ok((Some(UTILS.to_string()), "utils.lua".to_string())),
                    "karaskel.lua" => return Ok((Some(KARASKEL.to_string()), "karaskel.lua".to_string())),
                    "unicode.lua" => return Ok((Some(UNICODE.to_string()), "unicode.lua".to_string())),
                    _ => {}
                }
                let mut cands = vec![sd2.join(&name)];
                for d in &dirs { cands.push(d.join(&name)); }
                for c in cands {
                    if let Ok(s) = std::fs::read_to_string(&c) { return Ok((Some(s), c.display().to_string())); }
                }
                Ok((None, String::new()))
            })?)?;
            lua.globals().set("__host", host)?;
            // package.path for require()
            let pkg: Table = lua.globals().get("package")?;
            let old: String = pkg.get("path")?;
            let mut np = format!("{}/?.lua;{}", script_dir.display(), old);
            for d in &cfg.include_dirs { np = format!("{}/?.lua;{}", d.display(), np); }
            pkg.set("path", np)?;
            lua.load(PRELUDE).set_name("=prelude").exec()?;
            // `require "utils"`-style access to the bundled libs
            let loaded: Table = pkg.get::<Table>("loaded")?;
            let _ = loaded;
            let preload: Table = pkg.get("preload")?;
            for (n, src) in [("utils", UTILS), ("unicode", UNICODE), ("karaskel", KARASKEL)] {
                let f = lua.create_function(move |lua, ()| { lua.load(src).set_name(n).exec()?; Ok(true) })?;
                preload.set(n, f)?;
            }
            Ok(())
        };
        setup().map_err(|e| format!("lua init: {e}"))?;
        let src = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = String::from_utf8_lossy(&src).to_string();
        let src = src.strip_prefix('\u{feff}').unwrap_or(&src).to_string();
        lua.load(&src).set_name(format!("@{}", path.display())).exec().map_err(|e| format!("{e}"))?;
        let g = lua.globals();
        let gs = |k: &str| g.get::<Option<String>>(k).ok().flatten().unwrap_or_default();
        let (name, description, author, version) = (gs("script_name"), gs("script_description"), gs("script_author"), gs("script_version"));
        let ver_ok = true;
        let _ = ver_ok;
        Ok(LuaScript { lua, path: path.to_path_buf(), name, description, author, version, log })
    }

    pub fn macros(&self) -> Vec<MacroInfo> {
        let mut out = vec![];
        if let Ok(m) = self.lua.globals().get::<Table>("__anisub") {
            for (key, is_filter) in [("macros", false), ("filters", true)] {
                if let Ok(t) = m.get::<Table>(key) {
                    for e in t.sequence_values::<Table>().flatten() {
                        out.push(MacroInfo { name: e.get("name").unwrap_or_default(), description: e.get("description").unwrap_or_default(), is_filter });
                    }
                }
            }
        }
        out
    }

    /// Run macro (or filter, after macros in listing order) number `idx` from `macros()`.
    pub fn run(&self, idx: usize, script: &mut Script, selection: &[usize], active: usize) -> Result<RunResult, String> {
        let infos = self.macros();
        let info = infos.get(idx).ok_or("no such macro")?.clone();
        let m: Table = self.lua.globals().get("__anisub").map_err(|e| e.to_string())?;
        let list: Table = m.get(if info.is_filter { "filters" } else { "macros" }).map_err(|e| e.to_string())?;
        let local_idx = if info.is_filter { idx - infos.iter().filter(|i| !i.is_filter).count() } else { idx };
        let entry: Table = list.get(local_idx + 1).map_err(|e| e.to_string())?;
        let func: Function = entry.get("process").map_err(|e| e.to_string())?;
        self.log.borrow_mut().clear();
        let data = script_to_lua(&self.lua, script).map_err(|e| e.to_string())?;
        let make: Function = m.get("make_subs").map_err(|e| e.to_string())?;
        let (index, newindex, len): (Function, Function, Function) = make.call(data.clone()).map_err(|e| e.to_string())?;
        let subs = self.lua.create_userdata(SubsProxy { index, newindex, len }).map_err(|e| e.to_string())?;
        let sel = self.lua.create_table().map_err(|e| e.to_string())?;
        for (i, s) in selection.iter().enumerate() {
            sel.set(i + 1, *s as i64 + 1).map_err(|e| e.to_string())?;
        }
        let ret: mlua::Result<MultiValue> = if info.is_filter {
            let cfg = self.lua.create_table().map_err(|e| e.to_string())?;
            func.call((subs, cfg))
        } else {
            func.call((subs, sel, active as i64 + 1))
        };
        let ret = match ret {
            Ok(r) => r,
            Err(e) => {
                let s = e.to_string();
                if s.contains("__anisub_cancel__") {
                    return Ok(RunResult { selection: None, log: self.log.borrow().clone() });
                }
                return Err(format!("{s}\n{}", self.log.borrow()));
            }
        };
        *script = lua_to_script(&data).map_err(|e| e.to_string())?;
        let selection = match ret.into_iter().next() {
            Some(Value::Table(t)) => Some(t.sequence_values::<i64>().flatten().map(|v| (v.max(1) - 1) as usize).collect()),
            _ => None,
        };
        Ok(RunResult { selection, log: self.log.borrow().clone() })
    }
}
