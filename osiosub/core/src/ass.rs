//! ASS / SSA data model, parser and writer. Unknown sections are preserved verbatim.
use crate::color::Color;
use crate::time;

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub primary: Color,
    pub secondary: Color,
    pub outline_color: Color,
    pub back: Color,
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
    pub alignment: i32,
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
            primary: Color(0x00FFFFFF),
            secondary: Color(0x000000FF),
            outline_color: Color(0x00000000),
            back: Color(0x00000000),
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
            alignment: 2,
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
            comment: false, layer: 0, start: 0, end: 5000, style: "Default".into(),
            actor: String::new(), margin_l: 0, margin_r: 0, margin_v: 0,
            effect: String::new(), text: String::new(),
        }
    }
}

/// A section we do not interpret (e.g. `[Fonts]`, `[Graphics]`, `[Aegisub Extradata]`).
#[derive(Clone, Debug, PartialEq)]
pub struct RawSection {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssFile {
    /// `[Script Info]` key/value pairs in file order (comments kept as key `;`).
    pub info: Vec<(String, String)>,
    /// `[Aegisub Project Garbage]` key/value pairs.
    pub garbage: Vec<(String, String)>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    pub extra: Vec<RawSection>,
}

#[derive(Debug)]
pub struct ParseError(pub String);
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}
impl std::error::Error for ParseError {}

fn pf(s: &str) -> f64 { s.trim().parse().unwrap_or(0.0) }
fn pi(s: &str) -> i32 { s.trim().parse::<f64>().map(|v| v as i32).unwrap_or(0) }
fn pb(s: &str) -> bool { pi(s) != 0 }

fn fnum(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

impl AssFile {
    /// New file with Aegisub-like defaults.
    pub fn new_default() -> Self {
        let mut f = AssFile::default();
        f.info = vec![
            ("Title".into(), "Default OsioSub file".into()),
            ("ScriptType".into(), "v4.00+".into()),
            ("WrapStyle".into(), "0".into()),
            ("ScaledBorderAndShadow".into(), "yes".into()),
            ("YCbCr Matrix".into(), "None".into()),
            ("PlayResX".into(), "1920".into()),
            ("PlayResY".into(), "1080".into()),
        ];
        f.styles.push(Style::default());
        f
    }

    pub fn info_get(&self, key: &str) -> Option<&str> {
        self.info.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
    pub fn info_set(&mut self, key: &str, val: &str) {
        if let Some(e) = self.info.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            e.1 = val.to_string();
        } else {
            self.info.push((key.to_string(), val.to_string()));
        }
    }
    pub fn play_res(&self) -> (i32, i32) {
        let x = self.info_get("PlayResX").and_then(|v| v.trim().parse().ok());
        let y = self.info_get("PlayResY").and_then(|v| v.trim().parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => (x, x * 3 / 4),
            (None, Some(y)) => (y * 4 / 3, y),
            _ => (384, 288),
        }
    }
    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles.iter().find(|s| s.name == name)
    }

    pub fn parse(input: &str) -> Result<AssFile, ParseError> {
        let input = input.trim_start_matches('\u{feff}');
        let mut f = AssFile::default();
        #[derive(PartialEq)]
        enum Sec { None, Info, Styles, Events, Garbage, Other }
        let mut sec = Sec::None;
        let mut style_fmt: Vec<String> = default_style_format();
        let mut event_fmt: Vec<String> = default_event_format();
        let mut v4 = false;
        let mut saw_any = false;
        for raw in input.lines() {
            let line = raw.trim_end_matches('\r');
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                let name = &t[1..t.len() - 1];
                saw_any = true;
                let l = name.to_ascii_lowercase();
                sec = match l.as_str() {
                    "script info" => Sec::Info,
                    "v4+ styles" => { v4 = false; Sec::Styles }
                    "v4 styles" => { v4 = true; Sec::Styles }
                    "events" => Sec::Events,
                    "aegisub project garbage" => Sec::Garbage,
                    _ => {
                        f.extra.push(RawSection { name: name.to_string(), lines: vec![] });
                        Sec::Other
                    }
                };
                continue;
            }
            if t.is_empty() { continue; }
            match sec {
                Sec::None => {}
                Sec::Info | Sec::Garbage => {
                    let target = if sec == Sec::Info { &mut f.info } else { &mut f.garbage };
                    if t.starts_with(';') {
                        target.push((";".into(), t[1..].trim_start().to_string()));
                    } else if let Some(i) = t.find(':') {
                        target.push((t[..i].trim().to_string(), t[i + 1..].trim().to_string()));
                    }
                }
                Sec::Styles => {
                    if let Some(rest) = strip_ci(t, "format:") {
                        style_fmt = rest.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                    } else if let Some(rest) = strip_ci(t, "style:") {
                        f.styles.push(parse_style(rest, &style_fmt, v4));
                    }
                }
                Sec::Events => {
                    if let Some(rest) = strip_ci(t, "format:") {
                        event_fmt = rest.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                    } else if let Some(rest) = strip_ci(t, "dialogue:") {
                        f.events.push(parse_event(rest, &event_fmt, false));
                    } else if let Some(rest) = strip_ci(t, "comment:") {
                        f.events.push(parse_event(rest, &event_fmt, true));
                    }
                    // picture/sound/movie/command lines are dropped (Aegisub drops them too)
                }
                Sec::Other => {
                    if let Some(s) = f.extra.last_mut() { s.lines.push(line.to_string()); }
                }
            }
        }
        if !saw_any {
            return Err(ParseError("not an ASS/SSA file (no sections)".into()));
        }
        if f.styles.is_empty() { f.styles.push(Style::default()); }
        Ok(f)
    }

    pub fn to_string(&self) -> String {
        let mut o = String::with_capacity(self.events.len() * 80 + 2048);
        o.push_str("[Script Info]\n");
        for (k, v) in &self.info {
            if k == ";" { o.push_str(&format!("; {}\n", v)); } else { o.push_str(&format!("{}: {}\n", k, v)); }
        }
        if !self.garbage.is_empty() {
            o.push_str("\n[Aegisub Project Garbage]\n");
            for (k, v) in &self.garbage { o.push_str(&format!("{}: {}\n", k, v)); }
        }
        o.push_str("\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
        for s in &self.styles { o.push_str(&format!("Style: {}\n", style_line(s))); }
        o.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        for e in &self.events { o.push_str(&event_line(e)); o.push('\n'); }
        for s in &self.extra {
            o.push_str(&format!("\n[{}]\n", s.name));
            for l in &s.lines { o.push_str(l); o.push('\n'); }
        }
        o
    }
}

pub fn style_line(s: &Style) -> String {
    let b = |v: bool| if v { "-1" } else { "0" };
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name, s.fontname, fnum(s.fontsize), s.primary.to_style(), s.secondary.to_style(),
        s.outline_color.to_style(), s.back.to_style(), b(s.bold), b(s.italic), b(s.underline),
        b(s.strikeout), fnum(s.scale_x), fnum(s.scale_y), fnum(s.spacing), fnum(s.angle),
        s.border_style, fnum(s.outline), fnum(s.shadow), s.alignment, s.margin_l, s.margin_r,
        s.margin_v, s.encoding
    )
}

pub fn event_line(e: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{:04},{:04},{:04},{},{}",
        if e.comment { "Comment" } else { "Dialogue" },
        e.layer, time::fmt_ass(e.start), time::fmt_ass(e.end), e.style, e.actor,
        e.margin_l, e.margin_r, e.margin_v, e.effect, e.text
    )
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(s[prefix.len()..].trim_start())
    } else { None }
}

fn default_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,outlinecolour,backcolour,bold,italic,underline,strikeout,scalex,scaley,spacing,angle,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,encoding"
        .split(',').map(String::from).collect()
}
fn default_event_format() -> Vec<String> {
    "layer,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect()
}

fn parse_style(rest: &str, fmt: &[String], v4: bool) -> Style {
    let mut s = Style::default();
    let n = fmt.len().max(1);
    let parts: Vec<&str> = rest.splitn(n, ',').collect();
    for (i, key) in fmt.iter().enumerate() {
        let Some(v) = parts.get(i).map(|v| v.trim()) else { continue };
        match key.as_str() {
            "name" => s.name = v.to_string(),
            "fontname" => s.fontname = v.to_string(),
            "fontsize" => s.fontsize = pf(v),
            "primarycolour" | "primarycolor" => s.primary = Color::parse(v).unwrap_or(s.primary),
            "secondarycolour" | "secondarycolor" => s.secondary = Color::parse(v).unwrap_or(s.secondary),
            "tertiarycolour" | "outlinecolour" | "outlinecolor" => s.outline_color = Color::parse(v).unwrap_or(s.outline_color),
            "backcolour" | "backcolor" => s.back = Color::parse(v).unwrap_or(s.back),
            "bold" => s.bold = pb(v),
            "italic" => s.italic = pb(v),
            "underline" => s.underline = pb(v),
            "strikeout" => s.strikeout = pb(v),
            "scalex" => s.scale_x = pf(v),
            "scaley" => s.scale_y = pf(v),
            "spacing" => s.spacing = pf(v),
            "angle" => s.angle = pf(v),
            "borderstyle" => s.border_style = pi(v),
            "outline" => s.outline = pf(v),
            "shadow" => s.shadow = pf(v),
            "alignment" => {
                let a = pi(v);
                s.alignment = if v4 { match a { 1..=3 => a, 5..=7 => a - 5 + 7, 9..=11 => a - 9 + 4, _ => 2 } } else { a };
            }
            "marginl" => s.margin_l = pi(v),
            "marginr" => s.margin_r = pi(v),
            "marginv" => s.margin_v = pi(v),
            "encoding" => s.encoding = pi(v),
            _ => {}
        }
    }
    s
}

fn parse_event(rest: &str, fmt: &[String], comment: bool) -> Event {
    let mut e = Event { comment, ..Default::default() };
    // Text is always last: split into fmt.len() fields, the last receives the remainder (commas intact).
    let n = fmt.len().max(1);
    let parts: Vec<&str> = rest.splitn(n, ',').collect();
    for (i, key) in fmt.iter().enumerate() {
        let Some(v) = parts.get(i) else { continue };
        match key.as_str() {
            "layer" | "marked" => e.layer = pi(&v.replace("Marked=", "")),
            "start" => e.start = time::parse_ass(v).unwrap_or(0),
            "end" => e.end = time::parse_ass(v).unwrap_or(0),
            "style" => e.style = v.trim().trim_start_matches('*').to_string(),
            "name" | "actor" => e.actor = v.trim().to_string(),
            "marginl" => e.margin_l = pi(v),
            "marginr" => e.margin_r = pi(v),
            "marginv" => e.margin_v = pi(v),
            "marginb" | "margint" => {}
            "effect" => e.effect = v.trim().to_string(),
            "text" => e.text = v.to_string(),
            _ => {}
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "\u{feff}[Script Info]\nTitle: t\nPlayResX: 640\nPlayResY: 480\n\n[Aegisub Project Garbage]\nVideo File: a.mkv\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Merhaba, dünya\\NIkinci satır\nComment: 1,0:00:04.00,0:00:05.00,Default,x,0,0,0,fx,{\\k20}no\n\n[Fonts]\nfontname: x\n abc\n";
    #[test]
    fn roundtrip() {
        let f = AssFile::parse(SAMPLE).unwrap();
        assert_eq!(f.events.len(), 2);
        assert_eq!(f.events[0].text, "Merhaba, dünya\\NIkinci satır");
        assert_eq!(f.events[0].end, 3500);
        assert!(f.events[1].comment);
        assert_eq!(f.play_res(), (640, 480));
        assert_eq!(f.extra[0].name, "Fonts");
        let out = f.to_string();
        let g = AssFile::parse(&out).unwrap();
        assert_eq!(f, g);
    }
    #[test]
    fn reordered_format() {
        let s = "[Events]\nFormat: Start, End, Text, Style, Layer\nDialogue: 0:00:01.00,0:00:02.00,hi,there,1,0,0,0,,x\n";
        // text field is not last here: parser takes field-by-position, remainder lands in last field (Layer)
        let f = AssFile::parse(s).unwrap();
        assert_eq!(f.events[0].start, 1000);
    }
}
