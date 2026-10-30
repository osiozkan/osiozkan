//! ASS/SSA okuma ve yazma. Sütun sırası `Format:` satırından okunur; bilinmeyen bölümler korunur.
use crate::model::{Document, Event, Style};
use crate::time;

fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn split_fields(line: &str, n: usize) -> Vec<&str> {
    line.splitn(n, ',').collect()
}

fn cols(format: &str) -> Vec<String> {
    format.split(',').map(|c| c.trim().to_ascii_lowercase()).collect()
}

pub fn parse(input: &str) -> Document {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut doc = Document::default();
    let mut section = String::new();
    let mut style_fmt: Vec<String> = cols(
        "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding",
    );
    let mut ev_fmt: Vec<String> =
        cols("Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");
    let mut extra_idx: Option<usize> = None;

    for raw in input.lines() {
        let line = raw.trim_end_matches('\r');
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            extra_idx = None;
            let l = section.to_ascii_lowercase();
            if !matches!(l.as_str(), "script info" | "v4+ styles" | "v4 styles" | "events") {
                doc.extra_sections.push((section.clone(), Vec::new()));
                extra_idx = Some(doc.extra_sections.len() - 1);
            }
            continue;
        }
        if let Some(i) = extra_idx {
            doc.extra_sections[i].1.push(line.to_string());
            continue;
        }
        if t.is_empty() {
            continue;
        }
        match section.to_ascii_lowercase().as_str() {
            "script info" => {
                if t.starts_with(';') || t.starts_with('!') {
                    continue;
                }
                if let Some((k, v)) = t.split_once(':') {
                    doc.info.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            "v4+ styles" | "v4 styles" => {
                let Some((key, rest)) = t.split_once(':') else { continue };
                match key.trim().to_ascii_lowercase().as_str() {
                    "format" => style_fmt = cols(rest),
                    "style" => doc.styles.push(parse_style(rest.trim_start(), &style_fmt)),
                    _ => {}
                }
            }
            "events" => {
                let Some((key, rest)) = t.split_once(':') else { continue };
                let k = key.trim().to_ascii_lowercase();
                match k.as_str() {
                    "format" => ev_fmt = cols(rest),
                    "dialogue" | "comment" => {
                        let rest = rest.strip_prefix(' ').unwrap_or(rest);
                        doc.events.push(parse_event(rest, &ev_fmt, k == "comment"));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if doc.info_get("ScriptType").is_none() {
        doc.info.insert(0, ("ScriptType".into(), "v4.00+".into()));
    }
    doc
}

fn parse_style(s: &str, fmt: &[String]) -> Style {
    let f = split_fields(s, fmt.len());
    let mut st = Style::default();
    for (i, c) in fmt.iter().enumerate() {
        let Some(v) = f.get(i) else { break };
        let v = v.trim();
        match c.as_str() {
            "name" => st.name = v.to_string(),
            "fontname" => st.fontname = v.to_string(),
            "fontsize" => st.fontsize = num(v),
            "primarycolour" => st.primary = v.to_string(),
            "secondarycolour" => st.secondary = v.to_string(),
            "outlinecolour" | "tertiarycolour" => st.outline_color = v.to_string(),
            "backcolour" => st.back = v.to_string(),
            "bold" => st.bold = num(v) != 0.0,
            "italic" => st.italic = num(v) != 0.0,
            "underline" => st.underline = num(v) != 0.0,
            "strikeout" => st.strikeout = num(v) != 0.0,
            "scalex" => st.scale_x = num(v),
            "scaley" => st.scale_y = num(v),
            "spacing" => st.spacing = num(v),
            "angle" => st.angle = num(v),
            "borderstyle" => st.border_style = num(v) as i32,
            "outline" => st.outline = num(v),
            "shadow" => st.shadow = num(v),
            "alignment" => st.alignment = num(v) as i32,
            "marginl" => st.margin_l = num(v) as i32,
            "marginr" => st.margin_r = num(v) as i32,
            "marginv" => st.margin_v = num(v) as i32,
            "encoding" => st.encoding = num(v) as i32,
            _ => {}
        }
    }
    st
}

fn parse_event(s: &str, fmt: &[String], comment: bool) -> Event {
    let f = split_fields(s, fmt.len());
    let mut e = Event { comment, ..Event::default() };
    for (i, c) in fmt.iter().enumerate() {
        let Some(v) = f.get(i) else { break };
        match c.as_str() {
            "layer" | "marked" => e.layer = v.trim().trim_start_matches("Marked=").parse().unwrap_or(0),
            "start" => e.start = time::parse_ass(v).unwrap_or(0),
            "end" => e.end = time::parse_ass(v).unwrap_or(0),
            "style" => e.style = v.trim().to_string(),
            "name" | "actor" => e.actor = v.trim().to_string(),
            "marginl" => e.margin_l = num(v) as i32,
            "marginr" => e.margin_r = num(v) as i32,
            "marginv" | "margint" => e.margin_t = num(v) as i32,
            "effect" => e.effect = v.trim().to_string(),
            "text" => e.text = v.to_string(),
            _ => {}
        }
    }
    e
}

fn b(v: bool) -> &'static str {
    if v { "-1" } else { "0" }
}

pub fn style_line(s: &Style) -> String {
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name, s.fontname, fmt_num(s.fontsize), s.primary, s.secondary, s.outline_color, s.back,
        b(s.bold), b(s.italic), b(s.underline), b(s.strikeout),
        fmt_num(s.scale_x), fmt_num(s.scale_y), fmt_num(s.spacing), fmt_num(s.angle),
        s.border_style, fmt_num(s.outline), fmt_num(s.shadow), s.alignment,
        s.margin_l, s.margin_r, s.margin_v, s.encoding
    )
}

pub fn event_line(e: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{},{},{},{},{}",
        if e.comment { "Comment" } else { "Dialogue" },
        e.layer, time::fmt_ass(e.start), time::fmt_ass(e.end), e.style, e.actor,
        e.margin_l, e.margin_r, e.margin_t, e.effect, e.text
    )
}

pub fn write(doc: &Document) -> String {
    let mut o = String::new();
    o.push_str("[Script Info]\n");
    if doc.info_get("ScriptType").is_none() {
        o.push_str("ScriptType: v4.00+\n");
    }
    for (k, v) in &doc.info {
        o.push_str(&format!("{}: {}\n", k, v));
    }
    o.push_str("\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    for s in &doc.styles {
        o.push_str(&style_line(s));
        o.push('\n');
    }
    o.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for e in &doc.events {
        o.push_str(&event_line(e));
        o.push('\n');
    }
    for (name, lines) in &doc.extra_sections {
        o.push_str(&format!("\n[{}]\n", name));
        for l in lines {
            o.push_str(l);
            o.push('\n');
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "\u{feff}[Script Info]\nTitle: t\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,1.5,2,10,10,25,162\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,Ali,0,0,0,,Merhaba, dünya{\\i1}!{\\i0}\nComment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,not\n\n[Fonts]\nfontname: x.ttf\nABCD\n";
    #[test]
    fn parse_write_roundtrip() {
        let d = parse(SAMPLE);
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].text, "Merhaba, dünya{\\i1}!{\\i0}");
        assert!(d.events[1].comment);
        assert_eq!(d.styles[0].shadow, 1.5);
        assert!(d.styles[0].bold);
        let w = write(&d);
        let d2 = parse(&w);
        assert_eq!(d, d2);
        assert_eq!(d2.extra_sections[0].1[1], "ABCD");
    }
}
