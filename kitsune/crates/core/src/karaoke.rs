//! Karaoke (\k, \kf, \ko) hece ayrıştırma ve üretme.
use crate::tags::{parse_block, split, Part};

#[derive(Debug, Clone, PartialEq)]
pub struct Syllable {
    /// Santisaniye
    pub duration_cs: i64,
    pub kind: String,
    pub text: String,
}

/// "{\k20}Ka{\k30}ra" -> heceler. Etiket bloğundaki diğer etiketler yoksayılır.
pub fn parse(text: &str) -> Vec<Syllable> {
    let mut out: Vec<Syllable> = vec![];
    let mut cur: Option<Syllable> = None;
    for p in split(text) {
        match p {
            Part::Tags(b) => {
                for (n, a) in parse_block(b) {
                    if matches!(n.as_str(), "k" | "K" | "kf" | "ko") {
                        if let Some(s) = cur.take() { out.push(s); }
                        let n = if n == "K" { "kf".to_string() } else { n };
                        cur = Some(Syllable { duration_cs: a.parse().unwrap_or(0), kind: n, text: String::new() });
                    }
                }
            }
            Part::Text(t) => {
                if let Some(s) = cur.as_mut() { s.text.push_str(t); }
                else if !t.is_empty() { cur = Some(Syllable { duration_cs: 0, kind: "k".into(), text: t.into() }); }
            }
        }
    }
    if let Some(s) = cur { out.push(s); }
    out
}

pub fn build(sylls: &[Syllable]) -> String {
    sylls.iter().map(|s| format!("{{\\{}{}}}{}", s.kind, s.duration_cs, s.text)).collect()
}

/// Metni her karakter için eşit uzunlukta \k ile böler (toplam süre ms).
pub fn split_even(text: &str, total_ms: i64) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() { return String::new(); }
    let total_cs = total_ms / 10;
    let each = total_cs / chars.len() as i64;
    let mut rem = total_cs - each * chars.len() as i64;
    let mut sylls = vec![];
    for c in chars {
        let extra = if rem > 0 { rem -= 1; 1 } else { 0 };
        sylls.push(Syllable { duration_cs: each + extra, kind: "k".into(), text: c.to_string() });
    }
    build(&sylls)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn karaoke() {
        let s = parse("{\\k20}Ka{\\kf30}ra");
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].kind, "kf");
        assert_eq!(build(&s), "{\\k20}Ka{\\kf30}ra");
        let e = split_even("abc", 1000);
        assert_eq!(parse(&e).iter().map(|s| s.duration_cs).sum::<i64>(), 100);
    }
}
