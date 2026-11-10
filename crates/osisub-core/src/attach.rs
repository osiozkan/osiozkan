//! ASS ekleri: `[Fonts]` ve `[Graphics]` bölümlerindeki gömülü dosyalar (ASS'e özgü UUencode:
//! her 6 bit + 33, satır başına 80 karakter). Aegisub "Ekler" yöneticisinin karşılığı.
use crate::ass::{Entry, Script};

pub const SEC_FONTS: &str = "Fonts";
pub const SEC_GRAPHICS: &str = "Graphics";

#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub section: String,
    pub name: String,
    /// Kodlanmış verinin entries içindeki satırları (başlık hariç)
    pub size: usize,
}

pub fn encode(data: &[u8]) -> Vec<String> {
    let mut s = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let v = [b[0] >> 2, ((b[0] & 0x3) << 4) | (b[1] >> 4), ((b[1] & 0xf) << 2) | (b[2] >> 6), b[2] & 0x3f];
        let n = match ch.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for x in v.iter().take(n) {
            s.push((x + 33) as char);
        }
    }
    s.as_bytes().chunks(80).map(|c| String::from_utf8_lossy(c).into_owned()).collect()
}

pub fn decode(lines: &[&str]) -> Vec<u8> {
    let chars: Vec<u8> = lines.iter().flat_map(|l| l.trim().bytes()).filter(|b| (33..=96).contains(b)).map(|b| b - 33).collect();
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    for g in chars.chunks(4) {
        let c = |i: usize| *g.get(i).unwrap_or(&0);
        let b = [(c(0) << 2) | (c(1) >> 4), ((c(1) & 0xf) << 4) | (c(2) >> 2), ((c(2) & 0x3) << 6) | c(3)];
        let n = match g.len() {
            2 => 1,
            3 => 2,
            4 => 3,
            _ => 0,
        };
        out.extend_from_slice(&b[..n]);
    }
    out
}

fn header(raw: &str) -> Option<&str> {
    raw.strip_prefix("fontname:").or_else(|| raw.strip_prefix("filename:")).map(|s| s.trim())
}

/// (başlık girdi indeksi, son veri satırı indeksi + 1) aralıkları
fn spans(script: &Script) -> Vec<(String, String, usize, usize)> {
    let mut out = vec![];
    let mut cur: Option<(String, String, usize)> = None;
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Unknown { section, raw } = e else {
            if let Some((s, n, st)) = cur.take() {
                out.push((s, n, st, i));
            }
            continue;
        };
        if section != SEC_FONTS && section != SEC_GRAPHICS {
            if let Some((s, n, st)) = cur.take() {
                out.push((s, n, st, i));
            }
            continue;
        }
        if let Some(name) = header(raw) {
            if let Some((s, n, st)) = cur.take() {
                out.push((s, n, st, i));
            }
            cur = Some((section.clone(), name.to_string(), i));
        } else if cur.as_ref().map_or(false, |c| &c.0 != section) {
            let (s, n, st) = cur.take().unwrap();
            out.push((s, n, st, i));
        }
    }
    if let Some((s, n, st)) = cur.take() {
        out.push((s, n, st, script.entries.len()));
    }
    out
}

fn data_lines(script: &Script, a: usize, b: usize) -> Vec<&str> {
    script.entries[a + 1..b].iter().filter_map(|e| if let Entry::Unknown { raw, .. } = e { Some(raw.as_str()) } else { None }).filter(|r| !r.trim().is_empty()).collect()
}

pub fn list(script: &Script) -> Vec<Attachment> {
    spans(script).into_iter().map(|(section, name, a, b)| Attachment { size: decode(&data_lines(script, a, b)).len(), section, name }).collect()
}

pub fn extract(script: &Script, name: &str) -> Option<Vec<u8>> {
    spans(script).into_iter().find(|s| s.1 == name).map(|(_, _, a, b)| decode(&data_lines(script, a, b)))
}

pub fn remove(script: &mut Script, name: &str) -> bool {
    match spans(script).into_iter().find(|s| s.1 == name) {
        Some((_, _, a, b)) => {
            script.entries.drain(a..b);
            true
        }
        None => false,
    }
}

/// Dosyayı ekler (aynı adlı ek varsa değiştirir). Yazı tipleri (`ttf/otf/ttc`) `[Fonts]`, diğerleri
/// `[Graphics]`. ASS kuralı: yazı tipi adı `ad_0.ttf` biçiminde saklanır.
pub fn attach(script: &mut Script, file_name: &str, data: &[u8]) -> String {
    let lower = file_name.to_ascii_lowercase();
    let is_font = [".ttf", ".otf", ".ttc", ".otc"].iter().any(|e| lower.ends_with(e));
    let section = if is_font { SEC_FONTS } else { SEC_GRAPHICS };
    let name = if is_font {
        match file_name.rsplit_once('.') {
            Some((stem, ext)) if !stem.ends_with("_0") && !stem.chars().last().map_or(false, |c| c.is_ascii_digit()) => format!("{}_0.{}", stem, ext),
            _ => file_name.to_string(),
        }
    } else {
        file_name.to_string()
    };
    remove(script, &name);
    let mut block = vec![Entry::Unknown { section: section.to_string(), raw: format!("{}: {}", if is_font { "fontname" } else { "filename" }, name) }];
    block.extend(encode(data).into_iter().map(|l| Entry::Unknown { section: section.to_string(), raw: l }));
    // aynı bölümün sonuna, yoksa betiğin sonuna
    let at = script.entries.iter().rposition(|e| matches!(e, Entry::Unknown { section: s, .. } if s == section)).map(|i| i + 1).unwrap_or(script.entries.len());
    script.entries.splice(at..at, block);
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_roundtrip_all_tail_lengths() {
        for n in [0usize, 1, 2, 3, 4, 5, 59, 60, 61, 1000] {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 % 256) as u8).collect();
            let enc = encode(&data);
            assert!(enc.iter().all(|l| l.len() <= 80 && l.bytes().all(|b| (33..=96).contains(&b))));
            let refs: Vec<&str> = enc.iter().map(|s| s.as_str()).collect();
            assert_eq!(decode(&refs), data, "n={}", n);
        }
        // Bilinen örnek: "Man" → 6 bitlik 19,22,5,46 → +33
        assert_eq!(encode(b"Man"), vec!["47&O".to_string()]);
    }

    #[test]
    fn attach_list_extract_remove_and_survive_save() {
        let mut s = Script::new_default();
        let font: Vec<u8> = (0..500u32).map(|i| (i % 251) as u8).collect();
        let name = attach(&mut s, "Yazı.ttf", &font);
        assert_eq!(name, "Yazı_0.ttf");
        attach(&mut s, "logo.png", b"\x89PNG....");
        let l = list(&s);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].section.as_str(), l[0].size), (SEC_FONTS, 500));
        // dosyaya yaz → oku → aynı veri
        let back = Script::parse(&s.to_ass_string());
        assert_eq!(extract(&back, "Yazı_0.ttf").unwrap(), font);
        assert_eq!(extract(&back, "logo.png").unwrap(), b"\x89PNG....");
        let mut b2 = back.clone();
        assert!(remove(&mut b2, "Yazı_0.ttf"));
        assert_eq!(list(&b2).len(), 1);
        assert!(b2.to_ass_string().contains("[Graphics]"));
    }

    #[test]
    fn data_line_that_looks_like_section_header() {
        // "[" = 58+33, "]" = 60+33: veri satırı "[...]" biçiminde olabilir
        // ilk bayt 232 → ilk 6 bit 58 → '['; son bayt 0x3C → son 6 bit 60 → ']' (60 bayt = tam 80 karakter)
        let mut data: Vec<u8> = (0..60u32).map(|i| (i * 7 + 11) as u8).collect();
        data[0] = 232;
        data[59] = 0x3C;
        let l = &encode(&data)[0];
        assert!(l.len() == 80 && l.starts_with('[') && l.ends_with(']'), "{}", l);
        let mut s = Script::new_default();
        let mut all = data.clone();
        all.extend_from_slice(&[1, 2, 3, 4, 5]);
        attach(&mut s, "x.ttf", &all);
        let back = Script::parse(&s.to_ass_string());
        assert_eq!(extract(&back, "x_0.ttf").unwrap(), all);
    }
}
