//! Aegisub-compatible `subs` object: a flat, 1-indexed list of info / style / dialogue lines.

use asscore::{ass, time, Color, Event, Extra, Script, Style};
use mlua::{Function, Lua, MetaMethod, MultiValue, Result as LResult, Table, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum LuaLine {
    Info(String, String),
    Style(Style),
    Event(Event),
    /// Unrecognised class, kept as raw text.
    Raw(String),
}

#[derive(Default)]
pub struct Subs {
    pub lines: Vec<LuaLine>,
    pub res: (i32, i32),
    /// Carry-through data we don't expose to Lua.
    pub garbage: Vec<(String, String)>,
    pub extradata: Vec<Extra>,
    pub raw_sections: Vec<(String, Vec<String>)>,
}

impl Subs {
    pub fn from_script(sc: &Script) -> Subs {
        let mut lines = Vec::with_capacity(sc.info.len() + sc.styles.len() + sc.events.len());
        for (k, v) in &sc.info {
            lines.push(LuaLine::Info(k.clone(), v.clone()));
        }
        for s in &sc.styles {
            lines.push(LuaLine::Style(s.clone()));
        }
        for e in &sc.events {
            lines.push(LuaLine::Event(e.clone()));
        }
        Subs { lines, res: sc.res(), garbage: sc.garbage.clone(), extradata: sc.extradata.clone(), raw_sections: sc.raw_sections.clone() }
    }

    pub fn lua_index_of_event(&self, ev: usize) -> Option<usize> {
        let mut n = 0;
        for (i, l) in self.lines.iter().enumerate() {
            if matches!(l, LuaLine::Event(_)) {
                if n == ev {
                    return Some(i);
                }
                n += 1;
            }
        }
        None
    }

    pub fn into_script(self) -> Script {
        let mut sc = Script::default();
        sc.garbage = self.garbage;
        sc.extradata = self.extradata;
        sc.raw_sections = self.raw_sections;
        for l in self.lines {
            match l {
                LuaLine::Info(k, v) => sc.info.push((k, v)),
                LuaLine::Style(s) => sc.styles.push(s),
                LuaLine::Event(e) => sc.events.push(e),
                LuaLine::Raw(_) => {}
            }
        }
        asscore::extradata::prune(&mut sc);
        sc
    }
}

pub type SubsRc = Rc<RefCell<Subs>>;

fn color_str(c: &Color) -> String {
    format!("&H{:02X}{:02X}{:02X}{:02X}&", c.a, c.b, c.g, c.r)
}

pub fn line_to_table(lua: &Lua, l: &LuaLine, extradata: &[Extra]) -> LResult<Table> {
    let t = lua.create_table()?;
    match l {
        LuaLine::Info(k, v) => {
            t.set("class", "info")?;
            t.set("section", "[Script Info]")?;
            t.set("key", k.as_str())?;
            t.set("value", v.as_str())?;
            t.set("raw", if k == ";" { format!("; {}", v) } else { format!("{}: {}", k, v) })?;
        }
        LuaLine::Style(s) => {
            t.set("class", "style")?;
            t.set("section", "[V4+ Styles]")?;
            t.set("raw", ass::style_line(s))?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", color_str(&s.primary))?;
            t.set("color2", color_str(&s.secondary))?;
            t.set("color3", color_str(&s.outline_color))?;
            t.set("color4", color_str(&s.back))?;
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
            t.set("relative_to", 2)?;
            t.set("vertical", false)?;
        }
        LuaLine::Event(e) => {
            t.set("class", "dialogue")?;
            t.set("section", "[Events]")?;
            t.set("raw", e.to_ass())?;
            t.set("comment", e.comment)?;
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
            // `{=1=2}` at the start refers to extradata: scripts get it as `extra`, not in the text
            let (_, text) = asscore::extradata::split_marker(&e.text);
            t.set("text", text)?;
            let extra = lua.create_table()?;
            for (k, v) in asscore::extradata::line_values(extradata, &e.text) { extra.set(k, v)?; }
            t.set("extra", extra)?;
        }
        LuaLine::Raw(r) => {
            t.set("class", "unknown")?;
            t.set("raw", r.as_str())?;
        }
    }
    Ok(t)
}

fn get_f64(t: &Table, k: &str, d: f64) -> f64 {
    match t.get::<Value>(k) {
        Ok(Value::Number(n)) => n,
        Ok(Value::Integer(n)) => n as f64,
        Ok(Value::String(s)) => s.to_str().ok().and_then(|s| s.trim().parse().ok()).unwrap_or(d),
        _ => d,
    }
}
fn get_i(t: &Table, k: &str, d: i64) -> i64 {
    let v = get_f64(t, k, d as f64);
    if v.is_finite() { v.round() as i64 } else { d }
}
fn get_s(t: &Table, k: &str) -> String {
    match t.get::<Value>(k) {
        Ok(Value::String(s)) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
        Ok(Value::Number(n)) => asscore::fmt_num(n),
        Ok(Value::Integer(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn get_b(t: &Table, k: &str) -> bool {
    match t.get::<Value>(k) {
        Ok(Value::Boolean(b)) => b,
        Ok(Value::Number(n)) => n != 0.0,
        Ok(Value::Integer(n)) => n != 0,
        _ => false,
    }
}
fn has(t: &Table, k: &str) -> bool {
    !matches!(t.get::<Value>(k), Ok(Value::Nil) | Err(_))
}

/// `line.extra` as ids into the extradata (entries are shared and added as needed).
fn extra_ids(t: &Table, extradata: &mut Vec<Extra>) -> Vec<u32> {
    let Ok(Value::Table(ex)) = t.get::<Value>("extra") else { return vec![] };
    let mut kv: Vec<(String, String)> = ex.pairs::<Value, Value>().flatten().filter_map(|(k, v)| {
        let k = match k { Value::String(s) => s.to_str().ok()?.to_string(), _ => return None };
        let v = match v { Value::String(s) => s.to_str().ok()?.to_string(), Value::Number(n) => asscore::fmt_num(n), Value::Integer(i) => i.to_string(), Value::Boolean(b) => b.to_string(), _ => return None };
        Some((k, v))
    }).collect();
    kv.sort();
    let mut ids: Vec<u32> = kv.iter().map(|(k, v)| asscore::extradata::intern(extradata, k, v)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub fn table_to_line(t: &Table, extradata: &mut Vec<Extra>) -> LuaLine {
    let class = get_s(t, "class");
    match class.as_str() {
        "dialogue" => {
            let mt = if has(t, "margin_t") { get_i(t, "margin_t", 0) } else { get_i(t, "margin_v", 0) };
            let text = get_s(t, "text");
            let ids = extra_ids(t, extradata);
            // a marker typed into the text by the script itself is kept as it is
            let text = if ids.is_empty() { text } else { asscore::extradata::with_marker(&ids, asscore::extradata::split_marker(&text).1) };
            LuaLine::Event(Event {
                comment: get_b(t, "comment"),
                layer: get_i(t, "layer", 0) as i32,
                start: get_i(t, "start_time", 0),
                end: get_i(t, "end_time", 0),
                style: get_s(t, "style"),
                actor: get_s(t, "actor"),
                margin_l: get_i(t, "margin_l", 0) as i32,
                margin_r: get_i(t, "margin_r", 0) as i32,
                margin_v: mt as i32,
                effect: get_s(t, "effect"),
                text,
            })
        }
        "style" => {
            let col = |k: &str, d: Color| Color::parse(&get_s(t, k)).unwrap_or(d);
            let mt = if has(t, "margin_t") { get_i(t, "margin_t", 10) } else { get_i(t, "margin_v", 10) };
            LuaLine::Style(Style {
                name: get_s(t, "name"),
                fontname: get_s(t, "fontname"),
                fontsize: get_f64(t, "fontsize", 48.0),
                primary: col("color1", Color::WHITE),
                secondary: col("color2", Color::RED),
                outline_color: col("color3", Color::BLACK),
                back: col("color4", Color::BLACK),
                bold: get_b(t, "bold"),
                italic: get_b(t, "italic"),
                underline: get_b(t, "underline"),
                strikeout: get_b(t, "strikeout"),
                scale_x: get_f64(t, "scale_x", 100.0),
                scale_y: get_f64(t, "scale_y", 100.0),
                spacing: get_f64(t, "spacing", 0.0),
                angle: get_f64(t, "angle", 0.0),
                border_style: match t.get::<Value>("borderstyle") {
                    Ok(Value::Boolean(b)) => if b { 3 } else { 1 },
                    _ => get_i(t, "borderstyle", 1) as i32,
                },
                outline: get_f64(t, "outline", 2.0),
                shadow: get_f64(t, "shadow", 2.0),
                alignment: get_i(t, "align", 2) as i32,
                margin_l: get_i(t, "margin_l", 10) as i32,
                margin_r: get_i(t, "margin_r", 10) as i32,
                margin_v: mt as i32,
                encoding: get_i(t, "encoding", 1) as i32,
            })
        }
        "info" => LuaLine::Info(get_s(t, "key"), get_s(t, "value")),
        _ => LuaLine::Raw(get_s(t, "raw")),
    }
}

fn idx_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) if n.fract() == 0.0 => Some(*n as i64),
        _ => None,
    }
}

pub struct SubsUd(pub SubsRc);

impl UserData for SubsUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().lines.len() as i64));
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: Value| -> LResult<Value> {
            if let Some(i) = idx_of(&key) {
                let s = this.0.borrow();
                if i >= 1 && (i as usize) <= s.lines.len() {
                    return Ok(Value::Table(line_to_table(lua, &s.lines[i as usize - 1], &s.extradata)?));
                }
                return Ok(Value::Nil);
            }
            let name = match &key {
                Value::String(s) => s.to_str()?.to_string(),
                _ => return Ok(Value::Nil),
            };
            let rc = this.0.clone();
            let f: Function = match name.as_str() {
                "n" => return Ok(Value::Integer(rc.borrow().lines.len() as i64)),
                "insert" => lua.create_function(move |_, args: MultiValue| {
                    let mut it = args.into_iter();
                    let pos = it.next().as_ref().and_then(idx_of).unwrap_or(1);
                    let mut s = rc.borrow_mut();
                    let mut at = ((pos - 1).max(0) as usize).min(s.lines.len());
                    for v in it {
                        if let Value::Table(t) = v {
                            let l = table_to_line(&t, &mut s.extradata);
                            s.lines.insert(at, l);
                            at += 1;
                        }
                    }
                    Ok(())
                })?,
                "append" => lua.create_function(move |_, args: MultiValue| {
                    let mut s = rc.borrow_mut();
                    for v in args {
                        if let Value::Table(t) = v {
                            let l = table_to_line(&t, &mut s.extradata);
                            s.lines.push(l);
                        }
                    }
                    Ok(())
                })?,
                "delete" => lua.create_function(move |_, args: MultiValue| {
                    let mut idx: Vec<usize> = args.iter().filter_map(idx_of).filter(|i| *i >= 1).map(|i| i as usize - 1).collect();
                    idx.sort_unstable();
                    idx.dedup();
                    let mut s = rc.borrow_mut();
                    for i in idx.into_iter().rev() {
                        if i < s.lines.len() {
                            s.lines.remove(i);
                        }
                    }
                    Ok(())
                })?,
                "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                    let mut s = rc.borrow_mut();
                    let a = (a.max(1) - 1) as usize;
                    let b = (b.max(0) as usize).min(s.lines.len());
                    if a < b {
                        s.lines.drain(a..b);
                    }
                    Ok(())
                })?,
                "script_resolution" => lua.create_function(move |_, ()| {
                    let r = rc.borrow().res;
                    Ok((r.0, r.1))
                })?,
                _ => return Ok(Value::Nil),
            };
            Ok(Value::Function(f))
        });
        methods.add_meta_method(MetaMethod::NewIndex, |_, this, (key, val): (Value, Value)| {
            if let (Some(i), Value::Table(t)) = (idx_of(&key), val) {
                let mut s = this.0.borrow_mut();
                let l = table_to_line(&t, &mut s.extradata);
                let n = s.lines.len() as i64;
                // Aegisub: subs[0] = line appends, subs[-i] = line inserts before line i
                if i >= 1 && i <= n {
                    s.lines[i as usize - 1] = l;
                } else if i == 0 || i == n + 1 {
                    s.lines.push(l);
                } else if i < 0 && -i <= n {
                    s.lines.insert((-i - 1) as usize, l);
                }
            }
            Ok(())
        });
    }
}

#[allow(dead_code)]
pub fn fmt_time(ms: i64) -> String {
    time::format_ass(ms)
}
