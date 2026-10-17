//! Aegisub-compatible `[Fonts]` / `[Graphics]` attachments.
//!
//! Each entry is `fontname: <name>` (or `filename: <name>` for graphics) followed by the file
//! bytes in Aegisub's uuencode variant: every 3 bytes become 4 characters, each a 6-bit value
//! plus 33, with at most 80 characters per line. A trailing 1 or 2 bytes give 2 or 3 characters.
//! The section is stored verbatim in `Script::raw_sections`.

use crate::Error;
use asscore::Script;
use std::path::Path;

/// Which attachment section to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachKind {
    Font,
    Graphic,
}

impl AttachKind {
    /// Section header line.
    pub fn header(self) -> &'static str {
        match self {
            AttachKind::Font => "[Fonts]",
            AttachKind::Graphic => "[Graphics]",
        }
    }
    /// Key introducing each entry.
    pub fn key(self) -> &'static str {
        match self {
            AttachKind::Font => "fontname",
            AttachKind::Graphic => "filename",
        }
    }
}

const LINE: usize = 80;

/// Encode bytes into lines of at most 80 characters.
pub fn encode(data: &[u8]) -> Vec<String> {
    let mut all = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let v = [b[0] >> 2, ((b[0] & 3) << 4) | (b[1] >> 4), ((b[1] & 15) << 2) | (b[2] >> 6), b[2] & 63];
        for x in v.iter().take(ch.len() + 1) {
            all.push((x + 33) as char);
        }
    }
    all.as_bytes().chunks(LINE).map(|c| String::from_utf8_lossy(c).into_owned()).collect()
}

/// Decode lines produced by [`encode`]. Characters outside `!`..=`` ` `` (whitespace, `\r`) are ignored.
pub fn decode<S: AsRef<str>>(lines: &[S]) -> Vec<u8> {
    let vals: Vec<u8> = lines
        .iter()
        .flat_map(|l| l.as_ref().bytes())
        .filter(|b| (33..97).contains(b))
        .map(|b| b - 33)
        .collect();
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for ch in vals.chunks(4) {
        let c = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0), *ch.get(3).unwrap_or(&0)];
        let b = [(c[0] << 2) | (c[1] >> 4), (c[1] << 4) | (c[2] >> 2), (c[2] << 6) | c[3]];
        match ch.len() {
            4 => out.extend_from_slice(&b),
            3 => out.extend_from_slice(&b[..2]),
            2 => out.push(b[0]),
            _ => {}
        }
    }
    out
}

fn is_entry_start(line: &str) -> Option<&str> {
    let t = line.trim_start();
    for k in ["fontname:", "filename:"] {
        if t.len() >= k.len() && t.is_char_boundary(k.len()) && t[..k.len()].eq_ignore_ascii_case(k) {
            return Some(t[k.len()..].trim());
        }
    }
    None
}

fn parse_entries(lines: &[String]) -> Vec<(String, Vec<String>)> {
    let mut v: Vec<(String, Vec<String>)> = Vec::new();
    for l in lines {
        if let Some(name) = is_entry_start(l) {
            v.push((name.to_string(), Vec::new()));
        } else if let Some(last) = v.last_mut() {
            if !l.trim().is_empty() {
                last.1.push(l.trim().to_string());
            }
        }
    }
    v
}

fn find_section(script: &Script, kind: AttachKind) -> Option<usize> {
    script.raw_sections.iter().position(|(h, _)| h.trim().eq_ignore_ascii_case(kind.header()))
}

fn store(script: &mut Script, kind: AttachKind, entries: Vec<(String, Vec<String>)>) {
    let mut lines = Vec::new();
    for (name, data) in entries {
        lines.push(format!("{}: {}", kind.key(), name));
        lines.extend(data);
    }
    match (find_section(script, kind), lines.is_empty()) {
        (Some(i), true) => {
            script.raw_sections.remove(i);
        }
        (Some(i), false) => script.raw_sections[i].1 = lines,
        (None, false) => script.raw_sections.push((kind.header().to_string(), lines)),
        (None, true) => {}
    }
}

/// Add (or replace, matching the name case-insensitively) an attachment.
pub fn attach(script: &mut Script, kind: AttachKind, name: &str, data: &[u8]) {
    let mut entries = attachments_raw(script, kind);
    entries.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    entries.push((name.to_string(), encode(data)));
    store(script, kind, entries);
}

fn attachments_raw(script: &Script, kind: AttachKind) -> Vec<(String, Vec<String>)> {
    find_section(script, kind).map(|i| parse_entries(&script.raw_sections[i].1)).unwrap_or_default()
}

/// Decoded attachments of a kind as `(name, bytes)`.
pub fn attachments(script: &Script, kind: AttachKind) -> Vec<(String, Vec<u8>)> {
    attachments_raw(script, kind).into_iter().map(|(n, d)| (n, decode(&d))).collect()
}

/// Remove one attachment by name (case-insensitive). Drops the section if it becomes empty.
pub fn remove_attachment(script: &mut Script, kind: AttachKind, name: &str) -> bool {
    let mut entries = attachments_raw(script, kind);
    let before = entries.len();
    entries.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    if entries.len() == before {
        return false;
    }
    store(script, kind, entries);
    true
}

/// Entry name Aegisub-style: `<family>_<B><I>0.ttf` where `0` is the charset id.
fn font_entry_name(data: &[u8], ext: &str) -> Result<String, Error> {
    let face = ttf_parser::Face::parse(data, 0).map_err(|e| Error::BadFont(e.to_string()))?;
    let family = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode())
        .find_map(|n| n.to_string())
        .ok_or_else(|| Error::BadFont("no family name".into()))?;
    let safe: String = family.chars().map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':') { '_' } else { c }).collect();
    Ok(format!(
        "{}_{}{}0.{}",
        safe,
        if face.is_bold() { "B" } else { "" },
        if face.is_italic() { "I" } else { "" },
        ext
    ))
}

/// Embed a font file into `[Fonts]`. Returns the entry name, e.g. `Arial_B0.ttf`
/// (`.otf` files keep `.otf`, everything else gets `.ttf`). For collections face 0 names the
/// entry and the whole file is embedded.
pub fn embed(script: &mut Script, font_file: &Path) -> Result<String, Error> {
    let data = std::fs::read(font_file)?;
    let is_otf = font_file.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("otf"));
    let name = font_entry_name(&data, if is_otf { "otf" } else { "ttf" })?;
    attach(script, AttachKind::Font, &name, &data);
    Ok(name)
}

/// Embed any image file into `[Graphics]` under its file name.
pub fn embed_graphic(script: &mut Script, file: &Path) -> Result<String, Error> {
    let data = std::fs::read(file)?;
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
    attach(script, AttachKind::Graphic, &name, &data);
    Ok(name)
}

/// Decoded embedded fonts as `(entry name, file bytes)`.
pub fn extract_embedded(script: &Script) -> Vec<(String, Vec<u8>)> {
    attachments(script, AttachKind::Font)
}

/// Decoded embedded graphics as `(entry name, file bytes)`.
pub fn extract_graphics(script: &Script) -> Vec<(String, Vec<u8>)> {
    attachments(script, AttachKind::Graphic)
}

/// Remove an embedded font by entry name (case-insensitive); true if something was removed.
pub fn remove_embedded(script: &mut Script, name: &str) -> bool {
    remove_attachment(script, AttachKind::Font, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asscore::ass;

    #[test]
    fn known_vectors() {
        assert_eq!(encode(b"Cat"), vec!["1W&U"]);
        assert_eq!(encode(&[0, 0, 0]), vec!["!!!!"]);
        assert_eq!(encode(&[255, 255, 255]), vec!["````"]);
        assert_eq!(encode(&[0xFF]), vec!["`Q"]);
        assert_eq!(encode(&[0xFF, 0xFF]), vec!["``]"]);
        assert!(encode(&[]).is_empty());
    }

    #[test]
    fn roundtrip_all_lengths_and_line_width() {
        for n in 0..400usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let lines = encode(&data);
            assert!(lines.iter().all(|l| l.len() <= 80 && l.bytes().all(|b| (33..97).contains(&b))));
            if lines.len() > 1 {
                assert!(lines[..lines.len() - 1].iter().all(|l| l.len() == 80));
            }
            assert_eq!(decode(&lines), data, "len {n}");
        }
        // 60 bytes -> exactly one 80 char line
        assert_eq!(encode(&[7u8; 60]).len(), 1);
        assert_eq!(encode(&[7u8; 61]).len(), 2);
    }

    #[test]
    fn attach_extract_replace_remove() {
        let mut s = Script::new();
        attach(&mut s, AttachKind::Font, "a_0.ttf", b"hello fonts");
        attach(&mut s, AttachKind::Font, "b_B0.ttf", &[1, 2, 3, 4, 5]);
        attach(&mut s, AttachKind::Graphic, "logo.png", b"\x89PNG");
        assert_eq!(s.raw_sections.len(), 2);
        assert_eq!(s.raw_sections[0].0, "[Fonts]");
        assert_eq!(s.raw_sections[0].1[0], "fontname: a_0.ttf");
        assert_eq!(s.raw_sections[1].1[0], "filename: logo.png");
        let f = extract_embedded(&s);
        assert_eq!(f, vec![("a_0.ttf".to_string(), b"hello fonts".to_vec()), ("b_B0.ttf".to_string(), vec![1, 2, 3, 4, 5])]);
        assert_eq!(extract_graphics(&s)[0].1, b"\x89PNG");
        attach(&mut s, AttachKind::Font, "A_0.TTF", b"new");
        assert_eq!(extract_embedded(&s).len(), 2);
        assert_eq!(extract_embedded(&s)[1].1, b"new");
        assert!(remove_embedded(&mut s, "b_b0.ttf"));
        assert!(!remove_embedded(&mut s, "b_b0.ttf"));
        assert!(remove_embedded(&mut s, "a_0.ttf"));
        assert_eq!(s.raw_sections.len(), 1, "empty [Fonts] section removed");
        assert!(extract_embedded(&s).is_empty());
    }

    #[test]
    fn ass_roundtrip_keeps_sections_intact() {
        let mut s = Script::new();
        s.events.push(asscore::Event::new(0, 1000, "Default", "x"));
        let big: Vec<u8> = (0..1000u32).map(|i| (i * 31 % 251) as u8).collect();
        attach(&mut s, AttachKind::Font, "Foo_B0.ttf", &big);
        attach(&mut s, AttachKind::Font, "Bar_I0.ttf", &big[..77]);
        attach(&mut s, AttachKind::Graphic, "pic.jpg", &big[..500]);
        let text = ass::write(&s);
        assert!(text.contains("\n[Fonts]\nfontname: Foo_B0.ttf\n"));
        let p = ass::parse(&text);
        assert_eq!(p.raw_sections, s.raw_sections);
        assert_eq!(extract_embedded(&p)[0].1, big);
        assert_eq!(extract_graphics(&p)[0].1, &big[..500]);
        let text2 = ass::write(&p);
        assert_eq!(text, text2);
    }

    #[test]
    fn parses_aegisub_style_file() {
        let src = "[Script Info]\nTitle: t\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,x\n\n[Fonts]\nfontname: arial_0.ttf\n1W&U\n!!!!\n\r\nfontname: x_B0.ttf\n``\n\n[Graphics]\nfilename: a.png\n```\n";
        let s = ass::parse(src);
        let f = extract_embedded(&s);
        assert_eq!(f[0], ("arial_0.ttf".to_string(), vec![b'C', b'a', b't', 0, 0, 0]));
        assert_eq!(f[1], ("x_B0.ttf".to_string(), vec![0xFF]));
        assert_eq!(extract_graphics(&s)[0].1, vec![0xFF, 0xFF]);
    }

    #[test]
    fn embed_rejects_non_font_and_names_real_font() {
        let dir = std::env::temp_dir().join(format!("yuzu-fonts-embed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.ttf");
        std::fs::write(&bad, b"not a font").unwrap();
        let mut s = Script::new();
        assert!(matches!(embed(&mut s, &bad), Err(Error::BadFont(_))));
        assert!(s.raw_sections.is_empty());
        let _ = std::fs::remove_dir_all(&dir);

        // With a real system font (skipped when none is installed).
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let path = db.faces().find_map(|f| match &f.source {
            fontdb::Source::File(p) if f.index == 0 && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf")) => Some(p.clone()),
            _ => None,
        });
        let Some(path) = path else {
            eprintln!("no system ttf; skipping");
            return;
        };
        let name = embed(&mut s, &path).unwrap();
        assert!(name.ends_with("0.ttf") && name.contains('_'));
        assert_eq!(extract_embedded(&s)[0].1, std::fs::read(&path).unwrap());
    }
}
