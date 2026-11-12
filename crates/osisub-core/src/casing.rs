//! Harf dönüştürme (Subtitle Edit "Change casing"): BÜYÜK, küçük, Başlık Biçimi, Cümle biçimi.
//! Etiket bloklarına, `\N`/`\n`/`\h` kaçışlarına ve `\p` çizimlerine dokunmaz; Türkçe kipte
//! i/İ ve ı/I doğru eşlenir (`i` → `İ`, `I` → `ı`).
use crate::overrides::{self, Seg};
use crate::qc::{tr_lower, tr_upper};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Case {
    Upper,
    Lower,
    Title,
    Sentence,
}

fn up(s: &str, tr: bool) -> String {
    if tr { tr_upper(s) } else { s.to_uppercase() }
}
fn low(s: &str, tr: bool) -> String {
    if tr { tr_lower(s) } else { s.to_lowercase() }
}

/// Metin parçasını kaçışlar korunarak parçalara ayırır: (metin, kaçış mı)
fn pieces(t: &str) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = vec![];
    let mut cur = String::new();
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && matches!(it.peek(), Some('N' | 'n' | 'h')) {
            if !cur.is_empty() { out.push((std::mem::take(&mut cur), false)); }
            out.push((format!("\\{}", it.next().unwrap()), true));
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() { out.push((cur, false)); }
    out
}

/// Bir satır metnine harf dönüşümü uygular.
pub fn apply(text: &str, case: Case, turkish: bool) -> String {
    let mut segs = overrides::parse(text);
    let mut drawing = false;
    // Başlık/cümle için bağlam: sonraki harf büyük mü olmalı
    let mut word_start = true;
    let mut sentence_start = true;
    for g in segs.iter_mut() {
        match g {
            Seg::Block(tags) => {
                if let Some(p) = tags.iter().rev().find(|t| t.name == "p") {
                    drawing = p.number().unwrap_or(0.0) > 0.0;
                }
            }
            Seg::Text(t) if !drawing => {
                let mut out = String::new();
                for (p, esc) in pieces(t) {
                    if esc {
                        word_start = true;
                        out.push_str(&p);
                        continue;
                    }
                    match case {
                        Case::Upper => out.push_str(&up(&p, turkish)),
                        Case::Lower => out.push_str(&low(&p, turkish)),
                        Case::Title | Case::Sentence => {
                            for c in p.chars() {
                                let cs = c.to_string();
                                if c.is_alphabetic() {
                                    let make_up = if case == Case::Title { word_start } else { sentence_start };
                                    out.push_str(&if make_up { up(&cs, turkish) } else { low(&cs, turkish) });
                                    word_start = false;
                                    sentence_start = false;
                                } else {
                                    out.push(c);
                                    if c.is_whitespace() || matches!(c, '-' | '"' | '(' | '«' | '“') {
                                        word_start = true;
                                    }
                                    if matches!(c, '.' | '!' | '?' | '…') {
                                        sentence_start = true;
                                    }
                                }
                            }
                        }
                    }
                }
                *t = out;
            }
            _ => {}
        }
    }
    overrides::to_string(&segs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_casing_keeps_tags() {
        let t = "{\\i1}iyi ışıklar\\Nİstanbul{\\i0} ılık";
        assert_eq!(apply(t, Case::Upper, true), "{\\i1}İYİ IŞIKLAR\\NİSTANBUL{\\i0} ILIK");
        assert_eq!(apply(t, Case::Upper, false), "{\\i1}IYI IŞIKLAR\\NİSTANBUL{\\i0} ILIK");
        assert_eq!(apply("İYİ IŞIK\\Nyarın", Case::Lower, true), "iyi ışık\\Nyarın");
        // \N küçültülünce \n (yumuşak kırılma) olmamalı
        assert!(apply("A\\NB", Case::Lower, true).contains("\\N"));
        assert_eq!(apply("iSTANBUL'DA bir gün", Case::Title, true), "İstanbul'da Bir Gün");
        assert_eq!(apply("MERHABA. NASILSIN? iyi\\Nolsun", Case::Sentence, true), "Merhaba. Nasılsın? İyi\\Nolsun");
        // çizim değişmez
        assert_eq!(apply("{\\p1}m 0 0 l 10 10{\\p0}abc", Case::Upper, true), "{\\p1}m 0 0 l 10 10{\\p0}ABC");
        // etiket içindeki \fn adı değişmez
        assert_eq!(apply("{\\fnArial}abc", Case::Upper, true), "{\\fnArial}ABC");
    }
}
