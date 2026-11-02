//! Aegisub Automation 4 (Lua) compatible runtime for osisub.
//!
//! Scripts run on LuaJIT (Lua 5.1 semantics), exactly like Aegisub, and see the
//! subtitle file as the `subs` object (`subs[i]`, `#subs`, `subs.insert`, ...).

use mlua::prelude::*;
use osisub_core::ass::{event_line, norm_color, Entry, Event, Script, Style};
use std::cell::RefCell;
use std::rc::Rc;

const PRELUDE: &str = include_str!("../lua/prelude.lua");

fn embedded(name: &str) -> Option<&'static str> {
    Some(match name {
        "util.lua" => include_str!("../lua/util.lua"),
        "unicode.lua" => include_str!("../lua/unicode.lua"),
        "utf8.lua" => include_str!("../lua/utf8.lua"),
        "karaskel.lua" => include_str!("../lua/karaskel.lua"),
        _ => return None,
    })
}

#[derive(Clone, Debug, Default)]
pub struct ScriptMeta {
    pub file: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

#[derive(Clone, Debug)]
pub struct MacroInfo {
    pub script: String,
    pub name: String,
    pub description: String,
    pub is_filter: bool,
}

struct MacroDef {
    info: MacroInfo,
    process: LuaFunction,
    validate: Option<LuaFunction>,
}

#[derive(Debug)]
pub enum RunError {
    Lua(String),
    Cancelled,
    NotFound(String),
    Invalid,
}
impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Lua(s) => write!(f, "{s}"),
            RunError::Cancelled => write!(f, "cancelled by script"),
            RunError::NotFound(n) => write!(f, "no macro or filter named '{n}'"),
            RunError::Invalid => write!(f, "macro validation failed for this selection"),
        }
    }
}
impl std::error::Error for RunError {}

fn map_err(e: LuaError) -> RunError {
    let s = e.to_string();
    if s.contains("__osisub_cancel__") {
        RunError::Cancelled
    } else {
        RunError::Lua(s)
    }
}

#[derive(Default)]
pub struct VideoInfo {
    pub width: i32,
    pub height: i32,
    pub fps: f64,
}

pub struct Automation {
    lua: Lua,
    script: Rc<RefCell<Script>>,
    macros: Rc<RefCell<Vec<MacroDef>>>,
    logs: Rc<RefCell<Vec<String>>>,
    video: Rc<RefCell<Option<VideoInfo>>>,
    metas: Rc<RefCell<Vec<ScriptMeta>>>,
    include_dirs: Rc<RefCell<Vec<String>>>,
}

pub struct MacroResult {
    pub selection: Vec<usize>,
    pub active: Option<usize>,
}

impl Automation {
    pub fn new(script: Script) -> LuaResult<Self> {
        let lua = Lua::new();
        let script = Rc::new(RefCell::new(script));
        let macros: Rc<RefCell<Vec<MacroDef>>> = Rc::default();
        let logs: Rc<RefCell<Vec<String>>> = Rc::default();
        let video: Rc<RefCell<Option<VideoInfo>>> = Rc::default();
        let metas: Rc<RefCell<Vec<ScriptMeta>>> = Rc::default();
        let include_dirs: Rc<RefCell<Vec<String>>> = Rc::default();
        let a = Automation { lua, script, macros, logs, video, metas, include_dirs };
        a.install()?;
        Ok(a)
    }

    pub fn script(&self) -> std::cell::Ref<'_, Script> {
        self.script.borrow()
    }
    pub fn take_script(&self) -> Script {
        self.script.borrow().clone()
    }
    pub fn logs(&self) -> Vec<String> {
        self.logs.borrow().clone()
    }
    pub fn set_video(&self, width: i32, height: i32, fps: f64) {
        *self.video.borrow_mut() = Some(VideoInfo { width, height, fps });
    }
    pub fn add_include_dir(&self, dir: &str) {
        self.include_dirs.borrow_mut().push(dir.to_string());
        let _ = self.sync_include_dirs();
    }
    /// Values returned by `aegisub.dialog.display` for named controls (overrides defaults).
    pub fn set_dialog_values(&self, values: &[(String, String)]) -> LuaResult<()> {
        let t = self.lua.create_table()?;
        for (k, v) in values {
            let val: LuaValue = if let Ok(n) = v.parse::<f64>() {
                LuaValue::Number(n)
            } else if v == "true" || v == "false" {
                LuaValue::Boolean(v == "true")
            } else {
                LuaValue::String(self.lua.create_string(v)?)
            };
            t.set(k.as_str(), val)?;
        }
        self.lua.globals().set("__osisub_dialog_values", t)
    }

    fn sync_include_dirs(&self) -> LuaResult<()> {
        let t = self.lua.create_table()?;
        for (i, d) in self.include_dirs.borrow().iter().enumerate() {
            t.set(i + 1, d.as_str())?;
        }
        self.lua.globals().set("__osisub_include_dirs", t)
    }

    fn install(&self) -> LuaResult<()> {
        let lua = &self.lua;
        let g = lua.globals();

        g.set("__osisub_log", {
            let logs = self.logs.clone();
            lua.create_function(move |_, s: String| {
                logs.borrow_mut().push(s);
                Ok(())
            })?
        })?;
        g.set("__osisub_embedded", lua.create_function(|lua, name: String| {
            let src = embedded(&name).ok_or_else(|| LuaError::external(format!("no embedded module {name}")))?;
            lua.load(src).set_name(format!("={name}")).eval::<LuaValue>()
        })?)?;
        g.set("__osisub_upper", lua.create_function(|_, s: String| Ok(s.to_uppercase()))?)?;
        g.set("__osisub_lower", lua.create_function(|_, s: String| Ok(s.to_lowercase()))?)?;

        let aegisub = lua.create_table()?;
        aegisub.set("dialog", lua.create_table()?)?;
        g.set("aegisub", aegisub.clone())?;

        // register_macro(name, description, process, validate, is_active)
        {
            let macros = self.macros.clone();
            let metas = self.metas.clone();
            aegisub.set("register_macro", lua.create_function(
                move |lua, (name, desc, process, validate, _active): (String, Option<String>, LuaFunction, Option<LuaFunction>, Option<LuaFunction>)| {
                    let script = lua.globals().get::<Option<String>>("script_name").ok().flatten().or_else(|| metas.borrow().last().map(|m| m.name.clone())).unwrap_or_default();
                    macros.borrow_mut().push(MacroDef {
                        info: MacroInfo { script, name, description: desc.unwrap_or_default(), is_filter: false },
                        process,
                        validate,
                    });
                    Ok(())
                })?)?;
        }
        // register_filter(name, description, priority, process, config)
        {
            let macros = self.macros.clone();
            let metas = self.metas.clone();
            aegisub.set("register_filter", lua.create_function(
                move |lua, (name, desc, _prio, process, _config): (String, Option<String>, Option<i64>, LuaFunction, Option<LuaFunction>)| {
                    let script = lua.globals().get::<Option<String>>("script_name").ok().flatten().or_else(|| metas.borrow().last().map(|m| m.name.clone())).unwrap_or_default();
                    macros.borrow_mut().push(MacroDef {
                        info: MacroInfo { script, name, description: desc.unwrap_or_default(), is_filter: true },
                        process,
                        validate: None,
                    });
                    Ok(())
                })?)?;
        }

        // video-related
        {
            let video = self.video.clone();
            aegisub.set("frame_from_ms", lua.create_function(move |_, ms: f64| {
                Ok(video.borrow().as_ref().map(|v| osisub_core::time::ms_to_frame(ms as i64, v.fps)))
            })?)?;
            let video = self.video.clone();
            aegisub.set("ms_from_frame", lua.create_function(move |_, f: i64| {
                Ok(video.borrow().as_ref().map(|v| osisub_core::time::frame_to_ms(f, v.fps)))
            })?)?;
            let video = self.video.clone();
            aegisub.set("video_size", lua.create_function(move |_, ()| {
                Ok(match video.borrow().as_ref() {
                    Some(v) => (Some(v.width), Some(v.height), Some(1.0f64), Some(1i32)),
                    None => (None, None, None, None),
                })
            })?)?;
            let video = self.video.clone();
            aegisub.set("project_properties", lua.create_function(move |lua, ()| {
                let t = lua.create_table()?;
                for k in ["audio_file", "video_file", "timecodes_file", "keyframes_file"] {
                    t.set(k, "")?;
                }
                t.set("video_position", 0)?;
                t.set("ar_mode", 0)?;
                t.set("ar_value", 0)?;
                t.set("video_zoom", 1.0)?;
                if let Some(v) = video.borrow().as_ref() {
                    t.set("video_width", v.width)?;
                    t.set("video_height", v.height)?;
                }
                Ok(t)
            })?)?;
        }

        // Approximate text metrics (no font access yet): returns width, height, descent, external leading.
        aegisub.set("text_extents", lua.create_function(|_, (style, text): (LuaTable, String)| {
            let size: f64 = style.get("fontsize").unwrap_or(40.0);
            let sx: f64 = style.get("scale_x").unwrap_or(100.0);
            let sy: f64 = style.get("scale_y").unwrap_or(100.0);
            let spacing: f64 = style.get("spacing").unwrap_or(0.0);
            let bold: bool = style.get("bold").unwrap_or(false);
            Ok(approx_extents(&text, size, sx, sy, spacing, bold))
        })?)?;

        lua.load(PRELUDE).set_name("=prelude.lua").exec()?;
        self.sync_include_dirs()?;
        Ok(())
    }

    /// Load and execute an automation script file; registered macros become available.
    pub fn load_file(&self, path: &str) -> Result<ScriptMeta, RunError> {
        let src = std::fs::read(path).map_err(|e| RunError::Lua(format!("{path}: {e}")))?;
        let dir = std::path::Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let dir = if dir.is_empty() { ".".to_string() } else { dir };
        self.include_dirs.borrow_mut().insert(0, dir.clone());
        self.sync_include_dirs().map_err(map_err)?;
        let g = self.lua.globals();
        // package.path so that plain `require "foo"` finds neighbours of the script
        let pkg: LuaTable = g.get("package").map_err(map_err)?;
        let old: String = pkg.get("path").unwrap_or_default();
        pkg.set("path", format!("{dir}/?.lua;{dir}/?/init.lua;{old}")).map_err(map_err)?;
        for k in ["script_name", "script_description", "script_author", "script_version"] {
            g.set(k, LuaValue::Nil).map_err(map_err)?;
        }
        g.set("__osisub_script_file", path).map_err(map_err)?;
        let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.metas.borrow_mut().push(ScriptMeta { file: path.into(), name: stem.clone(), ..Default::default() });
        let chunk = self.lua.load(&src[..]).set_name(format!("@{path}"));
        let res = chunk.exec();
        let get = |k: &str| g.get::<Option<String>>(k).ok().flatten().unwrap_or_default();
        let meta = ScriptMeta {
            file: path.into(),
            name: if get("script_name").is_empty() { stem } else { get("script_name") },
            description: get("script_description"),
            author: get("script_author"),
            version: get("script_version"),
        };
        *self.metas.borrow_mut().last_mut().unwrap() = meta.clone();
        res.map_err(map_err)?;
        Ok(meta)
    }

    pub fn list_macros(&self) -> Vec<MacroInfo> {
        self.macros.borrow().iter().map(|m| m.info.clone()).collect()
    }

    /// Run a macro/filter by name. `selection` and `active` are 1-based indices into the entry list (like Aegisub).
    /// On error or cancellation the script is restored to its prior state.
    pub fn run(&self, name: &str, selection: &[usize], active: usize) -> Result<MacroResult, RunError> {
        let (process, validate, is_filter) = {
            let ms = self.macros.borrow();
            let m = ms.iter().find(|m| m.info.name == name).ok_or_else(|| RunError::NotFound(name.into()))?;
            (m.process.clone(), m.validate.clone(), m.info.is_filter)
        };
        let subs = self.lua.create_userdata(Subs(self.script.clone())).map_err(map_err)?;
        let sel = self.lua.create_sequence_from(selection.iter().copied()).map_err(map_err)?;
        if let Some(v) = validate {
            let ok: LuaValue = v.call((subs.clone(), sel.clone(), active)).map_err(map_err)?;
            if matches!(ok, LuaValue::Nil | LuaValue::Boolean(false)) {
                return Err(RunError::Invalid);
            }
        }
        let backup = self.script.borrow().clone();
        let ret: LuaResult<LuaMultiValue> = if is_filter {
            let cfg = self.lua.create_table().map_err(map_err)?;
            process.call((subs, cfg))
        } else {
            process.call((subs, sel, active))
        };
        match ret {
            Ok(vals) => {
                let mut v = vals.into_iter();
                let selection = match v.next() {
                    Some(LuaValue::Table(t)) => t.sequence_values::<usize>().filter_map(|x| x.ok()).collect(),
                    _ => selection.to_vec(),
                };
                let active = match v.next() {
                    Some(LuaValue::Integer(i)) => Some(i as usize),
                    Some(LuaValue::Number(n)) => Some(n as usize),
                    _ => None,
                };
                Ok(MacroResult { selection, active })
            }
            Err(e) => {
                *self.script.borrow_mut() = backup;
                Err(map_err(e))
            }
        }
    }
}

fn approx_extents(text: &str, size: f64, sx: f64, sy: f64, spacing: f64, bold: bool) -> (f64, f64, f64, f64) {
    let mut w = 0.0;
    for c in text.chars() {
        let f = if c.is_whitespace() {
            0.28
        } else if "il.,:;!'|ıİ".contains(c) {
            0.28
        } else if "mwMW@".contains(c) {
            0.85
        } else if c.is_ascii_uppercase() {
            0.68
        } else if (c as u32) >= 0x2E80 {
            1.0
        } else {
            0.52
        };
        w += size * f + spacing;
    }
    if bold {
        w *= 1.05;
    }
    (w * sx / 100.0, size * 1.15 * sy / 100.0, size * 0.2 * sy / 100.0, 0.0)
}

// ---- the `subs` userdata ---------------------------------------------------

struct Subs(Rc<RefCell<Script>>);

fn entry_to_table(lua: &Lua, e: &Entry) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("section", e.section())?;
    match e {
        Entry::Info { key, value } => {
            t.set("class", "info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            t.set("raw", format!("{key}: {value}"))?;
        }
        Entry::Style(s) => {
            t.set("class", "style")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", s.color1.as_str())?;
            t.set("color2", s.color2.as_str())?;
            t.set("color3", s.color3.as_str())?;
            t.set("color4", s.color4.as_str())?;
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
            t.set("margin_t", s.margin_v)?;
            t.set("margin_b", s.margin_v)?;
            t.set("margin_v", s.margin_v)?;
            t.set("encoding", s.encoding)?;
            t.set("relative_to", 2)?;
            t.set("raw", format!("Style: {}", s.name))?;
        }
        Entry::Dialogue(d) => {
            t.set("class", "dialogue")?;
            t.set("comment", d.comment)?;
            t.set("layer", d.layer)?;
            t.set("start_time", d.start)?;
            t.set("end_time", d.end)?;
            t.set("style", d.style.as_str())?;
            t.set("actor", d.actor.as_str())?;
            t.set("margin_l", d.margin_l)?;
            t.set("margin_r", d.margin_r)?;
            t.set("margin_t", d.margin_v)?;
            t.set("margin_b", d.margin_v)?;
            t.set("margin_v", d.margin_v)?;
            t.set("effect", d.effect.as_str())?;
            t.set("text", d.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", event_line(d))?;
        }
        Entry::Raw { section, text } => {
            t.set("class", "unknown")?;
            t.set("section", section.as_str())?;
            t.set("raw", text.as_str())?;
        }
    }
    Ok(t)
}

fn gs(t: &LuaTable, k: &str, d: &str) -> LuaResult<String> {
    Ok(t.get::<Option<String>>(k)?.unwrap_or_else(|| d.to_string()))
}
fn gf(t: &LuaTable, k: &str, d: f64) -> LuaResult<f64> {
    Ok(t.get::<Option<f64>>(k)?.unwrap_or(d))
}
fn gi(t: &LuaTable, k: &str, d: i32) -> LuaResult<i32> {
    Ok(t.get::<Option<f64>>(k)?.map(|v| v as i32).unwrap_or(d))
}
fn gb(t: &LuaTable, k: &str, d: bool) -> LuaResult<bool> {
    Ok(t.get::<Option<bool>>(k)?.unwrap_or(d))
}

/// Resolve the vertical margin when scripts may have set `margin_v` or `margin_t`/`margin_b`.
fn margin_v(t: &LuaTable, orig: Option<i32>) -> LuaResult<i32> {
    let mt = t.get::<Option<f64>>("margin_t")?.map(|v| v as i32);
    let mv = t.get::<Option<f64>>("margin_v")?.map(|v| v as i32);
    Ok(match (mt, mv, orig) {
        (Some(a), Some(b), Some(o)) => if a != o { a } else { b },
        (Some(a), Some(_), None) => a,
        (Some(a), None, _) => a,
        (None, Some(b), _) => b,
        (None, None, o) => o.unwrap_or(0),
    })
}

fn table_to_entry(t: &LuaTable, orig: Option<&Entry>) -> LuaResult<Entry> {
    let class = gs(t, "class", "")?;
    match class.as_str() {
        "info" => Ok(Entry::Info { key: gs(t, "key", "")?, value: gs(t, "value", "")? }),
        "style" => {
            let om = match orig { Some(Entry::Style(s)) => Some(s.margin_v), _ => None };
            let d = Style::default();
            Ok(Entry::Style(Style {
                name: gs(t, "name", &d.name)?,
                fontname: gs(t, "fontname", &d.fontname)?,
                fontsize: gf(t, "fontsize", d.fontsize)?,
                color1: norm_color(&gs(t, "color1", &d.color1)?),
                color2: norm_color(&gs(t, "color2", &d.color2)?),
                color3: norm_color(&gs(t, "color3", &d.color3)?),
                color4: norm_color(&gs(t, "color4", &d.color4)?),
                bold: gb(t, "bold", false)?,
                italic: gb(t, "italic", false)?,
                underline: gb(t, "underline", false)?,
                strikeout: gb(t, "strikeout", false)?,
                scale_x: gf(t, "scale_x", 100.0)?,
                scale_y: gf(t, "scale_y", 100.0)?,
                spacing: gf(t, "spacing", 0.0)?,
                angle: gf(t, "angle", 0.0)?,
                borderstyle: gi(t, "borderstyle", 1)?,
                outline: gf(t, "outline", 2.0)?,
                shadow: gf(t, "shadow", 2.0)?,
                align: gi(t, "align", 2)?,
                margin_l: gi(t, "margin_l", 10)?,
                margin_r: gi(t, "margin_r", 10)?,
                margin_v: margin_v(t, om)?,
                encoding: gi(t, "encoding", 1)?,
            }))
        }
        "dialogue" => {
            let om = match orig { Some(Entry::Dialogue(d)) => Some(d.margin_v), _ => None };
            Ok(Entry::Dialogue(Event {
                comment: gb(t, "comment", false)?,
                layer: gi(t, "layer", 0)?,
                start: gf(t, "start_time", 0.0)?.round() as i64,
                end: gf(t, "end_time", 0.0)?.round() as i64,
                style: gs(t, "style", "Default")?,
                actor: gs(t, "actor", "")?,
                margin_l: gi(t, "margin_l", 0)?,
                margin_r: gi(t, "margin_r", 0)?,
                margin_v: margin_v(t, om)?,
                effect: gs(t, "effect", "")?,
                text: gs(t, "text", "")?,
            }))
        }
        "unknown" => Ok(Entry::Raw { section: gs(t, "section", "")?, text: gs(t, "raw", "")? }),
        other => Err(LuaError::external(format!("invalid line class '{other}'"))),
    }
}

impl LuaUserData for Subs {
    fn add_methods<M: LuaUserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(LuaMetaMethod::Len, |_, this, ()| Ok(this.0.borrow().entries.len()));
        m.add_meta_method(LuaMetaMethod::Index, |lua, this, key: LuaValue| {
            match key {
                LuaValue::Integer(_) | LuaValue::Number(_) => {
                    let i = match key {
                        LuaValue::Integer(i) => i as f64,
                        LuaValue::Number(n) => n,
                        _ => unreachable!(),
                    };
                    let sc = this.0.borrow();
                    if i < 1.0 || i as usize > sc.entries.len() {
                        return Err(LuaError::external(format!("Requested out-of-range line from subtitle file: {i}")));
                    }
                    Ok(LuaValue::Table(entry_to_table(lua, &sc.entries[i as usize - 1])?))
                }
                LuaValue::String(s) => {
                    let s = s.to_str()?.to_string();
                    let sc = this.0.clone();
                    let f: LuaFunction = match s.as_str() {
                        "n" => return Ok(LuaValue::Integer(sc.borrow().entries.len() as i64)),
                        "insert" => lua.create_function(move |_, args: LuaVariadic<LuaValue>| {
                            let mut it = args.into_iter();
                            let idx = it.next().and_then(|v| v.as_integer()).ok_or_else(|| LuaError::external("insert: bad index"))?;
                            let mut sc = sc.borrow_mut();
                            let mut at = (idx.max(1) as usize - 1).min(sc.entries.len());
                            for v in it {
                                if let LuaValue::Table(t) = v {
                                    sc.entries.insert(at, table_to_entry(&t, None)?);
                                    at += 1;
                                }
                            }
                            Ok(())
                        })?,
                        "append" => lua.create_function(move |_, args: LuaVariadic<LuaTable>| {
                            let mut sc = sc.borrow_mut();
                            for t in args.iter() {
                                sc.entries.push(table_to_entry(t, None)?);
                            }
                            Ok(())
                        })?,
                        "delete" => lua.create_function(move |_, args: LuaVariadic<usize>| {
                            let mut idx: Vec<usize> = args.into_iter().collect();
                            idx.sort_unstable_by(|a, b| b.cmp(a));
                            idx.dedup();
                            let mut sc = sc.borrow_mut();
                            for i in idx {
                                if i >= 1 && i <= sc.entries.len() {
                                    sc.entries.remove(i - 1);
                                }
                            }
                            Ok(())
                        })?,
                        "deleterange" => lua.create_function(move |_, (a, b): (usize, usize)| {
                            let mut sc = sc.borrow_mut();
                            let (a, b) = (a.max(1), b.min(sc.entries.len()));
                            if a <= b {
                                sc.entries.drain(a - 1..b);
                            }
                            Ok(())
                        })?,
                        _ => return Ok(LuaValue::Nil),
                    };
                    Ok(LuaValue::Function(f))
                }
                _ => Ok(LuaValue::Nil),
            }
        });
        m.add_meta_method(LuaMetaMethod::NewIndex, |_, this, (i, t): (usize, LuaTable)| {
            let mut sc = this.0.borrow_mut();
            if i < 1 || i > sc.entries.len() {
                return Err(LuaError::external(format!("Attempt to write out-of-range line to subtitle file: {i}")));
            }
            let e = table_to_entry(&t, Some(&sc.entries[i - 1]))?;
            sc.entries[i - 1] = e;
            Ok(())
        });
    }
}
