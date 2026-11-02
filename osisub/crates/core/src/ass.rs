//! ASS/SSA data model, parser and writer.
//!
//! The script is a flat list of [`Entry`] values in file order, which mirrors
//! how Aegisub exposes a file to Lua automation scripts (`subs[i]`).

use crate::time::{format_ass_time, parse_ass_time};

pub const SEC_INFO: &str = "Script Info";
pub const SEC_STYLES: &str = "V4+ Styles";
pub const SEC_EVENTS: &str = "Events";

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub color1: String,
    pub color2: String,
    pub color3: String,
    pub color4: String,
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
    pub margin_v: i32,
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(),
            fontname: "Arial".into(),
            fontsize: 48.0,
            color1: "&H00FFFFFF".into(),
            color2: "&H000000FF".into(),
            color3: "&H00000000".into(),
            color4: "&H00000000".into(),
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
            margin_v: 10,
            encoding: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub comment: bool,
    pub layer: i32,
    pub start: i64,
    pub end: i64,
    pub style: String,
    pub actor: String,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_v: i32,
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
            margin_v: 0,
            effect: String::new(),
            text: String::new(),
        }
    }
}

impl Event {
    pub fn duration(&self) -> i64 {
        self.end - self.start
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// `Key: Value` in `[Script Info]`.
    Info { key: String, value: String },
    Style(Style),
    Dialogue(Event),
    /// Anything else (comments, unknown sections such as `[Fonts]`), kept verbatim.
    Raw { section: String, text: String },
}

impl Entry {
    pub fn section(&self) -> &str {
        match self {
            Entry::Info { .. } => SEC_INFO,
            Entry::Style(_) => SEC_STYLES,
            Entry::Dialogue(_) => SEC_EVENTS,
            Entry::Raw { section, .. } => section,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub entries: Vec<Entry>,
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}
impl std::error::Error for ParseError {}

pub const STYLE_FORMAT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
pub const EVENT_FORMAT: &str = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";

fn fields(fmt: &str) -> Vec<String> {
    fmt.split(',').map(|s| s.trim().to_ascii_lowercase()).collect()
}

fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}
fn int(s: &str) -> i32 {
    let s = s.trim();
    s.parse::<i32>().or_else(|_| s.parse::<f64>().map(|f| f as i32)).unwrap_or(0)
}
fn boolean(s: &str) -> bool {
    int(s) != 0
}

/// Format a number like Aegisub: no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Normalise an ASS colour string such as `H00FFFFFF`, `&HFFFFFF&` or a decimal
/// number into `&HAABBGGRR` (no trailing `&`, matching Aegisub's style colours).
pub fn norm_color(s: &str) -> String {
    let t = s.trim().trim_end_matches('&');
    let hex = if let Some(h) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")).or_else(|| t.strip_prefix('H')) {
        u32::from_str_radix(h, 16).unwrap_or(0)
    } else {
        t.parse::<i64>().map(|v| v as u32).unwrap_or(0)
    };
    format!("&H{:08X}", hex)
}

impl Script {
    pub fn parse(input: &str) -> Result<Script, ParseError> {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut sc = Script::default();
        let mut section = String::new();
        let mut style_fmt = fields(STYLE_FORMAT);
        let mut event_fmt = fields(EVENT_FORMAT);
        let mut v4 = false; // legacy SSA
        for (ln, raw) in input.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                section = t[1..t.len() - 1].to_string();
                if section.eq_ignore_ascii_case("V4 Styles") {
                    v4 = true;
                    section = SEC_STYLES.to_string();
                }
                continue;
            }
            if t.is_empty() {
                // Blank lines are structural, except inside unknown sections (kept verbatim).
                if !is_known(&section) && !section.is_empty() {
                    sc.entries.push(Entry::Raw { section: section.clone(), text: String::new() });
                }
                continue;
            }
            let lsec = section.to_ascii_lowercase();
            match lsec.as_str() {
                "script info" => {
                    if t.starts_with(';') || t.starts_with('!') {
                        sc.entries.push(Entry::Raw { section: SEC_INFO.into(), text: t.to_string() });
                    } else if let Some((k, v)) = t.split_once(':') {
                        sc.entries.push(Entry::Info { key: k.trim().to_string(), value: v.trim().to_string() });
                    } else {
                        sc.entries.push(Entry::Raw { section: SEC_INFO.into(), text: t.to_string() });
                    }
                }
                "v4+ styles" | "v4 styles" => {
                    let Some((k, v)) = t.split_once(':') else { continue };
                    match k.trim().to_ascii_lowercase().as_str() {
                        "format" => style_fmt = fields(v),
                        "style" => sc.entries.push(Entry::Style(parse_style(v, &style_fmt, v4))),
                        _ => sc.entries.push(Entry::Raw { section: SEC_STYLES.into(), text: t.to_string() }),
                    }
                }
                "events" => {
                    let Some((k, v)) = t.split_once(':') else {
                        return Err(ParseError { line: ln + 1, msg: format!("bad event line: {t}") });
                    };
                    let kl = k.trim().to_ascii_lowercase();
                    match kl.as_str() {
                        "format" => event_fmt = fields(v),
                        "dialogue" | "comment" => {
                            let ev = parse_event(v.strip_prefix(' ').unwrap_or(v), &event_fmt, kl == "comment")
                                .ok_or_else(|| ParseError { line: ln + 1, msg: format!("bad event: {t}") })?;
                            sc.entries.push(Entry::Dialogue(ev));
                        }
                        // Picture/Sound/Movie/Command events are preserved verbatim.
                        _ => sc.entries.push(Entry::Raw { section: SEC_EVENTS.into(), text: t.to_string() }),
                    }
                }
                _ => sc.entries.push(Entry::Raw { section: section.clone(), text: line.to_string() }),
            }
        }
        Ok(sc)
    }

    pub fn to_ass(&self) -> String {
        let mut out = String::new();
        let mut cur: Option<&str> = None;
        for e in &self.entries {
            let sec = e.section();
            if cur != Some(sec) {
                if cur.is_some() {
                    out.push('\n');
                }
                out.push_str(&format!("[{sec}]\n"));
                if sec == SEC_STYLES {
                    out.push_str(&format!("Format: {STYLE_FORMAT}\n"));
                } else if sec == SEC_EVENTS {
                    out.push_str(&format!("Format: {EVENT_FORMAT}\n"));
                }
                cur = Some(sec);
            }
            match e {
                Entry::Info { key, value } => out.push_str(&format!("{key}: {value}\n")),
                Entry::Style(s) => out.push_str(&format!("Style: {}\n", style_line(s))),
                Entry::Dialogue(d) => out.push_str(&format!("{}\n", event_line(d))),
                Entry::Raw { text, .. } => {
                    out.push_str(text);
                    out.push('\n');
                }
            }
        }
        out
    }

    // ---- convenience accessors ----
    pub fn info(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            Entry::Info { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }
    pub fn set_info(&mut self, key: &str, value: &str) {
        for e in &mut self.entries {
            if let Entry::Info { key: k, value: v } = e {
                if k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        let pos = self.entries.iter().rposition(|e| matches!(e, Entry::Info { .. })).map_or(0, |p| p + 1);
        self.entries.insert(pos, Entry::Info { key: key.into(), value: value.into() });
    }
    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.entries.iter().filter_map(|e| if let Entry::Style(s) = e { Some(s) } else { None })
    }
    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles().find(|s| s.name == name)
    }
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.entries.iter().filter_map(|e| if let Entry::Dialogue(d) = e { Some(d) } else { None })
    }
    pub fn events_mut(&mut self) -> impl Iterator<Item = &mut Event> {
        self.entries.iter_mut().filter_map(|e| if let Entry::Dialogue(d) = e { Some(d) } else { None })
    }
    pub fn resolution(&self) -> (i32, i32) {
        let x = self.info("PlayResX").and_then(|v| v.parse().ok());
        let y = self.info("PlayResY").and_then(|v| v.parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (None, Some(y)) => (y * 4 / 3, y),
            (Some(x), None) => (x, x * 3 / 4),
            _ => (384, 288),
        }
    }

    /// A fresh script with Aegisub-like defaults.
    pub fn new_default() -> Script {
        let mut sc = Script::default();
        for (k, v) in [
            ("ScriptType", "v4.00+"),
            ("WrapStyle", "0"),
            ("ScaledBorderAndShadow", "yes"),
            ("PlayResX", "1920"),
            ("PlayResY", "1080"),
            ("YCbCr Matrix", "TV.709"),
        ] {
            sc.entries.push(Entry::Info { key: k.into(), value: v.into() });
        }
        sc.entries.push(Entry::Style(Style::default()));
        sc
    }
}

fn is_known(sec: &str) -> bool {
    matches!(sec.to_ascii_lowercase().as_str(), "script info" | "v4+ styles" | "v4 styles" | "events")
}

fn parse_style(v: &str, fmt: &[String], v4: bool) -> Style {
    let mut st = Style::default();
    let vals: Vec<&str> = v.trim_start().splitn(fmt.len(), ',').collect();
    for (k, val) in fmt.iter().zip(vals.iter()) {
        let val = val.trim();
        match k.as_str() {
            "name" => st.name = val.to_string(),
            "fontname" => st.fontname = val.to_string(),
            "fontsize" => st.fontsize = num(val),
            "primarycolour" => st.color1 = norm_color(val),
            "secondarycolour" => st.color2 = norm_color(val),
            "outlinecolour" | "tertiarycolour" => st.color3 = norm_color(val),
            "backcolour" => st.color4 = norm_color(val),
            "bold" => st.bold = boolean(val),
            "italic" => st.italic = boolean(val),
            "underline" => st.underline = boolean(val),
            "strikeout" => st.strikeout = boolean(val),
            "scalex" => st.scale_x = num(val),
            "scaley" => st.scale_y = num(val),
            "spacing" => st.spacing = num(val),
            "angle" => st.angle = num(val),
            "borderstyle" => st.borderstyle = int(val),
            "outline" => st.outline = num(val),
            "shadow" => st.shadow = num(val),
            "alignment" => {
                let a = int(val);
                st.align = if v4 { ssa_to_ass_align(a) } else { a };
            }
            "marginl" => st.margin_l = int(val),
            "marginr" => st.margin_r = int(val),
            "marginv" => st.margin_v = int(val),
            "encoding" => st.encoding = int(val),
            _ => {}
        }
    }
    st
}

/// Legacy SSA alignment (1-3 bottom, 5-7 top, 9-11 middle) -> numpad.
fn ssa_to_ass_align(a: i32) -> i32 {
    match a {
        1..=3 => a,
        5..=7 => a - 5 + 7,
        9..=11 => a - 9 + 4,
        _ => 2,
    }
}

fn style_line(s: &Style) -> String {
    let b = |v: bool| if v { "-1" } else { "0" };
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name, s.fontname, fmt_num(s.fontsize), s.color1, s.color2, s.color3, s.color4,
        b(s.bold), b(s.italic), b(s.underline), b(s.strikeout),
        fmt_num(s.scale_x), fmt_num(s.scale_y), fmt_num(s.spacing), fmt_num(s.angle),
        s.borderstyle, fmt_num(s.outline), fmt_num(s.shadow), s.align,
        s.margin_l, s.margin_r, s.margin_v, s.encoding
    )
}

fn parse_event(v: &str, fmt: &[String], comment: bool) -> Option<Event> {
    let vals: Vec<&str> = v.splitn(fmt.len(), ',').collect();
    if vals.len() < fmt.len() {
        return None;
    }
    let mut ev = Event { comment, ..Default::default() };
    for (k, val) in fmt.iter().zip(vals.iter()) {
        match k.as_str() {
            "layer" | "marked" => ev.layer = int(&val.replace("Marked=", "")),
            "start" => ev.start = parse_ass_time(val)?,
            "end" => ev.end = parse_ass_time(val)?,
            "style" => ev.style = val.trim().to_string(),
            "name" | "actor" => ev.actor = val.trim().to_string(),
            "marginl" => ev.margin_l = int(val),
            "marginr" => ev.margin_r = int(val),
            "marginv" => ev.margin_v = int(val),
            "effect" => ev.effect = val.trim().to_string(),
            "text" => ev.text = val.to_string(),
            _ => {}
        }
    }
    Some(ev)
}

pub fn event_line(d: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{},{},{},{},{}",
        if d.comment { "Comment" } else { "Dialogue" },
        d.layer, format_ass_time(d.start), format_ass_time(d.end), d.style, d.actor,
        d.margin_l, d.margin_r, d.margin_v, d.effect, d.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[Script Info]\n; hi\nTitle: T\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, {\\i1}world{\\i0}\\Nline2\nComment: 1,0:00:04.00,0:00:05.00,Default,note,0,0,0,,x\n\n[Fonts]\nfontname: abc\n";

    #[test]
    fn parse_and_roundtrip() {
        let sc = Script::parse(SAMPLE).unwrap();
        assert_eq!(sc.info("playresx"), Some("1280"));
        assert_eq!(sc.resolution(), (1280, 720));
        assert_eq!(sc.style("Default").unwrap().fontsize, 40.0);
        assert!(sc.style("Default").unwrap().bold);
        let evs: Vec<_> = sc.events().collect();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].text, "Hello, {\\i1}world{\\i0}\\Nline2");
        assert_eq!(evs[0].end, 3500);
        assert!(evs[1].comment);
        let out = sc.to_ass();
        let again = Script::parse(&out).unwrap();
        assert_eq!(sc, again);
        assert!(out.contains("[Fonts]\nfontname: abc"));
    }

    #[test]
    fn colors() {
        assert_eq!(norm_color("&HFFFFFF&"), "&H00FFFFFF");
        assert_eq!(norm_color("16777215"), "&H00FFFFFF");
    }

    #[test]
    fn legacy_ssa_align() {
        assert_eq!(ssa_to_ass_align(6), 8);
        assert_eq!(ssa_to_ass_align(10), 5);
    }
}
