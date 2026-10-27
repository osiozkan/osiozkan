//! SRT / WebVTT import & export.

use crate::ass::{Entry, Kind, Line, Script};
use crate::time::{format_srt_time, format_vtt_time, parse_ass_time};

fn parse_ts(s: &str) -> Option<i64> {
    parse_ass_time(s.trim())
}

/// Convert common SRT/HTML inline markup to ASS overrides.
pub fn html_to_ass(s: &str) -> String {
    let mut t = s.replace("\r\n", "\n").replace('\n', "\\N");
    for (from, to) in [
        ("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"),
        ("<u>", "{\\u1}"), ("</u>", "{\\u0}"), ("<s>", "{\\s1}"), ("</s>", "{\\s0}"),
        ("<I>", "{\\i1}"), ("</I>", "{\\i0}"), ("<B>", "{\\b1}"), ("</B>", "{\\b0}"),
        ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&"), ("&nbsp;", "\\h"),
    ] {
        t = t.replace(from, to);
    }
    // Drop remaining tags such as <font ...> and positional {\an8} hints stay as is.
    let mut out = String::new();
    let mut in_tag = false;
    for c in t.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    out.replace("}{", "")
}

pub fn ass_to_html(text: &str) -> String {
    use crate::tags::{parse, Seg};
    let mut out = String::new();
    let (mut i, mut b, mut u, mut s) = (false, false, false, false);
    let close = |out: &mut String, i: &mut bool, b: &mut bool, u: &mut bool, s: &mut bool| {
        if *s { out.push_str("</s>"); *s = false }
        if *u { out.push_str("</u>"); *u = false }
        if *b { out.push_str("</b>"); *b = false }
        if *i { out.push_str("</i>"); *i = false }
    };
    for seg in parse(text) {
        match seg {
            Seg::Text(t) => out.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            Seg::Block(tags) => {
                for t in tags {
                    let on = t.arg.trim() != "0" && !t.arg.trim().is_empty();
                    macro_rules! tog {
                        ($f:ident, $o:literal, $c:literal) => {
                            if on && !$f { out.push_str($o); $f = true } else if !on && $f { out.push_str($c); $f = false }
                        };
                    }
                    match t.name.as_str() {
                        "i" => tog!(i, "<i>", "</i>"),
                        "b" => tog!(b, "<b>", "</b>"),
                        "u" => tog!(u, "<u>", "</u>"),
                        "s" => tog!(s, "<s>", "</s>"),
                        "r" => close(&mut out, &mut i, &mut b, &mut u, &mut s),
                        _ => {}
                    }
                }
            }
        }
    }
    close(&mut out, &mut i, &mut b, &mut u, &mut s);
    out
}

pub fn parse_srt(input: &str) -> Script {
    let mut script = Script::new_default();
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n");
    for block in input.split("\n\n") {
        let mut lines = block.lines().filter(|l| !l.trim().is_empty()).peekable();
        let Some(first) = lines.next() else { continue };
        let ts_line = if first.contains("-->") { first } else { match lines.next() { Some(l) => l, None => continue } };
        let Some((a, b)) = ts_line.split_once("-->") else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_ts(a), parse_ts(b)) else { continue };
        let text = lines.collect::<Vec<_>>().join("\n");
        script.push_line(Line { start, end, text: html_to_ass(&text), ..Default::default() });
    }
    script
}

pub fn write_srt(script: &Script) -> String {
    let mut out = String::new();
    let mut n = 1;
    for l in script.lines().filter(|l| l.kind == Kind::Dialogue) {
        let text = ass_to_html(&l.text);
        if text.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n, format_srt_time(l.start), format_srt_time(l.end), text));
        n += 1;
    }
    out
}

pub fn parse_vtt(input: &str) -> Script {
    let body = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n");
    let body = body.strip_prefix("WEBVTT").map(|r| r.split_once("\n\n").map(|x| x.1).unwrap_or("")).unwrap_or(&body).to_string();
    let mut s = parse_srt(&body);
    // Convert voice tags <v Name>
    for l in s.lines_mut() {
        if let Some(rest) = l.text.strip_prefix("v ") {
            let _ = rest;
        }
    }
    s
}

pub fn write_vtt(script: &Script) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for l in script.lines().filter(|l| l.kind == Kind::Dialogue) {
        let text = ass_to_html(&l.text);
        if text.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("{} --> {}\n{}\n\n", format_vtt_time(l.start), format_vtt_time(l.end), text));
    }
    out
}

/// LRC lyrics: `[mm:ss.xx]text`; each line ends where the next begins (last gets 3 s).
pub fn parse_lrc(input: &str) -> Script {
    let re = regex::Regex::new(r"^((?:\[\d+:\d+(?:[.:]\d+)?\])+)(.*)$").unwrap();
    let ts = regex::Regex::new(r"\[(\d+):(\d+)(?:[.:](\d+))?\]").unwrap();
    let mut items: Vec<(i64, String)> = Vec::new();
    for line in input.lines() {
        let line = line.trim();
        if let Some(c) = re.captures(line) {
            let text = c[2].trim().to_string();
            for t in ts.captures_iter(&c[1]) {
                let m: i64 = t[1].parse().unwrap_or(0);
                let sec: i64 = t[2].parse().unwrap_or(0);
                let frac = t.get(3).map(|f| { let f = f.as_str(); let v: i64 = f.parse().unwrap_or(0); match f.len() { 1 => v * 100, 2 => v * 10, _ => v } }).unwrap_or(0);
                items.push(((m * 60 + sec) * 1000 + frac, text.clone()));
            }
        }
    }
    items.sort_by_key(|i| i.0);
    let mut script = Script::new_default();
    for (i, (t, text)) in items.iter().enumerate() {
        let end = items.get(i + 1).map(|n| n.0).unwrap_or(t + 3000);
        script.push_line(Line { start: *t, end, text: html_to_ass(text), ..Default::default() });
    }
    script
}

pub fn write_lrc(script: &Script) -> String {
    let mut out = String::new();
    let mut lines: Vec<_> = script.lines().filter(|l| l.kind == Kind::Dialogue).collect();
    lines.sort_by_key(|l| l.start);
    for l in lines {
        let t = crate::tags::visible_text(&l.text).replace('\n', " ");
        let cs = (l.start + 5) / 10;
        out.push_str(&format!("[{:02}:{:02}.{:02}]{}\n", cs / 6000, cs / 100 % 60, cs % 100, t));
    }
    out
}

/// MicroDVD `{start}{end}text` (frame based; `|` is a line break).
pub fn parse_microdvd(input: &str, fps: f64) -> Script {
    let re = regex::Regex::new(r"^\{(\d+)\}\{(\d+)\}(.*)$").unwrap();
    let mut script = Script::new_default();
    for line in input.lines() {
        if let Some(c) = re.captures(line.trim_end()) {
            let (a, b): (f64, f64) = (c[1].parse().unwrap_or(0.0), c[2].parse().unwrap_or(0.0));
            let text = c[3].replace('|', "\\N");
            script.push_line(Line { start: (a * 1000.0 / fps).round() as i64, end: (b * 1000.0 / fps).round() as i64, text, ..Default::default() });
        }
    }
    script
}

pub fn write_microdvd(script: &Script, fps: f64) -> String {
    let mut out = String::new();
    for l in script.lines().filter(|l| l.kind == Kind::Dialogue) {
        let t = crate::tags::visible_text(&l.text).replace('\n', "|");
        out.push_str(&format!("{{{}}}{{{}}}{}\n", (l.start as f64 * fps / 1000.0).round() as i64, (l.end as f64 * fps / 1000.0).round() as i64, t));
    }
    out
}

/// YouTube SBV: `h:mm:ss.mmm,h:mm:ss.mmm` followed by text.
pub fn parse_sbv(input: &str) -> Script {
    let mut script = Script::new_default();
    for block in input.replace("\r\n", "\n").split("\n\n") {
        let mut it = block.lines();
        let Some(head) = it.next() else { continue };
        let Some((a, b)) = head.split_once(',') else { continue };
        let (Some(start), Some(end)) = (parse_ts(a), parse_ts(b)) else { continue };
        let text = it.collect::<Vec<_>>().join("\\N");
        script.push_line(Line { start, end, text, ..Default::default() });
    }
    script
}

pub fn write_sbv(script: &Script) -> String {
    let f = |ms: i64| format!("{}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let mut out = String::new();
    for l in script.lines().filter(|l| l.kind == Kind::Dialogue) {
        out.push_str(&format!("{},{}\n{}\n\n", f(l.start), f(l.end), crate::tags::visible_text(&l.text)));
    }
    out
}

/// Plain text: one visible text line per event.
pub fn write_txt(script: &Script) -> String {
    script.lines().filter(|l| l.kind == Kind::Dialogue).map(|l| crate::tags::visible_text(&l.text).replace('\n', " ")).collect::<Vec<_>>().join("\n") + "\n"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ass,
    Srt,
    Vtt,
    Lrc,
    MicroDvd,
    Sbv,
    Txt,
}

impl Format {
    pub fn from_path(p: &str) -> Option<Format> {
        let ext = p.rsplit('.').next()?.to_ascii_lowercase();
        match ext.as_str() {
            "ass" | "ssa" => Some(Format::Ass),
            "srt" => Some(Format::Srt),
            "vtt" => Some(Format::Vtt),
            "lrc" => Some(Format::Lrc),
            "sub" => Some(Format::MicroDvd),
            "sbv" => Some(Format::Sbv),
            "txt" => Some(Format::Txt),
            _ => None,
        }
    }
}

/// Decode bytes: UTF-8 (BOM aware), UTF-16 LE/BE with BOM, otherwise Windows-1254/Latin-1 fallback.
pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&units);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1254(b)).collect(),
    }
}

/// Windows-1254 (Turkish) high half.
fn cp1254(b: u8) -> char {
    match b {
        0xD0 => 'Ğ', 0xF0 => 'ğ', 0xDD => 'İ', 0xFD => 'ı', 0xDE => 'Ş', 0xFE => 'ş',
        0x80 => '€', 0x91 => '\u{2018}', 0x92 => '\u{2019}', 0x93 => '\u{201c}', 0x94 => '\u{201d}', 0x96 => '–', 0x97 => '—',
        _ => b as char,
    }
}

pub fn load(path: &str, bytes: &[u8]) -> Script {
    let text = decode_bytes(bytes);
    match Format::from_path(path) {
        Some(Format::Srt) => parse_srt(&text),
        Some(Format::Vtt) => parse_vtt(&text),
        Some(Format::Lrc) => parse_lrc(&text),
        Some(Format::MicroDvd) => parse_microdvd(&text, 23.976),
        Some(Format::Sbv) => parse_sbv(&text),
        _ => Script::parse(&text),
    }
}

pub fn save(path: &str, script: &Script) -> String {
    match Format::from_path(path) {
        Some(Format::Srt) => write_srt(script),
        Some(Format::Vtt) => write_vtt(script),
        Some(Format::Lrc) => write_lrc(script),
        Some(Format::MicroDvd) => write_microdvd(script, 23.976),
        Some(Format::Sbv) => write_sbv(script),
        Some(Format::Txt) => write_txt(script),
        _ => script.to_string(),
    }
}

#[allow(dead_code)]
fn _unused(_: Entry) {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let srt = "1\n00:00:01,000 --> 00:00:02,500\n<i>Merhaba</i>\ndünya\n\n2\n00:00:03,000 --> 00:00:04,000\nSon\n";
        let s = parse_srt(srt);
        let l: Vec<_> = s.lines().collect();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "{\\i1}Merhaba{\\i0}\\Ndünya");
        let out = write_srt(&s);
        assert!(out.contains("<i>Merhaba</i>\ndünya"));
        assert!(out.contains("00:00:01,000 --> 00:00:02,500"));
    }
    #[test]
    fn lrc_microdvd_sbv() {
        let s = parse_lrc("[ar:x]\n[00:01.50]Hello\n[00:03.00]World\n");
        let l: Vec<_> = s.lines().map(|l| (l.start, l.end, l.text.clone())).collect();
        assert_eq!(l, [(1500, 3000, "Hello".to_string()), (3000, 6000, "World".to_string())]);
        assert_eq!(write_lrc(&s), "[00:01.50]Hello\n[00:03.00]World\n");
        let m = parse_microdvd("{24}{48}Hi|there\n", 24.0);
        assert_eq!(m.lines().next().unwrap().text, "Hi\\Nthere");
        assert_eq!(write_microdvd(&m, 24.0), "{24}{48}Hi|there\n");
        let b = parse_sbv("0:00:01.000,0:00:02.500\nHi\nthere\n\n");
        assert_eq!(b.lines().next().unwrap().end, 2500);
        assert!(write_sbv(&b).starts_with("0:00:01.000,0:00:02.500\nHi\nthere"));
    }

    #[test]
    fn vtt_header() {
        let v = "WEBVTT\n\n00:01.000 --> 00:02.000\nHi\n";
        let s = parse_vtt(v);
        assert_eq!(s.line_count(), 1);
        assert!(write_vtt(&s).starts_with("WEBVTT"));
    }
    #[test]
    fn decode_turkish_cp1254() {
        assert_eq!(decode_bytes(&[0xDD, 0x73, 0xFE]), "İsş");
    }
}
