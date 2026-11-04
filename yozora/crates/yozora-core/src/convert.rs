//! Diğer altyazı biçimleriyle dönüşüm: SRT, WebVTT, MicroDVD (.sub), düz metin.
use crate::ass::{Document, Event};
use crate::time;

fn ass_text_from_plain(t: &str) -> String {
    // Satır sonları ASS'e, basit HTML etiketleri ASS etiketlerine.
    let mut s = t.replace("\r\n", "\n").replace('\r', "\n").trim_matches('\n').replace('\n', "\\N");
    for (h, a) in [
        ("<i>", "{\\i1}"),
        ("</i>", "{\\i0}"),
        ("<b>", "{\\b1}"),
        ("</b>", "{\\b0}"),
        ("<u>", "{\\u1}"),
        ("</u>", "{\\u0}"),
        ("<I>", "{\\i1}"),
        ("</I>", "{\\i0}"),
        ("<B>", "{\\b1}"),
        ("</B>", "{\\b0}"),
    ] {
        s = s.replace(h, a);
    }
    // Kalan HTML/font etiketlerini sil.
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn plain_from_ass_text(t: &str, html: bool) -> String {
    let mut out = String::new();
    for b in crate::tags::parse_blocks(t) {
        match b {
            crate::tags::Block::Text(x) => out.push_str(&x),
            crate::tags::Block::Override(tags) if html => {
                for tg in tags {
                    let on = !tg.arg.trim().starts_with('0');
                    match (tg.name.as_str(), on) {
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
            _ => {}
        }
    }
    out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

pub fn from_srt(input: &str) -> Document {
    let mut doc = Document::new_default();
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n").replace('\r', "\n");
    for block in input.split("\n\n") {
        let mut lines = block.lines().filter(|l| !l.trim().is_empty()).peekable();
        // İsteğe bağlı sıra numarası.
        if let Some(first) = lines.peek() {
            if first.trim().chars().all(|c| c.is_ascii_digit()) && !first.contains("-->") {
                lines.next();
            }
        }
        let Some(tl) = lines.next() else { continue };
        let Some((a, b)) = tl.split_once("-->") else {
            doc.warnings.push(format!("zaman satırı anlaşılamadı: {tl}"));
            continue;
        };
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(s), Some(e)) = (time::parse_srt(a), time::parse_srt(b)) else {
            doc.warnings.push(format!("geçersiz zaman: {tl}"));
            continue;
        };
        let text: Vec<&str> = lines.collect();
        doc.events.push(Event { start: s, end: e, text: ass_text_from_plain(&text.join("\n")), ..Event::default() });
    }
    doc
}

pub fn to_srt(doc: &Document) -> String {
    let mut out = String::new();
    let mut n = 0;
    let mut evs: Vec<&Event> = doc.events.iter().filter(|e| !e.comment).collect();
    evs.sort_by_key(|e| (e.start, e.end));
    for e in evs {
        n += 1;
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            n,
            time::format_srt(e.start),
            time::format_srt(e.end),
            plain_from_ass_text(&e.text, true)
        ));
    }
    out
}

pub fn from_vtt(input: &str) -> Document {
    let body = input.strip_prefix('\u{feff}').unwrap_or(input);
    let body = body.replacen("WEBVTT", "", 1);
    let mut doc = Document::new_default();
    let norm = body.replace("\r\n", "\n").replace('\r', "\n");
    for block in norm.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[ti].split_once("-->") else { continue };
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let norm_t = |s: &str| -> Option<i64> {
            let s = s.trim();
            // MM:SS.mmm -> 00:MM:SS.mmm
            if s.matches(':').count() == 1 {
                time::parse_ass(&format!("0:{s}"))
            } else {
                time::parse_ass(s)
            }
        };
        let (Some(s), Some(e)) = (norm_t(a), norm_t(b)) else { continue };
        doc.events.push(Event {
            start: s,
            end: e,
            text: ass_text_from_plain(&lines[ti + 1..].join("\n")),
            ..Event::default()
        });
    }
    doc
}

pub fn to_vtt(doc: &Document) -> String {
    let mut out = String::from("WEBVTT\n\n");
    let mut evs: Vec<&Event> = doc.events.iter().filter(|e| !e.comment).collect();
    evs.sort_by_key(|e| (e.start, e.end));
    for e in evs {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            time::format_vtt(e.start),
            time::format_vtt(e.end),
            plain_from_ass_text(&e.text, true)
        ));
    }
    out
}

/// MicroDVD: `{start}{end}text`, `|` satır sonu.
pub fn from_microdvd(input: &str, fps: f64) -> Document {
    let mut doc = Document::new_default();
    for line in input.lines() {
        let l = line.trim();
        if !l.starts_with('{') {
            continue;
        }
        let Some(c1) = l.find('}') else { continue };
        let rest = &l[c1 + 1..];
        if !rest.starts_with('{') {
            continue;
        }
        let Some(c2) = rest.find('}') else { continue };
        let (Ok(a), Ok(b)) = (l[1..c1].parse::<i64>(), rest[1..c2].parse::<i64>()) else { continue };
        let text = rest[c2 + 1..].replace('|', "\\N");
        doc.events.push(Event {
            start: time::ms_from_frame(a, fps),
            end: time::ms_from_frame(b, fps),
            text,
            ..Event::default()
        });
    }
    doc
}

pub fn to_microdvd(doc: &Document, fps: f64) -> String {
    let mut out = String::new();
    for e in doc.events.iter().filter(|e| !e.comment) {
        out.push_str(&format!(
            "{{{}}}{{{}}}{}\n",
            time::frame_from_ms(e.start, fps),
            time::frame_from_ms(e.end, fps),
            plain_from_ass_text(&e.text, false).replace('\n', "|")
        ));
    }
    out
}

pub fn to_txt(doc: &Document) -> String {
    let mut out = String::new();
    for e in doc.events.iter().filter(|e| !e.comment) {
        out.push_str(&plain_from_ass_text(&e.text, false));
        out.push('\n');
    }
    out
}

/// Biçim bir dosya uzantısından seçilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ass,
    Srt,
    Vtt,
    MicroDvd,
    Txt,
}

impl Format {
    pub fn from_path(p: &str) -> Option<Format> {
        let ext = p.rsplit('.').next()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "ass" | "ssa" => Format::Ass,
            "srt" => Format::Srt,
            "vtt" => Format::Vtt,
            "sub" => Format::MicroDvd,
            "txt" => Format::Txt,
            _ => return None,
        })
    }
}

pub fn load(text: &str, fmt: Format, fps: f64) -> Document {
    match fmt {
        Format::Ass => Document::parse(text),
        Format::Srt => from_srt(text),
        Format::Vtt => from_vtt(text),
        Format::MicroDvd => from_microdvd(text, fps),
        Format::Txt => {
            let mut d = Document::new_default();
            for (i, l) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
                d.events.push(Event { start: i as i64 * 2000, end: i as i64 * 2000 + 2000, text: l.into(), ..Event::default() });
            }
            d
        }
    }
}

pub fn save(doc: &Document, fmt: Format, fps: f64) -> String {
    match fmt {
        Format::Ass => doc.to_ass_string(),
        Format::Srt => to_srt(doc),
        Format::Vtt => to_vtt(doc),
        Format::MicroDvd => to_microdvd(doc, fps),
        Format::Txt => to_txt(doc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nMerhaba <i>dünya</i>\r\nikinci satır\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nTek\r\n";
        let d = from_srt(srt);
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].text, "Merhaba {\\i1}dünya{\\i0}\\Nikinci satır");
        let back = to_srt(&d);
        assert!(back.contains("Merhaba <i>dünya</i>\nikinci satır"));
        assert_eq!(from_srt(&back).events, d.events);
    }
    #[test]
    fn vtt_short_times() {
        let d = from_vtt("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\nhi\n");
        assert_eq!(d.events[0].start, 1000);
        assert!(to_vtt(&d).contains("00:00:01.000 --> 00:00:02.000"));
    }
    #[test]
    fn microdvd() {
        let d = from_microdvd("{24}{48}a|b\n", 24.0);
        assert_eq!(d.events[0].start, 1000);
        assert_eq!(d.events[0].text, "a\\Nb");
        assert_eq!(to_microdvd(&d, 24.0), "{24}{48}a|b\n");
    }
}
