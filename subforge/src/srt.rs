//! SRT / WebVTT import and export.

use crate::ass::{Document, Entry, Event};
use crate::tags::{split_blocks, Seg};
use crate::time::{fmt_srt_time, parse_srt_time};
use regex::Regex;

fn html_to_ass(s: &str) -> String {
    let mut t = s.to_string();
    let rep = [
        (r"(?i)<\s*i\s*>", "{\\i1}"),
        (r"(?i)<\s*/\s*i\s*>", "{\\i0}"),
        (r"(?i)<\s*b\s*>", "{\\b1}"),
        (r"(?i)<\s*/\s*b\s*>", "{\\b0}"),
        (r"(?i)<\s*u\s*>", "{\\u1}"),
        (r"(?i)<\s*/\s*u\s*>", "{\\u0}"),
        (r"(?i)<\s*s\s*>", "{\\s1}"),
        (r"(?i)<\s*/\s*s\s*>", "{\\s0}"),
    ];
    for (p, r) in rep {
        t = Regex::new(p).unwrap().replace_all(&t, r).to_string();
    }
    // <font color="#rrggbb">
    let font = Regex::new(r#"(?i)<\s*font[^>]*color\s*=\s*["']?#?([0-9a-fA-F]{6})["']?[^>]*>"#).unwrap();
    t = font
        .replace_all(&t, |c: &regex::Captures| {
            let h = &c[1];
            format!("{{\\c&H{}{}{}&}}", &h[4..6], &h[2..4], &h[0..2]).to_uppercase().replace("\\C", "\\c")
        })
        .to_string();
    t = Regex::new(r"(?i)<\s*/\s*font\s*>").unwrap().replace_all(&t, "{\\c}").to_string();
    // strip any other html-ish tags (<c.class>, <00:00:01.000>, <v name>)
    t = Regex::new(r"</?[a-zA-Z][^>]*>|<\d{2}:\d{2}[^>]*>").unwrap().replace_all(&t, "").to_string();
    t = t.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", "\\h");
    t.replace("\r\n", "\\N").replace('\n', "\\N")
}

pub fn parse_srt(src: &str) -> Document {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src).replace("\r\n", "\n");
    let mut doc = Document::new_default();
    let arrow = Regex::new(r"^\s*(\S+)\s*-->\s*(\S+)").unwrap();
    let mut lines = src.lines().peekable();
    while let Some(l) = lines.next() {
        let l = l.trim();
        if l.is_empty() || l.starts_with("WEBVTT") || l.starts_with("NOTE") || l.starts_with("STYLE") {
            continue;
        }
        let tl = if arrow.is_match(l) {
            l.to_string()
        } else if l.chars().all(|c| c.is_ascii_digit()) {
            match lines.peek() {
                Some(n) if arrow.is_match(n) => lines.next().unwrap().trim().to_string(),
                _ => continue,
            }
        } else if lines.peek().map(|n| arrow.is_match(n)).unwrap_or(false) {
            // VTT cue identifier
            lines.next().unwrap().trim().to_string()
        } else {
            continue;
        };
        let Some(c) = arrow.captures(&tl) else { continue };
        let (Some(s), Some(e)) = (parse_srt_time(&c[1]), parse_srt_time(&c[2])) else { continue };
        let mut body: Vec<&str> = Vec::new();
        while let Some(n) = lines.peek() {
            if n.trim().is_empty() {
                break;
            }
            body.push(lines.next().unwrap());
        }
        let text = html_to_ass(&body.join("\n"));
        doc.entries.push(Entry::Event(Event { start: s, end: e, text, ..Event::default() }));
    }
    doc
}

fn ass_to_html(text: &str) -> String {
    let mut out = String::new();
    let (mut i, mut b, mut u) = (false, false, false);
    for seg in split_blocks(text) {
        match seg {
            Seg::Text(t) => out.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            Seg::Tags(t) => {
                for (tag, flag, open, close) in [("i", &mut i, "<i>", "</i>"), ("b", &mut b, "<b>", "</b>"), ("u", &mut u, "<u>", "</u>")] {
                    let re = Regex::new(&format!(r"\\{}([01])", tag)).unwrap();
                    if let Some(m) = re.captures_iter(t).last() {
                        let on = &m[1] == "1";
                        if on && !*flag {
                            out.push_str(open);
                        } else if !on && *flag {
                            out.push_str(close);
                        }
                        *flag = on;
                    }
                }
            }
        }
    }
    if u { out.push_str("</u>"); }
    if b { out.push_str("</b>"); }
    if i { out.push_str("</i>"); }
    out
}

pub fn to_srt(doc: &Document) -> String {
    let mut out = String::new();
    let mut n = 0;
    for e in doc.events().filter(|e| !e.comment) {
        let body = ass_to_html(&e.text);
        if body.trim().is_empty() {
            continue;
        }
        n += 1;
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n, fmt_srt_time(e.start, ','), fmt_srt_time(e.end, ','), body));
    }
    out
}

pub fn to_vtt(doc: &Document) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for e in doc.events().filter(|e| !e.comment) {
        let body = ass_to_html(&e.text);
        if body.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("{} --> {}\n{}\n\n", fmt_srt_time(e.start, '.'), fmt_srt_time(e.end, '.'), body));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let d = parse_srt("1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hello</i>\r\nworld\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nBye\r\n");
        let ev: Vec<_> = d.events().collect();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        let s = to_srt(&d);
        assert!(s.contains("<i>Hello</i>\nworld"));
        assert!(s.contains("00:00:01,000 --> 00:00:02,500"));
        let v = to_vtt(&d);
        assert!(v.starts_with("WEBVTT") && v.contains("00:00:03.000 --> 00:00:04.000"));
        let d2 = parse_srt(&v);
        assert_eq!(d2.events().count(), 2);
    }
}
