//! ASS/SSA belge modeli, ayrıştırıcı ve yazıcı.
//!
//! Tasarım hedefleri: kayıpsız gidiş-dönüş (bilinmeyen bölümler korunur),
//! hataya dayanıklı ayrıştırma (asla panik/hata fırlatmaz, uyarı toplar),
//! SSA (v4.00) içe aktarma, Aegisub'un yazdığı biçimle uyumlu çıktı.
use crate::color::Color;
use crate::time;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InfoLine {
    Pair { key: String, value: String },
    /// `;` ile başlayan yorum ya da anlaşılmayan satır.
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
            primary: Color::WHITE,
            secondary: Color { r: 255, g: 0, b: 0, a: 0 },
            outline_color: Color::BLACK,
            back: Color::BLACK,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Bilinmeyen/ham bölüm (Fonts, Graphics, Aegisub Project Garbage, Extradata...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSection {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Document {
    pub info: Vec<InfoLine>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    pub extra: Vec<RawSection>,
    /// Ayrıştırma sırasında toplanan uyarılar (yazılmaz).
    #[serde(skip)]
    pub warnings: Vec<String>,
}

const DEFAULT_STYLE_FORMAT: &[&str] = &[
    "name", "fontname", "fontsize", "primarycolour", "secondarycolour", "outlinecolour", "backcolour", "bold",
    "italic", "underline", "strikeout", "scalex", "scaley", "spacing", "angle", "borderstyle", "outline",
    "shadow", "alignment", "marginl", "marginr", "marginv", "encoding",
];
const DEFAULT_EVENT_FORMAT: &[&str] =
    &["layer", "start", "end", "style", "name", "marginl", "marginr", "marginv", "effect", "text"];

fn pnum(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(0.0)
}
fn pint(s: &str) -> i32 {
    let t = s.trim();
    t.parse::<i32>().unwrap_or_else(|_| t.parse::<f64>().map(|f| f as i32).unwrap_or(0))
}
fn pbool(s: &str) -> bool {
    pint(s) != 0
}

/// SSA hizalamasını (1-3 alt, 5-7 üst, 9-11 orta) numpad düzenine çevirir.
fn ssa_align_to_numpad(a: i32) -> i32 {
    match a {
        1..=3 => a,
        5..=7 => a - 5 + 7,
        9..=11 => a - 9 + 4,
        other => other,
    }
}

impl Document {
    /// Aegisub'un varsayılan boş belgesi gibi: tek "Default" stili.
    pub fn new_default() -> Self {
        let mut d = Document::default();
        d.set_info("ScriptType", "v4.00+");
        d.set_info("PlayResX", "1920");
        d.set_info("PlayResY", "1080");
        d.set_info("WrapStyle", "0");
        d.set_info("ScaledBorderAndShadow", "yes");
        d.set_info("YCbCr Matrix", "TV.709");
        d.styles.push(Style::default());
        d
    }

    pub fn get_info(&self, key: &str) -> Option<&str> {
        self.info.iter().find_map(|l| match l {
            InfoLine::Pair { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn set_info(&mut self, key: &str, value: &str) {
        for l in self.info.iter_mut() {
            if let InfoLine::Pair { key: k, value: v } = l {
                if k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        self.info.push(InfoLine::Pair { key: key.into(), value: value.into() });
    }

    pub fn play_res(&self) -> (i32, i32) {
        let x = self.get_info("PlayResX").and_then(|v| v.trim().parse().ok());
        let y = self.get_info("PlayResY").and_then(|v| v.trim().parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => (x, ((x as f64) * 3.0 / 4.0) as i32),
            (None, Some(y)) => (((y as f64) * 4.0 / 3.0) as i32, y),
            (None, None) => (384, 288),
        }
    }

    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles.iter().find(|s| s.name == name)
    }

    /// Stil adını belgede ve tüm olaylarda yeniden adlandırır.
    pub fn rename_style(&mut self, old: &str, new: &str) -> bool {
        let Some(st) = self.styles.iter_mut().find(|s| s.name == old) else { return false };
        st.name = new.to_string();
        for e in self.events.iter_mut().filter(|e| e.style == old) {
            e.style = new.to_string();
        }
        true
    }

    // ───────────────────────── Ayrıştırma ─────────────────────────

    pub fn parse(input: &str) -> Document {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut doc = Document::default();
        enum Sec {
            None,
            Info,
            Styles { ssa: bool },
            Events,
            Raw(usize),
        }
        let mut sec = Sec::None;
        let mut style_fmt: Vec<String> = DEFAULT_STYLE_FORMAT.iter().map(|s| s.to_string()).collect();
        let mut event_fmt: Vec<String> = DEFAULT_EVENT_FORMAT.iter().map(|s| s.to_string()).collect();
        let mut ssa_events = false;

        for (ln, raw) in input.split('\n').enumerate() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
                let name = &trimmed[1..trimmed.len() - 1];
                let lname = name.to_ascii_lowercase();
                sec = match lname.as_str() {
                    "script info" => Sec::Info,
                    "v4+ styles" | "v4 styles+" => Sec::Styles { ssa: false },
                    "v4 styles" => Sec::Styles { ssa: true },
                    "events" => Sec::Events,
                    _ => {
                        doc.extra.push(RawSection { name: name.to_string(), lines: Vec::new() });
                        Sec::Raw(doc.extra.len() - 1)
                    }
                };
                continue;
            }
            match sec {
                Sec::None => {
                    if !trimmed.is_empty() {
                        doc.warnings.push(format!("satır {}: bölüm başlığından önce içerik yok sayıldı", ln + 1));
                    }
                }
                Sec::Raw(i) => {
                    // Boş satırlar korunmaz ama içerik satırları aynen saklanır (font uuencode vb.).
                    if !trimmed.is_empty() {
                        doc.extra[i].lines.push(line.to_string());
                    }
                }
                Sec::Info => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.starts_with(';') || trimmed.starts_with('!') {
                        doc.info.push(InfoLine::Raw(line.to_string()));
                    } else if let Some((k, v)) = line.split_once(':') {
                        let (k, v) = (k.trim().to_string(), v.trim_start().to_string());
                        if k.eq_ignore_ascii_case("ScriptType") && v.trim().eq_ignore_ascii_case("v4.00") {
                            ssa_events = true;
                        }
                        doc.info.push(InfoLine::Pair { key: k, value: v });
                    } else {
                        doc.info.push(InfoLine::Raw(line.to_string()));
                    }
                }
                Sec::Styles { ssa } => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    let Some((tag, rest)) = trimmed.split_once(':') else { continue };
                    match tag.trim().to_ascii_lowercase().as_str() {
                        "format" => {
                            style_fmt = rest.split(',').map(|c| c.trim().to_ascii_lowercase()).collect();
                        }
                        "style" => {
                            let vals: Vec<&str> = rest.trim_start().split(',').collect();
                            doc.styles.push(parse_style(&style_fmt, &vals, ssa));
                        }
                        _ => {}
                    }
                }
                Sec::Events => {
                    if trimmed.is_empty() {
                        continue;
                    }
                    let Some((tag, rest)) = line.split_once(':') else { continue };
                    let tag = tag.trim().to_ascii_lowercase();
                    match tag.as_str() {
                        "format" => {
                            event_fmt = rest.split(',').map(|c| c.trim().to_ascii_lowercase()).collect();
                        }
                        "dialogue" | "comment" => {
                            let rest = rest.strip_prefix(' ').unwrap_or(rest);
                            let ncols = event_fmt.len().max(1);
                            let vals: Vec<&str> = rest.splitn(ncols, ',').collect();
                            match parse_event(&event_fmt, &vals, tag == "comment", ssa_events) {
                                Some(e) => doc.events.push(e),
                                None => doc.warnings.push(format!("satır {}: bozuk olay satırı atlandı", ln + 1)),
                            }
                        }
                        _ => doc.warnings.push(format!("satır {}: desteklenmeyen olay türü '{}'", ln + 1, tag)),
                    }
                }
            }
        }
        doc
    }

    // ───────────────────────── Yazma ─────────────────────────

    pub fn to_ass_string(&self) -> String {
        let mut out = String::with_capacity(self.events.len() * 90 + 1024);
        out.push_str("[Script Info]\n");
        let has_type = self.get_info("ScriptType").is_some();
        if !has_type {
            out.push_str("ScriptType: v4.00+\n");
        }
        for l in &self.info {
            match l {
                InfoLine::Pair { key, value } => {
                    if key.eq_ignore_ascii_case("ScriptType") {
                        // Çıktı her zaman ASS (v4.00+).
                        out.push_str("ScriptType: v4.00+\n");
                    } else {
                        out.push_str(key);
                        out.push_str(": ");
                        out.push_str(value);
                        out.push('\n');
                    }
                }
                InfoLine::Raw(r) => {
                    out.push_str(r);
                    out.push('\n');
                }
            }
        }
        out.push('\n');
        self.write_raw(&mut out, "Aegisub Project Garbage");
        out.push_str("[V4+ Styles]\n");
        out.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
        for s in &self.styles {
            out.push_str(&format!(
                "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                s.name,
                s.fontname,
                fnum(s.fontsize),
                s.primary.to_style_string(),
                s.secondary.to_style_string(),
                s.outline_color.to_style_string(),
                s.back.to_style_string(),
                if s.bold { -1 } else { 0 },
                if s.italic { -1 } else { 0 },
                if s.underline { -1 } else { 0 },
                if s.strikeout { -1 } else { 0 },
                fnum(s.scale_x),
                fnum(s.scale_y),
                fnum(s.spacing),
                fnum(s.angle),
                s.border_style,
                fnum(s.outline),
                fnum(s.shadow),
                s.alignment,
                s.margin_l,
                s.margin_r,
                s.margin_v,
                s.encoding
            ));
        }
        out.push('\n');
        self.write_raw(&mut out, "Fonts");
        self.write_raw(&mut out, "Graphics");
        out.push_str("[Events]\n");
        out.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        for e in &self.events {
            out.push_str(&e.to_line());
            out.push('\n');
        }
        out.push('\n');
        for sec in &self.extra {
            let n = sec.name.to_ascii_lowercase();
            if n != "aegisub project garbage" && n != "fonts" && n != "graphics" {
                self.write_raw(&mut out, &sec.name);
            }
        }
        out
    }

    fn write_raw(&self, out: &mut String, name: &str) {
        for sec in self.extra.iter().filter(|s| s.name.eq_ignore_ascii_case(name)) {
            out.push_str(&format!("[{}]\n", sec.name));
            for l in &sec.lines {
                out.push_str(l);
                out.push('\n');
            }
            out.push('\n');
        }
    }
}

impl Event {
    pub fn to_line(&self) -> String {
        format!(
            "{}: {},{},{},{},{},{},{},{},{},{}",
            if self.comment { "Comment" } else { "Dialogue" },
            self.layer,
            time::format_ass(self.start),
            time::format_ass(self.end),
            self.style,
            self.actor,
            self.margin_l,
            self.margin_r,
            self.margin_v,
            self.effect,
            self.text
        )
    }
}

fn fnum(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.6}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn parse_style(fmt: &[String], vals: &[&str], ssa: bool) -> Style {
    let mut s = Style::default();
    for (i, col) in fmt.iter().enumerate() {
        let Some(v) = vals.get(i) else { break };
        let v = v.trim();
        match col.as_str() {
            "name" => s.name = v.to_string(),
            "fontname" => s.fontname = v.to_string(),
            "fontsize" => s.fontsize = pnum(v),
            "primarycolour" => s.primary = Color::parse(v).unwrap_or(s.primary),
            "secondarycolour" => s.secondary = Color::parse(v).unwrap_or(s.secondary),
            "outlinecolour" | "tertiarycolour" => s.outline_color = Color::parse(v).unwrap_or(s.outline_color),
            "backcolour" => s.back = Color::parse(v).unwrap_or(s.back),
            "bold" => s.bold = pbool(v),
            "italic" => s.italic = pbool(v),
            "underline" => s.underline = pbool(v),
            "strikeout" => s.strikeout = pbool(v),
            "scalex" => s.scale_x = pnum(v),
            "scaley" => s.scale_y = pnum(v),
            "spacing" => s.spacing = pnum(v),
            "angle" => s.angle = pnum(v),
            "borderstyle" => s.border_style = pint(v),
            "outline" => s.outline = pnum(v),
            "shadow" => s.shadow = pnum(v),
            "alignment" => s.alignment = if ssa { ssa_align_to_numpad(pint(v)) } else { pint(v) },
            "marginl" => s.margin_l = pint(v),
            "marginr" => s.margin_r = pint(v),
            "marginv" => s.margin_v = pint(v),
            "encoding" => s.encoding = pint(v),
            _ => {}
        }
    }
    s
}

fn parse_event(fmt: &[String], vals: &[&str], comment: bool, _ssa: bool) -> Option<Event> {
    let mut e = Event { comment, ..Event::default() };
    let mut got_start = false;
    let mut got_end = false;
    for (i, col) in fmt.iter().enumerate() {
        let Some(v) = vals.get(i) else { break };
        match col.as_str() {
            "layer" | "marked" => {
                let t = v.trim().trim_start_matches("Marked=");
                e.layer = pint(t);
            }
            "start" => {
                e.start = time::parse_ass(v)?;
                got_start = true;
            }
            "end" => {
                e.end = time::parse_ass(v)?;
                got_end = true;
            }
            "style" => e.style = v.trim().trim_start_matches('*').to_string(),
            "name" | "actor" => e.actor = v.trim().to_string(),
            "marginl" => e.margin_l = pint(v),
            "marginr" => e.margin_r = pint(v),
            "marginv" | "margint" => e.margin_v = pint(v),
            "effect" => e.effect = v.trim().to_string(),
            "text" => e.text = v.to_string(),
            _ => {}
        }
    }
    if got_start && got_end {
        Some(e)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[Script Info]\r\n; comment\r\nTitle: Test\r\nScriptType: v4.00+\r\nPlayResX: 1280\r\nPlayResY: 720\r\n\r\n[V4+ Styles]\r\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\r\nStyle: Default,Arial,40.5,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\r\n\r\n[Fonts]\r\nfontname: x.ttf\r\nABCDEF\r\n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: 0,0:00:01.00,0:00:03.50,Default,Ali,0,0,0,,Merhaba, dünya {\\i1}test{\\i0}\r\nComment: 1,0:00:04.00,0:00:05.00,Default,,0,0,0,fx,not\r\n";

    #[test]
    fn parse_and_roundtrip() {
        let d = Document::parse(SAMPLE);
        assert_eq!(d.get_info("title"), Some("Test"));
        assert_eq!(d.play_res(), (1280, 720));
        assert_eq!(d.styles.len(), 1);
        assert!(d.styles[0].bold);
        assert_eq!(d.styles[0].fontsize, 40.5);
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].text, "Merhaba, dünya {\\i1}test{\\i0}");
        assert_eq!(d.events[0].start, 1000);
        assert!(d.events[1].comment);
        assert_eq!(d.extra[0].lines.len(), 2);
        let out = d.to_ass_string();
        let d2 = Document::parse(&out);
        assert_eq!(d.events, d2.events);
        assert_eq!(d.styles, d2.styles);
        assert_eq!(d.extra, d2.extra);
        assert_eq!(d.info, d2.info);
    }

    #[test]
    fn ssa_import() {
        let ssa = "[Script Info]\nScriptType: v4.00\n[V4 Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\nStyle: Top,Arial,20,16777215,255,0,0,0,0,1,2,2,6,10,10,10,0,0\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: Marked=0,0:00:01.00,0:00:02.00,Top,,0,0,0,,hi\n";
        let d = Document::parse(ssa);
        assert_eq!(d.styles[0].alignment, 8); // SSA 6 (üst orta) -> numpad 8
        assert_eq!(d.events[0].text, "hi");
        assert!(d.to_ass_string().contains("ScriptType: v4.00+"));
    }

    #[test]
    fn tolerant() {
        let d = Document::parse("garbage\n[Events]\nDialogue: broken\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,ok\n");
        assert_eq!(d.events.len(), 1);
        assert_eq!(d.warnings.len(), 2);
    }
}
