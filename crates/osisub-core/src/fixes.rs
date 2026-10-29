//! "Sık hataları düzelt" kuralları (Subtitle Edit esinli), Türkçe'ye duyarlı.
use crate::ass::{Dialogue, Entry, Script};
use crate::qc::{tr_lower, tr_upper};
use crate::tags::{self, Part};

#[derive(Clone, Debug)]
pub struct Fixes {
    pub mojibake: bool,
    pub double_spaces: bool,
    pub edge_spaces: bool,
    pub ellipsis: bool,
    pub space_before_punct: bool,
    pub space_after_punct: bool,
    pub sentence_case: bool,
    pub dialogue_dash: bool,
    pub remove_hi: bool,
    /// Etiketsiz satırlarda bu karakterden uzun metni iki olaya böl (None = kapalı).
    pub split_long: Option<usize>,
    pub min_duration_ms: Option<i64>,
    pub min_gap_ms: Option<i64>,
    pub only: Option<Vec<usize>>,
}

impl Default for Fixes {
    fn default() -> Self {
        Fixes { mojibake: true, double_spaces: true, edge_spaces: true, ellipsis: false, space_before_punct: true, space_after_punct: true, sentence_case: false, dialogue_dash: true, remove_hi: false, split_long: None, min_duration_ms: None, min_gap_ms: None, only: None }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub changed_lines: usize,
    pub timing_changes: usize,
    pub split_lines: usize,
}

// ---------------------------------------------------------------- yardımcılar

/// Düz metin parçalarına (etiket blokları dışında) işlev uygular.
pub fn map_text(text: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    for p in tags::split(text) {
        match p {
            Part::Block(b) => {
                out.push('{');
                out.push_str(b);
                out.push('}');
            }
            Part::Text(t) => out.push_str(&f(t)),
        }
    }
    out
}

fn cp1252_byte(c: char) -> Option<u8> {
    let cp = c as u32;
    if cp < 0x80 || (0xA0..0x100).contains(&cp) {
        return Some(cp as u8);
    }
    Some(match c {
        '€' => 0x80, '‚' => 0x82, 'ƒ' => 0x83, '„' => 0x84, '…' => 0x85, '†' => 0x86, '‡' => 0x87, 'ˆ' => 0x88, '‰' => 0x89,
        'Š' => 0x8A, '‹' => 0x8B, 'Œ' => 0x8C, 'Ž' => 0x8E, '‘' => 0x91, '’' => 0x92, '“' => 0x93, '”' => 0x94, '•' => 0x95,
        '–' => 0x96, '—' => 0x97, '˜' => 0x98, '™' => 0x99, 'š' => 0x9A, '›' => 0x9B, 'œ' => 0x9C, 'ž' => 0x9E, 'Ÿ' => 0x9F,
        // cp1252'de tanımsız 0x81,0x8D,0x8F,0x90,0x9D kontrol karakterleri olarak gelebilir
        '\u{81}' => 0x81, '\u{8D}' => 0x8D, '\u{8F}' => 0x8F, '\u{90}' => 0x90, '\u{9D}' => 0x9D,
        _ => return None,
    })
}

/// UTF-8 metnin Latin-1/cp1252 olarak yanlış çözülmesiyle oluşan bozulmayı onarır ("Ã¼" → "ü").
/// Ayrıca ISO-8859-1 ile açılmış Türkçe metindeki Ð Ý Þ ð ý þ harflerini düzeltir.
pub fn fix_mojibake(s: &str) -> String {
    // 1) Çift kodlanmış UTF-8: yalnızca tüm karakterler cp1252'ye eşlenebiliyor ve sonuç geçerli UTF-8 ise.
    if s.chars().any(|c| matches!(c, 'Ã' | 'Ä' | 'Å' | 'Â')) {
        let bytes: Option<Vec<u8>> = s.chars().map(cp1252_byte).collect();
        if let Some(b) = bytes {
            if let Ok(fixed) = String::from_utf8(b) {
                if fixed != s {
                    return fixed;
                }
            }
        }
    }
    // 2) ISO-8859-1 ile yanlış açılmış cp1254: Ð Ý Þ ð ý þ → Ğ İ Ş ğ ı ş.
    //    Yalnızca Türkçe bağlam ipucu varsa (tek başına Ý/Þ/Ð İngilizce/İzlandaca olabilir) uygula.
    let suspicious = s.chars().any(|c| matches!(c, 'Ð' | 'Ý' | 'Þ' | 'ð' | 'ý' | 'þ'));
    if suspicious {
        let lower = s.to_lowercase();
        let hint = ["ý", "þ", "ð"].iter().any(|c| lower.contains(c)) && lower.chars().any(|c| "abcdefghijklmnoprstuvyz".contains(c));
        if hint {
            return s
                .chars()
                .map(|c| match c {
                    'Ð' => 'Ğ',
                    'Ý' => 'İ',
                    'Þ' => 'Ş',
                    'ð' => 'ğ',
                    'ý' => 'ı',
                    'þ' => 'ş',
                    c => c,
                })
                .collect();
        }
    }
    s.to_string()
}

fn is_sentence_end(s: &str) -> bool {
    let t = tags::strip_tags(s);
    let t = t.trim_end_matches(|c: char| c == '"' || c == '\'' || c == '”' || c == '’' || c == ')' || c.is_whitespace());
    t.ends_with('.') || t.ends_with('!') || t.ends_with('?') || t.ends_with('…')
}

fn upper_first_letter(s: &str) -> String {
    let mut done = false;
    s.chars()
        .map(|c| {
            if !done && c.is_alphabetic() {
                done = true;
                tr_upper(&c.to_string())
            } else {
                if !c.is_whitespace() && !matches!(c, '-' | '"' | '\'' | '“' | '‘' | '(' | '[' | '¿' | '¡' | '♪' | '…') {
                    done = true;
                }
                c.to_string()
            }
        })
        .collect()
}

/// Seslendirme/işitme engelli notlarını ([ağlıyor], (müzik), ♪ satırları, "ALİ:") siler.
pub fn remove_hi(text: &str) -> String {
    let t = map_text(text, |t| {
        let mut out = String::new();
        let mut depth_sq = 0;
        let mut depth_rd = 0;
        for c in t.chars() {
            match c {
                '[' => depth_sq += 1,
                ']' if depth_sq > 0 => depth_sq -= 1,
                '(' => depth_rd += 1,
                ')' if depth_rd > 0 => depth_rd -= 1,
                c if depth_sq == 0 && depth_rd == 0 => out.push(c),
                _ => {}
            }
        }
        out
    });
    // "KONUŞMACI:" (büyük harfli ön ek) temizle.
    let lines: Vec<String> = t
        .split("\\N")
        .map(|l| {
            let v = tags::strip_tags(l);
            if let Some(p) = v.find(':') {
                let head = &v[..p];
                if head.chars().count() <= 20 && head.chars().any(|c| c.is_alphabetic()) && head.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
                    return l.replacen(&format!("{}:", head), "", 1).trim_start().to_string();
                }
            }
            l.to_string()
        })
        .filter(|l| !tags::strip_tags(l).trim().is_empty() && !tags::strip_tags(l).trim().chars().all(|c| c == '♪' || c == '#' || c.is_whitespace()))
        .collect();
    lines.join("\\N").trim().to_string()
}

fn fix_punct_spacing(t: &str, before: bool, after: bool) -> String {
    let c: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if before && ch == ' ' && i + 1 < c.len() && matches!(c[i + 1], '.' | ',' | '!' | '?' | ';' | ':') {
            // " ..." (üç nokta) ve " :)" gibi durumlar bozulmasın; yalnızca tek noktalama + sonrası boşluk/son.
            let next_next = c.get(i + 2).copied();
            if !matches!(next_next, Some('.') | Some(')') | Some('(') | Some('D') | Some('P')) {
                i += 1;
                continue;
            }
        }
        out.push(ch);
        if after && matches!(ch, ',' | ';' | '!' | '?') && i + 1 < c.len() {
            let n = c[i + 1];
            if n.is_alphabetic() && !c[..i].last().map_or(false, |p| p.is_ascii_digit()) {
                out.push(' ');
            }
        }
        // ". " için: nokta + büyük harf (kısaltma/ondalık değil)
        if after && ch == '.' && i + 1 < c.len() && c[i + 1].is_alphabetic() && c[i + 1].is_uppercase() && i > 0 && c[i - 1].is_alphabetic() && c[i - 1].is_lowercase() {
            out.push(' ');
        }
        i += 1;
    }
    out
}

fn fix_dialogue_dash(text: &str) -> String {
    text.split("\\N")
        .map(|l| {
            let v = l.trim_start();
            if let Some(rest) = v.strip_prefix('-') {
                if !rest.starts_with(' ') && !rest.starts_with('-') && !rest.is_empty() && !rest.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    return format!("- {}", rest);
                }
            }
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\\N")
}

pub fn fix_text(text: &str, f: &Fixes) -> String {
    let mut t = text.to_string();
    if f.mojibake {
        t = map_text(&t, fix_mojibake);
    }
    if f.remove_hi {
        t = remove_hi(&t);
    }
    if f.ellipsis {
        t = map_text(&t, |x| x.replace("...", "…"));
    }
    if f.space_before_punct || f.space_after_punct {
        t = map_text(&t, |x| fix_punct_spacing(x, f.space_before_punct, f.space_after_punct));
    }
    if f.double_spaces {
        t = map_text(&t, |x| {
            let mut s = x.to_string();
            while s.contains("  ") {
                s = s.replace("  ", " ");
            }
            s
        });
    }
    if f.edge_spaces {
        t = t.replace(" \\N", "\\N").replace("\\N ", "\\N");
        t = t.trim().to_string();
    }
    if f.dialogue_dash {
        t = fix_dialogue_dash(&t);
    }
    t
}

/// Düzeltmeleri uygular; değişen satır sayısını döndürür.
pub fn apply(script: &mut Script, f: &Fixes) -> Report {
    let mut rep = Report::default();
    let mut prev_end_sentence = true;
    let mut prev_idx: Option<usize> = None;
    for i in 0..script.entries.len() {
        let in_scope = f.only.as_ref().map_or(true, |v| v.contains(&i));
        let Entry::Dialogue(d) = &mut script.entries[i] else { continue };
        if d.comment {
            continue;
        }
        if in_scope {
            let mut nt = fix_text(&d.text, f);
            if f.sentence_case {
                if prev_end_sentence {
                    nt = upper_first_in_text(&nt);
                }
                nt = capitalize_after_sentence_ends(&nt);
            }
            if nt != d.text {
                d.text = nt;
                rep.changed_lines += 1;
            }
        }
        prev_end_sentence = is_sentence_end(&d.text);
        let _ = prev_idx;
        prev_idx = Some(i);
    }
    if let Some(max) = f.split_long {
        rep.split_lines += split_long_lines(script, max, f.only.as_deref());
    }
    if f.min_duration_ms.is_some() || f.min_gap_ms.is_some() {
        rep.timing_changes += fix_timing(script, f);
    }
    rep
}

/// Etiketsiz, tek satırlı ve `max` karakterden uzun olayları, ortaya en yakın cümle sonu/boşlukta ikiye böler;
/// süre karakter oranında paylaşılır. Bölünen olay sayısını döndürür.
pub fn split_long_lines(script: &mut Script, max: usize, only: Option<&[usize]>) -> usize {
    let mut out: Vec<Entry> = Vec::with_capacity(script.entries.len());
    let mut n = 0;
    for (i, e) in std::mem::take(&mut script.entries).into_iter().enumerate() {
        let Entry::Dialogue(d) = &e else {
            out.push(e);
            continue;
        };
        let eligible = only.map_or(true, |o| o.contains(&i)) && !d.comment && !d.text.contains('{') && !d.text.contains("\\N") && d.text.chars().count() > max && d.duration() > 0;
        if !eligible {
            out.push(e);
            continue;
        }
        let chars: Vec<char> = d.text.chars().collect();
        let mid = chars.len() / 2;
        // Önce cümle sonu (. ! ? …), yoksa boşluk; ortaya en yakın olanı seç.
        let pick = |pred: &dyn Fn(usize) -> bool| (1..chars.len() - 1).filter(|&k| pred(k)).min_by_key(|&k| k.abs_diff(mid));
        let cut = pick(&|k| matches!(chars[k], '.' | '!' | '?' | '…') && chars.get(k + 1) == Some(&' ')).map(|k| k + 1).or_else(|| pick(&|k| chars[k] == ' '));
        let Some(cut) = cut else {
            out.push(e);
            continue;
        };
        let (a, b): (String, String) = (chars[..cut].iter().collect::<String>().trim().to_string(), chars[cut..].iter().collect::<String>().trim().to_string());
        if a.is_empty() || b.is_empty() {
            out.push(e);
            continue;
        }
        let t = d.start_time + (d.duration() as f64 * a.chars().count() as f64 / (a.chars().count() + b.chars().count()) as f64).round() as i64;
        out.push(Entry::Dialogue(Dialogue { text: a, end_time: t, ..d.clone() }));
        out.push(Entry::Dialogue(Dialogue { text: b, start_time: t, ..d.clone() }));
        n += 1;
    }
    script.entries = out;
    n
}

fn upper_first_in_text(t: &str) -> String {
    let mut first = true;
    map_text(t, |x| {
        if first && !x.trim().is_empty() {
            first = false;
            upper_first_letter(x)
        } else {
            x.to_string()
        }
    })
}

fn capitalize_after_sentence_ends(t: &str) -> String {
    map_text(t, |x| {
        let chars: Vec<char> = x.chars().collect();
        let mut out = String::new();
        let mut cap_next = false;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if cap_next && c.is_alphabetic() {
                out.push_str(&tr_upper(&c.to_string()));
                cap_next = false;
            } else {
                out.push(c);
                if !c.is_whitespace() && !matches!(c, '"' | '\'' | '”' | '’' | ')') {
                    cap_next = false;
                }
            }
            if matches!(c, '.' | '!' | '?') && i + 1 < chars.len() && chars[i + 1] == ' ' && !(c == '.' && i > 0 && chars[i - 1].is_ascii_digit()) {
                cap_next = true;
            }
            i += 1;
        }
        let _ = tr_lower;
        out
    })
}

fn fix_timing(script: &mut Script, f: &Fixes) -> usize {
    let order: Vec<usize> = {
        let mut v: Vec<usize> = script.entries.iter().enumerate().filter(|(i, e)| matches!(e, Entry::Dialogue(d) if !d.comment) && f.only.as_ref().map_or(true, |o| o.contains(i))).map(|(i, _)| i).collect();
        v.sort_by_key(|&i| if let Entry::Dialogue(d) = &script.entries[i] { d.start_time } else { 0 });
        v
    };
    let mut n = 0;
    for k in 0..order.len() {
        let next_start = order.get(k + 1).and_then(|&j| if let Entry::Dialogue(d) = &script.entries[j] { Some(d.start_time) } else { None });
        let Entry::Dialogue(d) = &mut script.entries[order[k]] else { continue };
        let before = (d.start_time, d.end_time);
        if let Some(min) = f.min_duration_ms {
            if d.end_time - d.start_time < min {
                let want = d.start_time + min;
                d.end_time = match next_start {
                    Some(ns) => want.min(ns.max(d.end_time)),
                    None => want,
                };
            }
        }
        if let (Some(gap), Some(ns)) = (f.min_gap_ms, next_start) {
            if ns - d.end_time < gap && ns > d.start_time {
                d.end_time = (ns - gap).max(d.start_time + 1);
            }
        }
        if before != (d.start_time, d.end_time) {
            n += 1;
        }
    }
    n
}

#[allow(dead_code)]
fn _unused(_: &Dialogue) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(texts: &[&str]) -> Script {
        Script::from_dialogues(texts.iter().enumerate().map(|(i, t)| Dialogue { start_time: i as i64 * 3000, end_time: i as i64 * 3000 + 2000, text: t.to_string(), ..Default::default() }).collect())
    }

    #[test]
    fn mojibake_utf8_and_latin1() {
        assert_eq!(fix_mojibake("GÃ¼naydÄ±n ÅŸehir"), "Günaydın şehir");
        assert_eq!(fix_mojibake("Ýstanbul'da yaþýyorum"), "İstanbul'da yaşıyorum");
        assert_eq!(fix_mojibake("Normal metin"), "Normal metin");
        assert_eq!(fix_mojibake("Þ"), "Þ", "tek başına belirsiz: dokunma");
    }

    #[test]
    fn spacing_dash_ellipsis() {
        let f = Fixes { ellipsis: true, ..Default::default() };
        assert_eq!(fix_text("Merhaba ,dünya !  Nasılsın ?", &f), "Merhaba, dünya! Nasılsın?");
        assert_eq!(fix_text("-Selam\\N-Merhaba", &f), "- Selam\\N- Merhaba");
        assert_eq!(fix_text("Bekle...", &f), "Bekle…");
        assert_eq!(fix_text("{\\i1}Selam{\\i0}  dünya", &f), "{\\i1}Selam{\\i0} dünya");
    }

    #[test]
    fn sentence_case_turkish() {
        let mut sc = s(&["merhaba. ışık yandı.", "i̇stanbul güzel"]);
        let f = Fixes { sentence_case: true, mojibake: false, ..Default::default() };
        apply(&mut sc, &f);
        let v: Vec<&str> = sc.dialogues().map(|d| d.text.as_str()).collect();
        assert_eq!(v[0], "Merhaba. Işık yandı.");
        assert!(v[1].starts_with('İ') || v[1].starts_with('I'), "{}", v[1]);
    }

    #[test]
    fn split_long() {
        let mut sc = s(&["Bu çok uzun bir cümle. Ve ikincisi de en az onun kadar uzun olmalı ki bölünsün.", "kısa"]);
        let f = Fixes { split_long: Some(40), mojibake: false, ..Default::default() };
        let r = apply(&mut sc, &f);
        assert_eq!(r.split_lines, 1);
        let v: Vec<(i64, i64, &str)> = sc.dialogues().map(|d| (d.start_time, d.end_time, d.text.as_str())).collect();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].2, "Bu çok uzun bir cümle.");
        assert_eq!(v[0].1, v[1].0, "ardışık");
        assert_eq!(v[1].1, 2000);
    }

    #[test]
    fn hearing_impaired() {
        assert_eq!(remove_hi("[kapı çalar] Gel içeri"), "Gel içeri".to_string());
        assert_eq!(remove_hi("ALİ: Merhaba"), "Merhaba");
        assert_eq!(remove_hi("♪ şarkı ♪"), "♪ şarkı ♪", "şarkı sözleri korunur");
        assert_eq!(remove_hi("(müzik)"), "");
    }

    #[test]
    fn timing_rules() {
        let mut sc = Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 300, text: "a".into(), ..Default::default() },
            Dialogue { start_time: 1000, end_time: 2000, text: "b".into(), ..Default::default() },
            Dialogue { start_time: 2020, end_time: 3000, text: "c".into(), ..Default::default() },
        ]);
        let f = Fixes { min_duration_ms: Some(800), min_gap_ms: Some(80), mojibake: false, ..Default::default() };
        let r = apply(&mut sc, &f);
        let v: Vec<(i64, i64)> = sc.dialogues().map(|d| (d.start_time, d.end_time)).collect();
        assert_eq!(v, vec![(0, 800), (1000, 1940), (2020, 3000)]);
        assert_eq!(r.timing_changes, 2);
    }
}
