//! Çeviri belleği (TM): kaynak → hedef çiftleri, bulanık (fuzzy) eşleşme ile öneri.
//! Bölümler arasında tekrar eden cümleler (OP/ED sözleri, kalıp sözler, sonraki bölüm tanıtımı)
//! için. Kalıcı biçim: TSV (`kaynak<TAB>hedef`), `\N` ve sekmeler kaçışlı.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub source: String,
    pub target: String,
    /// 0..=100 benzerlik
    pub score: u32,
}

#[derive(Default, Clone, Debug)]
pub struct Memory {
    /// normalize kaynak → (özgün kaynak, hedef)
    entries: HashMap<String, (String, String)>,
}

/// Karşılaştırma için: etiketsiz, küçük harf, noktalama ve fazla boşluk yok.
pub fn normalize(s: &str) -> String {
    let plain = crate::tags::visible_text(s).replace('\n', " ");
    let mut out = String::with_capacity(plain.len());
    let mut space = false;
    for c in plain.chars() {
        if c.is_alphanumeric() {
            for l in c.to_lowercase() {
                out.push(l);
            }
            space = false;
        } else if !space && !out.is_empty() {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

/// Karakter düzeyinde Levenshtein uzaklığı (iki satırlık tampon).
fn levenshtein(a: &[char], b: &[char]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + if ca == cb { 0 } else { 1 });
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// 0..=100 benzerlik (normalize edilmiş metinler üzerinde).
pub fn similarity(a: &str, b: &str) -> u32 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let m = a.len().max(b.len());
    if m == 0 {
        return 100;
    }
    (((m - levenshtein(&a, &b)) as f64 / m as f64) * 100.0).round() as u32
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Memory {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Çift ekler; aynı (normalize) kaynak varsa hedefi günceller. Boş/aynı metinler eklenmez.
    pub fn add(&mut self, source: &str, target: &str) -> bool {
        let k = normalize(source);
        let t = target.trim();
        if k.is_empty() || t.is_empty() || normalize(t) == k {
            return false;
        }
        let new = (source.trim().to_string(), t.to_string());
        if self.entries.get(&k) == Some(&new) {
            return false;
        }
        self.entries.insert(k, new);
        true
    }

    /// En iyi `n` öneri (benzerlik >= `min_score`), yüksekten düşüğe.
    pub fn suggest(&self, source: &str, n: usize, min_score: u32) -> Vec<Suggestion> {
        let k = normalize(source);
        if k.is_empty() {
            return vec![];
        }
        if let Some((s, t)) = self.entries.get(&k) {
            return vec![Suggestion { source: s.clone(), target: t.clone(), score: 100 }];
        }
        let klen = k.chars().count();
        let mut v: Vec<Suggestion> = self
            .entries
            .iter()
            // uzunluk farkı tek başına eşiği aşıyorsa Levenshtein'a gerek yok
            .filter(|(nk, _)| {
                let l = nk.chars().count();
                let m = l.max(klen).max(1);
                ((m - l.abs_diff(klen)) as f64 / m as f64 * 100.0) >= min_score as f64
            })
            .map(|(nk, (s, t))| Suggestion { source: s.clone(), target: t.clone(), score: similarity(&k, nk) })
            .filter(|s| s.score >= min_score)
            .collect();
        v.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.source.cmp(&b.source)));
        v.truncate(n);
        v
    }

    /// Başka bir belleği ekler (aynı kaynakta gelen hedef kazanır).
    pub fn merge(&mut self, other: Memory) {
        self.entries.extend(other.entries);
    }

    pub fn parse(text: &str) -> Memory {
        let mut m = Memory::default();
        for l in text.lines() {
            if l.starts_with('#') {
                continue;
            }
            if let Some((s, t)) = l.split_once('\t') {
                m.add(&unesc(s), &unesc(t));
            }
        }
        m
    }

    pub fn to_tsv(&self) -> String {
        let mut v: Vec<&(String, String)> = self.entries.values().collect();
        v.sort();
        let mut out = String::from("# osisub çeviri belleği v1 (kaynak<TAB>hedef)\n");
        for (s, t) in v {
            out.push_str(&esc(s));
            out.push('\t');
            out.push_str(&esc(t));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_fuzzy() {
        let mut m = Memory::default();
        assert!(m.add("I'll protect everyone!", "Herkesi koruyacağım!"));
        assert!(m.add("See you next time!", "Bir dahaki sefere görüşürüz!"));
        assert!(!m.add("same", "Same"), "kaynak = hedef eklenmemeli");
        let s = m.suggest("{\\i1}I'll protect everyone{\\i0}", 3, 60);
        assert_eq!(s[0].score, 100);
        assert_eq!(s[0].target, "Herkesi koruyacağım!");
        let f = m.suggest("I will protect everyone!", 3, 60);
        assert_eq!(f.len(), 1);
        assert!(f[0].score >= 80 && f[0].score < 100, "{}", f[0].score);
        assert!(m.suggest("Completely unrelated sentence here", 3, 60).is_empty());
    }

    #[test]
    fn tsv_roundtrip() {
        let mut m = Memory::default();
        m.add("Line one\\Nline two", "Bir\tiki");
        m.add("back\\slash", "ters\\eğik");
        let t = m.to_tsv();
        let m2 = Memory::parse(&t);
        assert_eq!(m2.len(), 2);
        assert_eq!(m2.suggest("Line one line two", 1, 100)[0].target, "Bir\tiki");
        assert_eq!(m2.to_tsv(), t);
        let mut m3 = Memory::default();
        m3.add("Line one line two", "Yeni");
        let mut m4 = m2.clone();
        m4.merge(m3);
        assert_eq!(m4.len(), 2);
        assert_eq!(m4.suggest("line one, line two", 1, 100)[0].target, "Yeni");
    }

    #[test]
    fn similarity_basics() {
        assert_eq!(similarity("abc", "abc"), 100);
        assert_eq!(similarity("", ""), 100);
        assert_eq!(similarity("abcd", "abce"), 75);
        assert_eq!(normalize("{\\b1}Hey,  YOU!\\Nthere"), "hey you there");
    }
}
