//! ASS/SSA document model, parser and writer. Round-trips Aegisub files.

use crate::time::{fmt_ass_time, fmt_num, parse_ass_time};

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    /// Primary, secondary, outline, back as 0xAABBGGRR (ASS alpha 00 = opaque).
    pub colors: [u32; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
    pub angle: f64,
    pub border_style: i32,
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
            colors: [0x00FFFFFF, 0x000000FF, 0x00000000, 0x00000000],
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
            scale_x: 100.0,
            scale_y: 100.0,
            spacing: 0.0,
            angle: 0.0,
            border_style: 1,
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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// `key: value` in [Script Info]. Comment lines use key ";".
    Info { key: String, value: String },
    Style(Style),
    Event(Event),
}

/// Sections we preserve verbatim ([Fonts], [Graphics], [Aegisub Project Garbage], ...).
#[derive(Debug, Clone, PartialEq)]
pub struct RawSection {
    pub header: String,
    pub lines: Vec<String>,
    pub after_events: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub entries: Vec<Entry>,
    pub extras: Vec<RawSection>,
    /// true when the source used SSA v4 (`[V4 Styles]`).
    pub ssa_v4: bool,
}

pub fn parse_color(s: &str) -> u32 {
    let t = s.trim().trim_start_matches('&').trim_end_matches('&');
    let t = t.trim_start_matches(['H', 'h']);
    if s.trim().starts_with('&') || s.trim().to_ascii_uppercase().starts_with("H") {
        u32::from_str_radix(t, 16).unwrap_or(0)
    } else {
        s.trim().parse::<i64>().map(|v| v as u32).unwrap_or(0)
    }
}

pub fn fmt_color(c: u32) -> String {
    format!("&H{:08X}", c)
}

fn pf(s: &str, d: f64) -> f64 {
    s.trim().parse().unwrap_or(d)
}
fn pi(s: &str, d: i32) -> i32 {
    let t = s.trim();
    t.parse::<i32>().or_else(|_| t.parse::<f64>().map(|v| v as i32)).unwrap_or(d)
}
fn pb(s: &str) -> bool {
    pi(s, 0) != 0
}

const STYLE_FMT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
const EVENT_FMT: &str = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";

fn split_format(line: &str) -> Vec<String> {
    line.split(',').map(|s| s.trim().to_ascii_lowercase()).collect()
}

fn parse_style(fmt: &[String], rest: &str) -> Style {
    let vals: Vec<&str> = rest.splitn(fmt.len().max(1), ',').collect();
    let mut st = Style::default();
    // SSA v4 alignment differs; remap below if needed by caller.
    for (k, v) in fmt.iter().zip(vals.iter()) {
        let v = v.trim();
        match k.as_str() {
            "name" => st.name = v.to_string(),
            "fontname" => st.fontname = v.to_string(),
            "fontsize" => st.fontsize = pf(v, 48.0),
            "primarycolour" => st.colors[0] = parse_color(v),
            "secondarycolour" => st.colors[1] = parse_color(v),
            "outlinecolour" | "tertiarycolour" => st.colors[2] = parse_color(v),
            "backcolour" => st.colors[3] = parse_color(v),
            "bold" => st.bold = pb(v),
            "italic" => st.italic = pb(v),
            "underline" => st.underline = pb(v),
            "strikeout" => st.strikeout = pb(v),
            "scalex" => st.scale_x = pf(v, 100.0),
            "scaley" => st.scale_y = pf(v, 100.0),
            "spacing" => st.spacing = pf(v, 0.0),
            "angle" => st.angle = pf(v, 0.0),
            "borderstyle" => st.border_style = pi(v, 1),
            "outline" => st.outline = pf(v, 2.0),
            "shadow" => st.shadow = pf(v, 2.0),
            "alignment" => st.align = pi(v, 2),
            "marginl" => st.margin_l = pi(v, 10),
            "marginr" => st.margin_r = pi(v, 10),
            "marginv" => st.margin_v = pi(v, 10),
            "encoding" => st.encoding = pi(v, 1),
            _ => {}
        }
    }
    st
}

/// SSA v4 numpad-less alignment (1-3 bottom, 5-7 top, 9-11 mid) -> ASS numpad.
fn ssa_align_to_ass(a: i32) -> i32 {
    match a {
        1..=3 => a,
        5..=7 => a - 4 + 6,
        9..=11 => a - 8 + 3,
        _ => 2,
    }
}

fn parse_event(fmt: &[String], comment: bool, rest: &str) -> Event {
    let vals: Vec<&str> = rest.splitn(fmt.len().max(1), ',').collect();
    let mut ev = Event { comment, ..Event::default() };
    for (k, v) in fmt.iter().zip(vals.iter()) {
        let vt = v.trim();
        match k.as_str() {
            "layer" | "marked" => ev.layer = pi(vt.trim_start_matches("Marked="), 0),
            "start" => ev.start = parse_ass_time(vt).unwrap_or(0),
            "end" => ev.end = parse_ass_time(vt).unwrap_or(0),
            "style" => ev.style = vt.to_string(),
            "name" | "actor" => ev.actor = vt.to_string(),
            "marginl" => ev.margin_l = pi(vt, 0),
            "marginr" => ev.margin_r = pi(vt, 0),
            "marginv" => ev.margin_v = pi(vt, 0),
            "effect" => ev.effect = vt.to_string(),
            "text" => ev.text = v.to_string(), // keep leading spaces
            _ => {}
        }
    }
    ev
}

impl Document {
    pub fn new_default() -> Document {
        let mut d = Document::default();
        for (k, v) in [
            ("Title", "Untitled"),
            ("ScriptType", "v4.00+"),
            ("WrapStyle", "0"),
            ("ScaledBorderAndShadow", "yes"),
            ("YCbCr Matrix", "TV.709"),
            ("PlayResX", "1920"),
            ("PlayResY", "1080"),
        ] {
            d.set_info(k, v);
        }
        d.entries.push(Entry::Style(Style::default()));
        d
    }

    pub fn parse(src: &str) -> Document {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        let mut doc = Document::default();
        let mut section = String::new();
        let mut style_fmt = split_format(STYLE_FMT);
        let mut event_fmt = split_format(EVENT_FMT);
        let mut seen_events = false;
        let mut raw_idx: Option<usize> = None;
        for line in src.lines() {
            let t = line.trim_end_matches('\r');
            let tt = t.trim();
            let in_blob = matches!(section.to_ascii_lowercase().as_str(), "[fonts]" | "[graphics]");
            let known = ["[script info]", "[v4+ styles]", "[v4 styles]", "[events]", "[fonts]", "[graphics]", "[aegisub project garbage]", "[aegisub extradata]"];
            // uuencoded font data can look like "[...]": inside blobs only real section names count
            if tt.starts_with('[') && tt.ends_with(']') && (!in_blob || known.contains(&tt.to_ascii_lowercase().as_str())) {
                section = tt.to_string();
                raw_idx = None;
                let low = section.to_ascii_lowercase();
                match low.as_str() {
                    "[script info]" | "[v4+ styles]" | "[v4 styles]" | "[events]" => {
                        if low == "[v4 styles]" {
                            doc.ssa_v4 = true;
                        }
                        if low == "[events]" {
                            seen_events = true;
                        }
                    }
                    _ => {
                        doc.extras.push(RawSection {
                            header: section.clone(),
                            lines: vec![],
                            after_events: seen_events,
                        });
                        raw_idx = Some(doc.extras.len() - 1);
                    }
                }
                continue;
            }
            let low = section.to_ascii_lowercase();
            match low.as_str() {
                "[script info]" => {
                    if tt.is_empty() {
                        continue;
                    }
                    if let Some(c) = tt.strip_prefix(';') {
                        doc.entries.push(Entry::Info { key: ";".into(), value: c.to_string() });
                    } else if let Some((k, v)) = t.split_once(':') {
                        doc.entries.push(Entry::Info {
                            key: k.trim().to_string(),
                            value: v.trim().to_string(),
                        });
                    }
                }
                "[v4+ styles]" | "[v4 styles]" => {
                    if let Some(f) = tt.strip_prefix("Format:") {
                        style_fmt = split_format(f);
                    } else if let Some(r) = tt.strip_prefix("Style:") {
                        let mut st = parse_style(&style_fmt, r.trim_start());
                        if doc.ssa_v4 {
                            st.align = ssa_align_to_ass(st.align);
                        }
                        doc.entries.push(Entry::Style(st));
                    }
                }
                "[events]" => {
                    if let Some(f) = tt.strip_prefix("Format:") {
                        event_fmt = split_format(f);
                    } else if let Some(r) = t.strip_prefix("Dialogue:") {
                        doc.entries.push(Entry::Event(parse_event(&event_fmt, false, r.strip_prefix(' ').unwrap_or(r))));
                    } else if let Some(r) = t.strip_prefix("Comment:") {
                        doc.entries.push(Entry::Event(parse_event(&event_fmt, true, r.strip_prefix(' ').unwrap_or(r))));
                    }
                }
                _ => {
                    if let Some(i) = raw_idx {
                        doc.extras[i].lines.push(t.to_string());
                    }
                }
            }
        }
        if doc.ssa_v4 {
            // we always write [V4+ Styles]; make the header agree
            doc.set_info("ScriptType", "v4.00+");
        }
        doc
    }

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        out.push_str("[Script Info]\n");
        for e in &self.entries {
            if let Entry::Info { key, value } = e {
                if key == ";" {
                    out.push_str(&format!(";{}\n", value));
                } else {
                    out.push_str(&format!("{}: {}\n", key, value));
                }
            }
        }
        out.push_str("\n[V4+ Styles]\nFormat: ");
        out.push_str(STYLE_FMT);
        out.push('\n');
        for e in &self.entries {
            if let Entry::Style(s) = e {
                out.push_str(&format_style(s));
                out.push('\n');
            }
        }
        out.push('\n');
        for x in self.extras.iter().filter(|x| !x.after_events) {
            write_raw(&mut out, x);
        }
        out.push_str("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        for e in &self.entries {
            if let Entry::Event(ev) = e {
                out.push_str(&format_event(ev));
                out.push('\n');
            }
        }
        out.push('\n');
        for x in self.extras.iter().filter(|x| x.after_events) {
            write_raw(&mut out, x);
        }
        while out.ends_with("\n\n") {
            out.pop();
        }
        out
    }

    // ---- accessors ----
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
        // insert after last Info
        let pos = self.entries.iter().rposition(|e| matches!(e, Entry::Info { .. })).map(|p| p + 1).unwrap_or(0);
        self.entries.insert(pos, Entry::Info { key: key.into(), value: value.into() });
    }

    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.entries.iter().filter_map(|e| if let Entry::Style(s) = e { Some(s) } else { None })
    }
    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles().find(|s| s.name == name)
    }
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.entries.iter().filter_map(|e| if let Entry::Event(s) = e { Some(s) } else { None })
    }
    pub fn events_mut(&mut self) -> impl Iterator<Item = &mut Event> {
        self.entries.iter_mut().filter_map(|e| if let Entry::Event(s) = e { Some(s) } else { None })
    }
    pub fn res(&self) -> (i32, i32) {
        let x = self.info("PlayResX").and_then(|v| v.parse().ok());
        let y = self.info("PlayResY").and_then(|v| v.parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (None, Some(y)) => ((y as f64 * 4.0 / 3.0) as i32, y),
            (Some(x), None) => (x, (x as f64 * 3.0 / 4.0) as i32),
            _ => (384, 288),
        }
    }
    /// Value of a `Key: value` line inside `[Aegisub Project Garbage]` (e.g. "Video File").
    pub fn garbage(&self, key: &str) -> Option<String> {
        let sec = self.extras.iter().find(|x| x.header.eq_ignore_ascii_case("[Aegisub Project Garbage]"))?;
        sec.lines.iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            if k.trim().eq_ignore_ascii_case(key) { Some(v.trim().to_string()) } else { None }
        })
    }

    pub fn set_garbage(&mut self, key: &str, value: &str) {
        let idx = self.extras.iter().position(|x| x.header.eq_ignore_ascii_case("[Aegisub Project Garbage]"));
        let idx = match idx {
            Some(i) => i,
            None => {
                self.extras.push(RawSection { header: "[Aegisub Project Garbage]".into(), lines: vec![], after_events: false });
                self.extras.len() - 1
            }
        };
        let lines = &mut self.extras[idx].lines;
        let new = format!("{}: {}", key, value);
        match lines.iter_mut().find(|l| l.split_once(':').map(|(k, _)| k.trim().eq_ignore_ascii_case(key)).unwrap_or(false)) {
            Some(l) => *l = new,
            None => lines.push(new),
        }
    }

    pub fn sort_events_by_start(&mut self) {
        let first = self.entries.iter().position(|e| matches!(e, Entry::Event(_)));
        if let Some(first) = first {
            let tail: Vec<Entry> = self.entries.drain(first..).collect();
            let (mut evs, rest): (Vec<_>, Vec<_>) = tail.into_iter().partition(|e| matches!(e, Entry::Event(_)));
            evs.sort_by_key(|e| if let Entry::Event(ev) = e { (ev.start, ev.layer) } else { (0, 0) });
            self.entries.extend(evs);
            self.entries.extend(rest);
        }
    }
}

fn write_raw(out: &mut String, x: &RawSection) {
    out.push_str(&x.header);
    out.push('\n');
    let mut lines = x.lines.clone();
    while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    out.push('\n');
}

pub fn format_style(s: &Style) -> String {
    let b = |v: bool| if v { "-1" } else { "0" };
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name,
        s.fontname,
        fmt_num(s.fontsize),
        fmt_color(s.colors[0]),
        fmt_color(s.colors[1]),
        fmt_color(s.colors[2]),
        fmt_color(s.colors[3]),
        b(s.bold),
        b(s.italic),
        b(s.underline),
        b(s.strikeout),
        fmt_num(s.scale_x),
        fmt_num(s.scale_y),
        fmt_num(s.spacing),
        fmt_num(s.angle),
        s.border_style,
        fmt_num(s.outline),
        fmt_num(s.shadow),
        s.align,
        s.margin_l,
        s.margin_r,
        s.margin_v,
        s.encoding
    )
}

pub fn format_event(e: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{},{},{},{},{}",
        if e.comment { "Comment" } else { "Dialogue" },
        e.layer,
        fmt_ass_time(e.start),
        fmt_ass_time(e.end),
        e.style,
        e.actor,
        e.margin_l,
        e.margin_r,
        e.margin_v,
        e.effect,
        e.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "\u{feff}[Script Info]\r\n; hi\r\nTitle: T\r\nPlayResX: 1280\r\nPlayResY: 720\r\n\r\n[V4+ Styles]\r\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\r\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\r\n\r\n[Aegisub Project Garbage]\r\nAudio File: a.mka\r\n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: 0,0:00:01.00,0:00:02.50,Default,,0,0,0,,{\\k50}Hel{\\k50}lo, world\r\nComment: 1,0:00:03.00,0:00:04.00,Default,act,0,0,0,fx, note\r\n";
    #[test]
    fn ssa_v4_import() {
        let ssa = "[Script Info]\nScriptType: v4.00\n\n[V4 Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\nStyle: Default,Arial,20,16777215,255,0,0,-1,0,1,2,2,6,10,10,10,0,1\n\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: Marked=0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Hi, there\n";
        let d = Document::parse(ssa);
        let st = d.style("Default").unwrap();
        assert_eq!(st.colors[0], 16777215);
        assert_eq!(st.align, 8, "SSA 6 (top-center) -> ASS numpad 8");
        assert!(st.bold);
        assert_eq!(d.events().next().unwrap().text, "Hi, there");
        assert_eq!(d.info("ScriptType"), Some("v4.00+"));
        assert!(d.to_string().contains("[V4+ Styles]"));
    }

    #[test]
    fn garbage_get_set() {
        let mut d = Document::parse(SAMPLE);
        assert_eq!(d.garbage("audio file").as_deref(), Some("a.mka"));
        d.set_garbage("Video File", "../v.mkv");
        d.set_garbage("Audio File", "b.mka");
        assert_eq!(d.garbage("Video File").as_deref(), Some("../v.mkv"));
        let out = d.to_string();
        assert!(out.contains("Audio File: b.mka") && out.contains("Video File: ../v.mkv"));
        assert!(!out.contains("a.mka"));
        let mut n = Document::new_default();
        n.set_garbage("Video File", "x.mkv");
        assert!(n.to_string().contains("[Aegisub Project Garbage]"));
    }

    #[test]
    fn parse_write_roundtrip() {
        let d = Document::parse(SAMPLE);
        assert_eq!(d.events().count(), 2);
        assert_eq!(d.res(), (1280, 720));
        let e: Vec<_> = d.events().collect();
        assert_eq!(e[0].text, "{\\k50}Hel{\\k50}lo, world");
        assert_eq!(e[1].text, " note");
        assert!(e[1].comment);
        assert!(d.style("Default").unwrap().bold);
        let out = d.to_string();
        let d2 = Document::parse(&out);
        assert_eq!(d, d2);
        assert!(out.contains("[Aegisub Project Garbage]"));
    }
}
