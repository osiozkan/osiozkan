//! Text and tag tools (Subtitle Edit–style cleanup, case conversion, line balancing, karaoke helpers).
use crate::ass::{Event, Script};
use crate::tags::{self, Item, Segment};
use crate::turkish;

/// Apply `f` to every plain-text run, leaving `{...}` override blocks untouched.
pub fn map_visible(text: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    for seg in tags::parse(text) {
        match seg {
            Segment::Text(t) => out.push_str(&f(&t)),
            seg @ Segment::Block(_) => out.push_str(&tags::to_string(&[seg])),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Title,
    Sentence,
}

/// Turkish-aware case conversion that never touches tags or `\N`-style escapes.
pub fn change_case(text: &str, case: Case) -> String {
    // protect \N \n \h escapes by splitting on them
    let conv = |t: &str| -> String {
        let mut out = String::new();
        let mut rest = t;
        let mut at_sentence_start = true;
        while !rest.is_empty() {
            if let Some(esc) = ["\\N", "\\n", "\\h"].iter().find(|e| rest.starts_with(**e)) {
                out.push_str(esc);
                rest = &rest[esc.len()..];
                if *esc == "\\N" {
                    at_sentence_start = case == Case::Sentence && at_sentence_start;
                }
                continue;
            }
            let end = rest.find('\\').map(|i| if i == 0 { 1 } else { i }).unwrap_or(rest.len());
            let chunk = &rest[..end];
            match case {
                Case::Upper => out.push_str(&turkish::upper(chunk)),
                Case::Lower => out.push_str(&turkish::lower(chunk)),
                Case::Title => out.push_str(&turkish::title(chunk)),
                Case::Sentence => {
                    for c in chunk.chars() {
                        if c.is_alphabetic() {
                            let s = c.to_string();
                            out.push_str(&if at_sentence_start { turkish::upper(&s) } else { turkish::lower(&s) });
                            at_sentence_start = false;
                        } else {
                            out.push(c);
                            if matches!(c, '.' | '!' | '?' | '…') {
                                at_sentence_start = true;
                            }
                        }
                    }
                }
            }
            rest = &rest[end..];
        }
        out
    };
    map_visible(text, conv)
}

/// Replace manual breaks with spaces.
pub fn unbreak(text: &str) -> String {
    let t = text.replace("\\N", " ").replace("\\n", " ");
    let mut t = t;
    while t.contains("  ") {
        t = t.replace("  ", " ");
    }
    t.trim().to_string()
}

/// Insert a `\N` so no line is longer than `max_chars` when possible, balancing the halves.
/// Texts with existing `\N` are unbroken first. Tags are kept intact.
pub fn balance_lines(text: &str, max_chars: usize) -> String {
    let flat = unbreak(text);
    let vis_len = tags::visible_text(&flat).chars().count();
    if vis_len <= max_chars {
        return flat;
    }
    // Candidate split positions: spaces outside {} blocks (byte index in `flat`).
    let mut cands: Vec<(usize, usize)> = vec![]; // (byte idx of space, visible chars before)
    let mut in_tag = false;
    let mut seen = 0usize;
    for (i, c) in flat.char_indices() {
        match c {
            '{' => in_tag = true,
            '}' => in_tag = false,
            ' ' if !in_tag => cands.push((i, seen)),
            _ if !in_tag => {}
            _ => {}
        }
        if !in_tag && c != '}' && c != '{' {
            seen += 1;
        }
    }
    if cands.is_empty() {
        return flat;
    }
    // choose the split minimizing the longer line; prefer breaking after punctuation
    let best = cands
        .iter()
        .min_by(|a, b| {
            let score = |&&(idx, before): &&(usize, usize)| -> f64 {
                let left = before as f64;
                let right = (vis_len - before - 1) as f64;
                let mut s = left.max(right);
                let prev = flat[..idx].chars().last().unwrap_or(' ');
                if matches!(prev, ',' | '.' | '?' | '!' | ';' | ':' | '…') {
                    s -= 3.0;
                }
                s
            };
            score(a).partial_cmp(&score(b)).unwrap()
        })
        .copied()
        .unwrap();
    let mut out = String::with_capacity(flat.len() + 2);
    out.push_str(flat[..best.0].trim_end());
    out.push_str("\\N");
    out.push_str(flat[best.0 + 1..].trim_start());
    out
}

/// Remove hearing-impaired text: `[door slams]`, `(laughs)`, `♪ lyrics ♪`, `NAME:` prefixes.
pub fn strip_hearing_impaired(text: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<[regex::Regex; 4]> = OnceLock::new();
    let res = RE.get_or_init(|| {
        [
            regex::Regex::new(r"\[[^\]]*\]").unwrap(),
            regex::Regex::new(r"\([^)]*\)").unwrap(),
            regex::Regex::new(r"[♪♫][^♪♫]*[♪♫]?").unwrap(),
            regex::Regex::new(r"^\s*[A-ZÇĞİÖŞÜ][A-ZÇĞİÖŞÜ0-9 ]{1,20}:\s*").unwrap(),
        ]
    });
    map_visible(text, |t| {
        let mut s = t.to_string();
        for r in res.iter() {
            s = r.replace_all(&s, "").into_owned();
        }
        s
    })
}

#[derive(Debug, Clone)]
pub struct FixOptions {
    pub ellipsis: bool,
    pub space_before_punct: bool,
    pub space_after_punct: bool,
    pub double_spaces: bool,
    pub dash_dialogue: bool,
    pub trim_lines: bool,
    pub quotes: bool,
}

impl Default for FixOptions {
    fn default() -> Self {
        FixOptions { ellipsis: true, space_before_punct: true, space_after_punct: true, double_spaces: true, dash_dialogue: true, trim_lines: true, quotes: false }
    }
}

/// Typographic cleanup of visible text (Turkish/English conventions).
pub fn fix_common(text: &str, o: &FixOptions) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RES: OnceLock<Vec<Regex>> = OnceLock::new();
    let r = RES.get_or_init(|| {
        vec![
            Regex::new(r"\.{3,}").unwrap(),                          // 0 ellipsis
            Regex::new(r"[ \t]+([,.;:!?…])").unwrap(),               // 1 space before punct
            Regex::new(r"([,;:!?])([^\s\d\\{}\-,.;:!?\)\]'’\x22…])").unwrap(), // 2 missing space after
            Regex::new(r"[ \t]{2,}").unwrap(),                       // 3 double spaces
            Regex::new(r"(^|\\N)-([^\s-])").unwrap(),                // 4 dash dialogue "-Foo" → "- Foo"
            Regex::new(r"[ ]*\\N[ ]*").unwrap(),                     // 5 trim around breaks
            Regex::new(r"“|”|„").unwrap(),                           // 6 curly quotes
        ]
    });
    map_visible(text, |t| {
        let mut s = t.to_string();
        if o.ellipsis {
            s = r[0].replace_all(&s, "…").into_owned();
        }
        if o.space_before_punct {
            s = r[1].replace_all(&s, "$1").into_owned();
        }
        if o.space_after_punct {
            s = r[2].replace_all(&s, "$1 $2").into_owned();
        }
        if o.double_spaces {
            s = r[3].replace_all(&s, " ").into_owned();
        }
        if o.dash_dialogue {
            s = r[4].replace_all(&s, "$1- $2").into_owned();
        }
        if o.trim_lines {
            s = r[5].replace_all(&s, "\\N").into_owned();
        }
        if o.quotes {
            s = r[6].replace_all(&s, "\"").into_owned();
        }
        s
    })
    .trim()
    .to_string()
}

// ---------------------------------------------------------------- tag operations

/// Remove override tags by name (e.g. `["fscx","fscy"]`); empty blocks vanish.
pub fn remove_tags(text: &str, names: &[&str]) -> String {
    let mut segs = tags::parse(text);
    for seg in segs.iter_mut() {
        if let Segment::Block(items) = seg {
            items.retain(|it| !matches!(it, Item::Tag { name, .. } if names.contains(&name.as_str())));
        }
    }
    segs.retain(|s| !matches!(s, Segment::Block(items) if items.is_empty()));
    tags::to_string(&segs)
}

/// Strip every override block.
pub fn remove_all_tags(text: &str) -> String {
    tags::strip_tags(text)
}

/// Merge adjacent `{..}{..}` blocks and drop redundant duplicates of the same tag in one block.
pub fn merge_blocks(text: &str) -> String {
    let segs = tags::parse(text);
    let mut out: Vec<Segment> = vec![];
    for seg in segs {
        match (out.last_mut(), seg) {
            (Some(Segment::Block(prev)), Segment::Block(items)) => {
                // keep karaoke blocks separate
                let has_k = |v: &Vec<Item>| v.iter().any(|i| matches!(i, Item::Tag { name, .. } if matches!(name.as_str(), "k" | "K" | "kf" | "ko")));
                if has_k(prev) && has_k(&items) {
                    out.push(Segment::Block(items));
                } else {
                    prev.extend(items);
                }
            }
            (_, s) => out.push(s),
        }
    }
    tags::to_string(&out)
}

/// Set (or replace) an override tag in the line's first block, creating the block if needed.
/// `\pos` and `\move` replace each other. Example: `set_override(t, "pos", "(960,540)")`.
pub fn set_override(text: &str, name: &str, value: &str) -> String {
    let mut segs = tags::parse(text);
    let conflicts: &[&str] = match name {
        "pos" | "move" => &["pos", "move"],
        _ => &[],
    };
    let new_item = Item::Tag { name: name.to_string(), value: value.to_string() };
    match segs.first_mut() {
        Some(Segment::Block(items)) => {
            items.retain(|it| !matches!(it, Item::Tag { name: n, .. } if n == name || conflicts.contains(&n.as_str())));
            items.insert(0, new_item);
        }
        _ => segs.insert(0, Segment::Block(vec![new_item])),
    }
    tags::to_string(&segs)
}

/// Read the first occurrence of a tag's raw value (e.g. `pos` → `(10,20)`).
pub fn get_override(text: &str, name: &str) -> Option<String> {
    tags::parse(text).into_iter().find_map(|seg| match seg {
        Segment::Block(items) => items.into_iter().find_map(|it| match it {
            Item::Tag { name: n, value } if n == name => Some(value),
            _ => None,
        }),
        _ => None,
    })
}

// ---------------------------------------------------------------- karaoke helpers

/// Give every character of the (visible) text an equal `\k` slice of `total_cs`.
pub fn kara_split_chars(text: &str, total_cs: i64) -> String {
    let plain = tags::strip_tags(text);
    let chars: Vec<char> = plain.chars().collect();
    if chars.is_empty() {
        return text.to_string();
    }
    let per = (total_cs / chars.len() as i64).max(1);
    let mut out = String::new();
    let mut used = 0;
    for (i, c) in chars.iter().enumerate() {
        let d = if i + 1 == chars.len() { (total_cs - used).max(1) } else { per };
        used += d;
        out.push_str(&format!("{{\\k{d}}}{c}"));
    }
    out
}

/// Total `\k` duration (centiseconds) of a line.
pub fn kara_total_cs(text: &str) -> i64 {
    tags::syllables(text).iter().map(|s| s.duration_cs).sum()
}

/// Rebuild line text from syllables `(duration_cs, kind, other_tags, text)`.
pub fn kara_build(syls: &[(i64, String, String, String)]) -> String {
    let mut out = String::new();
    for (d, kind, other, text) in syls {
        out.push_str(&format!("{{\\{kind}{d}{other}}}{text}"));
    }
    out
}

/// Parse into editable syllables `(duration_cs, kind, other_tags, text)`.
pub fn kara_parse(text: &str) -> Vec<(i64, String, String, String)> {
    tags::syllables(text).into_iter().map(|s| (s.duration_cs, s.kind, s.other_tags, s.text)).collect()
}

/// Rescale all `\k` values so they sum to `target_cs` (keeps proportions, fixes rounding drift).
pub fn kara_scale_to(text: &str, target_cs: i64) -> String {
    let total = kara_total_cs(text);
    if total <= 0 || target_cs <= 0 {
        return text.to_string();
    }
    let mut segs = tags::parse(text);
    let mut used = 0i64;
    let mut seen = 0i64;
    let n_k: usize = segs.iter().map(|s| if let Segment::Block(i) = s { i.iter().filter(|x| matches!(x, Item::Tag { name, .. } if matches!(name.as_str(), "k"|"K"|"kf"|"ko"))).count() } else { 0 }).sum();
    for seg in segs.iter_mut() {
        if let Segment::Block(items) = seg {
            for it in items.iter_mut() {
                if let Item::Tag { name, value } = it {
                    if matches!(name.as_str(), "k" | "K" | "kf" | "ko") {
                        seen += 1;
                        let old: i64 = value.trim().parse().unwrap_or(0);
                        let new = if seen as usize == n_k { (target_cs - used).max(0) } else { (old as f64 * target_cs as f64 / total as f64).round() as i64 };
                        used += new;
                        *value = new.to_string();
                    }
                }
            }
        }
    }
    tags::to_string(&segs)
}

// ---------------------------------------------------------------- styles

/// Names of styles no event uses.
pub fn unused_styles(script: &Script) -> Vec<String> {
    script.styles.iter().filter(|s| !script.events.iter().any(|e| e.style == s.name)).map(|s| s.name.clone()).collect()
}

/// Copy styles from `other` into `script` (replacing same-named ones if `overwrite`).
pub fn import_styles(script: &mut Script, other: &Script, names: &[String], overwrite: bool) -> usize {
    let mut n = 0;
    for st in other.styles.iter().filter(|s| names.is_empty() || names.contains(&s.name)) {
        match script.styles.iter_mut().find(|s| s.name == st.name) {
            Some(existing) if overwrite => {
                *existing = st.clone();
                n += 1;
            }
            Some(_) => {}
            None => {
                script.styles.push(st.clone());
                n += 1;
            }
        }
    }
    n
}

/// Split an event at each `\N` into separate events with proportional timing.
pub fn split_on_breaks(e: &Event) -> Vec<Event> {
    let parts: Vec<&str> = e.text.split("\\N").collect();
    if parts.len() < 2 {
        return vec![e.clone()];
    }
    let total: usize = parts.iter().map(|p| tags::visible_len(p).max(1)).sum();
    let mut t = e.start;
    let mut out = vec![];
    for (i, p) in parts.iter().enumerate() {
        let w = tags::visible_len(p).max(1);
        let end = if i + 1 == parts.len() { e.end } else { t + ((e.end - e.start) as f64 * w as f64 / total as f64).round() as i64 };
        let mut ne = e.clone();
        ne.text = p.trim().to_string();
        ne.start = t;
        ne.end = end;
        out.push(ne);
        t = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_is_tag_safe_and_turkish() {
        assert_eq!(change_case("{\\i1}istanbul{\\i0} ışık\\Nilk", Case::Upper), "{\\i1}İSTANBUL{\\i0} IŞIK\\Nİlk".replace("İlk", "İLK"));
        assert_eq!(change_case("İSTANBUL IŞIK", Case::Lower), "istanbul ışık");
        assert_eq!(change_case("MERHABA. NASILSIN? iyiyim", Case::Sentence), "Merhaba. Nasılsın? İyiyim");
    }

    #[test]
    fn balancing() {
        let t = "Bu çok uzun bir cümle ve iki satıra bölünmesi gerekiyor gerçekten";
        let b = balance_lines(t, 30);
        let parts: Vec<&str> = b.split("\\N").collect();
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.chars().count() <= 36), "{b}");
        assert_eq!(balance_lines("kısa", 30), "kısa");
        assert_eq!(unbreak("a\\Nb"), "a b");
        // tags are never split
        let tagged = "{\\i1}Bu çok uzun bir cümle{\\i0} ve iki satıra bölünmesi gerekiyor gerçekten";
        assert!(balance_lines(tagged, 30).contains("{\\i1}"));
    }

    #[test]
    fn hearing_impaired_and_fixes() {
        assert_eq!(strip_hearing_impaired("[kapı çarpar] Selam (güler)").trim(), "Selam");
        assert_eq!(strip_hearing_impaired("AHMET: Merhaba"), "Merhaba");
        let f = fix_common("Merhaba ,dünya ... nasılsın?Iyi  misin", &FixOptions::default());
        assert_eq!(f, "Merhaba, dünya… nasılsın? Iyi misin");
        assert_eq!(fix_common("-Evet\\N-Hayır", &FixOptions::default()), "- Evet\\N- Hayır");
    }

    #[test]
    fn tag_ops() {
        assert_eq!(remove_tags("{\\an8\\fscx120\\b1}x", &["fscx", "an"]), "{\\b1}x");
        assert_eq!(remove_tags("{\\fscx120}x", &["fscx"]), "x");
        assert_eq!(merge_blocks("{\\b1}{\\i1}x"), "{\\b1\\i1}x");
        assert_eq!(merge_blocks("{\\k10}a{\\k10}b"), "{\\k10}a{\\k10}b");
    }

    #[test]
    fn kara_parse_build_roundtrip() {
        let t = "{\\k20\\b1}ka{\\kf30}ra{\\ko50}oke";
        let p = kara_parse(t);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0], (20, "k".to_string(), "\\b1".to_string(), "ka".to_string()));
        assert_eq!(kara_build(&p), t);
    }

    #[test]
    fn override_set_get() {
        assert_eq!(set_override("hello", "pos", "(1,2)"), "{\\pos(1,2)}hello");
        assert_eq!(set_override("{\\an8\\pos(1,2)}x", "pos", "(5,6)"), "{\\pos(5,6)\\an8}x");
        assert_eq!(set_override("{\\move(1,2,3,4)}x", "pos", "(5,6)"), "{\\pos(5,6)}x");
        assert_eq!(get_override("{\\an8\\pos(1,2)}x", "pos").as_deref(), Some("(1,2)"));
        assert_eq!(get_override("x", "pos"), None);
    }

    #[test]
    fn karaoke() {
        let k = kara_split_chars("abc", 90);
        assert_eq!(k, "{\\k30}a{\\k30}b{\\k30}c");
        assert_eq!(kara_total_cs(&k), 90);
        assert_eq!(kara_total_cs(&kara_scale_to(&k, 100)), 100);
        assert_eq!(kara_scale_to("{\\k10}a{\\k30}b", 80), "{\\k20}a{\\k60}b");
    }

    #[test]
    fn styles_and_split() {
        let mut a = Script::new();
        let mut b = Script::new();
        b.styles.push(crate::ass::Style { name: "Sign".into(), ..Default::default() });
        assert_eq!(import_styles(&mut a, &b, &[], false), 1);
        assert_eq!(a.styles.len(), 2);
        assert_eq!(unused_styles(&a), vec!["Default".to_string(), "Sign".to_string()]);
        let e = Event::new(0, 1000, "Default", "ab\\Ncdef");
        let s = split_on_breaks(&e);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].end, s[1].start);
        assert_eq!(s[1].end, 1000);
    }
}
