//! Kitsune çekirdeği: ASS/SRT modeli, ayrıştırıcılar, zamanlama, etiketler, denetimler.
pub mod ass;
pub mod check;
pub mod karaoke;
pub mod srt;
pub mod tags;
pub mod time;

pub use ass::{Document, Event, EventKind, Style};

use std::path::Path;

/// Dosyayı uzantısına göre okur (.ass/.ssa/.srt/.vtt). Kodlama UTF-8 (BOM'lu/BOM'suz) varsayılır;
/// geçersizse kayıplı dönüştürülür.
pub fn load_file(path: &Path) -> Result<Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let text = decode(&bytes);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    Ok(match ext.as_str() {
        "srt" => srt::to_document(&srt::parse_srt(&text), (1920, 1080)),
        "vtt" => srt::to_document(&srt::parse_vtt(&text), (1920, 1080)),
        _ => Document::parse(&text),
    })
}

/// UTF-8 / UTF-16 (BOM'lu) çözer.
pub fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&u);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn save_file(doc: &Document, path: &Path) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let out = match ext.as_str() {
        "srt" => srt::write_srt(&srt::from_document(doc)),
        _ => doc.to_string().replace('\n', "\r\n"),
    };
    std::fs::write(path, out).map_err(|e| format!("{}: {}", path.display(), e))
}
