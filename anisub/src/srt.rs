//! SRT / WebVTT import and export, converting to and from the ASS model.

use crate::ass::{Dialogue, Document, Entry, Style};

fn parse_ts(s: &str) -> Option<i64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, rest) = match parts.as_slice() {
        [h, m, r] => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?, *r),
        [m, r] => (0, m.parse::<i64>().ok()?, *r),
        _ => return None,
    };
    let (sec, frac) = rest.split_once('.').unwrap_or((rest, "0"));
    let digits: String = frac.chars().take(3).collect();
    let ms = digits.parse::<i64>().ok()? * 10i64.pow(3 - digits.len() as u32);
    Some(((h * 60 + m) * 60 + sec.parse::<i64>().ok()?) * 1000 + ms)
}

fn fmt_ts(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, sep, ms % 1000)
}

/// Convert basic HTML-ish SRT markup to ASS override tags.
fn srt_text_to_ass(t: &str) -> String {
    let mut s = t.replace("\r", "").replace('\n', "\\N");
    for (a, b) in [("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"),
                   ("<u>", "{\\u1}"), ("</u>", "{\\u0}"), ("<s>", "{\\s1}"), ("</s>", "{\\s0}")] {
        s = s.replace(a, b).replace(&a.to_uppercase(), b);
    }
    s
}

fn ass_text_to_srt(t: &str) -> String {
    let mut out = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut tag = String::new();
                for c in chars.by_ref() {
                    if c == '}' { break; }
                    tag.push(c);
                }
                for part in tag.split('\\') {
                    match part {
                        "i1" => out.push_str("<i>"),
                        "i0" => out.push_str("</i>"),
                        "b1" => out.push_str("<b>"),
                        "b0" => out.push_str("</b>"),
                        "u1" => out.push_str("<u>"),
                        "u0" => out.push_str("</u>"),
                        _ => {}
                    }
                }
            }
            '\\' if matches!(chars.peek(), Some('N') | Some('n')) => {
                chars.next();
                out.push('\n');
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn parse(src: &str, play_res: (i64, i64)) -> Document {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src).replace("\r\n", "\n");
    let mut doc = Document::default();
    let push = |doc: &mut Document, k: &str, v: String| doc.entries.push(Entry::Info { key: k.into(), value: v });
    push(&mut doc, "ScriptType", "v4.00+".into());
    push(&mut doc, "PlayResX", play_res.0.to_string());
    push(&mut doc, "PlayResY", play_res.1.to_string());
    push(&mut doc, "WrapStyle", "0".into());
    doc.entries.push(Entry::Style(Style::default()));
    for block in src.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[ti].split_once("-->") else { continue };
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(s), Some(e)) = (parse_ts(a), parse_ts(b)) else { continue };
        let text = srt_text_to_ass(&lines[ti + 1..].join("\n"));
        doc.entries.push(Entry::Dialogue(Dialogue {
            start_time: s, end_time: e, style: "Default".into(), text, ..Default::default()
        }));
    }
    doc
}

pub fn write_srt(doc: &Document) -> String {
    let mut out = String::new();
    let mut lines: Vec<&Dialogue> = doc.dialogues().filter(|d| !d.comment).collect();
    lines.sort_by_key(|d| d.start_time);
    for (i, d) in lines.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, fmt_ts(d.start_time, ','), fmt_ts(d.end_time, ','), ass_text_to_srt(&d.text)));
    }
    out
}

pub fn write_vtt(doc: &Document) -> String {
    let mut out = String::from("WEBVTT\n\n");
    let mut lines: Vec<&Dialogue> = doc.dialogues().filter(|d| !d.comment).collect();
    lines.sort_by_key(|d| d.start_time);
    for d in lines {
        out.push_str(&format!("{} --> {}\n{}\n\n", fmt_ts(d.start_time, '.'), fmt_ts(d.end_time, '.'), ass_text_to_srt(&d.text)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let d = parse("1\n00:00:01,000 --> 00:00:02,500\n<i>Hi</i>\nthere\n\n2\n00:00:03,000 --> 00:00:04,000\nBye\n", (1920, 1080));
        assert_eq!(d.dialogues().count(), 2);
        assert_eq!(d.dialogues().next().unwrap().text, "{\\i1}Hi{\\i0}\\Nthere");
        let s = write_srt(&d);
        assert!(s.contains("00:00:01,000 --> 00:00:02,500\n<i>Hi</i>\nthere"));
        assert!(write_vtt(&d).starts_with("WEBVTT"));
    }
}
