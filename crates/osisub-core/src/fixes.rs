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
    /// Türkçe yaygın yazım hataları (herşey → her şey, gelecekmisin → gelecek misin; core::trfix)
    pub tr_spelling: bool,
    /// Etiketsiz satırlarda bu karakterden uzun metni iki olaya böl (None = kapalı).
    pub split_long: Option<usize>,
    /// Satırları dengeli iki satıra kır / kısaları tek satır yap (satır başına karakter; None = kapalı)
    pub balance_lines: Option<usize>,
    pub min_duration_ms: Option<i64>,
    pub min_gap_ms: Option<i64>,
    /// Çok uzun süreleri kısalt (SE FixLongDisplayTimes; None = kapalı)
    pub max_duration_ms: Option<i64>,
    /// Gereksiz noktalama (SE FixUnneededPeriods/FixCommas/FixDoubleApostrophes): `?.` → `?`, `....` → `...`,
    /// `,,` → `,`, `''` → `"`, `……` → `…`
    pub punct_cleanup: bool,
    /// Görünmez/benzer Unicode karakterleri sadeleştir (SE NormalizeStrings): bölünmez boşluk, sıfır
    /// genişlikli boşluk, metin içi BOM, ‐‑‒– tireleri, tam genişlikli iki nokta
    pub normalize_unicode: bool,
    /// Eksik açılış köşeli/normal parantezini ekle (SE FixMissingOpenBracket): `müzik]` → `[müzik]`
    pub missing_bracket: bool,
    /// Tek konuşmacılı satırdaki baştaki diyalog tiresini sil (SE RemoveDialogFirstLineInNonDialogs)
    pub lone_dash: bool,
    pub only: Option<Vec<usize>>,
}

impl Default for Fixes {
    fn default() -> Self {
        Fixes { mojibake: true, double_spaces: true, edge_spaces: true, ellipsis: false, space_before_punct: true, space_after_punct: true, sentence_case: false, dialogue_dash: true, remove_hi: false, tr_spelling: false, split_long: None, balance_lines: None, min_duration_ms: None, min_gap_ms: None, max_duration_ms: None, punct_cleanup: true, normalize_unicode: true, missing_bracket: true, lone_dash: false, only: None }
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
    let t = remove_bracket_groups(text);
    // "KONUŞMACI:" (büyük harfli ön ek) temizle.
    let lines: Vec<String> = t
        .split("\\N")
        .map(|l| {
            let v = tags::strip_tags(l);
            if let Some(p) = v.find(':') {
                let head = &v[..p];
                // konuşmacı etiketi: satır başında 1-3 sözcük, yalnızca harf/rakam/boşluk ("NE?! HAYIR: DUR!" değil)
                let words = head.split_whitespace().count();
                let clean = head.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '.' || c == '\'' || c == '-');
                if (1..=3).contains(&words) && clean && head.chars().count() <= 20 && head.chars().any(|c| c.is_alphabetic()) && head.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
                    return l.replacen(&format!("{}:", head), "", 1).trim_start().to_string();
                }
            }
            l.to_string()
        })
        .filter(|l| !tags::strip_tags(l).trim().is_empty() && !tags::strip_tags(l).trim().chars().all(|c| c == '♪' || c == '#' || c.is_whitespace()))
        .collect();
    lines.join("\\N").trim().to_string()
}

/// Kapanan [..] / (..) gruplarını siler (etiket blokları grubun içinde de olabilir). Kapanmayan
/// parantez (":(" gülen yüz) metindir.
fn remove_bracket_groups(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch == '{' {
            // etiket bloğunu olduğu gibi kopyala
            let mut j = i;
            while j < c.len() && c[j] != '}' { j += 1; }
            let end = j.min(c.len() - 1);
            out.extend(&c[i..=end]);
            i = end + 1;
            continue;
        }
        if ch == '[' || ch == '(' {
            let close = if ch == '[' { ']' } else { ')' };
            // eşleşen kapanış (aynı \N satırı içinde, iç içe sayılarak, etiket blokları atlanarak)
            let (mut j, mut depth, mut in_tag, mut found) = (i + 1, 1, false, None);
            while j < c.len() {
                let x = c[j];
                if in_tag { if x == '}' { in_tag = false; } }
                else if x == '{' { in_tag = true; }
                else if x == '\\' && c.get(j + 1) == Some(&'N') { break; }
                else if x == ch { depth += 1; }
                else if x == close { depth -= 1; if depth == 0 { found = Some(j); break; } }
                j += 1;
            }
            if let Some(j) = found {
                i = j + 1;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
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
            // ":3", ":O", ":/", ";)" gibi ifadeler: iki nokta/noktalı virgülden sonra boşluk ya da son gelmiyorsa dokunma
            let emoticon = matches!(c[i + 1], ':' | ';') && next_next.map_or(false, |n| !n.is_whitespace());
            if !emoticon && !matches!(next_next, Some('.') | Some(')') | Some('(') | Some('D') | Some('P')) {
                i += 1;
                continue;
            }
        }
        out.push(ch);
        if after && matches!(ch, ',' | ';' | '!' | '?') && i + 1 < c.len() {
            let n = c[i + 1];
            // adres içindeki "?" / "," (www.site.com/?q=ara) bölünmez
            let word: String = c[..i].iter().rev().take_while(|c| !c.is_whitespace()).collect::<Vec<_>>().into_iter().rev().collect();
            let url = word.contains("://") || word.contains("www.") || word.ends_with('/');
            if !url && n.is_alphabetic() && !c[..i].last().map_or(false, |p| p.is_ascii_digit()) {
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
            // baştaki etiket blokları ({\an8}) atlanır; -, – ve — çizgileri
            let mut head_len = 0;
            let mut v = l;
            loop {
                let t = v.trim_start();
                head_len += v.len() - t.len();
                v = t;
                if v.starts_with('{') {
                    if let Some(e) = v.find('}') { head_len += e + 1; v = &v[e + 1..]; continue; }
                }
                break;
            }
            if let Some(d) = v.chars().next().filter(|c| matches!(c, '-' | '–' | '—')) {
                let rest = &v[d.len_utf8()..];
                if !rest.starts_with(' ') && !rest.starts_with(d) && !rest.is_empty() && !rest.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    return format!("{}{} {}", &l[..head_len], d, rest);
                }
            }
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\\N")
}

/// SE NormalizeStrings: görünmez ve benzer karakterler (em tire — ve … korunur).
pub fn normalize_unicode(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\u{200B}' | '\u{FEFF}' | '\u{200E}' | '\u{200F}' | '\u{2060}' => None,
            '\u{00A0}' | '\u{202F}' | '\u{2007}' => Some(' '),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2043}' => Some('-'),
            '\u{02F8}' | '\u{FF1A}' | '\u{FE13}' => Some(':'),
            _ => Some(c),
        })
        .collect()
}

/// SE FixUnneededPeriods + FixCommas + FixDoubleApostrophes.
pub fn punct_cleanup(s: &str) -> String {
    let mut t = s.to_string();
    // "?." / "!." (ardından nokta gelmiyorsa; "?..." korunur)
    for p in ['?', '!'] {
        let mut out = String::with_capacity(t.len());
        let cs: Vec<char> = t.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            out.push(cs[i]);
            if cs[i] == p && cs.get(i + 1) == Some(&'.') && cs.get(i + 2) != Some(&'.') {
                i += 2;
                continue;
            }
            i += 1;
        }
        t = out;
    }
    while t.contains("....") {
        t = t.replace("....", "...");
    }
    for (a, b) in [("……", "…"), (".…", "…"), ("….", "…"), (",,", ","), ("''", "\"")] {
        while t.contains(a) {
            t = t.replace(a, b);
        }
    }
    t
}

/// SE FixMissingOpenBracket: `müzik çalıyor]` → `[müzik çalıyor]` (satırda açılış yoksa).
fn missing_bracket(line: &str) -> String {
    let mut out = line.to_string();
    for (o, c) in [('[', ']'), ('(', ')')] {
        if let Some(k) = out.find(c) {
            let lead = out.len() - out.trim_start_matches(['-', ' ', '–']).len();
            let inner = out.get(lead..k).unwrap_or("");
            // gülen yüz (:) ;)), madde işareti (a) 1)) ve sayılar değil: en az iki harf, kapanıştan önce harf/nokta
            let ok = inner.chars().filter(|c| c.is_alphabetic()).count() >= 2 && inner.chars().last().map_or(false, |l| l.is_alphabetic() || ".!?…".contains(l));
            if ok && out.matches(c).count() == 1 && !out.contains(o) {
                // satır başındaki tire/boşluktan sonra aç
                out.insert(lead, o);
            }
        }
    }
    out
}

/// Tek konuşmacılı satırda baştaki diyalog tiresi (iki satırdan yalnız biri tireyle başlıyorsa ya da tek satır).
fn lone_dash(text: &str) -> String {
    let lines: Vec<&str> = text.split("\\N").collect();
    let dashed = |l: &str| tags::strip_tags(l).trim_start().starts_with(['-', '–']);
    let n = lines.iter().filter(|l| dashed(l)).count();
    if n != 1 || !dashed(lines[0]) {
        return text.to_string();
    }
    let mut first = lines[0].to_string();
    // etiketlerden sonraki ilk tireyi ve ardındaki boşluğu sil
    let mut done = false;
    first = map_text(&first, |x| {
        if done {
            return x.to_string();
        }
        let tt = x.trim_start();
        if tt.is_empty() {
            return x.to_string();
        }
        done = true;
        match tt.strip_prefix('-').or_else(|| tt.strip_prefix('–')) {
            Some(r) => format!("{}{}", &x[..x.len() - tt.len()], r.trim_start()),
            None => x.to_string(),
        }
    });
    let mut v = vec![first];
    v.extend(lines[1..].iter().map(|s| s.to_string()));
    v.join("\\N")
}

pub fn fix_text(text: &str, f: &Fixes) -> String {
    let mut t = text.to_string();
    if f.mojibake {
        t = map_text(&t, fix_mojibake);
    }
    if f.normalize_unicode {
        t = map_text(&t, normalize_unicode);
    }
    if f.punct_cleanup {
        t = map_text(&t, punct_cleanup);
    }
    if f.missing_bracket {
        t = t.split("\\N").map(missing_bracket).collect::<Vec<_>>().join("\\N");
    }
    if f.lone_dash {
        t = lone_dash(&t);
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
    if f.tr_spelling {
        t = crate::trfix::fix_text(&t).0;
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
        // Tipografi/karaoke/fx satırlarında yalnızca kodlama onarımı güvenlidir; cümle zincirine de katılmazlar.
        if crate::qc::is_typeset(d) || crate::qc::is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx") {
            if in_scope && f.mojibake {
                let nt = map_text(&d.text, fix_mojibake);
                if nt != d.text {
                    d.text = nt;
                    rep.changed_lines += 1;
                }
            }
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
            if let Some(max) = f.balance_lines {
                nt = crate::linebreak::balance(&nt, max);
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
    // Dengelemeden sonra hâlâ sığmayanlar (iki satıra sığmayan metin) bölünebilir
    if let Some(max) = f.split_long {
        rep.split_lines += split_long_lines(script, max, f.only.as_deref());
    }
    if f.min_duration_ms.is_some() || f.min_gap_ms.is_some() || f.max_duration_ms.is_some() {
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
            // "Ben... gidiyorum": üç nokta cümle içinde de kullanılır, sonrası büyütülmez
            if matches!(c, '.' | '!' | '?') && i + 1 < chars.len() && chars[i + 1] == ' ' && !(c == '.' && i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1] == '.')) {
                cap_next = true;
            }
            i += 1;
        }
        let _ = tr_lower;
        out
    })
}

fn fix_timing(script: &mut Script, f: &Fixes) -> usize {
    // Yalnızca diyalog satırları; "sonraki satır" aynı stil+katmandan seçilir (araya giren tabelalar
    // diyaloğu kısaltmasın). Tipografi/karaoke/fx satırlarının zamanına dokunulmaz.
    let mut groups: std::collections::HashMap<(String, i32), Vec<usize>> = std::collections::HashMap::new();
    for (i, e) in script.entries.iter().enumerate() {
        if let Entry::Dialogue(d) = e {
            if !d.comment && f.only.as_ref().map_or(true, |o| o.contains(&i)) && !crate::qc::is_typeset(d) && !crate::qc::is_karaoke(d) && !d.effect.eq_ignore_ascii_case("fx") {
                groups.entry((d.style.clone(), d.layer)).or_default().push(i);
            }
        }
    }
    let mut n = 0;
    for (_, mut order) in groups {
        order.sort_by_key(|&i| if let Entry::Dialogue(d) = &script.entries[i] { d.start_time } else { 0 });
        n += fix_timing_chain(script, f, &order);
    }
    n
}

fn fix_timing_chain(script: &mut Script, f: &Fixes, order: &[usize]) -> usize {
    let mut n = 0;
    for k in 0..order.len() {
        let next_start = order.get(k + 1).and_then(|&j| if let Entry::Dialogue(d) = &script.entries[j] { Some(d.start_time) } else { None });
        let Entry::Dialogue(d) = &mut script.entries[order[k]] else { continue };
        let before = (d.start_time, d.end_time);
        if let Some(max) = f.max_duration_ms.filter(|&m| m > 0) {
            if d.end_time - d.start_time > max {
                d.end_time = d.start_time + max;
            }
        }
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
    #[test]
    fn se_rules_punct_unicode_bracket_dash() {
        let f = Fixes::default();
        assert_eq!(fix_text("Ne?. Gel!. Hadi?... Yok....", &f), "Ne? Gel! Hadi?... Yok...");
        assert_eq!(fix_text("Evet,, tamam. ''Merhaba''", &f), "Evet, tamam. \"Merhaba\"");
        assert_eq!(fix_text("Bekle……", &f), "Bekle…");
        assert_eq!(fix_text("A\u{00A0}B\u{200B}C \u{2013} D", &f), "A BC - D");
        assert_eq!(fix_text("müzik çalıyor] Merhaba", &f), "[müzik çalıyor] Merhaba");
        assert_eq!(fix_text("- kapı çalar) Kim o?", &f), "- (kapı çalar) Kim o?");
        assert_eq!(fix_text("[tamam] ve (bu)", &f), "[tamam] ve (bu)");
        for keep in ["Güzel :)", "a) birinci", "Bunu sevdim ;)", "1) madde", "Tamam :]"] {
            assert_eq!(fix_text(keep, &f), keep);
        }
        // etiket içine dokunulmaz
        assert_eq!(fix_text("{\\an8}Ne?.", &f), "{\\an8}Ne?");
        let g = Fixes { lone_dash: true, ..Fixes::default() };
        assert_eq!(fix_text("- Merhaba.", &g), "Merhaba.");
        assert_eq!(fix_text("{\\i1}- Merhaba.\\Nnasılsın?", &g), "{\\i1}Merhaba.\\Nnasılsın?");
        assert_eq!(fix_text("- Merhaba.\\N- Selam.", &g), "- Merhaba.\\N- Selam.");
        // ikileme bozulmaz
        assert_eq!(fix_text("yavaş yavaş gel", &f), "yavaş yavaş gel");
    }

    #[test]
    fn max_duration_caps() {
        let mut sc = Script::from_dialogues(vec![Dialogue { start_time: 0, end_time: 20000, text: "uzun".into(), ..Default::default() }]);
        let r = apply(&mut sc, &Fixes { max_duration_ms: Some(8000), ..Fixes::default() });
        assert_eq!(r.timing_changes, 1);
        assert_eq!(sc.dialogues().next().unwrap().end_time, 8000);
    }

    use super::*;

    fn s(texts: &[&str]) -> Script {
        Script::from_dialogues(texts.iter().enumerate().map(|(i, t)| Dialogue { start_time: i as i64 * 3000, end_time: i as i64 * 3000 + 2000, text: t.to_string(), ..Default::default() }).collect())
    }

    #[test]
    fn typesetting_and_karaoke_untouched() {
        let mk = |a: i64, b: i64, t: &str| Dialogue { start_time: a, end_time: b, text: t.into(), ..Default::default() };
        let mut sc = Script::from_dialogues(vec![
            mk(0, 500, "merhaba  dünya"),
            mk(600, 700, "{\\pos(10,10)}OKUL  ADI"),
            mk(2000, 4000, "ikinci satır"),
            mk(5000, 5300, "{\\k10}ka  {\\k20}ra"),
        ]);
        let f = Fixes { min_duration_ms: Some(1500), sentence_case: true, ..Default::default() };
        apply(&mut sc, &f);
        let v: Vec<&Dialogue> = sc.dialogues().collect();
        assert_eq!(v[0].text, "Merhaba dünya");
        assert_eq!(v[0].end_time, 1500, "tabela diyaloğun uzatılmasını engellememeli");
        assert_eq!((v[1].text.as_str(), v[1].end_time), ("{\\pos(10,10)}OKUL  ADI", 700));
        assert_eq!(v[3].text, "{\\k10}ka  {\\k20}ra");
        assert_eq!(v[3].end_time, 5300);
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

    #[test]
    fn inceleme_metin_bozulmaz() {
        assert_eq!(remove_hi("Üzgünüm :( ama gitmeliyim."), "Üzgünüm :( ama gitmeliyim.");
        assert_eq!(remove_hi("NE?! HAYIR: DUR!"), "NE?! HAYIR: DUR!");
        assert_eq!(remove_hi("[{\\i1}kapı çalar{\\i0}] Gel"), "Gel");
        assert_eq!(remove_hi("(gülüyor) Tamam\\NALİ: Evet"), "Tamam\\NEvet");
        assert_eq!(fix_punct_spacing("Tamam :3 Hayır :O dedi : evet", true, true), "Tamam :3 Hayır :O dedi: evet");
        assert_eq!(fix_punct_spacing("www.site.com/?q=ara", true, true), "www.site.com/?q=ara");
        assert_eq!(fix_dialogue_dash("{\\an8}-Ne?\\N–Hiç."), "{\\an8}- Ne?\\N– Hiç.");
    }
}