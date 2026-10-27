//! Tiny UI translation layer: English source strings → Turkish.

use std::sync::atomic::{AtomicBool, Ordering};

static TURKISH: AtomicBool = AtomicBool::new(true);

pub fn set_turkish(on: bool) {
    TURKISH.store(on, Ordering::Relaxed);
}
pub fn is_turkish() -> bool {
    TURKISH.load(Ordering::Relaxed)
}

fn pref_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| std::path::PathBuf::from(h).join(".kaku").join("lang"))
}

pub fn save_pref(turkish: bool) {
    if let Some(p) = pref_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, if turkish { "tr" } else { "en" });
    }
}

/// Load the saved language (default Turkish).
pub fn load_pref() {
    if let Some(p) = pref_path() {
        if let Ok(s) = std::fs::read_to_string(p) {
            set_turkish(s.trim() != "en");
        }
    }
}

pub fn t(s: &'static str) -> &'static str {
    if !is_turkish() {
        return s;
    }
    match s {
        "Open…   Ctrl+O" => "Aç…   Ctrl+O",
        "Save   Ctrl+S" => "Kaydet   Ctrl+S",
        "Save as…" => "Farklı kaydet…",
        "New" => "Yeni",
        "Find / Replace…   Ctrl+F" => "Bul / Değiştir…   Ctrl+F",
        "Select all" => "Tümünü seç",
        "Insert line   Ctrl+I" => "Satır ekle   Ctrl+I",
        "Duplicate" => "Çoğalt",
        "Delete" => "Sil",
        "Split at middle" => "Ortadan böl",
        "Merge selected" => "Seçilenleri birleştir",
        "Sort by start time" => "Başlangıca göre sırala",
        "Fix common errors" => "Yaygın hataları düzelt",
        "Remove empty lines" => "Boş satırları sil",
        "Balance long lines" => "Uzun satırları dengele",
        "Remove hearing-impaired text" => "İşitme engelli metnini sil",
        "Check / collect fonts…" => "Fontları denetle / topla…",
        "Resample resolution…" => "Çözünürlüğü yeniden ölçekle…",
        "Remove duplicates" => "Yinelenenleri sil",
        "Shift times…" => "Zamanları kaydır…",
        "Timing post-processor…" => "Zamanlama son işlemcisi…",
        "Close small gaps (≤300 ms)" => "Küçük boşlukları kapat (≤300 ms)",
        "Enforce 2-frame gap (84 ms)" => "2 kare boşluk uygula (84 ms)",
        "Extend to reading speed" => "Okuma hızına uzat",
        "Open video / audio…" => "Video / ses aç…",
        "Auto-split selected by word" => "Seçileni kelimeye göre otomatik böl",
        "Auto-split selected by character" => "Seçileni harfe göre otomatik böl",
        "Remove karaoke tags" => "Karaoke etiketlerini sil",
        "Load Lua script…" => "Lua scripti yükle…",
        "Re-check" => "Yeniden denetle",
        "Import…" => "İçe aktar…",
        "Rename" => "Yeniden adlandır",
        "Find next" => "Sonrakini bul",
        "Replace" => "Değiştir",
        "Replace all" => "Tümünü değiştir",
        "Selected" => "Seçilenler",
        "All" => "Tümü",
        "From selected to end" => "Seçilenden sona",
        "Re-scan" => "Yeniden tara",
        "Collect font files to folder…" => "Font dosyalarını klasöre topla…",
        "Resample" => "Ölçekle",
        "Apply" => "Uygula",
        "File" => "Dosya",
        "Edit" => "Düzen",
        "Subtitles" => "Altyazı",
        "Change case (selected)" => "Harf boyutu (seçilen)",
        "Timing" => "Zamanlama",
        "Media" => "Medya",
        "Karaoke" => "Karaoke",
        "Automation" => "Otomasyon",
        "No line selected" => "Satır seçilmedi",
        "Layer" => "Katman",
        "Start" => "Başlangıç",
        "End" => "Bitiş",
        "Actor" => "Karakter",
        "Effect" => "Efekt",
        "Chars" => "Karakter",
        "Min ms" => "En az ms",
        "Max ms" => "En çok ms",
        "No styles" => "Stil yok",
        "Font" => "Yazı tipi",
        "Size" => "Boyut",
        "Bold / Italic" => "Kalın / İtalik",
        "Outline / Shadow" => "Kenar / Gölge",
        "Alignment" => "Hizalama",
        "Margins L R V" => "Kenar boşluğu S Sğ D",
        "Find" => "Bul",
        "Milliseconds" => "Milisaniye",
        "Lead-in (ms)" => "Öne çek (ms)",
        "Lead-out (ms)" => "Uzat (ms)",
        "Join gaps up to (ms)" => "Şu boşluğa kadar birleştir (ms)",
        "Find / Replace" => "Bul / Değiştir",
        "Shift times" => "Zamanları kaydır",
        "Fonts" => "Fontlar",
        "Resample resolution" => "Çözünürlüğü ölçekle",
        "Timing post-processor" => "Zamanlama son işlemcisi",
        "Automation log" => "Otomasyon günlüğü",
        "Lines" => "Satır",
        "Status" => "Durum",
        "Undo   Ctrl+Z" => "Geri al   Ctrl+Z",
        "Redo   Ctrl+Y" => "Yinele   Ctrl+Y",
        "▶ Play active line audio" => "▶ Etkin satırın sesini çal",
        "■ Stop" => "■ Durdur",
        "Snap all lines to keyframes (±250 ms)" => "Tüm satırları keyframe'e yapıştır (±250 ms)",
        "Turkish i/İ rules" => "Türkçe i/İ kuralları",
        "Snap dragging to keyframes (Alt = off)" => "Sürüklerken keyframe'e yapış (Alt = kapat)",
        "Waveform follows active line" => "Dalga formu etkin satırı izlesin",
        "Comment" => "Yorum",
        "Click video to set \\pos" => "\\pos için videoya tıkla",
        "Regex" => "Regex",
        "Match case" => "Büyük/küçük harf",
        "Whole word" => "Tam sözcük",
        other => other,
    }
}
