//! Text decoding with Turkish-aware fallback (UTF-8 / UTF-16 / windows-1254 / ISO-8859-9).
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1252, WINDOWS_1254};

#[derive(Debug, Clone)]
pub struct Detected {
    pub name: &'static str,
    pub had_bom: bool,
    /// true when decoding hit invalid sequences and replacement chars were used.
    pub lossy: bool,
    /// true when double-encoded UTF-8 (`Ã¶`, `Ä±`) was repaired.
    pub mojibake_fixed: bool,
}

fn det(name: &'static str, had_bom: bool, lossy: bool) -> Detected {
    Detected { name, had_bom, lossy, mojibake_fixed: false }
}

pub fn decode(bytes: &[u8]) -> (String, Detected) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let (s, _, bad) = UTF_8.decode(rest);
        return finish_utf8(s.into_owned(), true, bad);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _, bad) = UTF_16LE.decode(&bytes[2..]);
        return (s.into_owned(), det("UTF-16LE", true, bad));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, bad) = UTF_16BE.decode(&bytes[2..]);
        return (s.into_owned(), det("UTF-16BE", true, bad));
    }
    if let Some(enc) = guess_utf16(bytes) {
        let (s, bad) = enc.decode_without_bom_handling(bytes);
        if !bad {
            return (s.into_owned(), det(enc.name(), false, false));
        }
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => return finish_utf8(s.to_string(), false, false),
        Err(_) => {
            if let Some(s) = decode_mixed(bytes) {
                return (s, det("UTF-8/windows-1254", false, false));
            }
        }
    }
    // Legacy 8-bit: pick Turkish if it yields Turkish-specific letters, else 1252.
    let enc = guess_legacy(bytes);
    let (s, bad) = enc.decode_without_bom_handling(bytes);
    (s.into_owned(), det(enc.name(), false, bad))
}

fn finish_utf8(s: String, had_bom: bool, lossy: bool) -> (String, Detected) {
    match repair_mojibake(&s) {
        Some(fixed) => (fixed, Detected { name: "UTF-8", had_bom, lossy, mojibake_fixed: true }),
        None => (s, det("UTF-8", had_bom, lossy)),
    }
}

/// UTF-16 without BOM: lots of NULs, all on one byte parity.
fn guess_utf16(bytes: &[u8]) -> Option<&'static Encoding> {
    if bytes.len() < 4 {
        return None;
    }
    let sample = &bytes[..bytes.len().min(4096) & !1];
    let (mut even, mut odd) = (0usize, 0usize);
    for (i, b) in sample.iter().enumerate() {
        if *b == 0 {
            if i % 2 == 0 {
                even += 1;
            } else {
                odd += 1;
            }
        }
    }
    let half = sample.len() / 2;
    if odd * 10 >= half * 3 && even * 20 <= half {
        Some(UTF_16LE)
    } else if even * 10 >= half * 3 && odd * 20 <= half {
        Some(UTF_16BE)
    } else {
        None
    }
}

/// Bytes that are only letters in Turkish text (ğ Ğ ı İ ş Ş in windows-1254 / ISO-8859-9).
fn is_turkish_specific(b: u8) -> bool {
    matches!(b, 0xD0 | 0xDD | 0xDE | 0xF0 | 0xFD | 0xFE)
}

/// Accented Latin letters common in western text but absent from Turkish.
fn is_western_only(b: u8) -> bool {
    matches!(b, 0xE0 | 0xE1 | 0xE3 | 0xE4 | 0xE5 | 0xE8 | 0xE9 | 0xEA | 0xEB | 0xEC | 0xED | 0xEF | 0xF1 | 0xF2 | 0xF3 | 0xF5 | 0xF9 | 0xFA | 0xDF)
}

fn guess_legacy(bytes: &[u8]) -> &'static Encoding {
    // In 1252 those map to ð Ð ý Ý þ Þ — rare in real western text.
    let tr = bytes.iter().filter(|b| is_turkish_specific(**b)).count();
    let west = bytes.iter().filter(|b| is_western_only(**b)).count();
    if tr > 0 && tr * 2 >= west {
        WINDOWS_1254
    } else {
        WINDOWS_1252
    }
}

/// Mostly UTF-8 with a few stray 8-bit bytes (a file edited by tools with different encodings):
/// valid sequences stay, invalid bytes are read as windows-1254. `None` if legacy bytes dominate.
fn decode_mixed(bytes: &[u8]) -> Option<String> {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    let (mut good, mut bad) = (0usize, 0usize);
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                good += s.chars().filter(|c| !c.is_ascii()).count();
                out.push_str(s);
                break;
            }
            Err(e) => {
                let (ok, after) = rest.split_at(e.valid_up_to());
                let s = std::str::from_utf8(ok).unwrap();
                good += s.chars().filter(|c| !c.is_ascii()).count();
                out.push_str(s);
                let n = e.error_len().unwrap_or(after.len());
                let (t, _) = WINDOWS_1254.decode_without_bom_handling(&after[..n]);
                out.push_str(&t);
                bad += n;
                rest = &after[n..];
            }
        }
    }
    (good > 0 && good >= bad * 2).then_some(out)
}

/// Byte a mojibake character stands for (windows-1252 / Latin-1 reading of the original byte).
fn mojibake_byte(c: char) -> Option<u8> {
    if (c as u32) < 0x100 {
        return Some(c as u32 as u8);
    }
    let mut buf = [0u8; 4];
    let (b, _, bad) = WINDOWS_1252.encode(c.encode_utf8(&mut buf));
    (!bad && b.len() == 1).then(|| b[0])
}

/// Repair UTF-8 that was decoded as windows-1252/1254/Latin-1 and saved again, run by run, so
/// partly broken text survives. `None` when nothing was repaired.
pub fn repair_mojibake(s: &str) -> Option<String> {
    if s.is_ascii() {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    let mut run: Vec<(char, u8)> = vec![];
    let mut changed = false;
    let mut flush = |run: &mut Vec<(char, u8)>, out: &mut String| {
        if run.is_empty() {
            return;
        }
        let bytes: Vec<u8> = run.iter().map(|x| x.1).collect();
        // the run is a candidate only if it begins with a UTF-8 lead byte
        match std::str::from_utf8(&bytes) {
            Ok(fixed) if bytes.iter().any(|b| *b >= 0xC2) && !fixed.chars().any(|c| c.is_control()) => {
                out.push_str(fixed);
                changed = true;
            }
            _ => out.extend(run.iter().map(|x| x.0)),
        }
        run.clear();
    };
    for c in s.chars() {
        match (c.is_ascii(), mojibake_byte(c)) {
            (false, Some(b)) => run.push((c, b)),
            _ => {
                flush(&mut run, &mut out);
                out.push(c);
            }
        }
    }
    flush(&mut run, &mut out);
    changed.then_some(out)
}

/// ISO-8859-9 = Latin-1 with six Turkish letters swapped in.
const ISO_8859_9: [(u8, char); 6] = [(0xD0, 'Ğ'), (0xDD, 'İ'), (0xDE, 'Ş'), (0xF0, 'ğ'), (0xFD, 'ı'), (0xFE, 'ş')];

fn iso_8859_9_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| ISO_8859_9.iter().find(|(x, _)| x == b).map(|(_, c)| *c).unwrap_or(*b as char)).collect()
}

fn iso_8859_9_encode(c: char) -> Option<u8> {
    if let Some((b, _)) = ISO_8859_9.iter().find(|(_, x)| *x == c) {
        return Some(*b);
    }
    let v = c as u32;
    (v < 0x100 && !ISO_8859_9.iter().any(|(b, _)| *b as u32 == v)).then_some(v as u8)
}

/// Canonical name for an encoding label (`utf8`, `latin5`, `cp1254`, ...). `None` if unknown.
pub fn canonical_name(label: &str) -> Option<&'static str> {
    let l = label.trim().to_ascii_lowercase().replace('_', "-");
    match l.as_str() {
        "utf-8" | "utf8" => return Some("UTF-8"),
        "utf-16" | "utf-16le" | "utf16" | "utf16le" => return Some("UTF-16LE"),
        "utf-16be" | "utf16be" => return Some("UTF-16BE"),
        "iso-8859-9" | "iso8859-9" | "latin5" | "latin-5" | "l5" | "iso-ir-148" => return Some("ISO-8859-9"),
        _ => {}
    }
    Encoding::for_label(l.as_bytes()).map(|e| e.name())
}

/// Decode with a forced encoding (for "reopen with encoding").
pub fn decode_as(bytes: &[u8], label: &str) -> Option<(String, Detected)> {
    let name = canonical_name(label)?;
    if name == "ISO-8859-9" {
        return Some((iso_8859_9_decode(bytes), det(name, false, false)));
    }
    let enc = Encoding::for_label(name.as_bytes())?;
    let (s, used, bad) = enc.decode(bytes);
    Some((s.into_owned(), det(name, used != enc, bad)))
}

/// Encode text with a named encoding (for exporting legacy Turkish files). Unmappable characters
/// become `?`-like replacements; use [`encode_strict`] to detect them.
pub fn encode(text: &str, name: &str) -> Vec<u8> {
    let enc = canonical_name(name).unwrap_or("UTF-8");
    match encode_strict(text, enc) {
        Ok(b) => b,
        Err(_) => match enc {
            "ISO-8859-9" => text.chars().map(|c| iso_8859_9_encode(c).unwrap_or(b'?')).collect(),
            _ => Encoding::for_label(enc.as_bytes()).unwrap_or(UTF_8).encode(text).0.into_owned(),
        },
    }
}

/// Encode, failing with the list of characters the encoding cannot represent.
pub fn encode_strict(text: &str, name: &str) -> Result<Vec<u8>, Vec<char>> {
    let enc = canonical_name(name).unwrap_or("UTF-8");
    match enc {
        "UTF-8" => Ok(text.as_bytes().to_vec()),
        "UTF-16LE" => Ok(text.encode_utf16().flat_map(u16::to_le_bytes).collect()),
        "UTF-16BE" => Ok(text.encode_utf16().flat_map(u16::to_be_bytes).collect()),
        "ISO-8859-9" => {
            let bad = unmappable(text, |c| iso_8859_9_encode(c).is_some());
            if bad.is_empty() {
                Ok(text.chars().filter_map(iso_8859_9_encode).collect())
            } else {
                Err(bad)
            }
        }
        _ => {
            let e = Encoding::for_label(enc.as_bytes()).unwrap_or(UTF_8);
            let (b, _, had_bad) = e.encode(text);
            if !had_bad {
                return Ok(b.into_owned());
            }
            Err(unmappable(text, |c| !e.encode(c.encode_utf8(&mut [0u8; 4])).2))
        }
    }
}

fn unmappable(text: &str, ok: impl Fn(char) -> bool) -> Vec<char> {
    let mut bad: Vec<char> = vec![];
    for c in text.chars() {
        if !ok(c) && !bad.contains(&c) {
            bad.push(c);
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    const TR: &str = "Işık ğüşöçİ";

    #[test]
    fn turkish_1254() {
        let (b, _, _) = WINDOWS_1254.encode(TR);
        let (s, d) = decode(&b);
        assert_eq!(s, TR);
        assert_eq!(d.name, "windows-1254");
    }

    #[test]
    fn bom() {
        let (s, d) = decode(b"\xEF\xBB\xBFhello");
        assert_eq!(s, "hello");
        assert!(d.had_bom);
    }

    #[test]
    fn utf8_without_bom() {
        let (s, d) = decode(format!("{TR}\nline").as_bytes());
        assert_eq!((s.as_str(), d.name, d.had_bom, d.mojibake_fixed), ("Işık ğüşöçİ\nline", "UTF-8", false, false));
    }

    #[test]
    fn iso_8859_9_roundtrip_and_distinct_from_1252() {
        let b = encode_strict(TR, "iso-8859-9").unwrap();
        assert_eq!(b, [0x49, 0xFE, 0xFD, 0x6B, 0x20, 0xF0, 0xFC, 0xFE, 0xF6, 0xE7, 0xDD]);
        let (s, d) = decode_as(&b, "latin5").unwrap();
        assert_eq!((s.as_str(), d.name), (TR, "ISO-8859-9"));
        // auto detection reads them as Turkish too
        assert_eq!(decode(&b).0, TR);
        // Latin-1 only characters that Turkish lacks cannot be written
        assert_eq!(encode_strict("ðþý", "iso-8859-9"), Err(vec!['ð', 'þ', 'ý']));
        assert_eq!(encode("ð", "iso-8859-9"), b"?");
    }

    #[test]
    fn western_text_stays_1252() {
        let (b, _, _) = WINDOWS_1252.encode("Café déjà vu, naïve señor");
        let (s, d) = decode(&b);
        assert_eq!(s, "Café déjà vu, naïve señor");
        assert_eq!(d.name, "windows-1252");
    }

    #[test]
    fn utf16_without_bom() {
        let text = format!("1\r\n00:00:01,000 --> 00:00:02,000\r\n{TR}\r\n");
        for (le, name) in [(true, "UTF-16LE"), (false, "UTF-16BE")] {
            let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| if le { u.to_le_bytes() } else { u.to_be_bytes() }).collect();
            let (s, d) = decode(&bytes);
            assert_eq!((s.as_str(), d.name, d.had_bom), (text.as_str(), name, false));
        }
    }

    #[test]
    fn mojibake_repaired() {
        let garbled = "KÃ¶pek ve Ä±ÅŸÄ±k, Ã‡ok gÃ¼zel Ä°stanbul ÄŸ";
        let (s, d) = decode(garbled.as_bytes());
        assert_eq!(s, "Köpek ve ışık, Çok güzel İstanbul ğ");
        assert!(d.mojibake_fixed);
        // partly fixed text with real Turkish letters next to the garbage
        let (s, _) = decode("ışık ve Ã¶rnek".as_bytes());
        assert_eq!(s, "ışık ve örnek");
        // legitimate accents must not be touched
        let (s, d) = decode("Café, Ã© rare".as_bytes());
        assert_eq!(d.mojibake_fixed, true);
        assert_eq!(s, "Café, é rare");
        let (s, d) = decode("Çok güzel é ö".as_bytes());
        assert_eq!((s.as_str(), d.mojibake_fixed), ("Çok güzel é ö", false));
    }

    #[test]
    fn mixed_utf8_and_legacy_bytes() {
        let mut b = "Merhaba dünya şarkı ığ ".as_bytes().to_vec();
        b.push(0xFE); // stray windows-1254 ş
        b.extend_from_slice(b" end");
        let (s, d) = decode(&b);
        assert_eq!(s, "Merhaba dünya şarkı ığ ş end");
        assert_eq!(d.name, "UTF-8/windows-1254");
    }

    #[test]
    fn strict_encoding_reports_unmappable() {
        assert_eq!(encode_strict("ışık", "windows-1252"), Err(vec!['ı', 'ş']));
        assert_eq!(encode_strict("ışık", "windows-1254").unwrap(), vec![0xFD, 0xFE, 0xFD, 0x6B]);
        assert_eq!(encode_strict("a", "utf-16be").unwrap(), vec![0, 0x61]);
        assert_eq!(canonical_name("CP1254"), Some("windows-1254"));
        assert_eq!(canonical_name("bogus"), None);
    }
}
