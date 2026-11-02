//! SRT and WebVTT import/export (converted to/from ASS events).

use crate::ass::{Entry, Event, Script};
use crate::tags::{split_blocks, Block};
use crate::time::{format_srt_time, parse_srt_time};

/// Convert HTML-ish SRT markup to ASS override tags.
pub fn html_to_ass(s: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let b = s.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            if let Some(end) = s[i..].find('>') {
                let tag = s[i + 1..i + end].trim().to_ascii_lowercase();
                let rep = match tag.as_str() {
                    "i" => Some("{\\i1}".to_string()),
                    "/i" => Some("{\\i0}".to_string()),
                    "b" => Some("{\\b1}".to_string()),
                    "/b" => Some("{\\b0}".to_string()),
                    "u" => Some("{\\u1}".to_string()),
                    "/u" => Some("{\\u0}".to_string()),
                    "s" => Some("{\\s1}".to_string()),
                    "/s" => Some("{\\s0}".to_string()),
                    "/font" => Some("{\\c}".to_string()),
                    t if t.starts_with("font") => font_color(t).map(|c| format!("{{\\c{c}}}")).or(Some(String::new())),
                    t if t.starts_with('/') || t.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == 'c') => {
                        Some(String::new()) // other known markup (<c.x>, </c>, <v ..>) dropped
                    }
                    _ => None,
                };
                if let Some(r) = rep {
                    out.push_str(&r);
                    i += end + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn font_color(tag: &str) -> Option<String> {
    let p = tag.find("color")?;
    let rest = tag[p + 5..].trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let hex = rest.strip_prefix('#')?;
    let hex: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 6 {
        return None;
    }
    // RRGGBB -> &HBBGGRR&
    Some(format!("&H{}{}{}&", &hex[4..6], &hex[2..4], &hex[0..2]).to_uppercase())
}

fn parse_cues(input: &str, vtt: bool) -> Script {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n");
    let mut sc = Script::new_default();
    let mut ev_added = false;
    for block in input.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty() || false).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = lines[ti].split_once("-->").unwrap();
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_srt_time(a), parse_srt_time(b)) else { continue };
        let text = lines[ti + 1..].iter().map(|l| html_to_ass(l)).collect::<Vec<_>>().join("\\N");
        let _ = vtt;
        sc.entries.push(Entry::Dialogue(Event { start, end, text, ..Default::default() }));
        ev_added = true;
    }
    let _ = ev_added;
    sc
}

pub fn parse_srt(input: &str) -> Script {
    parse_cues(input, false)
}
pub fn parse_vtt(input: &str) -> Script {
    parse_cues(input, true)
}

/// ASS override tags -> HTML-ish markup (only i/b/u/s and primary colour survive).
pub fn ass_to_html(text: &str) -> String {
    let mut out = String::new();
    let (mut i, mut b, mut u, mut s) = (false, false, false, false);
    let mut color = false;
    for blk in split_blocks(text) {
        match blk {
            Block::Text(t) => out.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            Block::Override(o) => {
                for tag in o.split('\\').skip(1) {
                    let flag = |name: &str| tag.strip_prefix(name).and_then(|r| match r.trim() { "1" => Some(true), "0" => Some(false), _ => None });
                    for (name, st, open, close) in [("i", &mut i, "<i>", "</i>"), ("b", &mut b, "<b>", "</b>"), ("u", &mut u, "<u>", "</u>"), ("s", &mut s, "<s>", "</s>")] {
                        if let Some(v) = flag(name) {
                            if v && !*st { out.push_str(open); *st = true; }
                            if !v && *st { out.push_str(close); *st = false; }
                        }
                    }
                    if let Some(c) = tag.strip_prefix("1c").or_else(|| tag.strip_prefix('c')) {
                        if color { out.push_str("</font>"); color = false; }
                        let h = c.trim_start_matches("&H").trim_end_matches('&');
                        if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
                            out.push_str(&format!("<font color=\"#{}{}{}\">", &h[4..6], &h[2..4], &h[0..2]));
                            color = true;
                        }
                    }
                }
            }
        }
    }
    if color { out.push_str("</font>"); }
    if s { out.push_str("</s>"); }
    if u { out.push_str("</u>"); }
    if b { out.push_str("</b>"); }
    if i { out.push_str("</i>"); }
    out
}

pub fn to_srt(sc: &Script) -> String {
    let mut out = String::new();
    for (n, e) in sc.events().filter(|e| !e.comment).enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n + 1, format_srt_time(e.start, ','), format_srt_time(e.end, ','), ass_to_html(&e.text)));
    }
    out
}

pub fn to_vtt(sc: &Script) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for e in sc.events().filter(|e| !e.comment) {
        out.push_str(&format!("{} --> {}\n{}\n\n", format_srt_time(e.start, '.'), format_srt_time(e.end, '.'), ass_to_html(&e.text)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let src = "1\n00:00:01,000 --> 00:00:02,500\n<i>Hello</i>\nworld\n\n2\n00:00:03,000 --> 00:00:04,000\nBye\n";
        let sc = parse_srt(src);
        let ev: Vec<_> = sc.events().collect();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        let out = to_srt(&sc);
        assert!(out.contains("<i>Hello</i>\nworld"));
        assert!(out.contains("00:00:01,000 --> 00:00:02,500"));
    }
    #[test]
    fn vtt() {
        let sc = parse_vtt("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\n<c.red>Hi</c>\n");
        assert_eq!(sc.events().next().unwrap().text, "Hi");
        assert!(to_vtt(&sc).starts_with("WEBVTT"));
    }
    #[test]
    fn font_color() {
        let sc = parse_srt("1\n00:00:00,000 --> 00:00:01,000\n<font color=\"#ff0000\">R</font>\n");
        assert_eq!(sc.events().next().unwrap().text, "{\\c&H0000FF&}R{\\c}");
    }
}
