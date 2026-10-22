//! SubRip import/export.
use crate::ass::{Event, Script};
use crate::time;

pub fn parse(text: &str) -> Script {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let mut script = Script::new_default();
    for block in text.split("\n\n") {
        let mut lines = block.trim().lines().peekable();
        let Some(first) = lines.next() else { continue };
        let timing = if first.contains("-->") { first } else { match lines.next() { Some(l) => l, None => continue } };
        let Some((a, b)) = timing.split_once("-->") else { continue };
        let (Some(start), Some(end)) = (time::parse_srt(a), time::parse_srt(b.split_whitespace().next().unwrap_or(""))) else { continue };
        let body: Vec<&str> = lines.collect();
        let text = srt_to_ass_text(&body.join("\n"));
        script.push_event(Event { start_time: start, end_time: end, text, ..Default::default() });
    }
    script
}

fn srt_to_ass_text(s: &str) -> String {
    let mut out = s.replace('\n', "\\N");
    for (from, to) in [("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"), ("<u>", "{\\u1}"), ("</u>", "{\\u0}"), ("<I>", "{\\i1}"), ("</I>", "{\\i0}"), ("<B>", "{\\b1}"), ("</B>", "{\\b0}")] {
        out = out.replace(from, to);
    }
    // drop remaining html-ish tags such as <font ...>
    let re = regex::Regex::new(r"</?font[^>]*>").unwrap();
    re.replace_all(&out, "").into_owned()
}

fn ass_to_srt_text(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(p) = rest.find('{') {
        out.push_str(&rest[..p]);
        let Some(q) = rest[p..].find('}') else { rest = &rest[p..]; break };
        let block = &rest[p + 1..p + q];
        for t in block.split('\\') {
            match t {
                "i1" => out.push_str("<i>"),
                "i0" => out.push_str("</i>"),
                "b1" => out.push_str("<b>"),
                "b0" => out.push_str("</b>"),
                "u1" => out.push_str("<u>"),
                "u0" => out.push_str("</u>"),
                _ => {}
            }
        }
        rest = &rest[p + q + 1..];
    }
    out.push_str(rest);
    out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

pub fn write(script: &Script) -> String {
    let mut evs: Vec<&Event> = script.events().filter(|e| !e.comment).collect();
    evs.sort_by_key(|e| e.start_time);
    let mut out = String::new();
    for (i, e) in evs.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, time::format_srt(e.start_time), time::format_srt(e.end_time), ass_to_srt_text(&e.text)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let s = parse("1\n00:00:01,000 --> 00:00:02,500\n<i>Hi</i>\nthere\n\n2\n00:00:03,000 --> 00:00:04,000\nBye\n");
        assert_eq!(s.events().count(), 2);
        assert_eq!(s.events().next().unwrap().text, "{\\i1}Hi{\\i0}\\Nthere");
        let out = write(&s);
        assert!(out.contains("<i>Hi</i>\nthere"));
    }
}
