//! Subtitle-Edit style text helpers: balanced line breaking, casing, hearing-impaired removal, TPP.

use crate::ass::{Kind, Script};
use crate::tags::{self, Seg};
use regex::Regex;

/// Break plain text into at most `max_lines` rows of ~equal length, preferring spaces; returns text with `\N`.
pub fn balance(text: &str, max_chars: usize) -> String {
    let plain: Vec<&str> = text.split_whitespace().collect();
    let total: usize = plain.iter().map(|w| w.chars().count()).sum::<usize>() + plain.len().saturating_sub(1);
    if total <= max_chars || plain.len() < 2 {
        return plain.join(" ");
    }
    // Try two lines: choose the split minimising the length difference.
    let mut best = (usize::MAX, 1usize);
    let mut left = 0usize;
    for i in 1..plain.len() {
        left += plain[i - 1].chars().count() + if i > 1 { 1 } else { 0 };
        let right = total - left - 1;
        let diff = left.abs_diff(right);
        let over = left.saturating_sub(max_chars) + right.saturating_sub(max_chars);
        let score = diff + over * 10;
        if score < best.0 {
            best = (score, i);
        }
    }
    format!("{}\\N{}", plain[..best.1].join(" "), plain[best.1..].join(" "))
}

/// Re-balance every dialogue line without override tags (tagged lines are left alone to avoid breaking effects).
pub fn balance_script(script: &mut Script, max_chars: usize) -> usize {
    let mut n = 0;
    for l in script.lines_mut() {
        if l.kind != Kind::Dialogue || l.text.contains('{') {
            continue;
        }
        let flat = l.text.replace("\\N", " ").replace("\\n", " ");
        let b = balance(&flat, max_chars);
        if b != l.text {
            l.text = b;
            n += 1;
        }
    }
    n
}

#[derive(Debug, Clone, Copy)]
pub enum Casing {
    Upper,
    Lower,
    Sentence,
    Title,
}

fn turkish_upper(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'i' => vec!['İ'], 'ı' => vec!['I'], c => c.to_uppercase().collect() }).collect()
}
fn turkish_lower(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'I' => vec!['ı'], 'İ' => vec!['i'], c => c.to_lowercase().collect() }).collect()
}

/// Change casing of visible text only (tags untouched). `turkish` enables İ/ı handling.
pub fn change_case(text: &str, mode: Casing, turkish: bool) -> String {
    let up = |s: &str| if turkish { turkish_upper(s) } else { s.to_uppercase() };
    let lo = |s: &str| if turkish { turkish_lower(s) } else { s.to_lowercase() };
    let mut new_sentence = true;
    let segs: Vec<Seg> = tags::parse(text)
        .into_iter()
        .map(|s| match s {
            Seg::Text(t) => {
                let mut out = String::new();
                let mut word_start = true;
                let mut it = t.chars().peekable();
                while let Some(c) = it.next() {
                    if c == '\\' && matches!(it.peek(), Some('N') | Some('n') | Some('h')) {
                        out.push(c);
                        out.push(it.next().unwrap());
                        word_start = true;
                        continue;
                    }
                    let piece = c.to_string();
                    let r = match mode {
                        Casing::Upper => up(&piece),
                        Casing::Lower => lo(&piece),
                        Casing::Sentence => {
                            if c.is_alphabetic() && new_sentence {
                                new_sentence = false;
                                up(&piece)
                            } else {
                                if matches!(c, '.' | '!' | '?' | '…') {
                                    new_sentence = true;
                                }
                                lo(&piece)
                            }
                        }
                        Casing::Title => {
                            if c.is_alphabetic() && word_start {
                                up(&piece)
                            } else {
                                lo(&piece)
                            }
                        }
                    };
                    word_start = !c.is_alphanumeric() && c != '\'' && c != '’';
                    out.push_str(&r);
                }
                Seg::Text(out)
            }
            b => b,
        })
        .collect();
    tags::render(&segs)
}

/// Remove hearing-impaired annotations: `[noise]`, `(laughs)`, `♪ lyrics ♪`, `SPEAKER:` prefixes. Lines left empty are removed.
pub fn remove_hearing_impaired(script: &mut Script) -> usize {
    let brackets = Regex::new(r"\[[^\]]*\]|\([^)]*\)|（[^）]*）").unwrap();
    let music = Regex::new(r"[♪♫][^♪♫]*[♪♫]?").unwrap();
    let speaker = Regex::new(r"^\s*[A-ZÇĞİÖŞÜ][A-ZÇĞİÖŞÜ0-9 ]{1,20}:\s*").unwrap();
    let ws = Regex::new(r" {2,}").unwrap();
    let mut n = 0;
    for l in script.lines_mut() {
        if l.kind != Kind::Dialogue || tags::has_drawing(&l.text) {
            continue;
        }
        let segs: Vec<Seg> = tags::parse(&l.text)
            .into_iter()
            .map(|s| match s {
                Seg::Text(t) => {
                    let t = brackets.replace_all(&t, "");
                    let t = music.replace_all(&t, "");
                    let t = speaker.replace(&t, "");
                    Seg::Text(ws.replace_all(&t, " ").trim_start_matches(' ').to_string())
                }
                b => b,
            })
            .collect();
        let new = tags::render(&segs).replace("\\N\\N", "\\N");
        let new = new.trim().trim_end_matches("\\N").trim_start_matches("\\N").trim().to_string();
        if new != l.text {
            l.text = new;
            n += 1;
        }
    }
    n
}

/// Timing post-processor: lead-in/lead-out and joining of near lines. Times in ms.
#[derive(Debug, Clone, Copy)]
pub struct Tpp {
    pub lead_in: i64,
    pub lead_out: i64,
    /// Join consecutive same-style lines whose gap is below this (bridges to the midpoint).
    pub join_gap: i64,
}

pub fn timing_post_process(script: &mut Script, o: Tpp) -> usize {
    let mut lines: Vec<&mut crate::ass::Line> = script.lines_mut().filter(|l| l.kind == Kind::Dialogue && !tags::has_drawing(&l.text)).collect();
    lines.sort_by_key(|l| l.start);
    let mut n = 0;
    for l in lines.iter_mut() {
        let (s, e) = (l.start, l.end);
        l.start = (l.start - o.lead_in).max(0);
        l.end += o.lead_out;
        if (l.start, l.end) != (s, e) {
            n += 1;
        }
    }
    for i in 0..lines.len().saturating_sub(1) {
        let (a_end, b_start) = (lines[i].end, lines[i + 1].start);
        if lines[i].style == lines[i + 1].style {
            if b_start < a_end {
                // lead-in/out created an overlap: meet in the middle of the original gap.
                let mid = (a_end + b_start) / 2;
                lines[i].end = mid;
                lines[i + 1].start = mid;
                n += 1;
            } else if b_start - a_end <= o.join_gap && b_start != a_end {
                let mid = (a_end + b_start) / 2;
                lines[i].end = mid;
                lines[i + 1].start = mid;
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;

    #[test]
    fn balance_two_lines() {
        assert_eq!(balance("short", 42), "short");
        let b = balance("This is a rather long subtitle that needs breaking into two rows", 42);
        let parts: Vec<_> = b.split("\\N").collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].len() <= 42 && parts[1].len() <= 42);
        assert!((parts[0].len() as i32 - parts[1].len() as i32).abs() < 12);
    }

    #[test]
    fn casing_turkish() {
        assert_eq!(change_case("{\\i1}iyi ışık", Casing::Upper, true), "{\\i1}İYİ IŞIK");
        assert_eq!(change_case("İYİ IŞIK", Casing::Lower, true), "iyi ışık");
        assert_eq!(change_case("MERHABA. NASILSIN? iyiyim", Casing::Sentence, true), "Merhaba. Nasılsın? İyiyim");
        assert_eq!(change_case("hello wORLD", Casing::Title, false), "Hello World");
    }

    #[test]
    fn hi_removal() {
        let mut s = Script::new_default();
        s.push_line(Line::new(0, 1, "[door slams] Hello"));
        s.push_line(Line::new(0, 1, "MARK: Hi there (laughs)"));
        s.push_line(Line::new(0, 1, "♪ la la la ♪"));
        assert_eq!(remove_hearing_impaired(&mut s), 3);
        let v: Vec<_> = s.lines().map(|l| l.text.clone()).collect();
        assert_eq!(v, ["Hello", "Hi there", ""]);
    }

    #[test]
    fn tpp() {
        let mut s = Script::new_default();
        s.push_line(Line::new(1000, 2000, "a"));
        s.push_line(Line::new(2100, 3000, "b"));
        timing_post_process(&mut s, Tpp { lead_in: 100, lead_out: 100, join_gap: 300 });
        let v: Vec<_> = s.lines().map(|l| (l.start, l.end)).collect();
        assert_eq!(v[0].0, 900);
        assert_eq!(v[0].1, v[1].0);
    }
}
