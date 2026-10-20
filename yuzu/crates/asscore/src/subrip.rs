//! SRT and WebVTT import/export.

use crate::model::*;
use crate::time;

fn parse_ts(s: &str) -> Option<i64> { time::parse_ass(&s.trim().replace(',', ".")) }

fn html_to_ass(t: &str) -> String {
    let re = regex::Regex::new(r"(?i)</?(i|b|u|s)>").unwrap();
    let mut out = re.replace_all(t, |c: &regex::Captures| {
        let m = c.get(0).unwrap().as_str();
        let on = !m.starts_with("</");
        let tag = c[1].to_ascii_lowercase();
        let tag = if tag == "s" { "s" } else { &tag };
        format!("{{\\{}{}}}", tag, if on { 1 } else { 0 })
    }).to_string();
    let any = regex::Regex::new(r"</?[a-zA-Z][^>]*>").unwrap();
    out = any.replace_all(&out, "").to_string();
    out = out.replace("\r\n", "\\N").replace('\n', "\\N");
    out
}

pub fn parse_srt(input: &str) -> Script {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n");
    let mut sc = Script::new();
    for block in input.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty() || false).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = lines[ti].split_once("-->").unwrap();
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(s), Some(e)) = (parse_ts(a), parse_ts(b)) else { continue };
        let text = lines[ti + 1..].join("\n");
        sc.events.push(Event::new(s, e, "Default", &html_to_ass(&text)));
    }
    sc
}

pub fn parse_vtt(input: &str) -> Script {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n");
    let mut sc = Script::new();
    for block in input.split("\n\n") {
        if block.trim_start().starts_with("WEBVTT") || block.trim_start().starts_with("NOTE") || block.trim_start().starts_with("STYLE") { continue; }
        let lines: Vec<&str> = block.lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = lines[ti].split_once("-->").unwrap();
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(s), Some(e)) = (parse_ts(a), parse_ts(b)) else { continue };
        let text = lines[ti + 1..].join("\n");
        sc.events.push(Event::new(s, e, "Default", &html_to_ass(&text)));
    }
    sc
}

fn ass_to_html(t: &str) -> String {
    let mut out = String::new();
    for p in crate::tags::split_parts(t) {
        match p {
            crate::tags::Part::Text(x) => out.push_str(&x.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            crate::tags::Part::Block(b) => {
                for tag in crate::tags::split_block_tags(&b) {
                    let (n, a) = crate::tags::tag_name_arg(&tag);
                    if matches!(n.as_str(), "i" | "b" | "u" | "s") {
                        let on = a.trim() == "1";
                        out.push_str(&format!("<{}{}>", if on { "" } else { "/" }, n));
                    }
                }
            }
        }
    }
    out
}

pub fn write_srt(sc: &Script) -> String {
    let mut o = String::new();
    let mut n = 1;
    for e in sc.events.iter().filter(|e| !e.comment) {
        o.push_str(&format!("{}\n{} --> {}\n{}\n\n", n, time::format_srt(e.start), time::format_srt(e.end), ass_to_html(&e.text)));
        n += 1;
    }
    o
}

pub fn write_vtt(sc: &Script) -> String {
    let mut o = String::from("WEBVTT\n\n");
    for e in sc.events.iter().filter(|e| !e.comment) {
        o.push_str(&format!("{} --> {}\n{}\n\n", time::format_vtt(e.start), time::format_vtt(e.end), ass_to_html(&e.text)));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let s = parse_srt("1\n00:00:01,000 --> 00:00:02,500\n<i>Hello</i>\nworld\n\n2\n00:00:03,000 --> 00:00:04,000\nBye\n");
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "{\\i1}Hello{\\i0}\\Nworld");
        let out = write_srt(&s);
        assert!(out.contains("<i>Hello</i>\nworld"));
        let v = write_vtt(&s);
        assert!(v.starts_with("WEBVTT") && v.contains("00:00:01.000 --> 00:00:02.500"));
        assert_eq!(parse_vtt(&v).events.len(), 2);
    }
}
