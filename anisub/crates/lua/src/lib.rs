//! Automation 4 (Aegisub Lua) compatibility layer.
//!
//! A [`Runtime`] owns a Lua state with the `aegisub` API, the `subs` object backed by a shared
//! [`AssFile`], and bundled copies of the standard include libraries (`utils.lua`,
//! `karaskel.lua`, `unicode.lua`, ...).

use anisub_core::ass::{self, AssFile, Event, Line, Style};
use anisub_core::color;
use mlua::{Lua, MetaMethod, MultiValue, Table, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type Shared = Rc<RefCell<AssFile>>;
pub type Result<T> = mlua::Result<T>;

const PRELUDE: &str = include_str!("../lua/prelude.lua");
const LIBS: &[(&str, &str)] = &[
    ("utils.lua", include_str!("../lua/utils.lua")),
    ("unicode.lua", include_str!("../lua/unicode.lua")),
    ("karaskel.lua", include_str!("../lua/karaskel.lua")),
    ("cleantags.lua", include_str!("../lua/cleantags.lua")),
];

struct Subs(Shared);

fn color_str(c: u32) -> String {
    color::format(c)
}

fn line_to_table(lua: &Lua, l: &Line) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("section", l.section())?;
    match l {
        Line::Info { key, value } => {
            t.set("class", "info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            t.set("raw", format!("{key}: {value}"))?;
        }
        Line::Style(s) => {
            t.set("class", "style")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", color_str(s.primary))?;
            t.set("color2", color_str(s.secondary))?;
            t.set("color3", color_str(s.outline))?;
            t.set("color4", color_str(s.back))?;
            t.set("bold", s.bold)?;
            t.set("italic", s.italic)?;
            t.set("underline", s.underline)?;
            t.set("strikeout", s.strikeout)?;
            t.set("scale_x", s.scale_x)?;
            t.set("scale_y", s.scale_y)?;
            t.set("spacing", s.spacing)?;
            t.set("angle", s.angle)?;
            t.set("borderstyle", s.border_style)?;
            t.set("outline", s.outline_w)?;
            t.set("shadow", s.shadow)?;
            t.set("align", s.align)?;
            t.set("margin_l", s.margin_l)?;
            t.set("margin_r", s.margin_r)?;
            t.set("margin_t", s.margin_t)?;
            t.set("margin_b", s.margin_t)?;
            t.set("encoding", s.encoding)?;
            t.set("relative_to", 2)?;
            t.set("raw", format!("Style: {}", ass::write_style(s)))?;
        }
        Line::Dialogue(e) => {
            t.set("class", "dialogue")?;
            t.set("comment", e.comment)?;
            t.set("layer", e.layer)?;
            t.set("start_time", e.start)?;
            t.set("end_time", e.end)?;
            t.set("style", e.style.as_str())?;
            t.set("actor", e.actor.as_str())?;
            t.set("margin_l", e.margin_l)?;
            t.set("margin_r", e.margin_r)?;
            t.set("margin_t", e.margin_t)?;
            t.set("margin_b", e.margin_t)?;
            t.set("effect", e.effect.as_str())?;
            t.set("text", e.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", ass::write_event(e).trim_end_matches('\n'))?;
        }
        Line::Raw { text, .. } => {
            t.set("class", "unknown")?;
            t.set("raw", text.as_str())?;
        }
    }
    Ok(t)
}

fn get_str(t: &Table, k: &str, d: &str) -> Result<String> {
    Ok(t.get::<Option<String>>(k)?.unwrap_or_else(|| d.to_string()))
}
fn get_f(t: &Table, k: &str, d: f64) -> Result<f64> {
    Ok(t.get::<Option<f64>>(k)?.unwrap_or(d))
}
fn get_i(t: &Table, k: &str, d: i64) -> Result<i64> {
    Ok(t.get::<Option<f64>>(k)?.map(|f| f.round() as i64).unwrap_or(d))
}
fn get_b(t: &Table, k: &str) -> Result<bool> {
    Ok(t.get::<Option<bool>>(k)?.unwrap_or(false))
}
fn get_col(t: &Table, k: &str, d: u32) -> Result<u32> {
    Ok(t.get::<Option<String>>(k)?.and_then(|s| color::parse(&s)).unwrap_or(d))
}

fn table_to_line(t: &Table) -> Result<Line> {
    let class = get_str(t, "class", "dialogue")?;
    Ok(match class.as_str() {
        "info" => Line::Info { key: get_str(t, "key", "")?, value: get_str(t, "value", "")? },
        "style" => {
            let d = Style::default();
            Line::Style(Style {
                name: get_str(t, "name", &d.name)?,
                fontname: get_str(t, "fontname", &d.fontname)?,
                fontsize: get_f(t, "fontsize", d.fontsize)?,
                primary: get_col(t, "color1", d.primary)?,
                secondary: get_col(t, "color2", d.secondary)?,
                outline: get_col(t, "color3", d.outline)?,
                back: get_col(t, "color4", d.back)?,
                bold: get_b(t, "bold")?,
                italic: get_b(t, "italic")?,
                underline: get_b(t, "underline")?,
                strikeout: get_b(t, "strikeout")?,
                scale_x: get_f(t, "scale_x", 100.0)?,
                scale_y: get_f(t, "scale_y", 100.0)?,
                spacing: get_f(t, "spacing", 0.0)?,
                angle: get_f(t, "angle", 0.0)?,
                border_style: get_i(t, "borderstyle", 1)? as i32,
                outline_w: get_f(t, "outline", 2.0)?,
                shadow: get_f(t, "shadow", 2.0)?,
                align: get_i(t, "align", 2)? as i32,
                margin_l: get_i(t, "margin_l", 10)? as i32,
                margin_r: get_i(t, "margin_r", 10)? as i32,
                margin_t: get_i(t, "margin_t", get_i(t, "margin_b", 10)?)? as i32,
                encoding: get_i(t, "encoding", 1)? as i32,
            })
        }
        "dialogue" => Line::Dialogue(Event {
            comment: get_b(t, "comment")?,
            layer: get_i(t, "layer", 0)? as i32,
            start: get_i(t, "start_time", 0)?,
            end: get_i(t, "end_time", 0)?,
            style: get_str(t, "style", "Default")?,
            actor: get_str(t, "actor", "")?,
            margin_l: get_i(t, "margin_l", 0)? as i32,
            margin_r: get_i(t, "margin_r", 0)? as i32,
            margin_t: get_i(t, "margin_t", get_i(t, "margin_b", 0)?)? as i32,
            effect: get_str(t, "effect", "")?,
            text: get_str(t, "text", "")?,
        }),
        other => {
            return Err(mlua::Error::runtime(format!("unsupported line class '{other}'")));
        }
    })
}

fn idx(i: i64, len: usize) -> Result<usize> {
    if i < 1 || i as usize > len {
        Err(mlua::Error::runtime(format!("subtitle index {i} out of range 1..{len}")))
    } else {
        Ok(i as usize - 1)
    }
}

fn lines_from(args: MultiValue) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    for v in args {
        match v {
            Value::Table(t) => out.push(table_to_line(&t)?),
            Value::Nil => {}
            _ => return Err(mlua::Error::runtime("expected line table")),
        }
    }
    Ok(out)
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().lines.len()));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| match key {
            Value::Integer(_) | Value::Number(_) => {
                let i = match key {
                    Value::Integer(i) => i,
                    Value::Number(n) => n as i64,
                    _ => unreachable!(),
                };
                let f = this.0.borrow();
                let l = &f.lines[idx(i, f.lines.len())?];
                Ok(Value::Table(line_to_table(lua, l)?))
            }
            Value::String(s) => {
                let s = s.to_str()?;
                let f = this.0.clone();
                match &*s {
                    "n" => Ok(Value::Integer(f.borrow().lines.len() as i64)),
                    "insert" => Ok(Value::Function(lua.create_function(
                        move |_, (_this, at, rest): (Value, i64, MultiValue)| {
                            let new = lines_from(rest)?;
                            let mut file = f.borrow_mut();
                            let len = file.lines.len();
                            let at = (at.max(1) as usize - 1).min(len);
                            for (k, l) in new.into_iter().enumerate() {
                                file.lines.insert(at + k, l);
                            }
                            Ok(())
                        },
                    )?)),
                    "append" => Ok(Value::Function(lua.create_function(
                        move |_, (_this, rest): (Value, MultiValue)| {
                            f.borrow_mut().lines.extend(lines_from(rest)?);
                            Ok(())
                        },
                    )?)),
                    "delete" => Ok(Value::Function(lua.create_function(
                        move |_, (_this, rest): (Value, MultiValue)| {
                            let mut ids: Vec<i64> = Vec::new();
                            for v in rest {
                                match v {
                                    Value::Integer(i) => ids.push(i),
                                    Value::Number(n) => ids.push(n as i64),
                                    Value::Table(t) => {
                                        for p in t.sequence_values::<i64>() {
                                            ids.push(p?);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            ids.sort_unstable();
                            ids.dedup();
                            let mut file = f.borrow_mut();
                            for &i in ids.iter().rev() {
                                let k = idx(i, file.lines.len())?;
                                file.lines.remove(k);
                            }
                            Ok(())
                        },
                    )?)),
                    "deleterange" => Ok(Value::Function(lua.create_function(
                        move |_, (_this, a, b): (Value, i64, i64)| {
                            let mut file = f.borrow_mut();
                            let len = file.lines.len();
                            let (a, b) = (idx(a, len)?, idx(b, len)?);
                            file.lines.drain(a..=b.max(a));
                            Ok(())
                        },
                    )?)),
                    _ => Ok(Value::Nil),
                }
            }
            _ => Ok(Value::Nil),
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (i, v): (i64, Value)| {
            let mut f = this.0.borrow_mut();
            let len = f.lines.len();
            match v {
                Value::Nil => {
                    let k = idx(i, len)?;
                    f.lines.remove(k);
                }
                Value::Table(t) => {
                    let l = table_to_line(&t)?;
                    if i as usize == len + 1 {
                        f.lines.push(l);
                    } else {
                        let k = idx(i, len)?;
                        f.lines[k] = l;
                    }
                }
                _ => return Err(mlua::Error::runtime("expected line table")),
            }
            Ok(())
        });
    }
}

#[derive(Debug, Clone)]
pub struct MacroInfo {
    pub name: String,
    pub description: String,
    pub kind: &'static str,
}

#[derive(Debug, Default)]
pub struct RunResult {
    pub selection: Option<Vec<i64>>,
    pub active: Option<i64>,
    pub cancelled: bool,
}

pub struct Runtime {
    pub lua: Lua,
    pub subs: Shared,
    include_dirs: Rc<RefCell<Vec<PathBuf>>>,
}

impl Runtime {
    pub fn new(file: AssFile) -> Result<Runtime> {
        let lua = Lua::new();
        let subs: Shared = Rc::new(RefCell::new(file));
        let include_dirs: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
        lua.load(PRELUDE).set_name("prelude.lua").exec()?;

        let g = lua.globals();
        let dirs = include_dirs.clone();
        g.set(
            "include",
            lua.create_function(move |lua, name: String| {
                for d in dirs.borrow().iter() {
                    let p = d.join(&name);
                    if p.is_file() {
                        let src = std::fs::read(&p).map_err(mlua::Error::external)?;
                        return lua.load(&anisub_core::encoding::decode(&src)).set_name(format!("@{}", p.display())).exec();
                    }
                }
                if let Some((_, src)) = LIBS.iter().find(|(n, _)| *n == name) {
                    return lua.load(*src).set_name(format!("@{name}")).exec();
                }
                Err(mlua::Error::runtime(format!("Lua include not found: {name}")))
            })?,
        )?;

        let rt = Runtime { lua, subs, include_dirs };
        Ok(rt)
    }

    pub fn add_include_dir(&self, dir: impl Into<PathBuf>) {
        self.include_dirs.borrow_mut().push(dir.into());
    }

    pub fn set_fps(&self, fps: f64) -> Result<()> {
        self.lua.globals().get::<Table>("aegisub")?.set("__fps", fps)
    }

    /// Load an automation script (registering its macros) from disk.
    pub fn load_script(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            self.add_include_dir(dir);
        }
        let src = std::fs::read(path).map_err(mlua::Error::external)?;
        let code = anisub_core::encoding::decode(&src);
        self.load_source(&code, &path.display().to_string())
    }

    pub fn load_source(&self, code: &str, name: &str) -> Result<()> {
        self.lua.load(code).set_name(format!("@{name}")).exec()
    }

    pub fn macros(&self) -> Result<Vec<MacroInfo>> {
        let a: Table = self.lua.globals().get("aegisub")?;
        let mut out = Vec::new();
        for (key, kind) in [("__macros", "macro"), ("__filters", "filter")] {
            let t: Table = a.get(key)?;
            for e in t.sequence_values::<Table>() {
                let e = e?;
                out.push(MacroInfo { name: e.get("name")?, description: e.get("description")?, kind });
            }
        }
        Ok(out)
    }

    /// Run a macro by (1-based) position in [`Runtime::macros`] order, or by name.
    pub fn run(&self, name: &str, selected: Vec<i64>, active: i64) -> Result<RunResult> {
        let a: Table = self.lua.globals().get("aegisub")?;
        let runner: mlua::Function = a.get("__run")?;
        let sel = self.lua.create_sequence_from(selected)?;
        let subs = self.lua.create_userdata(Subs(self.subs.clone()))?;
        let r: Table = runner.call((name, subs, sel, active))?;
        let selection = r.get::<Option<Table>>("selection")?.map(|t| {
            t.sequence_values::<i64>().filter_map(|v| v.ok()).collect()
        });
        Ok(RunResult {
            selection,
            active: r.get("active")?,
            cancelled: r.get::<Option<bool>>("cancelled")?.unwrap_or(false),
        })
    }

    /// Captured `aegisub.log` output since the last call.
    pub fn take_log(&self) -> Result<String> {
        let a: Table = self.lua.globals().get("aegisub")?;
        let f: mlua::Function = a.get("__take_log")?;
        f.call(())
    }
}
