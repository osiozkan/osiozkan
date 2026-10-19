//! ASS/SSA document model, parser and writer.
use crate::color::Color;
use crate::time::{format_ass, parse_time, Ms};

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub color1: Color, // primary
    pub color2: Color, // secondary (karaoke)
    pub color3: Color, // outline
    pub color4: Color, // shadow / back
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
    pub margin_b: i32,
    pub encoding: i32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            name: "Default".into(),
            fontname: "Arial".into(),
            fontsize: 48.0,
            color1: Color::WHITE,
            color2: Color { r: 255, g: 0, b: 0, a: 0 },
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
            margin_b: 10,
            encoding: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Dialogue,
    Comment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub layer: i32,
    pub start: Ms,
    pub end: Ms,
    pub style: String,
    pub actor: String,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_t: i32,
    pub margin_b: i32,
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
            margin_t: 0,
            margin_b: 0,
            effect: String::new(),
            text: String::new(),
        }
    }
}

impl Event {
    pub fn new(start: Ms, end: Ms, style: &str, text: &str) -> Event {
        Event { start, end, style: style.into(), text: text.into(), ..Default::default() }
    }
    pub fn duration(&self) -> Ms {
        self.end - self.start
    }
    pub fn is_comment(&self) -> bool {
        self.kind == EventKind::Comment
    }
}

/// A section the model doesn't interpret (Fonts, Graphics, Aegisub Project Garbage...).
#[derive(Debug, Clone, PartialEq)]
pub struct RawSection {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    /// (key, value). Comment lines are stored with key ";" and value = text after it.
    pub info: Vec<(String, String)>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    pub raw_sections: Vec<RawSection>,
    /// Section order as seen in the source (names without brackets).
    pub order: Vec<String>,
    pub is_ssa: bool,
}

impl Default for Script {
    fn default() -> Self {
        Script::new()
    }
}

const STYLES_V4P: &str = "V4+ Styles";
const STYLES_V4: &str = "V4 Styles";
const INFO: &str = "Script Info";
const EVENTS: &str = "Events";

impl Script {
    /// A fresh script with Aegisub-like defaults.
    pub fn new() -> Script {
        Script {
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
            raw_sections: vec![],
            order: vec![INFO.into(), STYLES_V4P.into(), EVENTS.into()],
            is_ssa: false,
        }
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

    pub fn raw_section(&self, name: &str) -> Option<&RawSection> {
        self.raw_sections.iter().find(|s| s.name == name)
    }

    /// Value from the `[Aegisub Project Garbage]` section (e.g. "Video File").
    pub fn garbage_get(&self, key: &str) -> Option<String> {
        let sec = self.raw_section("Aegisub Project Garbage")?;
        sec.lines.iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    }

    pub fn garbage_set(&mut self, key: &str, value: &str) {
        let name = "Aegisub Project Garbage";
        if self.raw_section(name).is_none() {
            self.raw_sections.push(RawSection { name: name.into(), lines: vec![] });
            let pos = self.order.iter().position(|n| n == INFO).map(|p| p + 1).unwrap_or(self.order.len());
            self.order.insert(pos.min(self.order.len()), name.into());
        }
        let sec = self.raw_sections.iter_mut().find(|s| s.name == name).unwrap();
        let line = format!("{key}: {value}");
        if let Some(l) = sec.lines.iter_mut().find(|l| l.split_once(':').map(|(k, _)| k.trim() == key).unwrap_or(false)) {
            *l = line;
        } else {
            sec.lines.push(line);
        }
    }

    // ---------------------------------------------------------------- parse
    pub fn parse(text: &str) -> Script {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let text = if text.contains('\r') { std::borrow::Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n")) } else { std::borrow::Cow::Borrowed(text) };
        let mut script = Script {
            info: vec![],
            styles: vec![],
            events: vec![],
            raw_sections: vec![],
            order: vec![],
            is_ssa: false,
        };
        #[derive(PartialEq)]
        enum Sec {
            None,
            Info,
            Styles,
            Events,
            Raw(usize),
        }
        let mut sec = Sec::None;
        let mut style_fmt: Option<Vec<String>> = None;
        let mut event_fmt: Option<Vec<String>> = None;
        let mut saw_styles = false;
        for line in text.split('\n') {
            let line = line.trim_start_matches('\u{feff}');
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
                let name = t[1..t.len() - 1].to_string();
                let lname = name.to_ascii_lowercase();
                script.order.push(name.clone());
                sec = match lname.as_str() {
                    "script info" => {
                        script.order.pop();
                        script.order.push(INFO.into());
                        Sec::Info
                    }
                    "v4+ styles" | "v4 styles" | "v4++ styles" => {
                        script.is_ssa = lname == "v4 styles";
                        saw_styles = true;
                        script.order.pop();
                        script.order.push(if script.is_ssa { STYLES_V4 } else { STYLES_V4P }.into());
                        Sec::Styles
                    }
                    "events" => {
                        script.order.pop();
                        script.order.push(EVENTS.into());
                        Sec::Events
                    }
                    _ => {
                        script.raw_sections.push(RawSection { name, lines: vec![] });
                        Sec::Raw(script.raw_sections.len() - 1)
                    }
                };
                continue;
            }
            match sec {
                Sec::None => {}
                Sec::Raw(i) => script.raw_sections[i].lines.push(line.to_string()),
                Sec::Info => {
                    if t.is_empty() {
                        continue;
                    }
                    if let Some(c) = t.strip_prefix(';') {
                        script.info.push((";".into(), c.to_string()));
                    } else if let Some((k, v)) = t.split_once(':') {
                        script.info.push((k.trim().to_string(), v.trim().to_string()));
                    } else {
                        // key-less junk line: kept verbatim (empty key)
                        script.info.push((String::new(), t.to_string()));
                    }
                }
                Sec::Styles => {
                    if t.is_empty() {
                        continue;
                    }
                    if let Some(f) = strip_prefix_ci(t, "Format:") {
                        style_fmt = Some(split_format(f));
                    } else if let Some(d) = strip_prefix_ci(t, "Style:") {
                        let fmt = style_fmt.clone().unwrap_or_else(|| if script.is_ssa { ssa_style_format() } else { default_style_format() });
                        script.styles.push(parse_style(d.trim_start(), &fmt, script.is_ssa));
                    }
                }
                Sec::Events => {
                    if t.is_empty() {
                        continue;
                    }
                    if let Some(f) = strip_prefix_ci(t, "Format:") {
                        event_fmt = Some(split_format(f));
                    } else if let Some((kind, d)) = event_prefix(line.trim_start()) {
                        let fmt = event_fmt.clone().unwrap_or_else(|| {
                            if d.len() >= 7 && d[..7].eq_ignore_ascii_case("marked=") {
                                ssa_event_format()
                            } else {
                                default_event_format()
                            }
                        });
                        if let Some(ev) = parse_event(kind, d, &fmt) {
                            script.events.push(ev);
                        }
                    }
                }
            }
        }
        for need in [INFO, EVENTS] {
            if !script.order.iter().any(|n| n == need) {
                script.order.push(need.into());
            }
        }
        if script.is_ssa {
            for e in &mut script.events {
                if let Some(r) = e.style.strip_prefix('*') {
                    e.style = r.to_string();
                }
            }
        }
        if !saw_styles {
            script.is_ssa = script.info_get("ScriptType").map(|v| v.trim().eq_ignore_ascii_case("v4.00")).unwrap_or(false);
            let pos = script.order.iter().position(|n| n == EVENTS).unwrap_or(script.order.len());
            script.order.insert(pos, if script.is_ssa { STYLES_V4 } else { STYLES_V4P }.into());
        }
        script
    }

    // ---------------------------------------------------------------- write
    pub fn to_ass(&self) -> String {
        let mut o = String::with_capacity(64 * 1024);
        let nl = "\r\n";
        let mut first = true;
        for name in &self.order {
            if !first {
                o.push_str(nl);
            }
            first = false;
            o.push('[');
            o.push_str(name);
            o.push(']');
            o.push_str(nl);
            match name.as_str() {
                INFO => {
                    for (k, v) in &self.info {
                        if k.is_empty() {
                            o.push_str(v);
                        } else if self.is_ssa && k.eq_ignore_ascii_case("ScriptType") && v.trim().eq_ignore_ascii_case("v4.00+") {
                            o.push_str("ScriptType: v4.00");
                        } else if k == ";" {
                            o.push(';');
                            o.push_str(v);
                        } else {
                            o.push_str(k);
                            o.push_str(": ");
                            o.push_str(v);
                        }
                        o.push_str(nl);
                    }
                }
                STYLES_V4P | STYLES_V4 if self.is_ssa => {
                    o.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding");
                    o.push_str(nl);
                    for s in &self.styles {
                        o.push_str(&ssa_style_line(s));
                        o.push_str(nl);
                    }
                }
                STYLES_V4P | STYLES_V4 => {
                    o.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding");
                    o.push_str(nl);
                    for s in &self.styles {
                        o.push_str(&style_line(s));
                        o.push_str(nl);
                    }
                }
                EVENTS if self.is_ssa => {
                    o.push_str("Format: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");
                    o.push_str(nl);
                    for e in &self.events {
                        let mut line = event_line(&Event { layer: 0, ..e.clone() });
                        let at = line.find(' ').unwrap_or(0) + 1;
                        line.insert_str(at, "Marked=");
                        o.push_str(&line);
                        o.push_str(nl);
                    }
                }
                EVENTS => {
                    o.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");
                    o.push_str(nl);
                    for e in &self.events {
                        o.push_str(&event_line(e));
                        o.push_str(nl);
                    }
                }
                other => {
                    if let Some(r) = self.raw_sections.iter().find(|r| r.name == other) {
                        let mut lines: Vec<&String> = r.lines.iter().collect();
                        while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                            lines.pop();
                        }
                        for l in lines {
                            o.push_str(l);
                            o.push_str(nl);
                        }
                    }
                }
            }
        }
        o
    }
}

fn strip_prefix_ci<'a>(s: &'a str, p: &str) -> Option<&'a str> {
    if s.len() >= p.len() && s.is_char_boundary(p.len()) && s[..p.len()].eq_ignore_ascii_case(p) {
        Some(&s[p.len()..])
    } else {
        None
    }
}

fn event_prefix(t: &str) -> Option<(EventKind, &str)> {
    if let Some(d) = strip_prefix_ci(t, "Dialogue:") {
        Some((EventKind::Dialogue, d.trim_start()))
    } else {
        strip_prefix_ci(t, "Comment:").map(|d| (EventKind::Comment, d.trim_start()))
    }
}

fn default_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,outlinecolour,backcolour,bold,italic,underline,strikeout,scalex,scaley,spacing,angle,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,encoding"
        .split(',')
        .map(String::from)
        .collect()
}

fn default_event_format() -> Vec<String> {
    "layer,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect()
}

fn ssa_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,tertiarycolour,backcolour,bold,italic,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,alphalevel,encoding"
        .split(',')
        .map(String::from)
        .collect()
}

fn ssa_event_format() -> Vec<String> {
    "marked,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect()
}

fn split_format(f: &str) -> Vec<String> {
    f.split(',').map(|s| s.trim().to_ascii_lowercase()).collect()
}

/// SSA alignment (1-3 bottom, 5-7 top, 9-11 middle) to ASS numpad.
fn ssa_align_to_ass(a: i32) -> i32 {
    let col = ((a - 1).rem_euclid(4)).min(2) + 1;
    if a & 4 != 0 {
        6 + col
    } else if a & 8 != 0 {
        3 + col
    } else {
        col
    }
}

fn ass_align_to_ssa(n: i32) -> i32 {
    let n = n.clamp(1, 9);
    let col = (n - 1) % 3 + 1;
    col + [0, 8, 4][((n - 1) / 3) as usize]
}

fn parse_bool(s: &str) -> bool {
    let s = s.trim();
    s != "0" && !s.is_empty()
}

fn num<T: std::str::FromStr + Default>(s: &str) -> T {
    s.trim().parse().unwrap_or_default()
}

fn parse_style(data: &str, fmt: &[String], ssa: bool) -> Style {
    let parts: Vec<&str> = data.splitn(fmt.len().max(1), ',').collect();
    let mut st = Style::default();
    st.margin_t = 0;
    st.margin_b = 0;
    let mut have_marginv = false;
    for (i, key) in fmt.iter().enumerate() {
        let v = match parts.get(i) {
            Some(v) => v.trim(),
            None => continue,
        };
        match key.as_str() {
            "name" => st.name = if ssa { v.strip_prefix('*').unwrap_or(v).to_string() } else { v.to_string() },
            "fontname" => st.fontname = v.to_string(),
            "fontsize" => st.fontsize = num(v),
            "primarycolour" => st.color1 = Color::parse(v).unwrap_or(st.color1),
            "secondarycolour" => st.color2 = Color::parse(v).unwrap_or(st.color2),
            "tertiarycolour" | "outlinecolour" => st.color3 = Color::parse(v).unwrap_or(st.color3),
            "backcolour" => st.color4 = Color::parse(v).unwrap_or(st.color4),
            "bold" => st.bold = parse_bool(v),
            "italic" => st.italic = parse_bool(v),
            "underline" => st.underline = parse_bool(v),
            "strikeout" => st.strikeout = parse_bool(v),
            "scalex" => st.scale_x = num(v),
            "scaley" => st.scale_y = num(v),
            "spacing" => st.spacing = num(v),
            "angle" => st.angle = num(v),
            "borderstyle" => st.borderstyle = num(v),
            "outline" => st.outline = num(v),
            "shadow" => st.shadow = num(v),
            "alignment" => st.align = if ssa { ssa_align_to_ass(num(v)) } else { num(v) },
            "marginl" => st.margin_l = num(v),
            "marginr" => st.margin_r = num(v),
            "marginv" => {
                let m: i32 = num(v);
                st.margin_t = m;
                st.margin_b = m;
                have_marginv = true;
            }
            "margint" => st.margin_t = num(v),
            "marginb" => st.margin_b = num(v),
            "encoding" => st.encoding = num(v),
            _ => {}
        }
    }
    let _ = have_marginv;
    if ssa && st.align == 0 {
        st.align = 2;
    }
    st
}

fn b(v: bool) -> &'static str {
    if v {
        "-1"
    } else {
        "0"
    }
}

/// Format floats the way Aegisub does (no trailing zeros).
pub fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn style_line(s: &Style) -> String {
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        s.name,
        s.fontname,
        fmt_num(s.fontsize),
        s.color1.to_style(),
        s.color2.to_style(),
        s.color3.to_style(),
        s.color4.to_style(),
        b(s.bold),
        b(s.italic),
        b(s.underline),
        b(s.strikeout),
        fmt_num(s.scale_x),
        fmt_num(s.scale_y),
        fmt_num(s.spacing),
        fmt_num(s.angle),
        s.borderstyle,
        fmt_num(s.outline),
        fmt_num(s.shadow),
        s.align,
        s.margin_l,
        s.margin_r,
        s.margin_t,
        s.encoding
    )
}

fn ssa_style_line(s: &Style) -> String {
    let dec = |c: &crate::color::Color| (c.r as u32 | (c.g as u32) << 8 | (c.b as u32) << 16).to_string();
    format!(
        "Style: {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},0,{}",
        s.name,
        s.fontname,
        fmt_num(s.fontsize),
        dec(&s.color1),
        dec(&s.color2),
        dec(&s.color3),
        dec(&s.color4),
        b(s.bold),
        b(s.italic),
        s.borderstyle,
        fmt_num(s.outline),
        fmt_num(s.shadow),
        ass_align_to_ssa(s.align),
        s.margin_l,
        s.margin_r,
        s.margin_b,
        s.encoding
    )
}

fn parse_event(kind: EventKind, data: &str, fmt: &[String]) -> Option<Event> {
    let parts: Vec<&str> = data.splitn(fmt.len().max(1), ',').collect();
    let mut ev = Event { kind, ..Default::default() };
    ev.style = String::new();
    for (i, key) in fmt.iter().enumerate() {
        let v = match parts.get(i) {
            Some(v) => *v,
            None => continue,
        };
        match key.as_str() {
            "layer" | "marked" => {
                let v = v.trim();
                ev.layer = strip_prefix_ci(v, "Marked=").unwrap_or(v).trim().parse().unwrap_or(0)
            }
            "start" => ev.start = parse_time(v).unwrap_or(0),
            "end" => ev.end = parse_time(v).unwrap_or(0),
            "style" => ev.style = v.trim().to_string(),
            "name" | "actor" => ev.actor = v.trim().to_string(),
            "marginl" => ev.margin_l = num(v),
            "marginr" => ev.margin_r = num(v),
            "marginv" => {
                let m: i32 = num(v);
                ev.margin_t = m;
                ev.margin_b = m;
            }
            "margint" => ev.margin_t = num(v),
            "marginb" => ev.margin_b = num(v),
            "effect" => ev.effect = v.trim().to_string(),
            "text" => ev.text = v.to_string(),
            _ => {}
        }
    }
    if ev.style.is_empty() {
        ev.style = "Default".into();
    }
    Some(ev)
}

fn event_line(e: &Event) -> String {
    format!(
        "{}: {},{},{},{},{},{},{},{},{},{}",
        if e.kind == EventKind::Comment { "Comment" } else { "Dialogue" },
        e.layer,
        format_ass(e.start),
        format_ass(e.end),
        e.style,
        e.actor,
        e.margin_l,
        e.margin_r,
        e.margin_t,
        e.effect,
        e.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[Script Info]\r\n; comment\r\nTitle: Test\r\nPlayResX: 1280\r\nPlayResY: 720\r\n\r\n[Aegisub Project Garbage]\r\nVideo File: a.mkv\r\n\r\n[V4+ Styles]\r\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\r\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,25,162\r\n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Merhaba, {\\i1}dünya{\\i0}\\Nİkinci satır\r\nComment: 0,0:00:04.00,0:00:05.00,Default,x,0,0,0,fx,note\r\n";

    #[test]
    fn parse_and_roundtrip() {
        let s = Script::parse(SAMPLE);
        assert_eq!(s.info_get("Title"), Some("Test"));
        assert_eq!(s.play_res(), (1280, 720));
        assert_eq!(s.styles.len(), 1);
        assert!(s.styles[0].bold);
        assert_eq!(s.styles[0].margin_b, 25);
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].start, 1000);
        assert_eq!(s.events[0].end, 3500);
        assert_eq!(s.events[0].text, "Merhaba, {\\i1}dünya{\\i0}\\Nİkinci satır");
        assert!(s.events[1].is_comment());
        assert_eq!(s.garbage_get("Video File").as_deref(), Some("a.mkv"));
        let out = s.to_ass();
        let s2 = Script::parse(&out);
        assert_eq!(s.events, s2.events);
        assert_eq!(s.styles, s2.styles);
        assert_eq!(s.info, s2.info);
        assert_eq!(s2.raw_sections, s.raw_sections);
        // stable under a second roundtrip
        assert_eq!(out, s2.to_ass());
    }

    #[test]
    fn text_with_commas_and_missing_fields() {
        let s = Script::parse("[Events]\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,a,b,c\n");
        assert_eq!(s.events[0].text, "a,b,c");
    }

    const SSA: &str = "[Script Info]\r\nScriptType: v4.00\r\nTitle: Eski\r\n\r\n[V4 Styles]\r\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\r\nStyle: *Default,Arial,20,16777215,65535,0,0,-1,0,1,2,1,6,10,10,15,0,162\r\nStyle: Alt,Tahoma,18,255,255,0,0,0,0,1,2,1,9,10,10,15,0,0\r\n\r\n[Events]\r\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nDialogue: Marked=0,0:00:01.00,0:00:02.00,*Default,,0,0,0,,Şimdi, İyi mi?\r\nDialogue: Marked=0,0:00:03.00,0:00:04.00,Alt,Kim,0,0,0,,{\\an8}ğ\r\n";

    #[test]
    fn ssa_v4_read_write() {
        let s = Script::parse(SSA);
        assert!(s.is_ssa);
        assert_eq!(s.styles[0].name, "Default");
        assert_eq!(s.styles[0].align, 8); // SSA 6 = top center
        assert_eq!(s.styles[1].align, 4); // SSA 9 = middle left
        assert_eq!(s.styles[0].color1, Color::WHITE);
        assert_eq!(s.styles[0].color2, Color::rgb(255, 255, 0));
        assert_eq!(s.styles[1].color1, Color::rgb(255, 0, 0));
        assert_eq!(s.events[0].style, "Default");
        assert_eq!(s.events[0].text, "Şimdi, İyi mi?");
        let out = s.to_ass();
        assert!(out.contains("[V4 Styles]"));
        assert!(out.contains("Format: Marked, Start"));
        assert!(out.contains("Dialogue: Marked=0,0:00:01.00"));
        assert!(out.contains("Style: Default,Arial,20,16777215,65535,0,0,-1,0,1,2,1,6,10,10,15,0,162"));
        let s2 = Script::parse(&out);
        assert!(s2.is_ssa);
        assert_eq!(s.styles, s2.styles);
        assert_eq!(s.events, s2.events);
        assert_eq!(out, s2.to_ass());
    }

    #[test]
    fn ssa_without_format_lines() {
        let s = Script::parse("[V4 Styles]\nStyle: Default,Arial,20,16777215,65535,0,0,0,0,1,2,1,2,10,10,15,0,1\n[Events]\nDialogue: Marked=0,0:00:01.00,0:00:02.00,Default,,0,0,0,,x\n");
        assert_eq!(s.styles[0].fontsize, 20.0);
        assert_eq!(s.events[0].end, 2000);
        // ASS with no formats at all
        let s = Script::parse("[Events]\nDialogue: 1,0:00:01.00,0:00:02.00,Foo,Bar,1,2,3,fx,t\n");
        assert_eq!((s.events[0].layer, s.events[0].margin_l, s.events[0].margin_t, s.events[0].effect.as_str()), (1, 1, 3, "fx"));
        assert!(!s.is_ssa);
        // ScriptType alone marks SSA
        assert!(Script::parse("[Script Info]\nScriptType: v4.00\n[Events]\n").is_ssa);
    }

    #[test]
    fn reordered_and_short_lines() {
        let src = "[V4+ Styles]\nFormat: Fontname, Name, Alignment, PrimaryColour, Fontsize\nStyle: Verdana,Foo,8,&H000000FF,33\n[Events]\nFormat: Text, Start, End, Style\nDialogue: Hello, you,0:00:01.00,0:00:02.00,Foo\nDialogue: 0,0:00:03.00\n";
        let s = Script::parse(src);
        assert_eq!((s.styles[0].name.as_str(), s.styles[0].fontname.as_str(), s.styles[0].align, s.styles[0].fontsize), ("Foo", "Verdana", 8, 33.0));
        assert_eq!(s.styles[0].color1, Color::rgb(255, 0, 0));
        // text is first so splitn(4) leaves "Hello" as text; missing fields default
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "Hello");
    }

    #[test]
    fn messy_lines_preserved() {
        let src = "\u{feff}[Script Info]  \r\nTitle: A  \r\nstray junk line\r\n; note \n\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,trailing space  \r\nDialogue: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,two\rComment: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\i1}c\n";
        let s = Script::parse(src);
        assert_eq!(s.events.len(), 3);
        assert_eq!(s.events[0].text, "trailing space  ");
        assert!(s.events[2].is_comment());
        assert!(s.info.contains(&(String::new(), "stray junk line".to_string())));
        let out = s.to_ass();
        assert!(out.contains("stray junk line\r\n"));
        assert_eq!(Script::parse(&out).events, s.events);
    }

    #[test]
    fn duplicate_styles_and_events_before_styles() {
        let src = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,A,,0,0,0,,x\n[V4+ Styles]\nStyle: A,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\nStyle: A,Arial,30,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n";
        let s = Script::parse(src);
        assert_eq!(s.styles.len(), 2);
        assert_eq!(s.style("A").unwrap().fontsize, 20.0);
        let out = s.to_ass();
        assert!(out.find("[Events]").unwrap() < out.find("[V4+ Styles]").unwrap());
        assert_eq!(Script::parse(&out).styles, s.styles);
    }

    /// Deterministic fuzz: odd generated documents must survive parse -> write -> parse -> write unchanged.
    #[test]
    fn fuzz_roundtrip() {
        let mut seed = 99u64;
        let mut rnd = move |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        let texts = ["", " ", "a,b,,c", "{\\an8\\pos(1,2)}x", "ş ğ ı İ ö ü ç", "{", "}", "\\N\\N", "  lead", "trail  ", "Dialogue: x", "[Events]x", ";c", "a:b"];
        for _ in 0..200 {
            let mut doc = String::new();
            if rnd(2) == 0 {
                doc.push('\u{feff}');
            }
            let nl = ["\n", "\r\n"][rnd(2) as usize];
            doc.push_str(&format!("[Script Info]{nl}Title: T{}{nl}", rnd(9)));
            if rnd(3) == 0 {
                doc.push_str(&format!("junk{nl}"));
            }
            let ssa = rnd(3) == 0;
            if ssa {
                doc.push_str(&format!("ScriptType: v4.00{nl}[V4 Styles]{nl}Style: S{},Arial,{},{},0,0,0,0,0,1,1,1,{},1,2,3,0,0{nl}", rnd(3), 10 + rnd(50), rnd(16777215), 1 + rnd(11) / 4 * 4 + rnd(3)));
            } else {
                doc.push_str(&format!("[V4+ Styles]{nl}Style: S{},Arial,{},&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,-1,0,0,100,100,0,0,1,2,2,{},1,2,3,1{nl}", rnd(3), 10 + rnd(50), 1 + rnd(9)));
            }
            doc.push_str(&format!("[Events]{nl}"));
            for _ in 0..rnd(6) {
                let k = ["Dialogue", "Comment"][rnd(2) as usize];
                let t = texts[rnd(texts.len() as u64) as usize];
                let mk = if ssa { "Marked=0".to_string() } else { rnd(3).to_string() };
                doc.push_str(&format!("{k}: {mk},0:00:{:02}.{:02},0:01:{:02}.{:02},S{},,{},{},{},fx,{t}{nl}", rnd(60), rnd(100), rnd(60), rnd(100), rnd(3), rnd(5), rnd(5), rnd(5)));
            }
            if rnd(2) == 0 {
                doc.push_str(&format!("[Aegisub Extradata]{nl}Data: 0,k,e1#2C2{nl}"));
            }
            let a = Script::parse(&doc);
            let o1 = a.to_ass();
            let b = Script::parse(&o1);
            assert_eq!(a.events, b.events, "{doc:?}");
            assert_eq!(a.styles, b.styles, "{doc:?}");
            assert_eq!(a.info, b.info, "{doc:?}");
            assert_eq!(a.raw_sections, b.raw_sections, "{doc:?}");
            assert_eq!(o1, b.to_ass(), "{doc:?}");
        }
    }
}
