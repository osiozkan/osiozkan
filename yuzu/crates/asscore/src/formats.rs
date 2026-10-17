//! Additional subtitle formats (the Subtitle Edit "format zoo"): MicroDVD, SubViewer 2, SBV, LRC,
//! plain text, CSV, TTML/DFXP, SAMI and JSON, plus format detection and dispatch helpers.
//!
//! Every format has `parse_xxx(&str) -> Script` and `write_xxx(&Script) -> String` (some take an
//! extra argument such as the frame rate). Inside a [`Script`] the text is always ASS text:
//! `\N` is a line break and `{\i1}`-style override blocks carry formatting. SRT/VTT/ASS live in
//! [`crate::subrip`] / [`crate::ass`] and are only dispatched to from here.

#![allow(clippy::regex_creation_in_loops)]

use crate::model::*;
use crate::tags::{self, Part};
use crate::time;
use regex::Regex;

// ------------------------------------------------------------------------------------------
// Format enum / options / dispatch
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Ass,
    Srt,
    Vtt,
    MicroDvd,
    SubViewer2,
    Sbv,
    Lrc,
    Text,
    Csv,
    Ttml,
    Sami,
    Json,
}

impl Format {
    pub const ALL: [Format; 12] = [
        Format::Ass, Format::Srt, Format::Vtt, Format::MicroDvd, Format::SubViewer2, Format::Sbv,
        Format::Lrc, Format::Text, Format::Csv, Format::Ttml, Format::Sami, Format::Json,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Format::Ass => "Advanced SubStation Alpha",
            Format::Srt => "SubRip",
            Format::Vtt => "WebVTT",
            Format::MicroDvd => "MicroDVD",
            Format::SubViewer2 => "SubViewer 2.0",
            Format::Sbv => "YouTube SBV",
            Format::Lrc => "LRC",
            Format::Text => "Plain text",
            Format::Csv => "CSV",
            Format::Ttml => "TTML/DFXP",
            Format::Sami => "SAMI",
            Format::Json => "JSON",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Format::Ass => "ass",
            Format::Srt => "srt",
            Format::Vtt => "vtt",
            Format::MicroDvd => "sub",
            Format::SubViewer2 => "sub",
            Format::Sbv => "sbv",
            Format::Lrc => "lrc",
            Format::Text => "txt",
            Format::Csv => "csv",
            Format::Ttml => "ttml",
            Format::Sami => "smi",
            Format::Json => "json",
        }
    }
}

/// Options for the format dispatch helpers.
#[derive(Clone, Debug)]
pub struct FormatOptions {
    /// Frame rate for MicroDVD.
    pub fps: f64,
    pub csv: CsvOptions,
    /// Write word-level `<mm:ss.xx>` stamps in LRC when the cue has `\k` tags.
    pub lrc_word_level: bool,
    pub text: TextImport,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions { fps: 23.976, csv: CsvOptions::default(), lrc_word_level: false, text: TextImport::default() }
    }
}

/// Detect the format from a file name (extension) and, where ambiguous, the content.
pub fn detect(name: &str, content: &str) -> Option<Format> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|x| x.1).unwrap_or("");
    let c = content.trim_start_matches('\u{feff}').trim_start();
    let by_content = sniff(c);
    match ext {
        "ass" | "ssa" => Some(Format::Ass),
        "srt" => Some(Format::Srt),
        "vtt" => Some(Format::Vtt),
        "sbv" => Some(Format::Sbv),
        "lrc" => Some(Format::Lrc),
        "csv" | "tsv" => Some(Format::Csv),
        "smi" | "sami" => Some(Format::Sami),
        "ttml" | "dfxp" | "tt" => Some(Format::Ttml),
        "json" => Some(Format::Json),
        "sub" => match by_content {
            Some(Format::SubViewer2) => Some(Format::SubViewer2),
            _ => Some(Format::MicroDvd),
        },
        "xml" => match by_content {
            Some(Format::Ttml) => Some(Format::Ttml),
            _ => None,
        },
        "txt" => by_content.or(Some(Format::Text)),
        _ => by_content,
    }
}

fn sniff(c: &str) -> Option<Format> {
    let head: String = c.chars().take(4000).collect();
    let lower = head.to_ascii_lowercase();
    if lower.starts_with("webvtt") {
        return Some(Format::Vtt);
    }
    if lower.contains("[script info]") || lower.contains("[events]") {
        return Some(Format::Ass);
    }
    if lower.contains("<sami") {
        return Some(Format::Sami);
    }
    if lower.contains("<tt ") || lower.contains("<tt>") || lower.contains(":tt ") {
        return Some(Format::Ttml);
    }
    if lower.contains("[information]") || lower.contains("[subtitle]") {
        return Some(Format::SubViewer2);
    }
    let first = head.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    if Regex::new(r"^\{\d+\}\{\d*\}").unwrap().is_match(first) {
        return Some(Format::MicroDvd);
    }
    if Regex::new(r"^\d+:\d\d:\d\d\.\d+,\d+:\d\d:\d\d\.\d+\s*$").unwrap().is_match(first) {
        return Some(Format::Sbv);
    }
    if head.contains("-->") {
        return Some(Format::Srt);
    }
    if Regex::new(r"(?m)^\[\d+:\d+(?:[.:]\d+)?\]").unwrap().is_match(&head) {
        return Some(Format::Lrc);
    }
    let t = c.trim_start();
    if t.starts_with('[') || t.starts_with('{') {
        let end = t.trim_end();
        if ((t.starts_with('[') && end.ends_with(']')) || (t.starts_with('{') && end.ends_with('}')))
            && parse_json_value(t).is_some()
        {
            return Some(Format::Json);
        }
    }
    None
}

/// Detect and parse. `None` when the format cannot be determined.
pub fn load_any(name: &str, content: &str, opts: &FormatOptions) -> Option<Script> {
    let f = detect(name, content)?;
    Some(parse_as(f, content, opts))
}

pub fn parse_as(f: Format, content: &str, opts: &FormatOptions) -> Script {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    match f {
        Format::Ass => crate::ass::parse(content),
        Format::Srt => crate::subrip::parse_srt(content),
        Format::Vtt => crate::subrip::parse_vtt(content),
        Format::MicroDvd => parse_microdvd(content, opts.fps),
        Format::SubViewer2 => parse_subviewer2(content),
        Format::Sbv => parse_sbv(content),
        Format::Lrc => parse_lrc(content),
        Format::Text => parse_text_opts(content, &opts.text),
        Format::Csv => parse_csv(content, &opts.csv),
        Format::Ttml => parse_ttml(content),
        Format::Sami => parse_sami(content),
        Format::Json => parse_json(content),
    }
}

/// Serialise in the chosen format.
pub fn save_any(f: Format, sc: &Script, opts: &FormatOptions) -> String {
    match f {
        Format::Ass => crate::ass::write(sc),
        Format::Srt => crate::subrip::write_srt(sc),
        Format::Vtt => crate::subrip::write_vtt(sc),
        Format::MicroDvd => write_microdvd(sc, opts.fps),
        Format::SubViewer2 => write_subviewer2(sc),
        Format::Sbv => write_sbv(sc),
        Format::Lrc => write_lrc(sc, opts.lrc_word_level),
        Format::Text => write_text(sc),
        Format::Csv => write_csv(sc, &opts.csv),
        Format::Ttml => write_ttml(sc),
        Format::Sami => write_sami(sc),
        Format::Json => write_json(sc),
    }
}

// ------------------------------------------------------------------------------------------
// Shared markup helpers
// ------------------------------------------------------------------------------------------

#[derive(Clone, Default, PartialEq, Debug)]
struct St {
    i: bool,
    b: bool,
    u: bool,
    s: bool,
    color: Option<(u8, u8, u8)>,
}

/// Split ASS text into runs of constant formatting. `\N`/`\n` become `'\n'`, `\h` a space.
fn segments(text: &str) -> Vec<(St, String)> {
    let mut st = St::default();
    let mut out: Vec<(St, String)> = Vec::new();
    for p in tags::split_parts(text) {
        match p {
            Part::Block(b) => {
                for tag in tags::split_block_tags(&b) {
                    let (n, a) = tags::tag_name_arg(&tag);
                    let a = a.trim();
                    match n.as_str() {
                        "i" => st.i = a == "1",
                        "b" => st.b = a.parse::<i32>().map(|v| v == 1 || v >= 600).unwrap_or(false),
                        "u" => st.u = a == "1",
                        "s" => st.s = a == "1",
                        "c" | "1c" => {
                            st.color = if a.is_empty() { None } else { Color::parse(a).map(|c| (c.r, c.g, c.b)) }
                        }
                        "r" => st = St::default(),
                        _ => {}
                    }
                }
            }
            Part::Text(t) => {
                let t = t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ");
                if t.is_empty() {
                    continue;
                }
                match out.last_mut() {
                    Some((s, x)) if *s == st => x.push_str(&t),
                    _ => out.push((st.clone(), t)),
                }
            }
        }
    }
    out
}

fn plain(text: &str) -> String {
    segments(text).into_iter().map(|x| x.1).collect()
}

fn hex6(c: (u8, u8, u8)) -> String {
    format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

/// ASS text -> HTML-ish markup (`<i>`, `<b>`, `<u>`, `<s>`, optional `<font color>`), newlines as `nl`.
fn to_html(text: &str, colour: bool, esc: bool, nl: &str) -> String {
    let mut out = String::new();
    let mut open: Vec<&'static str> = Vec::new(); // closing tags, innermost last
    let mut cur = St::default();
    let close_all = |out: &mut String, open: &mut Vec<&'static str>| {
        while let Some(c) = open.pop() {
            out.push_str(c);
        }
    };
    for (st, t) in segments(text) {
        let st = if colour { st } else { St { color: None, ..st } };
        if st != cur {
            close_all(&mut out, &mut open);
            if st.b { out.push_str("<b>"); open.push("</b>"); }
            if st.i { out.push_str("<i>"); open.push("</i>"); }
            if st.u { out.push_str("<u>"); open.push("</u>"); }
            if st.s { out.push_str("<s>"); open.push("</s>"); }
            if let Some(c) = st.color {
                out.push_str(&format!("<font color=\"{}\">", hex6(c)));
                open.push("</font>");
            }
            cur = st;
        }
        let t = if esc { t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") } else { t };
        out.push_str(&t.replace('\n', nl));
    }
    close_all(&mut out, &mut open);
    out
}

/// HTML-ish markup -> ASS text. Unknown tags are dropped; newlines and `<br>` become `\N`.
fn from_html(t: &str) -> String {
    let re = Regex::new(r"(?i)<(/?)(i|b|u|s|em|strong|strike|font|br)\b([^>]*?)(/?)>").unwrap();
    let col = Regex::new(r#"(?i)color\s*=\s*["']?#?([0-9a-fA-F]{6})"#).unwrap();
    let mut out = re
        .replace_all(t, |c: &regex::Captures| {
            let closing = &c[1] == "/";
            let tag = c[2].to_ascii_lowercase();
            match tag.as_str() {
                "br" => "\\N".to_string(),
                "font" => {
                    if closing {
                        "{\\c}".to_string()
                    } else if let Some(m) = col.captures(&c[3]) {
                        let h = &m[1];
                        format!("{{\\c&H{}{}{}&}}", h[4..6].to_ascii_uppercase(), h[2..4].to_ascii_uppercase(), h[0..2].to_ascii_uppercase())
                    } else {
                        String::new()
                    }
                }
                _ => {
                    let n = match tag.as_str() {
                        "em" => "i",
                        "strong" => "b",
                        "strike" => "s",
                        o => o,
                    };
                    format!("{{\\{}{}}}", n, if closing { 0 } else { 1 })
                }
            }
        })
        .to_string();
    let any = Regex::new(r"</?[a-zA-Z][^>]*>").unwrap();
    out = any.replace_all(&out, "").to_string();
    out.replace("\r\n", "\\N").replace('\n', "\\N").replace('\r', "")
}

fn decode_entities(s: &str) -> String {
    let re = Regex::new(r"&(#x[0-9a-fA-F]+|#\d+|[a-zA-Z]+);").unwrap();
    re.replace_all(s, |c: &regex::Captures| {
        let e = &c[1];
        if let Some(h) = e.strip_prefix("#x") {
            u32::from_str_radix(h, 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_else(|| c[0].to_string())
        } else if let Some(d) = e.strip_prefix('#') {
            d.parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_else(|| c[0].to_string())
        } else {
            match e {
                "amp" => "&".into(),
                "lt" => "<".into(),
                "gt" => ">".into(),
                "quot" => "\"".into(),
                "apos" => "'".into(),
                "nbsp" => "\u{a0}".into(),
                _ => c[0].to_string(),
            }
        }
    })
    .into_owned()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn norm_input(s: &str) -> String {
    s.strip_prefix('\u{feff}').unwrap_or(s).replace("\r\n", "\n").replace('\r', "\n")
}

/// Format `ms` as `[h]h:mm:ss<sep><frac>` with `digits` fractional digits (rounded, carries).
fn hms(ms: i64, digits: u32, hour_width: usize, sep: char) -> String {
    let ms = ms.max(0);
    let unit = 10i64.pow(3 - digits);
    let total = (ms + unit / 2) / unit;
    let scale = 10i64.pow(digits);
    let frac = total % scale;
    let secs = total / scale;
    format!(
        "{:0hw$}:{:02}:{:02}{}{:0fw$}",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60,
        sep,
        frac,
        hw = hour_width,
        fw = digits as usize
    )
}

fn writable(sc: &Script) -> impl Iterator<Item = &Event> {
    sc.events.iter().filter(|e| !e.comment)
}

fn script_with(events: Vec<Event>) -> Script {
    let mut sc = Script::new();
    sc.events = events;
    sc
}

/// Fix cues whose end is unknown (`end <= start` placeholder `i64::MIN` aside): extend to next start.
fn fill_ends(evs: &mut [Event], pending: &[bool], default_len: i64) {
    for i in 0..evs.len() {
        if pending[i] {
            evs[i].end = if i + 1 < evs.len() { evs[i + 1].start.max(evs[i].start) } else { evs[i].start + default_len };
        }
    }
}

// ------------------------------------------------------------------------------------------
// MicroDVD
// ------------------------------------------------------------------------------------------

fn frame_to_ms(f: i64, fps: f64) -> i64 {
    (f as f64 * 1000.0 / fps).round() as i64
}
fn ms_to_frame(ms: i64, fps: f64) -> i64 {
    (ms as f64 * fps / 1000.0).round() as i64
}

/// Parse MicroDVD `{start}{end}text` (frames). A first cue `{1}{1}23.976` sets the frame rate.
/// `|` is the line break; `{y:ibus}` and `{c:$bbggrr}` prefixes map to ASS tags.
pub fn parse_microdvd(input: &str, fps: f64) -> Script {
    let input = norm_input(input);
    let mut fps = if fps > 0.0 { fps } else { 23.976 };
    let line_re = Regex::new(r"^\{(\d+)\}\{(\d*)\}(.*)$").unwrap();
    let style_re = Regex::new(r"^((?:\{[a-zA-Z]:[^}]*\})*)").unwrap();
    let one = Regex::new(r"\{([a-zA-Z]):([^}]*)\}").unwrap();
    let mut raw: Vec<(i64, Option<i64>, String)> = Vec::new();
    for (n, line) in input.lines().enumerate() {
        let Some(c) = line_re.captures(line.trim()) else { continue };
        let (Ok(s), e) = (c[1].parse::<i64>(), c[2].parse::<i64>().ok()) else { continue };
        let text = c[3].to_string();
        if raw.is_empty() && n == 0 || (raw.is_empty() && s <= 1) {
            if let Ok(f) = text.trim().parse::<f64>() {
                if (5.0..=120.0).contains(&f) && e.map(|x| x <= 1).unwrap_or(false) {
                    fps = f;
                    continue;
                }
            }
        }
        raw.push((s, e, text));
    }
    let mut evs = Vec::new();
    let mut pending = Vec::new();
    for (s, e, text) in raw {
        let mut lines = Vec::new();
        for seg in text.split('|') {
            let m = style_re.find(seg).map(|m| m.as_str().to_string()).unwrap_or_default();
            let body = &seg[m.len()..];
            let (mut pre, mut post) = (String::new(), String::new());
            for t in one.captures_iter(&m) {
                match (&t[1], t[2].trim()) {
                    ("y" | "Y", flags) => {
                        for ch in flags.chars() {
                            if matches!(ch, 'i' | 'b' | 'u' | 's') {
                                pre.push_str(&format!("{{\\{}1}}", ch));
                                post.push_str(&format!("{{\\{}0}}", ch));
                            }
                        }
                    }
                    ("c" | "C", v) => {
                        let h = v.trim_start_matches('$').trim_end_matches('&');
                        if let Ok(x) = u32::from_str_radix(h, 16) {
                            pre.push_str(&format!("{{\\c&H{:06X}&}}", x & 0xFFFFFF));
                            post.push_str("{\\c}");
                        }
                    }
                    _ => {}
                }
            }
            lines.push(format!("{}{}{}", pre, body, post));
        }
        let mut ev = Event::new(frame_to_ms(s, fps), 0, "Default", &lines.join("\\N"));
        match e {
            Some(e) => {
                ev.end = frame_to_ms(e, fps);
                pending.push(false);
            }
            None => pending.push(true),
        }
        evs.push(ev);
    }
    fill_ends(&mut evs, &pending, 2000);
    script_with(evs)
}

/// Write MicroDVD at the given frame rate. A line whose runs all share one style gets `{y:..}`/`{c:..}`.
pub fn write_microdvd(sc: &Script, fps: f64) -> String {
    let fps = if fps > 0.0 { fps } else { 23.976 };
    let mut o = String::new();
    for e in writable(sc) {
        let segs = segments(&e.text);
        // regroup into lines
        let mut lines: Vec<Vec<(St, String)>> = vec![vec![]];
        for (st, t) in segs {
            let mut it = t.split('\n');
            if let Some(first) = it.next() {
                if !first.is_empty() { lines.last_mut().unwrap().push((st.clone(), first.to_string())); }
            }
            for rest in it {
                lines.push(vec![]);
                if !rest.is_empty() { lines.last_mut().unwrap().push((st.clone(), rest.to_string())); }
            }
        }
        let rendered: Vec<String> = lines
            .iter()
            .map(|l| {
                let txt: String = l.iter().map(|x| x.1.as_str()).collect();
                match l.first() {
                    Some((st, _)) if l.iter().all(|x| &x.0 == st) => {
                        let mut pre = String::new();
                        let flags: String = [(st.i, 'i'), (st.b, 'b'), (st.u, 'u'), (st.s, 's')]
                            .iter()
                            .filter(|x| x.0)
                            .map(|x| x.1)
                            .collect();
                        if !flags.is_empty() { pre.push_str(&format!("{{y:{}}}", flags)); }
                        if let Some(c) = st.color { pre.push_str(&format!("{{c:${:02X}{:02X}{:02X}}}", c.2, c.1, c.0)); }
                        format!("{}{}", pre, txt)
                    }
                    _ => txt,
                }
            })
            .collect();
        o.push_str(&format!("{{{}}}{{{}}}{}\n", ms_to_frame(e.start, fps), ms_to_frame(e.end, fps), rendered.join("|")));
    }
    o
}

// ------------------------------------------------------------------------------------------
// SubViewer 2
// ------------------------------------------------------------------------------------------

pub fn parse_subviewer2(input: &str) -> Script {
    let input = norm_input(input);
    let time_re = Regex::new(r"^(\d+:\d+:\d+[.,:]\d+)\s*,\s*(\d+:\d+:\d+[.,:]\d+)\s*$").unwrap();
    let mut sc_info: Vec<(String, String)> = Vec::new();
    let mut evs = Vec::new();
    let mut lines = input.lines().peekable();
    while let Some(l) = lines.next() {
        let t = l.trim();
        if let Some(v) = t.strip_prefix("[TITLE]") {
            if !v.trim().is_empty() { sc_info.push(("Title".into(), v.trim().into())); }
        } else if let Some(v) = t.strip_prefix("[AUTHOR]") {
            if !v.trim().is_empty() { sc_info.push(("Original Script".into(), v.trim().into())); }
        }
        if let Some(c) = time_re.captures(t) {
            let norm = |s: &str| {
                // `hh:mm:ss:cc` variant -> `hh:mm:ss.cc`
                let p: Vec<&str> = s.split(':').collect();
                if p.len() == 4 { format!("{}:{}:{}.{}", p[0], p[1], p[2], p[3]) } else { s.replace(',', ".") }
            };
            let (Some(s), Some(e)) = (time::parse_ass(&norm(&c[1])), time::parse_ass(&norm(&c[2]))) else { continue };
            let mut text = Vec::new();
            while let Some(n) = lines.peek() {
                if n.trim().is_empty() { break; }
                text.push(lines.next().unwrap().trim_end().to_string());
            }
            let joined = text.join("[br]");
            let mut ev = Event::new(s, e, "Default", "");
            ev.text = from_html(&joined.replace("[br]", "<br>").replace("[BR]", "<br>"));
            evs.push(ev);
        }
    }
    let mut sc = script_with(evs);
    for (k, v) in sc_info { sc.info_set(&k, &v); }
    sc
}

pub fn write_subviewer2(sc: &Script) -> String {
    let mut o = String::new();
    o.push_str("[INFORMATION]\n");
    o.push_str(&format!("[TITLE]{}\n", sc.info_get("Title").unwrap_or("")));
    o.push_str(&format!("[AUTHOR]{}\n", sc.info_get("Original Script").unwrap_or("")));
    o.push_str("[SOURCE]\n[PRG]\n[FILEPATH]\n[DELAY]0\n[CD TRACK]0\n[COMMENT]\n[END INFORMATION]\n[SUBTITLE]\n");
    o.push_str("[COLF]&HFFFFFF,[STYLE]bd,[SIZE]18,[FONT]Arial\n");
    for e in writable(sc) {
        o.push_str(&format!("{},{}\n{}\n\n", hms(e.start, 2, 2, '.'), hms(e.end, 2, 2, '.'), to_html(&e.text, false, false, "[br]")));
    }
    o
}

// ------------------------------------------------------------------------------------------
// SBV (YouTube)
// ------------------------------------------------------------------------------------------

pub fn parse_sbv(input: &str) -> Script {
    let input = norm_input(input);
    let re = Regex::new(r"^(\d+:\d+:\d+\.\d+)\s*,\s*(\d+:\d+:\d+\.\d+)\s*$").unwrap();
    let mut evs = Vec::new();
    for block in input.split("\n\n") {
        let lines: Vec<&str> = block.lines().skip_while(|l| l.trim().is_empty()).collect();
        let Some(first) = lines.first() else { continue };
        let Some(c) = re.captures(first.trim()) else { continue };
        let (Some(s), Some(e)) = (time::parse_ass(&c[1]), time::parse_ass(&c[2])) else { continue };
        evs.push(Event::new(s, e, "Default", &lines[1..].join("\\N")));
    }
    script_with(evs)
}

pub fn write_sbv(sc: &Script) -> String {
    let mut o = String::new();
    for e in writable(sc) {
        o.push_str(&format!("{},{}\n{}\n\n", hms(e.start, 3, 1, '.'), hms(e.end, 3, 1, '.'), plain(&e.text)));
    }
    o
}

// ------------------------------------------------------------------------------------------
// LRC
// ------------------------------------------------------------------------------------------

fn lrc_stamp_ms(m: &str, s: &str, f: Option<&str>) -> i64 {
    let m: i64 = m.parse().unwrap_or(0);
    let s: i64 = s.parse().unwrap_or(0);
    let frac = match f {
        None | Some("") => 0,
        Some(f) => {
            let f: String = f.chars().take(3).collect();
            let v: i64 = f.parse().unwrap_or(0);
            v * 10i64.pow(3 - f.len() as u32)
        }
    };
    (m * 60 + s) * 1000 + frac
}

fn lrc_fmt(ms: i64) -> String {
    let cs = (ms.max(0) + 5) / 10;
    format!("{:02}:{:02}.{:02}", cs / 6000, (cs / 100) % 60, cs % 100)
}

/// Parse LRC. Cue end = next stamp (an empty line ends the previous cue); the last cue lasts 3 s.
/// Enhanced `<mm:ss.xx>` word stamps become `{\kN}` karaoke syllables. Metadata `[ti:..]` goes to
/// `info` as `lrc_ti`.
pub fn parse_lrc(input: &str) -> Script {
    let input = norm_input(input);
    let line_re = Regex::new(r"^((?:\[\d+:\d+(?:[.:]\d+)?\])+)(.*)$").unwrap();
    let stamp_re = Regex::new(r"\[(\d+):(\d+)(?:[.:](\d+))?\]").unwrap();
    let word_re = Regex::new(r"<(\d+):(\d+)(?:[.:](\d+))?>").unwrap();
    let meta_re = Regex::new(r"^\[([A-Za-z#]+):(.*)\]\s*$").unwrap();
    struct Entry { t: i64, text: String, words: Vec<(i64, String)>, lead: String }
    let mut entries: Vec<Entry> = Vec::new();
    let mut meta: Vec<(String, String)> = Vec::new();
    for line in input.lines() {
        let l = line.trim_end();
        if let Some(c) = line_re.captures(l) {
            let body = c[2].to_string();
            let mut words = Vec::new();
            let mut lead = body.clone();
            let ms: Vec<_> = word_re.captures_iter(&body).collect();
            if !ms.is_empty() {
                lead = body[..ms[0].get(0).unwrap().start()].to_string();
                for (k, m) in ms.iter().enumerate() {
                    let end = ms.get(k + 1).map(|n| n.get(0).unwrap().start()).unwrap_or(body.len());
                    let t = lrc_stamp_ms(&m[1], &m[2], m.get(3).map(|x| x.as_str()));
                    words.push((t, body[m.get(0).unwrap().end()..end].to_string()));
                }
            }
            let plain_body = word_re.replace_all(&body, "").trim().to_string();
            for (k, s) in stamp_re.captures_iter(&c[1]).enumerate() {
                let t = lrc_stamp_ms(&s[1], &s[2], s.get(3).map(|x| x.as_str()));
                entries.push(Entry {
                    t,
                    text: plain_body.clone(),
                    words: if k == 0 { words.clone() } else { vec![] },
                    lead: lead.clone(),
                });
            }
        } else if let Some(c) = meta_re.captures(l.trim()) {
            meta.push((c[1].to_ascii_lowercase(), c[2].trim().to_string()));
        }
    }
    entries.sort_by_key(|e| e.t); // stable: same-stamp lines keep file order
    let mut evs: Vec<Event> = Vec::new();
    let n = entries.len();
    for i in 0..n {
        if entries[i].text.is_empty() {
            continue;
        }
        // merge consecutive same-stamp lines into one multi-line cue
        let start = entries[i].t;
        if i > 0 && entries[i - 1].t == start && !entries[i - 1].text.is_empty() {
            if let Some(last) = evs.last_mut() {
                last.text.push_str("\\N");
                last.text.push_str(&entries[i].text);
            }
            continue;
        }
        let end = entries[i + 1..].iter().find(|x| x.t > start).map(|x| x.t).unwrap_or(start + 3000);
        let text = if entries[i].words.is_empty() {
            entries[i].text.clone()
        } else {
            let mut t = entries[i].lead.clone();
            let w = &entries[i].words;
            for (k, (ws, wt)) in w.iter().enumerate() {
                let we = w.get(k + 1).map(|x| x.0).unwrap_or(end);
                let cs = ((we - ws).max(0) + 5) / 10;
                t.push_str(&format!("{{\\k{}}}{}", cs, wt));
            }
            t
        };
        evs.push(Event::new(start, end, "Default", &text));
    }
    let mut sc = script_with(evs);
    for (k, v) in meta {
        sc.info_set(&format!("lrc_{}", k), &v);
    }
    sc
}

/// Write LRC; with `word_level`, cues carrying `\k` tags get enhanced `<mm:ss.xx>` word stamps.
/// An empty stamp line is emitted wherever a cue ends before the next one starts.
pub fn write_lrc(sc: &Script, word_level: bool) -> String {
    let mut o = String::new();
    for (k, v) in &sc.info {
        if let Some(key) = k.strip_prefix("lrc_") {
            o.push_str(&format!("[{}:{}]\n", key, v));
        }
    }
    let evs: Vec<&Event> = writable(sc).collect();
    for (i, e) in evs.iter().enumerate() {
        let syl = tags::karaoke_syllables(&e.text);
        if word_level && syl.iter().any(|s| s.duration_cs > 0) {
            o.push_str(&format!("[{}]", lrc_fmt(e.start)));
            let mut t = e.start;
            for s in &syl {
                o.push_str(&format!("<{}>{}", lrc_fmt(t), plain(&s.text).replace('\n', " ")));
                t += s.duration_cs * 10;
            }
            o.push('\n');
        } else {
            for line in plain(&e.text).split('\n') {
                o.push_str(&format!("[{}]{}\n", lrc_fmt(e.start), line));
            }
        }
        let gap = evs.get(i + 1).map(|n| n.start > e.end).unwrap_or(true);
        if gap {
            o.push_str(&format!("[{}]\n", lrc_fmt(e.end)));
        }
    }
    o
}

// ------------------------------------------------------------------------------------------
// Plain text
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct TextImport {
    /// Every line is a cue. When false, paragraphs (separated by blank lines) are cues; a file
    /// without any blank line is treated line-per-cue anyway.
    pub per_line: bool,
    /// Fixed duration per cue; `None` computes one from the length (65 ms/char, at least 1.2 s).
    pub duration_ms: Option<i64>,
    pub gap_ms: i64,
    pub start_ms: i64,
}

pub fn parse_text(input: &str) -> Script {
    parse_text_opts(input, &TextImport::default())
}

/// Time-less import: cues are laid out back to back starting at `start_ms`.
pub fn parse_text_opts(input: &str, o: &TextImport) -> Script {
    let input = norm_input(input);
    let has_blank = input.lines().any(|l| l.trim().is_empty());
    let mut cues: Vec<Vec<&str>> = Vec::new();
    if o.per_line || !has_blank {
        for l in input.lines().filter(|l| !l.trim().is_empty()) {
            cues.push(vec![l.trim()]);
        }
    } else {
        let mut cur: Vec<&str> = Vec::new();
        for l in input.lines() {
            if l.trim().is_empty() {
                if !cur.is_empty() { cues.push(std::mem::take(&mut cur)); }
            } else {
                cur.push(l.trim());
            }
        }
        if !cur.is_empty() { cues.push(cur); }
    }
    let mut t = o.start_ms;
    let mut evs = Vec::new();
    for c in cues {
        let chars: usize = c.iter().map(|l| l.chars().count()).sum();
        let d = o.duration_ms.unwrap_or_else(|| (chars as i64 * 65).max(1200));
        evs.push(Event::new(t, t + d, "Default", &c.join("\\N")));
        t += d + o.gap_ms;
    }
    script_with(evs)
}

pub fn write_text(sc: &Script) -> String {
    let mut o = String::new();
    for e in writable(sc) {
        o.push_str(&plain(&e.text));
        o.push_str("\n\n");
    }
    o
}

// ------------------------------------------------------------------------------------------
// CSV
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsvColumn {
    Index,
    Start,
    End,
    Duration,
    Text,
    Style,
    Actor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsvTime {
    /// Integer milliseconds.
    Millis,
    /// `HH:MM:SS,mmm`
    Srt,
    /// `H:MM:SS.cc`
    Ass,
    /// Decimal seconds with 3 decimals.
    Seconds,
}

#[derive(Clone, Debug)]
pub struct CsvOptions {
    pub delimiter: char,
    pub quote: char,
    /// Write a header row / skip + interpret one when reading.
    pub header: bool,
    pub columns: Vec<CsvColumn>,
    pub time: CsvTime,
}

impl Default for CsvOptions {
    fn default() -> Self {
        CsvOptions {
            delimiter: ',',
            quote: '"',
            header: true,
            columns: vec![CsvColumn::Index, CsvColumn::Start, CsvColumn::End, CsvColumn::Text],
            time: CsvTime::Srt,
        }
    }
}

fn csv_col_name(c: CsvColumn) -> &'static str {
    match c {
        CsvColumn::Index => "Number",
        CsvColumn::Start => "Start",
        CsvColumn::End => "End",
        CsvColumn::Duration => "Duration",
        CsvColumn::Text => "Text",
        CsvColumn::Style => "Style",
        CsvColumn::Actor => "Actor",
    }
}

fn csv_col_from(name: &str) -> Option<CsvColumn> {
    match name.trim().to_ascii_lowercase().as_str() {
        "number" | "index" | "#" | "no" | "nr" | "id" => Some(CsvColumn::Index),
        "start" | "start time" | "begin" | "in" | "show" => Some(CsvColumn::Start),
        "end" | "end time" | "stop" | "out" | "hide" => Some(CsvColumn::End),
        "duration" | "dur" => Some(CsvColumn::Duration),
        "text" | "content" | "subtitle" | "line" => Some(CsvColumn::Text),
        "style" => Some(CsvColumn::Style),
        "actor" | "name" | "speaker" => Some(CsvColumn::Actor),
        _ => None,
    }
}

fn csv_records(input: &str, delim: char, quote: char) -> Vec<Vec<String>> {
    let mut recs = Vec::new();
    let mut rec: Vec<String> = Vec::new();
    let mut f = String::new();
    let mut inq = false;
    let mut any = false;
    let mut it = input.chars().peekable();
    while let Some(c) = it.next() {
        if inq {
            if c == quote {
                if it.peek() == Some(&quote) { f.push(quote); it.next(); } else { inq = false; }
            } else {
                f.push(c);
            }
        } else if c == quote {
            inq = true;
            any = true;
        } else if c == delim {
            rec.push(std::mem::take(&mut f));
            any = true;
        } else if c == '\n' {
            if any || !f.is_empty() || !rec.is_empty() {
                rec.push(std::mem::take(&mut f));
                recs.push(std::mem::take(&mut rec));
            }
            any = false;
        } else if c != '\r' {
            f.push(c);
        }
    }
    if any || !f.is_empty() || !rec.is_empty() {
        rec.push(f);
        recs.push(rec);
    }
    recs
}

fn csv_time(s: &str, mode: CsvTime) -> Option<i64> {
    let s = s.trim();
    if s.contains(':') {
        return time::parse_ass(s);
    }
    if mode == CsvTime::Seconds {
        return s.replace(',', ".").parse::<f64>().ok().map(|v| (v * 1000.0).round() as i64);
    }
    s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().map(|v| v.round() as i64))
}

fn csv_fmt_time(ms: i64, mode: CsvTime) -> String {
    match mode {
        CsvTime::Millis => ms.to_string(),
        CsvTime::Srt => time::format_srt(ms),
        CsvTime::Ass => time::format_ass(ms),
        CsvTime::Seconds => format!("{}.{:03}", ms.div_euclid(1000), ms.rem_euclid(1000)),
    }
}

pub fn parse_csv(input: &str, o: &CsvOptions) -> Script {
    let input = norm_input(input);
    let recs = csv_records(&input, o.delimiter, o.quote);
    let mut cols = o.columns.clone();
    let mut iter = recs.into_iter();
    if o.header {
        // peek at the header; use it for column order when every name is recognised
        if let Some(h) = iter.next() {
            let named: Vec<Option<CsvColumn>> = h.iter().map(|n| csv_col_from(n)).collect();
            if named.iter().all(|x| x.is_some()) && named.contains(&Some(CsvColumn::Text)) {
                cols = named.into_iter().flatten().collect();
            }
        }
    }
    let mut evs = Vec::new();
    for r in iter {
        let (mut s, mut e, mut d, mut text, mut style, mut actor) = (None, None, None, String::new(), String::new(), String::new());
        for (i, c) in cols.iter().enumerate() {
            let Some(v) = r.get(i) else { continue };
            match c {
                CsvColumn::Start => s = csv_time(v, o.time),
                CsvColumn::End => e = csv_time(v, o.time),
                CsvColumn::Duration => d = csv_time(v, o.time),
                CsvColumn::Text => text = v.clone(),
                CsvColumn::Style => style = v.clone(),
                CsvColumn::Actor => actor = v.clone(),
                CsvColumn::Index => {}
            }
        }
        let Some(s) = s else { continue };
        let Some(e) = e.or(d.map(|d| s + d)) else { continue };
        let mut ev = Event::new(s, e, if style.is_empty() { "Default" } else { &style }, &from_html(&text));
        ev.actor = actor;
        evs.push(ev);
    }
    script_with(evs)
}

pub fn write_csv(sc: &Script, o: &CsvOptions) -> String {
    let q = o.quote;
    let esc = |s: &str| {
        if s.contains(o.delimiter) || s.contains(q) || s.contains('\n') || s.starts_with(' ') || s.ends_with(' ') {
            format!("{q}{}{q}", s.replace(q, &format!("{q}{q}")))
        } else {
            s.to_string()
        }
    };
    let d = o.delimiter.to_string();
    let mut out = String::new();
    if o.header {
        out.push_str(&o.columns.iter().map(|c| csv_col_name(*c)).collect::<Vec<_>>().join(&d));
        out.push('\n');
    }
    for (n, e) in writable(sc).enumerate() {
        let row: Vec<String> = o
            .columns
            .iter()
            .map(|c| match c {
                CsvColumn::Index => (n + 1).to_string(),
                CsvColumn::Start => csv_fmt_time(e.start, o.time),
                CsvColumn::End => csv_fmt_time(e.end, o.time),
                CsvColumn::Duration => csv_fmt_time(e.end - e.start, o.time),
                CsvColumn::Text => esc(&to_html(&e.text, false, false, "\n")),
                CsvColumn::Style => esc(&e.style),
                CsvColumn::Actor => esc(&e.actor),
            })
            .collect();
        out.push_str(&row.join(&d));
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------------------------------------
// TTML / DFXP
// ------------------------------------------------------------------------------------------

fn xml_attrs(s: &str) -> Vec<(String, String)> {
    let re = Regex::new(r#"([\w:.\-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    re.captures_iter(s)
        .map(|c| {
            let k = c[1].to_string();
            let local = k.rsplit(':').next().unwrap_or(&k).to_string();
            (local, decode_entities(c.get(2).or(c.get(3)).map(|m| m.as_str()).unwrap_or("")))
        })
        .collect()
}

fn attr<'a>(a: &'a [(String, String)], k: &str) -> Option<&'a str> {
    a.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

fn ttml_color(v: &str) -> Option<(u8, u8, u8)> {
    let v = v.trim();
    if let Some(h) = v.strip_prefix('#') {
        if h.len() >= 6 {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            return Some((r, g, b));
        }
        if h.len() == 3 {
            let x = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|n| n * 17);
            return Some((x(0)?, x(1)?, x(2)?));
        }
        return None;
    }
    match v.to_ascii_lowercase().as_str() {
        "white" => Some((255, 255, 255)),
        "black" => Some((0, 0, 0)),
        "red" => Some((255, 0, 0)),
        "lime" | "green" => Some((0, 255, 0)),
        "blue" => Some((0, 0, 255)),
        "yellow" => Some((255, 255, 0)),
        "cyan" | "aqua" => Some((0, 255, 255)),
        "magenta" | "fuchsia" => Some((255, 0, 255)),
        _ => None,
    }
}

fn ttml_time(s: &str, fps: f64, tick: f64) -> Option<i64> {
    let s = s.trim();
    let metric = Regex::new(r"^(\d+(?:\.\d+)?)(ms|h|m|s|f|t)$").unwrap();
    if let Some(c) = metric.captures(s) {
        let v: f64 = c[1].parse().ok()?;
        let ms = match &c[2] {
            "ms" => v,
            "h" => v * 3_600_000.0,
            "m" => v * 60_000.0,
            "s" => v * 1000.0,
            "f" => v * 1000.0 / fps,
            _ => v * 1000.0 / tick,
        };
        return Some(ms.round() as i64);
    }
    let p: Vec<&str> = s.split(':').collect();
    if p.len() == 4 {
        let h: i64 = p[0].parse().ok()?;
        let m: i64 = p[1].parse().ok()?;
        let sec: i64 = p[2].parse().ok()?;
        let f: f64 = p[3].parse().ok()?;
        return Some(((h * 3600 + m * 60 + sec) as f64 * 1000.0 + f * 1000.0 / fps).round() as i64);
    }
    time::parse_ass(s)
}

/// Opening ASS tags for a set of TTML style attributes plus the matching closers.
fn ttml_open(a: &[(String, String)]) -> (String, String) {
    let (mut o, mut c) = (String::new(), String::new());
    if let Some(v) = attr(a, "fontStyle") {
        if matches!(v, "italic" | "oblique") { o.push_str("{\\i1}"); c.insert_str(0, "{\\i0}"); }
    }
    if let Some(v) = attr(a, "fontWeight") {
        if v == "bold" { o.push_str("{\\b1}"); c.insert_str(0, "{\\b0}"); }
    }
    if let Some(v) = attr(a, "textDecoration") {
        if v.contains("underline") { o.push_str("{\\u1}"); c.insert_str(0, "{\\u0}"); }
        if v.contains("lineThrough") { o.push_str("{\\s1}"); c.insert_str(0, "{\\s0}"); }
    }
    if let Some(col) = attr(a, "color").and_then(ttml_color) {
        o.push_str(&format!("{{\\c&H{:02X}{:02X}{:02X}&}}", col.2, col.1, col.0));
        c.insert_str(0, "{\\c}");
    }
    (o, c)
}

/// begin, end, dur, text so far, span closers, paragraph closer
type TtmlPara = (i64, Option<i64>, Option<i64>, String, Vec<String>, String);

pub fn parse_ttml(input: &str) -> Script {
    let input = norm_input(input);
    let tok = Regex::new(r"(?s)<!--.*?-->|<[^>]+>|[^<]+").unwrap();
    let mut fps = 30.0;
    let mut tick = 10_000_000.0;
    let mut styles: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut evs: Vec<Event> = Vec::new();
    // current paragraph
    let ws = Regex::new(r"[ \t\n]+").unwrap();
    let mut cur: Option<TtmlPara> = None;
    let style_attrs = |styles: &Vec<(String, Vec<(String, String)>)>, a: &[(String, String)]| -> Vec<(String, String)> {
        let mut m: Vec<(String, String)> = Vec::new();
        if let Some(refs) = attr(a, "style") {
            for r in refs.split_whitespace() {
                if let Some((_, s)) = styles.iter().find(|(id, _)| id == r) {
                    m.extend(s.iter().cloned());
                }
            }
        }
        m.extend(a.iter().cloned());
        m
    };
    for m in tok.find_iter(&input) {
        let t = m.as_str();
        if t.starts_with("<!--") || t.starts_with("<?") {
            continue;
        }
        if let Some(rest) = t.strip_prefix('<') {
            let body = rest.trim_end_matches('>');
            let closing = body.starts_with('/');
            let selfclose = body.ends_with('/');
            let body = body.trim_start_matches('/').trim_end_matches('/');
            let (name, attrs_s) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
            let local = name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase();
            let a = xml_attrs(attrs_s);
            match (local.as_str(), closing) {
                ("tt", false) => {
                    if let Some(f) = attr(&a, "frameRate").and_then(|v| v.parse::<f64>().ok()) {
                        let mult = attr(&a, "frameRateMultiplier")
                            .and_then(|v| {
                                let p: Vec<f64> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                                if p.len() == 2 && p[1] != 0.0 { Some(p[0] / p[1]) } else { None }
                            })
                            .unwrap_or(1.0);
                        fps = f * mult;
                    }
                    if let Some(v) = attr(&a, "tickRate").and_then(|v| v.parse::<f64>().ok()) { tick = v; }
                }
                ("style", false) => {
                    if let Some(id) = attr(&a, "id") { styles.push((id.to_string(), a.clone())); }
                }
                ("p", false) if !selfclose => {
                    let a2 = style_attrs(&styles, &a);
                    let b = attr(&a, "begin").and_then(|v| ttml_time(v, fps, tick));
                    let e = attr(&a, "end").and_then(|v| ttml_time(v, fps, tick));
                    let d = attr(&a, "dur").and_then(|v| ttml_time(v, fps, tick));
                    let (o, c) = ttml_open(&a2);
                    cur = b.map(|b| (b, e, d, o, vec![], c));
                }
                ("p", true) => {
                    if let Some((b, e, d, text, _, pc)) = cur.take() {
                        // collapse whitespace outside of `\N`
                        let mut txt = ws.replace_all(&text, " ").to_string();
                        txt = txt.replace(" \\N", "\\N").replace("\\N ", "\\N");
                        let txt = format!("{}{}", txt.trim(), pc);
                        let end = e.or(d.map(|d| b + d)).unwrap_or(b);
                        evs.push(Event::new(b, end, "Default", txt.trim()));
                    }
                }
                ("span", false) if cur.is_some() => {
                    let a2 = style_attrs(&styles, &a);
                    let (o, c) = ttml_open(&a2);
                    let cu = cur.as_mut().unwrap();
                    cu.3.push_str(&o);
                    cu.4.push(if selfclose { String::new() } else { c });
                    if selfclose { let c = cu.4.pop().unwrap_or_default(); cu.3.push_str(&c); }
                }
                ("span", true) => {
                    if let Some(cu) = cur.as_mut() {
                        if let Some(c) = cu.4.pop() { cu.3.push_str(&c); }
                    }
                }
                ("br", false) => {
                    if let Some(cu) = cur.as_mut() { cu.3.push_str("\\N"); }
                }
                _ => {}
            }
        } else if let Some(cu) = cur.as_mut() {
            // text node: the paragraph's own `\N` markers are already inserted; escape braces
            cu.3.push_str(&decode_entities(t).replace('{', "(").replace('}', ")"));
        }
    }
    // The p-level closer is appended on `</p>`; ensure leading open tags were closed there.
    script_with(evs)
}

pub fn write_ttml(sc: &Script) -> String {
    let lang = sc.info_get("Language").unwrap_or("und").to_string();
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    o.push_str(&format!(
        "<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xml:lang=\"{}\">\n",
        escape_xml(&lang)
    ));
    o.push_str("  <head></head>\n  <body>\n    <div>\n");
    for e in writable(sc) {
        let mut body = String::new();
        for (st, t) in segments(&e.text) {
            let mut attrs = String::new();
            if st.i { attrs.push_str(" tts:fontStyle=\"italic\""); }
            if st.b { attrs.push_str(" tts:fontWeight=\"bold\""); }
            match (st.u, st.s) {
                (true, true) => attrs.push_str(" tts:textDecoration=\"underline lineThrough\""),
                (true, false) => attrs.push_str(" tts:textDecoration=\"underline\""),
                (false, true) => attrs.push_str(" tts:textDecoration=\"lineThrough\""),
                _ => {}
            }
            if let Some(c) = st.color { attrs.push_str(&format!(" tts:color=\"{}\"", hex6(c))); }
            let inner = escape_xml(&t).replace('\n', "<br/>");
            if attrs.is_empty() { body.push_str(&inner); } else { body.push_str(&format!("<span{}>{}</span>", attrs, inner)); }
        }
        o.push_str(&format!(
            "      <p begin=\"{}\" end=\"{}\">{}</p>\n",
            hms(e.start, 3, 2, '.'),
            hms(e.end, 3, 2, '.'),
            body
        ));
    }
    o.push_str("    </div>\n  </body>\n</tt>\n");
    o
}

// ------------------------------------------------------------------------------------------
// SAMI
// ------------------------------------------------------------------------------------------

pub fn parse_sami(input: &str) -> Script {
    let input = norm_input(input);
    let sync_re = Regex::new(r#"(?i)<sync\s+start\s*=\s*["']?(\d+)["']?[^>]*>"#).unwrap();
    let p_re = Regex::new(r"(?i)<p\b([^>]*)>").unwrap();
    let class_re = Regex::new(r#"(?i)class\s*=\s*["']?([\w\-]+)"#).unwrap();
    let body_end = Regex::new(r"(?i)</body>").unwrap();
    let p_close = Regex::new(r"(?i)</p>").unwrap();
    let syncs: Vec<_> = sync_re.captures_iter(&input).collect();
    let mut first_class: Option<String> = None;
    let mut rows: Vec<(i64, String)> = Vec::new(); // (start, text; empty = blank)
    for (k, c) in syncs.iter().enumerate() {
        let m = c.get(0).unwrap();
        let mut end = syncs.get(k + 1).map(|n| n.get(0).unwrap().start()).unwrap_or(input.len());
        if let Some(b) = body_end.find(&input[m.end()..end]) { end = m.end() + b.start(); }
        let chunk = &input[m.end()..end];
        let start: i64 = c[1].parse().unwrap_or(0);
        let ps: Vec<_> = p_re.captures_iter(chunk).collect();
        let mut text = String::new();
        if ps.is_empty() {
            text = chunk.to_string();
        } else {
            for (j, p) in ps.iter().enumerate() {
                let pm = p.get(0).unwrap();
                let pend = ps.get(j + 1).map(|n| n.get(0).unwrap().start()).unwrap_or(chunk.len());
                let cls = class_re.captures(&p[1]).map(|x| x[1].to_string());
                if first_class.is_none() { first_class = cls.clone(); }
                if cls == first_class {
                    text.push_str(&chunk[pm.end()..pend]);
                }
            }
        }
        let text = p_close.replace_all(&text, "").replace('\n', " ");
        let ass = from_html(&text);
        let ass = decode_entities(&ass).replace('\u{a0}', " ");
        rows.push((start, ass.trim().replace(" \\N", "\\N").replace("\\N ", "\\N")));
    }
    let mut evs = Vec::new();
    for (i, (s, t)) in rows.iter().enumerate() {
        if t.trim().is_empty() || t == "\\N" { continue; }
        let end = rows.get(i + 1).map(|r| r.0).unwrap_or(s + 3000);
        evs.push(Event::new(*s, end, "Default", t));
    }
    script_with(evs)
}

pub fn write_sami(sc: &Script) -> String {
    let mut o = String::new();
    o.push_str("<SAMI>\n<HEAD>\n");
    o.push_str(&format!("<TITLE>{}</TITLE>\n", escape_xml(sc.info_get("Title").unwrap_or(""))));
    o.push_str("<STYLE TYPE=\"text/css\">\n<!--\nP { margin-left:8pt; margin-right:8pt; margin-bottom:2pt; margin-top:2pt; text-align:center; font-size:12pt; font-family:Arial; font-weight:normal; color:white; background-color:black; }\n.ENCC { Name:English; lang:en-US; SAMIType:CC; }\n-->\n</STYLE>\n</HEAD>\n<BODY>\n");
    let evs: Vec<&Event> = writable(sc).collect();
    for (i, e) in evs.iter().enumerate() {
        let t = to_html(&e.text, true, true, "<br>");
        o.push_str(&format!("<SYNC Start={}><P Class=ENCC>{}\n", e.start.max(0), t));
        if evs.get(i + 1).map(|n| n.start > e.end).unwrap_or(true) {
            o.push_str(&format!("<SYNC Start={}><P Class=ENCC>&nbsp;\n", e.end.max(0)));
        }
    }
    o.push_str("</BODY>\n</SAMI>\n");
    o
}

// ------------------------------------------------------------------------------------------
// JSON (hand written)
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum J {
    Null,
    Bool,
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

struct JP<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl<'a> JP<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() { self.i += 1; }
    }
    fn value(&mut self, depth: u32) -> Option<J> {
        if depth > 64 { return None; }
        self.ws();
        match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    match *self.s.get(self.i)? {
                        b'}' => { self.i += 1; return Some(J::Obj(v)); }
                        b',' => { self.i += 1; }
                        b'"' => {
                            let k = self.string()?;
                            self.ws();
                            if *self.s.get(self.i)? != b':' { return None; }
                            self.i += 1;
                            v.push((k, self.value(depth + 1)?));
                        }
                        _ => return None,
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    match *self.s.get(self.i)? {
                        b']' => { self.i += 1; return Some(J::Arr(v)); }
                        b',' => { self.i += 1; }
                        _ => v.push(self.value(depth + 1)?),
                    }
                }
            }
            b'"' => self.string().map(J::Str),
            b't' if self.src[self.i..].starts_with("true") => { self.i += 4; Some(J::Bool) }
            b'f' if self.src[self.i..].starts_with("false") => { self.i += 5; Some(J::Bool) }
            b'n' if self.src[self.i..].starts_with("null") => { self.i += 4; Some(J::Null) }
            _ => {
                let st = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') { self.i += 1; }
                self.src[st..self.i].parse::<f64>().ok().map(J::Num)
            }
        }
    }
    fn hex4(&mut self) -> Option<u32> {
        let h = self.src.get(self.i..self.i + 4)?;
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }
    fn string(&mut self) -> Option<String> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let rest = &self.src[self.i..];
            let c = rest.chars().next()?;
            self.i += c.len_utf8();
            match c {
                '"' => return Some(out),
                '\\' => {
                    let e = self.src[self.i..].chars().next()?;
                    self.i += e.len_utf8();
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) && self.src[self.i..].starts_with("\\u") {
                                self.i += 2;
                                let lo = self.hex4()?;
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        o => out.push(o),
                    }
                }
                c => out.push(c),
            }
        }
    }
}

fn parse_json_value(s: &str) -> Option<J> {
    let mut p = JP { s: s.as_bytes(), src: s, i: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.i == s.len() { Some(v) } else { None }
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn json_time(v: &J) -> Option<i64> {
    match v {
        // Integers are milliseconds; numbers with a fractional part are seconds (Whisper style).
        J::Num(n) => Some(if n.fract() == 0.0 { *n as i64 } else { (n * 1000.0).round() as i64 }),
        J::Str(s) => time::parse_ass(s).or_else(|| s.trim().parse::<f64>().ok().map(|n| n.round() as i64)),
        _ => None,
    }
}

/// Parse `[{"start":ms,"end":ms,"text":"..."}]`. Also accepts timestamp strings, an enclosing
/// object holding the array, `begin/stop/content` aliases, and optional `style` / `actor` keys.
pub fn parse_json(input: &str) -> Script {
    let Some(root) = parse_json_value(input.strip_prefix('\u{feff}').unwrap_or(input).trim()) else {
        return script_with(vec![]);
    };
    let arr = match root {
        J::Arr(a) => a,
        J::Obj(o) => o.into_iter().find_map(|(_, v)| if let J::Arr(a) = v { Some(a) } else { None }).unwrap_or_default(),
        _ => vec![],
    };
    let mut evs = Vec::new();
    for item in arr {
        let J::Obj(o) = item else { continue };
        let get = |names: &[&str]| o.iter().find(|(k, _)| names.contains(&k.to_ascii_lowercase().as_str())).map(|x| &x.1);
        let Some(s) = get(&["start", "begin", "start_time", "from"]).and_then(json_time) else { continue };
        let e = get(&["end", "stop", "end_time", "to"]).and_then(json_time)
            .or_else(|| get(&["duration"]).and_then(json_time).map(|d| s + d));
        let Some(e) = e else { continue };
        let text = match get(&["text", "content", "line"]) {
            Some(J::Str(t)) => from_html(t),
            _ => String::new(),
        };
        let style = match get(&["style"]) { Some(J::Str(s)) if !s.is_empty() => s.clone(), _ => "Default".into() };
        let mut ev = Event::new(s, e, &style, &text);
        if let Some(J::Str(a)) = get(&["actor", "speaker"]) { ev.actor = a.clone(); }
        evs.push(ev);
    }
    script_with(evs)
}

pub fn write_json(sc: &Script) -> String {
    let mut o = String::from("[\n");
    let evs: Vec<&Event> = writable(sc).collect();
    for (i, e) in evs.iter().enumerate() {
        o.push_str(&format!(
            "  {{\"start\": {}, \"end\": {}, \"text\": {}",
            e.start,
            e.end,
            json_escape(&to_html(&e.text, false, false, "\n"))
        ));
        if e.style != "Default" && !e.style.is_empty() { o.push_str(&format!(", \"style\": {}", json_escape(&e.style))); }
        if !e.actor.is_empty() { o.push_str(&format!(", \"actor\": {}", json_escape(&e.actor))); }
        o.push('}');
        if i + 1 < evs.len() { o.push(','); }
        o.push('\n');
    }
    o.push_str("]\n");
    o
}

// ------------------------------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Script {
        script_with(vec![
            Event::new(1000, 2500, "Default", "{\\i1}Hello{\\i0}\\Nworld"),
            Event::new(3123, 4999, "Default", "Plain & <simple>"),
        ])
    }

    #[test]
    fn microdvd_roundtrip_exact_at_25fps() {
        let sc = sample();
        let s = write_microdvd(&sc, 25.0);
        assert!(s.starts_with("{25}{63}{y:i}Hello|world\n"), "{}", s);
        let back = parse_microdvd(&s, 25.0);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.events[0].start, 1000);
        assert_eq!(back.events[0].end, 2520); // frame 63 @25fps = 2520ms (round-to-frame)
        assert_eq!(back.events[0].text, "{\\i1}Hello{\\i0}\\Nworld");
    }

    #[test]
    fn microdvd_parse_styles_and_fps_header() {
        let s = "{1}{1}25.000\n{25}{50}{y:i}Hi|{y:b}There\n{100}{}Open\n";
        let sc = parse_microdvd(s, 23.976);
        assert_eq!(sc.events.len(), 2);
        assert_eq!(sc.events[0].start, 1000);
        assert_eq!(sc.events[0].end, 2000);
        assert_eq!(sc.events[0].text, "{\\i1}Hi{\\i0}\\N{\\b1}There{\\b0}");
        assert_eq!(sc.events[1].start, 4000);
        assert_eq!(sc.events[1].end, 6000);
        let c = parse_microdvd("{0}{10}{c:$0000FF}red", 25.0);
        assert_eq!(c.events[0].text, "{\\c&H0000FF&}red{\\c}");
        let w = write_microdvd(&c, 25.0);
        assert_eq!(w, "{0}{10}{c:$0000FF}red\n");
    }

    #[test]
    fn subviewer2_roundtrip() {
        let sc = sample();
        let s = write_subviewer2(&sc);
        assert!(s.contains("00:00:01.00,00:00:02.50\n<i>Hello</i>[br]world"));
        let back = parse_subviewer2(&s);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.events[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        assert_eq!(back.events[0].end, 2500);
        assert_eq!(back.events[1].start, 3120); // centisecond precision
    }

    #[test]
    fn sbv_exact_ms() {
        let sc = sample();
        let s = write_sbv(&sc);
        assert!(s.starts_with("0:00:01.000,0:00:02.500\nHello\nworld\n\n"));
        assert!(s.contains("0:00:03.123,0:00:04.999"));
        let back = parse_sbv(&s);
        assert_eq!(back.events[1].start, 3123);
        assert_eq!(back.events[1].end, 4999);
        assert_eq!(back.events[0].text, "Hello\\Nworld");
    }

    #[test]
    fn lrc_basic_and_roundtrip() {
        let s = "[ti:Song]\n[00:01.00]Hello\n[00:02.50]\n[00:03.12]World\n[00:05.123]last\n";
        let sc = parse_lrc(s);
        assert_eq!(sc.events.len(), 3);
        assert_eq!((sc.events[0].start, sc.events[0].end), (1000, 2500));
        assert_eq!((sc.events[1].start, sc.events[1].end), (3120, 5123));
        assert_eq!(sc.events[2].end, 8123);
        assert_eq!(sc.info_get("lrc_ti"), Some("Song"));
        let out = write_lrc(&sc, false);
        assert!(out.contains("[ti:Song]"));
        assert!(out.contains("[00:01.00]Hello\n[00:02.50]\n[00:03.12]World\n"));
        let back = parse_lrc(&out);
        assert_eq!(back.events[0].end, 2500);
    }

    #[test]
    fn lrc_multi_stamp_and_multiline() {
        let sc = parse_lrc("[00:01.00][00:10.00]Chorus\n");
        assert_eq!(sc.events.len(), 2);
        assert_eq!(sc.events[1].start, 10_000);
        let m = script_with(vec![Event::new(0, 1000, "Default", "a\\Nb")]);
        let out = write_lrc(&m, false);
        assert_eq!(parse_lrc(&out).events[0].text, "a\\Nb");
    }

    #[test]
    fn lrc_word_level() {
        let s = "[00:01.00]<00:01.00>Hel<00:01.50>lo <00:02.00>you\n[00:03.00]\n";
        let sc = parse_lrc(s);
        assert_eq!(sc.events[0].text, "{\\k50}Hel{\\k50}lo {\\k100}you");
        assert_eq!(sc.events[0].end, 3000);
        let out = write_lrc(&sc, true);
        assert!(out.contains("[00:01.00]<00:01.00>Hel<00:01.50>lo <00:02.00>you"));
        let flat = write_lrc(&sc, false);
        assert!(flat.contains("[00:01.00]Hello you"));
    }

    #[test]
    fn text_import() {
        let sc = parse_text("one\ntwo\n\nthree\n");
        assert_eq!(sc.events.len(), 2);
        assert_eq!(sc.events[0].text, "one\\Ntwo");
        assert_eq!(sc.events[1].start, sc.events[0].end);
        let sc = parse_text("a\nb\nc");
        assert_eq!(sc.events.len(), 3);
        let o = TextImport { per_line: true, duration_ms: Some(2000), gap_ms: 100, start_ms: 500 };
        let sc = parse_text_opts("x\n\ny", &o);
        assert_eq!((sc.events[1].start, sc.events[1].end), (2600, 4600));
        assert_eq!(write_text(&sample()), "Hello\nworld\n\nPlain & <simple>\n\n");
    }

    #[test]
    fn csv_roundtrip_quotes_and_newlines() {
        let mut sc = sample();
        sc.events[1].text = "He said \"hi\", ok".into();
        let o = CsvOptions::default();
        let s = write_csv(&sc, &o);
        assert!(s.starts_with("Number,Start,End,Text\n1,00:00:01,000,00:00:02,500,\"<i>Hello</i>\nworld\"\n"), "{}", s);
        // the SRT time contains a comma, so use another delimiter for a clean roundtrip
        let o2 = CsvOptions { delimiter: ';', ..CsvOptions::default() };
        let s = write_csv(&sc, &o2);
        let back = parse_csv(&s, &o2);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.events[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        assert_eq!(back.events[1].text, "He said \"hi\", ok");
        assert_eq!(back.events[1].start, 3123);
    }

    #[test]
    fn csv_millis_and_header_mapping() {
        let o = CsvOptions { time: CsvTime::Millis, columns: vec![CsvColumn::Start, CsvColumn::End, CsvColumn::Text], ..CsvOptions::default() };
        let s = write_csv(&sample(), &o);
        assert!(s.starts_with("Start,End,Text\n1000,2500,"));
        let back = parse_csv(&s, &o);
        assert_eq!(back.events[1].end, 4999);
        // header decides the column order
        let s = "Text,End,Start\nhello,2.5,1\n";
        let o = CsvOptions { time: CsvTime::Seconds, ..CsvOptions::default() };
        let sc = parse_csv(s, &o);
        assert_eq!((sc.events[0].start, sc.events[0].end, sc.events[0].text.as_str()), (1000, 2500, "hello"));
        let o = CsvOptions { header: false, time: CsvTime::Seconds, columns: vec![CsvColumn::Start, CsvColumn::Duration, CsvColumn::Text], ..CsvOptions::default() };
        let sc = parse_csv("1.0,0.5,x\n", &o);
        assert_eq!(sc.events[0].end, 1500);
    }

    #[test]
    fn ttml_roundtrip_with_formatting() {
        let sc = script_with(vec![
            Event::new(1000, 2500, "Default", "{\\i1}Hello{\\i0}\\N{\\b1}wor{\\c&H0000FF&}ld{\\r} & <co>"),
            Event::new(3000, 4001, "Default", "{\\u1}under{\\u0}"),
        ]);
        let s = write_ttml(&sc);
        assert!(s.contains("<p begin=\"00:00:01.000\" end=\"00:00:02.500\"><span tts:fontStyle=\"italic\">Hello</span><br/>"), "{}", s);
        assert!(s.contains("tts:color=\"#FF0000\""));
        assert!(s.contains("&amp; &lt;co&gt;"));
        assert!(s.contains("tts:textDecoration=\"underline\""));
        let back = parse_ttml(&s);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.events[0].start, 1000);
        assert_eq!(back.events[1].end, 4001);
        assert_eq!(back.events[0].text, "{\\i1}Hello{\\i0}\\N{\\b1}wor{\\b0}{\\b1}{\\c&H0000FF&}ld{\\c}{\\b0} & <co>");
        assert_eq!(back.events[1].text, "{\\u1}under{\\u0}");
    }

    #[test]
    fn ttml_parse_external_styles_and_time_forms() {
        let x = r#"<?xml version="1.0"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tts="http://www.w3.org/ns/ttml#styling" xmlns:ttp="http://www.w3.org/ns/ttml#parameter" ttp:frameRate="25">
<head><styling><style xml:id="it" tts:fontStyle="italic"/></styling></head>
<body><div>
<p begin="00:00:01:12" end="2.5s">A
  <span style="it">B</span><br/>C</p>
<p begin="3000ms" dur="1s" tts:fontWeight="bold">D</p>
<p begin="10000000t" end="20000000t">E</p>
</div></body></tt>"#;
        let sc = parse_ttml(x);
        assert_eq!(sc.events.len(), 3);
        assert_eq!(sc.events[0].start, 1480);
        assert_eq!(sc.events[0].end, 2500);
        assert_eq!(sc.events[0].text, "A {\\i1}B{\\i0}\\NC");
        assert_eq!((sc.events[1].start, sc.events[1].end), (3000, 4000));
        assert_eq!(sc.events[1].text, "{\\b1}D{\\b0}");
        assert_eq!((sc.events[2].start, sc.events[2].end), (1000, 2000));
    }

    #[test]
    fn sami_roundtrip() {
        let sc = sample();
        let s = write_sami(&sc);
        assert!(s.contains("<SYNC Start=1000><P Class=ENCC><i>Hello</i><br>world"));
        assert!(s.contains("<SYNC Start=2500><P Class=ENCC>&nbsp;"));
        let back = parse_sami(&s);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.events[0].end, 2500);
        assert_eq!(back.events[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        assert_eq!(back.events[1].text, "Plain & <simple>");
        assert_eq!(back.events[1].start, 3123);
    }

    #[test]
    fn sami_multi_language_keeps_first_class() {
        let s = "<SAMI><BODY><SYNC Start=0><P Class=ENCC>Hi<P Class=KRCC>Annyeong\n<SYNC Start=1000><P Class=ENCC>&nbsp;\n</BODY></SAMI>";
        let sc = parse_sami(s);
        assert_eq!(sc.events.len(), 1);
        assert_eq!(sc.events[0].text, "Hi");
        assert_eq!(sc.events[0].end, 1000);
    }

    #[test]
    fn json_roundtrip_and_tolerance() {
        let mut sc = sample();
        sc.events[1].text = "quote \"x\" \\ tab\there \u{1F600} ü".into();
        sc.events[1].actor = "Bob".into();
        let s = write_json(&sc);
        assert!(s.contains("\"start\": 1000, \"end\": 2500, \"text\": \"<i>Hello</i>\\nworld\""));
        let back = parse_json(&s);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.events[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        assert_eq!(back.events[1].text, sc.events[1].text);
        assert_eq!(back.events[1].actor, "Bob");
        assert_eq!(back.events[1].start, 3123);
        let t = parse_json(r#"{"segments":[{"start":1.5,"end":"00:00:03.000","text":"a\ud83d\ude00"},{"start":5}]}"#);
        assert_eq!(t.events.len(), 1);
        assert_eq!((t.events[0].start, t.events[0].end), (1500, 3000));
        assert_eq!(t.events[0].text, "a\u{1F600}");
        assert!(parse_json("not json").events.is_empty());
    }

    #[test]
    fn detection() {
        assert_eq!(detect("a.srt", ""), Some(Format::Srt));
        assert_eq!(detect("a.sub", "[INFORMATION]\n"), Some(Format::SubViewer2));
        assert_eq!(detect("a.sub", "{1}{2}x"), Some(Format::MicroDvd));
        assert_eq!(detect("x", "WEBVTT\n\n"), Some(Format::Vtt));
        assert_eq!(detect("x", "[Script Info]\n"), Some(Format::Ass));
        assert_eq!(detect("x", "{10}{20}text"), Some(Format::MicroDvd));
        assert_eq!(detect("x", "0:00:01.000,0:00:02.000\nhi"), Some(Format::Sbv));
        assert_eq!(detect("x", "[00:01.00]hi"), Some(Format::Lrc));
        assert_eq!(detect("x", "<SAMI><BODY>"), Some(Format::Sami));
        assert_eq!(detect("x.xml", "<tt xmlns=\"a\">"), Some(Format::Ttml));
        assert_eq!(detect("x", "[{\"start\":1,\"end\":2,\"text\":\"a\"}]"), Some(Format::Json));
        assert_eq!(detect("x.txt", "hello"), Some(Format::Text));
        assert_eq!(detect("x.bin", "random"), None);
        assert_eq!(detect("x", "1\n00:00:01,000 --> 00:00:02,000\nhi"), Some(Format::Srt));
    }

    #[test]
    fn dispatch_roundtrips_all() {
        let sc = sample();
        let opts = FormatOptions { fps: 25.0, ..Default::default() };
        for f in Format::ALL {
            if matches!(f, Format::Text) { continue; }
            let out = save_any(f, &sc, &opts);
            let name = format!("file.{}", f.extension());
            let back = load_any(&name, &out, &opts).unwrap_or_else(|| panic!("{:?} not detected", f));
            assert_eq!(back.events.len(), 2, "{:?}\n{}", f, out);
            assert_eq!(back.events[0].start, 1000, "{:?}", f);
        }
        let t = load_any("a.txt", &save_any(Format::Text, &sc, &opts), &opts).unwrap();
        assert_eq!(t.events.len(), 2);
    }

    #[test]
    fn html_helpers() {
        assert_eq!(to_html("{\\b1}a{\\i1}b{\\b0\\i0}c", false, false, "\n"), "<b>a</b><b><i>b</i></b>c");
        assert_eq!(from_html("<font color=\"#FF0000\">x</font><br>y"), "{\\c&H0000FF&}x{\\c}\\Ny");
        assert_eq!(decode_entities("a&amp;b&#65;&#x42;&nbsp;"), "a&bAB\u{a0}");
        assert_eq!(hms(59_999, 2, 2, '.'), "00:01:00.00");
    }
}
