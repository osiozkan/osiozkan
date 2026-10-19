//! Line ⇄ Lua table conversion following Aegisub's Automation 4 field names.
use crate::doc::Line;
use mlua::{Lua, Result, Table, Value};
use subforge_core::ass::{Event, EventKind, Style};
use subforge_core::color::Color;

fn getf(t: &Table, k: &str) -> Result<Option<f64>> {
    match t.get::<Value>(k)? {
        Value::Nil => Ok(None),
        Value::Integer(i) => Ok(Some(i as f64)),
        Value::Number(n) => Ok(Some(n)),
        Value::String(s) => Ok(s.to_str()?.trim().parse().ok()),
        Value::Boolean(b) => Ok(Some(b as i32 as f64)),
        _ => Ok(None),
    }
}

fn gets(t: &Table, k: &str) -> Result<Option<String>> {
    match t.get::<Value>(k)? {
        Value::Nil => Ok(None),
        Value::String(s) => Ok(Some(s.to_str()?.to_string())),
        Value::Integer(i) => Ok(Some(i.to_string())),
        Value::Number(n) => Ok(Some(subforge_core::ass::fmt_num(n))),
        _ => Ok(None),
    }
}

fn getb(t: &Table, k: &str) -> Result<Option<bool>> {
    match t.get::<Value>(k)? {
        Value::Nil => Ok(None),
        Value::Boolean(b) => Ok(Some(b)),
        Value::Integer(i) => Ok(Some(i != 0)),
        Value::Number(n) => Ok(Some(n != 0.0)),
        _ => Ok(None),
    }
}

fn geti(t: &Table, k: &str) -> Result<Option<i32>> {
    Ok(getf(t, k)?.map(|f| f.round() as i32))
}

pub fn line_to_table(lua: &Lua, line: &Line) -> Result<Table> {
    let t = lua.create_table()?;
    match line {
        Line::Info(k, v) => {
            t.set("class", "info")?;
            t.set("section", "Script Info")?;
            t.set("key", k.as_str())?;
            t.set("value", v.as_str())?;
            t.set("raw", if k == ";" { format!(";{v}") } else { format!("{k}: {v}") })?;
        }
        Line::Style(s) => {
            t.set("class", "style")?;
            t.set("section", "V4+ Styles")?;
            t.set("name", s.name.as_str())?;
            t.set("fontname", s.fontname.as_str())?;
            t.set("fontsize", s.fontsize)?;
            t.set("color1", s.color1.to_style())?;
            t.set("color2", s.color2.to_style())?;
            t.set("color3", s.color3.to_style())?;
            t.set("color4", s.color4.to_style())?;
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
        }
        Line::Event(e) => {
            t.set("class", "dialogue")?;
            t.set("section", "Events")?;
            t.set("comment", e.kind == EventKind::Comment)?;
            t.set("layer", e.layer)?;
            t.set("start_time", e.start)?;
            t.set("end_time", e.end)?;
            t.set("style", e.style.as_str())?;
            t.set("actor", e.actor.as_str())?;
            t.set("margin_l", e.margin_l)?;
            t.set("margin_r", e.margin_r)?;
            t.set("margin_t", e.margin_t)?;
            t.set("margin_b", e.margin_b)?;
            t.set("margin_v", e.margin_t)?;
            t.set("effect", e.effect.as_str())?;
            t.set("text", e.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
        }
    }
    Ok(t)
}

pub fn table_to_line(t: &Table) -> Result<Line> {
    let class = gets(t, "class")?.unwrap_or_default().to_ascii_lowercase();
    match class.as_str() {
        "dialogue" => {
            let d = Event::default();
            let margin_t = geti(t, "margin_t")?.or(geti(t, "margin_v")?).unwrap_or(0);
            Ok(Line::Event(Event {
                kind: if getb(t, "comment")?.unwrap_or(false) { EventKind::Comment } else { EventKind::Dialogue },
                layer: geti(t, "layer")?.unwrap_or(0),
                start: getf(t, "start_time")?.map(|f| f.round() as i64).unwrap_or(d.start),
                end: getf(t, "end_time")?.map(|f| f.round() as i64).unwrap_or(d.end),
                style: gets(t, "style")?.unwrap_or_else(|| "Default".into()),
                actor: gets(t, "actor")?.unwrap_or_default(),
                margin_l: geti(t, "margin_l")?.unwrap_or(0),
                margin_r: geti(t, "margin_r")?.unwrap_or(0),
                margin_t,
                margin_b: geti(t, "margin_b")?.unwrap_or(margin_t),
                effect: gets(t, "effect")?.unwrap_or_default(),
                text: gets(t, "text")?.unwrap_or_default(),
            }))
        }
        "style" => {
            let d = Style::default();
            let col = |k: &str, def: Color| -> Result<Color> { Ok(gets(t, k)?.and_then(|s| Color::parse(&s)).unwrap_or(def)) };
            let margin_t = geti(t, "margin_t")?.or(geti(t, "margin_v")?).unwrap_or(d.margin_t);
            Ok(Line::Style(Style {
                name: gets(t, "name")?.unwrap_or_else(|| "Default".into()),
                fontname: gets(t, "fontname")?.unwrap_or(d.fontname.clone()),
                fontsize: getf(t, "fontsize")?.unwrap_or(d.fontsize),
                color1: col("color1", d.color1)?,
                color2: col("color2", d.color2)?,
                color3: col("color3", d.color3)?,
                color4: col("color4", d.color4)?,
                bold: getb(t, "bold")?.unwrap_or(false),
                italic: getb(t, "italic")?.unwrap_or(false),
                underline: getb(t, "underline")?.unwrap_or(false),
                strikeout: getb(t, "strikeout")?.unwrap_or(false),
                scale_x: getf(t, "scale_x")?.unwrap_or(100.0),
                scale_y: getf(t, "scale_y")?.unwrap_or(100.0),
                spacing: getf(t, "spacing")?.unwrap_or(0.0),
                angle: getf(t, "angle")?.unwrap_or(0.0),
                borderstyle: geti(t, "borderstyle")?.unwrap_or(1),
                outline: getf(t, "outline")?.unwrap_or(d.outline),
                shadow: getf(t, "shadow")?.unwrap_or(d.shadow),
                align: geti(t, "align")?.unwrap_or(2),
                margin_l: geti(t, "margin_l")?.unwrap_or(d.margin_l),
                margin_r: geti(t, "margin_r")?.unwrap_or(d.margin_r),
                margin_t,
                margin_b: geti(t, "margin_b")?.unwrap_or(margin_t),
                encoding: geti(t, "encoding")?.unwrap_or(1),
            }))
        }
        "info" => Ok(Line::Info(gets(t, "key")?.unwrap_or_default(), gets(t, "value")?.unwrap_or_default())),
        other => Err(mlua::Error::runtime(format!("attempt to insert a line with unknown class '{other}'"))),
    }
}

/// Style table (as scripts pass to `aegisub.text_extents`) → Style.
pub fn table_to_style(t: &Table) -> Result<Style> {
    match table_to_line(&{
        // tolerate tables missing `class`
        let c = t.clone();
        if c.get::<Value>("class")?.is_nil() {
            c.set("class", "style")?;
        }
        c
    })? {
        Line::Style(s) => Ok(s),
        _ => Ok(Style::default()),
    }
}
