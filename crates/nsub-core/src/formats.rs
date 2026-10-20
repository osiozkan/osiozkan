//! Import/export of non-ASS subtitle formats and file decoding.

use crate::ass::{Event, Script};
use crate::time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ass,
    Ssa,
    Srt,
    Vtt,
    MicroDvd,
    Lrc,
}

impl Format {
    pub fn from_extension(path: &str) -> Option<Format> {
        let ext = path.rsplit('.').next()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "ass" => Format::Ass,
            "ssa" => Format::Ssa,
            "srt" => Format::Srt,
            "vtt" => Format::Vtt,
            "sub" => Format::MicroDvd,
            "lrc" => Format::Lrc,
            _ => return None,
        })
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Ass => "ass",
            Format::Ssa => "ssa",
            Format::Srt => "srt",
            Format::Vtt => "vtt",
            Format::MicroDvd => "sub",
            Format::Lrc => "lrc",
        }
    }

    /// Guess from content when the extension is missing or wrong.
    pub fn sniff(text: &str) -> Option<Format> {
        let t = text.trim_start_matches('\u{feff}').trim_start();
        if t.starts_with("[Script Info]") {
            return Some(Format::Ass);
        }
        if t.starts_with("WEBVTT") {
            return Some(Format::Vtt);
        }
        if t.starts_with('{') && t.contains("}{") {
            return Some(Format::MicroDvd);
        }
        if t.starts_with('[') && t.contains("]") && t.lines().next().is_some_and(|l| l.contains(':')) && !t.contains("-->") {
            return Some(Format::Lrc);
        }
        if t.contains("-->") {
            return Some(Format::Srt);
        }
        None
    }
}

/// Decode bytes (BOM, UTF-8, UTF-16, or detected legacy codepage such as windows-1254 for Turkish).
pub fn decode_bytes(bytes: &[u8]) -> (String, &'static str) {
    if let Some((enc, _)) = encoding_rs::Encoding::for_bom(bytes) {
        let (s, _) = enc.decode_with_bom_removal(bytes);
        return (s.into_owned(), enc.name());
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), "UTF-8");
    }
    let mut det = chardetng::EncodingDetector::new();
    det.feed(bytes, true);
    let enc = det.guess(Some(b"tr"), true);
    let (s, _, _) = enc.decode(bytes);
    (s.into_owned(), enc.name())
}

pub fn load(bytes: &[u8], hint_path: Option<&str>) -> Result<Script, String> {
    let (text, _) = decode_bytes(bytes);
    let fmt = hint_path
        .and_then(Format::from_extension)
        .filter(|f| *f != Format::MicroDvd && *f != Format::Lrc || Format::sniff(&text) == Some(*f))
        .or_else(|| Format::sniff(&text))
        .ok_or("unrecognised subtitle format")?;
    Ok(match fmt {
        Format::Ass | Format::Ssa => Script::parse(&text),
        Format::Srt => parse_srt(&text),
        Format::Vtt => parse_vtt(&text),
        Format::MicroDvd => parse_microdvd(&text, 23.976),
        Format::Lrc => parse_lrc(&text),
    })
}

pub fn save(script: &Script, fmt: Format) -> String {
    match fmt {
        Format::Ass | Format::Ssa => script.to_ass_string(),
        Format::Srt => write_srt(script),
        Format::Vtt => write_vtt(script),
        Format::MicroDvd => write_microdvd(script, 23.976),
        Format::Lrc => write_lrc(script),
    }
}

/// Convert simple HTML-ish SRT markup to ASS override tags.
pub fn html_to_ass(s: &str) -> String {
    let mut t = s.replace("\r\n", "\n").replace('\r', "\n");
    let map = [
        ("<i>", "{\\i1}"),
        ("</i>", "{\\i0}"),
        ("<b>", "{\\b1}"),
        ("</b>", "{\\b0}"),
        ("<u>", "{\\u1}"),
        ("</u>", "{\\u0}"),
        ("<s>", "{\\s1}"),
        ("</s>", "{\\s0}"),
    ];
    for (a, b) in map {
        t = t.replace(a, b).replace(&a.to_uppercase(), b);
    }
    // <font color="#rrggbb"> ... </font>
    let re = regex::Regex::new(r#"(?i)<font\s+color=["']?#?([0-9a-f]{6})["']?\s*>"#).unwrap();
    t = re
        .replace_all(&t, |c: &regex::Captures| {
            let v = &c[1];
            format!("{{\\c&H{}{}{}&}}", v[4..6].to_uppercase(), v[2..4].to_uppercase(), v[0..2].to_uppercase())
        })
        .into_owned();
    t = regex::Regex::new(r"(?i)</font>").unwrap().replace_all(&t, "{\\c}").into_owned();
    // drop other tags
    t = regex::Regex::new(r"</?[a-zA-Z][^>]*>").unwrap().replace_all(&t, "").into_owned();
    t.replace('\n', "\\N")
}

/// Convert ASS text to SRT-ish HTML (italic/bold/underline only).
pub fn ass_to_html(s: &str) -> String {
    use crate::tags::{parse, Segment};
    let mut out = String::new();
    let (mut b, mut i, mut u) = (false, false, false);
    for seg in parse(s) {
        match seg {
            Segment::Text(t) => out.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            Segment::Block(tags) => {
                for tg in tags {
                    let on = tg.value.trim() == "1";
                    let off = tg.value.trim() == "0";
                    for (name, flag, open, close) in [("b", &mut b, "<b>", "</b>"), ("i", &mut i, "<i>", "</i>"), ("u", &mut u, "<u>", "</u>")] {
                        if tg.name == name {
                            if on && !*flag {
                                out.push_str(open);
                                *flag = true;
                            } else if off && *flag {
                                out.push_str(close);
                                *flag = false;
                            }
                        }
                    }
                }
            }
        }
    }
    if u {
        out.push_str("</u>");
    }
    if i {
        out.push_str("</i>");
    }
    if b {
        out.push_str("</b>");
    }
    out
}

pub fn parse_srt(text: &str) -> Script {
    let mut script = Script::new();
    script.set_info("PlayResX", "1920");
    script.set_info("PlayResY", "1080");
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n").replace('\r', "\n");
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.trim_matches('\n').lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[ti].split_once("-->") else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (time::parse_clock(a.trim()), time::parse_clock(b)) else {
            script.warnings.push(format!("bad time line: {}", lines[ti]));
            continue;
        };
        let body = lines[ti + 1..].join("\n");
        script.events.push(Event { start, end, text: html_to_ass(&body), ..Event::default() });
    }
    script
}

pub fn write_srt(script: &Script) -> String {
    let mut out = String::new();
    let mut n = 0;
    for e in script.events.iter().filter(|e| !e.comment) {
        n += 1;
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n, time::format_srt(e.start), time::format_srt(e.end), ass_to_html(&e.text)));
    }
    out
}

pub fn parse_vtt(text: &str) -> Script {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let body = text.trim_start().strip_prefix("WEBVTT").map(|r| r.split_once("\n\n").map(|x| x.1).unwrap_or("")).unwrap_or(text);
    let mut s = parse_srt(body);
    // `parse_srt` accepts `.` fraction separators already.
    s.events.retain(|e| !e.text.is_empty());
    s
}

pub fn write_vtt(script: &Script) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for e in script.events.iter().filter(|e| !e.comment) {
        out.push_str(&format!("{} --> {}\n{}\n\n", time::format_vtt(e.start), time::format_vtt(e.end), ass_to_html(&e.text)));
    }
    out
}

pub fn parse_microdvd(text: &str, default_fps: f64) -> Script {
    let mut script = Script::new();
    let re = regex::Regex::new(r"^\{(\d+)\}\{(\d+)\}(.*)$").unwrap();
    let mut fps = default_fps;
    for (i, line) in text.lines().enumerate() {
        let Some(c) = re.captures(line.trim_end_matches('\r')) else { continue };
        let (a, b): (f64, f64) = (c[1].parse().unwrap_or(0.0), c[2].parse().unwrap_or(0.0));
        if i == 0 && a == 1.0 && b == 1.0 {
            if let Ok(f) = c[3].trim().parse::<f64>() {
                fps = f;
                continue;
            }
        }
        script.events.push(Event {
            start: (a * 1000.0 / fps).round() as i64,
            end: (b * 1000.0 / fps).round() as i64,
            text: c[3].replace('|', "\\N"),
            ..Event::default()
        });
    }
    script
}

pub fn write_microdvd(script: &Script, fps: f64) -> String {
    let mut out = format!("{{1}}{{1}}{}\n", fps);
    for e in script.events.iter().filter(|e| !e.comment) {
        out.push_str(&format!(
            "{{{}}}{{{}}}{}\n",
            (e.start as f64 * fps / 1000.0).round() as i64,
            (e.end as f64 * fps / 1000.0).round() as i64,
            crate::tags::strip_tags(&e.text).replace("\\N", "|")
        ));
    }
    out
}

pub fn parse_lrc(text: &str) -> Script {
    let mut script = Script::new();
    let re = regex::Regex::new(r"^\[(\d+):(\d+(?:[.:]\d+)?)\](.*)$").unwrap();
    let mut rows: Vec<(i64, String)> = Vec::new();
    for line in text.lines() {
        if let Some(c) = re.captures(line.trim_end_matches('\r')) {
            let m: i64 = c[1].parse().unwrap_or(0);
            let sec = c[2].replace(':', ".").parse::<f64>().unwrap_or(0.0);
            rows.push((m * 60_000 + (sec * 1000.0).round() as i64, c[3].trim().to_string()));
        }
    }
    for (i, (t, txt)) in rows.iter().enumerate() {
        let end = rows.get(i + 1).map(|r| r.0).unwrap_or(t + 3000);
        script.events.push(Event { start: *t, end, text: txt.clone(), ..Event::default() });
    }
    script
}

pub fn write_lrc(script: &Script) -> String {
    let mut out = String::new();
    for e in script.events.iter().filter(|e| !e.comment) {
        out.push_str(&format!("[{:02}:{:05.2}]{}\n", e.start / 60_000, (e.start % 60_000) as f64 / 1000.0, crate::tags::visible_text(&e.text).replace('\n', " ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_roundtrip() {
        let src = "1\n00:00:01,000 --> 00:00:02,500\n<i>Merhaba</i>\nDünya\n\n2\n00:00:03,000 --> 00:00:04,000\nİkinci\n";
        let s = parse_srt(src);
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "{\\i1}Merhaba{\\i0}\\NDünya");
        let out = write_srt(&s);
        assert!(out.contains("<i>Merhaba</i>\nDünya"));
        assert!(out.contains("00:00:01,000 --> 00:00:02,500"));
    }

    #[test]
    fn vtt() {
        let s = parse_vtt("WEBVTT\n\n00:01.000 --> 00:02.000\nhi\n");
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.events[0].start, 1000);
        assert!(write_vtt(&s).starts_with("WEBVTT"));
    }

    #[test]
    fn microdvd() {
        let s = parse_microdvd("{1}{1}25\n{25}{50}a|b\n", 23.976);
        assert_eq!(s.events[0].start, 1000);
        assert_eq!(s.events[0].text, "a\\Nb");
    }

    #[test]
    fn decode_turkish_cp1254() {
        // "İşğüöç" in windows-1254
        let bytes = [0xDD, 0xFE, 0xF0, 0xFC, 0xF6, 0xE7, b' ', b'Y', b'a', b'n', b'a', b' ', b'b', b'i', b'r', b' ', b'c', b'u', b'm', b'l', b'e'];
        let (s, _) = decode_bytes(&bytes);
        assert!(s.starts_with("İşğüöç"), "{s}");
    }

    #[test]
    fn sniff() {
        assert_eq!(Format::sniff("[Script Info]\n"), Some(Format::Ass));
        assert_eq!(Format::sniff("1\n00:00:01,000 --> 00:00:02,000\nx"), Some(Format::Srt));
    }
}
