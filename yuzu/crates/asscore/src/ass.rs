//! ASS / SSA reader and writer. Tolerant of malformed input: unparseable lines are kept as comments in info.

use crate::model::*;
use crate::time;

pub fn parse(input: &str) -> Script {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut sc = Script::default();
    #[derive(PartialEq)]
    enum Sec { None, Info, Styles, Events, Garbage, Extra, Raw }
    let mut sec = Sec::None;
    let mut style_fmt: Vec<String> = default_style_format();
    let mut event_fmt: Vec<String> = default_event_format();
    let mut ssa = false;
    for raw in input.lines() {
        let line = raw.trim_end_matches('\r');
        let t = line.trim();
        if t.is_empty() { continue; }
        let in_raw_data = sec == Sec::Raw && sc.raw_sections.last().map(|(h, _)| h.eq_ignore_ascii_case("[Fonts]") || h.eq_ignore_ascii_case("[Graphics]")).unwrap_or(false);
        let known_header = matches!(t.to_ascii_lowercase().as_str(), "[script info]" | "[v4+ styles]" | "[v4 styles]" | "[events]" | "[fonts]" | "[graphics]" | "[aegisub project garbage]" | "[aegisub extradata]");
        if t.starts_with('[') && t.ends_with(']') && (!in_raw_data || known_header) {
            let name = t.to_ascii_lowercase();
            sec = match name.as_str() {
                "[script info]" => Sec::Info,
                "[v4+ styles]" => Sec::Styles,
                "[v4 styles]" => { ssa = true; Sec::Styles }
                "[events]" => Sec::Events,
                "[aegisub project garbage]" => Sec::Garbage,
                "[aegisub extradata]" => Sec::Extra,
                _ => { sc.raw_sections.push((t.to_string(), vec![])); Sec::Raw }
            };
            continue;
        }
        match sec {
            Sec::None => {}
            Sec::Info => {
                if let Some(c) = t.strip_prefix(';') {
                    sc.info.push((";".into(), c.trim_start().to_string()));
                } else if let Some((k, v)) = t.split_once(':') {
                    sc.info.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            Sec::Garbage => {
                if let Some((k, v)) = t.split_once(':') {
                    sc.garbage.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            Sec::Styles => {
                if let Some(f) = t.strip_prefix("Format:") {
                    style_fmt = f.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                } else if let Some(v) = t.strip_prefix("Style:") {
                    sc.styles.push(parse_style(v.trim_start(), &style_fmt, ssa));
                }
            }
            Sec::Events => {
                if let Some(f) = t.strip_prefix("Format:") {
                    event_fmt = f.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                } else if let Some(ev) = parse_event(line.trim_start(), &event_fmt) {
                    sc.events.push(ev);
                }
            }
            Sec::Extra => {
                // Data: id,key,value(inline-encoded)
                if let Some(d) = t.strip_prefix("Data:") {
                    let mut it = d.trim_start().splitn(3, ',');
                    if let (Some(id), Some(k), Some(v)) = (it.next(), it.next(), it.next()) {
                        if let Ok(id) = id.trim().parse() {
                            sc.extradata.push(Extra { id, key: k.to_string(), value: v.to_string() });
                        }
                    }
                }
            }
            Sec::Raw => {
                if let Some(last) = sc.raw_sections.last_mut() { last.1.push(line.to_string()); }
            }
        }
    }
    if ssa {
        if let Some(e) = sc.info.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("ScriptType")) { e.1 = "v4.00+".into(); }
    }
    sc
}

fn default_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,outlinecolour,backcolour,bold,italic,underline,strikeout,scalex,scaley,spacing,angle,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,encoding"
        .split(',').map(String::from).collect()
}
fn default_event_format() -> Vec<String> {
    "layer,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect()
}

fn b(v: &str) -> bool { v.trim().parse::<i32>().map(|n| n != 0).unwrap_or(false) }
fn n(v: &str, d: f64) -> f64 { v.trim().parse::<f64>().unwrap_or(d) }
fn i(v: &str, d: i32) -> i32 { v.trim().parse::<f64>().map(|x| x as i32).unwrap_or(d) }

/// SSA alignment (1-3 bottom, 5-7 top, 9-11 mid) → ASS numpad.
fn ssa_align(a: i32) -> i32 {
    match a { 1..=3 => a, 5..=7 => a - 5 + 7, 9..=11 => a - 9 + 4, _ => 2 }
}

fn parse_style(v: &str, fmt: &[String], ssa: bool) -> Style {
    let mut st = Style::default();
    let parts: Vec<&str> = v.splitn(fmt.len().max(1), ',').collect();
    for (k, val) in fmt.iter().zip(parts.iter()) {
        let val = *val;
        match k.as_str() {
            "name" => st.name = val.trim().to_string(),
            "fontname" => st.fontname = val.trim().to_string(),
            "fontsize" => st.fontsize = n(val, 48.0),
            "primarycolour" => st.primary = Color::parse(val).unwrap_or(Color::WHITE),
            "secondarycolour" => st.secondary = Color::parse(val).unwrap_or(Color::RED),
            "outlinecolour" | "tertiarycolour" => st.outline_color = Color::parse(val).unwrap_or(Color::BLACK),
            "backcolour" => st.back = Color::parse(val).unwrap_or(Color::BLACK),
            "bold" => st.bold = b(val),
            "italic" => st.italic = b(val),
            "underline" => st.underline = b(val),
            "strikeout" => st.strikeout = b(val),
            "scalex" => st.scale_x = n(val, 100.0),
            "scaley" => st.scale_y = n(val, 100.0),
            "spacing" => st.spacing = n(val, 0.0),
            "angle" => st.angle = n(val, 0.0),
            "borderstyle" => st.border_style = i(val, 1),
            "outline" => st.outline = n(val, 2.0),
            "shadow" => st.shadow = n(val, 2.0),
            "alignment" => { let a = i(val, 2); st.alignment = if ssa { ssa_align(a) } else { a } }
            "marginl" => st.margin_l = i(val, 10),
            "marginr" => st.margin_r = i(val, 10),
            "marginv" => st.margin_v = i(val, 10),
            "encoding" => st.encoding = i(val, 1),
            _ => {}
        }
    }
    st
}

fn parse_event(line: &str, fmt: &[String]) -> Option<Event> {
    let (comment, rest) = if let Some(r) = line.strip_prefix("Dialogue:") { (false, r) }
        else if let Some(r) = line.strip_prefix("Comment:") { (true, r) }
        else { return None };
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    let parts: Vec<&str> = rest.splitn(fmt.len().max(1), ',').collect();
    let mut ev = Event { comment, ..Default::default() };
    for (k, val) in fmt.iter().zip(parts.iter()) {
        let val = *val;
        match k.as_str() {
            "layer" | "marked" => ev.layer = i(&val.replace("Marked=", ""), 0),
            "start" => ev.start = time::parse_ass(val)?,
            "end" => ev.end = time::parse_ass(val)?,
            "style" => ev.style = val.trim().trim_start_matches('*').to_string(),
            "name" | "actor" => ev.actor = val.trim().to_string(),
            "marginl" => ev.margin_l = i(val, 0),
            "marginr" => ev.margin_r = i(val, 0),
            "marginv" => ev.margin_v = i(val, 0),
            "effect" => ev.effect = val.trim().to_string(),
            "text" => ev.text = val.to_string(),
            _ => {}
        }
    }
    Some(ev)
}

pub fn write(sc: &Script) -> String {
    let mut o = String::with_capacity(sc.events.len() * 96 + 2048);
    o.push_str("[Script Info]\n");
    for (k, v) in &sc.info {
        if k == ";" { o.push_str(&format!("; {}\n", v)); } else { o.push_str(&format!("{}: {}\n", k, v)); }
    }
    if !sc.garbage.is_empty() {
        o.push_str("\n[Aegisub Project Garbage]\n");
        for (k, v) in &sc.garbage { o.push_str(&format!("{}: {}\n", k, v)); }
    }
    o.push_str("\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    for s in &sc.styles { o.push_str(&style_line(s)); o.push('\n'); }
    o.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for e in &sc.events { o.push_str(&e.to_ass()); o.push('\n'); }
    for (h, lines) in &sc.raw_sections {
        o.push_str(&format!("\n{}\n", h));
        for l in lines { o.push_str(l); o.push('\n'); }
    }
    if !sc.extradata.is_empty() {
        o.push_str("\n[Aegisub Extradata]\n");
        for x in &sc.extradata { o.push_str(&format!("Data: {},{},{}\n", x.id, x.key, x.value)); }
    }
    o
}

pub fn style_line(s: &Style) -> String {
    let bi = |v: bool| if v { "-1" } else { "0" };
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name, s.fontname, fmt_num(s.fontsize),
        s.primary.to_style(), s.secondary.to_style(), s.outline_color.to_style(), s.back.to_style(),
        bi(s.bold), bi(s.italic), bi(s.underline), bi(s.strikeout),
        fmt_num(s.scale_x), fmt_num(s.scale_y), fmt_num(s.spacing), fmt_num(s.angle),
        s.border_style, fmt_num(s.outline), fmt_num(s.shadow), s.alignment,
        s.margin_l, s.margin_r, s.margin_v, s.encoding
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "\u{feff}[Script Info]\n; Comment\nTitle: Test\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, {\\i1}world{\\i0}\nComment: 0,0:00:04.00,0:00:05.00,Default,x,0,0,0,fx,note\n";
    #[test]
    fn parse_write_roundtrip() {
        let s = parse(SAMPLE);
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "Hello, {\\i1}world{\\i0}");
        assert!(s.events[1].comment);
        assert_eq!(s.styles[0].bold, true);
        assert_eq!(s.res(), (1280, 720));
        let out = write(&s);
        let s2 = parse(&out);
        assert_eq!(s2.events, s.events);
        assert_eq!(s2.styles, s.styles);
        assert_eq!(s2.info, s.info);
    }
    #[test]
    fn ssa_alignment() {
        assert_eq!(ssa_align(6), 8);
        assert_eq!(ssa_align(10), 5);
        assert_eq!(ssa_align(2), 2);
    }
}
