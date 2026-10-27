//! ASS/SSA document model. The script is a *flat* ordered list of entries, which is
//! exactly how Aegisub's automation API sees a file (`subs[i]`), so Lua scripts map 1:1.

use crate::color::Color;
use crate::time::{format_ass_time, parse_ass_time};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
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
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(),
            fontname: "Arial".into(),
            fontsize: 48.0,
            color1: Color::WHITE,
            color2: Color { a: 0, b: 0, g: 0, r: 255 },
            color3: Color::BLACK,
            color4: Color::BLACK,
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
            encoding: 1,
        }
    }
}

pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.3}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn b(v: bool) -> &'static str {
    if v { "-1" } else { "0" }
}

impl Style {
    pub fn to_line(&self) -> String {
        format!(
            "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.name, self.fontname, fmt_num(self.fontsize), self.color1, self.color2, self.color3, self.color4,
            b(self.bold), b(self.italic), b(self.underline), b(self.strikeout),
            fmt_num(self.scale_x), fmt_num(self.scale_y), fmt_num(self.spacing), fmt_num(self.angle),
            self.borderstyle, fmt_num(self.outline), fmt_num(self.shadow), self.align,
            self.margin_l, self.margin_r, self.margin_t, self.encoding
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dialogue,
    Comment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub layer: i32,
    pub start: i64,
    pub end: i64,
    pub style: String,
    pub actor: String,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub effect: String,
    pub text: String,
}

impl Default for Line {
    fn default() -> Self {
        Line {
            kind: Kind::Dialogue,
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

impl Line {
    pub fn new(start: i64, end: i64, text: impl Into<String>) -> Self {
        Line { start, end, text: text.into(), ..Default::default() }
    }
    pub fn duration(&self) -> i64 {
        self.end - self.start
    }
    pub fn is_comment(&self) -> bool {
        self.kind == Kind::Comment
    }
    pub fn to_line(&self) -> String {
        format!(
            "{}: {},{},{},{},{},{},{},{},{},{}",
            if self.is_comment() { "Comment" } else { "Dialogue" },
            self.layer, format_ass_time(self.start), format_ass_time(self.end), self.style, self.actor,
            self.margin_l, self.margin_r, self.margin_t, self.effect, self.text
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Info { key: String, value: String },
    Style(Style),
    Line(Line),
    /// Anything we don't model: comments in headers, [Fonts], [Graphics], [Aegisub Extradata]…
    Raw { section: String, text: String },
}

impl Entry {
    pub fn section(&self) -> &str {
        match self {
            Entry::Info { .. } => "Script Info",
            Entry::Style(_) => "V4+ Styles",
            Entry::Line(_) => "Events",
            Entry::Raw { section, .. } => section,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Script {
    pub entries: Vec<Entry>,
}

const DIALOGUE_FORMAT: &str = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";
const STYLE_FORMAT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";

fn split_n(s: &str, n: usize) -> Vec<&str> {
    s.splitn(n, ',').collect()
}

fn pf(s: &str, d: f64) -> f64 {
    s.trim().parse().unwrap_or(d)
}
fn pi(s: &str, d: i32) -> i32 {
    let t = s.trim();
    t.parse().unwrap_or_else(|_| t.parse::<f64>().map(|f| f as i32).unwrap_or(d))
}
fn pb(s: &str) -> bool {
    matches!(s.trim(), "-1" | "1") || s.trim().parse::<i32>().map(|v| v != 0).unwrap_or(false)
}

/// Convert SSA (v4) alignment numbers to ASS numpad alignment.
fn ssa_align(a: i32) -> i32 {
    match a {
        1..=3 => a,
        5..=7 => a + 2, // top: 5,6,7 -> 7,8,9
        9..=11 => a - 5, // middle: 9,10,11 -> 4,5,6
        _ => 2,
    }
}

impl Script {
    pub fn new_default() -> Script {
        let mut s = Script::default();
        for (k, v) in [
            ("ScriptType", "v4.00+"),
            ("PlayResX", "1920"),
            ("PlayResY", "1080"),
            ("WrapStyle", "0"),
            ("ScaledBorderAndShadow", "yes"),
            ("YCbCr Matrix", "TV.709"),
        ] {
            s.entries.push(Entry::Info { key: k.into(), value: v.into() });
        }
        s.entries.push(Entry::Style(Style::default()));
        s
    }

    pub fn parse(input: &str) -> Script {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut entries = Vec::new();
        let mut section = String::new();
        let mut style_fmt: Vec<String> = split_fmt(STYLE_FORMAT);
        let mut ev_fmt: Vec<String> = split_fmt(DIALOGUE_FORMAT);
        let mut ssa = false;
        for raw in input.lines() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].to_string();
                continue;
            }
            let lsec = section.to_ascii_lowercase();
            match lsec.as_str() {
                "script info" => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.starts_with(';') {
                        entries.push(Entry::Raw { section: section.clone(), text: line.to_string() });
                    } else if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("ScriptType") && v.trim().eq_ignore_ascii_case("v4.00") {
                            ssa = true;
                        }
                        entries.push(Entry::Info { key: k.trim().to_string(), value: v.trim().to_string() });
                    }
                }
                "v4+ styles" | "v4 styles" | "v4++ styles" => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(f) = strip_ci(trimmed, "Format:") {
                        style_fmt = split_fmt(f);
                    } else if let Some(body) = strip_ci(trimmed, "Style:") {
                        entries.push(Entry::Style(parse_style(body.trim_start(), &style_fmt, ssa)));
                    } else {
                        entries.push(Entry::Raw { section: section.clone(), text: line.to_string() });
                    }
                }
                "events" => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(f) = strip_ci(trimmed, "Format:") {
                        ev_fmt = split_fmt(f);
                    } else if let Some(body) = strip_ci(trimmed, "Dialogue:") {
                        if let Some(l) = parse_event(body.trim_start(), &ev_fmt, Kind::Dialogue) {
                            entries.push(Entry::Line(l));
                        }
                    } else if let Some(body) = strip_ci(trimmed, "Comment:") {
                        if let Some(l) = parse_event(body.trim_start(), &ev_fmt, Kind::Comment) {
                            entries.push(Entry::Line(l));
                        }
                    } else {
                        // Picture:, Sound:, Movie:, Command: are valid, keep them.
                        entries.push(Entry::Raw { section: section.clone(), text: line.to_string() });
                    }
                }
                _ => {
                    if section.is_empty() && trimmed.is_empty() {
                        continue;
                    }
                    if !section.is_empty() {
                        entries.push(Entry::Raw { section: section.clone(), text: line.to_string() });
                    }
                }
            }
        }
        // Trim trailing blank raw lines per section.
        Script { entries }
    }

    pub fn to_string(&self) -> String {
        let mut out = String::with_capacity(self.entries.len() * 80);
        let mut cur: Option<&str> = None;
        for e in &self.entries {
            let sec = e.section();
            if cur != Some(sec) {
                if cur.is_some() {
                    out.push('\n');
                }
                out.push_str(&format!("[{}]\n", sec));
                match e {
                    Entry::Style(_) => {
                        out.push_str("Format: ");
                        out.push_str(STYLE_FORMAT);
                        out.push('\n');
                    }
                    Entry::Line(_) => {
                        out.push_str("Format: ");
                        out.push_str(DIALOGUE_FORMAT);
                        out.push('\n');
                    }
                    _ => {}
                }
                cur = Some(sec);
            }
            match e {
                Entry::Info { key, value } => out.push_str(&format!("{}: {}\n", key, value)),
                Entry::Style(s) => {
                    out.push_str(&s.to_line());
                    out.push('\n');
                }
                Entry::Line(l) => {
                    out.push_str(&l.to_line());
                    out.push('\n');
                }
                Entry::Raw { text, .. } => {
                    out.push_str(text);
                    out.push('\n');
                }
            }
        }
        out
    }

    pub fn info(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            Entry::Info { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn set_info(&mut self, key: &str, value: &str) {
        for e in self.entries.iter_mut() {
            if let Entry::Info { key: k, value: v } = e {
                if k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        let pos = self.entries.iter().rposition(|e| matches!(e, Entry::Info { .. })).map(|p| p + 1).unwrap_or(0);
        self.entries.insert(pos, Entry::Info { key: key.into(), value: value.into() });
    }

    pub fn play_res(&self) -> (i32, i32) {
        let x = self.info("PlayResX").and_then(|v| v.parse().ok());
        let y = self.info("PlayResY").and_then(|v| v.parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (None, Some(y)) => (y * 4 / 3, y),
            (Some(x), None) => (x, x * 3 / 4),
            _ => (384, 288),
        }
    }

    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.entries.iter().filter_map(|e| if let Entry::Style(s) = e { Some(s) } else { None })
    }
    pub fn style_map(&self) -> HashMap<&str, &Style> {
        self.styles().map(|s| (s.name.as_str(), s)).collect()
    }
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.entries.iter().filter_map(|e| if let Entry::Line(l) = e { Some(l) } else { None })
    }
    pub fn lines_mut(&mut self) -> impl Iterator<Item = &mut Line> {
        self.entries.iter_mut().filter_map(|e| if let Entry::Line(l) = e { Some(l) } else { None })
    }
    pub fn line_count(&self) -> usize {
        self.lines().count()
    }
    /// Append a line after the last event (or at the end).
    pub fn push_line(&mut self, l: Line) {
        let pos = self.entries.iter().rposition(|e| matches!(e, Entry::Line(_))).map(|p| p + 1).unwrap_or(self.entries.len());
        self.entries.insert(pos, Entry::Line(l));
    }
    /// Stable sort of the event lines by start time (then layer), leaving other entries in place.
    pub fn sort_by_start(&mut self) {
        let idx: Vec<usize> = (0..self.entries.len()).filter(|&i| matches!(self.entries[i], Entry::Line(_))).collect();
        let mut lines: Vec<Line> = idx
            .iter()
            .map(|&i| if let Entry::Line(l) = &self.entries[i] { l.clone() } else { unreachable!() })
            .collect();
        lines.sort_by_key(|l| (l.start, l.layer));
        for (i, l) in idx.into_iter().zip(lines) {
            self.entries[i] = Entry::Line(l);
        }
    }
}

fn split_fmt(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_ascii_lowercase()).collect()
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn parse_style(body: &str, fmt: &[String], ssa: bool) -> Style {
    let vals = split_n(body, fmt.len());
    let mut st = Style::default();
    for (name, v) in fmt.iter().zip(vals.iter()) {
        let v = *v;
        match name.as_str() {
            "name" => st.name = v.trim().to_string(),
            "fontname" => st.fontname = v.trim().to_string(),
            "fontsize" => st.fontsize = pf(v, 48.0),
            "primarycolour" | "primarycolor" => st.color1 = Color::parse(v).unwrap_or(st.color1),
            "secondarycolour" | "secondarycolor" => st.color2 = Color::parse(v).unwrap_or(st.color2),
            "tertiarycolour" | "outlinecolour" | "outlinecolor" => st.color3 = Color::parse(v).unwrap_or(st.color3),
            "backcolour" | "backcolor" => st.color4 = Color::parse(v).unwrap_or(st.color4),
            "bold" => st.bold = pb(v),
            "italic" => st.italic = pb(v),
            "underline" => st.underline = pb(v),
            "strikeout" => st.strikeout = pb(v),
            "scalex" => st.scale_x = pf(v, 100.0),
            "scaley" => st.scale_y = pf(v, 100.0),
            "spacing" => st.spacing = pf(v, 0.0),
            "angle" => st.angle = pf(v, 0.0),
            "borderstyle" => st.borderstyle = pi(v, 1),
            "outline" => st.outline = pf(v, 2.0),
            "shadow" => st.shadow = pf(v, 2.0),
            "alignment" => {
                let a = pi(v, 2);
                st.align = if ssa { ssa_align(a) } else { a };
            }
            "marginl" => st.margin_l = pi(v, 10),
            "marginr" => st.margin_r = pi(v, 10),
            "marginv" => st.margin_t = pi(v, 10),
            "encoding" => st.encoding = pi(v, 1),
            _ => {}
        }
    }
    st
}

fn parse_event(body: &str, fmt: &[String], kind: Kind) -> Option<Line> {
    let vals = split_n(body, fmt.len());
    if vals.len() < fmt.len() {
        // Tolerate a missing trailing Text field.
        if vals.len() + 1 < fmt.len() {
            return None;
        }
    }
    let mut l = Line { kind, ..Default::default() };
    for (name, v) in fmt.iter().zip(vals.iter()) {
        let v = *v;
        match name.as_str() {
            "layer" | "marked" => l.layer = pi(v.trim_start_matches(|c: char| c.is_alphabetic() || c == '='), 0),
            "start" => l.start = parse_ass_time(v)?,
            "end" => l.end = parse_ass_time(v)?,
            "style" => l.style = v.trim().trim_start_matches('*').to_string(),
            "name" | "actor" => l.actor = v.to_string(),
            "marginl" => l.margin_l = pi(v, 0),
            "marginr" => l.margin_r = pi(v, 0),
            "marginv" => l.margin_t = pi(v, 0),
            "effect" => l.effect = v.to_string(),
            "text" => l.text = v.to_string(),
            _ => {}
        }
    }
    Some(l)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[Script Info]\n; comment\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, {\\i1}world{\\i0}\nComment: 1,0:00:04.00,0:00:05.00,Default,x,0,0,0,fx,note\n\n[Fonts]\nfontname: abc\n";

    #[test]
    fn parse_write_roundtrip() {
        let s = Script::parse(SAMPLE);
        assert_eq!(s.line_count(), 2);
        assert_eq!(s.play_res(), (1280, 720));
        let l = s.lines().next().unwrap();
        assert_eq!(l.text, "Hello, {\\i1}world{\\i0}");
        assert_eq!(l.end, 3500);
        let out = s.to_string();
        let s2 = Script::parse(&out);
        assert_eq!(s, s2);
        assert!(out.contains("[Fonts]"));
    }

    #[test]
    fn ssa_alignment() {
        assert_eq!(ssa_align(6), 8);
        assert_eq!(ssa_align(10), 5);
        assert_eq!(ssa_align(2), 2);
    }
}
