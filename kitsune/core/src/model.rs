//! Belge modeli. Aegisub'daki gibi düz bir satır listesi görünümü `Document::entry_count` ile sunulur.

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
    pub fn duration(&self) -> i64 {
        self.end - self.start
    }
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub info: Vec<(String, String)>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    /// Bilinmeyen bölümler (Fonts, Graphics, Aegisub Project Garbage…) aynen korunur.
    pub extra_sections: Vec<(String, Vec<String>)>,
}

impl Document {
    /// Boş, varsayılan stilli yeni belge.
    pub fn new() -> Self {
        let mut d = Document::default();
        d.info.push(("ScriptType".into(), "v4.00+".into()));
        d.info.push(("PlayResX".into(), "1920".into()));
        d.info.push(("PlayResY".into(), "1080".into()));
        d.info.push(("WrapStyle".into(), "0".into()));
        d.info.push(("ScaledBorderAndShadow".into(), "yes".into()));
        d.styles.push(Style::default());
        d
    }

    pub fn entry_count(&self) -> usize {
        self.info.len() + self.styles.len() + self.events.len()
    }

    pub fn info_get(&self, key: &str) -> Option<&str> {
        self.info.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn info_set(&mut self, key: &str, value: &str) {
        match self.info.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(e) => e.1 = value.to_string(),
            None => self.info.push((key.to_string(), value.to_string())),
        }
    }

    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles.iter().find(|s| s.name == name)
    }

    pub fn play_res(&self) -> (i32, i32) {
        let g = |k: &str, d: i32| self.info_get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        let x = g("PlayResX", 384);
        let y = g("PlayResY", 288);
        (x, y)
    }

    pub fn sort_by_start(&mut self) {
        self.events.sort_by_key(|e| (e.start, e.end));
    }
}
