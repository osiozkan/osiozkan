//! Arayüz dili. Anahtar Türkçe metnin kendisidir: çevirisi olmayan metin Türkçe kalır,
//! böylece yeni bir düğme eklemek çeviriyi bozmaz (`i18n/en.jsonl` sonradan güncellenir).
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Tr = 0,
    En = 1,
}

static LANG: AtomicU8 = AtomicU8::new(0);

static EN: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| parse(include_str!("../i18n/en.jsonl")));

fn parse(src: &'static str) -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    for line in src.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok([k, v]) = serde_json::from_str::<[String; 2]>(line) {
            if !v.is_empty() {
                m.insert(&*Box::leak(k.into_boxed_str()), &*Box::leak(v.into_boxed_str()));
            }
        }
    }
    m
}

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::Tr }
}

/// Lang kodundan ("tr", "en"); bilinmeyen → Türkçe.
pub fn from_code(c: &str) -> Lang {
    if c.eq_ignore_ascii_case("en") { Lang::En } else { Lang::Tr }
}

pub fn code(l: Lang) -> &'static str {
    match l {
        Lang::Tr => "tr",
        Lang::En => "en",
    }
}

/// Arayüz metnini geçerli dile çevirir (çevirisi yoksa olduğu gibi).
pub fn tr(s: &'static str) -> &'static str {
    match lang() {
        Lang::Tr => s,
        Lang::En => EN.get(s).copied().unwrap_or(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tablo_gecerli() {
        let src = include_str!("../i18n/en.jsonl");
        let n = src.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(EN.len(), n, "en.jsonl'de bozuk ya da yinelenen satır var");
        for (k, v) in EN.iter() {
            // ASS kaçışları ve kısayollar çeviride korunmalı
            for tok in ["\\N", "Ctrl+", "{}"] {
                assert_eq!(k.matches(tok).count(), v.matches(tok).count(), "{tok} kayboldu: {k} → {v}");
            }
        }
    }

    /// Kaynakta `tr("…")` ile sarılmış her metnin İngilizcesi olmalı (yeni metin eklerken
    /// `i18n/en.jsonl`'ye satır ekleyin).
    #[test]
    fn kaynaktaki_metinlerin_cevirisi_var() {
        fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() { walk(&p, out) } else if p.extension().map_or(false, |x| x == "rs") { out.push(p) }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut missing = Vec::new();
        for f in files.iter().filter(|f| !f.ends_with("i18n.rs")) {
            let src = std::fs::read_to_string(f).unwrap();
            for (i, _) in src.match_indices("tr(\"") {
                if i > 0 && src.as_bytes()[i - 1].is_ascii_alphanumeric() || i > 0 && src.as_bytes()[i - 1] == b'_' { continue }
                let mut v = String::new();
                let mut it = src[i + 4..].chars();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' => match it.next() { Some('n') => v.push('\n'), Some('t') => v.push('\t'), Some(o) => v.push(o), None => {} },
                        _ => v.push(c),
                    }
                }
                if !EN.contains_key(v.as_str()) { missing.push(format!("{}: {v}", f.file_name().unwrap().to_string_lossy())) }
            }
        }
        assert!(missing.is_empty(), "çevirisi olmayan {} metin:\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn eksik_ceviri_turkce_kalir() {
        set_lang(Lang::En);
        assert_eq!(tr("bu metnin çevirisi yok"), "bu metnin çevirisi yok");
        set_lang(Lang::Tr);
        assert_eq!(tr("Dosya"), "Dosya");
    }
}
