//! Gereksiz etiketleri sil (Aegisub "Clean Tags" betiğinin güvenli karşılığı).
//!
//! 1. Aynı blokta aynı etiket birden çok kez varsa sonuncusu geçerlidir → öncekiler silinir
//!    (`{\fs20\fs30}` → `{\fs30}`; `\c` = `\1c`, `\fr` = `\frz` eşanlamlıdır).
//! 2. Önceki bloklarda zaten aynı değere ayarlanmış etiket tekrarlanırsa silinir
//!    (`{\i1}a{\i1}b` → `{\i1}a{}b` → boş blok atılır).
//!
//! Güvenlik: `\t` içeren bloklar ve sonrasındaki durum izlenmez (canlandırma değeri değiştirir);
//! `\r` durumu sıfırlar; ilk geçerli olan etiketler (`\pos \move \org \an \a \fad \fade \clip
//! \iclip \q`) ile karaoke (`\k…`), çizim (`\p`) ve not blokları hiç değiştirilmez.
use crate::overrides::{self, Seg, Tag};
use std::collections::HashMap;

/// Değeri sonraki etiketle ezilen ve satır boyunca durum olarak izlenebilen etiketler.
const STATEFUL: &[&str] = &[
    "fs", "fn", "b", "i", "u", "s", "bord", "xbord", "ybord", "shad", "xshad", "yshad", "be", "blur",
    "fscx", "fscy", "fsp", "frx", "fry", "frz", "fax", "fay", "1c", "2c", "3c", "4c", "1a", "2a", "3a", "4a", "alpha", "fe",
];

fn canon(name: &str) -> &str {
    match name {
        "c" => "1c",
        "fr" => "frz",
        n => n,
    }
}

fn norm_arg(a: &str) -> String {
    a.trim().trim_matches(|c| c == '(' || c == ')').trim().to_ascii_uppercase()
}

/// Temizlenmiş metin; değişiklik yoksa `None`.
pub fn clean(text: &str) -> Option<String> {
    let segs = overrides::parse(text);
    // \alpha tüm saydamlıkları ayarlar: izlemeyi karıştırmamak için alpha ile 1a-4a birlikte
    // görülürse yalnızca blok içi tekrar silinir (durum izleme alpha ailesinde kapatılır).
    let mut state: HashMap<String, String> = HashMap::new();
    let mut tracking = true;
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    for seg in segs {
        let Seg::Block(tags) = seg else { out.push(seg); continue };
        if tags.iter().any(|t| t.name.is_empty()) || tags.is_empty() {
            out.push(Seg::Block(tags)); // not bloğu / boş blok: dokunma
            continue;
        }
        let has_t = tags.iter().any(|t| t.name == "t");
        let is_draw = tags.iter().any(|t| t.name == "p" && t.number().unwrap_or(0.0) > 0.0);
        // 1) blok içi tekrarlar (\t yoksa): sondan başa, görülen adı atla
        let mut keep = vec![true; tags.len()];
        if !has_t {
            let mut seen: Vec<String> = vec![];
            for (i, t) in tags.iter().enumerate().rev() {
                let n = canon(&t.name);
                if !STATEFUL.contains(&n) {
                    if n == "r" { break; } // \r öncesini olduğu gibi bırak
                    continue;
                }
                if seen.iter().any(|s| s == n) {
                    keep[i] = false;
                } else {
                    seen.push(n.to_string());
                }
            }
        }
        // 2) bloklar arası: önceki değerle aynıysa sil
        let mut kept: Vec<Tag> = vec![];
        for (i, t) in tags.into_iter().enumerate() {
            if !keep[i] { continue; }
            let n = canon(&t.name).to_string();
            if n == "r" {
                state.clear();
                tracking = true;
                kept.push(t);
                continue;
            }
            if n == "t" || n == "alpha" {
                if n == "t" { tracking = false; }
                state.retain(|k, _| !(n == "alpha" && k.ends_with('a')));
                kept.push(t);
                continue;
            }
            if tracking && !has_t && !is_draw && STATEFUL.contains(&n.as_str()) {
                let v = norm_arg(&t.arg);
                if state.get(&n) == Some(&v) { continue; }
                state.insert(n, v);
            }
            kept.push(t);
        }
        if !kept.is_empty() {
            out.push(Seg::Block(kept));
        }
    }
    // silinen bloklardan sonra bitişik metin parçalarını birleştir
    let mut merged: Vec<Seg> = vec![];
    for s in out {
        match (merged.last_mut(), s) {
            (Some(Seg::Text(a)), Seg::Text(b)) => a.push_str(&b),
            (_, s) => merged.push(s),
        }
    }
    let nt = overrides::to_string(&merged);
    (nt != text).then_some(nt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(t: &str) -> String {
        clean(t).unwrap_or_else(|| t.to_string())
    }

    #[test]
    fn blok_ici_tekrar() {
        assert_eq!(c("{\\fs20\\bord2\\fs30}a"), "{\\bord2\\fs30}a");
        assert_eq!(c("{\\c&H00FF00&\\1c&H0000FF&}a"), "{\\1c&H0000FF&}a");
        assert_eq!(c("{\\fr10\\frz20}a"), "{\\frz20}a");
        // \t varsa dokunma
        assert_eq!(c("{\\fs20\\t(\\fs40)\\fs30}a"), "{\\fs20\\t(\\fs40)\\fs30}a");
        // ilk geçerli etiketler korunur
        assert_eq!(c("{\\pos(1,2)\\pos(3,4)}a"), "{\\pos(1,2)\\pos(3,4)}a");
    }

    #[test]
    fn bloklar_arasi() {
        assert_eq!(c("{\\i1}a{\\i1}b"), "{\\i1}ab");
        assert_eq!(c("{\\i1}a{\\i0}b{\\i1}c"), "{\\i1}a{\\i0}b{\\i1}c");
        assert_eq!(c("{\\c&HFF&}a{\\1c&Hff&}b"), "{\\c&HFF&}ab");
        // \r sıfırlar: sonrası gerekli
        assert_eq!(c("{\\i1}a{\\r}b{\\i1}c"), "{\\i1}a{\\r}b{\\i1}c");
        // \t sonrası izlenmez
        assert_eq!(c("{\\fs20\\t(0,500,\\fs40)}a{\\fs20}b"), "{\\fs20\\t(0,500,\\fs40)}a{\\fs20}b");
        // karaoke ve notlar korunur
        assert_eq!(c("{\\k10}a{\\k10}b"), "{\\k10}a{\\k10}b");
        assert_eq!(c("{\\b1}a{not}{\\b1}b"), "{\\b1}a{not}b");
        assert!(clean("düz metin").is_none());
        assert!(clean("{\\an8}a").is_none());
    }
}
