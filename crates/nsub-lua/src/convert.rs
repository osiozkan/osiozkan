//! Conversion between core types and Aegisub-style Lua line tables.

use mlua::{Lua, Result, Table, Value};
use nsub_core::ass::{Event, InfoLine, Script, Style};
use nsub_core::Color;

/// A line in Aegisub's flat view of the file.
#[derive(Clone, Debug)]
pub enum Entry {
    Info(InfoLine),
    Style(Style),
    Event(Event),
}

pub fn flatten(script: &Script) -> Vec<Entry> {
    let mut v = Vec::with_capacity(script.info.len() + script.styles.len() + script.events.len());
    v.extend(script.info.iter().cloned().map(Entry::Info));
    v.extend(script.styles.iter().cloned().map(Entry::Style));
    v.extend(script.events.iter().cloned().map(Entry::Event));
    v
}

/// Write the (possibly reordered) flat list back; lines are partitioned by class, order kept within a class.
pub fn unflatten(entries: Vec<Entry>, into: &mut Script) {
    into.info.clear();
    into.styles.clear();
    into.events.clear();
    for e in entries {
        match e {
            Entry::Info(i) => into.info.push(i),
            Entry::Style(s) => into.styles.push(s),
            Entry::Event(e) => into.events.push(e),
        }
    }
}

fn color_str(c: Color) -> String {
    // Aegisub exposes style colours as "&HAABBGGRR&"
    format!("{}&", c.to_style_string())
}

pub fn entry_to_table(lua: &Lua, e: &Entry) -> Result<Table> {
    let t = lua.create_table()?;
    match e {
        Entry::Info(i) => {
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", i.key.as_str())?;
            t.set("value", i.value.as_str())?;
            if i.key == ";" {
                t.set("raw", i.value.as_str())?;
            }
        }
        Entry::Style(s) => {
            t.set("class", "style")?;
            t.set("section", "V4+ Styles")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", color_str(s.color1))?;
            t.set("color2", color_str(s.color2))?;
            t.set("color3", color_str(s.color3))?;
            t.set("color4", color_str(s.color4))?;
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
        }
        Entry::Event(e) => {
            t.set("class", "dialogue")?;
            t.set("section", "Events")?;
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
        }
    }
    Ok(t)
}

fn num(t: &Table, k: &str, default: f64) -> f64 {
    match t.get::<Value>(k) {
        Ok(Value::Number(n)) => n,
        Ok(Value::Integer(i)) => i as f64,
        Ok(Value::String(s)) => s.to_str().ok().and_then(|s| s.trim().parse().ok()).unwrap_or(default),
        _ => default,
    }
}
fn int(t: &Table, k: &str, default: i64) -> i64 {
    num(t, k, default as f64).round() as i64
}
fn string(t: &Table, k: &str, default: &str) -> String {
    match t.get::<Value>(k) {
        Ok(Value::String(s)) => s.to_string_lossy().to_string(),
        Ok(Value::Number(n)) => n.to_string(),
        Ok(Value::Integer(n)) => n.to_string(),
        _ => default.to_string(),
    }
}
fn boolean(t: &Table, k: &str, default: bool) -> bool {
    match t.get::<Value>(k) {
        Ok(Value::Boolean(b)) => b,
        Ok(Value::Number(n)) => n != 0.0,
        Ok(Value::Integer(n)) => n != 0,
        _ => default,
    }
}
fn color(t: &Table, k: &str, default: Color) -> Color {
    Color::parse(&string(t, k, "")).unwrap_or(default)
}

pub fn table_to_entry(t: &Table) -> std::result::Result<Entry, String> {
    let class = string(t, "class", "dialogue").to_ascii_lowercase();
    match class.as_str() {
        "dialogue" => {
            let mt = int(t, "margin_t", i64::MIN);
            let mb = int(t, "margin_b", i64::MIN);
            let mv = if mt != i64::MIN { mt } else if mb != i64::MIN { mb } else { int(t, "margin_v", 0) };
            Ok(Entry::Event(Event {
                comment: boolean(t, "comment", false),
                layer: int(t, "layer", 0) as i32,
                start: int(t, "start_time", 0),
                end: int(t, "end_time", 0),
                style: string(t, "style", "Default"),
                actor: string(t, "actor", ""),
                margin_l: int(t, "margin_l", 0) as i32,
                margin_r: int(t, "margin_r", 0) as i32,
                margin_t: mv as i32,
                effect: string(t, "effect", ""),
                text: string(t, "text", ""),
            }))
        }
        "style" => {
            let d = Style::default();
            let mt = int(t, "margin_t", i64::MIN);
            let mb = int(t, "margin_b", i64::MIN);
            let mv = if mt != i64::MIN { mt } else if mb != i64::MIN { mb } else { d.margin_t as i64 };
            Ok(Entry::Style(Style {
                name: string(t, "name", &d.name),
                fontname: string(t, "fontname", &d.fontname),
                fontsize: num(t, "fontsize", d.fontsize),
                color1: color(t, "color1", d.color1),
                color2: color(t, "color2", d.color2),
                color3: color(t, "color3", d.color3),
                color4: color(t, "color4", d.color4),
                bold: boolean(t, "bold", d.bold),
                italic: boolean(t, "italic", d.italic),
                underline: boolean(t, "underline", d.underline),
                strikeout: boolean(t, "strikeout", d.strikeout),
                scale_x: num(t, "scale_x", d.scale_x),
                scale_y: num(t, "scale_y", d.scale_y),
                spacing: num(t, "spacing", d.spacing),
                angle: num(t, "angle", d.angle),
                borderstyle: int(t, "borderstyle", d.borderstyle as i64) as i32,
                outline: num(t, "outline", d.outline),
                shadow: num(t, "shadow", d.shadow),
                align: int(t, "align", d.align as i64) as i32,
                margin_l: int(t, "margin_l", d.margin_l as i64) as i32,
                margin_r: int(t, "margin_r", d.margin_r as i64) as i32,
                margin_t: mv as i32,
                margin_b: mv as i32,
                encoding: int(t, "encoding", d.encoding as i64) as i32,
            }))
        }
        "info" => Ok(Entry::Info(InfoLine { key: string(t, "key", ""), value: string(t, "value", "") })),
        other => Err(format!("unsupported line class '{}'", other)),
    }
}
