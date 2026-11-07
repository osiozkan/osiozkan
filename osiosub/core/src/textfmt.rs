//! Import/export of SRT and WebVTT, with format auto-detection.
use crate::ass::{AssFile, Event};
use crate::time;

fn html_to_ass(s: &str) -> String {
    let mut t = s.replace("\r", "");
    for (a, b) in [("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"), ("<u>", "{\\u1}"), ("</u>", "{\\u0}"),
                   ("<I>", "{\\i1}"), ("</I>", "{\\i0}"), ("<B>", "{\\b1}"), ("</B>", "{\\b0}")] {
        t = t.replace(a, b);
    }
    // drop remaining tags e.g. <font ...>, <c.x>, <v Name>
    let re = regex::Regex::new(r"</?(font|c|v|ruby|rt|lang)[^>]*>").unwrap();
    t = re.replace_all(&t, "").into_owned();
    t = t.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", "\\h");
    t.trim_end().replace('\n', "\\N")
}

fn ass_to_html(s: &str) -> String {
    let mut t = s.to_string();
    for (a, b) in [("{\\i1}", "<i>"), ("{\\i0}", "</i>"), ("{\\b1}", "<b>"), ("{\\b0}", "</b>"), ("{\\u1}", "<u>"), ("{\\u0}", "</u>")] {
        t = t.replace(a, b);
    }
    let t = crate::tags::strip_tags(&t);
    t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

fn parse_cues(input: &str, vtt: bool) -> Vec<Event> {
    let input = input.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    for block in input.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.is_empty() || true).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = lines[ti].split_once("-->").unwrap();
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(s), Some(e)) = (time::parse_generic(a), time::parse_generic(b)) else { continue };
        let _ = vtt;
        let text = lines[ti + 1..].join("\n");
        out.push(Event { start: s, end: e, text: html_to_ass(&text), ..Default::default() });
    }
    out
}

pub fn parse_srt(input: &str) -> AssFile {
    let mut f = AssFile::new_default();
    f.events = parse_cues(input, false);
    f
}

pub fn parse_vtt(input: &str) -> AssFile {
    let mut f = AssFile::new_default();
    f.events = parse_cues(input, true);
    f
}

pub fn to_srt(f: &AssFile) -> String {
    let mut o = String::new();
    let mut evs: Vec<&Event> = f.events.iter().filter(|e| !e.comment).collect();
    evs.sort_by_key(|e| e.start);
    for (i, e) in evs.iter().enumerate() {
        o.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, time::fmt_srt(e.start), time::fmt_srt(e.end), ass_to_html(&e.text)));
    }
    o
}

pub fn to_vtt(f: &AssFile) -> String {
    let mut o = String::from("WEBVTT\n\n");
    let mut evs: Vec<&Event> = f.events.iter().filter(|e| !e.comment).collect();
    evs.sort_by_key(|e| e.start);
    for e in evs {
        o.push_str(&format!("{} --> {}\n{}\n\n", time::fmt_vtt(e.start), time::fmt_vtt(e.end), ass_to_html(&e.text)));
    }
    o
}

/// Plain text transcript (one line per event).
pub fn to_txt(f: &AssFile) -> String {
    f.events.iter().filter(|e| !e.comment).map(|e| crate::tags::visible_text(&e.text).replace('\n', " ")).collect::<Vec<_>>().join("\n")
}

/// Detect by content/extension and parse.
pub fn open_any(name: &str, content: &str) -> Result<AssFile, crate::ass::ParseError> {
    let lower = name.to_ascii_lowercase();
    let head = content.trim_start_matches('\u{feff}').trim_start();
    if lower.ends_with(".vtt") || head.starts_with("WEBVTT") { return Ok(parse_vtt(content)); }
    if lower.ends_with(".srt") { return Ok(parse_srt(content)); }
    if head.starts_with('[') { return AssFile::parse(content); }
    if content.contains("-->") { return Ok(parse_srt(content)); }
    AssFile::parse(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let s = "1\n00:00:01,000 --> 00:00:02,500\n<i>Selam</i>\nnaber\n\n2\n00:00:03,000 --> 00:00:04,000\nhoş\n";
        let f = parse_srt(s);
        assert_eq!(f.events.len(), 2);
        assert_eq!(f.events[0].text, "{\\i1}Selam{\\i0}\\Nnaber");
        let o = to_srt(&f);
        assert!(o.contains("<i>Selam</i>\nnaber"));
        assert_eq!(parse_srt(&o).events, f.events);
    }
    #[test]
    fn vtt() {
        let f = parse_vtt("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\nhi\n");
        assert_eq!(f.events[0].start, 1000);
    }
}
