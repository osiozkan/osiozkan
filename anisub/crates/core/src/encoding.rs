//! Subtitle file decoding. Detects BOMs, then UTF-8, then falls back to
//! Windows-1254 (Turkish), the most common legacy encoding for Turkish subs.

use encoding_rs::{UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1254};

pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return UTF_8.decode_without_bom_handling(rest).0.into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return UTF_16LE.decode_without_bom_handling(rest).0.into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return UTF_16BE.decode_without_bom_handling(rest).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => WINDOWS_1254.decode_without_bom_handling(bytes).0.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detect() {
        assert_eq!(decode(b"\xEF\xBB\xBFabc"), "abc");
        assert_eq!(decode("şğı".as_bytes()), "şğı");
        // 0xFE = ş, 0xDD = İ in windows-1254
        assert_eq!(decode(&[0xFE, 0xDD]), "şİ");
        assert_eq!(decode(&[0xFF, 0xFE, b'a', 0]), "a");
    }
}
