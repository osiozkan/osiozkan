//! asscore — subtitle document model and operations for Yuzu.
pub mod ass;
pub mod model;
pub mod ops;
pub mod subrip;
pub mod tags;
pub mod time;

pub use model::*;

/// Detect format by extension/content and load.
pub fn load_str(name: &str, content: &str) -> Script {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".srt") { subrip::parse_srt(content) }
    else if lower.ends_with(".vtt") { subrip::parse_vtt(content) }
    else { ass::parse(content) }
}

/// Decode bytes: UTF-8 (with/without BOM), UTF-16 LE/BE with BOM, fallback lossy.
pub fn decode_bytes(b: &[u8]) -> String {
    if b.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(b).into_owned()
    }
}
