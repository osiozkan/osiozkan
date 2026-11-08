//! Flat subtitle model mirroring Aegisub's `subs` array so Lua macros map 1:1.

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub color1: String, // &HAABBGGRR
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
    pub margin_t: i32,
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(), fontname: "Arial".into(), fontsize: 48.0,
            color1: "&H00FFFFFF".into(), color2: "&H000000FF".into(),
            color3: "&H00000000".into(), color4: "&H00000000".into(),
            bold: false, italic: false, underline: false, strikeout: false,
            scale_x: 100.0, scale_y: 100.0, spacing: 0.0, angle: 0.0,
            borderstyle: 1, outline: 2.0, shadow: 2.0, align: 2,
            margin_l: 10, margin_r: 10, margin_t: 10, encoding: 1,
        }
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
    pub margin_t: i32,
    pub effect: String,
    pub text: String,
    /// Aegisub "extra" (e.g. from Aegisub's own metadata); kept opaque.
    pub extra: String,
}

impl Event {
    pub fn new(start: i64, end: i64, text: &str) -> Self {
        Event { style: "Default".into(), start, end, text: text.into(), ..Default::default() }
    }
    pub fn duration(&self) -> i64 { self.end - self.start }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Info { key: String, value: String },
    Style(Style),
    Event(Event),
    /// Anything else (comments, [Fonts], [Aegisub Project Garbage] ...), keeps section.
    Raw { section: String, raw: String },
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub entries: Vec<Entry>,
}

impl Script {
    pub fn new_default() -> Self {
        let mut s = Script::default();
        s.set_info("ScriptType", "v4.00+");
        s.set_info("PlayResX", "1920");
        s.set_info("PlayResY", "1080");
        s.set_info("WrapStyle", "0");
        s.set_info("ScaledBorderAndShadow", "yes");
        s.entries.push(Entry::Style(Style::default()));
        s
    }

    pub fn info(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            Entry::Info { key: k, value } if k.eq_ignore_ascii_case(key) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn set_info(&mut self, key: &str, val: &str) {
        for e in self.entries.iter_mut() {
            if let Entry::Info { key: k, value } = e {
                if k.eq_ignore_ascii_case(key) { *value = val.into(); return; }
            }
        }
        // insert after last info
        let pos = self.entries.iter().rposition(|e| matches!(e, Entry::Info { .. })).map(|p| p + 1).unwrap_or(0);
        self.entries.insert(pos, Entry::Info { key: key.into(), value: val.into() });
    }

    pub fn play_res(&self) -> (i32, i32) {
        let x = self.info("PlayResX").and_then(|v| v.trim().parse().ok());
        let y = self.info("PlayResY").and_then(|v| v.trim().parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => (x, x * 3 / 4),
            (None, Some(y)) => (y * 4 / 3, y),
            _ => (384, 288),
        }
    }

    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.entries.iter().filter_map(|e| if let Entry::Event(ev) = e { Some(ev) } else { None })
    }
    pub fn events_mut(&mut self) -> impl Iterator<Item = &mut Event> {
        self.entries.iter_mut().filter_map(|e| if let Entry::Event(ev) = e { Some(ev) } else { None })
    }
    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.entries.iter().filter_map(|e| if let Entry::Style(s) = e { Some(s) } else { None })
    }
    pub fn style(&self, name: &str) -> Option<&Style> { self.styles().find(|s| s.name == name) }

    /// Sort events by start time (stable), keeping non-event entries in place before them.
    pub fn sort_by_start(&mut self) {
        let first = self.entries.iter().position(|e| matches!(e, Entry::Event(_)));
        let Some(first) = first else { return };
        let mut evs: Vec<Entry> = Vec::new();
        let mut rest: Vec<Entry> = Vec::new();
        for (i, e) in std::mem::take(&mut self.entries).into_iter().enumerate() {
            if matches!(e, Entry::Event(_)) && i >= first { evs.push(e) } else { rest.push(e) }
        }
        evs.sort_by_key(|e| if let Entry::Event(ev) = e { (ev.start, ev.layer) } else { (0, 0) });
        let split = rest.iter().position(|e| !matches!(e, Entry::Info { .. } | Entry::Style(_)) ).unwrap_or(rest.len());
        let _ = split;
        rest.extend(evs);
        self.entries = rest;
    }
}
