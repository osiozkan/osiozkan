//! ASS/SSA document model, parser and writer.
//!
//! The document is a flat list of [`Line`]s in file order, which is exactly
//! the view Aegisub's Automation 4 exposes (`subs[i]`).

use crate::{color, time, time::Ms};

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub primary: u32,
    pub secondary: u32,
    pub outline: u32,
    pub back: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
    pub angle: f64,
    pub border_style: i32,
    pub outline_w: f64,
    pub shadow: f64,
    pub align: i32,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(),
            fontname: "Arial".into(),
            fontsize: 48.0,
            primary: 0x00FFFFFF,
            secondary: 0x000000FF,
            outline: 0x00000000,
            back: 0x00000000,
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 0.0,
            angle: 0.0,
            border_style: 1,
            outline_w: 2.0,
            shadow: 2.0,
            align: 2,
            margin_l: 10,
            margin_r: 10,
            margin_t: 10,
            encoding: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub comment: bool,
    pub layer: i32,
    pub start: Ms,
    pub end: Ms,
    pub style: String,
    pub actor: String,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub effect: String,
    pub text: String,
}

impl Default for Event {
    fn default() -> Self {
        Event {
            comment: false,
            layer: 0,
            start: 0,
            end: 5000,
            style: "Default".into(),
            actor: String::new(),
            margin_l: 0,
            margin_r: 0,
            margin_t: 0,
            effect: String::new(),
            text: String::new(),
        }
    }
}

impl Event {
    pub fn duration(&self) -> Ms {
        self.end - self.start
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// `[Script Info]` key/value (comments are stored with key `;`).
    Info { key: String, value: String },
    Style(Style),
    Dialogue(Event),
    /// Any other section content (`[Fonts]`, `[Graphics]`, `[Aegisub Project Garbage]`, ...).
    Raw { section: String, text: String },
}

impl Line {
    pub fn section(&self) -> &str {
        match self {
            Line::Info { .. } => "Script Info",
            Line::Style(_) => "V4+ Styles",
            Line::Dialogue(_) => "Events",
            Line::Raw { section, .. } => section,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssFile {
    pub lines: Vec<Line>,
}

const STYLE_FORMAT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
const EVENT_FORMAT: &str =
    "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";

fn split_format(s: &str) -> Vec<String> {
    s.split(',').map(|f| f.trim().to_ascii_lowercase()).collect()
}

fn b(s: &str) -> bool {
    s.trim().parse::<i32>().map(|v| v != 0).unwrap_or(false)
}
fn fnum(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}
fn inum(s: &str) -> i32 {
    let t = s.trim();
    t.parse().or_else(|_| t.parse::<f64>().map(|f| f as i32)).unwrap_or(0)
}

impl AssFile {
    pub fn new_default() -> Self {
        let mut f = AssFile::default();
        for (k, v) in [
            ("ScriptType", "v4.00+"),
            ("WrapStyle", "0"),
            ("ScaledBorderAndShadow", "yes"),
            ("PlayResX", "1920"),
            ("PlayResY", "1080"),
        ] {
            f.set_info(k, v);
        }
        f.lines.push(Line::Style(Style::default()));
        f
    }

    pub fn parse(text: &str) -> AssFile {
        let mut file = AssFile::default();
        let mut section = String::new();
        let mut style_fmt = split_format(STYLE_FORMAT);
        let mut event_fmt = split_format(EVENT_FORMAT);
        for raw in text.trim_start_matches('\u{feff}').split('\n') {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].to_string();
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            let lsec = section.to_ascii_lowercase();
            match lsec.as_str() {
                "script info" => {
                    if let Some(rest) = trimmed.strip_prefix(';') {
                        file.lines.push(Line::Info { key: ";".into(), value: rest.to_string() });
                    } else if let Some((k, v)) = trimmed.split_once(':') {
                        file.lines.push(Line::Info {
                            key: k.trim().to_string(),
                            value: v.trim().to_string(),
                        });
                    }
                }
                "v4+ styles" | "v4 styles" | "v4 styles+" => {
                    if let Some(f) = strip_key(trimmed, "Format") {
                        style_fmt = split_format(f);
                    } else if let Some(v) = strip_key(trimmed, "Style") {
                        file.lines.push(Line::Style(parse_style(v, &style_fmt)));
                    }
                }
                "events" => {
                    if let Some(f) = strip_key(trimmed, "Format") {
                        event_fmt = split_format(f);
                    } else if let Some(v) = strip_key(trimmed, "Dialogue") {
                        file.lines.push(Line::Dialogue(parse_event(v, false, &event_fmt)));
                    } else if let Some(v) = strip_key(trimmed, "Comment") {
                        file.lines.push(Line::Dialogue(parse_event(v, true, &event_fmt)));
                    }
                }
                "" => {}
                _ => file.lines.push(Line::Raw { section: section.clone(), text: line.to_string() }),
            }
        }
        file
    }

    pub fn to_ass_string(&self) -> String {
        let mut order: Vec<String> = vec!["Script Info".into()];
        for l in &self.lines {
            let s = l.section().to_string();
            if !order.contains(&s) {
                order.push(s);
            }
        }
        // Canonical Aegisub order for the well-known sections.
        let rank = |s: &str| match s {
            "Script Info" => 0,
            "Aegisub Project Garbage" => 1,
            "V4+ Styles" => 2,
            "Fonts" => 3,
            "Graphics" => 4,
            "Events" => 5,
            _ => 6,
        };
        let mut sections = order;
        sections.sort_by_key(|s| rank(s)); // stable: unknown sections keep file order
        let mut out = String::new();
        for sec in sections {
            let body: Vec<&Line> = self.lines.iter().filter(|l| l.section() == sec).collect();
            if body.is_empty() && sec != "Script Info" && sec != "V4+ Styles" && sec != "Events" {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{sec}]\n"));
            match sec.as_str() {
                "V4+ Styles" => {
                    out.push_str(&format!("Format: {STYLE_FORMAT}\n"));
                }
                "Events" => {
                    out.push_str(&format!("Format: {EVENT_FORMAT}\n"));
                }
                _ => {}
            }
            for l in body {
                match l {
                    Line::Info { key, value } if key == ";" => {
                        out.push_str(&format!(";{value}\n"));
                    }
                    Line::Info { key, value } => out.push_str(&format!("{key}: {value}\n")),
                    Line::Style(s) => out.push_str(&format!("Style: {}\n", write_style(s))),
                    Line::Dialogue(e) => out.push_str(&write_event(e)),
                    Line::Raw { text, .. } => {
                        out.push_str(text);
                        out.push('\n');
                    }
                }
            }
        }
        out
    }

    pub fn info(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|l| match l {
            Line::Info { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn set_info(&mut self, key: &str, value: &str) {
        for l in &mut self.lines {
            if let Line::Info { key: k, value: v } = l {
                if k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        let pos = self
            .lines
            .iter()
            .rposition(|l| matches!(l, Line::Info { .. }))
            .map(|p| p + 1)
            .unwrap_or(0);
        self.lines.insert(pos, Line::Info { key: key.into(), value: value.into() });
    }

    pub fn res(&self) -> (i32, i32) {
        let x = self.info("PlayResX").and_then(|v| v.parse().ok());
        let y = self.info("PlayResY").and_then(|v| v.parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => (x, if x == 1280 { 1024 } else { x * 3 / 4 }),
            (None, Some(y)) => (if y == 1024 { 1280 } else { y * 4 / 3 }, y),
            _ => (384, 288),
        }
    }

    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.lines.iter().filter_map(|l| if let Line::Style(s) = l { Some(s) } else { None })
    }

    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles().find(|s| s.name == name)
    }

    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.lines.iter().filter_map(|l| if let Line::Dialogue(e) = l { Some(e) } else { None })
    }

    pub fn events_mut(&mut self) -> impl Iterator<Item = &mut Event> {
        self.lines.iter_mut().filter_map(|l| if let Line::Dialogue(e) = l { Some(e) } else { None })
    }
}

fn strip_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (k, v) = line.split_once(':')?;
    if k.trim().eq_ignore_ascii_case(key) {
        Some(v.trim_start())
    } else {
        None
    }
}

fn parse_style(v: &str, fmt: &[String]) -> Style {
    let mut s = Style::default();
    let parts: Vec<&str> = v.splitn(fmt.len().max(1), ',').collect();
    for (name, val) in fmt.iter().zip(parts) {
        let val = val.trim();
        match name.as_str() {
            "name" => s.name = val.to_string(),
            "fontname" => s.fontname = val.to_string(),
            "fontsize" => s.fontsize = fnum(val),
            "primarycolour" => s.primary = color::parse(val).unwrap_or(s.primary),
            "secondarycolour" => s.secondary = color::parse(val).unwrap_or(s.secondary),
            "outlinecolour" | "tertiarycolour" => s.outline = color::parse(val).unwrap_or(s.outline),
            "backcolour" => s.back = color::parse(val).unwrap_or(s.back),
            "bold" => s.bold = b(val),
            "italic" => s.italic = b(val),
            "underline" => s.underline = b(val),
            "strikeout" => s.strikeout = b(val),
            "scalex" => s.scale_x = fnum(val),
            "scaley" => s.scale_y = fnum(val),
            "spacing" => s.spacing = fnum(val),
            "angle" => s.angle = fnum(val),
            "borderstyle" => s.border_style = inum(val),
            "outline" => s.outline_w = fnum(val),
            "shadow" => s.shadow = fnum(val),
            "alignment" => s.align = inum(val),
            "marginl" => s.margin_l = inum(val),
            "marginr" => s.margin_r = inum(val),
            "marginv" => s.margin_t = inum(val),
            "encoding" => s.encoding = inum(val),
            _ => {}
        }
    }
    s
}

fn parse_event(v: &str, comment: bool, fmt: &[String]) -> Event {
    let mut e = Event { comment, ..Event::default() };
    // `Text` is always last and may contain commas.
    let parts: Vec<&str> = v.splitn(fmt.len().max(1), ',').collect();
    for (name, val) in fmt.iter().zip(parts) {
        match name.as_str() {
            "layer" | "marked" => e.layer = inum(&val.replace("Marked=", "")),
            "start" => e.start = time::parse(val).unwrap_or(0),
            "end" => e.end = time::parse(val).unwrap_or(0),
            "style" => e.style = val.trim().to_string(),
            "name" | "actor" => e.actor = val.trim().to_string(),
            "marginl" => e.margin_l = inum(val),
            "marginr" => e.margin_r = inum(val),
            "marginv" => e.margin_t = inum(val),
            "effect" => e.effect = val.trim().to_string(),
            "text" => e.text = val.to_string(),
            _ => {}
        }
    }
    e
}

fn num(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{}", f as i64)
    } else {
        let s = format!("{f:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn write_style(s: &Style) -> String {
    let flag = |v: bool| if v { "-1" } else { "0" };
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name,
        s.fontname,
        num(s.fontsize),
        color::format(s.primary),
        color::format(s.secondary),
        color::format(s.outline),
        color::format(s.back),
        flag(s.bold),
        flag(s.italic),
        flag(s.underline),
        flag(s.strikeout),
        num(s.scale_x),
        num(s.scale_y),
        num(s.spacing),
        num(s.angle),
        s.border_style,
        num(s.outline_w),
        num(s.shadow),
        s.align,
        s.margin_l,
        s.margin_r,
        s.margin_t,
        s.encoding
    )
}

pub fn write_event(e: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{:04},{:04},{:04},{},{}\n",
        if e.comment { "Comment" } else { "Dialogue" },
        e.layer,
        time::format_ass(e.start),
        time::format_ass(e.end),
        e.style,
        e.actor,
        e.margin_l,
        e.margin_r,
        e.margin_t,
        e.effect,
        e.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[Script Info]\n; hi\nTitle: T\nPlayResX: 1280\nPlayResY: 720\n\n[Aegisub Project Garbage]\nVideo File: a.mkv\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,Bob,0,0,0,,Merhaba, dünya\\Nİkinci\nComment: 1,0:00:04.00,0:00:05.00,Default,,0,0,0,,{\\k10}ko\n";

    #[test]
    fn parse_and_roundtrip() {
        let f = AssFile::parse(SAMPLE);
        assert_eq!(f.res(), (1280, 720));
        assert_eq!(f.style("Default").unwrap().bold, true);
        let ev: Vec<_> = f.events().collect();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].text, "Merhaba, dünya\\Nİkinci");
        assert_eq!(ev[0].end, 3500);
        assert!(ev[1].comment);
        let out = f.to_ass_string();
        let g = AssFile::parse(&out);
        assert_eq!(f, g);
        assert!(out.contains("[Aegisub Project Garbage]\nVideo File: a.mkv"));
    }

    #[test]
    fn reordered_format() {
        let f = AssFile::parse("[Events]\nFormat: Start, End, Text, Style\nDialogue: 0:00:01.00,0:00:02.00,a,b,Sty\n");
        let e = f.events().next().unwrap();
        // Text is not last here, so splitn(4) puts the remainder in the last field.
        assert_eq!(e.start, 1000);
        assert_eq!(e.text, "a");
    }
}
