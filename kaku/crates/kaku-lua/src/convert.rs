//! Conversion between `kaku_core` entries and Aegisub-style Lua line tables.

use kaku_core::ass::{Entry, Kind, Line, Style};
use kaku_core::color::Color;
use mlua::{Lua, Result, Table, Value};

fn s(t: &Table, k: &str, d: &str) -> String {
    t.get::<Option<String>>(k).ok().flatten().unwrap_or_else(|| d.to_string())
}
fn n(t: &Table, k: &str, d: f64) -> f64 {
    t.get::<Option<f64>>(k).ok().flatten().unwrap_or(d)
}
fn i(t: &Table, k: &str, d: i32) -> i32 {
    n(t, k, d as f64) as i32
}
fn b(t: &Table, k: &str, d: bool) -> bool {
    t.get::<Option<bool>>(k).ok().flatten().unwrap_or(d)
}
fn col(t: &Table, k: &str, d: Color) -> Color {
    t.get::<Option<String>>(k).ok().flatten().and_then(|v| Color::parse(&v)).unwrap_or(d)
}

pub fn entry_to_table(lua: &Lua, e: &Entry) -> Result<Table> {
    let t = lua.create_table()?;
    match e {
        Entry::Info { key, value } => {
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", key.as_str())?;
            t.set("value", value.as_str())?;
            t.set("raw", format!("{}: {}", key, value))?;
        }
        Entry::Style(st) => {
            t.set("class", "style")?;
            t.set("section", "V4+ Styles")?;
            t.set("name", st.name.as_str())?;
            t.set("fontname", st.fontname.as_str())?;
            t.set("fontsize", st.fontsize)?;
            t.set("color1", st.color1.to_string())?;
            t.set("color2", st.color2.to_string())?;
            t.set("color3", st.color3.to_string())?;
            t.set("color4", st.color4.to_string())?;
            t.set("bold", st.bold)?;
            t.set("italic", st.italic)?;
            t.set("underline", st.underline)?;
            t.set("strikeout", st.strikeout)?;
            t.set("scale_x", st.scale_x)?;
            t.set("scale_y", st.scale_y)?;
            t.set("spacing", st.spacing)?;
            t.set("angle", st.angle)?;
            t.set("borderstyle", st.borderstyle)?;
            t.set("outline", st.outline)?;
            t.set("shadow", st.shadow)?;
            t.set("align", st.align)?;
            t.set("margin_l", st.margin_l)?;
            t.set("margin_r", st.margin_r)?;
            t.set("margin_t", st.margin_t)?;
            t.set("margin_b", st.margin_t)?;
            t.set("encoding", st.encoding)?;
            t.set("relative_to", 2)?;
            t.set("raw", st.to_line())?;
        }
        Entry::Line(l) => {
            t.set("class", "dialogue")?;
            t.set("section", "Events")?;
            t.set("comment", l.kind == Kind::Comment)?;
            t.set("layer", l.layer)?;
            t.set("start_time", l.start)?;
            t.set("end_time", l.end)?;
            t.set("style", l.style.as_str())?;
            t.set("actor", l.actor.as_str())?;
            t.set("margin_l", l.margin_l)?;
            t.set("margin_r", l.margin_r)?;
            t.set("margin_t", l.margin_t)?;
            t.set("margin_b", l.margin_t)?;
            t.set("effect", l.effect.as_str())?;
            t.set("text", l.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", l.to_line())?;
        }
        Entry::Raw { section, text } => {
            t.set("class", "unknown")?;
            t.set("section", section.as_str())?;
            t.set("raw", text.as_str())?;
        }
    }
    Ok(t)
}

pub fn table_to_entry(v: &Value) -> Result<Entry> {
    let t = match v {
        Value::Table(t) => t,
        _ => return Err(mlua::Error::runtime("subtitle line must be a table")),
    };
    let class = s(t, "class", "dialogue").to_ascii_lowercase();
    Ok(match class.as_str() {
        "info" => Entry::Info { key: s(t, "key", ""), value: t.get::<Option<mlua::Value>>("value")?.map(|v| match v { Value::String(x) => x.to_string_lossy().to_string(), Value::Integer(x) => x.to_string(), Value::Number(x) => kaku_core::ass::fmt_num(x), o => format!("{:?}", o) }).unwrap_or_default() },
        "style" => {
            let d = Style::default();
            Entry::Style(Style {
                name: s(t, "name", &d.name),
                fontname: s(t, "fontname", &d.fontname),
                fontsize: n(t, "fontsize", d.fontsize),
                color1: col(t, "color1", d.color1),
                color2: col(t, "color2", d.color2),
                color3: col(t, "color3", d.color3),
                color4: col(t, "color4", d.color4),
                bold: b(t, "bold", false),
                italic: b(t, "italic", false),
                underline: b(t, "underline", false),
                strikeout: b(t, "strikeout", false),
                scale_x: n(t, "scale_x", 100.0),
                scale_y: n(t, "scale_y", 100.0),
                spacing: n(t, "spacing", 0.0),
                angle: n(t, "angle", 0.0),
                borderstyle: i(t, "borderstyle", 1),
                outline: n(t, "outline", d.outline),
                shadow: n(t, "shadow", d.shadow),
                align: i(t, "align", 2),
                margin_l: i(t, "margin_l", d.margin_l),
                margin_r: i(t, "margin_r", d.margin_r),
                margin_t: t.get::<Option<f64>>("margin_t")?.or(t.get::<Option<f64>>("margin_b")?).map(|x| x as i32).unwrap_or(d.margin_t),
                encoding: i(t, "encoding", 1),
            })
        }
        "unknown" => Entry::Raw { section: s(t, "section", "Unknown"), text: s(t, "raw", "") },
        _ => Entry::Line(Line {
            kind: if b(t, "comment", false) { Kind::Comment } else { Kind::Dialogue },
            layer: i(t, "layer", 0),
            start: n(t, "start_time", 0.0).round() as i64,
            end: n(t, "end_time", 0.0).round() as i64,
            style: s(t, "style", "Default"),
            actor: s(t, "actor", ""),
            margin_l: i(t, "margin_l", 0),
            margin_r: i(t, "margin_r", 0),
            margin_t: t.get::<Option<f64>>("margin_t")?.or(t.get::<Option<f64>>("margin_b")?).map(|x| x as i32).unwrap_or(0),
            effect: s(t, "effect", ""),
            text: s(t, "text", ""),
        }),
    })
}
