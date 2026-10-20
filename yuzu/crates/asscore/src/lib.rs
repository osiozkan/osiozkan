//! asscore — subtitle document model and operations for Yuzu.
pub mod ass;
pub mod compare;
pub mod dsp;
pub mod encoding;
pub mod extradata;
pub mod catalog;
pub mod karaoke;
pub mod lines;
pub mod fixes;
pub mod formats;
pub mod glossary;
pub mod model;
pub mod ops;
pub mod replace;
pub mod subrip;
pub mod sync;
pub mod tags;
pub mod typeset;
pub mod time;
pub mod transfer;

pub use model::*;

/// Detect format by extension/content and load.
pub fn load_str(name: &str, content: &str) -> Script {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".srt") { subrip::parse_srt(content) }
    else if lower.ends_with(".vtt") { subrip::parse_vtt(content) }
    else { ass::parse(content) }
}

/// Decode bytes with encoding detection (UTF-8/16 BOMs, legacy code pages such as Windows-1254).
pub fn decode_bytes(b: &[u8]) -> String {
    encoding::decode(b).text
}
