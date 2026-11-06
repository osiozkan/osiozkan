//! SRT / WebVTT okuma-yazma ve ASS'e dönüştürme.
use crate::ass::{Document, Event};
use crate::time;

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub start: i64,
    pub end: i64,
    pub text: String,
}

pub fn parse_srt(input: &str) -> Vec<Cue> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = vec![];
    for block in input.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty() || false).collect();
        if lines.is_empty() { continue; }
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = lines[ti].split_once("-->").unwrap();
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (time::parse_srt(a), time::parse_srt(b)) else { continue };
        let text = lines[ti + 1..].join("\\N");
        cues.push(Cue { start, end, text });
    }
    cues
}

pub fn parse_vtt(input: &str) -> Vec<Cue> {
    parse_srt(input.trim_start_matches("WEBVTT"))
}

/// HTML-benzeri SRT etiketlerini ASS'e çevirir.
pub fn html_to_ass(s: &str) -> String {
    let mut o = s.to_string();
    for (a, b) in [("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"), ("<u>", "{\\u1}"), ("</u>", "{\\u0}"), ("<I>", "{\\i1}"), ("</I>", "{\\i0}"), ("<B>", "{\\b1}"), ("</B>", "{\\b0}")] {
        o = o.replace(a, b);
    }
    // kalan etiketleri (font vb.) at
    let mut out = String::new();
    let mut in_tag = false;
    for c in o.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn to_document(cues: &[Cue], play_res: (i32, i32)) -> Document {
    let mut d = Document::new();
    d.info_set("PlayResX", &play_res.0.to_string());
    d.info_set("PlayResY", &play_res.1.to_string());
    for c in cues {
        d.events.push(Event { start: c.start, end: c.end, text: html_to_ass(&c.text), ..Event::default() });
    }
    d
}

pub fn from_document(doc: &Document) -> Vec<Cue> {
    doc.events
        .iter()
        .filter(|e| !e.is_comment() && !e.text.trim().is_empty())
        .map(|e| {
            let mut t = String::new();
            for p in crate::tags::split(&e.text) {
                match p {
                    crate::tags::Part::Tags(b) => {
                        for (n, a) in crate::tags::parse_block(b) {
                            match (n.as_str(), a.as_str()) {
                                ("i", "1") => t.push_str("<i>"),
                                ("i", "0") => t.push_str("</i>"),
                                ("b", "1") => t.push_str("<b>"),
                                ("b", "0") => t.push_str("</b>"),
                                ("u", "1") => t.push_str("<u>"),
                                ("u", "0") => t.push_str("</u>"),
                                _ => {}
                            }
                        }
                    }
                    crate::tags::Part::Text(x) => t.push_str(&x.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ")),
                }
            }
            Cue { start: e.start, end: e.end, text: t }
        })
        .collect()
}

pub fn write_srt(cues: &[Cue]) -> String {
    let mut o = String::new();
    for (i, c) in cues.iter().enumerate() {
        o.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, time::format_srt(c.start), time::format_srt(c.end), c.text));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let src = "1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Selam</i>\r\nDünya\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nİki\r\n";
        let cues = parse_srt(src);
        assert_eq!(cues.len(), 2);
        let doc = to_document(&cues, (1920, 1080));
        assert_eq!(doc.events[0].text, "{\\i1}Selam{\\i0}\\NDünya");
        let back = from_document(&doc);
        assert_eq!(back[0].text, "<i>Selam</i>\nDünya");
        assert!(write_srt(&back).contains("00:00:01,000 --> 00:00:02,500"));
    }
}
