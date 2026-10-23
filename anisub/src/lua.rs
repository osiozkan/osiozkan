//! Aegisub-compatible Lua automation host.
//!
//! Loads an automation script (macros and export filters), exposes the subtitle
//! file as `subs` with Aegisub's API, and runs the selected macro.

use crate::ass::{fmt_num, Dialogue, Document, Entry, Style};
use anyhow::{anyhow, Result};
use mlua::{Function, Lua, MetaMethod, MultiValue, Table, UserData, UserDataMethods, Value};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

const AEGISUB_LUA: &str = include_str!("../lua/aegisub.lua");
const UTILS_LUA: &str = include_str!("../lua/utils.lua");
const KARASKEL_LUA: &str = include_str!("../lua/karaskel.lua");

#[derive(Clone)]
struct Subs(Rc<RefCell<Vec<Entry>>>);

fn style_to_table(lua: &Lua, s: &Style) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("class", "style")?;
    t.set("section", "V4+ Styles")?;
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
    t.set("margin_t", s.margin_t)?;
    t.set("margin_b", s.margin_b)?;
    t.set("encoding", s.encoding)?;
    Ok(t)
}

fn entry_to_table(lua: &Lua, e: &Entry) -> mlua::Result<Table> {
    match e {
        Entry::Style(s) => style_to_table(lua, s),
        Entry::Info { key, value } => {
            let t = lua.create_table()?;
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            Ok(t)
        }
        Entry::Dialogue(d) => {
            let t = lua.create_table()?;
            t.set("class", "dialogue")?;
            t.set("section", "Events")?;
            t.set("comment", d.comment)?;
            t.set("layer", d.layer)?;
            t.set("start_time", d.start_time)?;
            t.set("end_time", d.end_time)?;
            t.set("style", d.style.as_str())?;
            t.set("actor", d.actor.as_str())?;
            t.set("margin_l", d.margin_l)?;
            t.set("margin_r", d.margin_r)?;
            t.set("margin_t", d.margin_t)?;
            t.set("margin_b", d.margin_b)?;
            t.set("effect", d.effect.as_str())?;
            t.set("text", d.text.as_str())?;
            t.set("raw", crate::ass::Document { entries: vec![e.clone()] }.to_string().lines().last().unwrap_or(""))?;
            Ok(t)
        }
        Entry::Raw { section, text } => {
            let t = lua.create_table()?;
            t.set("class", "unknown")?;
            t.set("section", section.as_str())?;
            t.set("raw", text.as_str())?;
            Ok(t)
        }
    }
}

fn s(t: &Table, k: &str) -> String {
    t.get::<Option<String>>(k).ok().flatten().unwrap_or_default()
}
fn n(t: &Table, k: &str) -> f64 {
    match t.get::<Value>(k) {
        Ok(Value::Number(x)) => x,
        Ok(Value::Integer(x)) => x as f64,
        Ok(Value::String(x)) => x.to_str().ok().and_then(|v| v.parse().ok()).unwrap_or(0.0),
        _ => 0.0,
    }
}
fn b(t: &Table, k: &str) -> bool {
    matches!(t.get::<Value>(k), Ok(Value::Boolean(true)))
}

fn table_to_entry(t: &Table) -> mlua::Result<Entry> {
    let class = s(t, "class");
    Ok(match class.as_str() {
        "dialogue" => Entry::Dialogue(Dialogue {
            comment: b(t, "comment"),
            layer: n(t, "layer") as i64,
            start_time: n(t, "start_time").round() as i64,
            end_time: n(t, "end_time").round() as i64,
            style: s(t, "style"),
            actor: s(t, "actor"),
            margin_l: n(t, "margin_l") as i64,
            margin_r: n(t, "margin_r") as i64,
            margin_t: n(t, "margin_t") as i64,
            margin_b: n(t, "margin_b") as i64,
            effect: s(t, "effect"),
            text: s(t, "text"),
        }),
        "style" => Entry::Style(Style {
            name: s(t, "name"),
            fontname: s(t, "fontname"),
            fontsize: n(t, "fontsize"),
            color1: s(t, "color1"),
            color2: s(t, "color2"),
            color3: s(t, "color3"),
            color4: s(t, "color4"),
            bold: b(t, "bold"),
            italic: b(t, "italic"),
            underline: b(t, "underline"),
            strikeout: b(t, "strikeout"),
            scale_x: n(t, "scale_x"),
            scale_y: n(t, "scale_y"),
            spacing: n(t, "spacing"),
            angle: n(t, "angle"),
            borderstyle: n(t, "borderstyle") as i64,
            outline: n(t, "outline"),
            shadow: n(t, "shadow"),
            align: n(t, "align") as i64,
            margin_l: n(t, "margin_l") as i64,
            margin_r: n(t, "margin_r") as i64,
            margin_t: n(t, "margin_t") as i64,
            margin_b: n(t, "margin_b") as i64,
            encoding: n(t, "encoding") as i64,
        }),
        "info" => Entry::Info { key: s(t, "key"), value: s(t, "value") },
        _ => Entry::Raw { section: s(t, "section"), text: s(t, "raw") },
    })
}

fn skip_self(args: MultiValue) -> Vec<Value> {
    let mut v: Vec<Value> = args.into_iter().collect();
    if matches!(v.first(), Some(Value::UserData(_))) {
        v.remove(0);
    }
    v
}

fn idx(v: &Value) -> mlua::Result<usize> {
    match v {
        Value::Integer(i) => Ok(*i as usize),
        Value::Number(x) => Ok(*x as usize),
        _ => Err(mlua::Error::runtime("expected line index")),
    }
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().len()));
        m.add_meta_method(MetaMethod::Index, |lua, this, key: Value| -> mlua::Result<Value> {
            match key {
                Value::Integer(_) | Value::Number(_) => {
                    let i = idx(&key)?;
                    let v = this.0.borrow();
                    if i == 0 || i > v.len() {
                        return Err(mlua::Error::runtime(format!("Requested out-of-range line from subtitle file: {}", i)));
                    }
                    Ok(Value::Table(entry_to_table(lua, &v[i - 1])?))
                }
                Value::String(k) => {
                    let name = k.to_str()?.to_string();
                    let rc = this.0.clone();
                    let f: Function = match name.as_str() {
                        "insert" => lua.create_function(move |_, args: MultiValue| {
                            let a = skip_self(args);
                            let at = idx(a.first().ok_or_else(|| mlua::Error::runtime("insert: missing index"))?)?;
                            let mut v = rc.borrow_mut();
                            let mut pos = at.saturating_sub(1).min(v.len());
                            for l in &a[1..] {
                                if let Value::Table(t) = l {
                                    v.insert(pos, table_to_entry(t)?);
                                    pos += 1;
                                }
                            }
                            Ok(())
                        })?,
                        "append" => lua.create_function(move |_, args: MultiValue| {
                            let a = skip_self(args);
                            let mut v = rc.borrow_mut();
                            for l in &a {
                                if let Value::Table(t) = l {
                                    v.push(table_to_entry(t)?);
                                }
                            }
                            Ok(())
                        })?,
                        "delete" => lua.create_function(move |_, args: MultiValue| {
                            let a = skip_self(args);
                            let mut ids: Vec<usize> = Vec::new();
                            for x in &a {
                                match x {
                                    Value::Table(t) => {
                                        for p in t.clone().sequence_values::<i64>() {
                                            ids.push(p? as usize);
                                        }
                                    }
                                    other => ids.push(idx(other)?),
                                }
                            }
                            ids.sort_unstable_by(|a, b| b.cmp(a));
                            ids.dedup();
                            let mut v = rc.borrow_mut();
                            for i in ids {
                                if i >= 1 && i <= v.len() {
                                    v.remove(i - 1);
                                }
                            }
                            Ok(())
                        })?,
                        "deleterange" => lua.create_function(move |_, args: MultiValue| {
                            let a = skip_self(args);
                            let from = idx(&a[0])?;
                            let to = idx(a.get(1).unwrap_or(&a[0]))?;
                            let mut v = rc.borrow_mut();
                            let (from, to) = (from.max(1), to.min(v.len()));
                            if from <= to {
                                v.drain(from - 1..to);
                            }
                            Ok(())
                        })?,
                        "n" => return Ok(Value::Integer(this.0.borrow().len() as i64)),
                        _ => return Ok(Value::Nil),
                    };
                    Ok(Value::Function(f))
                }
                _ => Ok(Value::Nil),
            }
        });
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            let i = idx(&key)?;
            let mut v = this.0.borrow_mut();
            if i == 0 || i > v.len() {
                return Err(mlua::Error::runtime("Attempt to write out-of-range line to subtitle file"));
            }
            match val {
                Value::Table(t) => v[i - 1] = table_to_entry(&t)?,
                Value::Nil => {
                    v.remove(i - 1);
                }
                _ => return Err(mlua::Error::runtime("line must be a table")),
            }
            Ok(())
        });
    }
}

trait L<T> {
    fn l(self) -> Result<T>;
}
impl<T> L<T> for mlua::Result<T> {
    fn l(self) -> Result<T> {
        self.map_err(|e| anyhow!("{}", e))
    }
}

pub struct Macro {
    pub name: String,
    pub description: String,
    pub kind: &'static str,
}

pub struct Host {
    lua: Lua,
    entries: Rc<RefCell<Vec<Entry>>>,
    pub log: Rc<RefCell<Vec<String>>>,
    #[allow(dead_code)]
    fps: Rc<Cell<f64>>,
    #[allow(dead_code)]
    script_dir: PathBuf,
}

/// Rough text metrics (no font rendering yet): width scales with font size and character count.
fn approx_extents(style: &Table, text: &str) -> (f64, f64, f64, f64) {
    let size = n(style, "fontsize").max(1.0);
    let sx = n(style, "scale_x").max(1.0) / 100.0;
    let spacing = n(style, "spacing");
    let mut w = 0.0;
    for c in text.chars() {
        let cw = if (c as u32) >= 0x2E80 { 1.0 } else if c.is_uppercase() { 0.66 } else if c == ' ' { 0.28 } else { 0.5 };
        w += cw * size * sx + spacing;
    }
    (w, size * 1.2, size * 0.22, 0.0)
}

impl Host {
    pub fn new(doc: &Document, script_path: Option<&Path>, fps: f64) -> Result<Host> {
        Self::init(doc, script_path, fps).l()
    }

    fn init(doc: &Document, script_path: Option<&Path>, fps: f64) -> mlua::Result<Host> {
        let lua = Lua::new();
        let entries = Rc::new(RefCell::new(doc.entries.clone()));
        let log = Rc::new(RefCell::new(Vec::new()));
        let fps_c = Rc::new(Cell::new(fps));
        let script_dir = script_path.and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        let res = {
            let x = doc.info("PlayResX").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let y = doc.info("PlayResY").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            (x, y)
        };
        {
            let host = lua.create_table()?;
            let l = log.clone();
            host.set("log", lua.create_function(move |_, s: String| {
                l.borrow_mut().push(s);
                Ok(())
            })?)?;
            host.set("progress", lua.create_function(|_, _p: Value| Ok(()))?)?;
            host.set("decode_path", lua.create_function(|_, p: String| Ok(p))?)?;
            let f = fps_c.clone();
            host.set("fps", lua.create_function(move |_, ()| Ok(f.get()))?)?;
            host.set("video_size", lua.create_function(move |_, ()| Ok(res))?)?;
            host.set("text_extents", lua.create_function(|_, (style, text): (Table, String)| Ok(approx_extents(&style, &text)))?)?;
            lua.globals().set("__host", host)?;

            let sd = script_dir.clone();
            let include = lua.create_function(move |lua, name: String| {
                let g = lua.globals();
                let src = match name.as_str() {
                    "utils.lua" => Some(UTILS_LUA.to_string()),
                    "karaskel.lua" => Some(KARASKEL_LUA.to_string()),
                    _ => None,
                };
                let src = match src {
                    Some(s) => s,
                    None => {
                        let mut cands = vec![sd.join(&name), PathBuf::from(&name)];
                        if let Ok(extra) = std::env::var("ANISUB_INCLUDE") {
                            for d in extra.split(':') {
                                cands.push(Path::new(d).join(&name));
                            }
                        }
                        let found = cands.into_iter().find(|p| p.is_file());
                        match found {
                            Some(p) => std::fs::read_to_string(p).map_err(mlua::Error::external)?,
                            None => return Err(mlua::Error::runtime(format!("include: cannot find '{}'", name))),
                        }
                    }
                };
                let f = lua.load(&src).set_name(format!("@{}", name)).into_function()?;
                f.call::<()>(())?;
                let _ = g;
                Ok(())
            })?;
            lua.globals().set("include", include)?;
            // Common aliases scripts rely on from LuaJIT / Lua 5.2
            lua.load("unpack = unpack or table.unpack; loadstring = loadstring or load; table.getn = table.getn or function(t) return #t end")
                .exec()?;
            crate::modules::install(&lua)?;
            lua.load(AEGISUB_LUA).set_name("@aegisub.lua").exec()?;
        }
        Ok(Host { lua, entries, log, fps: fps_c, script_dir })
    }

    #[allow(dead_code)]
    pub fn set_fps(&self, f: f64) {
        self.fps.set(f);
    }
    #[allow(dead_code)]
    pub fn script_dir(&self) -> &Path {
        &self.script_dir
    }

    pub fn load_script(&self, path: &Path) -> Result<()> {
        let src = std::fs::read(path)?;
        let src = String::from_utf8_lossy(&src).into_owned();
        let src = src.strip_prefix('\u{feff}').unwrap_or(&src).to_string();
        self.load_source(&src, &path.display().to_string())
    }

    pub fn load_source(&self, src: &str, name: &str) -> Result<()> {
        self.lua.load(src).set_name(format!("@{}", name)).exec().l()?;
        Ok(())
    }

    pub fn macros(&self) -> Result<Vec<Macro>> {
        (|| -> mlua::Result<Vec<Macro>> {
            let mut out = Vec::new();
            for (global, kind) in [("__macros", "macro"), ("__filters", "filter")] {
                let t: Table = self.lua.globals().get(global)?;
                for e in t.sequence_values::<Table>() {
                    let e = e?;
                    out.push(Macro { name: e.get("name")?, description: e.get::<Option<String>>("description")?.unwrap_or_default(), kind });
                }
            }
            Ok(out)
        })()
        .l()
    }

    /// Run a macro/filter by name or 1-based number; `selected` are 1-based line indices
    /// (default: all dialogue lines). Returns the new document.
    pub fn run(&self, which: &str, selected: Option<Vec<usize>>) -> Result<Document> {
        let (tbl, entry) = self.find(which)?;
        self.prepare_and_call(tbl, entry, selected)
    }

    fn prepare_and_call(&self, tbl: &'static str, entry: Table, selected: Option<Vec<usize>>) -> Result<Document> {
        let sel: Vec<usize> = selected.unwrap_or_else(|| {
            self.entries.borrow().iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i + 1).collect()
        });
        let subs = self.lua.create_userdata(Subs(self.entries.clone())).l()?;
        let f: Function = entry.get("fn").l()?;
        let res = if tbl == "__macros" {
            let sel_t = self.lua.create_sequence_from(sel.iter().map(|x| *x as i64)).l()?;
            let active = sel.first().copied().unwrap_or(1) as i64;
            f.call::<MultiValue>((subs, sel_t, active))
        } else {
            let cfg = self.lua.create_table().l()?;
            f.call::<MultiValue>((subs, cfg))
        };
        if let Err(e) = res {
            let msg = e.to_string();
            if msg.contains("aegisub_cancel") {
                return Err(anyhow!("macro cancelled"));
            }
            return Err(anyhow!("{}", msg));
        }
        Ok(Document { entries: self.entries.borrow().clone() })
    }

    fn find(&self, which: &str) -> Result<(&'static str, Table)> {
        self.find_inner(which).l()?.ok_or_else(|| anyhow!("no macro or filter named '{}'", which))
    }

    fn find_inner(&self, which: &str) -> mlua::Result<Option<(&'static str, Table)>> {
        let mut all: Vec<(&'static str, Table)> = Vec::new();
        for g in ["__macros", "__filters"] {
            let t: Table = self.lua.globals().get(g)?;
            for e in t.sequence_values::<Table>() {
                all.push((if g == "__macros" { "__macros" } else { "__filters" }, e?));
            }
        }
        if let Ok(i) = which.parse::<usize>() {
            if i >= 1 && i <= all.len() {
                return Ok(Some(all.swap_remove(i - 1)));
            }
        }
        for (g, t) in all {
            if t.get::<String>("name")? == which {
                return Ok(Some((g, t)));
            }
        }
        Ok(None)
    }
}

#[allow(dead_code)]
fn _unused(v: f64) -> String {
    fmt_num(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_edits_lines() {
        let doc = Document::parse("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,a\n");
        let h = Host::new(&doc, None, 24.0).unwrap();
        h.load_source(
            "aegisub.register_macro('t','d',function(subs,sel) local l=subs[1]; l.text='b'; subs[1]=l; subs.append(l); subs.delete(1) end)",
            "t",
        )
        .unwrap();
        let out = h.run("t", None).unwrap();
        assert_eq!(out.dialogues().count(), 1);
        assert_eq!(out.dialogues().next().unwrap().text, "b");
    }

    #[test]
    fn re_and_unicode() {
        let h = Host::new(&Document::default(), None, 24.0).unwrap();
        h.load_source(
            "aegisub.register_macro('r','',function(subs)\n assert(re.sub('foo bar', '(\\\\w+) (\\\\w+)', '\\\\2 \\\\1') == 'bar foo')\n assert(re.find('abcabc','b')[2].first == 5)\n assert(re.match('(a)(b)', 'xab')[3].str == 'b')\n assert(unicode.len('あい') == 2)\n assert(unicode.sub('あいう', 2, 3) == 'いう')\n end)",
            "t",
        )
        .unwrap();
        h.run("r", None).unwrap();
    }

    #[test]
    fn frame_helpers() {
        let h = Host::new(&Document::default(), None, 24.0).unwrap();
        h.load_source("aegisub.register_macro('f','',function(subs) assert(aegisub.frame_from_ms(1000)==24); assert(aegisub.ms_from_frame(24)==1000) end)", "t").unwrap();
        h.run("f", None).unwrap();
    }
}
