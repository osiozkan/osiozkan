//! Çevrilmemiş satır denetimi (yayın öncesi QC): çeviri sırasında atlanan satırları bulur.
//!
//! - Referans (kaynak altyazı) varsa: zamanı örtüşen referans satırıyla aynı metin.
//! - Japonca karakter (kana/kanji) kalmış diyalog satırı (tipografi/karaoke hariç).
//! - Belge çoğunlukla Türkçeyse: sık İngilizce sözcüklerden oluşan, Türkçe harf içermeyen satır.
use crate::ass::{Entry, Script};
use crate::qc::{is_karaoke, is_typeset, Issue, Kind};
use crate::tags;

const EN_WORDS: &[&str] = &[
    "the", "and", "you", "are", "is", "what", "this", "that", "i'm", "don't", "it's", "we", "they", "have", "with", "for", "not", "your", "my", "me", "be", "was", "can", "will", "just", "do", "of", "to", "in", "it", "a", "i", "he", "she", "there", "here", "how", "why", "yes", "no", "please", "thank", "sorry",
];

fn norm(s: &str) -> String {
    tags::visible_text(s).to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_japanese(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x4E00..=0x9FFF | 0x3400..=0x4DBF))
}

fn has_turkish_letters(s: &str) -> bool {
    s.chars().any(|c| "ğüşıöçĞÜŞİÖÇ".contains(c))
}

/// Satır İngilizce görünüyor mu: ≥3 sözcük, en az ikisi ve üçte biri sık İngilizce sözcük.
fn looks_english(s: &str) -> bool {
    let words: Vec<String> = s.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_string()).filter(|w| !w.is_empty()).collect();
    if words.len() < 3 {
        return false;
    }
    let hits = words.iter().filter(|w| EN_WORDS.contains(&w.as_str())).count();
    hits >= 2 && hits * 3 >= words.len()
}

pub fn check(script: &Script, reference: Option<&Script>) -> Vec<Issue> {
    let mut out = Vec::new();
    let lines: Vec<(usize, &crate::ass::Dialogue)> = script
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Entry::Dialogue(d) if !d.comment && !is_typeset(d) && !is_karaoke(d) && !d.effect.eq_ignore_ascii_case("fx") => Some((i, d)),
            _ => None,
        })
        .collect();
    // Belge Türkçe mi? (Türkçe harfli satır oranı)
    let worded: Vec<String> = lines.iter().map(|(_, d)| tags::visible_text(&d.text)).filter(|t| t.split_whitespace().count() >= 3).collect();
    let turkish_doc = !worded.is_empty() && worded.iter().filter(|t| has_turkish_letters(t)).count() * 10 >= worded.len() * 3;
    let mut refs: Vec<(i64, i64, String)> = reference
        .map(|r| r.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, norm(&d.text))).collect())
        .unwrap_or_default();
    refs.sort_by_key(|r| r.0);
    for (i, d) in lines {
        let vis = tags::visible_text(&d.text);
        let n = norm(&d.text);
        if n.chars().filter(|c| c.is_alphabetic()).count() < 4 {
            continue;
        }
        if !refs.is_empty() {
            let end = refs.partition_point(|r| r.0 < d.end_time);
            if refs[..end].iter().rev().take(10).any(|r| r.1 > d.start_time && r.2 == n) {
                out.push(Issue { entry: i, kind: Kind::Untranslated, message: "Çevrilmemiş: referans satırla aynı".into() });
                continue;
            }
        }
        if has_japanese(&vis) {
            out.push(Issue { entry: i, kind: Kind::Untranslated, message: "Japonca metin kalmış".into() });
        } else if turkish_doc && !has_turkish_letters(&vis) && looks_english(&vis.to_lowercase()) {
            out.push(Issue { entry: i, kind: Kind::Untranslated, message: "İngilizce kalmış olabilir".into() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn d(a: i64, b: i64, t: &str) -> Dialogue {
        Dialogue { start_time: a, end_time: b, text: t.into(), ..Default::default() }
    }

    #[test]
    fn finds_untranslated_lines() {
        let tr = Script::from_dialogues(vec![
            d(0, 1000, "Bugün çok güzel bir gün."),
            d(1000, 2000, "Where are you going, Tanaka?"),
            d(2000, 3000, "{\\i1}Nereye gidiyorsun?{\\i0}"),
            d(3000, 4000, "おはよう、先輩！"),
            d(4000, 5000, "Şimdi değil, sonra konuşuruz."),
            d(5000, 6000, "{\\an8\\pos(100,100)}東京駅"), // tabela: atlanır
            d(6000, 7000, "Okay."),                       // kısa: atlanır
            d(7000, 8000, "Thank you so much!"),
        ]);
        let en = Script::from_dialogues(vec![d(7000, 8000, "Thank you so much!"), d(0, 1000, "It's a nice day.")]);
        let v = check(&tr, Some(&en));
        let got: Vec<(usize, &str)> = v.iter().map(|i| (i.entry, i.message.as_str())).collect();
        let idx = |k: usize| tr.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).nth(k).unwrap().0;
        assert_eq!(got, vec![(idx(1), "İngilizce kalmış olabilir"), (idx(3), "Japonca metin kalmış"), (idx(7), "Çevrilmemiş: referans satırla aynı")]);
        // İngilizce belgede İngilizce satırlar sorun değil
        let en_doc = Script::from_dialogues(vec![d(0, 1000, "Where are you going?"), d(1000, 2000, "I don't know what this is.")]);
        assert!(check(&en_doc, None).is_empty());
    }
}
