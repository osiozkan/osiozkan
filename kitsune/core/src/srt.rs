//! SRT and WebVTT readers/writers. Basic HTML-ish tags map to ASS override tags.
use crate::model::*;
use crate::time::*;

/// Convert SRT/VTT inline markup to ASS text.
pub fn markup_to_ass(s: &str) -> String {
    let mut t = s.replace("\r", "").replace('\n', "\\N");
    for (a, b) in [("<i>", "{\\i1}"), ("</i>", "{\\i0}"), ("<b>", "{\\b1}"), ("</b>", "{\\b0}"),
                   ("<u>", "{\\u1}"), ("</u>", "{\\u0}"), ("<s>", "{\\s1}"), ("</s>", "{\\s0}"),
                   ("<I>", "{\\i1}"), ("</I>", "{\\i0}"), ("<B>", "{\\b1}"), ("</B>", "{\\b0}"),
                   ("<U>", "{\\u1}"), ("</U>", "{\\u0}")] {
        t = t.replace(a, b);
    }
    // strip other tags such as <font ...>, <c.x>, </font>
    let mut out = String::new();
    let mut in_tag = false;
    for ch in t.chars() {
        match ch { '<' => in_tag = true, '>' if in_tag => in_tag = false, c if !in_tag => out.push(c), _ => {} }
    }
    out.replace("{\\i0}{\\i1}", "").replace("{\\i1}{\\i1}", "{\\i1}")
}

/// Convert ASS text to SRT/VTT markup (drops unsupported tags).
pub fn ass_to_markup(s: &str) -> String {
    let mut out = String::new();
    let (mut i, mut b, mut u, mut st) = (false, false, false, false);
    let mut rest = s;
    while !rest.is_empty() {
        if rest.starts_with('{') {
            let end = rest.find('}').unwrap_or(rest.len() - 1);
            let blk = &rest[1..end];
            for tag in blk.split('\\').skip(1) {
                let flag = |name: &str, cur: &mut bool, o: char, out: &mut String| {
                    if let Some(v) = tag.strip_prefix(name) {
                        let on = v.trim() != "0";
                        if on && !*cur { out.push_str(&format!("<{}>", o)); *cur = true; }
                        if !on && *cur { out.push_str(&format!("</{}>", o)); *cur = false; }
                    }
                };
                if tag.starts_with('i') && !tag.starts_with("iclip") { flag("i", &mut i, 'i', &mut out); }
                else if tag.starts_with('b') && !tag.starts_with("blur") && !tag.starts_with("bord") && !tag.starts_with("be") { flag("b", &mut b, 'b', &mut out); }
                else if tag.starts_with('u') { flag("u", &mut u, 'u', &mut out); }
                else if tag.starts_with('s') && tag[1..].chars().next().map_or(false, |c| c.is_ascii_digit()) { flag("s", &mut st, 's', &mut out); }
            }
            rest = &rest[(end + 1).min(rest.len())..];
        } else if let Some(r) = rest.strip_prefix("\\N").or_else(|| rest.strip_prefix("\\n")) {
            out.push('\n'); rest = r;
        } else if let Some(r) = rest.strip_prefix("\\h") {
            out.push(' '); rest = r;
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c); rest = &rest[c.len_utf8()..];
        }
    }
    for (on, c) in [(i, 'i'), (b, 'b'), (u, 'u'), (st, 's')] { if on { out.push_str(&format!("</{}>", c)); } }
    out
}

fn blocks(input: &str) -> Vec<Vec<&str>> {
    let mut res = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for l in input.trim_start_matches('\u{feff}').lines() {
        let l = l.trim_end_matches('\r');
        if l.trim().is_empty() { if !cur.is_empty() { res.push(std::mem::take(&mut cur)); } }
        else { cur.push(l); }
    }
    if !cur.is_empty() { res.push(cur); }
    res
}

fn parse_cues(input: &str) -> Script {
    let mut s = Script::new_default();
    for blk in blocks(input) {
        let Some(ti) = blk.iter().position(|l| l.contains("-->")) else { continue };
        let (a, b) = blk[ti].split_once("-->").unwrap();
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(st), Some(en)) = (parse_hms_ms(a.trim()), parse_hms_ms(b)) else { continue };
        let text = blk[ti + 1..].join("\n");
        s.entries.push(Entry::Event(Event::new(st, en, &markup_to_ass(&text))));
    }
    s
}

pub fn parse_srt(input: &str) -> Script { parse_cues(input) }
pub fn parse_vtt(input: &str) -> Script { parse_cues(input) }

pub fn write_srt(script: &Script) -> String {
    let mut out = String::new();
    for (n, e) in script.events().filter(|e| !e.comment).enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n + 1, fmt_hms_ms(e.start, ','), fmt_hms_ms(e.end, ','), ass_to_markup(&e.text)));
    }
    out
}

pub fn write_vtt(script: &Script) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for e in script.events().filter(|e| !e.comment) {
        out.push_str(&format!("{} --> {}\n{}\n\n", fmt_hms_ms(e.start, '.'), fmt_hms_ms(e.end, '.'), ass_to_markup(&e.text)));
    }
    out
}
