//! Kanji zamanlayıcı (Aegisub "Kanji Timer"): romaji karaoke satırının `\k` zamanlamasını aynı
//! şarkının kanji/kana satırına aktarır. Kana birimleri romaji hecelerle okunuşlarından eşlenir;
//! kanji dizileri ardından gelen kananın okunuşuna kadar olan heceleri alır. Sonuç gruplar halinde
//! düzenlenebilir: kaynak `ki+mi|no|na+ma+e`, hedef `君|の|名前`.
use crate::karaoke::{self, Syl};

fn kana_romaji(c: char) -> Option<&'static str> {
    // katakana → hiragana
    let h = if ('\u{30a1}'..='\u{30f6}').contains(&c) { char::from_u32(c as u32 - 0x60)? } else { c };
    Some(match h {
        'あ' => "a", 'い' => "i", 'う' => "u", 'え' => "e", 'お' => "o",
        'か' => "ka", 'き' => "ki", 'く' => "ku", 'け' => "ke", 'こ' => "ko",
        'が' => "ga", 'ぎ' => "gi", 'ぐ' => "gu", 'げ' => "ge", 'ご' => "go",
        'さ' => "sa", 'し' => "shi", 'す' => "su", 'せ' => "se", 'そ' => "so",
        'ざ' => "za", 'じ' => "ji", 'ず' => "zu", 'ぜ' => "ze", 'ぞ' => "zo",
        'た' => "ta", 'ち' => "chi", 'つ' => "tsu", 'て' => "te", 'と' => "to",
        'だ' => "da", 'ぢ' => "ji", 'づ' => "zu", 'で' => "de", 'ど' => "do",
        'な' => "na", 'に' => "ni", 'ぬ' => "nu", 'ね' => "ne", 'の' => "no",
        'は' => "ha", 'ひ' => "hi", 'ふ' => "fu", 'へ' => "he", 'ほ' => "ho",
        'ば' => "ba", 'び' => "bi", 'ぶ' => "bu", 'べ' => "be", 'ぼ' => "bo",
        'ぱ' => "pa", 'ぴ' => "pi", 'ぷ' => "pu", 'ぺ' => "pe", 'ぽ' => "po",
        'ま' => "ma", 'み' => "mi", 'む' => "mu", 'め' => "me", 'も' => "mo",
        'や' => "ya", 'ゆ' => "yu", 'よ' => "yo",
        'ら' => "ra", 'り' => "ri", 'る' => "ru", 'れ' => "re", 'ろ' => "ro",
        'わ' => "wa", 'を' => "o", 'ん' => "n", 'ゔ' => "vu",
        'ゃ' => "ya", 'ゅ' => "yu", 'ょ' => "yo", 'ぁ' => "a", 'ぃ' => "i", 'ぅ' => "u", 'ぇ' => "e", 'ぉ' => "o", 'ゎ' => "wa",
        _ => return None,
    })
}

fn last_vowel(s: &str) -> Option<char> {
    s.chars().rev().find(|c| "aeiou".contains(*c))
}

/// Kana birimin (ör. "きょ", "とー") romajisi; kanji/bilinmeyen karakter varsa `None`.
/// `っ` sonraki birimin ilk ünsüzü olur (çağıran doldurur, burada boş döner).
pub fn unit_romaji(u: &str) -> Option<String> {
    let mut out = String::new();
    for c in u.chars() {
        if !c.is_alphanumeric() && !crate::karaoke::is_cjk(c) {
            continue; // noktalama/boşluk
        }
        match c {
            'っ' | 'ッ' => {}
            'ー' | '〜' | '～' => {
                let v = last_vowel(&out)?;
                out.push(v);
            }
            'ゃ' | 'ゅ' | 'ょ' | 'ャ' | 'ュ' | 'ョ' => {
                // き+ゃ → kya, し+ゃ → sha, ち+ゃ → cha, じ+ゃ → ja
                let y = kana_romaji(c)?;
                if out.ends_with("shi") || out.ends_with("chi") || out.ends_with('j') || out.ends_with("ji") {
                    out.pop();
                    out.push_str(&y[1..]);
                } else {
                    out.pop();
                    out.push_str(y);
                }
            }
            'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' => {
                out.pop();
                out.push_str(kana_romaji(c)?);
            }
            _ => out.push_str(kana_romaji(c)?),
        }
    }
    Some(out)
}

fn norm(s: &str) -> String {
    let s: String = s.chars().filter(|c| c.is_alphabetic()).flat_map(|c| c.to_lowercase()).map(|c| match c { 'ā' | 'â' => 'a', 'ē' | 'ê' => 'e', 'ī' | 'î' => 'i', 'ō' | 'ô' => 'o', 'ū' | 'û' => 'u', c => c }).collect();
    canon(&s)
}

/// Hepburn/Kunrei farklarını kaldırır (shi=si, chi=ti, tsu=tu, fu=hu, ji=zi, sha=sya…).
fn canon(s: &str) -> String {
    s.replace("tsu", "tu").replace("sh", "sy").replace("ch", "ty").replace("fu", "hu").replace('j', "zy").replace("syi", "si").replace("tyi", "ti").replace("zyi", "zi")
}

/// Grubun kabul edilen okunuşları: edatlar を=wo/o, は=wa/ha, へ=e/he.
fn readings(text: &str, r: &str) -> Vec<String> {
    let mut v = vec![canon(r)];
    match text.trim() {
        "を" | "ヲ" => v.push("wo".into()),
        "は" | "ハ" => v.push("wa".into()),
        "へ" | "ヘ" => v.push("e".into()),
        _ => {}
    }
    v
}

/// Hedef metni birimlere böler (kana küçük harfleri ve ー önceki birime yapışır).
pub fn dest_units(dest: &str) -> Vec<String> {
    karaoke::auto_syllables(&crate::tags::strip_tags(dest))
}

/// Otomatik hizalama: her hedef grubu için kaynak hece sayısı ve hedef grup metinleri.
/// Ardışık kanji birimleri tek grup olur.
pub fn auto_align(src: &[Syl], dest: &str) -> (Vec<usize>, Vec<String>) {
    let units = dest_units(dest);
    // kanji dizilerini birleştir
    let mut groups: Vec<(String, Option<String>)> = vec![];
    for u in units {
        let r = unit_romaji(&u);
        match (groups.last_mut(), &r) {
            (Some((t, None)), None) => t.push_str(&u),
            _ => groups.push((u, r)),
        }
    }
    // っ: sonraki grubun ilk ünsüzü
    for i in 0..groups.len() {
        if groups[i].1.as_deref() == Some("") && groups[i].0.contains(['っ', 'ッ']) {
            let next = groups.get(i + 1).and_then(|g| g.1.clone()).and_then(|r| r.chars().next());
            groups[i].1 = Some(next.map(|c| c.to_string()).unwrap_or_default());
        }
    }
    let srom: Vec<String> = src.iter().map(|s| norm(&s.visible())).collect();
    let mut counts = vec![];
    let mut j = 0;
    for gi in 0..groups.len() {
        let start = j;
        match &groups[gi].1 {
            Some(r) => {
                let rs = readings(&groups[gi].0, r);
                // en uzun eşleşen okunuşu seç
                let mut best = 0;
                for r in &rs {
                    let (mut acc, mut k) = (String::new(), j);
                    while k < srom.len() {
                        let cand = format!("{acc}{}", srom[k]);
                        if !r.starts_with(&cand) { break; }
                        acc = cand;
                        k += 1;
                        if acc.len() >= r.len() { break; }
                    }
                    if acc == *r { best = best.max(k - j); }
                }
                // eşleşme yoksa tek hece al
                j += if best > 0 { best } else { 1 };
            }
            None => {
                // sonraki okunuşu bilinen grubun başına kadar
                let next = groups[gi + 1..].iter().find(|g| g.1.as_deref().map_or(false, |r| !r.is_empty())).map(|g| readings(&g.0, g.1.as_deref().unwrap()));
                let later_groups = groups.len() - gi - 1;
                match next {
                    Some(rs) => {
                        j += 1;
                        while j < srom.len() && !(!srom[j].is_empty() && rs.iter().any(|r| r.starts_with(&srom[j]))) {
                            j += 1;
                        }
                    }
                    None => j = srom.len().saturating_sub(later_groups).max(j + 1),
                }
            }
        }
        j = j.min(srom.len());
        counts.push(j - start);
    }
    // artan kaynak heceler son gruba
    if let Some(l) = counts.last_mut() {
        *l += srom.len() - j;
    }
    (counts, groups.into_iter().map(|g| g.0).collect())
}

/// Kaynak grupları `ki+mi|no|…` biçiminde.
pub fn src_string(src: &[Syl], counts: &[usize]) -> String {
    let mut out = vec![];
    let mut j = 0;
    for &n in counts {
        let part: Vec<String> = src[j.min(src.len())..(j + n).min(src.len())].iter().map(|s| s.visible().trim().to_string()).collect();
        out.push(part.join("+"));
        j += n;
    }
    out.join("|")
}

/// `ki+mi|no|…` metninden grup başına hece sayıları (boş grup = 0).
pub fn parse_src_counts(s: &str) -> Vec<usize> {
    s.split('|').map(|g| if g.trim().is_empty() { 0 } else { g.split('+').count() }).collect()
}

/// Grupların süreleri toplanarak hedef `\k` satırı kurulur. Hata: grup sayıları uyuşmuyor ya da
/// toplam hece sayısı kaynakla aynı değil.
pub fn build(src: &[Syl], counts: &[usize], dest_groups: &[String]) -> Result<String, String> {
    if counts.len() != dest_groups.len() {
        return Err(format!("Kaynak {} grup, hedef {} grup: aynı olmalı", counts.len(), dest_groups.len()));
    }
    let total: usize = counts.iter().sum();
    if total != src.len() {
        return Err(format!("Kaynak gruplarında {} hece var, satırda {}", total, src.len()));
    }
    let kind = src.iter().find(|s| s.dur > 0).map(|s| s.kind.clone()).unwrap_or_else(|| "k".into());
    let mut out = vec![];
    let mut j = 0;
    for (n, t) in counts.iter().zip(dest_groups) {
        let dur: i64 = src[j..j + n].iter().map(|s| s.dur).sum();
        out.push(Syl { dur, kind: kind.clone(), text: t.clone() });
        j += n;
    }
    Ok(karaoke::build(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syls(s: &str) -> Vec<Syl> {
        s.split('|').map(|t| Syl { dur: 10, kind: "k".into(), text: t.into() }).collect()
    }

    #[test]
    fn romaji_of_kana() {
        assert_eq!(unit_romaji("きょ").as_deref(), Some("kyo"));
        assert_eq!(unit_romaji("しゃ").as_deref(), Some("sha"));
        assert_eq!(unit_romaji("ジャ").as_deref(), Some("ja"));
        assert_eq!(unit_romaji("とー").as_deref(), Some("too"));
        assert_eq!(unit_romaji("ファ").as_deref(), Some("fa"));
        assert_eq!(unit_romaji("君"), None);
    }

    #[test]
    fn aligns_kanji_and_kana() {
        let src = syls("ki|mi |no |na|ma|e");
        let (c, d) = auto_align(&src, "君の名前");
        assert_eq!((c.clone(), d.clone()), (vec![2, 1, 3], vec!["君".to_string(), "の".into(), "名前".into()]));
        assert_eq!(src_string(&src, &c), "ki+mi|no|na+ma+e");
        assert_eq!(build(&src, &c, &d).unwrap(), "{\\k20}君{\\k10}の{\\k30}名前");
        // kana, küçük kana, sokuon
        let src = syls("a|ri|ga|to|u");
        assert_eq!(auto_align(&src, "ありがとう").0, vec![1, 1, 1, 1, 1]);
        let src = syls("kyo|u |mo |ki|t|to");
        let (c, d) = auto_align(&src, "今日もきっと");
        assert_eq!(c, vec![2, 1, 1, 1, 1], "{d:?}");
        // kanji ortada: 夢を見た → yu|me|wo|mi|ta ("を" = o; "wo" eşleşmez → tek hece alınır)
        let src = syls("yu|me|wo|mi|ta");
        let (c, d) = auto_align(&src, "夢を見た");
        assert_eq!(d, vec!["夢", "を", "見", "た"]);
        assert_eq!(c, vec![2, 1, 1, 1]);
        // Kunrei yazımı (si, tu) ve uzun ünlü
        let src = syls("si|n|zi|te|ru");
        assert_eq!(auto_align(&src, "信じてる").0, vec![2, 1, 1, 1]);
    }

    #[test]
    fn manual_groups() {
        let src = syls("ki|mi|no|na|ma|e");
        assert_eq!(parse_src_counts("ki+mi|no|na+ma+e"), vec![2, 1, 3]);
        let d: Vec<String> = "君|の|名前".split('|').map(String::from).collect();
        assert!(build(&src, &[2, 1, 2], &d).is_err());
        assert!(build(&src, &[2, 4], &d).is_err());
        assert_eq!(build(&src, &[3, 0, 3], &d).unwrap(), "{\\k30}君{\\k0}の{\\k30}名前");
    }
}
