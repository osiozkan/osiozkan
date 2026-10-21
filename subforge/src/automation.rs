//! Aegisub "Automation 4" Lua compatibility layer.
//!
//! A `Script` owns a Lua state and the shared subtitle document. Scripts register macros and
//! export filters exactly as they would in Aegisub; `subs` behaves like Aegisub's subtitle
//! file userdata (1-based index, `#subs`, append/insert/delete/deleterange).

use crate::ass::{fmt_color, parse_color, Document, Entry, Event, Style};
use crate::time::fmt_num;
use anyhow::{anyhow, Result};
use mlua::{Function, Lua, MetaMethod, Table, UserData, UserDataMethods, Value, Variadic};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type SharedDoc = Rc<RefCell<Document>>;

const PRELUDE: &str = include_str!("../lua/prelude.lua");

/// Aegisub's stock macros (unmodified, licence headers intact), bundled so they work out of the box.
pub const BUILTIN_SCRIPTS: &[(&str, &str)] = &[
    ("kara-templater.lua", include_str!("../lua/aegisub/autoload/kara-templater.lua")),
    ("cleantags-autoload.lua", include_str!("../lua/aegisub/autoload/cleantags-autoload.lua")),
    ("karaoke-auto-leadin.lua", include_str!("../lua/aegisub/autoload/karaoke-auto-leadin.lua")),
    ("macro-1-edgeblur.lua", include_str!("../lua/aegisub/autoload/macro-1-edgeblur.lua")),
    ("strip-tags.lua", include_str!("../lua/aegisub/autoload/strip-tags.lua")),
];

/// Stock include files resolved by `include()` when nothing on disk matches.
pub const BUILTIN_INCLUDES: &[(&str, &str)] = &[
    ("karaskel.lua", include_str!("../lua/aegisub/include/karaskel.lua")),
    ("karaskel-auto4.lua", include_str!("../lua/aegisub/include/karaskel-auto4.lua")),
    ("cleantags.lua", include_str!("../lua/aegisub/include/cleantags.lua")),
];

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Write a bundled script to a temp file (scripts are loaded from disk) and return its path.
/// Accepts `builtin:name.lua` or `name.lua`.
pub fn materialize_builtin(name: &str) -> Option<PathBuf> {
    let name = name.strip_prefix("builtin:").unwrap_or(name);
    let src = builtin_script(name)?;
    let dir = std::env::temp_dir().join(format!("subforge-builtin-{}", env!("CARGO_PKG_VERSION")));
    std::fs::create_dir_all(&dir).ok()?;
    let p = dir.join(name);
    if std::fs::read_to_string(&p).map(|c| c != src).unwrap_or(true) {
        std::fs::write(&p, src).ok()?;
    }
    Some(p)
}

/// Source of a bundled script by file name.
pub fn builtin_script(name: &str) -> Option<&'static str> {
    BUILTIN_SCRIPTS.iter().find(|(n, _)| *n == name).map(|(_, s)| strip_bom(s))
}

#[derive(Debug, Clone)]
pub struct MacroInfo {
    pub name: String,
    pub description: String,
    pub is_filter: bool,
}

/// Interactive dialog callback: receives `{controls, buttons, ids}` as JSON, returns
/// `{button, values}` (or None => headless defaults).
pub type DialogHook = std::sync::Arc<dyn Fn(serde_json::Value) -> Option<serde_json::Value> + Send + Sync>;
/// Progress callback: (`set` | `task` | `title`, value).
pub type ProgressHook = std::sync::Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

#[derive(Default, Clone)]
pub struct RunOptions {
    /// 1-based entry indices (into the flat subs list). Empty = all dialogue lines.
    pub selection: Vec<usize>,
    pub active: Option<usize>,
    /// Values to answer `aegisub.dialog.display` controls with, keyed by control name.
    pub params: HashMap<String, String>,
    pub fps: Option<f64>,
    /// Extra directories searched by `include()` (e.g. an Aegisub `automation/include`).
    pub include_dirs: Vec<PathBuf>,
    /// Extra font directories for `aegisub.text_extents`.
    pub font_dirs: Vec<PathBuf>,
    pub dialog_hook: Option<DialogHook>,
    pub progress_hook: Option<ProgressHook>,
    /// Set by the host to ask a running script to stop (`aegisub.progress.is_cancelled()`).
    pub cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

#[derive(Debug, Default)]
pub struct RunResult {
    pub log: Vec<String>,
    pub selection: Vec<usize>,
    pub cancelled: bool,
}

pub struct Script {
    lua: Lua,
    doc: SharedDoc,
    pub path: PathBuf,
    log: Rc<RefCell<Vec<String>>>,
}

fn color_str(c: u32) -> String {
    // Aegisub exposes colors as &HAABBGGRR for styles
    fmt_color(c)
}

fn entry_to_table(lua: &Lua, e: &Entry) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    match e {
        Entry::Info { key, value } => {
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            t.set("raw", if key == ";" { format!(";{}", value) } else { format!("{}: {}", key, value) })?;
        }
        Entry::Style(s) => {
            t.set("class", "style")?;
            t.set("section", "V4+ Styles")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            for (i, c) in s.colors.iter().enumerate() {
                t.set(format!("color{}", i + 1), color_str(*c))?;
            }
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
            t.set("align", s.align)?;
            t.set("margin_l", s.margin_l)?;
            t.set("margin_r", s.margin_r)?;
            t.set("margin_t", s.margin_v)?;
            t.set("margin_b", s.margin_v)?;
            t.set("encoding", s.encoding)?;
            t.set("relative_to", 2)?;
            t.set("raw", crate::ass::format_style(s))?;
        }
        Entry::Event(ev) => {
            t.set("class", "dialogue")?;
            t.set("section", "Events")?;
            t.set("comment", ev.comment)?;
            t.set("layer", ev.layer)?;
            t.set("start_time", ev.start)?;
            t.set("end_time", ev.end)?;
            t.set("style", ev.style.as_str())?;
            t.set("actor", ev.actor.as_str())?;
            t.set("margin_l", ev.margin_l)?;
            t.set("margin_r", ev.margin_r)?;
            t.set("margin_t", ev.margin_v)?;
            t.set("margin_b", ev.margin_v)?;
            t.set("effect", ev.effect.as_str())?;
            t.set("text", ev.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", crate::ass::format_event(ev))?;
        }
    }
    Ok(t)
}

fn get_str(t: &Table, k: &str, d: &str) -> String {
    match t.get::<Value>(k) {
        Ok(Value::String(s)) => s.to_string_lossy().to_string(),
        Ok(Value::Integer(i)) => i.to_string(),
        Ok(Value::Number(n)) => fmt_num(n),
        _ => d.to_string(),
    }
}
fn get_f64(t: &Table, k: &str, d: f64) -> f64 {
    match t.get::<Value>(k) {
        Ok(Value::Integer(i)) => i as f64,
        Ok(Value::Number(n)) => n,
        Ok(Value::String(s)) => s.to_str().ok().and_then(|s| s.trim().parse().ok()).unwrap_or(d),
        _ => d,
    }
}
fn get_bool(t: &Table, k: &str) -> bool {
    match t.get::<Value>(k) {
        Ok(Value::Boolean(b)) => b,
        Ok(Value::Integer(i)) => i != 0,
        Ok(Value::Number(n)) => n != 0.0,
        _ => false,
    }
}
fn get_color(t: &Table, k: &str, d: u32) -> u32 {
    match t.get::<Value>(k) {
        Ok(Value::String(s)) => parse_color(&s.to_string_lossy()),
        Ok(Value::Integer(i)) => i as u32,
        _ => d,
    }
}

fn table_to_entry(t: &Table) -> mlua::Result<Entry> {
    let class = get_str(t, "class", "");
    match class.as_str() {
        "info" => Ok(Entry::Info { key: get_str(t, "key", ""), value: get_str(t, "value", "") }),
        "style" => {
            let d = Style::default();
            let mv = get_f64(t, "margin_t", get_f64(t, "margin_b", d.margin_v as f64)) as i32;
            Ok(Entry::Style(Style {
                name: get_str(t, "name", "Default"),
                fontname: get_str(t, "fontname", &d.fontname),
                fontsize: get_f64(t, "fontsize", d.fontsize),
                colors: [
                    get_color(t, "color1", d.colors[0]),
                    get_color(t, "color2", d.colors[1]),
                    get_color(t, "color3", d.colors[2]),
                    get_color(t, "color4", d.colors[3]),
                ],
                bold: get_bool(t, "bold"),
                italic: get_bool(t, "italic"),
                underline: get_bool(t, "underline"),
                strikeout: get_bool(t, "strikeout"),
                scale_x: get_f64(t, "scale_x", 100.0),
                scale_y: get_f64(t, "scale_y", 100.0),
                spacing: get_f64(t, "spacing", 0.0),
                angle: get_f64(t, "angle", 0.0),
                border_style: get_f64(t, "borderstyle", 1.0) as i32,
                outline: get_f64(t, "outline", d.outline),
                shadow: get_f64(t, "shadow", d.shadow),
                align: get_f64(t, "align", 2.0) as i32,
                margin_l: get_f64(t, "margin_l", 10.0) as i32,
                margin_r: get_f64(t, "margin_r", 10.0) as i32,
                margin_v: mv,
                encoding: get_f64(t, "encoding", 1.0) as i32,
            }))
        }
        "dialogue" => Ok(Entry::Event(Event {
            comment: get_bool(t, "comment"),
            layer: get_f64(t, "layer", 0.0) as i32,
            start: get_f64(t, "start_time", 0.0).round() as i64,
            end: get_f64(t, "end_time", 0.0).round() as i64,
            style: get_str(t, "style", "Default"),
            actor: get_str(t, "actor", ""),
            margin_l: get_f64(t, "margin_l", 0.0) as i32,
            margin_r: get_f64(t, "margin_r", 0.0) as i32,
            margin_v: get_f64(t, "margin_t", get_f64(t, "margin_b", 0.0)) as i32,
            effect: get_str(t, "effect", ""),
            text: get_str(t, "text", ""),
        })),
        other => Err(mlua::Error::runtime(format!("Invalid or unsupported line class: '{}'", other))),
    }
}

struct Subs(SharedDoc);

fn idx_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn val_to_entries(args: &[Value]) -> mlua::Result<Vec<Entry>> {
    args.iter()
        .map(|a| match a {
            Value::Table(t) => table_to_entry(t),
            _ => Err(mlua::Error::runtime("expected a line table")),
        })
        .collect()
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().entries.len()));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| -> mlua::Result<Value> {
            if let Some(i) = idx_of(&key) {
                let d = this.0.borrow();
                if i < 1 || i as usize > d.entries.len() {
                    return Err(mlua::Error::runtime(format!("Requested out-of-range line from subtitle file: {}", i)));
                }
                return Ok(Value::Table(entry_to_table(lua, &d.entries[i as usize - 1])?));
            }
            let Value::String(name) = key else { return Ok(Value::Nil) };
            let doc = this.0.clone();
            let name = name.to_string_lossy().to_string();
            let f: Function = match name.as_str() {
                "n" => return Ok(Value::Integer(doc.borrow().entries.len() as i64)),
                "script_resolution" => lua.create_function(move |_, _: Variadic<Value>| {
                    let (x, y) = doc.borrow().res();
                    Ok((x, y))
                })?,
                "append" => lua.create_function(move |_, args: Variadic<Value>| {
                    let es = val_to_entries(&args)?;
                    doc.borrow_mut().entries.extend(es);
                    Ok(())
                })?,
                "insert" => lua.create_function(move |_, args: Variadic<Value>| {
                    let at = args.first().and_then(idx_of).ok_or_else(|| mlua::Error::runtime("insert: index expected"))?;
                    let es = val_to_entries(&args[1..])?;
                    let mut d = doc.borrow_mut();
                    let pos = (at.max(1) as usize - 1).min(d.entries.len());
                    for (k, e) in es.into_iter().enumerate() {
                        d.entries.insert(pos + k, e);
                    }
                    Ok(())
                })?,
                "delete" => lua.create_function(move |_, args: Variadic<Value>| {
                    let mut idxs: Vec<i64> = Vec::new();
                    for a in args.iter() {
                        match a {
                            Value::Table(t) => {
                                for v in t.sequence_values::<i64>() {
                                    idxs.push(v?);
                                }
                            }
                            v => idxs.push(idx_of(v).ok_or_else(|| mlua::Error::runtime("delete: index expected"))?),
                        }
                    }
                    idxs.sort_unstable();
                    idxs.dedup();
                    let mut d = doc.borrow_mut();
                    for i in idxs.into_iter().rev() {
                        if i < 1 || i as usize > d.entries.len() {
                            return Err(mlua::Error::runtime(format!("delete: line out of range: {}", i)));
                        }
                        d.entries.remove(i as usize - 1);
                    }
                    Ok(())
                })?,
                "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                    let mut d = doc.borrow_mut();
                    let (a, b) = (a.max(1) as usize, (b.max(0) as usize).min(d.entries.len()));
                    if a <= b {
                        d.entries.drain(a - 1..b);
                    }
                    Ok(())
                })?,
                _ => return Ok(Value::Nil),
            };
            Ok(Value::Function(f))
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let i = idx_of(&key).ok_or_else(|| mlua::Error::runtime("subs: integer index expected"))?;
            let mut d = this.0.borrow_mut();
            let n = d.entries.len() as i64;
            match val {
                Value::Nil => {
                    if i < 1 || i > n {
                        return Err(mlua::Error::runtime("Attempt to delete out-of-range line"));
                    }
                    d.entries.remove(i as usize - 1);
                }
                Value::Table(t) => {
                    let e = table_to_entry(&t)?;
                    if i == 0 {
                        d.entries.push(e);
                    } else if i < 0 {
                        let pos = ((-i) as usize - 1).min(d.entries.len());
                        d.entries.insert(pos, e);
                    } else if i <= n {
                        d.entries[i as usize - 1] = e;
                    } else if i == n + 1 {
                        d.entries.push(e); // lenient: subs[#subs+1] = line appends
                    } else {
                        return Err(mlua::Error::runtime("Attempt to set out-of-range line"));
                    }
                }
                _ => return Err(mlua::Error::runtime("subs: line table expected")),
            }
            Ok(())
        });
    }
}

/// Rough text metrics until real font shaping lands: Latin ~0.52em, CJK/fullwidth 1em.
pub fn approx_char_width(c: char) -> f64 {
    let cp = c as u32;
    match cp {
        0x20 => 0.28,
        0x21..=0x7e => match c {
            'i' | 'l' | 'I' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.27,
            'm' | 'w' | 'M' | 'W' => 0.8,
            'A'..='Z' => 0.65,
            _ => 0.52,
        },
        0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xff00..=0xff60 => 1.0,
        _ => 0.6,
    }
}

fn text_extents(db: &mut crate::fonts::FontDb, style: &Table, text: &str) -> (f64, f64, f64, f64) {
    let e = db.extents(
        &get_str(style, "fontname", "Arial"),
        get_f64(style, "fontsize", 48.0),
        get_bool(style, "bold"),
        get_bool(style, "italic"),
        get_f64(style, "scale_x", 100.0),
        get_f64(style, "scale_y", 100.0),
        get_f64(style, "spacing", 0.0),
        text,
    );
    (e.width, e.height, e.descent, e.extlead)
}

impl Script {
    pub fn load(path: &Path, doc: SharedDoc, opts: &RunOptions) -> Result<Script> {
        // Full standard libraries (ffi, debug, jit) like Aegisub: scripts are trusted user code.
        let lua = unsafe { Lua::unsafe_new() };
        let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let script_dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let script_dir = if script_dir.as_os_str().is_empty() { PathBuf::from(".") } else { script_dir };
        let sf = lua.create_table().map_err(lerr)?;
        {
            let log = log.clone();
            sf.set(
                "log",
                lua.create_function(move |_, (lvl, msg): (i64, String)| {
                    log.borrow_mut().push(if lvl <= 1 { format!("[error] {}", msg) } else { msg });
                    Ok(())
                })
                .map_err(lerr)?,
            )
            .map_err(lerr)?;
        }
        {
            let hook = opts.progress_hook.clone();
            sf.set(
                "progress",
                lua.create_function(move |_, (kind, v): (String, Value)| {
                    if let Some(h) = &hook {
                        h(&kind, lua_to_json(&v, 0));
                    }
                    Ok(())
                })
                .map_err(lerr)?,
            )
            .map_err(lerr)?;
            let flag = opts.cancel_flag.clone();
            sf.set(
                "is_cancelled",
                lua.create_function(move |_, ()| Ok(flag.as_ref().map(|f| f.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false))).map_err(lerr)?,
            )
            .map_err(lerr)?;
            if let Some(h) = opts.dialog_hook.clone() {
                sf.set("has_dialog", true).map_err(lerr)?;
                {
                    let h2 = h.clone();
                    // aegisub.dialog.open/save -> {"file_dialog": {kind, title, default_file, default_dir, wildcards, multiple}}
                    sf.set(
                        "file_dialog",
                        lua.create_function(move |lua, (kind, title, default_file, default_dir, wildcards, multiple): (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<bool>)| {
                            let spec = serde_json::json!({"file_dialog": {"kind": kind, "title": title, "default_file": default_file, "default_dir": default_dir, "wildcards": wildcards, "multiple": multiple.unwrap_or(false)}});
                            match h2(spec).and_then(|r| r.get("values").and_then(|v| v.get("path")).cloned()) {
                                Some(p) if !p.is_null() => json_to_lua(lua, &p),
                                _ => Ok(Value::Nil),
                            }
                        })
                        .map_err(lerr)?,
                    )
                    .map_err(lerr)?;
                }
                sf.set(
                    "dialog",
                    lua.create_function(move |lua, (controls, buttons, ids): (Value, Value, Value)| {
                        let spec = serde_json::json!({"controls": lua_to_json(&controls, 0), "buttons": lua_to_json(&buttons, 0), "ids": lua_to_json(&ids, 0)});
                        match h(spec) {
                            Some(reply) => {
                                let button = reply.get("button").cloned().unwrap_or(serde_json::Value::Null);
                                let values = reply.get("values").cloned().unwrap_or(serde_json::json!({}));
                                Ok((json_to_lua(lua, &button)?, json_to_lua(lua, &values)?))
                            }
                            None => Ok((Value::Nil, Value::Nil)),
                        }
                    })
                    .map_err(lerr)?,
                )
                .map_err(lerr)?;
            }
        }
        {
            let mut db = crate::fonts::FontDb::new();
            db.add_dir(script_dir.clone());
            db.add_dir(script_dir.join("fonts"));
            for d in &opts.font_dirs {
                db.add_dir(d.clone());
            }
            let db = RefCell::new(db);
            sf.set(
                "text_extents",
                lua.create_function(move |_, (style, text): (Table, Option<String>)| {
                    Ok(text_extents(&mut db.borrow_mut(), &style, &text.unwrap_or_default()))
                })
                .map_err(lerr)?,
            )
            .map_err(lerr)?;
        }
        sf.set(
            "parse_karaoke_data",
            lua.create_function(|lua, text: String| {
                let t = lua.create_table()?;
                for (i, s) in crate::tags::karaoke_data(&text).iter().enumerate() {
                    let e = lua.create_table()?;
                    e.set("duration", s.dur_cs * 10)?;
                    e.set("start_time", s.start_ms)?;
                    e.set("end_time", s.start_ms + s.dur_cs * 10)?;
                    e.set("tag", s.kind.as_str())?;
                    e.set("text", s.raw.as_str())?;
                    e.set("text_stripped", s.text.as_str())?;
                    t.raw_set(i as i64, e)?;
                }
                Ok(t)
            })
            .map_err(lerr)?,
        )
        .map_err(lerr)?;
        sf.set("codepoint", lua.create_function(|_, c: String| Ok(c.chars().next().map(|c| c as u32).unwrap_or(0))).map_err(lerr)?).map_err(lerr)?;
        sf.set("upper", lua.create_function(|_, s: String| Ok(s.to_uppercase())).map_err(lerr)?).map_err(lerr)?;
        sf.set("lower", lua.create_function(|_, s: String| Ok(s.to_lowercase())).map_err(lerr)?).map_err(lerr)?;
        let params = lua.create_table().map_err(lerr)?;
        for (k, v) in &opts.params {
            params.set(k.as_str(), v.as_str()).map_err(lerr)?;
        }
        sf.set("params", params).map_err(lerr)?;
        if let Some(f) = opts.fps {
            sf.set("fps", f).map_err(lerr)?;
        }
        {
            let d = doc.clone();
            let dir = script_dir.clone();
            sf.set(
                "project_properties",
                lua.create_function(move |lua, ()| {
                    let t = lua.create_table()?;
                    let (x, y) = d.borrow().res();
                    t.set("video_zoom", 0)?;
                    t.set("ar_mode", 0)?;
                    t.set("ar_value", 0)?;
                    t.set("script_resolution", x)?;
                    let _ = dir.display();
                    t.set("res_x", x)?;
                    t.set("res_y", y)?;
                    Ok(t)
                })
                .map_err(lerr)?,
            )
            .map_err(lerr)?;
        }
        {
            let dir = script_dir.clone();
            sf.set(
                "decode_path",
                lua.create_function(move |_, p: String| {
                    let d = dir.to_string_lossy();
                    Ok(p.replace("?script", &d).replace("?data", &d).replace("?user", &d).replace("?temp", "/tmp").replace("?video", &d).replace("?audio", &d))
                })
                .map_err(lerr)?,
            )
            .map_err(lerr)?;
        }
        {
            let dir = script_dir.clone();
            let extra = opts.include_dirs.clone();
            sf.set(
                "find_include",
                lua.create_function(move |_, name: String| {
                    let mut candidates = vec![dir.join(&name), dir.join("include").join(&name), PathBuf::from(&name)];
                    candidates.extend(extra.iter().map(|d| d.join(&name)));
                    for c in candidates {
                        if let Ok(bytes) = std::fs::read(&c) {
                            let s = String::from_utf8_lossy(&bytes);
                            let s = s.strip_prefix('\u{feff}').unwrap_or(&s).to_string();
                            return Ok((Some(s), Some(c.to_string_lossy().to_string())));
                        }
                    }
                    if let Some((n, src)) = BUILTIN_INCLUDES.iter().find(|(n, _)| *n == name) {
                        return Ok((Some(strip_bom(src).to_string()), Some(format!("{} (built-in)", n))));
                    }
                    Ok((None, None))
                })
                .map_err(lerr)?,
            )
            .map_err(lerr)?;
        }
        crate::lua_re::install(&lua, &sf).map_err(lerr)?;
        crate::lua_lfs::install(&lua, &sf).map_err(lerr)?;
        lua.globals().set("__sf", sf).map_err(lerr)?;
        lua.load(PRELUDE).set_name("prelude.lua").exec().map_err(lerr)?;
        // package.path so `require "mod"` finds modules next to the script
        let pp = format!("{0}/?.lua;{0}/include/?.lua;./?.lua", script_dir.display());
        lua.load(format!("package.path = {:?} .. ';' .. package.path", pp)).exec().map_err(lerr)?;
        let src = std::fs::read(path)?;
        let src = String::from_utf8_lossy(&src).to_string();
        let src = src.strip_prefix('\u{feff}').unwrap_or(&src).to_string();
        lua.load(&src).set_name(format!("@{}", path.display())).exec().map_err(lerr)?;
        Ok(Script { lua, doc, path: path.to_path_buf(), log })
    }

    fn sf(&self) -> Result<Table> {
        self.lua.globals().get("__sf").map_err(lerr)
    }

    /// Script metadata globals (script_name, script_description, ...).
    pub fn meta(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        for k in ["script_name", "script_description", "script_author", "script_version"] {
            if let Ok(Value::String(s)) = self.lua.globals().get::<Value>(k) {
                m.insert(k.to_string(), s.to_string_lossy().to_string());
            }
        }
        m
    }

    pub fn list(&self) -> Result<Vec<MacroInfo>> {
        let sf = self.sf()?;
        let mut out = Vec::new();
        for (key, is_filter) in [("macros", false), ("filters", true)] {
            let t: Table = sf.get(key).map_err(lerr)?;
            for v in t.sequence_values::<Table>() {
                let v = v.map_err(lerr)?;
                out.push(MacroInfo {
                    name: get_str(&v, "name", ""),
                    description: get_str(&v, "description", ""),
                    is_filter,
                });
            }
        }
        Ok(out)
    }

    fn find(&self, name: &str) -> Result<(Table, bool)> {
        let sf = self.sf()?;
        for (key, is_filter) in [("macros", false), ("filters", true)] {
            let t: Table = sf.get(key).map_err(lerr)?;
            for v in t.sequence_values::<Table>() {
                let v = v.map_err(lerr)?;
                if get_str(&v, "name", "") == name {
                    return Ok((v, is_filter));
                }
            }
        }
        Err(anyhow!("macro or filter '{}' not found in {}", name, self.path.display()))
    }

    /// Run a macro (or export filter) by name. On error or cancel the document is restored.
    pub fn run(&self, name: &str, opts: &RunOptions) -> Result<RunResult> {
        let (m, is_filter) = self.find(name)?;
        let snapshot = self.doc.borrow().clone();
        let subs = self.lua.create_userdata(Subs(self.doc.clone())).map_err(lerr)?;
        let mut sel = opts.selection.clone();
        if sel.is_empty() {
            sel = self
                .doc
                .borrow()
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e, Entry::Event(_)))
                .map(|(i, _)| i + 1)
                .collect();
        }
        let sel_t = self.lua.create_sequence_from(sel.iter().map(|i| *i as i64)).map_err(lerr)?;
        let active = opts.active.or_else(|| sel.first().copied()).unwrap_or(1) as i64;
        let process: Function = m.get("process").map_err(lerr)?;
        let validate: Value = m.get("validate").map_err(lerr)?;
        if let Value::Function(v) = validate {
            if !is_filter {
                let ok: Value = v.call((subs.clone(), sel_t.clone(), active)).map_err(lerr)?;
                if matches!(ok, Value::Boolean(false) | Value::Nil) {
                    return Err(anyhow!("macro '{}' is not valid for this selection (validate returned false)", name));
                }
            }
        }
        let call: mlua::Result<mlua::MultiValue> = if is_filter {
            let cfg = self.lua.create_table().map_err(lerr)?;
            for (k, v) in &opts.params {
                cfg.set(k.as_str(), v.as_str()).map_err(lerr)?;
            }
            process.call((subs, cfg))
        } else {
            process.call((subs, sel_t, active))
        };
        let log = self.log.borrow().clone();
        match call {
            Ok(ret) => {
                let mut selection = sel;
                if let Some(Value::Table(t)) = ret.iter().next() {
                    selection = t.sequence_values::<i64>().filter_map(|v| v.ok()).map(|v| v as usize).collect();
                }
                normalize_order(&mut self.doc.borrow_mut());
                Ok(RunResult { log, selection, cancelled: false })
            }
            Err(e) => {
                *self.doc.borrow_mut() = snapshot;
                if e.to_string().contains("__sf_cancel__") {
                    Ok(RunResult { log, selection: vec![], cancelled: true })
                } else {
                    Err(anyhow!("{}", lerr(e)))
                }
            }
        }
    }
}

/// Aegisub keeps Info, Styles, Events grouped; mirror that after scripts finish.
fn normalize_order(doc: &mut Document) {
    let rank = |e: &Entry| match e {
        Entry::Info { .. } => 0,
        Entry::Style(_) => 1,
        Entry::Event(_) => 2,
    };
    doc.entries.sort_by_key(rank); // stable
}

fn lua_to_json(v: &Value, depth: usize) -> serde_json::Value {
    use serde_json::Value as J;
    if depth > 8 {
        return J::Null;
    }
    match v {
        Value::Nil => J::Null,
        Value::Boolean(b) => J::Bool(*b),
        Value::Integer(i) => J::from(*i),
        Value::Number(n) => serde_json::Number::from_f64(*n).map(J::Number).unwrap_or(J::Null),
        Value::String(s) => J::String(s.to_string_lossy().to_string()),
        Value::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                J::Array(t.sequence_values::<Value>().filter_map(|x| x.ok()).map(|x| lua_to_json(&x, depth + 1)).collect())
            } else {
                let mut m = serde_json::Map::new();
                for (k, val) in t.pairs::<Value, Value>().flatten() {
                    if let Value::String(k) = k {
                        m.insert(k.to_string_lossy().to_string(), lua_to_json(&val, depth + 1));
                    }
                }
                J::Object(m)
            }
        }
        _ => J::Null,
    }
}

fn json_to_lua(lua: &Lua, v: &serde_json::Value) -> mlua::Result<Value> {
    use serde_json::Value as J;
    Ok(match v {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        J::String(s) => Value::String(lua.create_string(s)?),
        J::Array(a) => {
            let t = lua.create_table()?;
            for (i, x) in a.iter().enumerate() {
                t.raw_set(i as i64 + 1, json_to_lua(lua, x)?)?;
            }
            Value::Table(t)
        }
        J::Object(o) => {
            let t = lua.create_table()?;
            for (k, x) in o {
                t.set(k.as_str(), json_to_lua(lua, x)?)?;
            }
            Value::Table(t)
        }
    })
}

fn lerr(e: mlua::Error) -> anyhow::Error {
    anyhow!("{}", e)
}
