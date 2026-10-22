//! ASS/SSA data model, parser and writer.
//!
//! The script is stored as a flat, ordered list of lines (exactly how Aegisub's
//! Lua automation API sees it) so that scripts using `subs[i]`, `subs.insert`
//! etc. behave identically.

use crate::time;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// ASS alpha: 0 = opaque, 255 = transparent.
    pub a: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    /// Tolerant parser: `&H00BBGGRR&`, `&HBBGGRR`, `H..`, or a decimal integer.
    pub fn parse(s: &str) -> Option<Color> {
        let t = s.trim().trim_end_matches('&');
        let v: u32 = if let Some(h) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")).or_else(|| t.strip_prefix('H')).or_else(|| t.strip_prefix('h')) {
            u32::from_str_radix(h, 16).ok()?
        } else if let Some(h) = t.strip_prefix('#') {
            let rgb = u32::from_str_radix(h, 16).ok()?;
            return Some(Color::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 0));
        } else if let Ok(n) = t.parse::<i64>() {
            n as u32
        } else {
            u32::from_str_radix(t, 16).ok()?
        };
        Some(Color { a: (v >> 24) as u8, b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8 })
    }
    pub fn to_style_string(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }
    pub fn to_color_tag(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }
    pub fn to_alpha_tag(&self) -> String {
        format!("&H{:02X}&", self.a)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub color1: Color,
    pub color2: Color,
    pub color3: Color,
    pub color4: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
    pub angle: f64,
    pub borderstyle: i32,
    pub outline: f64,
    pub shadow: f64,
    pub align: i32,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub margin_b: i32,
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(),
            fontname: "Arial".into(),
            fontsize: 20.0,
            color1: Color::new(255, 255, 255, 0),
            color2: Color::new(255, 0, 0, 0),
            color3: Color::new(0, 0, 0, 0),
            color4: Color::new(0, 0, 0, 0),
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 0.0,
            angle: 0.0,
            borderstyle: 1,
            outline: 2.0,
            shadow: 2.0,
            align: 2,
            margin_l: 10,
            margin_r: 10,
            margin_t: 10,
            margin_b: 10,
            encoding: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub comment: bool,
    pub layer: i32,
    pub start_time: i64,
    pub end_time: i64,
    pub style: String,
    pub actor: String,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub margin_b: i32,
    pub effect: String,
    pub text: String,
}

impl Default for Event {
    fn default() -> Self {
        Event {
            comment: false,
            layer: 0,
            start_time: 0,
            end_time: 5000,
            style: "Default".into(),
            actor: String::new(),
            margin_l: 0,
            margin_r: 0,
            margin_t: 0,
            margin_b: 0,
            effect: String::new(),
            text: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineKind {
    /// `Key: Value` inside [Script Info] (also Aegisub Project Garbage).
    Info { key: String, value: String },
    Style(Style),
    Dialogue(Event),
    /// Anything else (comments, fonts, graphics, extradata, blank lines ...), kept verbatim.
    Raw(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub section: String,
    pub kind: LineKind,
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub lines: Vec<Line>,
}

pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.6}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

impl Script {
    pub fn new_default() -> Script {
        let mut s = Script::default();
        let info = |k: &str, v: &str| Line { section: "[Script Info]".into(), kind: LineKind::Info { key: k.into(), value: v.into() } };
        s.lines.push(info("ScriptType", "v4.00+"));
        s.lines.push(info("PlayResX", "1920"));
        s.lines.push(info("PlayResY", "1080"));
        s.lines.push(info("WrapStyle", "0"));
        s.lines.push(info("ScaledBorderAndShadow", "yes"));
        s.lines.push(Line { section: "[V4+ Styles]".into(), kind: LineKind::Style(Style::default()) });
        s
    }

    pub fn parse(text: &str) -> Script {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut script = Script::default();
        let mut section = String::new();
        let mut style_fmt: Vec<String> = default_style_format();
        let mut event_fmt: Vec<String> = default_event_format();
        let mut ssa = false;
        for raw in text.lines() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed.to_string();
                let l = section.to_ascii_lowercase();
                if l == "[v4 styles]" {
                    ssa = true;
                    section = "[V4 Styles]".into();
                }
                continue;
            }
            let sl = section.to_ascii_lowercase();
            let kind = if sl == "[script info]" || sl == "[aegisub project garbage]" {
                match line.split_once(':') {
                    Some((k, v)) if !line.starts_with(';') && !line.starts_with('!') => LineKind::Info { key: k.trim().to_string(), value: v.trim().to_string() },
                    _ => {
                        if trimmed.is_empty() {
                            continue;
                        }
                        LineKind::Raw(line.to_string())
                    }
                }
            } else if sl == "[v4+ styles]" || sl == "[v4 styles]" {
                if let Some(rest) = strip_key(line, "Format") {
                    style_fmt = split_format(rest);
                    continue;
                }
                match strip_key(line, "Style") {
                    Some(rest) => LineKind::Style(parse_style(rest, &style_fmt)),
                    None => {
                        if trimmed.is_empty() {
                            continue;
                        }
                        LineKind::Raw(line.to_string())
                    }
                }
            } else if sl == "[events]" {
                if let Some(rest) = strip_key(line, "Format") {
                    event_fmt = split_format(rest);
                    continue;
                }
                let (comment, rest) = if let Some(r) = strip_key(line, "Dialogue") {
                    (false, Some(r))
                } else if let Some(r) = strip_key(line, "Comment") {
                    (true, Some(r))
                } else {
                    (false, None)
                };
                match rest {
                    Some(r) => LineKind::Dialogue(parse_event(r, comment, &event_fmt, ssa)),
                    None => {
                        if trimmed.is_empty() {
                            continue;
                        }
                        LineKind::Raw(line.to_string())
                    }
                }
            } else {
                // Unknown section or content before any section header: keep as is.
                if trimmed.is_empty() && !sl.contains("fonts") && !sl.contains("graphics") {
                    continue;
                }
                LineKind::Raw(line.to_string())
            };
            script.lines.push(Line { section: section.clone(), kind });
        }
        script
    }

    pub fn to_ass_string(&self) -> String {
        let mut out = String::new();
        let mut cur = String::new();
        let mut first = true;
        let mut wrote_format = false;
        for l in &self.lines {
            if l.section != cur || first {
                if !first {
                    out.push('\n');
                }
                first = false;
                cur = l.section.clone();
                wrote_format = false;
                if !cur.is_empty() {
                    out.push_str(&cur);
                    out.push('\n');
                }
            }
            match &l.kind {
                LineKind::Info { key, value } => {
                    out.push_str(&format!("{key}: {value}\n"));
                }
                LineKind::Style(s) => {
                    if !wrote_format {
                        out.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
                        wrote_format = true;
                    }
                    out.push_str(&style_to_string(s));
                    out.push('\n');
                }
                LineKind::Dialogue(e) => {
                    if !wrote_format {
                        out.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
                        wrote_format = true;
                    }
                    out.push_str(&event_to_string(e));
                    out.push('\n');
                }
                LineKind::Raw(r) => {
                    out.push_str(r);
                    out.push('\n');
                }
            }
        }
        out
    }

    pub fn info(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|l| match &l.kind {
            LineKind::Info { key: k, value } if l.section == "[Script Info]" && k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn set_info(&mut self, key: &str, value: &str) {
        for l in self.lines.iter_mut() {
            if let LineKind::Info { key: k, value: v } = &mut l.kind {
                if l.section == "[Script Info]" && k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        let pos = self.lines.iter().rposition(|l| l.section == "[Script Info]").map(|p| p + 1).unwrap_or(0);
        self.lines.insert(pos, Line { section: "[Script Info]".into(), kind: LineKind::Info { key: key.into(), value: value.into() } });
    }

    pub fn play_res(&self) -> (i32, i32) {
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
        self.lines.iter().filter_map(|l| if let LineKind::Style(s) = &l.kind { Some(s) } else { None })
    }
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.lines.iter().filter_map(|l| if let LineKind::Dialogue(e) = &l.kind { Some(e) } else { None })
    }
    pub fn events_mut(&mut self) -> impl Iterator<Item = &mut Event> {
        self.lines.iter_mut().filter_map(|l| if let LineKind::Dialogue(e) = &mut l.kind { Some(e) } else { None })
    }
    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles().find(|s| s.name == name)
    }

    pub fn push_event(&mut self, e: Event) {
        self.lines.push(Line { section: "[Events]".into(), kind: LineKind::Dialogue(e) });
    }
}

fn default_style_format() -> Vec<String> {
    "Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding".split(',').map(|s| s.to_string()).collect()
}
fn default_event_format() -> Vec<String> {
    "Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text".split(',').map(|s| s.to_string()).collect()
}
fn split_format(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).collect()
}

fn strip_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let t = line.trim_start();
    if t.len() > key.len() && t[..key.len()].eq_ignore_ascii_case(key) && t[key.len()..].starts_with(':') {
        Some(t[key.len() + 1..].trim_start())
    } else {
        None
    }
}

fn parse_bool(s: &str) -> bool {
    s.trim().parse::<i32>().map(|v| v != 0).unwrap_or(false)
}
fn pf(s: &str, d: f64) -> f64 {
    s.trim().parse().unwrap_or(d)
}
fn pi(s: &str, d: i32) -> i32 {
    let t = s.trim();
    t.parse::<i32>().or_else(|_| t.parse::<f64>().map(|f| f as i32)).unwrap_or(d)
}

fn parse_style(rest: &str, fmt: &[String]) -> Style {
    let fields: Vec<&str> = rest.splitn(fmt.len().max(1), ',').collect();
    let mut st = Style::default();
    let mut margin_v: Option<i32> = None;
    for (i, name) in fmt.iter().enumerate() {
        let v = fields.get(i).copied().unwrap_or("");
        match name.to_ascii_lowercase().as_str() {
            "name" => st.name = v.trim().to_string(),
            "fontname" => st.fontname = v.trim().to_string(),
            "fontsize" => st.fontsize = pf(v, 20.0),
            "primarycolour" => st.color1 = Color::parse(v).unwrap_or(st.color1),
            "secondarycolour" => st.color2 = Color::parse(v).unwrap_or(st.color2),
            "outlinecolour" | "tertiarycolour" => st.color3 = Color::parse(v).unwrap_or(st.color3),
            "backcolour" => st.color4 = Color::parse(v).unwrap_or(st.color4),
            "bold" => st.bold = parse_bool(v),
            "italic" => st.italic = parse_bool(v),
            "underline" => st.underline = parse_bool(v),
            "strikeout" => st.strikeout = parse_bool(v),
            "scalex" => st.scale_x = pf(v, 100.0),
            "scaley" => st.scale_y = pf(v, 100.0),
            "spacing" => st.spacing = pf(v, 0.0),
            "angle" => st.angle = pf(v, 0.0),
            "borderstyle" => st.borderstyle = pi(v, 1),
            "outline" => st.outline = pf(v, 0.0),
            "shadow" => st.shadow = pf(v, 0.0),
            "alignment" => st.align = pi(v, 2),
            "marginl" => st.margin_l = pi(v, 0),
            "marginr" => st.margin_r = pi(v, 0),
            "marginv" => margin_v = Some(pi(v, 0)),
            "margint" => st.margin_t = pi(v, 0),
            "marginb" => st.margin_b = pi(v, 0),
            "encoding" => st.encoding = pi(v, 1),
            _ => {}
        }
    }
    if let Some(mv) = margin_v {
        st.margin_t = mv;
        st.margin_b = mv;
    }
    st
}

fn style_to_string(s: &Style) -> String {
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name,
        s.fontname,
        fmt_num(s.fontsize),
        s.color1.to_style_string(),
        s.color2.to_style_string(),
        s.color3.to_style_string(),
        s.color4.to_style_string(),
        if s.bold { -1 } else { 0 },
        if s.italic { -1 } else { 0 },
        if s.underline { -1 } else { 0 },
        if s.strikeout { -1 } else { 0 },
        fmt_num(s.scale_x),
        fmt_num(s.scale_y),
        fmt_num(s.spacing),
        fmt_num(s.angle),
        s.borderstyle,
        fmt_num(s.outline),
        fmt_num(s.shadow),
        s.align,
        s.margin_l,
        s.margin_r,
        s.margin_t,
        s.encoding
    )
}

fn parse_event(rest: &str, comment: bool, fmt: &[String], ssa: bool) -> Event {
    let fields: Vec<&str> = rest.splitn(fmt.len().max(1), ',').collect();
    let mut e = Event { comment, ..Default::default() };
    for (i, name) in fmt.iter().enumerate() {
        let v = fields.get(i).copied().unwrap_or("");
        match name.to_ascii_lowercase().as_str() {
            "layer" => e.layer = pi(v, 0),
            "marked" => e.layer = if ssa { 0 } else { pi(v.trim_start_matches("Marked="), 0) },
            "start" => e.start_time = time::parse_ass(v).unwrap_or(0),
            "end" => e.end_time = time::parse_ass(v).unwrap_or(0),
            "style" => e.style = v.trim().trim_start_matches('*').to_string(),
            "name" | "actor" => e.actor = v.trim().to_string(),
            "marginl" => e.margin_l = pi(v, 0),
            "marginr" => e.margin_r = pi(v, 0),
            "marginv" | "margint" => {
                e.margin_t = pi(v, 0);
                e.margin_b = e.margin_t;
            }
            "marginb" => e.margin_b = pi(v, 0),
            "effect" => e.effect = v.trim().to_string(),
            "text" => e.text = v.to_string(),
            _ => {}
        }
    }
    e
}

fn event_to_string(e: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{:04},{:04},{:04},{},{}",
        if e.comment { "Comment" } else { "Dialogue" },
        e.layer,
        time::format_ass(e.start_time),
        time::format_ass(e.end_time),
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
    const SAMPLE: &str = "[Script Info]\nTitle: x\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0000,0000,0000,,Hello, {\\i1}world{\\i0}\nComment: 1,0:00:04.00,0:00:05.00,Default,x,0,0,0,fx,note\n";
    #[test]
    fn parse_write_roundtrip() {
        let s = Script::parse(SAMPLE);
        assert_eq!(s.play_res(), (1280, 720));
        assert_eq!(s.events().count(), 2);
        assert_eq!(s.events().next().unwrap().text, "Hello, {\\i1}world{\\i0}");
        assert!(s.style("Default").unwrap().bold);
        let out = s.to_ass_string();
        let s2 = Script::parse(&out);
        assert_eq!(s.lines, s2.lines);
    }
    #[test]
    fn colors() {
        let c = Color::parse("&H00FF8040&").unwrap();
        assert_eq!((c.r, c.g, c.b), (0x40, 0x80, 0xFF));
        assert_eq!(c.to_style_string(), "&H00FF8040");
    }
}
