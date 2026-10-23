//! ASS/SSA document model, parser and writer.
//!
//! The model mirrors what Aegisub exposes to automation scripts: a flat list
//! of lines, each tagged with a class (info / style / dialogue / raw).

use std::fmt::Write as _;

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
    pub borderstyle: i64,
    pub outline: f64,
    pub shadow: f64,
    pub align: i64,
    pub margin_l: i64,
    pub margin_r: i64,
    pub margin_t: i64,
    pub margin_b: i64,
    pub encoding: i64,
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
            margin_t: 10,
            margin_b: 10,
            encoding: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Dialogue {
    pub comment: bool,
    pub layer: i64,
    pub start_time: i64,
    pub end_time: i64,
    pub style: String,
    pub actor: String,
    pub margin_l: i64,
    pub margin_r: i64,
    pub margin_t: i64,
    pub margin_b: i64,
    pub effect: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Info { key: String, value: String },
    Style(Style),
    Dialogue(Dialogue),
    /// Any line we do not model (comments, other sections, fonts, ...).
    Raw { section: String, text: String },
}

impl Entry {
    pub fn section(&self) -> &str {
        match self {
            Entry::Info { .. } => "Script Info",
            Entry::Style(_) => "V4+ Styles",
            Entry::Dialogue(_) => "Events",
            Entry::Raw { section, .. } => section,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub entries: Vec<Entry>,
}

// ---------- time & numbers ----------

pub fn parse_time(s: &str) -> i64 {
    let s = s.trim();
    let mut parts = s.split(':');
    let h: i64 = parts.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    let m: i64 = parts.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    let sec = parts.next().unwrap_or("0");
    let (si, frac) = sec.split_once('.').unwrap_or((sec, ""));
    let si: i64 = si.trim().parse().unwrap_or(0);
    let mut ms = 0i64;
    if !frac.is_empty() {
        let digits: String = frac.chars().filter(|c| c.is_ascii_digit()).take(3).collect();
        let n: i64 = digits.parse().unwrap_or(0);
        ms = n * 10i64.pow(3 - digits.len() as u32);
    }
    ((h * 60 + m) * 60 + si) * 1000 + ms
}

/// Format as H:MM:SS.cc (centisecond precision, rounded like Aegisub).
pub fn format_time(ms: i64) -> String {
    let ms = ms.max(0);
    let cs_total = (ms + 5) / 10;
    let cs = cs_total % 100;
    let s_total = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s_total / 3600, (s_total / 60) % 60, s_total % 60, cs)
}

fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}
fn int(s: &str) -> i64 {
    let s = s.trim();
    s.parse::<i64>().unwrap_or_else(|_| s.parse::<f64>().map(|f| f as i64).unwrap_or(0))
}
fn flag(s: &str) -> bool {
    int(s) != 0
}

pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let mut s = format!("{:.3}", v);
        while s.ends_with('0') {
            s.pop();
        }
        s
    }
}

fn norm_color(s: &str) -> String {
    let t = s.trim();
    let t = t.trim_end_matches('&');
    let hex = t.trim_start_matches("&H").trim_start_matches("&h");
    if t.starts_with("&H") || t.starts_with("&h") {
        format!("&H{:0>8}", hex.to_uppercase())
    } else if let Ok(n) = t.parse::<i64>() {
        // SSA decimal BGR color
        format!("&H{:08X}", n as u32)
    } else {
        t.to_string()
    }
}

// ---------- parsing ----------

impl Document {
    pub fn parse(src: &str) -> Document {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        let mut doc = Document::default();
        let mut section = String::new();
        let mut style_fmt: Vec<String> = default_style_format();
        let mut event_fmt: Vec<String> = default_event_format();
        for raw in src.lines() {
            let line = raw.trim_end_matches('\r');
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                section = t[1..t.len() - 1].to_string();
                continue;
            }
            if t.is_empty() {
                continue;
            }
            let sl = section.to_ascii_lowercase();
            match sl.as_str() {
                "script info" => {
                    if t.starts_with(';') || !t.contains(':') {
                        doc.entries.push(Entry::Raw { section: section.clone(), text: line.to_string() });
                    } else {
                        let (k, v) = t.split_once(':').unwrap();
                        doc.entries.push(Entry::Info { key: k.trim().to_string(), value: v.trim().to_string() });
                    }
                }
                "v4+ styles" | "v4 styles" => {
                    if let Some((k, v)) = t.split_once(':') {
                        match k.trim().to_ascii_lowercase().as_str() {
                            "format" => style_fmt = split_fmt(v),
                            "style" => doc.entries.push(Entry::Style(parse_style(v, &style_fmt))),
                            _ => doc.entries.push(Entry::Raw { section: section.clone(), text: line.to_string() }),
                        }
                    }
                }
                "events" => {
                    if let Some((k, v)) = t.split_once(':') {
                        match k.trim().to_ascii_lowercase().as_str() {
                            "format" => event_fmt = split_fmt(v),
                            "dialogue" => doc.entries.push(Entry::Dialogue(parse_dialogue(v, &event_fmt, false))),
                            "comment" => doc.entries.push(Entry::Dialogue(parse_dialogue(v, &event_fmt, true))),
                            _ => doc.entries.push(Entry::Raw { section: section.clone(), text: line.to_string() }),
                        }
                    }
                }
                _ => doc.entries.push(Entry::Raw { section: section.clone(), text: line.to_string() }),
            }
        }
        doc
    }

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        let mut cur: Option<String> = None;
        for e in &self.entries {
            let sec = e.section().to_string();
            if cur.as_deref() != Some(sec.as_str()) {
                if cur.is_some() {
                    out.push('\n');
                }
                let _ = writeln!(out, "[{}]", sec);
                match e {
                    Entry::Style(_) => out.push_str(
                        "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n",
                    ),
                    Entry::Dialogue(_) => out.push_str(
                        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
                    ),
                    _ => {}
                }
                cur = Some(sec);
            }
            match e {
                Entry::Info { key, value } => {
                    let _ = writeln!(out, "{}: {}", key, value);
                }
                Entry::Raw { text, .. } => {
                    let _ = writeln!(out, "{}", text);
                }
                Entry::Style(s) => {
                    let b = |x: bool| if x { "-1" } else { "0" };
                    let _ = writeln!(
                        out,
                        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        s.name, s.fontname, fmt_num(s.fontsize), s.color1, s.color2, s.color3, s.color4,
                        b(s.bold), b(s.italic), b(s.underline), b(s.strikeout),
                        fmt_num(s.scale_x), fmt_num(s.scale_y), fmt_num(s.spacing), fmt_num(s.angle),
                        s.borderstyle, fmt_num(s.outline), fmt_num(s.shadow), s.align,
                        s.margin_l, s.margin_r, s.margin_t, s.encoding
                    );
                }
                Entry::Dialogue(d) => {
                    let _ = writeln!(
                        out,
                        "{}: {},{},{},{},{},{:04},{:04},{:04},{},{}",
                        if d.comment { "Comment" } else { "Dialogue" },
                        d.layer, format_time(d.start_time), format_time(d.end_time), d.style, d.actor,
                        d.margin_l, d.margin_r, d.margin_t, d.effect, d.text
                    );
                }
            }
        }
        out
    }

    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.entries.iter().filter_map(|e| if let Entry::Style(s) = e { Some(s) } else { None })
    }
    pub fn dialogues(&self) -> impl Iterator<Item = &Dialogue> {
        self.entries.iter().filter_map(|e| if let Entry::Dialogue(d) = e { Some(d) } else { None })
    }
    pub fn info(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            Entry::Info { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }
}

fn default_style_format() -> Vec<String> {
    "Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding"
        .split(',').map(|s| s.to_string()).collect()
}
fn default_event_format() -> Vec<String> {
    "Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text".split(',').map(|s| s.to_string()).collect()
}
fn split_fmt(v: &str) -> Vec<String> {
    v.split(',').map(|s| s.trim().to_string()).collect()
}

fn parse_style(v: &str, fmt: &[String]) -> Style {
    let mut st = Style::default();
    let vals: Vec<&str> = v.trim_start().splitn(fmt.len(), ',').collect();
    for (name, val) in fmt.iter().zip(vals.iter()) {
        match name.to_ascii_lowercase().as_str() {
            "name" => st.name = val.trim().to_string(),
            "fontname" => st.fontname = val.trim().to_string(),
            "fontsize" => st.fontsize = num(val),
            "primarycolour" | "primarycolor" => st.color1 = norm_color(val),
            "secondarycolour" | "secondarycolor" => st.color2 = norm_color(val),
            "outlinecolour" | "outlinecolor" | "tertiarycolour" => st.color3 = norm_color(val),
            "backcolour" | "backcolor" => st.color4 = norm_color(val),
            "bold" => st.bold = flag(val),
            "italic" => st.italic = flag(val),
            "underline" => st.underline = flag(val),
            "strikeout" => st.strikeout = flag(val),
            "scalex" => st.scale_x = num(val),
            "scaley" => st.scale_y = num(val),
            "spacing" => st.spacing = num(val),
            "angle" => st.angle = num(val),
            "borderstyle" => st.borderstyle = int(val),
            "outline" => st.outline = num(val),
            "shadow" => st.shadow = num(val),
            "alignment" => st.align = int(val),
            "marginl" => st.margin_l = int(val),
            "marginr" => st.margin_r = int(val),
            "marginv" => {
                st.margin_t = int(val);
                st.margin_b = int(val);
            }
            "encoding" => st.encoding = int(val),
            _ => {}
        }
    }
    st
}

fn parse_dialogue(v: &str, fmt: &[String], comment: bool) -> Dialogue {
    let mut d = Dialogue { comment, ..Default::default() };
    let vals: Vec<&str> = v.trim_start().splitn(fmt.len(), ',').collect();
    for (name, val) in fmt.iter().zip(vals.iter()) {
        match name.to_ascii_lowercase().as_str() {
            "layer" | "marked" => d.layer = int(&val.replace("Marked=", "")),
            "start" => d.start_time = parse_time(val),
            "end" => d.end_time = parse_time(val),
            "style" => d.style = val.trim().to_string(),
            "name" | "actor" => d.actor = val.trim().to_string(),
            "marginl" => d.margin_l = int(val),
            "marginr" => d.margin_r = int(val),
            "marginv" => {
                d.margin_t = int(val);
                d.margin_b = int(val);
            }
            "effect" => d.effect = val.trim().to_string(),
            "text" => d.text = val.to_string(),
            _ => {}
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_roundtrip() {
        assert_eq!(parse_time("1:02:03.45"), 3_723_450);
        assert_eq!(format_time(3_723_450), "1:02:03.45");
        assert_eq!(format_time(1004), "0:00:01.00");
        assert_eq!(format_time(1005), "0:00:01.01");
    }

    #[test]
    fn parse_write() {
        let src = "[Script Info]\nTitle: x\nPlayResX: 1920\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.50,Default,,0000,0000,0000,,Hello, {\\k20}world\n";
        let d = Document::parse(src);
        assert_eq!(d.dialogues().count(), 1);
        assert_eq!(d.dialogues().next().unwrap().text, "Hello, {\\k20}world");
        assert!(d.styles().next().unwrap().bold);
        assert_eq!(d.info("playresx"), Some("1920"));
        assert_eq!(Document::parse(&d.to_string()), d);
    }
}
