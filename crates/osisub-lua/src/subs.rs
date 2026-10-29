//! Lua'daki `subs` nesnesi ve satır tablosu dönüşümleri.
use mlua::{AnyUserData, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value, Variadic};
use osisub_core::ass::{self, Dialogue, Entry, Style};
use osisub_core::color::Color;
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedScript = Rc<RefCell<ass::Script>>;

pub struct Subs(pub SharedScript);

fn str_field(t: &Table, k: &str) -> Option<String> {
    match t.get::<Value>(k).ok()? {
        Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

fn num_field(t: &Table, k: &str) -> Option<f64> {
    match t.get::<Value>(k).ok()? {
        Value::Number(n) => Some(n),
        Value::Integer(n) => Some(n as f64),
        Value::String(s) => s.to_str().ok()?.trim().parse().ok(),
        _ => None,
    }
}

fn bool_field(t: &Table, k: &str) -> Option<bool> {
    match t.get::<Value>(k).ok()? {
        Value::Boolean(b) => Some(b),
        Value::Number(n) => Some(n != 0.0),
        Value::Integer(n) => Some(n != 0),
        _ => None,
    }
}

fn section_name(sec: &str) -> String {
    format!("[{}]", sec)
}

pub fn entry_to_table(lua: &Lua, e: &Entry) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("section", section_name(e.section()))?;
    match e {
        Entry::Info { key, value } => {
            t.set("class", "info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            t.set("raw", format!("{}: {}", key, value))?;
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
            t.set("margin_t", s.margin_t)?;
            t.set("margin_b", s.margin_b)?;
            t.set("margin_v", s.margin_t)?;
            t.set("encoding", s.encoding)?;
            t.set("relative_to", 2)?;
            t.set("raw", ass::style_line(s))?;
        }
        Entry::Dialogue(d) => {
            t.set("class", "dialogue")?;
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
            t.set("margin_v", d.margin_t)?;
            t.set("effect", d.effect.as_str())?;
            t.set("text", d.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", ass::dialogue_line(d))?;
        }
        Entry::Unknown { raw, .. } => {
            t.set("class", "unknown")?;
            t.set("raw", raw.as_str())?;
        }
    }
    Ok(t)
}

pub fn table_to_entry(t: &Table, default_section: &str) -> Result<Entry> {
    let class = str_field(t, "class").unwrap_or_default();
    let int = |k: &str, d: i32| num_field(t, k).map(|v| v as i32).unwrap_or(d);
    Ok(match class.as_str() {
        "info" => Entry::Info {
            key: str_field(t, "key").unwrap_or_default(),
            value: str_field(t, "value").unwrap_or_default(),
        },
        "style" => {
            let base = Style::default();
            let col = |k: &str, d: &str| str_field(t, k).map(|v| Color::parse(&v).map(|c| c.to_style_string()).unwrap_or(v)).unwrap_or(d.to_string());
            let mt = if t.contains_key("margin_t")? { int("margin_t", 10) } else { int("margin_v", 10) };
            let mb = if t.contains_key("margin_b")? { int("margin_b", mt) } else { mt };
            Entry::Style(Style {
                name: str_field(t, "name").unwrap_or(base.name),
                fontname: str_field(t, "fontname").unwrap_or(base.fontname),
                fontsize: num_field(t, "fontsize").unwrap_or(base.fontsize),
                color1: col("color1", &base.color1),
                color2: col("color2", &base.color2),
                color3: col("color3", &base.color3),
                color4: col("color4", &base.color4),
                bold: bool_field(t, "bold").unwrap_or(false),
                italic: bool_field(t, "italic").unwrap_or(false),
                underline: bool_field(t, "underline").unwrap_or(false),
                strikeout: bool_field(t, "strikeout").unwrap_or(false),
                scale_x: num_field(t, "scale_x").unwrap_or(100.0),
                scale_y: num_field(t, "scale_y").unwrap_or(100.0),
                spacing: num_field(t, "spacing").unwrap_or(0.0),
                angle: num_field(t, "angle").unwrap_or(0.0),
                borderstyle: int("borderstyle", 1),
                outline: num_field(t, "outline").unwrap_or(2.0),
                shadow: num_field(t, "shadow").unwrap_or(2.0),
                align: int("align", 2),
                margin_l: int("margin_l", 10),
                margin_r: int("margin_r", 10),
                margin_t: mt,
                margin_b: mb,
                encoding: int("encoding", 1),
            })
        }
        "dialogue" => {
            let mt = if t.contains_key("margin_t")? { int("margin_t", 0) } else { int("margin_v", 0) };
            let mb = if t.contains_key("margin_b")? { int("margin_b", mt) } else { mt };
            Entry::Dialogue(Dialogue {
                comment: bool_field(t, "comment").unwrap_or(false),
                layer: int("layer", 0),
                start_time: num_field(t, "start_time").unwrap_or(0.0).round() as i64,
                end_time: num_field(t, "end_time").unwrap_or(0.0).round() as i64,
                style: str_field(t, "style").unwrap_or_else(|| "Default".into()),
                actor: str_field(t, "actor").unwrap_or_default(),
                margin_l: int("margin_l", 0),
                margin_r: int("margin_r", 0),
                margin_t: mt,
                margin_b: mb,
                effect: str_field(t, "effect").unwrap_or_default(),
                text: str_field(t, "text").unwrap_or_default(),
            })
        }
        _ => {
            let section = str_field(t, "section")
                .map(|s| s.trim_start_matches('[').trim_end_matches(']').to_string())
                .unwrap_or_else(|| default_section.to_string());
            Entry::Unknown { section, raw: str_field(t, "raw").unwrap_or_default() }
        }
    })
}

fn idx(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) => Some(*n as i64),
        _ => None,
    }
}

fn last_section(script: &ass::Script) -> String {
    script.entries.last().map(|e| e.section().to_string()).unwrap_or_else(|| ass::SEC_EVENTS.to_string())
}

impl UserData for Subs {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(MetaMethod::Index, |lua, (ud, key): (AnyUserData, Value)| -> Result<Value> {
            let shared = ud.borrow::<Subs>()?.0.clone();
            match &key {
                Value::String(s) => {
                    let name = s.to_str()?.to_string();
                    let sh = shared.clone();
                    let f = match name.as_str() {
                        "n" => return Ok(Value::Integer(shared.borrow().entries.len() as i64)),
                        "append" => lua.create_function(move |lua, items: Variadic<Table>| {
                            let mut sc = sh.borrow_mut();
                            for t in items {
                                let sec = last_section(&sc);
                                let e = table_to_entry(&t, &sec)?;
                                sc.entries.push(e);
                            }
                            let _ = lua;
                            Ok(())
                        })?,
                        "insert" => lua.create_function(move |_, (i, items): (i64, Variadic<Table>)| {
                            let mut sc = sh.borrow_mut();
                            let mut pos = (i.max(1) - 1) as usize;
                            for t in items {
                                let sec = last_section(&sc);
                                let e = table_to_entry(&t, &sec)?;
                                let p = pos.min(sc.entries.len());
                                sc.entries.insert(p, e);
                                pos += 1;
                            }
                            Ok(())
                        })?,
                        "delete" => lua.create_function(move |_, items: Variadic<i64>| {
                            let mut sc = sh.borrow_mut();
                            let mut v: Vec<i64> = items.into_iter().collect();
                            v.sort_unstable();
                            v.dedup();
                            for i in v.into_iter().rev() {
                                if i >= 1 && (i as usize) <= sc.entries.len() {
                                    sc.entries.remove(i as usize - 1);
                                }
                            }
                            Ok(())
                        })?,
                        "deleterange" => lua.create_function(move |_, (a, b): (i64, i64)| {
                            let mut sc = sh.borrow_mut();
                            let a = (a.max(1) - 1) as usize;
                            let b = (b.max(0) as usize).min(sc.entries.len());
                            if a < b {
                                sc.entries.drain(a..b);
                            }
                            Ok(())
                        })?,
                        _ => return Ok(Value::Nil),
                    };
                    Ok(Value::Function(f))
                }
                other => {
                    let Some(i) = idx(other) else { return Ok(Value::Nil) };
                    let sc = shared.borrow();
                    if i >= 1 && (i as usize) <= sc.entries.len() {
                        Ok(Value::Table(entry_to_table(lua, &sc.entries[i as usize - 1])?))
                    } else {
                        Ok(Value::Nil)
                    }
                }
            }
        });
        methods.add_meta_function(MetaMethod::NewIndex, |_, (ud, key, val): (AnyUserData, Value, Value)| {
            let shared = ud.borrow::<Subs>()?.0.clone();
            let Some(i) = idx(&key) else { return Ok(()) };
            let mut sc = shared.borrow_mut();
            match val {
                Value::Nil => {
                    if i >= 1 && (i as usize) <= sc.entries.len() {
                        sc.entries.remove(i as usize - 1);
                    }
                }
                Value::Table(t) => {
                    let sec = last_section(&sc);
                    let e = table_to_entry(&t, &sec)?;
                    if i >= 1 && (i as usize) <= sc.entries.len() {
                        sc.entries[i as usize - 1] = e;
                    } else {
                        sc.entries.push(e);
                    }
                }
                _ => return Err(mlua::Error::runtime("subs: atama için tablo ya da nil gerekli")),
            }
            Ok(())
        });
        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.0.borrow().entries.len() as i64));
    }
}
