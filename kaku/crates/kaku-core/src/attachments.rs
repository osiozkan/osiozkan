//! Embedded fonts/graphics in ASS (`[Fonts]`/`[Graphics]`): the 6-bit "UUencode" flavour used by Aegisub/VSFilter.

use crate::ass::{Entry, Script};

/// Encode raw bytes into ASS attachment lines (80 chars wide, `+33` offset).
pub fn encode(data: &[u8]) -> Vec<String> {
    let mut chars = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let v = [b[0] >> 2, ((b[0] & 3) << 4) | (b[1] >> 4), ((b[1] & 15) << 2) | (b[2] >> 6), b[2] & 63];
        for &x in &v[..chunk.len() + 1] {
            chars.push((x + 33) as char);
        }
    }
    chars.as_bytes().chunks(80).map(|c| String::from_utf8_lossy(c).to_string()).collect()
}

pub fn decode(lines: &[&str]) -> Vec<u8> {
    let vals: Vec<u8> = lines.iter().flat_map(|l| l.trim_end().bytes()).filter(|b| (33..97).contains(b)).map(|b| b - 33).collect();
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let c = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0), *chunk.get(3).unwrap_or(&0)];
        let b = [(c[0] << 2) | (c[1] >> 4), ((c[1] & 15) << 4) | (c[2] >> 2), ((c[2] & 3) << 6) | c[3]];
        out.extend_from_slice(&b[..chunk.len().saturating_sub(1)]);
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub section: String,
    pub name: String,
    pub data: Vec<u8>,
}

/// Collect attachments from the raw `[Fonts]`/`[Graphics]` sections.
pub fn list(script: &Script) -> Vec<Attachment> {
    let mut out: Vec<Attachment> = Vec::new();
    let mut cur: Option<(String, String, Vec<String>)> = None;
    let flush = |cur: &mut Option<(String, String, Vec<String>)>, out: &mut Vec<Attachment>| {
        if let Some((sec, name, lines)) = cur.take() {
            let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
            out.push(Attachment { section: sec, name, data: decode(&refs) });
        }
    };
    for e in &script.entries {
        if let Entry::Raw { section, text } = e {
            if !(section.eq_ignore_ascii_case("Fonts") || section.eq_ignore_ascii_case("Graphics")) {
                continue;
            }
            let t = text.trim();
            if let Some(n) = t.strip_prefix("fontname:").or_else(|| t.strip_prefix("filename:")) {
                flush(&mut cur, &mut out);
                cur = Some((section.clone(), n.trim().to_string(), vec![]));
            } else if let Some((_, _, lines)) = cur.as_mut() {
                lines.push(t.to_string());
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Embed a file. `name` is the stored file name; fonts get Aegisub's `_0` charset suffix on the header line.
pub fn embed(script: &mut Script, name: &str, data: &[u8], is_font: bool) {
    let (section, key, stored) = if is_font {
        let stem = std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = std::path::Path::new(name).extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        ("Fonts", "fontname", format!("{stem}_0.{ext}"))
    } else {
        ("Graphics", "filename", name.to_string())
    };
    // Remove an existing attachment with the same name.
    remove(script, name);
    script.entries.push(Entry::Raw { section: section.into(), text: format!("{key}: {stored}") });
    for l in encode(data) {
        script.entries.push(Entry::Raw { section: section.into(), text: l });
    }
}

/// Remove attachment(s) whose stored name equals `name` (ignoring the `_0` suffix).
pub fn remove(script: &mut Script, name: &str) -> bool {
    let norm = |n: &str| n.replace("_0.", ".").to_lowercase();
    let target = norm(name);
    let mut removing = false;
    let mut removed = false;
    script.entries.retain(|e| {
        if let Entry::Raw { section, text } = e {
            if section.eq_ignore_ascii_case("Fonts") || section.eq_ignore_ascii_case("Graphics") {
                let t = text.trim();
                if let Some(n) = t.strip_prefix("fontname:").or_else(|| t.strip_prefix("filename:")) {
                    removing = norm(n.trim()) == target;
                }
                if removing {
                    removed = true;
                    return false;
                }
            }
        }
        true
    });
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_lengths() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let enc = encode(&data);
            let refs: Vec<&str> = enc.iter().map(|s| s.as_str()).collect();
            assert_eq!(decode(&refs), data, "len {len}");
        }
        let big: Vec<u8> = (0..10_000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        let enc = encode(&big);
        assert!(enc.iter().all(|l| l.len() <= 80));
        let refs: Vec<&str> = enc.iter().map(|s| s.as_str()).collect();
        assert_eq!(decode(&refs), big);
    }

    #[test]
    fn embed_list_remove() {
        let mut s = Script::new_default();
        embed(&mut s, "My Font.ttf", &[1, 2, 3, 4, 5, 250], true);
        let text = s.to_string();
        assert!(text.contains("[Fonts]\nfontname: My Font_0.ttf\n"));
        let s2 = Script::parse(&text);
        let l = list(&s2);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].data, vec![1, 2, 3, 4, 5, 250]);
        let mut s3 = s2.clone();
        assert!(remove(&mut s3, "My Font.ttf"));
        assert!(list(&s3).is_empty());
    }
}
