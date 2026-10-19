//! SRT and WebVTT import/export.
use crate::ass::{Event, Script};
use crate::time::{format_clock, parse_time};

fn html_to_ass(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let tag = rest[i + 1..i + j].trim().to_ascii_lowercase();
        match tag.as_str() {
            "i" => out.push_str("{\\i1}"),
            "/i" => out.push_str("{\\i0}"),
            "b" => out.push_str("{\\b1}"),
            "/b" => out.push_str("{\\b0}"),
            "u" => out.push_str("{\\u1}"),
            "/u" => out.push_str("{\\u0}"),
            "s" => out.push_str("{\\s1}"),
            "/s" => out.push_str("{\\s0}"),
            t if t.starts_with("font") => {
                if let Some(p) = t.find("color=") {
                    let v = t[p + 6..].trim_matches(|c| c == '"' || c == '\'' || c == ' ');
                    let v = v.split(['"', '\'', ' ']).next().unwrap_or("");
                    if let Some(hex) = v.strip_prefix('#') {
                        if hex.len() == 6 {
                            let (r, g, b) = (&hex[0..2], &hex[2..4], &hex[4..6]);
                            out.push_str(&format!("{{\\c&H{}{}{}&}}", b.to_uppercase(), g.to_uppercase(), r.to_uppercase()));
                        }
                    }
                }
            }
            "/font" => out.push_str("{\\c}"),
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_blocks(text: &str, vtt: bool) -> Script {
    let mut script = Script::new();
    script.info_set("Title", "Imported");
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = text.split('\n').peekable();
    if vtt {
        // skip header block
        for l in lines.by_ref() {
            if l.trim().is_empty() {
                break;
            }
        }
    }
    let mut cur: Option<Event> = None;
    let mut body: Vec<&str> = Vec::new();
    let flush = |cur: &mut Option<Event>, body: &mut Vec<&str>, script: &mut Script| {
        if let Some(mut e) = cur.take() {
            e.text = html_to_ass(&body.join("\\N"));
            script.events.push(e);
        }
        body.clear();
    };
    for l in lines {
        if let Some((a, b)) = l.split_once("-->") {
            // the line holding the timing; VTT may have settings after end time
            let a = a.trim();
            let b = b.trim().split_whitespace().next().unwrap_or("");
            if let (Some(s), Some(e)) = (parse_time(a), parse_time(b)) {
                flush(&mut cur, &mut body, &mut script);
                cur = Some(Event { start: s, end: e, ..Default::default() });
                continue;
            }
        }
        if cur.is_some() {
            if l.trim().is_empty() {
                flush(&mut cur, &mut body, &mut script);
            } else {
                body.push(l.trim_end());
            }
        }
    }
    flush(&mut cur, &mut body, &mut script);
    script
}

pub fn parse_srt(text: &str) -> Script {
    parse_blocks(text, false)
}

pub fn parse_vtt(text: &str) -> Script {
    parse_blocks(text, true)
}

fn ass_to_html(text: &str) -> String {
    use crate::tags::{parse, Item, Segment};
    let mut out = String::new();
    for seg in parse(text) {
        match seg {
            Segment::Text(t) => out.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            Segment::Block(items) => {
                for it in items {
                    if let Item::Tag { name, value } = it {
                        let on = value.trim() != "0";
                        match (name.as_str(), on) {
                            ("i", true) => out.push_str("<i>"),
                            ("i", false) => out.push_str("</i>"),
                            ("b", true) => out.push_str("<b>"),
                            ("b", false) => out.push_str("</b>"),
                            ("u", true) => out.push_str("<u>"),
                            ("u", false) => out.push_str("</u>"),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn write_srt(script: &Script) -> String {
    let mut o = String::new();
    let mut n = 1;
    let mut evs: Vec<&Event> = script.events.iter().filter(|e| !e.is_comment()).collect();
    evs.sort_by_key(|e| (e.start, e.end));
    for e in evs {
        let t = ass_to_html(&e.text);
        if t.trim().is_empty() {
            continue;
        }
        o.push_str(&format!("{n}\r\n{} --> {}\r\n{}\r\n\r\n", format_clock(e.start, ','), format_clock(e.end, ','), t.replace('\n', "\r\n")));
        n += 1;
    }
    o
}

pub fn write_vtt(script: &Script) -> String {
    let mut o = String::from("WEBVTT\n\n");
    let mut evs: Vec<&Event> = script.events.iter().filter(|e| !e.is_comment()).collect();
    evs.sort_by_key(|e| (e.start, e.end));
    for e in evs {
        let t = ass_to_html(&e.text);
        if t.trim().is_empty() {
            continue;
        }
        o.push_str(&format!("{} --> {}\n{}\n\n", format_clock(e.start, '.'), format_clock(e.end, '.'), t));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let s = parse_srt("1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Selam</i>\r\ndünya\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nTamam\r\n");
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "{\\i1}Selam{\\i0}\\Ndünya");
        let out = write_srt(&s);
        assert!(out.contains("<i>Selam</i>\r\ndünya"));
        assert!(out.contains("00:00:01,000 --> 00:00:02,500"));
    }
    #[test]
    fn vtt() {
        let s = parse_vtt("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\nHi\n");
        assert_eq!(s.events[0].start, 1000);
        assert_eq!(s.events[0].text, "Hi");
    }
}
