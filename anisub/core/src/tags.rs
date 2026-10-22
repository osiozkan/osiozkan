//! Override-tag aware text helpers.

/// Split ASS text into alternating (is_tag_block, content) pieces.
/// Tag blocks include the braces.
pub fn split_blocks(text: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with('{') {
            match rest.find('}') {
                Some(e) => {
                    out.push((true, &rest[..=e]));
                    rest = &rest[e + 1..];
                }
                None => {
                    out.push((false, rest));
                    break;
                }
            }
        } else {
            let e = rest.find('{').unwrap_or(rest.len());
            out.push((false, &rest[..e]));
            rest = &rest[e..];
        }
    }
    out
}

/// Remove all `{...}` blocks.
pub fn strip_tags(text: &str) -> String {
    split_blocks(text).into_iter().filter(|(t, _)| !*t).map(|(_, s)| s).collect()
}

/// Visible text: tags removed and `\N`, `\n`, `\h` resolved.
pub fn plain_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", " ").replace("\\h", "\u{a0}")
}

/// Count of visible characters excluding line breaks and whitespace (used for CPS).
pub fn visible_chars(text: &str) -> usize {
    plain_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strip() {
        assert_eq!(strip_tags("a{\\b1}b{\\b0}c"), "abc");
        assert_eq!(plain_text("a\\Nb{\\i1}"), "a\nb");
        assert_eq!(visible_chars("ab {\\i1}c\\Nd"), 4);
    }
}
