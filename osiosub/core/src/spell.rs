//! Spell checking (Hunspell dictionaries via the pure-Rust `spellbook`). Turkish is bundled.
use flate2::read::GzDecoder;
use std::collections::HashSet;
use std::io::Read;

pub struct Speller {
    dict: spellbook::Dictionary,
    pub ignore: HashSet<String>,
}

static TR_AFF: &[u8] = include_bytes!("../../dicts/tr.aff.gz");
static TR_DIC: &[u8] = include_bytes!("../../dicts/tr.dic.gz");

fn gunzip(b: &[u8]) -> String {
    let mut s = String::new();
    GzDecoder::new(b).read_to_string(&mut s).unwrap_or_default();
    s
}

#[derive(Clone, Debug, PartialEq)]
pub struct Misspelling { pub start: usize, pub end: usize, pub word: String }

impl Speller {
    pub fn from_strings(aff: &str, dic: &str) -> Result<Self, String> {
        let dict = spellbook::Dictionary::new(aff, dic).map_err(|e| e.to_string())?;
        Ok(Self { dict, ignore: HashSet::new() })
    }
    pub fn turkish() -> Result<Self, String> { Self::from_strings(&gunzip(TR_AFF), &gunzip(TR_DIC)) }

    pub fn check_word(&self, w: &str) -> bool {
        self.ignore.contains(w) || self.ignore.contains(&w.to_lowercase()) || self.dict.check(w) || self.dict.check(&crate::turkish::to_lower(w))
    }

    pub fn suggest(&self, w: &str, max: usize) -> Vec<String> {
        let mut v = Vec::new();
        self.dict.suggest(w, &mut v);
        v.truncate(max);
        v
    }

    /// Misspelled words in the *visible* text of an event (tags removed). Offsets are byte offsets in the stripped text.
    pub fn check_text(&self, text: &str) -> Vec<Misspelling> {
        let vis = crate::tags::visible_text(text);
        let mut out = Vec::new();
        let mut start: Option<usize> = None;
        let flush = |s: usize, e: usize, out: &mut Vec<Misspelling>, this: &Speller, vis: &str| {
            let w = &vis[s..e];
            if w.chars().count() < 2 || w.chars().any(|c| c.is_ascii_digit() || c == '\'' || c == '’') { return; }
            if w.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) { return; } // acronyms / shouting
            if !this.check_word(w) { out.push(Misspelling { start: s, end: e, word: w.to_string() }); }
        };
        let mut iter = vis.char_indices().peekable();
        while let Some((i, c)) = iter.next() {
            let in_word = c.is_alphanumeric() || ((c == '\'' || c == '’' || c == '-') && start.is_some() && iter.peek().map_or(false, |(_, n)| n.is_alphanumeric()));
            if in_word { if start.is_none() { start = Some(i); } }
            else if let Some(s) = start.take() { flush(s, i, &mut out, self, &vis); }
        }
        if let Some(s) = start { flush(s, vis.len(), &mut out, self, &vis); }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turkish_basic() {
        let s = Speller::turkish().unwrap();
        assert!(s.check_word("merhaba"));
        assert!(s.check_word("Merhaba"));
        assert!(!s.check_word("merhabaaa"));
        let m = s.check_text("{\\i1}Merhaba{\\i0} dünyaaa nasılsın");
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(m[0].word, "dünyaaa");
        assert!(!s.suggest("merhabaa", 5).is_empty());
    }
}
