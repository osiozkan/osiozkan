//! ASS/SSA document model, parser and writer.
//!
//! The model mirrors what Aegisub's automation API exposes: `info` lines, `styles` and
//! `events`, in that order. Unknown sections (`[Fonts]`, `[Graphics]`, `[Aegisub Project Garbage]`,
//! `[Aegisub Extradata]`) are preserved verbatim.

use crate::color::Color;
use crate::time;

#[derive(Clone, Debug, PartialEq)]
pub struct InfoLine {
    /// For comment lines (`; ...`) the key is `";"` and `value` holds the full raw line.
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub fontname: String,
    pub fontsize: f64,
    pub color1: Color,
    pub color2: Color,
    pub color3: Color,
    pub color4: Color,
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
            color2: Color::new(255, 0, 0, 0),
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

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn b2i(b: bool) -> &'static str {
    if b {
        "-1"
    } else {
        "0"
    }
}

impl Style {
    pub fn to_line(&self) -> String {
        let f: Vec<String> = vec![
            self.name.clone(),
            self.fontname.clone(),
            fmt_num(self.fontsize),
            self.color1.to_style_string(),
            self.color2.to_style_string(),
            self.color3.to_style_string(),
            self.color4.to_style_string(),
            b2i(self.bold).into(),
            b2i(self.italic).into(),
            b2i(self.underline).into(),
            b2i(self.strikeout).into(),
            fmt_num(self.scale_x),
            fmt_num(self.scale_y),
            fmt_num(self.spacing),
            fmt_num(self.angle),
            self.borderstyle.to_string(),
            fmt_num(self.outline),
            fmt_num(self.shadow),
            self.align.to_string(),
            self.margin_l.to_string(),
            self.margin_r.to_string(),
            self.margin_t.to_string(),
            self.encoding.to_string(),
        ];
        format!("Style: {}", f.join(","))
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

    pub fn to_line(&self) -> String {
        format!(
            "{}: {},{},{},{},{},{:04},{:04},{:04},{},{}",
            if self.comment { "Comment" } else { "Dialogue" },
            self.layer,
            time::format_ass(self.start),
            time::format_ass(self.end),
            self.style,
            self.actor,
            self.margin_l,
            self.margin_r,
            self.margin_t,
            self.effect,
            self.text
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtraSection {
    pub name: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub info: Vec<InfoLine>,
    pub styles: Vec<Style>,
    pub events: Vec<Event>,
    pub extra: Vec<ExtraSection>,
    /// Non-fatal problems found while parsing.
    pub warnings: Vec<String>,
}

impl Script {
    /// Empty script with sensible defaults, like File → New in Aegisub.
    pub fn new() -> Script {
        let mut s = Script::default();
        s.set_info("ScriptType", "v4.00+");
        s.set_info("WrapStyle", "0");
        s.set_info("ScaledBorderAndShadow", "yes");
        s.set_info("YCbCr Matrix", "None");
        s.set_info("PlayResX", "1920");
        s.set_info("PlayResY", "1080");
        s.styles.push(Style::default());
        s
    }

    pub fn get_info(&self, key: &str) -> Option<&str> {
        self.info.iter().find(|i| i.key.eq_ignore_ascii_case(key)).map(|i| i.value.as_str())
    }

    pub fn set_info(&mut self, key: &str, value: &str) {
        if let Some(i) = self.info.iter_mut().find(|i| i.key.eq_ignore_ascii_case(key)) {
            i.value = value.to_string();
        } else {
            self.info.push(InfoLine { key: key.to_string(), value: value.to_string() });
        }
    }

    /// Value from the `[Aegisub Project Garbage]` section (video/audio paths, active line, ...).
    pub fn project_property(&self, key: &str) -> Option<&str> {
        let sec = self.extra.iter().find(|x| x.name.eq_ignore_ascii_case("Aegisub Project Garbage"))?;
        sec.lines.iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(key).then(|| v.trim())
        })
    }

    pub fn set_project_property(&mut self, key: &str, value: &str) {
        let name = "Aegisub Project Garbage";
        let idx = match self.extra.iter().position(|x| x.name.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                self.extra.push(ExtraSection { name: name.into(), lines: vec![] });
                self.extra.len() - 1
            }
        };
        let line = format!("{key}: {value}");
        let lines = &mut self.extra[idx].lines;
        match lines.iter_mut().find(|l| l.split_once(':').map_or(false, |(k, _)| k.trim().eq_ignore_ascii_case(key))) {
            Some(l) => *l = line,
            None => lines.push(line),
        }
    }

    pub fn resolution(&self) -> (i32, i32) {
        let x = self.get_info("PlayResX").and_then(|v| v.trim().parse().ok());
        let y = self.get_info("PlayResY").and_then(|v| v.trim().parse().ok());
        match (x, y) {
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => (x, if x == 1280 { 720 } else { x * 3 / 4 }),
            (None, Some(y)) => (if y == 1024 { 1280 } else { y * 4 / 3 }, y),
            (None, None) => (384, 288),
        }
    }

    pub fn style(&self, name: &str) -> Option<&Style> {
        self.styles.iter().find(|s| s.name == name)
    }

    pub fn style_or_default(&self, name: &str) -> Style {
        self.style(name)
            .or_else(|| self.style("Default"))
            .or_else(|| self.styles.first())
            .cloned()
            .unwrap_or_default()
    }

    pub fn sort_by_start(&mut self) {
        self.events.sort_by_key(|e| e.start); // stable
    }

    // ------------------------------------------------------------------ parsing

    pub fn parse(input: &str) -> Script {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut script = Script::default();
        #[derive(PartialEq)]
        enum Sec {
            None,
            Info,
            Styles,
            Events,
            Extra(usize),
        }
        let mut sec = Sec::None;
        let mut style_fmt: Vec<String> = default_style_format();
        let mut event_fmt: Vec<String> = default_event_format();
        let mut is_ssa = false;

        for (ln, raw) in input.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let name = &trimmed[1..trimmed.len() - 1];
                let lname = name.to_ascii_lowercase();
                sec = match lname.as_str() {
                    "script info" => Sec::Info,
                    "v4+ styles" => Sec::Styles,
                    "v4 styles" => {
                        is_ssa = true;
                        style_fmt = default_ssa_style_format();
                        Sec::Styles
                    }
                    "events" => Sec::Events,
                    _ => {
                        script.extra.push(ExtraSection { name: name.to_string(), lines: vec![] });
                        Sec::Extra(script.extra.len() - 1)
                    }
                };
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            match sec {
                Sec::None => {}
                Sec::Extra(i) => script.extra[i].lines.push(line.to_string()),
                Sec::Info => {
                    if trimmed.starts_with(';') {
                        script.info.push(InfoLine { key: ";".into(), value: line.to_string() });
                    } else if let Some((k, v)) = line.split_once(':') {
                        let k = k.trim().to_string();
                        let v = v.trim().to_string();
                        if k.eq_ignore_ascii_case("ScriptType") && v.eq_ignore_ascii_case("v4.00") {
                            is_ssa = true;
                        }
                        script.info.push(InfoLine { key: k, value: v });
                    }
                }
                Sec::Styles => {
                    if let Some(rest) = strip_key(line, "Format") {
                        style_fmt = rest.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                    } else if let Some(rest) = strip_key(line, "Style") {
                        match parse_style(rest, &style_fmt, is_ssa) {
                            Some(s) => script.styles.push(s),
                            None => script.warnings.push(format!("line {}: bad style line", ln + 1)),
                        }
                    }
                }
                Sec::Events => {
                    if let Some(rest) = strip_key(line, "Format") {
                        event_fmt = rest.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
                    } else if let Some((comment, rest)) = strip_key(line, "Dialogue")
                        .map(|r| (false, r))
                        .or_else(|| strip_key(line, "Comment").map(|r| (true, r)))
                    {
                        match parse_event(rest, &event_fmt, comment) {
                            Some(e) => script.events.push(e),
                            None => script.warnings.push(format!("line {}: bad event line", ln + 1)),
                        }
                    }
                    // Picture/Sound/Movie/Command lines of SSA are ignored.
                }
            }
        }
        if is_ssa {
            script.set_info("ScriptType", "v4.00+");
            script.warnings.push("converted from SSA v4.00 to ASS v4.00+".into());
        }
        if script.styles.is_empty() {
            script.styles.push(Style::default());
        }
        script
    }

    // ------------------------------------------------------------------ writing

    pub fn to_ass_string(&self) -> String {
        let mut out = String::with_capacity(64 * 1024);
        out.push_str("[Script Info]\n");
        let mut has_type = false;
        for i in &self.info {
            if i.key == ";" {
                out.push_str(&i.value);
                out.push('\n');
                continue;
            }
            if i.key.eq_ignore_ascii_case("ScriptType") {
                has_type = true;
            }
            out.push_str(&format!("{}: {}\n", i.key, i.value));
        }
        if !has_type {
            out.push_str("ScriptType: v4.00+\n");
        }
        out.push_str("\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
        for s in &self.styles {
            out.push_str(&s.to_line());
            out.push('\n');
        }
        for x in &self.extra {
            out.push_str(&format!("\n[{}]\n", x.name));
            for l in &x.lines {
                out.push_str(l);
                out.push('\n');
            }
        }
        out.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        for e in &self.events {
            out.push_str(&e.to_line());
            out.push('\n');
        }
        out
    }
}

fn strip_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let l = line.trim_start();
    if l.len() > key.len() && l[..key.len()].eq_ignore_ascii_case(key) && l[key.len()..].starts_with(':') {
        Some(l[key.len() + 1..].trim_start())
    } else {
        None
    }
}

fn default_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,outlinecolour,backcolour,bold,italic,underline,strikeout,scalex,scaley,spacing,angle,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,encoding"
        .split(',')
        .map(String::from)
        .collect()
}
fn default_ssa_style_format() -> Vec<String> {
    "name,fontname,fontsize,primarycolour,secondarycolour,tertiarycolour,backcolour,bold,italic,borderstyle,outline,shadow,alignment,marginl,marginr,marginv,alphalevel,encoding"
        .split(',')
        .map(String::from)
        .collect()
}
fn default_event_format() -> Vec<String> {
    "layer,start,end,style,name,marginl,marginr,marginv,effect,text".split(',').map(String::from).collect()
}

fn pf(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}
fn pi(s: &str) -> i32 {
    let t = s.trim();
    t.parse::<i32>().unwrap_or_else(|_| t.parse::<f64>().map(|f| f as i32).unwrap_or(0))
}
fn pb(s: &str) -> bool {
    pi(s) != 0
}

fn parse_style(rest: &str, fmt: &[String], ssa: bool) -> Option<Style> {
    let parts: Vec<&str> = rest.splitn(fmt.len(), ',').collect();
    if parts.len() < fmt.len().min(3) {
        return None;
    }
    let mut s = Style::default();
    let mut angle_set = false;
    for (k, v) in fmt.iter().zip(parts.iter()) {
        match k.as_str() {
            "name" => s.name = v.trim().to_string(),
            "fontname" => s.fontname = v.trim().to_string(),
            "fontsize" => s.fontsize = pf(v),
            "primarycolour" => s.color1 = Color::parse(v)?,
            "secondarycolour" => s.color2 = Color::parse(v)?,
            "outlinecolour" | "tertiarycolour" => s.color3 = Color::parse(v)?,
            "backcolour" => s.color4 = Color::parse(v)?,
            "bold" => s.bold = pb(v),
            "italic" => s.italic = pb(v),
            "underline" => s.underline = pb(v),
            "strikeout" => s.strikeout = pb(v),
            "scalex" => s.scale_x = pf(v),
            "scaley" => s.scale_y = pf(v),
            "spacing" => s.spacing = pf(v),
            "angle" => {
                s.angle = pf(v);
                angle_set = true
            }
            "borderstyle" => s.borderstyle = pi(v),
            "outline" => s.outline = pf(v),
            "shadow" => s.shadow = pf(v),
            "alignment" => {
                let a = pi(v);
                s.align = if ssa {
                    // SSA: 1-3 bottom, 5-7 top (+4), 9-11 middle (+8)
                    match a {
                        1..=3 => a,
                        5..=7 => a - 4 + 6,
                        9..=11 => a - 8 + 3,
                        _ => 2,
                    }
                } else {
                    a
                }
            }
            "marginl" => s.margin_l = pi(v),
            "marginr" => s.margin_r = pi(v),
            "marginv" => {
                s.margin_t = pi(v);
                s.margin_b = pi(v)
            }
            "encoding" => s.encoding = pi(v),
            _ => {}
        }
    }
    let _ = angle_set;
    Some(s)
}

fn parse_event(rest: &str, fmt: &[String], comment: bool) -> Option<Event> {
    // `text` is always last and may contain commas.
    let parts: Vec<&str> = rest.splitn(fmt.len(), ',').collect();
    if parts.len() < fmt.len() {
        return None;
    }
    let mut e = Event { comment, ..Event::default() };
    for (k, v) in fmt.iter().zip(parts.iter()) {
        match k.as_str() {
            "layer" | "marked" => e.layer = pi(&v.replace("Marked=", "")),
            "start" => e.start = time::parse_ass(v)?,
            "end" => e.end = time::parse_ass(v)?,
            "style" => e.style = v.trim_start_matches('*').trim().to_string(),
            "name" | "actor" => e.actor = v.trim().to_string(),
            "marginl" => e.margin_l = pi(v),
            "marginr" => e.margin_r = pi(v),
            "marginv" | "margint" => e.margin_t = pi(v),
            "effect" => e.effect = v.trim().to_string(),
            "text" => e.text = v.to_string(),
            _ => {}
        }
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}[Script Info]\n; hello\nTitle: Test\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,20,1\n\n[Aegisub Project Garbage]\nAudio File: a.mka\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,Bob,0,0,0,,Hello, {\\i1}world{\\i0}\nComment: 1,0:00:04.00,0:00:05.00,Default,,0,0,0,fx,note\n";

    #[test]
    fn parse_and_write_roundtrip() {
        let s = Script::parse(SAMPLE);
        assert_eq!(s.resolution(), (1280, 720));
        assert_eq!(s.styles.len(), 1);
        assert!(s.styles[0].bold);
        assert_eq!(s.styles[0].margin_b, 20);
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "Hello, {\\i1}world{\\i0}");
        assert_eq!(s.events[0].start, 1000);
        assert_eq!(s.events[0].end, 3500);
        assert!(s.events[1].comment);
        assert_eq!(s.extra[0].name, "Aegisub Project Garbage");
        let out = s.to_ass_string();
        let s2 = Script::parse(&out);
        assert_eq!(s.events, s2.events);
        assert_eq!(s.styles, s2.styles);
        assert_eq!(s.extra, s2.extra);
        assert!(out.contains("; hello"));
    }

    #[test]
    fn project_properties() {
        let mut s = Script::parse(SAMPLE);
        assert_eq!(s.project_property("Audio File"), Some("a.mka"));
        s.set_project_property("Video File", "v.mkv");
        s.set_project_property("Audio File", "b.mka");
        let r = Script::parse(&s.to_ass_string());
        assert_eq!(r.project_property("video file"), Some("v.mkv"));
        assert_eq!(r.project_property("Audio File"), Some("b.mka"));
    }

    #[test]
    fn ssa_alignment_conversion() {
        let src = "[Script Info]\nScriptType: v4.00\n\n[V4 Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\nStyle: Top,Arial,20,16777215,255,0,0,0,0,1,2,2,6,10,10,10,0,0\n\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: Marked=0,0:00:01.00,0:00:02.00,Top,,0,0,0,,x\n";
        let s = Script::parse(src);
        assert_eq!(s.styles[0].align, 8); // SSA 6 = top-center -> ASS 8
        assert_eq!(s.get_info("ScriptType"), Some("v4.00+"));
        assert_eq!(s.events[0].text, "x");
    }
}
