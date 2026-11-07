//! Flattened file view as exposed to Aegisub scripts, and Lua table conversion.
use mlua::{Lua, Result, Table, Value};
use osiosub_core::ass::{AssFile, Event, Style};
use osiosub_core::color::Color;

#[derive(Clone, Debug)]
pub enum Line {
    Info(String, String),
    Style(Style),
    Event(Event),
}

pub fn flatten(f: &AssFile) -> Vec<Line> {
    let mut v = Vec::with_capacity(f.info.len() + f.styles.len() + f.events.len());
    for (k, val) in &f.info { v.push(Line::Info(k.clone(), val.clone())); }
    for s in &f.styles { v.push(Line::Style(s.clone())); }
    for e in &f.events { v.push(Line::Event(e.clone())); }
    v
}

/// Rebuild a file from the flat list, keeping non-script-visible parts of `base`.
pub fn unflatten(lines: &[Line], base: &AssFile) -> AssFile {
    let mut f = AssFile { garbage: base.garbage.clone(), extra: base.extra.clone(), ..Default::default() };
    for l in lines {
        match l {
            Line::Info(k, v) => f.info.push((k.clone(), v.clone())),
            Line::Style(s) => f.styles.push(s.clone()),
            Line::Event(e) => f.events.push(e.clone()),
        }
    }
    f
}

fn col(c: Color) -> String { format!("&H{:08X}&", c.0) }

pub fn to_table(lua: &Lua, l: &Line) -> Result<Table> {
    let t = lua.create_table()?;
    match l {
        Line::Info(k, v) => {
            t.set("class", "info")?; t.set("section", "[Script Info]")?;
            t.set("key", k.as_str())?; t.set("value", v.as_str())?;
            t.set("raw", if k == ";" { format!("; {}", v) } else { format!("{}: {}", k, v) })?;
        }
        Line::Style(s) => {
            t.set("class", "style")?; t.set("section", "[V4+ Styles]")?;
            t.set("name", s.name.as_str())?; t.set("fontname", s.fontname.as_str())?; t.set("fontsize", s.fontsize)?;
            t.set("color1", col(s.primary))?; t.set("color2", col(s.secondary))?;
            t.set("color3", col(s.outline_color))?; t.set("color4", col(s.back))?;
            t.set("bold", s.bold)?; t.set("italic", s.italic)?; t.set("underline", s.underline)?; t.set("strikeout", s.strikeout)?;
            t.set("scale_x", s.scale_x)?; t.set("scale_y", s.scale_y)?; t.set("spacing", s.spacing)?; t.set("angle", s.angle)?;
            t.set("borderstyle", s.border_style)?; t.set("outline", s.outline)?; t.set("shadow", s.shadow)?;
            t.set("align", s.alignment)?;
            t.set("margin_l", s.margin_l)?; t.set("margin_r", s.margin_r)?;
            t.set("margin_t", s.margin_v)?; t.set("margin_b", s.margin_v)?; t.set("margin_v", s.margin_v)?;
            t.set("encoding", s.encoding)?; t.set("relative_to", 2)?; t.set("vertical", false)?;
            t.set("raw", format!("Style: {}", osiosub_core::ass::style_line(s)))?;
        }
        Line::Event(e) => {
            t.set("class", "dialogue")?; t.set("section", "[Events]")?;
            t.set("comment", e.comment)?; t.set("layer", e.layer)?;
            t.set("start_time", e.start as f64)?; t.set("end_time", e.end as f64)?;
            t.set("style", e.style.as_str())?; t.set("actor", e.actor.as_str())?;
            t.set("margin_l", e.margin_l)?; t.set("margin_r", e.margin_r)?;
            t.set("margin_t", e.margin_v)?; t.set("margin_b", e.margin_v)?; t.set("margin_v", e.margin_v)?;
            t.set("effect", e.effect.as_str())?; t.set("text", e.text.as_str())?;
            t.set("extra", lua.create_table()?)?;
            t.set("raw", osiosub_core::ass::event_line(e))?;
        }
    }
    Ok(t)
}

fn s(t: &Table, k: &str) -> Option<String> {
    match t.get::<Value>(k).ok()? { Value::String(x) => x.to_str().ok().map(|x| x.to_string()), Value::Integer(i) => Some(i.to_string()), Value::Number(n) => Some(n.to_string()), _ => None }
}
fn n(t: &Table, k: &str) -> Option<f64> {
    match t.get::<Value>(k).ok()? { Value::Integer(i) => Some(i as f64), Value::Number(n) => Some(n), Value::String(x) => x.to_str().ok()?.trim().parse().ok(), _ => None }
}
fn b(t: &Table, k: &str) -> Option<bool> {
    match t.get::<Value>(k).ok()? { Value::Boolean(x) => Some(x), Value::Nil => None, Value::Integer(i) => Some(i != 0), Value::Number(x) => Some(x != 0.0), _ => None }
}

pub fn from_table(t: &Table) -> Result<Line> {
    let class = s(t, "class").unwrap_or_else(|| "dialogue".into());
    Ok(match class.as_str() {
        "info" => Line::Info(s(t, "key").unwrap_or_default(), s(t, "value").unwrap_or_default()),
        "style" => {
            let mut st = Style::default();
            if let Some(v) = s(t, "name") { st.name = v; }
            if let Some(v) = s(t, "fontname") { st.fontname = v; }
            if let Some(v) = n(t, "fontsize") { st.fontsize = v; }
            for (k, dst) in [("color1", &mut st.primary), ("color2", &mut st.secondary), ("color3", &mut st.outline_color), ("color4", &mut st.back)] {
                if let Some(c) = s(t, k).and_then(|v| Color::parse(&v)) { *dst = c; }
            }
            if let Some(v) = b(t, "bold") { st.bold = v; }
            if let Some(v) = b(t, "italic") { st.italic = v; }
            if let Some(v) = b(t, "underline") { st.underline = v; }
            if let Some(v) = b(t, "strikeout") { st.strikeout = v; }
            if let Some(v) = n(t, "scale_x") { st.scale_x = v; }
            if let Some(v) = n(t, "scale_y") { st.scale_y = v; }
            if let Some(v) = n(t, "spacing") { st.spacing = v; }
            if let Some(v) = n(t, "angle") { st.angle = v; }
            if let Some(v) = n(t, "borderstyle") { st.border_style = v as i32; }
            if let Some(v) = n(t, "outline") { st.outline = v; }
            if let Some(v) = n(t, "shadow") { st.shadow = v; }
            if let Some(v) = n(t, "align") { st.alignment = v as i32; }
            if let Some(v) = n(t, "margin_l") { st.margin_l = v as i32; }
            if let Some(v) = n(t, "margin_r") { st.margin_r = v as i32; }
            if let Some(v) = n(t, "margin_t").or(n(t, "margin_v")) { st.margin_v = v as i32; }
            if let Some(v) = n(t, "encoding") { st.encoding = v as i32; }
            Line::Style(st)
        }
        _ => {
            let mut e = Event::default();
            if let Some(v) = b(t, "comment") { e.comment = v; }
            if let Some(v) = n(t, "layer") { e.layer = v as i32; }
            if let Some(v) = n(t, "start_time") { e.start = v.round() as i64; }
            if let Some(v) = n(t, "end_time") { e.end = v.round() as i64; }
            if let Some(v) = s(t, "style") { e.style = v; }
            if let Some(v) = s(t, "actor") { e.actor = v; }
            if let Some(v) = n(t, "margin_l") { e.margin_l = v as i32; }
            if let Some(v) = n(t, "margin_r") { e.margin_r = v as i32; }
            if let Some(v) = n(t, "margin_t").or(n(t, "margin_v")) { e.margin_v = v as i32; }
            if let Some(v) = s(t, "effect") { e.effect = v; }
            if let Some(v) = s(t, "text") { e.text = v; }
            Line::Event(e)
        }
    })
}
