//! Fansub style-guide glossary: `wrong => right` pairs (names, terms, honorifics). One pair per line,
//! `#` comments. Matching is whole-word and case-insensitive; replacement keeps the first letter's case.

use crate::ass::Document;
use crate::tags::{split_blocks, Seg};
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub wrong: String,
    pub right: String,
}

pub fn parse(src: &str) -> Vec<Entry> {
    src.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (a, b) = l.split_once("=>").or_else(|| l.split_once('='))?;
            let (a, b) = (a.trim(), b.trim());
            if a.is_empty() { None } else { Some(Entry { wrong: a.to_string(), right: b.to_string() }) }
        })
        .collect()
}

fn word_re(w: &str) -> Regex {
    // Unicode-aware boundaries without look-around: (^|non-word)(word)(non-word|$)
    Regex::new(&format!(r"(?i)(^|[^\p{{L}}\p{{N}}_])({})([^\p{{L}}\p{{N}}_]|$)", regex::escape(w))).unwrap()
}

fn match_case(found: &str, right: &str) -> String {
    let mut f = found.chars();
    match (f.next(), right.chars().next()) {
        (Some(a), Some(b)) if a.is_uppercase() && b.is_lowercase() => {
            let mut out: String = b.to_uppercase().collect();
            out.push_str(&right[b.len_utf8()..]);
            out
        }
        _ => right.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// 1-based event index
    pub event: usize,
    pub wrong: String,
    pub right: String,
}

pub fn check(doc: &Document, entries: &[Entry]) -> Vec<Hit> {
    let res: Vec<(Regex, &Entry)> = entries.iter().map(|e| (word_re(&e.wrong), e)).collect();
    let mut out = Vec::new();
    for (i, ev) in doc.events().enumerate().filter(|(_, e)| !e.comment) {
        let plain = crate::tags::strip_tags(&ev.text);
        for (re, e) in &res {
            if re.is_match(&plain) {
                out.push(Hit { event: i + 1, wrong: e.wrong.clone(), right: e.right.clone() });
            }
        }
    }
    out
}

/// Replace every wrong term by its preferred form (only in plain text, never inside tags).
/// Returns the number of replacements.
pub fn apply(doc: &mut Document, entries: &[Entry]) -> usize {
    let res: Vec<(Regex, &Entry)> = entries.iter().map(|e| (word_re(&e.wrong), e)).collect();
    let mut n = 0;
    for ev in doc.events_mut().filter(|e| !e.comment) {
        let mut text = String::new();
        for seg in split_blocks(&ev.text) {
            match seg {
                Seg::Tags(t) => {
                    text.push('{');
                    text.push_str(t);
                    text.push('}');
                }
                Seg::Text(t) => {
                    let mut cur = t.to_string();
                    for (re, e) in &res {
                        // run twice so adjacent matches sharing a boundary char are both handled
                        for _ in 0..2 {
                            cur = re
                                .replace_all(&cur, |c: &regex::Captures| {
                                    let rep = match_case(&c[2], &e.right);
                                    if rep != &c[2] {
                                        n += 1;
                                    }
                                    format!("{}{}{}", &c[1], rep, &c[3])
                                })
                                .to_string();
                        }
                    }
                    text.push_str(&cur);
                }
            }
        }
        ev.text = text;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::{Entry as AEntry, Event};

    #[test]
    fn parse_check_apply() {
        let g = parse("# names\nsensei => Sensei\nOnii chan=Onii-chan\n\nbad =>\n");
        assert_eq!(g.len(), 3);
        let mut d = Document::new_default();
        for t in ["Hello sensei, hi", "{\\i1}Onii chan{\\i0}! sensei.", "nothing", "Senseis stay"] {
            d.entries.push(AEntry::Event(Event { text: t.into(), ..Event::default() }));
        }
        let hits = check(&d, &g);
        assert_eq!(hits.iter().map(|h| h.event).collect::<Vec<_>>(), vec![1, 2, 2]);
        // "Senseis" must not match: whole words only
        assert!(!hits.iter().any(|h| h.event == 4));
        let n = apply(&mut d, &g);
        assert_eq!(n, 3);
        let t: Vec<_> = d.events().map(|e| e.text.clone()).collect();
        assert_eq!(t[0], "Hello Sensei, hi");
        assert_eq!(t[1], "{\\i1}Onii-chan{\\i0}! Sensei.");
    }
}
