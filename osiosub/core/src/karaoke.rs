//! Karaoke helpers: automatic `\k` generation and removal.
use crate::ass::AssFile;
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split { Word, Char, Romaji }

/// Split plain text into syllables according to `mode`. Spaces stay attached to the previous syllable.
pub fn split_syllables(text: &str, mode: Split) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    match mode {
        Split::Char => {
            for c in chars { if c == ' ' { if let Some(l) = out.last_mut() { l.push(c); continue; } } out.push(c.to_string()); }
        }
        Split::Word => {
            let mut cur = String::new();
            for c in chars {
                if c == ' ' { cur.push(c); } else if cur.ends_with(' ') { out.push(std::mem::take(&mut cur)); cur.push(c); } else { cur.push(c); }
            }
            if !cur.is_empty() { out.push(cur); }
        }
        Split::Romaji => {
            let is_v = |c: char| "aeiouAEIOUāīūēōâîûêô".contains(c);
            let mut cur = String::new();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == ' ' { if cur.is_empty() { if let Some(l) = out.last_mut() { l.push(' '); } } else { cur.push(' '); out.push(std::mem::take(&mut cur)); } i += 1; continue; }
                cur.push(c);
                let next = chars.get(i + 1).copied();
                // syllable ends after a vowel, or on lone 'n' not followed by a vowel/y, or after doubled consonant start
                let ends = if is_v(c) { true } else if (c == 'n' || c == 'N') && next.map_or(true, |n| !is_v(n) && n != 'y' && n != 'Y') { true } else { false };
                let ended = ends;
                if ended && !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
                i += 1;
            }
            if !cur.is_empty() { out.push(cur); }
        }
    }
    out
}

/// Distribute `total_cs` centiseconds across syllables proportionally to their length (min 1).
pub fn auto_k(text: &str, total_cs: i64, mode: Split) -> String {
    let plain = crate::tags::strip_tags(text);
    let syl = split_syllables(&plain, mode);
    if syl.is_empty() { return text.to_string(); }
    let weights: Vec<f64> = syl.iter().map(|s| s.trim().chars().count().max(1) as f64).collect();
    let sum: f64 = weights.iter().sum();
    let mut left = total_cs;
    let mut out = String::new();
    for (i, s) in syl.iter().enumerate() {
        let d = if i + 1 == syl.len() { left.max(1) } else { ((total_cs as f64 * weights[i] / sum).round() as i64).max(1) };
        left -= d;
        out.push_str(&format!("{{\\k{}}}{}", d, s));
    }
    out
}

pub fn strip_k(text: &str) -> String {
    let re = Regex::new(r"\\[kK][fo]?\d+").unwrap();
    let out = re.replace_all(text, "").into_owned();
    out.replace("{}", "")
}

pub fn auto_k_all(f: &mut AssFile, idx: &[usize], mode: Split) -> usize {
    let mut n = 0;
    for &i in idx {
        if let Some(e) = f.events.get_mut(i) {
            let dur_cs = (e.end - e.start) / 10;
            e.text = auto_k(&e.text, dur_cs, mode); n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn syllables() {
        assert_eq!(split_syllables("sakura ga", Split::Romaji), vec!["sa", "ku", "ra ", "ga"]);
        assert_eq!(split_syllables("kanpai nano", Split::Romaji), vec!["ka", "n", "pa", "i ", "na", "no"]);
        assert_eq!(split_syllables("ab cd", Split::Word), vec!["ab ", "cd"]);
    }
    #[test]
    fn k_total() {
        let k = auto_k("sakura", 100, Split::Romaji);
        let sum: i64 = crate::tags::karaoke_syllables(&k).iter().map(|s| s.0).sum();
        assert_eq!(sum, 100);
        assert_eq!(strip_k(&k), "sakura");
    }
}
