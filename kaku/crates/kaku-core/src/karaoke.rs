//! Karaoke (\k) helpers: parse syllables, rebuild, auto-split, even timing.

use crate::tags::{parse, render, Seg, Tag};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq)]
pub struct Syl {
    /// Duration in centiseconds.
    pub dur_cs: i64,
    /// k, kf or ko
    pub kind: String,
    /// Other tags that share the block (kept verbatim).
    pub other: Vec<Tag>,
    pub text: String,
}

/// Parse text into syllables. Text before the first `\k` becomes a zero-duration syllable.
pub fn syllables(text: &str) -> Vec<Syl> {
    let mut out: Vec<Syl> = Vec::new();
    let mut cur: Option<Syl> = None;
    for seg in parse(text) {
        match seg {
            Seg::Text(t) => match cur.as_mut() {
                Some(c) => c.text.push_str(&t),
                None => cur = Some(Syl { dur_cs: 0, kind: "k".into(), other: vec![], text: t }),
            },
            Seg::Block(tags) => {
                let mut kt: Option<(String, i64)> = None;
                let mut rest = Vec::new();
                for t in tags {
                    if matches!(t.name.as_str(), "k" | "K" | "kf" | "ko") {
                        let n = if t.name == "K" { "kf" } else { t.name.as_str() };
                        kt = Some((n.to_string(), t.arg.trim().parse::<f64>().unwrap_or(0.0).round() as i64));
                    } else {
                        rest.push(t);
                    }
                }
                if let Some((kind, dur)) = kt {
                    if let Some(c) = cur.take() {
                        out.push(c);
                    }
                    cur = Some(Syl { dur_cs: dur, kind, other: rest, text: String::new() });
                } else if !rest.is_empty() {
                    // Non-karaoke block mid-syllable: embed into the syllable text.
                    let s = render(&[Seg::Block(rest)]);
                    match cur.as_mut() {
                        Some(c) => c.text.push_str(&s),
                        None => cur = Some(Syl { dur_cs: 0, kind: "k".into(), other: vec![], text: s }),
                    }
                }
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

pub fn build(sylls: &[Syl]) -> String {
    let mut s = String::new();
    for y in sylls {
        s.push('{');
        s.push_str(&format!("\\{}{}", y.kind, y.dur_cs));
        for t in &y.other {
            s.push_str(&t.render());
        }
        s.push('}');
        s.push_str(&y.text);
    }
    s
}

pub enum Split {
    Grapheme,
    Word,
}

/// Strip existing k-tags and split the text into syllables evenly filling `total_cs`.
pub fn auto_split(text: &str, total_cs: i64, mode: Split) -> String {
    let plain: String = syllables(text).iter().map(|s| s.text.as_str()).collect();
    let units: Vec<String> = match mode {
        Split::Grapheme => plain.graphemes(true).map(|g| g.to_string()).collect(),
        Split::Word => plain.split_word_bounds().map(|g| g.to_string()).collect::<Vec<_>>(),
    };
    // Merge whitespace-only units into the previous unit so spaces do not take time alone.
    let mut merged: Vec<String> = Vec::new();
    for u in units {
        if u.trim().is_empty() {
            if let Some(last) = merged.last_mut() {
                last.push_str(&u);
                continue;
            }
        }
        merged.push(u);
    }
    if merged.is_empty() {
        return plain;
    }
    let weights: Vec<usize> = merged.iter().map(|u| u.trim().chars().count().max(1)).collect();
    let tw: usize = weights.iter().sum();
    let mut sylls = Vec::new();
    let mut used = 0i64;
    let mut acc = 0usize;
    for (u, w) in merged.into_iter().zip(weights) {
        acc += w;
        let target = total_cs * acc as i64 / tw as i64;
        sylls.push(Syl { dur_cs: target - used, kind: "k".into(), other: vec![], text: u });
        used = target;
    }
    build(&sylls)
}

pub fn total_cs(text: &str) -> i64 {
    syllables(text).iter().map(|s| s.dur_cs).sum()
}

/// Remove every karaoke tag, keeping text and other tags.
pub fn strip_k(text: &str) -> String {
    let segs: Vec<Seg> = parse(text)
        .into_iter()
        .filter_map(|s| match s {
            Seg::Block(tags) => {
                let t: Vec<Tag> = tags.into_iter().filter(|t| !matches!(t.name.as_str(), "k" | "K" | "kf" | "ko")).collect();
                if t.is_empty() { None } else { Some(Seg::Block(t)) }
            }
            x => Some(x),
        })
        .collect();
    render(&segs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_build() {
        let t = "{\\k20}ka{\\kf30\\c&HFF&}ra{\\k50}oke";
        let s = syllables(t);
        assert_eq!(s.len(), 3);
        assert_eq!(s[1].dur_cs, 30);
        assert_eq!(s[1].other.len(), 1);
        assert_eq!(build(&s), t);
        assert_eq!(total_cs(t), 100);
    }
    #[test]
    fn split_fills() {
        let r = auto_split("Hello world", 100, Split::Word);
        assert_eq!(total_cs(&r), 100);
        assert_eq!(strip_k(&r), "Hello world");
        let g = auto_split("abcd", 40, Split::Grapheme);
        assert_eq!(g, "{\\k10}a{\\k10}b{\\k10}c{\\k10}d");
    }
}
