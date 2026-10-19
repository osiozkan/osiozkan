//! ASS override-tag parsing and text helpers.

/// Known tag names, longest first so prefix matching is greedy-correct.
const TAG_NAMES: &[&str] = &[
    "fscx", "fscy", "frx", "fry", "frz", "fax", "fay", "fad", "fade", "fsp", "blur", "bord", "shad", "xbord", "ybord", "xshad", "yshad",
    "alpha", "1a", "2a", "3a", "4a", "1c", "2c", "3c", "4c", "iclip", "clip", "move", "pos", "org", "kf", "ko", "kt", "an", "be", "fn",
    "fr", "fs", "fe", "pbo", "q", "r", "t", "a", "b", "c", "i", "k", "K", "p", "s", "u",
];

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `\name` + value (value is raw text, parentheses included for `\t(…)` etc.)
    Tag { name: String, value: String },
    /// Anything inside `{}` that isn't a tag (comments, junk).
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text(String),
    Block(Vec<Item>),
}

/// Parse an event text into text runs and override blocks.
pub fn parse(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if open > 0 {
                out.push(Segment::Text(rest[..open].to_string()));
            }
            let after = &rest[open + 1..];
            match after.find('}') {
                Some(close) => {
                    out.push(Segment::Block(parse_block(&after[..close])));
                    rest = &after[close + 1..];
                }
                None => {
                    // Unterminated: treat as literal text.
                    out.push(Segment::Text(rest[open..].to_string()));
                    break;
                }
            }
        } else {
            out.push(Segment::Text(rest.to_string()));
            break;
        }
    }
    out
}

fn parse_block(s: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut lit_start = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if i > lit_start {
                items.push(Item::Comment(s[lit_start..i].to_string()));
            }
            let (item, next) = parse_tag(s, i + 1);
            items.push(item);
            i = next;
            lit_start = i;
        } else {
            i += 1;
        }
    }
    if lit_start < s.len() {
        items.push(Item::Comment(s[lit_start..].to_string()));
    }
    items
}

fn parse_tag(s: &str, start: usize) -> (Item, usize) {
    let rest = &s[start..];
    let name = TAG_NAMES.iter().filter(|n| rest.starts_with(**n)).max_by_key(|n| n.len());
    let (name, vstart) = match name {
        Some(n) => (n.to_string(), start + n.len()),
        None => {
            // unknown tag: name = leading alphanumerics
            let n: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let l = n.len();
            (n, start + l)
        }
    };
    // value runs until the next backslash outside parentheses
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut j = vstart;
    while j < b.len() {
        match b[j] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'\\' if depth <= 0 => break,
            _ => {}
        }
        j += 1;
    }
    (Item::Tag { name, value: s[vstart..j].to_string() }, j)
}

pub fn to_string(segs: &[Segment]) -> String {
    let mut o = String::new();
    for s in segs {
        match s {
            Segment::Text(t) => o.push_str(t),
            Segment::Block(items) => {
                o.push('{');
                for it in items {
                    match it {
                        Item::Tag { name, value } => {
                            o.push('\\');
                            o.push_str(name);
                            o.push_str(value);
                        }
                        Item::Comment(c) => o.push_str(c),
                    }
                }
                o.push('}');
            }
        }
    }
    o
}

/// Remove all `{...}` override blocks.
pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = false;
    for c in text.chars() {
        match c {
            '{' if !depth => depth = true,
            '}' if depth => depth = false,
            c if !depth => out.push(c),
            _ => {}
        }
    }
    out
}

/// Text as the viewer sees it: tags stripped, `\N`→newline, `\n`/`\h`→space,
/// vector drawings (`\p1`) omitted.
pub fn visible_text(text: &str) -> String {
    let mut out = String::new();
    let mut drawing = false;
    for seg in parse(text) {
        match seg {
            Segment::Block(items) => {
                for it in items {
                    if let Item::Tag { name, value } = it {
                        if name == "p" {
                            drawing = value.trim().parse::<i32>().map(|v| v > 0).unwrap_or(false);
                        }
                    }
                }
            }
            Segment::Text(t) => {
                if !drawing {
                    out.push_str(&t);
                }
            }
        }
    }
    out.replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ")
}

/// Number of visible characters, ignoring whitespace and line breaks (for CPS).
pub fn visible_len(text: &str) -> usize {
    visible_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

/// Split into display lines (on `\N`).
pub fn lines(text: &str) -> Vec<String> {
    visible_text(text).split('\n').map(|s| s.to_string()).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syllable {
    pub duration_cs: i64,
    pub kind: String,
    pub text: String,
    /// non-karaoke tags that precede the text in the same block
    pub other_tags: String,
}

/// Extract karaoke syllables (`\k`, `\kf`, `\ko`, `\K`).
pub fn syllables(text: &str) -> Vec<Syllable> {
    let mut out: Vec<Syllable> = Vec::new();
    let mut pending: Option<(i64, String)> = None;
    let mut other = String::new();
    for seg in parse(text) {
        match seg {
            Segment::Block(items) => {
                for it in items {
                    match it {
                        Item::Tag { name, value } if matches!(name.as_str(), "k" | "K" | "kf" | "ko") => {
                            if let Some((d, k)) = pending.take() {
                                out.push(Syllable { duration_cs: d, kind: k, text: String::new(), other_tags: std::mem::take(&mut other) });
                            }
                            pending = Some((value.trim().parse().unwrap_or(0), name));
                        }
                        Item::Tag { name, value } => {
                            other.push('\\');
                            other.push_str(&name);
                            other.push_str(&value);
                        }
                        Item::Comment(_) => {}
                    }
                }
            }
            Segment::Text(t) => {
                let (d, k) = pending.take().unwrap_or((0, "k".into()));
                out.push(Syllable { duration_cs: d, kind: k, text: t, other_tags: std::mem::take(&mut other) });
            }
        }
    }
    if let Some((d, k)) = pending {
        out.push(Syllable { duration_cs: d, kind: k, text: String::new(), other_tags: other });
    }
    out
}

/// Validate braces; returns a message when unbalanced.
pub fn check_braces(text: &str) -> Option<&'static str> {
    let mut open = false;
    for c in text.chars() {
        match c {
            '{' if open => return Some("nested '{' inside override block"),
            '{' => open = true,
            '}' if !open => return Some("stray '}'"),
            '}' => open = false,
            _ => {}
        }
    }
    if open {
        Some("unclosed '{'")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        let t = "{\\an8\\pos(100,200)\\t(0,500,\\fscx120)\\fnArial Black}Hello {\\i1}world{\\i0}\\Nnext";
        let segs = parse(t);
        assert_eq!(to_string(&segs), t);
        if let Segment::Block(items) = &segs[0] {
            assert_eq!(items[0], Item::Tag { name: "an".into(), value: "8".into() });
            assert_eq!(items[1], Item::Tag { name: "pos".into(), value: "(100,200)".into() });
            assert_eq!(items[2], Item::Tag { name: "t".into(), value: "(0,500,\\fscx120)".into() });
            assert_eq!(items[3], Item::Tag { name: "fn".into(), value: "Arial Black".into() });
        } else {
            panic!()
        }
    }

    #[test]
    fn visible() {
        assert_eq!(visible_text("{\\b1}Selam{\\b0}\\Ndünya\\hx"), "Selam\ndünya x");
        assert_eq!(visible_text("{\\p1}m 0 0 l 10 10{\\p0}ok"), "ok");
        assert_eq!(visible_len("a b\\Nc"), 3);
    }

    #[test]
    fn karaoke() {
        let s = syllables("{\\k20}ka{\\k30\\b1}ra{\\kf10}o");
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].duration_cs, 20);
        assert_eq!(s[1].text, "ra");
        assert_eq!(s[1].other_tags, "\\b1");
        assert_eq!(s[2].kind, "kf");
    }

    #[test]
    fn braces() {
        assert!(check_braces("{a}b").is_none());
        assert!(check_braces("{a b").is_some());
        assert!(check_braces("a}").is_some());
    }
}
