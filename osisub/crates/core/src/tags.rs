//! ASS override-tag helpers.

#[derive(Debug, Clone, PartialEq)]
pub enum Block<'a> {
    Text(&'a str),
    /// Contents of a `{...}` block, without the braces.
    Override(&'a str),
}

/// Split dialogue text into plain-text and override blocks. An unterminated `{`
/// is treated as plain text (as renderers do).
pub fn split_blocks(text: &str) -> Vec<Block<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find('{') {
            Some(0) => match rest.find('}') {
                Some(end) => {
                    out.push(Block::Override(&rest[1..end]));
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push(Block::Text(rest));
                    break;
                }
            },
            Some(i) => {
                out.push(Block::Text(&rest[..i]));
                rest = &rest[i..];
            }
            None => {
                out.push(Block::Text(rest));
                break;
            }
        }
    }
    out
}

/// Text with override blocks removed; ASS escapes are left untouched.
pub fn strip_tags(text: &str) -> String {
    split_blocks(text)
        .into_iter()
        .filter_map(|b| if let Block::Text(t) = b { Some(t) } else { None })
        .collect()
}

/// Human-visible text: tags removed, `\N`/`\n` -> newline, `\h` -> space.
pub fn visible_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Visible lines of an event.
pub fn visible_lines(text: &str) -> Vec<String> {
    visible_text(text).split('\n').map(|s| s.to_string()).collect()
}

/// Characters that count towards reading speed (excludes whitespace and tags).
pub fn cps_chars(text: &str) -> usize {
    visible_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

/// True if any override block contains the given tag name (e.g. `pos`, `move`).
pub fn has_tag(text: &str, tag: &str) -> bool {
    split_blocks(text).iter().any(|b| match b {
        Block::Override(o) => o.split('\\').skip(1).any(|t| {
            t.starts_with(tag)
                && t[tag.len()..].chars().next().map_or(true, |c| !c.is_ascii_alphabetic() || tag.len() == 0)
        }),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks() {
        let b = split_blocks("a{\\i1}b{\\i0}c{x");
        assert_eq!(b.len(), 6);
        assert_eq!(strip_tags("a{\\i1}b{\\i0}c"), "abc");
    }
    #[test]
    fn visible() {
        assert_eq!(visible_text("{\\an8}Hi\\Nyou\\hok"), "Hi\nyou ok");
        assert_eq!(cps_chars("{\\b1}ab c"), 3);
    }
    #[test]
    fn tag_detect() {
        assert!(has_tag("{\\an8\\pos(1,2)}x", "pos"));
        assert!(!has_tag("{\\pos(1,2)}x", "p"));
        assert!(has_tag("{\\move(1,2,3,4)}x", "move"));
    }
}
