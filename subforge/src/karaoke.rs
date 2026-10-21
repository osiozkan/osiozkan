//! Karaoke authoring helpers: split text into syllables, assign \k timing, equalize/scale/shift.

use crate::ass::Document;
use crate::tags::{karaoke_data, split_blocks, Seg};
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitMode {
    Char,
    Word,
    /// romaji / Turkish / English-ish: consonant clusters + vowel groups
    Syllable,
}

/// Remove karaoke tags (keeps the other override tags).
pub fn strip_k(text: &str) -> String {
    let re = Regex::new(r"\\(?:kf|ko|k|K)-?\d+(?:\.\d+)?").unwrap();
    let mut out = String::new();
    for seg in split_blocks(text) {
        match seg {
            Seg::Tags(t) => {
                let r = re.replace_all(t, "");
                if !r.is_empty() {
                    out.push('{');
                    out.push_str(&r);
                    out.push('}');
                }
            }
            Seg::Text(t) => out.push_str(t),
        }
    }
    out
}

pub fn split_syllables(s: &str, mode: SplitMode) -> Vec<String> {
    match mode {
        SplitMode::Char => s.chars().map(|c| c.to_string()).collect(),
        SplitMode::Word => {
            let re = Regex::new(r"\S+\s*|\s+").unwrap();
            re.find_iter(s).map(|m| m.as_str().to_string()).collect()
        }
        SplitMode::Syllable => syllabify(s),
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c.to_lowercase().next().unwrap_or(c), 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'â' | 'î' | 'û' | 'ö' | 'ü' | 'ı' | 'à' | 'é' | 'è' | 'ê' | 'ë' | 'í' | 'ï' | 'ó' | 'ô' | 'ú' | 'ù' | 'ā' | 'ī' | 'ū' | 'ē' | 'ō')
}

/// Romaji/Turkish-style syllabification: [consonants][vowels][moraic n]; spaces and punctuation stick
/// to the preceding syllable so the text can be re-joined losslessly.
fn syllabify(s: &str) -> Vec<String> {
    let ch: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if !c.is_alphabetic() {
            // spaces/punctuation attach to the previous syllable
            match out.last_mut() {
                Some(last) => last.push(c),
                None => out.push(c.to_string()),
            }
            i += 1;
            continue;
        }
        let start = i;
        // y after a consonant is a glide (kyo), otherwise treated as vowel when leading
        while i < ch.len() && ch[i].is_alphabetic() && !is_vowel(ch[i]) && i - start < 3 {
            i += 1;
        }
        // consonant 'y' glides: treat as consonant if followed by a vowel
        if i < ch.len() && is_vowel(ch[i]) {
            while i < ch.len() && is_vowel(ch[i]) {
                i += 1;
            }
            // moraic n: n not followed by a vowel (nor by y)
            if i < ch.len() && (ch[i] == 'n' || ch[i] == 'N') {
                let nxt = ch.get(i + 1).copied();
                let followed_by_vowel = nxt.map(|x| is_vowel(x)).unwrap_or(false);
                if !followed_by_vowel {
                    i += 1;
                    if ch.get(i) == Some(&'\'') {
                        i += 1;
                    }
                }
            }
        } else if i == start {
            i += 1; // lone non-vowel char safety
        }
        out.push(ch[start..i].iter().collect());
    }
    out
}

/// Distribute `total_cs` across `n` syllables as evenly as possible (remainder to the first ones).
pub fn even_split(total_cs: i64, n: usize) -> Vec<i64> {
    if n == 0 {
        return vec![];
    }
    let base = total_cs / n as i64;
    let rem = (total_cs % n as i64) as usize;
    (0..n).map(|i| base + if i < rem { 1 } else { 0 }).collect()
}

/// Build `{\kN}syl` text. Leading override blocks of the original are preserved in front.
pub fn build(prefix: &str, syls: &[String], durs: &[i64], kind: &str) -> String {
    let mut out = String::from(prefix);
    for (s, d) in syls.iter().zip(durs) {
        out.push_str(&format!("{{\\{}{}}}{}", kind, d, s));
    }
    out
}

fn split_prefix(text: &str) -> (String, String) {
    // leading override blocks without karaoke tags are kept as prefix
    let mut pre = String::new();
    let mut rest = text;
    while rest.starts_with('{') {
        let Some(i) = rest.find('}') else { break };
        pre.push_str(&rest[..=i]);
        rest = &rest[i + 1..];
    }
    (pre, rest.to_string())
}

/// Replace karaoke timing of the selected (or all) lines by splitting their plain text.
/// Existing k tags are discarded. Line duration is split evenly. Returns lines changed.
pub fn auto_split(doc: &mut Document, mode: SplitMode, kind: &str, only: Option<&[usize]>) -> usize {
    let mut n = 0;
    for (i, e) in doc.events_mut().enumerate() {
        if e.comment || only.map(|o| !o.contains(&i)).unwrap_or(false) {
            continue;
        }
        let base = strip_k(&e.text);
        if base.contains("\\p1") {
            continue;
        }
        let (pre, rest) = split_prefix(&base);
        if rest.contains('{') {
            continue; // inline tags: leave manual
        }
        let syls: Vec<String> = split_syllables(&rest, mode).into_iter().filter(|s| !s.is_empty()).collect();
        if syls.is_empty() {
            continue;
        }
        let total = ((e.end - e.start) / 10).max(syls.len() as i64);
        e.text = build(&pre, &syls, &even_split(total, syls.len()), kind);
        n += 1;
    }
    n
}

/// Rescale all \k durations of each line so they sum to the line duration.
pub fn scale_to_line(doc: &mut Document, only: Option<&[usize]>) -> usize {
    let kre = Regex::new(r"\\(kf|ko|k|K)(-?\d+(?:\.\d+)?)").unwrap();
    let mut n = 0;
    for (i, e) in doc.events_mut().enumerate() {
        if e.comment || only.map(|o| !o.contains(&i)).unwrap_or(false) {
            continue;
        }
        let data = karaoke_data(&e.text);
        let sum: i64 = data.iter().map(|s| s.dur_cs).sum();
        let target = (e.end - e.start) / 10;
        if sum <= 0 || target <= 0 || sum == target {
            continue;
        }
        let k = target as f64 / sum as f64;
        let mut acc_src = 0.0;
        let mut acc_dst = 0i64;
        let mut new_text = String::new();
        for seg in split_blocks(&e.text) {
            match seg {
                Seg::Text(t) => new_text.push_str(t),
                Seg::Tags(t) => {
                    let r = kre.replace(t, |c: &regex::Captures| {
                        let d: f64 = c[2].parse().unwrap_or(0.0);
                        acc_src += d;
                        let end = (acc_src * k).round() as i64;
                        let nd = end - acc_dst;
                        acc_dst = end;
                        format!("\\{}{}", &c[1], nd)
                    });
                    new_text.push('{');
                    new_text.push_str(&r);
                    new_text.push('}');
                }
            }
        }
        e.text = new_text;
        n += 1;
    }
    n
}

/// Add `delta_cs` to the first syllable's duration (positive = lead-in delay, negative = shorter).
pub fn shift_first(doc: &mut Document, delta_cs: i64, only: Option<&[usize]>) -> usize {
    let kre = Regex::new(r"\\(kf|ko|k|K)(-?\d+)").unwrap();
    let mut n = 0;
    for (i, e) in doc.events_mut().enumerate() {
        if e.comment || only.map(|o| !o.contains(&i)).unwrap_or(false) {
            continue;
        }
        if let Some(m) = kre.captures(&e.text) {
            let d: i64 = m[2].parse().unwrap_or(0);
            let new = format!("\\{}{}", &m[1], (d + delta_cs).max(0));
            let r = m.get(0).unwrap().range();
            e.text.replace_range(r, &new);
            n += 1;
        }
    }
    n
}

pub fn remove_all(doc: &mut Document, only: Option<&[usize]>) -> usize {
    let mut n = 0;
    for (i, e) in doc.events_mut().enumerate() {
        if only.map(|o| !o.contains(&i)).unwrap_or(false) {
            continue;
        }
        let t = strip_k(&e.text);
        if t != e.text {
            e.text = t;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::{Entry, Event};

    fn one(text: &str, ms: i64) -> Document {
        let mut d = Document::new_default();
        d.entries.push(Entry::Event(Event { text: text.into(), start: 0, end: ms, ..Event::default() }));
        d
    }
    fn txt(d: &Document) -> String {
        d.events().next().unwrap().text.clone()
    }

    #[test]
    fn romaji_syllables() {
        let s = split_syllables("sakura no hana", SplitMode::Syllable);
        assert_eq!(s, ["sa", "ku", "ra ", "no ", "ha", "na"]);
        let s = split_syllables("shinkansen tsuki", SplitMode::Syllable);
        assert_eq!(s, ["shin", "kan", "sen ", "tsu", "ki"]);
        assert_eq!(split_syllables("a b", SplitMode::Word), ["a ", "b"]);
        assert_eq!(split_syllables("ab", SplitMode::Char), ["a", "b"]);
    }

    #[test]
    fn split_scale_shift() {
        let mut d = one("{\\an8}ka ra", 1000);
        assert_eq!(auto_split(&mut d, SplitMode::Syllable, "kf", None), 1);
        assert_eq!(txt(&d), "{\\an8}{\\kf50}ka {\\kf50}ra");
        let mut d = one("{\\k10}A{\\k30}B", 2000);
        assert_eq!(scale_to_line(&mut d, None), 1);
        assert_eq!(txt(&d), "{\\k50}A{\\k150}B");
        assert_eq!(shift_first(&mut d, -20, None), 1);
        assert_eq!(txt(&d), "{\\k30}A{\\k150}B");
        assert_eq!(remove_all(&mut d, None), 1);
        assert_eq!(txt(&d), "AB");
        assert_eq!(even_split(10, 3), vec![4, 3, 3]);
    }
}
