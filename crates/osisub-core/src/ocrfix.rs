//! OCR düzeltmesi (Subtitle Edit "OCR fix engine" benzeri): sözlükte olmayan sözcüklerde tipik
//! OCR karışıklıklarını tek tek dener; sözlükte bulunan ilk biçim seçilir. Sözlük dışarıdan
//! (`is_word`) verilir — yazım denetleyicisi (Hunspell) ya da test sözlüğü.

/// (yanlış, doğru) karışıklıkları; sırayla denenir.
const SWAPS: &[(&str, &str)] = &[
    ("rn", "m"), ("m", "rn"), ("vv", "w"), ("cl", "d"), ("li", "h"), ("ii", "ü"),
    ("l", "I"), ("I", "l"), ("1", "l"), ("1", "I"), ("l", "i"), ("i", "l"), ("0", "O"), ("0", "o"), ("O", "0"),
    ("5", "S"), ("$", "s"), ("|", "I"), ("|", "l"), ("é", "e"),
];

/// Zayıf karışıklıklar: kısa sözcüklerde başka geçerli sözcük üretme olasılığı yüksek
/// (yeu → yen), yalnızca 6+ harfli sözcüklerde denenir.
const WEAK: &[(&str, &str)] = &[("c", "e"), ("e", "c"), ("n", "u"), ("u", "n"), ("t", "f"), ("f", "t")];

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '|' || c == '$' || c == '\''
}

/// Olağan harf düzeni: küçük, Baş harfi büyük ya da TÜMÜ BÜYÜK (harf olmayanlar yok sayılır).
fn normal_case(w: &str) -> bool {
    let letters: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() <= 1 || letters[1..].iter().all(|c| c.is_lowercase()) || letters.iter().all(|c| c.is_uppercase())
}

/// Adayda harf kalmadıysa (OOO → 000) yalnızca özgün sözcükte zaten rakam varsa kabul (1O → 10).
fn keeps_letters(orig: &str, cand: &str) -> bool {
    cand.chars().any(|c| c.is_alphabetic()) || orig.chars().any(|c| c.is_ascii_digit())
}

/// Tek sözcüğü düzeltmeyi dener: önce aynen, sonra her karışıklığı tek tek her konumda, sonra
/// aynı karışıklığı tüm konumlarda. Bulunamazsa `None`.
pub fn fix_word(w: &str, is_word: &dyn Fn(&str) -> bool) -> Option<String> {
    // Tek başına "l"/"|": Hunspell en_US tek harfleri kabul eder, bu yüzden aşağıdaki erken dönüşe
    // takılırdı; OCR'da neredeyse her zaman "I" zamiridir ("l am here").
    if (w == "l" || w == "|") && is_word("I") {
        return Some("I".into());
    }
    if w.is_empty() || is_word(w) || w.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let weak: &[(&str, &str)] = if w.chars().count() >= 6 { WEAK } else { &[] };
    for (from, to) in SWAPS.iter().chain(weak) {
        // her konumda tek değişim
        let mut start = 0;
        while let Some(p) = w[start..].find(from) {
            let at = start + p;
            let cand = format!("{}{}{}", &w[..at], to, &w[at + from.len()..]);
            if normal_case(&cand) && keeps_letters(w, &cand) && is_word(&cand) {
                return Some(cand);
            }
            start = at + from.len();
        }
        if w.matches(from).count() > 1 {
            let cand = w.replace(from, to);
            if normal_case(&cand) && keeps_letters(w, &cand) && is_word(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

/// Metindeki sözcükleri düzeltir (etiket blokları ve `\N` korunur). Dönüş: (metin, düzeltme sayısı).
pub fn fix_text(text: &str, is_word: &dyn Fn(&str) -> bool) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut n = 0;
    for part in crate::tags::split(text) {
        match part {
            crate::tags::Part::Block(b) => { out.push('{'); out.push_str(&b); out.push('}'); }
            crate::tags::Part::Text(t) => {
                let mut cur = String::new();
                let flush = |cur: &mut String, out: &mut String, n: &mut usize| {
                    if cur.is_empty() { return; }
                    // baş/son kesme işaretleri sözcüğe dahil değil
                    let core = cur.trim_matches('\'');
                    let lead = &cur[..cur.find(core).unwrap_or(0)];
                    let tail = &cur[lead.len() + core.len()..];
                    // önce sözcüğün tamamı (l'm → I'm), olmazsa Türkçe ek öncesi gövde (Hinata'ya)
                    let whole = if is_word(core) && core != "l" && core != "|" { None } else { fix_word(core, is_word) };
                    let fixed = whole.or_else(|| match core.find('\'') {
                        Some(i) if !is_word(core) => fix_word(&core[..i], is_word).map(|f| format!("{f}{}", &core[i..])),
                        _ => None,
                    });
                    match fixed {
                        Some(f) => { out.push_str(lead); out.push_str(&f); out.push_str(tail); *n += 1; }
                        None => out.push_str(cur),
                    }
                    cur.clear();
                };
                let mut chars = t.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\\' && matches!(chars.peek(), Some('N' | 'n' | 'h')) {
                        flush(&mut cur, &mut out, &mut n);
                        out.push(c);
                        out.push(chars.next().unwrap());
                    } else if is_token_char(c) {
                        cur.push(c);
                    } else {
                        flush(&mut cur, &mut out, &mut n);
                        out.push(c);
                    }
                }
                flush(&mut cur, &mut out, &mut n);
            }
        }
    }
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(w: &str) -> bool {
        ["I", "I'm", "modern", "will", "hidden", "door", "Hello", "world", "Wow", "Sakura", "Open", "the", "Who's", "there", "cold", "outside", "Hinata", "and", "me"].iter().any(|d| d.eq_ignore_ascii_case(w) && (w.chars().next().unwrap().is_uppercase() == d.chars().next().unwrap().is_uppercase() || d.len() > 1))
    }

    #[test]
    fn tek_l_zamir() {
        // en_US Hunspell gibi tek harfleri kabul eden sözlük
        let w = |x: &str| ["I", "l", "m", "am", "here", "it", "is"].contains(&x);
        assert_eq!(super::fix_text("l am here", &w), ("I am here".to_string(), 1));
        assert_eq!(super::fix_text("| am", &w).0, "I am");
        // "I" sözlükte yoksa (ör. yalnız Türkçe) dokunma
        let tr = |x: &str| ["l", "geldi"].contains(&x);
        assert_eq!(super::fix_text("l geldi", &tr).1, 0);
    }

    #[test]
    fn sozcuk() {
        assert_eq!(fix_word("rnodern", &dict).as_deref(), Some("modern"));
        assert_eq!(fix_word("wi1l", &dict).as_deref(), Some("will"));
        assert_eq!(fix_word("hidclen", &dict).as_deref(), Some("hidden"));
        assert_eq!(fix_word("d0or", &dict).as_deref(), Some("door"));
        assert_eq!(fix_word("Hel1o", &dict).as_deref(), Some("Hello"));
        assert_eq!(fix_word("door", &dict), None);
        assert_eq!(fix_word("xyz", &dict), None);
        assert_eq!(fix_word("1999", &dict), None);
        // harfsiz her şeyi kabul eden sözlük (Hunspell rakamları kabul eder): OOO → 000 olmamalı
        let any_digits = |w: &str| !w.chars().any(|c| c.is_alphabetic());
        assert_eq!(fix_word("OOO", &any_digits), None);
        assert_eq!(fix_word("1O", &any_digits).as_deref(), Some("10"));
    }

    #[test]
    fn metin() {
        let (t, n) = fix_text("{\\an8}l'm here. Open the d0or,\\Nrnodern wor1d! Hinata'ya", &dict);
        assert_eq!(t, "{\\an8}I'm here. Open the door,\\Nmodern world! Hinata'ya");
        assert_eq!(n, 4);
        let (t, n) = fix_text("Who's there?", &dict);
        assert_eq!((t.as_str(), n), ("Who's there?", 0));
    }
}
