//! Term glossary: series-specific terms must be translated consistently (Osi's termcheck).
//!
//! File (UTF-8 TSV, first line a header, `#` lines are comments):
//! ```text
//! kaynak<TAB>türkçe<TAB>yasaklar<TAB>not
//! Stand<TAB>Stand<TAB>Stant|Ruh<TAB>
//! Hokage<TAB>Hokage<TAB>Hokage-sama|Ateş Gölgesi<TAB>
//! ```
//! Checks: a FORBIDDEN variant in a Turkish line (fix: replace it by the preferred term, the
//! Turkish suffix is kept: "Stantlar" → "Standlar"); a source term in the reference line whose
//! preferred translation (with any suffix) is missing from the Turkish line (no automatic fix).
//! Matching ignores case the Turkish way (i/İ, ı/I) and starts at a word start, so suffixed
//! forms ("Hokage'nin", "Nen'i") match.

#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub source: String,
    pub target: String,
    pub forbidden: Vec<String>,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct Glossary {
    pub terms: Vec<Term>,
}

/// One character to one character, Turkish rules: I → ı, İ → i, others lower-cased.
fn fold_char(c: char) -> char {
    match c {
        'I' => 'ı',
        'İ' => 'i',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

pub fn tr_fold(s: &str) -> String { s.chars().map(fold_char).collect() }

/// Upper-case the first letter the Turkish way (i → İ, ı → I).
fn tr_capitalise(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some('i') => format!("İ{}", it.as_str()),
        Some('ı') => format!("I{}", it.as_str()),
        Some(c) => format!("{}{}", c.to_uppercase(), it.as_str()),
        None => String::new(),
    }
}

/// A hit of a term in a text, in CHARACTER positions of the original text.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub term: usize,
    pub forbidden: String,
    pub start: usize,
    pub end: usize,
    pub suffix: String,
}

impl Glossary {
    pub fn parse(text: &str) -> Glossary {
        let mut terms = Vec::new();
        for (n, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.trim_start().starts_with('#') { continue; }
            let mut cols: Vec<&str> = line.split('\t').collect();
            if n == 0 && matches!(tr_fold(cols[0].trim()).as_str(), "kaynak" | "source") { continue; }
            cols.resize(4, "");
            let (src, tgt) = (cols[0].trim(), cols[1].trim());
            if src.is_empty() && tgt.is_empty() { continue; }
            terms.push(Term { source: src.into(), target: tgt.into(), forbidden: cols[2].split('|').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(), note: cols[3].trim().into() });
        }
        Glossary { terms }
    }

    pub fn load(path: &std::path::Path) -> std::io::Result<Glossary> {
        Ok(Glossary::parse(&String::from_utf8_lossy(&std::fs::read(path)?)))
    }

    pub fn to_tsv(&self) -> String {
        let mut o = String::from("kaynak\ttürkçe\tyasaklar\tnot\n");
        for t in &self.terms { o.push_str(&format!("{}\t{}\t{}\t{}\n", t.source, t.target, t.forbidden.join("|"), t.note)); }
        o
    }

    /// Matches of `term` (folded) at a word start, with the following Turkish suffix.
    fn find_term(term: &str, folded: &[char]) -> Vec<(usize, usize, usize)> {
        let t: Vec<char> = tr_fold(term).chars().collect();
        if t.is_empty() { return vec![]; }
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut out = Vec::new();
        let mut i = 0;
        while i + t.len() <= folded.len() {
            if folded[i..i + t.len()] == t[..] && (i == 0 || !is_word(folded[i - 1])) {
                let mut j = i + t.len();
                if j < folded.len() && (folded[j] == '\'' || folded[j] == '’') && j + 1 <= folded.len() { j += 1; }
                while j < folded.len() && is_word(folded[j]) { j += 1; }
                // a lone apostrophe without a suffix is not part of the hit
                if j == i + t.len() + 1 && matches!(folded[j - 1], '\'' | '’') { j -= 1; }
                out.push((i, i + t.len(), j));
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
        out
    }

    /// Forbidden variants used in `text`.
    pub fn find_forbidden(&self, text: &str) -> Vec<Hit> {
        let orig: Vec<char> = text.chars().collect();
        let folded: Vec<char> = orig.iter().map(|&c| fold_char(c)).collect();
        let mut out = Vec::new();
        for (k, t) in self.terms.iter().enumerate() {
            for bad in &t.forbidden {
                for (a, m, b) in Self::find_term(bad, &folded) {
                    out.push(Hit { term: k, forbidden: bad.clone(), start: a, end: b, suffix: orig[m..b].iter().collect() });
                }
            }
        }
        out
    }

    /// Terms whose source appears in `reference` but whose translation is missing from `turkish`.
    pub fn missing_terms(&self, reference: &str, turkish: &str) -> Vec<usize> {
        let rf: Vec<char> = tr_fold(reference).chars().collect();
        let tf: Vec<char> = tr_fold(turkish).chars().collect();
        self.terms.iter().enumerate().filter(|(_, t)| !t.source.is_empty() && !t.target.is_empty()
            && !Self::find_term(&t.source, &rf).is_empty() && Self::find_term(&t.target, &tf).is_empty()).map(|(i, _)| i).collect()
    }

    /// Replace forbidden variants by the preferred term (suffix kept, capital first letter kept).
    pub fn fix_forbidden(&self, text: &str) -> String {
        let mut hits = self.find_forbidden(text);
        // longer hits first, then from the end so earlier positions stay valid
        hits.sort_by(|a, b| (b.end - b.start).cmp(&(a.end - a.start)));
        let mut chosen: Vec<Hit> = Vec::new();
        for h in hits {
            if chosen.iter().all(|c| h.end <= c.start || h.start >= c.end) { chosen.push(h); }
        }
        chosen.sort_by(|a, b| b.start.cmp(&a.start));
        let mut chars: Vec<char> = text.chars().collect();
        for h in chosen {
            let stem_len = h.forbidden.chars().count();
            let orig: String = chars[h.start..h.start + stem_len].iter().collect();
            let mut rep = self.terms[h.term].target.clone();
            if orig.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) && rep.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                rep = tr_capitalise(&rep);
            }
            chars.splice(h.start..h.start + stem_len, rep.chars());
        }
        chars.into_iter().collect()
    }
}

/// Where a glossary is looked for: next to the script, then the user's config folder.
pub fn default_paths(script: Option<&std::path::Path>, config_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = script.and_then(|p| p.parent()) {
        v.push(d.join("terimler.tsv"));
        v.push(d.join("glossary.tsv"));
    }
    v.push(config_dir.join("terimler.tsv"));
    v
}

/// Glossary issues for a document: (event index, message, fixable).
pub fn check(g: &Glossary, events: &[crate::Event], reference_text: impl Fn(usize) -> Option<String>) -> Vec<(usize, String, bool)> {
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if e.comment { continue; }
        let plain = crate::tags::display_text(&e.text).replace('\n', " ");
        for h in g.find_forbidden(&plain) {
            let t = &g.terms[h.term];
            out.push((i, format!("yasak terim: “{}{}” → “{}{}”", h.forbidden, h.suffix, t.target, h.suffix), true));
        }
        if let Some(r) = reference_text(i) {
            for k in g.missing_terms(&r, &plain) {
                let t = &g.terms[k];
                out.push((i, format!("terim eksik: kaynakta “{}”, çeviride “{}” yok", t.source, t.target), false));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> Glossary {
        Glossary::parse("kaynak\ttürkçe\tyasaklar\tnot\n# yorum\nStand\tStand\tStant|Ruh\t\nHokage\tHokage\tAteş Gölgesi\t\nNen\tNen\tnen gücü\tbüyük harf\n")
    }

    #[test]
    fn parse_and_round_trip() {
        let gl = g();
        assert_eq!(gl.terms.len(), 3);
        assert_eq!(gl.terms[0].forbidden, vec!["Stant", "Ruh"]);
        let again = Glossary::parse(&gl.to_tsv());
        assert_eq!(again.terms, gl.terms);
    }

    #[test]
    fn forbidden_with_suffix_and_turkish_case() {
        let gl = g();
        let hits = gl.find_forbidden("Bu STANTLAR çok güçlü, stant'ın gücü.");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].suffix, "LAR");
        assert_eq!(hits[1].suffix, "'ın");
        // the preferred term is written as listed (OsiSub: only a capital first letter is carried over)
        assert_eq!(gl.fix_forbidden("Stantlar ve stant'ın"), "Standlar ve Stand'ın");
        // not inside another word
        assert!(gl.find_forbidden("İstant").is_empty());
        // "I" folds to "ı": IRUH is not "ruh" with a dotted i
        assert_eq!(gl.find_forbidden("RUH").len(), 1);
        assert_eq!(gl.fix_forbidden("Ateş Gölgesi'ni gördüm"), "Hokage'ni gördüm");
    }

    #[test]
    fn missing_terms_against_reference() {
        let gl = g();
        assert_eq!(gl.missing_terms("The Hokage is here.", "Ateş Gölgesi burada."), vec![1]);
        assert!(gl.missing_terms("The Hokage is here.", "Hokage'miz burada.").is_empty());
        let evs = vec![crate::Event::new(0, 1000, "Default", "Stantını çağır!")];
        let issues = check(&gl, &evs, |_| Some("Summon your Stand!".into()));
        assert_eq!(issues.len(), 2, "{:?}", issues);
        assert!(issues[0].2 && !issues[1].2);
    }
}
