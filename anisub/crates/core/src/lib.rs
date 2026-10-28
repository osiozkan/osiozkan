pub mod ass;
pub mod color;
pub mod encoding;
pub mod ops;
pub mod qc;
pub mod srt;
pub mod tags;
pub mod time;
pub mod turkish;

use std::path::Path;

/// Load any supported subtitle file (ASS/SSA/SRT) with automatic encoding detection.
pub fn load(path: &Path) -> std::io::Result<ass::AssFile> {
    let text = encoding::decode(&std::fs::read(path)?);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    Ok(match ext.as_str() {
        "srt" => srt::parse(&text),
        _ => ass::AssFile::parse(&text),
    })
}

/// Save as ASS/SSA or SRT depending on extension (UTF-8, no BOM).
pub fn save(file: &ass::AssFile, path: &Path) -> std::io::Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let out = match ext.as_str() {
        "srt" => srt::write(file),
        _ => file.to_ass_string(),
    };
    std::fs::write(path, out)
}
