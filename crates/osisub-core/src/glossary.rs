//! Terim sözlüğü: "kaynak = hedef" çiftleri; çeviride hedef karşılığın kullanıldığını denetler.
use crate::ass::{Entry, Script};
use crate::{qc, tags};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    pub source: String,
    pub target: String,
}

/// Satır biçimleri: `kaynak = hedef`, `kaynak<TAB>hedef`, `kaynak;hedef`. `#` ile başlayanlar yorumdur.
pub fn parse(text: &str) -> Vec<Term> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (a, b) = l.split_once('=').or_else(|| l.split_once('\t')).or_else(|| l.split_once(';'))?;
            let (a, b) = (a.trim(), b.trim());
            (!a.is_empty() && !b.is_empty()).then(|| Term { source: a.to_string(), target: b.to_string() })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// `Script::entries` indeksi.
    pub entry: usize,
    pub term: Term,
}

fn contains_word(hay: &str, needle: &str, allow_suffix: bool) -> bool {
    let (h, n) = (qc::tr_lower(hay), qc::tr_lower(needle));
    let mut start = 0;
    while let Some(i) = h[start..].find(&n) {
        let a = start + i;
        let b = a + n.len();
        let before_ok = h[..a].chars().next_back().map_or(true, |c| !c.is_alphanumeric());
        let after_ok = allow_suffix || h[b..].chars().next().map_or(true, |c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        start = b.max(a + 1);
        while !h.is_char_boundary(start) {
            start += 1;
        }
        if start >= h.len() {
            break;
        }
    }
    false
}

/// Satırın metni ile (zamana göre eşleşen) kaynak metin arasında terim denetimi.
/// `reference`: (başlangıç, bitiş, metin) listesi (başlangıca göre sıralı).
/// Hedef terim eklerle (ör. "Sakura'ya", "Sakuranın") geçebilir.
pub fn check(script: &Script, reference: &[(i64, i64, String)], terms: &[Term]) -> Vec<Finding> {
    let mut out = vec![];
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment {
            continue;
        }
        let end = reference.partition_point(|r| r.0 < d.end_time);
        let src: String = reference[..end].iter().rev().take(6).filter(|r| r.1 > d.start_time).map(|r| r.2.as_str()).collect::<Vec<_>>().join(" ");
        if src.is_empty() {
            continue;
        }
        let tgt = tags::visible_text(&d.text);
        for t in terms {
            if contains_word(&src, &t.source, false) && !contains_word(&tgt, &t.target, true) {
                out.push(Finding { entry: i, term: t.clone() });
            }
        }
    }
    out
}

/// Tek satır için (düzenleme sırasında) eksik terimler.
pub fn missing_in_line(source_text: &str, target_text: &str, terms: &[Term]) -> Vec<Term> {
    let tgt = tags::visible_text(target_text);
    terms.iter().filter(|t| contains_word(source_text, &t.source, false) && !contains_word(&tgt, &t.target, true)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    #[test]
    fn parse_and_check() {
        let terms = parse("# karakterler\nSakura = Sakura\nsenpai ; kıdemli\nKazehaya\tKazehaya\n");
        assert_eq!(terms.len(), 3);
        let s = Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 2000, text: "Sakura'ya selam söyle".into(), ..Default::default() },
            Dialogue { start_time: 3000, end_time: 5000, text: "Merhaba abi".into(), ..Default::default() },
            Dialogue { start_time: 6000, end_time: 8000, text: "Kazehaya geldi".into(), ..Default::default() },
        ]);
        let refs = vec![
            (0, 2000, "Say hi to Sakura".to_string()),
            (3000, 5000, "Hello senpai".to_string()),
            (6000, 8000, "Kazehaya came".to_string()),
        ];
        let f = check(&s, &refs, &terms);
        assert_eq!(f.len(), 1, "{:?}", f);
        assert_eq!(f[0].term.source, "senpai");
        assert!(missing_in_line("senpai is here", "kıdemli burada", &terms).is_empty());
        assert_eq!(missing_in_line("senpai is here", "abi burada", &terms).len(), 1);
    }

    #[test]
    fn turkish_case_and_word_boundary() {
        assert!(contains_word("İSTANBUL güzel", "istanbul", true));
        assert!(!contains_word("insanlar", "san", false));
        assert!(contains_word("Sakura'ya", "sakura", true));
    }
}
