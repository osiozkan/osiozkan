//! Override-tag helpers.
use regex::Regex;
use std::sync::OnceLock;

fn block_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\{[^}]*\}").unwrap())
}

/// Remove all `{...}` override blocks.
pub fn strip_tags(text: &str) -> String {
    block_re().replace_all(text, "").into_owned()
}

/// Text as the viewer reads it: no tags, `\N`/`\n` -> newline, `\h` -> space.
pub fn visible_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Visible character count excluding line breaks (used for CPS).
pub fn visible_len(text: &str) -> usize {
    visible_text(text).chars().filter(|c| *c != '\n').count()
}

/// Lengths of each visible line.
pub fn line_lengths(text: &str) -> Vec<usize> {
    visible_text(text).split('\n').map(|l| l.chars().count()).collect()
}

/// Split into alternating (is_tag_block, content) segments.
pub fn segments(text: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut last = 0;
    for m in block_re().find_iter(text) {
        if m.start() > last { out.push((false, text[last..m.start()].to_string())); }
        out.push((true, m.as_str().to_string()));
        last = m.end();
    }
    if last < text.len() { out.push((false, text[last..].to_string())); }
    out
}

/// Apply `f` to plain-text parts only, keeping `{...}` blocks untouched.
pub fn map_plain(text: &str, mut f: impl FnMut(&str) -> String) -> String {
    segments(text).into_iter().map(|(t, s)| if t { s } else { f(&s) }).collect()
}

/// Merge adjacent `{a}{b}` blocks into `{ab}`.
pub fn merge_blocks(text: &str) -> String {
    text.replace("}{", "")
}

/// Karaoke syllable: (duration in cs, text)
pub fn karaoke_syllables(text: &str) -> Vec<(i64, String)> {
    let re = Regex::new(r"\{[^}]*?\\k[fo]?(\d+)[^}]*\}").unwrap();
    let mut out = Vec::new();
    let mut pos = 0;
    let mut cur: Option<i64> = None;
    for c in re.captures_iter(text) {
        let m = c.get(0).unwrap();
        if let Some(d) = cur { out.push((d, text[pos..m.start()].to_string())); }
        cur = c[1].parse().ok();
        pos = m.end();
    }
    if let Some(d) = cur { out.push((d, text[pos..].to_string())); }
    out
}

/// Set (replace) `\pos(x,y)` in the first override block, creating one when needed.
pub fn set_pos(text: &str, x: f64, y: f64) -> String {
    let re = Regex::new(r"\\pos\([^)]*\)").unwrap();
    let f = |v: f64| { let s = format!("{:.2}", v); s.trim_end_matches('0').trim_end_matches('.').to_string() };
    let tag = format!("\\pos({},{})", f(x), f(y));
    if text.starts_with('{') {
        if let Some(end) = text.find('}') {
            let block = &text[..end];
            let cleaned = re.replace_all(block, "");
            return format!("{}{}{}", cleaned, tag, &text[end..]);
        }
    }
    format!("{{{}}}{}", tag, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basics() {
        assert_eq!(strip_tags("{\\i1}Hi{\\i0} you"), "Hi you");
        assert_eq!(visible_len("ab\\Ncd"), 4);
        assert_eq!(line_lengths("ab\\Ncde"), vec![2, 3]);
        assert_eq!(map_plain("{\\b1}a{\\b0}b", |s| s.to_uppercase()), "{\\b1}A{\\b0}B");
        let k = karaoke_syllables("{\\k10}ka{\\k20}ra");
        assert_eq!(k, vec![(10, "ka".into()), (20, "ra".into())]);
        assert_eq!(set_pos("hi", 10.0, 20.5), "{\\pos(10,20.5)}hi");
        assert_eq!(set_pos("{\\an8\\pos(1,2)}hi", 3.0, 4.0), "{\\an8\\pos(3,4)}hi");
    }
}
