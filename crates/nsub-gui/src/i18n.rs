//! Tiny i18n layer: English strings are the keys, Turkish is a lookup table. Unknown keys fall back to English.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En = 0,
    Tr = 1,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::Tr
    } else {
        Lang::En
    }
}

pub fn detect() -> Lang {
    let v = std::env::var("LC_ALL").or_else(|_| std::env::var("LC_MESSAGES")).or_else(|_| std::env::var("LANG")).unwrap_or_default();
    if v.to_ascii_lowercase().starts_with("tr") {
        Lang::Tr
    } else {
        Lang::En
    }
}

pub fn code(l: Lang) -> &'static str {
    match l {
        Lang::En => "en",
        Lang::Tr => "tr",
    }
}

pub fn from_code(c: &str) -> Option<Lang> {
    match c.trim() {
        "en" => Some(Lang::En),
        "tr" => Some(Lang::Tr),
        _ => None,
    }
}

/// Translate a UI string.
pub fn tr(s: &'static str) -> &'static str {
    if lang() == Lang::En {
        return s;
    }
    turkish(s).unwrap_or(s)
}

fn turkish(s: &str) -> Option<&'static str> {
    Some(match s {
        "File" => "Dosya",
        "Edit" => "Düzen",
        "Timing" => "Zamanlama",
        "Automation" => "Otomasyon",
        "View" => "Görünüm",
        "New" => "Yeni",
        "Open…   Ctrl+O" => "Aç…   Ctrl+O",
        "Save   Ctrl+S" => "Kaydet   Ctrl+S",
        "Save as…" => "Farklı kaydet…",
        "Open video…" => "Video aç…",
        "Open audio…" => "Ses aç…",
        "Undo   Ctrl+Z" => "Geri al   Ctrl+Z",
        "Redo   Ctrl+Shift+Z" => "Yinele   Ctrl+Shift+Z",
        "Insert line after" => "Altına satır ekle",
        "Duplicate   Ctrl+D" => "Çoğalt   Ctrl+D",
        "Merge selected" => "Seçilenleri birleştir",
        "Delete   Del" => "Sil   Del",
        "Delete" => "Sil",
        "Find / Replace…   Ctrl+F" => "Bul / Değiştir…   Ctrl+F",
        "Shift times" => "Zamanları kaydır",
        "Find / Replace" => "Bul / Değiştir",
        "Shift times…" => "Zamanları kaydır…",
        "Sort by start time" => "Başlangıç zamanına göre sırala",
        "Load Lua script…" => "Lua betiği yükle…",
        "Reload scripts" => "Betikleri yeniden yükle",
        "(no macros loaded)" => "(yüklü makro yok)",
        "Video" => "Video",
        "Waveform" => "Dalga formu",
        "QC panel" => "Kalite paneli",
        "Styles manager" => "Stil yöneticisi",
        "Log" => "Günlük",
        "Quality" => "Kalite",
        "Styles" => "Stiller",
        "Subtitles" => "Altyazı",
        "Follow selection" => "Seçimi izle",
        "No line selected" => "Satır seçilmedi",
        "Comment" => "Yorum",
        "Layer" => "Katman",
        "Start" => "Başlangıç",
        "End" => "Bitiş",
        "Dur" => "Süre",
        "Style" => "Stil",
        "Actor" => "Konuşmacı",
        "Effect" => "Efekt",
        "L/R/V" => "Sol/Sağ/Dikey",
        "Max CPS" => "Maks. CPS",
        "Min duration (ms)" => "Min. süre (ms)",
        "Max duration (ms)" => "Maks. süre (ms)",
        "Max line length" => "Maks. satır uzunluğu",
        "Max lines" => "Maks. satır sayısı",
        "Min gap (ms)" => "Min. aralık (ms)",
        "Re-check" => "Yeniden denetle",
        "Fix overlaps / short / spaces" => "Çakışma / kısa süre / boşlukları düzelt",
        "Copy" => "Kopyala",
        "Name" => "Ad",
        "Font" => "Yazı tipi",
        "Size" => "Boyut",
        "Scale X/Y" => "Ölçek X/Y",
        "Spacing / Angle" => "Aralık / Açı",
        "Border style" => "Kenar stili",
        "Outline / Shadow" => "Çerçeve / Gölge",
        "Alignment" => "Hizalama",
        "Margins L/R/V" => "Kenar boşlukları S/S/D",
        "Milliseconds" => "Milisaniye",
        "All lines (otherwise selected)" => "Tüm satırlar (aksi halde seçililer)",
        "Apply" => "Uygula",
        "Find" => "Bul",
        "Replace" => "Değiştir",
        "Regex" => "Düzenli ifade",
        "Match case" => "Büyük/küçük harf",
        "Whole word" => "Tam sözcük",
        "Replace all" => "Tümünü değiştir",
        "Find next" => "Sonrakini bul",
        "Cancel" => "İptal",
        "OK" => "Tamam",
        "→ start" => "→ başlangıç",
        "→ end" => "→ bitiş",
        "set start = here" => "başlangıcı buraya ayarla",
        "set end = here" => "bitişi buraya ayarla",
        "◀ frame" => "◀ kare",
        "frame ▶" => "kare ▶",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_and_translates() {
        set_lang(Lang::En);
        assert_eq!(tr("File"), "File");
        set_lang(Lang::Tr);
        assert_eq!(tr("File"), "Dosya");
        assert_eq!(tr("Something new"), "Something new");
        set_lang(Lang::En);
    }
}
