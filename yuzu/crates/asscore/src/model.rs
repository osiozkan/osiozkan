//! Subtitle document model (ASS/SSA superset).

use crate::time;

#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// ASS alpha: 0 = opaque, 255 = transparent.
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const RED: Color = Color { r: 255, g: 0, b: 0, a: 0 };

    /// Parse `&HAABBGGRR`, `&HBBGGRR&`, or a decimal long (SSA).
    pub fn parse(s: &str) -> Option<Color> {
        let t = s.trim().trim_end_matches('&');
        let v: u32 = if let Some(h) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")) {
            u32::from_str_radix(h, 16).ok()?
        } else if let Some(h) = t.strip_prefix("H").or_else(|| t.strip_prefix("h")) {
            u32::from_str_radix(h, 16).ok()?
        } else {
            t.parse::<i64>().ok()? as u32
        };
        Some(Color {
            a: (v >> 24) as u8,
            b: (v >> 16) as u8,
            g: (v >> 8) as u8,
            r: v as u8,
        })
    }
    /// Style form: `&HAABBGGRR`.
    pub fn to_style(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }
    /// Override-tag form: `&HBBGGRR&`.
    pub fn to_tag(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }
    pub fn alpha_tag(&self) -> String {
        format!("&H{:02X}&", self.a)
    }
}

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
            primary: Color::WHITE,
            secondary: Color::RED,
            outline_color: Color::BLACK,
            back: Color { r: 0, g: 0, b: 0, a: 0 },
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

/// A comma would end the field early in a `Dialogue:`/`Style:` row; Aegisub writes `;` instead.
pub fn safe_field(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains(',') { std::borrow::Cow::Owned(s.replace(',', ";")) } else { std::borrow::Cow::Borrowed(s) }
}

pub fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
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

impl Event {
    pub fn new(start: i64, end: i64, style: &str, text: &str) -> Event {
        Event { start, end, style: style.into(), text: text.into(), ..Default::default() }
    }
    pub fn duration(&self) -> i64 {
        self.end - self.start
    }
    /// Text with override blocks removed and `\N`/`\n`/`\h` normalised for display.
    pub fn plain_text(&self) -> String {
        crate::tags::strip_tags(&self.text)
    }
    pub fn to_ass(&self) -> String {
        format!(
            "{}: {},{},{},{},{},{},{},{},{},{}",
            if self.comment { "Comment" } else { "Dialogue" },
            self.layer,
            time::format_ass(self.start),
            time::format_ass(self.end),
            safe_field(&self.style),
            safe_field(&self.actor),
            self.margin_l,
            self.margin_r,
            self.margin_v,
            safe_field(&self.effect),
            self.text
        )
    }
}

/// Aegisub extradata entry (`[Aegisub Extradata]`).
#[derive(Clone, Debug, PartialEq)]
pub struct Extra {
    pub id: u32,
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    /// `[Script Info]` key/value pairs in file order. Comments are stored with key starting with `;`.
    pub info: Vec<(String, String)>,
    /// `[Aegisub Project Garbage]`
    pub garbage: Vec<(String, String)>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    pub extradata: Vec<Extra>,
    /// Unknown/raw sections (fonts, graphics) preserved verbatim: (header, lines)
    pub raw_sections: Vec<(String, Vec<String>)>,
}

impl Script {
    pub fn new() -> Script {
        let mut s = Script::default();
        s.info = vec![
            ("ScriptType".into(), "v4.00+".into()),
            ("WrapStyle".into(), "0".into()),
            ("ScaledBorderAndShadow".into(), "yes".into()),
            ("YCbCr Matrix".into(), "None".into()),
            ("PlayResX".into(), "1920".into()),
            ("PlayResY".into(), "1080".into()),
        ];
        s.styles.push(Style::default());
        s
    }
    pub fn info_get(&self, key: &str) -> Option<&str> {
        self.info.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
    pub fn info_set(&mut self, key: &str, value: &str) {
        if let Some(e) = self.info.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            e.1 = value.to_string();
        } else {
            self.info.push((key.to_string(), value.to_string()));
        }
    }
    pub fn res(&self) -> (i32, i32) {
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
    pub fn style_or_default(&self, name: &str) -> Style {
        self.style(name).or_else(|| self.style("Default")).cloned().unwrap_or_default()
    }
    pub fn sort_by_start(&mut self) {
        self.events.sort_by(|a, b| a.start.cmp(&b.start).then(a.layer.cmp(&b.layer)));
    }
    pub fn sort_by_end(&mut self) {
        self.events.sort_by(|a, b| a.end.cmp(&b.end).then(a.start.cmp(&b.start)));
    }
    pub fn sort_by_style(&mut self) {
        self.events.sort_by(|a, b| a.style.cmp(&b.style).then(a.start.cmp(&b.start)));
    }
}
