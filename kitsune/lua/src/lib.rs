//! Aegisub "Automation 4" compatible Lua host for Kitsune (LuaJIT, like Aegisub itself).
use kitsune_core::model::*;
use kitsune_core::tags;
use mlua::{Function, Lua, MultiValue, Table, Value, Variadic};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod libs;

const CANCEL: &str = "__KITSUNE_CANCEL__";

#[derive(Clone, Default)]
pub struct HostOptions {
    pub fps: Option<f64>,
    pub video_size: Option<(i32, i32)>,
    pub video_file: Option<String>,
    /// Values the (headless) dialog returns, by control name.
    pub dialog_overrides: HashMap<String, String>,
    /// Directory of the script being run (for `?script`).
    pub script_dir: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MacroInfo { pub name: String, pub description: String, pub kind: MacroKind }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MacroKind { Macro, Filter }

pub struct RunResult { pub selection: Vec<usize>, pub active: usize, pub cancelled: bool, pub log: Vec<String> }

pub struct Runner {
    lua: Lua,
    script: Rc<RefCell<Script>>,
    log: Rc<RefCell<Vec<String>>>,
    pub macros: Vec<MacroInfo>,
}

fn color_out(c: &str) -> String { let c = c.trim_end_matches('&'); format!("{}&", c) }
fn color_in(c: &str) -> String { c.trim_end_matches('&').to_string() }

fn entry_to_table(lua: &Lua, e: &Entry) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    match e {
        Entry::Info { key, value } => {
            t.set("class", "info")?; t.set("section", "Script Info")?;
            t.set("key", key.as_str())?; t.set("value", value.as_str())?;
            t.set("raw", format!("{}: {}", key, value))?;
        }
        Entry::Raw { section, raw } => {
            t.set("class", "unknown")?; t.set("section", section.as_str())?; t.set("raw", raw.as_str())?;
        }
        Entry::Style(s) => {
            t.set("class", "style")?; t.set("section", "V4+ Styles")?;
            t.set("name", s.name.as_str())?; t.set("fontname", s.fontname.as_str())?; t.set("fontsize", s.fontsize)?;
            t.set("color1", color_out(&s.color1))?; t.set("color2", color_out(&s.color2))?;
            t.set("color3", color_out(&s.color3))?; t.set("color4", color_out(&s.color4))?;
            t.set("bold", s.bold)?; t.set("italic", s.italic)?; t.set("underline", s.underline)?; t.set("strikeout", s.strikeout)?;
            t.set("scale_x", s.scale_x)?; t.set("scale_y", s.scale_y)?; t.set("spacing", s.spacing)?; t.set("angle", s.angle)?;
            t.set("borderstyle", s.borderstyle)?; t.set("outline", s.outline)?; t.set("shadow", s.shadow)?; t.set("align", s.align)?;
            t.set("margin_l", s.margin_l)?; t.set("margin_r", s.margin_r)?; t.set("margin_t", s.margin_t)?; t.set("margin_b", s.margin_t)?;
            t.set("encoding", s.encoding)?;
            t.set("raw", format!("Style: {}", s.name))?;
        }
        Entry::Event(v) => {
            t.set("class", "dialogue")?; t.set("section", "Events")?;
            t.set("comment", v.comment)?; t.set("layer", v.layer)?;
            t.set("start_time", v.start)?; t.set("end_time", v.end)?;
            t.set("style", v.style.as_str())?; t.set("actor", v.actor.as_str())?;
            t.set("margin_l", v.margin_l)?; t.set("margin_r", v.margin_r)?;
            t.set("margin_t", v.margin_t)?; t.set("margin_b", v.margin_t)?;
            t.set("effect", v.effect.as_str())?; t.set("text", v.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", format!("{}: {},{},{},{},{},{},{},{},{},{}", if v.comment { "Comment" } else { "Dialogue" }, v.layer,
                kitsune_core::time::ass_time(v.start), kitsune_core::time::ass_time(v.end), v.style, v.actor, v.margin_l, v.margin_r, v.margin_t, v.effect, v.text))?;
        }
    }
    Ok(t)
}

fn gs(t: &Table, k: &str) -> String { t.get::<Option<String>>(k).ok().flatten().unwrap_or_default() }
fn gf(t: &Table, k: &str, d: f64) -> f64 { t.get::<Option<f64>>(k).ok().flatten().unwrap_or(d) }
fn gb(t: &Table, k: &str) -> bool { match t.get::<Value>(k) { Ok(Value::Boolean(b)) => b, Ok(Value::Number(n)) => n != 0.0, Ok(Value::Integer(n)) => n != 0, _ => false } }

fn table_to_entry(t: &Table) -> mlua::Result<Entry> {
    let class = gs(t, "class");
    Ok(match class.as_str() {
        "info" => Entry::Info { key: gs(t, "key"), value: gs(t, "value") },
        "style" => {
            let mt = if t.contains_key("margin_t")? { gf(t, "margin_t", 10.0) } else { gf(t, "margin_b", 10.0) };
            Entry::Style(Style {
                name: gs(t, "name"), fontname: gs(t, "fontname"), fontsize: gf(t, "fontsize", 48.0),
                color1: color_in(&gs(t, "color1")), color2: color_in(&gs(t, "color2")),
                color3: color_in(&gs(t, "color3")), color4: color_in(&gs(t, "color4")),
                bold: gb(t, "bold"), italic: gb(t, "italic"), underline: gb(t, "underline"), strikeout: gb(t, "strikeout"),
                scale_x: gf(t, "scale_x", 100.0), scale_y: gf(t, "scale_y", 100.0), spacing: gf(t, "spacing", 0.0), angle: gf(t, "angle", 0.0),
                borderstyle: gf(t, "borderstyle", 1.0) as i32, outline: gf(t, "outline", 2.0), shadow: gf(t, "shadow", 2.0),
                align: gf(t, "align", 2.0) as i32, margin_l: gf(t, "margin_l", 10.0) as i32, margin_r: gf(t, "margin_r", 10.0) as i32,
                margin_t: mt as i32, encoding: gf(t, "encoding", 1.0) as i32,
            })
        }
        "dialogue" => {
            let mt = if t.contains_key("margin_t")? { gf(t, "margin_t", 0.0) } else { gf(t, "margin_b", 0.0) };
            Entry::Event(Event {
                comment: gb(t, "comment"), layer: gf(t, "layer", 0.0) as i32,
                start: gf(t, "start_time", 0.0).round() as i64, end: gf(t, "end_time", 0.0).round() as i64,
                style: gs(t, "style"), actor: gs(t, "actor"),
                margin_l: gf(t, "margin_l", 0.0) as i32, margin_r: gf(t, "margin_r", 0.0) as i32, margin_t: mt as i32,
                effect: gs(t, "effect"), text: gs(t, "text"), extra: String::new(),
            })
        }
        _ => Entry::Raw { section: gs(t, "section"), raw: gs(t, "raw") },
    })
}

struct Subs(Rc<RefCell<Script>>);

impl mlua::UserData for Subs {
    fn add_methods<M: mlua::UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(mlua::MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().entries.len()));
        m.add_meta_method(mlua::MetaMethod::Index, |lua, this, key: Value| {
            match key {
                Value::Integer(_) | Value::Number(_) => {
                    let i = match key { Value::Integer(i) => i as i64, Value::Number(n) => n as i64, _ => 0 };
                    let s = this.0.borrow();
                    if i < 1 || i as usize > s.entries.len() { return Err(mlua::Error::runtime(format!("Requested out-of-range line from subtitles: {}", i))); }
                    Ok(Value::Table(entry_to_table(lua, &s.entries[i as usize - 1])?))
                }
                Value::String(s) => {
                    let name = s.to_str()?.to_string();
                    let sc = this.0.clone();
                    let f: Function = match name.as_str() {
                        "n" => return Ok(Value::Integer(sc.borrow().entries.len() as _)),
                        "append" => lua.create_function(move |_, args: Variadic<Value>| {
                            for a in args.iter().filter(|a| matches!(a, Value::Table(_))) {
                                if let Value::Table(t) = a { sc.borrow_mut().entries.push(table_to_entry(t)?); }
                            }
                            Ok(())
                        })?,
                        "insert" => lua.create_function(move |_, args: Variadic<Value>| {
                            let mut it = args.into_iter().filter(|a| !matches!(a, Value::UserData(_)));
                            let idx = match it.next() { Some(Value::Integer(i)) => i as usize, Some(Value::Number(n)) => n as usize, _ => 1 };
                            let mut pos = idx.saturating_sub(1).min(sc.borrow().entries.len());
                            for a in it { if let Value::Table(t) = a { sc.borrow_mut().entries.insert(pos, table_to_entry(&t)?); pos += 1; } }
                            Ok(())
                        })?,
                        "delete" => lua.create_function(move |_, args: Variadic<Value>| {
                            let mut idxs: Vec<usize> = args.iter().filter_map(|a| match a { Value::Integer(i) => Some(*i as usize), Value::Number(n) => Some(*n as usize), Value::Table(t) => {
                                // delete(sel table)
                                let _ = t; None }, _ => None }).collect();
                            for a in args.iter() { if let Value::Table(t) = a { for v in t.sequence_values::<usize>().flatten() { idxs.push(v); } } }
                            idxs.sort_unstable(); idxs.dedup();
                            let mut s = sc.borrow_mut();
                            for i in idxs.into_iter().rev() { if i >= 1 && i <= s.entries.len() { s.entries.remove(i - 1); } }
                            Ok(())
                        })?,
                        "deleterange" => lua.create_function(move |_, args: Variadic<Value>| {
                            let nums: Vec<usize> = args.iter().filter_map(|a| match a { Value::Integer(i) => Some(*i as usize), Value::Number(n) => Some(*n as usize), _ => None }).collect();
                            if nums.len() >= 2 { let mut s = sc.borrow_mut(); let (a, b) = (nums[0].max(1), nums[1].min(s.entries.len())); if a <= b { s.entries.drain(a - 1..b); } }
                            Ok(())
                        })?,
                        "unpack" | "pairs" => return Ok(Value::Nil),
                        _ => return Ok(Value::Nil),
                    };
                    Ok(Value::Function(f))
                }
                _ => Ok(Value::Nil),
            }
        });
        m.add_meta_method(mlua::MetaMethod::NewIndex, |_, this, (i, v): (i64, Table)| {
            let mut s = this.0.borrow_mut();
            let e = table_to_entry(&v)?;
            if i >= 1 && (i as usize) <= s.entries.len() { s.entries[i as usize - 1] = e; }
            else if i as usize == s.entries.len() + 1 { s.entries.push(e); }
            else { return Err(mlua::Error::runtime(format!("Attempt to write out-of-range line: {}", i))); }
            Ok(())
        });
    }
}

impl Runner {
    pub fn new(script: Script, opts: HostOptions) -> mlua::Result<Runner> {
        let lua = Lua::new();
        let script = Rc::new(RefCell::new(script));
        let log = Rc::new(RefCell::new(Vec::new()));
        libs::install(&lua, &script, &log, &opts)?;
        Ok(Runner { lua, script, log, macros: Vec::new() })
    }

    pub fn script(&self) -> Script { self.script.borrow().clone() }
    pub fn set_script(&self, s: Script) { *self.script.borrow_mut() = s; }

    /// Load an automation script (.lua). Registers its macros.
    pub fn load_file(&mut self, path: &str) -> mlua::Result<()> {
        let code = std::fs::read(path).map_err(mlua::Error::external)?;
        let code = kitsune_core::decode(&code);
        let g = self.lua.globals();
        for k in ["script_name", "script_description", "script_author", "script_version"] { g.set(k, Value::Nil)?; }
        // directory for relative include() / require
        if let Some(dir) = std::path::Path::new(path).parent().and_then(|p| p.to_str()) {
            let pkg: Table = g.get("package")?;
            let cur: String = pkg.get("path")?;
            pkg.set("path", format!("{d}/?.lua;{d}/?/init.lua;{cur}", d = if dir.is_empty() { "." } else { dir }))?;
            self.lua.globals().set("__script_dir", dir)?;
        }
        self.lua.load(&code).set_name(format!("@{}", path)).exec()?;
        self.refresh_macros()
    }

    pub fn load_source(&mut self, name: &str, code: &str) -> mlua::Result<()> {
        self.lua.load(code).set_name(format!("={}", name)).exec()?;
        self.refresh_macros()
    }

    fn refresh_macros(&mut self) -> mlua::Result<()> {
        let reg: Table = self.lua.globals().get("__macros")?;
        self.macros.clear();
        for v in reg.sequence_values::<Table>() {
            let t = v?;
            self.macros.push(MacroInfo { name: t.get("name")?, description: t.get::<Option<String>>("description")?.unwrap_or_default(),
                kind: if t.get::<Option<bool>>("filter")?.unwrap_or(false) { MacroKind::Filter } else { MacroKind::Macro } });
        }
        Ok(())
    }

    /// Run macro `idx`. `selection` and `active` are 1-based indexes into the `subs` array.
    pub fn run(&mut self, idx: usize, selection: &[usize], active: usize) -> Result<RunResult, String> {
        let backup = self.script.borrow().clone();
        self.log.borrow_mut().clear();
        let reg: Table = self.lua.globals().get("__macros").map_err(|e| e.to_string())?;
        let m: Table = reg.get(idx + 1).map_err(|e| e.to_string())?;
        let is_filter = m.get::<Option<bool>>("filter").ok().flatten().unwrap_or(false);
        let f: Function = m.get("fn").map_err(|e| e.to_string())?;
        let subs = self.lua.create_userdata(Subs(self.script.clone())).map_err(|e| e.to_string())?;
        let sel = self.lua.create_sequence_from(selection.iter().copied()).map_err(|e| e.to_string())?;
        let res: mlua::Result<MultiValue> = if is_filter {
            let cfg = self.lua.create_table().map_err(|e| e.to_string())?;
            f.call((subs, cfg))
        } else { f.call((subs, sel, active)) };
        match res {
            Ok(rv) => {
                let mut sel_out = selection.to_vec();
                let mut act = active;
                let mut it = rv.into_iter();
                if let Some(Value::Table(t)) = it.next() { sel_out = t.sequence_values::<usize>().flatten().collect(); }
                else if let Some(Value::Integer(_)) = None::<Value> {}
                if let Some(Value::Integer(a)) = it.next() { act = a as usize; } else if let Some(&f) = sel_out.first() { act = f; }
                Ok(RunResult { selection: sel_out, active: act, cancelled: false, log: self.log.borrow().clone() })
            }
            Err(e) => {
                *self.script.borrow_mut() = backup;
                let s = e.to_string();
                if s.contains(CANCEL) { Ok(RunResult { selection: selection.to_vec(), active, cancelled: true, log: self.log.borrow().clone() }) }
                else { Err(s) }
            }
        }
    }

    /// Run `validate` of macro; true if no validator.
    pub fn validate(&self, idx: usize, selection: &[usize], active: usize) -> bool {
        let Ok(reg) = self.lua.globals().get::<Table>("__macros") else { return true };
        let Ok(m) = reg.get::<Table>(idx + 1) else { return true };
        let Ok(Some(v)) = m.get::<Option<Function>>("validate") else { return true };
        let Ok(subs) = self.lua.create_userdata(Subs(self.script.clone())) else { return true };
        let sel = self.lua.create_sequence_from(selection.iter().copied()).unwrap();
        v.call::<Value>((subs, sel, active)).map(|r| !matches!(r, Value::Nil | Value::Boolean(false))).unwrap_or(false)
    }
}

pub(crate) fn tags_strip(s: &str) -> String { tags::strip(s) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macro_roundtrip() {
        let mut s = Script::new_default();
        s.entries.push(Entry::Event(Event::new(0, 1000, "hello")));
        let mut r = Runner::new(s, HostOptions::default()).unwrap();
        r.load_source("t", r#"
            aegisub.register_macro("up", "d", function(subs, sel)
                for _, i in ipairs(sel) do local l = subs[i]; l.text = l.text .. "!" ; subs[i] = l end
                subs.insert(1, {class="info", key="Title", value="x"})
                return sel
            end)"#).unwrap();
        assert_eq!(r.macros.len(), 1);
        let n = r.script().entries.len();
        r.run(0, &[n], n).unwrap();
        let out = r.script();
        assert_eq!(out.info("Title"), Some("x"));
        assert_eq!(out.events().next().unwrap().text, "hello!");
    }
    #[test]
    fn cancel_restores() {
        let mut s = Script::new_default();
        s.entries.push(Entry::Event(Event::new(0, 1000, "a")));
        let mut r = Runner::new(s, HostOptions::default()).unwrap();
        r.load_source("t", r#"aegisub.register_macro("c","", function(subs) subs.deleterange(1,#subs); aegisub.cancel() end)"#).unwrap();
        let before = r.script().entries.len();
        let res = r.run(0, &[], 1).unwrap();
        assert!(res.cancelled);
        assert_eq!(r.script().entries.len(), before);
    }
}
