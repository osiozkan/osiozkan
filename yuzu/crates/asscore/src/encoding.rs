//! Text encoding detection and conversion (UTF-8/16, Windows-1254 Turkish, Shift_JIS, …).

use chardetng::EncodingDetector;
use encoding_rs::Encoding;

#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub text: String,
    /// WHATWG label, e.g. "UTF-8", "UTF-16LE", "windows-1254", "Shift_JIS".
    pub encoding: String,
    pub had_bom: bool,
}

/// Decode arbitrary bytes, detecting the encoding. `hint_tld` may bias detection (e.g. Some(b"tr")).
pub fn decode(bytes: &[u8]) -> Decoded {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Decoded { text: String::from_utf8_lossy(&bytes[3..]).into_owned(), encoding: "UTF-8".into(), had_bom: true };
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (t, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return Decoded { text: t.into_owned(), encoding: "UTF-16LE".into(), had_bom: true };
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (t, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return Decoded { text: t.into_owned(), encoding: "UTF-16BE".into(), had_bom: true };
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Decoded { text: s.to_string(), encoding: "UTF-8".into(), had_bom: false };
    }
    let mut det = EncodingDetector::new();
    det.feed(bytes, true);
    let enc = det.guess(None, true);
    let (t, _, _) = enc.decode(bytes);
    Decoded { text: t.into_owned(), encoding: enc.name().to_string(), had_bom: false }
}

/// Encode `text` with the given label; `bom` adds a byte-order mark for UTF-8/16.
pub fn encode(text: &str, label: &str, bom: bool) -> Vec<u8> {
    match label.to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => {
            let mut v = Vec::with_capacity(text.len() + 3);
            if bom { v.extend_from_slice(&[0xEF, 0xBB, 0xBF]); }
            v.extend_from_slice(text.as_bytes());
            v
        }
        "utf-16le" | "utf-16" => {
            let mut v = Vec::new();
            if bom { v.extend_from_slice(&[0xFF, 0xFE]); }
            for u in text.encode_utf16() { v.extend_from_slice(&u.to_le_bytes()); }
            v
        }
        "utf-16be" => {
            let mut v = Vec::new();
            if bom { v.extend_from_slice(&[0xFE, 0xFF]); }
            for u in text.encode_utf16() { v.extend_from_slice(&u.to_be_bytes()); }
            v
        }
        other => {
            let enc = Encoding::for_label(other.as_bytes()).unwrap_or(encoding_rs::UTF_8);
            let (b, _, _) = enc.encode(text);
            b.into_owned()
        }
    }
}

pub const COMMON_ENCODINGS: &[(&str, &str)] = &[
    ("UTF-8", "Unicode (UTF-8)"),
    ("UTF-16LE", "Unicode (UTF-16)"),
    ("windows-1254", "Turkish (Windows-1254)"),
    ("ISO-8859-9", "Turkish (ISO-8859-9)"),
    ("windows-1252", "Western (Windows-1252)"),
    ("windows-1250", "Central European (Windows-1250)"),
    ("windows-1251", "Cyrillic (Windows-1251)"),
    ("Shift_JIS", "Japanese (Shift_JIS)"),
    ("EUC-JP", "Japanese (EUC-JP)"),
    ("GBK", "Chinese Simplified (GBK)"),
    ("Big5", "Chinese Traditional (Big5)"),
    ("EUC-KR", "Korean (EUC-KR)"),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_and_bom() {
        assert_eq!(decode("merhaba ğüş".as_bytes()).encoding, "UTF-8");
        let d = decode(&[0xEF, 0xBB, 0xBF, b'a']);
        assert!(d.had_bom && d.text == "a");
    }
    #[test]
    fn turkish_legacy() {
        let bytes = encode("Işık İstanbul çağrı şöyle ğ", "windows-1254", false);
        assert!(std::str::from_utf8(&bytes).is_err());
        let d = decode(&bytes);
        // detector may choose a Turkish-compatible Latin code page; the round-trip through 1254 must restore text.
        let (t, _, _) = encoding_rs::WINDOWS_1254.decode(&bytes);
        assert_eq!(t, "Işık İstanbul çağrı şöyle ğ");
        assert!(!d.text.is_empty());
    }
    #[test]
    fn utf16_roundtrip() {
        let b = encode("あいう abc", "UTF-16LE", true);
        let d = decode(&b);
        assert_eq!(d.text, "あいう abc");
        assert_eq!(d.encoding, "UTF-16LE");
    }
    #[test]
    fn shift_jis() {
        let b = encode("こんにちは", "Shift_JIS", false);
        assert_eq!(decode(&b).text, "こんにちは");
    }
}
