//! Text encoding detection/conversion for subtitle files (UTF-8/16 BOMs, Windows-1254 Turkish,
//! Shift-JIS, ... via chardetng). Legacy Turkish subtitles are very often Windows-1254.

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};

#[derive(Debug, Clone)]
pub struct Decoded {
    pub text: String,
    pub encoding: &'static str,
    pub had_errors: bool,
}

/// Decode bytes, detecting the encoding. `hint` (e.g. "windows-1254") overrides detection.
pub fn decode(bytes: &[u8], hint: Option<&str>) -> Decoded {
    if let Some(h) = hint.and_then(|h| Encoding::for_label(h.as_bytes())) {
        let (t, _, err) = h.decode(bytes);
        return Decoded { text: t.into_owned(), encoding: h.name(), had_errors: err };
    }
    if let Some((enc, _)) = Encoding::for_bom(bytes) {
        let (t, _, err) = enc.decode(bytes);
        return Decoded { text: t.into_owned(), encoding: enc.name(), had_errors: err };
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Decoded { text: s.to_string(), encoding: "UTF-8", had_errors: false };
    }
    // UTF-16 without BOM: lots of NULs
    if bytes.len() >= 4 {
        let nul_even = bytes.iter().step_by(2).filter(|b| **b == 0).count();
        let nul_odd = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
        let half = bytes.len() / 4;
        if nul_odd > half && nul_even == 0 {
            let (t, _, e) = UTF_16LE.decode(bytes);
            return Decoded { text: t.into_owned(), encoding: "UTF-16LE", had_errors: e };
        }
        if nul_even > half && nul_odd == 0 {
            let (t, _, e) = UTF_16BE.decode(bytes);
            return Decoded { text: t.into_owned(), encoding: "UTF-16BE", had_errors: e };
        }
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    det.feed(bytes, true);
    let enc = det.guess(None, chardetng::Utf8Detection::Allow);
    let (t, _, err) = enc.decode(bytes);
    Decoded { text: t.into_owned(), encoding: enc.name(), had_errors: err }
}

/// Encode text for saving. Unknown label -> UTF-8. `bom` only applies to UTF-8.
pub fn encode(text: &str, label: &str, bom: bool) -> Vec<u8> {
    let enc = Encoding::for_label(label.as_bytes()).unwrap_or(UTF_8);
    if enc == UTF_8 {
        let mut v = Vec::new();
        if bom {
            v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        v.extend_from_slice(text.as_bytes());
        return v;
    }
    if enc == UTF_16LE || enc == UTF_16BE {
        let mut v = if enc == UTF_16LE { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
        for u in text.encode_utf16() {
            v.extend_from_slice(&if enc == UTF_16LE { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        return v;
    }
    let (b, _, _) = enc.encode(text);
    b.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turkish_cp1254_roundtrip() {
        let s = "Şimdi gidiyoruz çocuklar, İstanbul'a. Işık ğüöç ı İ";
        let b = encode(s, "windows-1254", false);
        assert!(std::str::from_utf8(&b).is_err());
        let d = decode(&b, None);
        assert_eq!(d.text, s, "detected as {}", d.encoding);
        let d2 = decode(&b, Some("windows-1254"));
        assert_eq!(d2.text, s);
    }
    #[test]
    fn utf8_and_utf16() {
        let s = "Merhaba dünya çşğ";
        assert_eq!(decode(s.as_bytes(), None).encoding, "UTF-8");
        let b = encode(s, "utf-16le", false);
        assert_eq!(decode(&b, None).text, s);
        let b = encode(s, "utf-8", true);
        assert_eq!(decode(&b, None).text, s);
    }
}
