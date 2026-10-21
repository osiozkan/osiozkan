//! Karaoke syllable parsing, matching Aegisub's `AssKaraoke` (used by `aegisub.parse_karaoke_data`).

use crate::tags::{parse, Segment};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Syllable {
    /// Absolute start time in ms (line start + preceding syllable durations).
    pub start_time: i64,
    pub duration: i64,
    /// `\k`, `\kf` or `\ko` (with the backslash, as Aegisub stores it).
    pub tag_type: String,
    /// Plain text of the syllable (override blocks removed).
    pub text: String,
    /// Override blocks keyed by byte offset into `text`.
    pub ovr_tags: BTreeMap<usize, String>,
}

impl Syllable {
    /// Text including inline override blocks; `k_tag` prepends this syllable's own `{\kNN}`.
    pub fn get_text(&self, k_tag: bool) -> String {
        let mut ret = String::new();
        if k_tag {
            ret = format!("{{{}{}}}", self.tag_type, (self.duration + 5) / 10);
        }
        let mut idx = 0;
        for (&pos, ovr) in &self.ovr_tags {
            ret.push_str(&self.text[idx..pos]);
            ret.push_str(ovr);
            idx = pos;
        }
        ret.push_str(&self.text[idx..]);
        ret
    }
}

pub fn parse_syllables(text: &str, line_start: i64) -> Vec<Syllable> {
    let mut syls = vec![];
    let mut syl = Syllable { start_time: line_start, duration: 0, tag_type: "\\k".into(), text: String::new(), ovr_tags: BTreeMap::new() };
    let mut drawing = false;
    for seg in parse(text) {
        match seg {
            Segment::Text(t) => {
                if drawing {
                    syl.ovr_tags.entry(syl.text.len()).or_default().push_str(&t);
                } else {
                    syl.text.push_str(&t);
                }
            }
            Segment::Block(tags) => {
                // A block holding only comment text is kept verbatim.
                if tags.iter().all(|t| t.name.is_empty()) {
                    let raw: String = tags.iter().map(|t| t.value.as_str()).collect();
                    syl.ovr_tags.entry(syl.text.len()).or_default().push_str(&format!("{{{raw}}}"));
                    continue;
                }
                let mut in_tag = false;
                for tag in tags {
                    let is_k = matches!(tag.name.as_str(), "k" | "K" | "kf" | "ko");
                    if is_k {
                        if in_tag {
                            syl.ovr_tags.entry(syl.text.len()).or_default().push('}');
                            in_tag = false;
                        }
                        let name = if tag.name == "K" { "kf".to_string() } else { tag.name.clone() };
                        if syl.duration > 0 || !syl.text.is_empty() {
                            syls.push(syl.clone());
                            syl.text.clear();
                            syl.ovr_tags.clear();
                        }
                        syl.tag_type = format!("\\{name}");
                        syl.start_time += syl.duration;
                        let d: f64 = tag.value.trim().parse().unwrap_or(0.0);
                        syl.duration = (d * 10.0) as i64;
                    } else {
                        if tag.name == "p" {
                            drawing = tag.value.trim().parse::<f64>().unwrap_or(0.0) > 0.0;
                        }
                        let o = syl.ovr_tags.entry(syl.text.len()).or_default();
                        if !in_tag {
                            o.push('{');
                        }
                        in_tag = true;
                        o.push_str(&tag.to_ass());
                    }
                }
                if in_tag {
                    syl.ovr_tags.entry(syl.text.len()).or_default().push('}');
                }
            }
        }
    }
    syls.push(syl);
    syls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_syllables() {
        let s = parse_syllables("{\\k20}ta{\\k30\\fs50}be{\\k50}ru", 1000);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].start_time, s[0].duration, s[0].text.as_str()), (1000, 200, "ta"));
        assert_eq!((s[1].start_time, s[1].duration), (1200, 300));
        assert_eq!(s[1].get_text(true), "{\\k30}{\\fs50}be");
        assert_eq!(s[2].start_time, 1500);
        assert_eq!(s[2].get_text(true), "{\\k50}ru");
    }

    #[test]
    fn leading_text_forms_first_syllable() {
        let s = parse_syllables("a{\\kf10}b", 0);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].text, "a");
        assert_eq!(s[1].tag_type, "\\kf");
    }
}

// ---------------------------------------------------------------------------------------------
// Karaoke editing helpers

/// Strip karaoke tags, keeping other override tags and text.
pub fn strip_kara(text: &str) -> String {
    let syls = parse_syllables(text, 0);
    syls.iter().map(|s| {
        let mut c = s.clone();
        c.duration = 0;
        c.get_text(false)
    }).collect()
}

/// Rebuild a line from syllable texts with `\k` durations (cs) given per syllable.
fn build(parts: &[(String, i64)], tag: &str) -> String {
    parts.iter().map(|(t, d)| format!("{{{tag}{d}}}{t}")).collect()
}

/// Auto-split a line into syllables (per word, per character, or per `chars` characters) and share
/// the event duration evenly. `tag` is `\k`, `\kf` or `\ko`.
pub fn auto_split(text: &str, duration_ms: i64, per_char: bool, tag: &str) -> String {
    let plain = strip_kara(text);
    let vis = crate::tags::strip_tags(&plain);
    let units: Vec<String> = if per_char {
        vis.chars().map(|c| c.to_string()).collect()
    } else {
        let mut v: Vec<String> = vec![];
        let mut cur = String::new();
        for c in vis.chars() {
            cur.push(c);
            if c == ' ' {
                v.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            v.push(cur);
        }
        v
    };
    if units.is_empty() {
        return text.to_string();
    }
    let total_cs = (duration_ms / 10).max(units.len() as i64);
    let base = total_cs / units.len() as i64;
    let mut rem = total_cs - base * units.len() as i64;
    let parts: Vec<(String, i64)> = units
        .into_iter()
        .map(|u| {
            let extra = if rem > 0 { rem -= 1; 1 } else { 0 };
            (u, base + extra)
        })
        .collect();
    build(&parts, tag)
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    #[test]
    fn strips_and_splits() {
        assert_eq!(strip_kara("{\\k20}ta{\\k30\\fs50}be"), "ta{\\fs50}be");
        let t = auto_split("ta be ru", 3000, false, "\\k");
        assert_eq!(t, "{\\k100}ta {\\k100}be {\\k100}ru");
        let c = auto_split("abc", 1000, true, "\\kf");
        assert_eq!(c, "{\\kf34}a{\\kf33}b{\\kf33}c");
        assert_eq!(parse_syllables(&c, 0).iter().map(|s| s.duration).sum::<i64>(), 1000);
    }
}
