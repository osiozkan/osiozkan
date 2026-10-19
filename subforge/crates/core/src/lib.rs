//! SubForge core: ASS/SRT/VTT model, parsing, tags, timing and QC.
pub mod ass;
pub mod color;
pub mod encoding;
pub mod qc;
pub mod srt;
pub mod tags;
pub mod time;
pub mod tools;
pub mod timing;
pub mod turkish;

pub use ass::{Event, EventKind, Script, Style};
pub use time::Ms;

/// Subtitle formats SubForge can read/write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ass,
    Srt,
    Vtt,
}

impl Format {
    pub fn from_path(path: &str) -> Option<Format> {
        let p = path.to_ascii_lowercase();
        if p.ends_with(".ass") || p.ends_with(".ssa") {
            Some(Format::Ass)
        } else if p.ends_with(".srt") {
            Some(Format::Srt)
        } else if p.ends_with(".vtt") {
            Some(Format::Vtt)
        } else {
            None
        }
    }
}

/// Load any supported subtitle file from bytes (encoding auto-detected).
pub fn load_bytes(bytes: &[u8], format: Format) -> Result<(Script, encoding::Detected), String> {
    let (text, det) = encoding::decode(bytes);
    let script = match format {
        Format::Ass => Script::parse(&text),
        Format::Srt => srt::parse_srt(&text),
        Format::Vtt => srt::parse_vtt(&text),
    };
    Ok((script, det))
}

pub fn load_path(path: &str) -> Result<(Script, encoding::Detected), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let fmt = Format::from_path(path).unwrap_or(Format::Ass);
    load_bytes(&bytes, fmt)
}

/// Serialize in the given format.
pub fn serialize(script: &Script, format: Format) -> String {
    match format {
        Format::Ass => script.to_ass(),
        Format::Srt => srt::write_srt(script),
        Format::Vtt => srt::write_vtt(script),
    }
}

/// Save to disk. ASS gets a UTF-8 BOM (VSFilter needs it to detect UTF-8).
pub fn save_path(script: &Script, path: &str) -> Result<(), String> {
    let fmt = Format::from_path(path).unwrap_or(Format::Ass);
    let text = serialize(script, fmt);
    let mut out = Vec::with_capacity(text.len() + 3);
    if fmt != Format::Vtt {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    out.extend_from_slice(text.as_bytes());
    std::fs::write(path, out).map_err(|e| format!("{path}: {e}"))
}
