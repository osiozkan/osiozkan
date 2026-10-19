//! ASS `[Fonts]`/`[Graphics]` attachments (ASS-flavoured uuencode) and `[Aegisub Extradata]`.
use crate::ass::{RawSection, Script};

pub const FONTS: &str = "Fonts";
pub const GRAPHICS: &str = "Graphics";
const EXTRADATA: &str = "Aegisub Extradata";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

/// One `Data: id,key,value` entry of `[Aegisub Extradata]`, key and value already decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraEntry {
    pub id: i64,
    pub key: String,
    pub value: String,
}

/// ASS uuencode: 6-bit groups offset by 33, no padding, line break after 80 characters.
pub fn uuencode(data: &[u8], line_breaks: bool) -> String {
    let mut out = String::with_capacity(data.len() * 4 / 3 + 8);
    let mut written = 0;
    let mut pos = 0;
    while pos < data.len() {
        let mut src = [0u8; 3];
        let n = (data.len() - pos).min(3);
        src[..n].copy_from_slice(&data[pos..pos + n]);
        let dst = [src[0] >> 2, ((src[0] & 3) << 4) | (src[1] >> 4), ((src[1] & 0xF) << 2) | (src[2] >> 6), src[2] & 0x3F];
        for d in dst.iter().take(n + 1) {
            out.push((d + 33) as char);
            written += 1;
            if line_breaks && written == 80 && pos + 3 < data.len() {
                written = 0;
                out.push_str("\r\n");
            }
        }
        pos += 3;
    }
    out
}

/// Inverse of [`uuencode`]; whitespace and other characters outside `!`..`` ` `` are skipped.
pub fn uudecode(text: &str) -> Vec<u8> {
    let chars: Vec<u8> = text.bytes().filter(|b| (33..=96).contains(b)).map(|b| b - 33).collect();
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    for g in chars.chunks(4) {
        if g.len() < 2 {
            break;
        }
        let s = |i: usize| g.get(i).copied().unwrap_or(0);
        let bytes = [(s(0) << 2) | (s(1) >> 4), ((s(1) & 0xF) << 4) | (s(2) >> 2), ((s(2) & 3) << 6) | s(3)];
        out.extend_from_slice(&bytes[..g.len() - 1]);
    }
    out
}

fn find_section<'a>(script: &'a Script, name: &str) -> Option<&'a RawSection> {
    script.raw_sections.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

fn header_name(line: &str) -> Option<String> {
    let (k, v) = line.trim().split_once(':')?;
    matches!(k.trim().to_ascii_lowercase().as_str(), "fontname" | "filename").then(|| v.trim().to_string())
}

impl Script {
    /// Decoded attachments of `[Fonts]` or `[Graphics]`.
    pub fn attachments(&self, section: &str) -> Vec<Attachment> {
        let Some(sec) = find_section(self, section) else { return vec![] };
        let mut out: Vec<(String, String)> = vec![];
        for l in &sec.lines {
            if let Some(n) = header_name(l) {
                out.push((n, String::new()));
            } else if let Some(last) = out.last_mut() {
                last.1.push_str(l.trim());
            }
        }
        out.into_iter().map(|(name, enc)| Attachment { name, data: uudecode(&enc) }).collect()
    }

    pub fn fonts(&self) -> Vec<Attachment> {
        self.attachments(FONTS)
    }

    pub fn graphics(&self) -> Vec<Attachment> {
        self.attachments(GRAPHICS)
    }

    /// Embed (or replace by name) an attachment, creating the section after the others if needed.
    pub fn embed_attachment(&mut self, section: &str, att: &Attachment) {
        let mut all = self.attachments(section);
        match all.iter_mut().find(|a| a.name == att.name) {
            Some(a) => a.data = att.data.clone(),
            None => all.push(att.clone()),
        }
        let key = if section.eq_ignore_ascii_case(GRAPHICS) { "filename" } else { "fontname" };
        let mut lines = vec![];
        for a in &all {
            lines.push(format!("{key}: {}", a.name));
            lines.extend(uuencode(&a.data, true).split("\r\n").map(String::from));
            lines.push(String::new());
        }
        match self.raw_sections.iter_mut().find(|s| s.name.eq_ignore_ascii_case(section)) {
            Some(s) => s.lines = lines,
            None => {
                self.raw_sections.push(RawSection { name: section.to_string(), lines });
                self.order.push(section.to_string());
            }
        }
    }

    /// Remove an attachment by name; drops the section when it becomes empty. Returns whether it existed.
    pub fn remove_attachment(&mut self, section: &str, name: &str) -> bool {
        let all = self.attachments(section);
        if !all.iter().any(|a| a.name == name) {
            return false;
        }
        self.raw_sections.retain(|s| !s.name.eq_ignore_ascii_case(section));
        self.order.retain(|n| !n.eq_ignore_ascii_case(section));
        for a in all.iter().filter(|a| a.name != name) {
            self.embed_attachment(section, a);
        }
        true
    }

    /// Parsed `Data:` entries of `[Aegisub Extradata]`. The section itself is kept verbatim in `raw_sections`.
    pub fn extradata(&self) -> Vec<ExtraEntry> {
        let Some(sec) = find_section(self, EXTRADATA) else { return vec![] };
        sec.lines
            .iter()
            .filter_map(|l| {
                let d = l.trim().strip_prefix("Data:")?;
                let mut it = d.trim_start().splitn(3, ',');
                let id = it.next()?.trim().parse().ok()?;
                let key = inline_decode(it.next()?);
                let value = decode_extra_value(it.next().unwrap_or(""));
                Some(ExtraEntry { id, key, value })
            })
            .collect()
    }

    pub fn extradata_get(&self, key: &str) -> Option<String> {
        self.extradata().into_iter().find(|e| e.key == key).map(|e| e.value)
    }
}

/// `#XX` hex escapes as used by Aegisub's inline string encoding.
fn inline_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Values start with `e` (inline escapes) or `u` (uuencode).
fn decode_extra_value(v: &str) -> String {
    let v = v.trim();
    match v.chars().next() {
        Some('e') => inline_decode(&v[1..]),
        Some('u') => String::from_utf8_lossy(&uudecode(&v[1..])).into_owned(),
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uu_known_vector() {
        assert_eq!(uuencode(b"Cat", false), "1W&U");
        assert_eq!(uuencode(b"a", false).len(), 2);
        assert_eq!(uuencode(b"ab", false).len(), 3);
        assert_eq!(uudecode("1W&U"), b"Cat");
    }

    #[test]
    fn uu_roundtrip_all_lengths() {
        let mut seed = 12345u32;
        for len in 0..400 {
            let data: Vec<u8> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 24) as u8
                })
                .collect();
            for lb in [false, true] {
                let enc = uuencode(&data, lb);
                assert_eq!(uudecode(&enc), data, "len {len}");
                assert!(!lb || enc.split("\r\n").all(|l| l.len() <= 80));
            }
        }
    }

    #[test]
    fn embed_extract_roundtrip_through_ass() {
        let mut s = Script::parse("[Script Info]\nTitle: x\n\n[Events]\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,hi\n");
        let font: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        s.embed_attachment(FONTS, &Attachment { name: "Arial_0.ttf".into(), data: font.clone() });
        s.embed_attachment(GRAPHICS, &Attachment { name: "logo.png".into(), data: vec![0x89, b'P', b'N', b'G', 0, 0] });
        let out = s.to_ass();
        assert!(out.contains("[Fonts]\r\nfontname: Arial_0.ttf\r\n"));
        let s2 = Script::parse(&out);
        assert_eq!(s2.fonts(), vec![Attachment { name: "Arial_0.ttf".into(), data: font }]);
        assert_eq!(s2.graphics()[0].name, "logo.png");
        assert_eq!(out, s2.to_ass());
        let mut s3 = s2.clone();
        assert!(s3.remove_attachment(FONTS, "Arial_0.ttf"));
        assert!(s3.fonts().is_empty());
        assert!(!s3.to_ass().contains("[Fonts]"));
    }

    #[test]
    fn extradata_entries() {
        let src = "[Script Info]\nTitle: t\n\n[Aegisub Extradata]\nData: 0,_aegi_perspective_ambient,e1#2C2\nData: 1,k#23,u&-F%\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";
        let s = Script::parse(src);
        let e = s.extradata();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], ExtraEntry { id: 0, key: "_aegi_perspective_ambient".into(), value: "1,2".into() });
        assert_eq!(e[1].key, "k#");
        assert_eq!(s.extradata_get("_aegi_perspective_ambient").as_deref(), Some("1,2"));
        let out = s.to_ass();
        assert!(out.contains("Data: 0,_aegi_perspective_ambient,e1#2C2\r\nData: 1,k#23,u&-F%\r\n"));
    }
}
