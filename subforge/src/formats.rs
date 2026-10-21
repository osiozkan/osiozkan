//! Additional subtitle formats (Subtitle Edit staples): LRC (incl. enhanced word timing), MicroDVD,
//! SubViewer-style SBV, and plain text import.

use crate::ass::{Document, Entry, Event};
use regex::Regex;

fn push(doc: &mut Document, start: i64, end: i64, text: String) {
    doc.entries.push(Entry::Event(Event { start, end, text, ..Event::default() }));
}

fn ms_lrc(m: &str, s: &str, frac: Option<&str>) -> i64 {
    let m: i64 = m.parse().unwrap_or(0);
    let s: i64 = s.parse().unwrap_or(0);
    let f = frac.map(|f| {
        let mut v: i64 = f.get(..f.len().min(3)).and_then(|x| x.parse().ok()).unwrap_or(0);
        for _ in f.len().min(3)..3 {
            v *= 10;
        }
        v
    });
    (m * 60 + s) * 1000 + f.unwrap_or(0)
}

/// LRC: `[mm:ss.xx]text`, several stamps per line allowed, `<mm:ss.xx>` word stamps become `\k`.
/// A line ends where the next one starts (the last one lasts `last_ms`).
pub fn parse_lrc(src: &str, last_ms: i64) -> Document {
    let stamp = Regex::new(r"\[(\d+):(\d{1,2})(?:[.:](\d{1,3}))?\]").unwrap();
    let word = Regex::new(r"<(\d+):(\d{1,2})(?:[.:](\d{1,3}))?>").unwrap();
    let mut doc = Document::new_default();
    let mut lines: Vec<(i64, String)> = Vec::new();
    for raw in src.replace("\r\n", "\n").lines() {
        let raw = raw.trim_start_matches('\u{feff}');
        let stamps: Vec<i64> = stamp.captures_iter(raw).map(|c| ms_lrc(&c[1], &c[2], c.get(3).map(|m| m.as_str()))).collect();
        if stamps.is_empty() {
            continue;
        }
        let text = stamp.replace_all(raw, "").trim().to_string();
        for t in stamps {
            lines.push((t, text.clone()));
        }
    }
    lines.sort_by_key(|l| l.0);
    for (i, (start, text)) in lines.iter().enumerate() {
        let end = lines.get(i + 1).map(|n| n.0).unwrap_or(start + last_ms);
        let text = if word.is_match(text) {
            // enhanced LRC: each <t> starts a syllable lasting until the next stamp (or line end)
            let marks: Vec<(usize, usize, i64)> = word.captures_iter(text).map(|c| { let m = c.get(0).unwrap(); (m.start(), m.end(), ms_lrc(&c[1], &c[2], c.get(3).map(|m| m.as_str()))) }).collect();
            let mut out = String::new();
            for (k, (s, e, t0)) in marks.iter().enumerate() {
                let t1 = marks.get(k + 1).map(|m| m.2).unwrap_or(end);
                let seg_end = marks.get(k + 1).map(|m| m.0).unwrap_or(text.len());
                let _ = t0;
                out.push_str(&format!("{{\\kf{}}}{}", ((t1 - t0).max(0) + 5) / 10, &text[*e..seg_end]));
                let _ = s;
            }
            out
        } else {
            text.clone()
        };
        push(&mut doc, *start, end, text);
    }
    doc
}

pub fn to_lrc(doc: &Document) -> String {
    let mut out = String::new();
    for e in doc.events().filter(|e| !e.comment) {
        let t = e.start.max(0);
        let text = crate::tags::visible_text(&e.text).replace('\n', " ");
        out.push_str(&format!("[{:02}:{:02}.{:02}]{}\n", t / 60000, (t / 1000) % 60, (t % 1000) / 10, text));
    }
    out
}

/// MicroDVD `{start}{end}text` (frames; `|` = line break). A first line `{1}{1}23.976` sets the rate.
pub fn parse_microdvd(src: &str, default_fps: f64) -> Document {
    let re = Regex::new(r"^\{(\d+)\}\{(\d+)\}(.*)$").unwrap();
    let mut doc = Document::new_default();
    let mut fps = default_fps;
    for raw in src.replace("\r\n", "\n").lines() {
        let Some(c) = re.captures(raw.trim_start_matches('\u{feff}')) else { continue };
        let (a, b): (i64, i64) = (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0));
        if a == 1 && b == 1 {
            if let Ok(f) = c[3].trim().parse::<f64>() {
                fps = f;
                continue;
            }
        }
        let text = c[3].replace('|', "\\N");
        let text = Regex::new(r"\{y:i\}").unwrap().replace_all(&text, "{\\i1}").to_string();
        push(&mut doc, (a as f64 * 1000.0 / fps).round() as i64, (b as f64 * 1000.0 / fps).round() as i64, text);
    }
    doc
}

pub fn to_microdvd(doc: &Document, fps: f64) -> String {
    let mut out = format!("{{1}}{{1}}{}\n", fps);
    for e in doc.events().filter(|e| !e.comment) {
        let f = |t: i64| (t as f64 * fps / 1000.0).round() as i64;
        let text = crate::tags::visible_text(&e.text).replace('\n', "|");
        out.push_str(&format!("{{{}}}{{{}}}{}\n", f(e.start), f(e.end), text));
    }
    out
}

/// YouTube SBV: `h:mm:ss.mmm,h:mm:ss.mmm` then text lines, blank line between cues.
pub fn parse_sbv(src: &str) -> Document {
    let re = Regex::new(r"^(\d+:\d{2}:\d{2}\.\d{1,3}),(\d+:\d{2}:\d{2}\.\d{1,3})\s*$").unwrap();
    let mut doc = Document::new_default();
    let src = src.replace("\r\n", "\n");
    let mut lines = src.lines().peekable();
    while let Some(l) = lines.next() {
        let Some(c) = re.captures(l.trim_start_matches('\u{feff}')) else { continue };
        let (Some(s), Some(e)) = (crate::time::parse_srt_time(&c[1]), crate::time::parse_srt_time(&c[2])) else { continue };
        let mut body = vec![];
        while let Some(n) = lines.peek() {
            if n.trim().is_empty() {
                break;
            }
            body.push(lines.next().unwrap());
        }
        push(&mut doc, s, e, body.join("\\N"));
    }
    doc
}

pub fn to_sbv(doc: &Document) -> String {
    let t = |ms: i64| format!("{}:{:02}:{:02}.{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000);
    let mut out = String::new();
    for e in doc.events().filter(|e| !e.comment) {
        out.push_str(&format!("{},{}\n{}\n\n", t(e.start.max(0)), t(e.end.max(0)), crate::tags::visible_text(&e.text)));
    }
    out
}

/// Plain text: one event per non-empty line, `each_ms` long, back to back.
pub fn parse_plain(src: &str, each_ms: i64) -> Document {
    let mut doc = Document::new_default();
    let mut t = 0;
    for l in src.replace("\r\n", "\n").lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        push(&mut doc, t, t + each_ms, l.to_string());
        t += each_ms;
    }
    doc
}

/// Pick a parser from the file extension. `fps` is only used by frame-based formats.
pub fn parse_by_extension(ext: &str, text: &str, fps: f64) -> Option<Document> {
    match ext {
        "lrc" => Some(parse_lrc(text, 4000)),
        "sub" if text.trim_start().starts_with('{') => Some(parse_microdvd(text, fps)),
        "sbv" => Some(parse_sbv(text)),
        _ => None,
    }
}

pub fn render_by_extension(ext: &str, doc: &Document, fps: f64) -> Option<String> {
    match ext {
        "lrc" => Some(to_lrc(doc)),
        "sub" => Some(to_microdvd(doc, fps)),
        "sbv" => Some(to_sbv(doc)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_basic_and_enhanced() {
        let d = parse_lrc("[00:01.00]Hello\n[00:03.50]World\n[00:05.00][00:09.00]Twice", 3000);
        let e: Vec<_> = d.events().map(|e| (e.start, e.end, e.text.clone())).collect();
        assert_eq!(e[0], (1000, 3500, "Hello".into()));
        assert_eq!(e[1], (3500, 5000, "World".into()));
        assert_eq!(e.len(), 4);
        let d = parse_lrc("[00:10.00]<00:10.00>Ka<00:10.50>ra<00:11.20>oke\n[00:12.00]end", 3000);
        let t = &d.events().next().unwrap().text;
        assert_eq!(t, "{\\kf50}Ka{\\kf70}ra{\\kf80}oke");
        assert!(to_lrc(&d).starts_with("[00:10.00]Karaoke"));
    }

    #[test]
    fn microdvd_and_sbv() {
        let d = parse_microdvd("{1}{1}25\n{25}{50}Hi|there", 23.976);
        let e = d.events().next().unwrap();
        assert_eq!((e.start, e.end, e.text.as_str()), (1000, 2000, "Hi\\Nthere"));
        assert_eq!(to_microdvd(&d, 25.0), "{1}{1}25\n{25}{50}Hi|there\n");
        let d = parse_sbv("0:00:01.000,0:00:02.500\nHello\nworld\n\n0:00:03.000,0:00:04.000\nBye\n");
        assert_eq!(d.events().count(), 2);
        assert_eq!(to_sbv(&d).lines().next().unwrap(), "0:00:01.000,0:00:02.500");
        let p = parse_plain("a\n\n b \n", 2000);
        assert_eq!(p.events().map(|e| e.start).collect::<Vec<_>>(), [0, 2000]);
    }
}
