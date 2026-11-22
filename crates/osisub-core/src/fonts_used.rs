//! Betikte kullanılan yazı tiplerini (stil + \fn + \b/\i) çıkarır.
use crate::ass::{Entry, Script};
use crate::overrides::{self, Seg};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontUse {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// Bu yazı tipini kullanan satır sayısı.
    pub lines: usize,
    /// Bu yazı tipiyle çizilen görünür karakterler → ilk geçtiği girdi indeksi (eksik glif denetimi için).
    pub chars: BTreeMap<char, usize>,
}

fn norm(f: &str) -> String {
    f.trim().trim_start_matches('@').to_string()
}

fn bold_from(arg: &str, cur: bool) -> bool {
    match arg.trim() {
        "" => cur,
        "0" => false,
        "1" => true,
        n => n.parse::<i32>().map(|w| w >= 600).unwrap_or(cur),
    }
}

pub fn used_fonts(script: &Script) -> Vec<FontUse> {
    let styles: BTreeMap<&str, &crate::ass::Style> = script.styles().map(|s| (s.name.as_str(), s)).collect();
    let mut counts: BTreeMap<(String, bool, bool), usize> = BTreeMap::new();
    let mut chars: BTreeMap<(String, bool, bool), BTreeMap<char, usize>> = BTreeMap::new();
    for (ei, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment {
            continue;
        }
        let Some(st) = styles.get(d.style.as_str()).copied().or_else(|| styles.get("Default").copied()) else { continue };
        let base = (norm(&st.fontname), st.bold, st.italic);
        let (mut fam, mut b, mut i) = base.clone();
        // \r ile değişen "sıfırlama" stili: boş \fn / \fn0, parametresiz \b / \i buna döner (libass)
        let mut reset = base.clone();
        let mut seen: Vec<(String, bool, bool)> = vec![];
        let mut drawing = false;
        let push = |f: &(String, bool, bool), seen: &mut Vec<(String, bool, bool)>| {
            if !seen.contains(f) {
                seen.push(f.clone());
            }
        };
        for seg in overrides::parse(&d.text) {
            match seg {
                Seg::Block(tags) => {
                    for t in tags {
                        match t.name.as_str() {
                            "fn" => {
                                let a = t.arg.trim();
                                fam = if a.is_empty() || a == "0" { reset.0.clone() } else { norm(a) };
                            }
                            "p" => drawing = t.number().unwrap_or(0.0) > 0.0,
                            "b" => b = if t.arg.trim().is_empty() { reset.1 } else { bold_from(&t.arg, b) },
                            "i" => i = if t.arg.trim().is_empty() { reset.2 } else { t.arg.trim() == "1" },
                            "r" => {
                                let name = t.arg.trim();
                                let s2 = styles.get(name).copied().unwrap_or(st);
                                reset = (norm(&s2.fontname), s2.bold, s2.italic);
                                (fam, b, i) = reset.clone();
                            }
                            _ => {}
                        }
                    }
                }
                Seg::Text(t) => {
                    // yalnızca görünür metin yazı tipi gerektirir (boş/boşluk satırları Aegisub gibi sayılmaz)
                    let clean = t.replace("\\N", " ").replace("\\n", " ").replace("\\h", "\u{a0}");
                    if !drawing && clean.chars().any(|c| !c.is_whitespace() || c == '\u{a0}') {
                        push(&(fam.clone(), b, i), &mut seen);
                        let m = chars.entry((fam.clone(), b, i)).or_default();
                        // \h (bölünmez boşluk) da yazı tipinde bulunmalı
                        for c in clean.chars().filter(|c| (!c.is_whitespace() || *c == '\u{a0}') && !c.is_control()) {
                            m.entry(c).or_insert(ei);
                        }
                    }
                }
            }
        }
        for f in seen {
            *counts.entry(f).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(k, lines)| {
            let ch = chars.remove(&k).unwrap_or_default();
            let (family, bold, italic) = k;
            FontUse { family, bold, italic, lines, chars: ch }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bos_fn_ve_parametresiz_b_i_stile_doner() {
        let st = "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,-1,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";
        let s = Script::parse(&format!("{st}Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{{\\fnImpact}}a{{\\fn}}b{{\\fn0\\i0}}c{{\\i}}d{{\\b1}}e{{\\b}}f\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,  \n"));
        let u = used_fonts(&s);
        let v: Vec<(&str, bool, bool, String)> = u.iter().map(|f| (f.family.as_str(), f.bold, f.italic, f.chars.keys().collect())).collect();
        assert_eq!(v, vec![("Arial", false, false, "c".into()), ("Arial", false, true, "bdf".into()), ("Arial", true, true, "e".into()), ("Impact", false, true, "a".into())]);
        // boşluk satırı sayılmaz: Arial italik yalnız 1 satırda
        assert_eq!(u[1].lines, 1);
    }

    #[test]
    fn collects_fonts() {
        let s = Script::parse("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\nStyle: Alt,@Meiryo,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,a{\\fnImpact\\b1}b{\\r}c\nDialogue: 0,0:00:01.00,0:00:02.00,Alt,,0,0,0,,x\nComment: 0,0:00:01.00,0:00:02.00,Alt,,0,0,0,,{\\fnNope}y\n");
        let u = used_fonts(&s);
        let names: Vec<(&str, bool, usize)> = u.iter().map(|f| (f.family.as_str(), f.bold, f.lines)).collect();
        assert_eq!(names, vec![("Arial", false, 1), ("Impact", true, 1), ("Meiryo", true, 1)]);
        assert_eq!(u[0].chars.keys().collect::<String>(), "ac");
        assert_eq!(u[1].chars.keys().collect::<String>(), "b");
        // \N ve çizimler karakter sayılmaz
        let s2 = Script::parse(&s.to_ass_string().replace("a{\\fnImpact\\b1}b{\\r}c", "ğ\\Nş{\\p1}m 0 0 l 1 1{\\p0}İ"));
        assert_eq!(used_fonts(&s2)[0].chars.keys().collect::<String>(), "ğİş");
    }
}
