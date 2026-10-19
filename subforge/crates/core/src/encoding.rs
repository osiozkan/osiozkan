//! Text decoding with Turkish-aware fallback (UTF-8 / UTF-16 / windows-1254).
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1252, WINDOWS_1254};

#[derive(Debug, Clone)]
pub struct Detected {
    pub name: &'static str,
    pub had_bom: bool,
    /// true when decoding hit invalid sequences and replacement chars were used.
    pub lossy: bool,
}

pub fn decode(bytes: &[u8]) -> (String, Detected) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let (s, _, bad) = UTF_8.decode(rest);
        return (s.into_owned(), Detected { name: "UTF-8", had_bom: true, lossy: bad });
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _, bad) = UTF_16LE.decode(&bytes[2..]);
        return (s.into_owned(), Detected { name: "UTF-16LE", had_bom: true, lossy: bad });
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, bad) = UTF_16BE.decode(&bytes[2..]);
        return (s.into_owned(), Detected { name: "UTF-16BE", had_bom: true, lossy: bad });
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), Detected { name: "UTF-8", had_bom: false, lossy: false });
    }
    // Legacy 8-bit: pick Turkish if it yields Turkish-specific letters, else 1252.
    let enc = guess_legacy(bytes);
    let (s, _, bad) = enc.decode(bytes);
    (s.into_owned(), Detected { name: enc.name(), had_bom: false, lossy: bad })
}

fn guess_legacy(bytes: &[u8]) -> &'static Encoding {
    // 0xFD ı, 0xDD İ, 0xFE ş, 0xDE Ş, 0xF0 ğ, 0xD0 Ğ in windows-1254.
    let turkish = bytes.iter().filter(|b| matches!(**b, 0xFD | 0xDD | 0xFE | 0xDE | 0xF0 | 0xD0)).count();
    // In 1252 those map to ý Ý þ Þ ð Ð — rare in real western text.
    if turkish > 0 {
        WINDOWS_1254
    } else {
        WINDOWS_1252
    }
}

/// Encode text with a named encoding (for exporting legacy Turkish files).
pub fn encode(text: &str, name: &str) -> Vec<u8> {
    let enc = Encoding::for_label(name.as_bytes()).unwrap_or(UTF_8);
    let (b, _, _) = enc.encode(text);
    b.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turkish_1254() {
        let (b, _, _) = WINDOWS_1254.encode("Işık ğüşöçİ");
        let (s, d) = decode(&b);
        assert_eq!(s, "Işık ğüşöçİ");
        assert_eq!(d.name, "windows-1254");
    }
    #[test]
    fn bom() {
        let (s, d) = decode(b"\xEF\xBB\xBFhello");
        assert_eq!(s, "hello");
        assert!(d.had_bom);
    }
}
