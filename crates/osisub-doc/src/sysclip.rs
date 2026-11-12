//! Sistem panosu (Lua `clipboard.get/set` ve "Üzerine yapıştır" için). X11'de pano içeriği onu
//! sunan nesne yaşadığı sürece kalır; bu yüzden tek bir `Clipboard` süreç boyunca tutulur.
use std::sync::{Mutex, OnceLock};

fn cell() -> &'static Mutex<Option<arboard::Clipboard>> {
    static C: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(arboard::Clipboard::new().ok()))
}

/// Panodaki metin (pano yoksa/boşsa `None`).
pub fn get() -> Option<String> {
    let mut g = cell().lock().ok()?;
    g.as_mut()?.get_text().ok()
}

/// Panoya metin yazar; başarıysa `true`.
pub fn set(text: &str) -> bool {
    let Ok(mut g) = cell().lock() else { return false };
    g.as_mut().map_or(false, |c| c.set_text(text.to_string()).is_ok())
}
