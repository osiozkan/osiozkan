//! ASS/SSA modeli, okuyucu ve yazıcı. Bilinmeyen bölümler ve yorumlar korunur.
use crate::time;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Dialogue,
    Comment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: EventKind,
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
            kind: EventKind::Dialogue,
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
    pub fn is_comment(&self) -> bool {
        self.kind == EventKind::Comment
    }
    pub fn to_line(&self) -> String {
        format!(
            "{}: {},{},{},{},{},{},{},{},{},{}",
            if self.is_comment() { "Comment" } else { "Dialogue" },
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

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub primary: String,
    pub secondary: String,
    pub outline_color: String,
    pub back: String,
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
            primary: "&H00FFFFFF".into(),
            secondary: "&H000000FF".into(),
            outline_color: "&H00000000".into(),
            back: "&H00000000".into(),
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

fn num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
fn flag(b: bool) -> &'static str {
    if b { "-1" } else { "0" }
}

impl Style {
    pub fn to_line(&self) -> String {
        format!(
            "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.name, self.fontname, num(self.fontsize), self.primary, self.secondary,
            self.outline_color, self.back, flag(self.bold), flag(self.italic), flag(self.underline),
            flag(self.strikeout), num(self.scale_x), num(self.scale_y), num(self.spacing),
            num(self.angle), self.border_style, num(self.outline), num(self.shadow),
            self.alignment, self.margin_l, self.margin_r, self.margin_v, self.encoding
        )
    }
}

pub const STYLE_FORMAT: &str = "Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding";
pub const EVENT_FORMAT: &str = "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text";

#[derive(Debug, Clone, Default)]
pub struct Section {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Document {
    /// (anahtar, değer); ";" ile başlayan anahtar = yorum satırı (değer satırın kalanı).
    pub info: Vec<(String, String)>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    /// Garbage/Fonts/Graphics/Extradata gibi bilinmeyen bölümler (ham satırlar).
    pub extra: Vec<Section>,
    /// Bölüm yazım sırası ("Script Info", "V4+ Styles", "Events" ya da extra adları).
    pub order: Vec<String>,
    pub warnings: Vec<String>,
}

impl Default for Document {
    fn default() -> Self {
        let mut d = Document {
            info: vec![
                ("Title".into(), "Untitled".into()),
                ("ScriptType".into(), "v4.00+".into()),
                ("WrapStyle".into(), "0".into()),
                ("ScaledBorderAndShadow".into(), "yes".into()),
                ("YCbCr Matrix".into(), "None".into()),
                ("PlayResX".into(), "1920".into()),
                ("PlayResY".into(), "1080".into()),
            ],
            styles: vec![Style::default()],
            events: vec![],
            extra: vec![],
            order: vec![],
            warnings: vec![],
        };
        d.order = vec!["Script Info".into(), "V4+ Styles".into(), "Events".into()];
        d
    }
}

fn split_n<'a>(s: &'a str, n: usize) -> Vec<&'a str> {
    s.splitn(n, ',').collect()
}

fn pf(s: &str, d: f64) -> f64 {
    s.trim().parse().unwrap_or(d)
}
fn pi(s: &str, d: i32) -> i32 {
    s.trim().parse::<f64>().map(|v| v as i32).unwrap_or(d)
}

/// SSA onluk BGR renk -> &HAABBGGRR
fn norm_color(s: &str) -> String {
    let t = s.trim();
    if t.starts_with('&') || t.starts_with('#') {
        let hex = t.trim_start_matches('&').trim_start_matches(['H', 'h']).trim_end_matches('&');
        let hex = hex.trim_start_matches('#');
        return format!("&H{:0>8}", hex.to_uppercase());
    }
    match t.parse::<i64>() {
        Ok(v) => format!("&H{:08X}", (v as u32)),
        Err(_) => "&H00FFFFFF".into(),
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn info_get(&self, key: &str) -> Option<&str> {
        self.info
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
    pub fn info_set(&mut self, key: &str, val: &str) {
        if let Some(e) = self.info.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            e.1 = val.into();
        } else {
            self.info.push((key.into(), val.into()));
        }
    }
    pub fn play_res(&self) -> (i32, i32) {
        let x = self.info_get("PlayResX").and_then(|v| v.trim().parse().ok());
        let y = self.info_get("PlayResY").and_then(|v| v.trim().parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (None, Some(y)) => (y * 4 / 3, y),
            (Some(x), None) => (x, x * 3 / 4),
            _ => (384, 288),
        }
    }
    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles.iter().find(|s| s.name == name)
    }

    pub fn parse(input: &str) -> Document {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut doc = Document { info: vec![], styles: vec![], events: vec![], extra: vec![], order: vec![], warnings: vec![] };
        let mut section = String::new();
        let mut style_fmt: Vec<String> = default_fmt(STYLE_FORMAT);
        let mut event_fmt: Vec<String> = default_fmt(EVENT_FORMAT);
        let mut ssa = false;
        for (ln, raw) in input.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                section = t[1..t.len() - 1].to_string();
                let low = section.to_ascii_lowercase();
                let canon = match low.as_str() {
                    "script info" => "Script Info".to_string(),
                    "v4+ styles" | "v4 styles" => {
                        ssa = low == "v4 styles";
                        "V4+ Styles".to_string()
                    }
                    "events" => "Events".to_string(),
                    _ => {
                        doc.extra.push(Section { name: section.clone(), lines: vec![] });
                        section.clone()
                    }
                };
                section = canon.clone();
                if !doc.order.contains(&canon) {
                    doc.order.push(canon);
                }
                continue;
            }
            match section.as_str() {
                "Script Info" => {
                    if t.is_empty() {
                        continue;
                    }
                    if let Some(c) = t.strip_prefix(';') {
                        doc.info.push((";".into(), c.to_string()));
                    } else if let Some((k, v)) = t.split_once(':') {
                        doc.info.push((k.trim().to_string(), v.trim().to_string()));
                    }
                }
                "V4+ Styles" => {
                    if t.is_empty() || t.starts_with(';') { continue; }
                    if let Some(f) = strip_ci(t, "Format:") {
                        style_fmt = f.split(',').map(|x| x.trim().to_ascii_lowercase()).collect();
                    } else if let Some(v) = strip_ci(t, "Style:") {
                        let cells = split_n(v.trim_start(), style_fmt.len());
                        doc.styles.push(parse_style(&style_fmt, &cells, ssa));
                    }
                }
                "Events" => {
                    if t.is_empty() { continue; }
                    if let Some(f) = strip_ci(t, "Format:") {
                        event_fmt = f.split(',').map(|x| x.trim().to_ascii_lowercase()).collect();
                    } else {
                        let (kind, rest) = if let Some(r) = strip_ci(t, "Dialogue:") {
                            (EventKind::Dialogue, r)
                        } else if let Some(r) = strip_ci(t, "Comment:") {
                            (EventKind::Comment, r)
                        } else {
                            continue; // Picture:, Sound: vb. yoksayılır
                        };
                        let cells = split_n(rest.strip_prefix(' ').unwrap_or(rest), event_fmt.len());
                        match parse_event(&event_fmt, &cells, kind) {
                            Some(e) => doc.events.push(e),
                            None => doc.warnings.push(format!("Satır {}: geçersiz olay atlandı", ln + 1)),
                        }
                    }
                }
                _ => {
                    if let Some(sec) = doc.extra.iter_mut().rev().find(|s| s.name == section) {
                        sec.lines.push(line.to_string());
                    } else if !t.is_empty() {
                        // bölüm başlığından önceki satırlar
                        doc.warnings.push(format!("Satır {}: bölüm dışı satır yoksayıldı", ln + 1));
                    }
                }
            }
        }
        for n in ["Script Info", "V4+ Styles", "Events"] {
            if !doc.order.iter().any(|o| o == n) {
                doc.order.push(n.into());
            }
        }
        if doc.styles.is_empty() {
            doc.styles.push(Style::default());
        }
        doc
    }

    pub fn to_string(&self) -> String {
        let mut o = String::new();
        for name in &self.order {
            match name.as_str() {
                "Script Info" => {
                    o.push_str("[Script Info]\n");
                    for (k, v) in &self.info {
                        if k == ";" { o.push_str(&format!(";{}\n", v)); } else { o.push_str(&format!("{}: {}\n", k, v)); }
                    }
                }
                "V4+ Styles" => {
                    o.push_str("[V4+ Styles]\nFormat: ");
                    o.push_str(STYLE_FORMAT);
                    o.push('\n');
                    for s in &self.styles { o.push_str(&s.to_line()); o.push('\n'); }
                }
                "Events" => {
                    o.push_str("[Events]\nFormat: ");
                    o.push_str(EVENT_FORMAT);
                    o.push('\n');
                    for e in &self.events { o.push_str(&e.to_line()); o.push('\n'); }
                }
                other => {
                    if let Some(sec) = self.extra.iter().find(|s| s.name == other) {
                        o.push_str(&format!("[{}]\n", sec.name));
                        for l in &sec.lines { o.push_str(l); o.push('\n'); }
                        if sec.lines.last().map_or(false, |l| l.is_empty()) { o.pop(); }
                    }
                }
            }
            o.push('\n');
        }
        while o.ends_with("\n\n") { o.pop(); }
        o
    }

    pub fn sort_by_start(&mut self) {
        self.events.sort_by_key(|e| (e.start, e.end));
    }
}

fn default_fmt(f: &str) -> Vec<String> {
    f.split(',').map(|x| x.trim().to_ascii_lowercase()).collect()
}

fn strip_ci<'a>(s: &'a str, p: &str) -> Option<&'a str> {
    if s.len() >= p.len() && s.is_char_boundary(p.len()) && s[..p.len()].eq_ignore_ascii_case(p) {
        Some(&s[p.len()..])
    } else {
        None
    }
}

fn parse_event(fmt: &[String], cells: &[&str], kind: EventKind) -> Option<Event> {
    let mut e = Event { kind, ..Event::default() };
    let mut got_start = false;
    let mut got_end = false;
    for (i, name) in fmt.iter().enumerate() {
        let c = cells.get(i).copied().unwrap_or("");
        match name.as_str() {
            "layer" | "marked" => e.layer = pi(&c.replace("Marked=", ""), 0),
            "start" => { e.start = time::parse_ass(c)?; got_start = true; }
            "end" => { e.end = time::parse_ass(c)?; got_end = true; }
            "style" => e.style = c.trim().trim_start_matches('*').to_string(),
            "name" | "actor" => e.actor = c.trim().to_string(),
            "marginl" => e.margin_l = pi(c, 0),
            "marginr" => e.margin_r = pi(c, 0),
            "marginv" => e.margin_v = pi(c, 0),
            "effect" => e.effect = c.trim().to_string(),
            "text" => e.text = c.to_string(),
            _ => {}
        }
    }
    if got_start && got_end { Some(e) } else { None }
}

fn parse_style(fmt: &[String], cells: &[&str], ssa: bool) -> Style {
    let mut s = Style::default();
    for (i, name) in fmt.iter().enumerate() {
        let c = cells.get(i).copied().unwrap_or("").trim();
        match name.as_str() {
            "name" => s.name = c.to_string(),
            "fontname" => s.fontname = c.to_string(),
            "fontsize" => s.fontsize = pf(c, 48.0),
            "primarycolour" => s.primary = norm_color(c),
            "secondarycolour" => s.secondary = norm_color(c),
            "outlinecolour" | "tertiarycolour" => s.outline_color = norm_color(c),
            "backcolour" => s.back = norm_color(c),
            "bold" => s.bold = pi(c, 0) != 0,
            "italic" => s.italic = pi(c, 0) != 0,
            "underline" => s.underline = pi(c, 0) != 0,
            "strikeout" => s.strikeout = pi(c, 0) != 0,
            "scalex" => s.scale_x = pf(c, 100.0),
            "scaley" => s.scale_y = pf(c, 100.0),
            "spacing" => s.spacing = pf(c, 0.0),
            "angle" => s.angle = pf(c, 0.0),
            "borderstyle" => s.border_style = pi(c, 1),
            "outline" => s.outline = pf(c, 2.0),
            "shadow" => s.shadow = pf(c, 2.0),
            "alignment" => {
                let a = pi(c, 2);
                s.alignment = if ssa { ssa_align(a) } else { a };
            }
            "marginl" => s.margin_l = pi(c, 10),
            "marginr" => s.margin_r = pi(c, 10),
            "marginv" => s.margin_v = pi(c, 10),
            "encoding" => s.encoding = pi(c, 1),
            _ => {}
        }
    }
    s
}

/// SSA hizalamasını (1-3 alt, 5-7 üst, 9-11 orta) numpad düzenine çevirir.
fn ssa_align(a: i32) -> i32 {
    match a {
        1..=3 => a,
        5..=7 => a - 5 + 7,
        9..=11 => a - 9 + 4,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "\u{feff}[Script Info]\r\n; yorum\r\nTitle: Test\r\nPlayResX: 1280\r\nPlayResY: 720\r\n\r\n[Aegisub Project Garbage]\r\nLast Style Storage: Default\r\n\r\n[V4+ Styles]\r\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\r\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\r\n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Merhaba, {\\i1}dünya{\\i0}\r\nComment: 0,0:00:04.00,0:00:05.00,Default,x,0,0,0,fx,not\r\n";
    #[test]
    fn parse_and_roundtrip() {
        let d = Document::parse(SAMPLE);
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].text, "Merhaba, {\\i1}dünya{\\i0}");
        assert_eq!(d.events[0].start, 1000);
        assert!(d.events[1].is_comment());
        assert!(d.styles[0].bold);
        assert_eq!(d.play_res(), (1280, 720));
        let out = d.to_string();
        let d2 = Document::parse(&out);
        assert_eq!(d.events, d2.events);
        assert_eq!(d.styles, d2.styles);
        assert!(out.contains("[Aegisub Project Garbage]"));
        assert!(out.find("Garbage").unwrap() < out.find("[V4+ Styles]").unwrap());
    }
    #[test]
    fn shuffled_format() {
        let d = Document::parse("[Events]\nFormat: Start, End, Text, Style\nDialogue: 0:00:01.00,0:00:02.00,a,b,c,Alt\n");
        // Text sınırlı sayıda bölünür: son sütun Style olduğundan "a,b,c" bölünür
        assert_eq!(d.events.len(), 1);
    }
}
