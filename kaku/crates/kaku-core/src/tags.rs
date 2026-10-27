//! Override-tag parsing: splits dialogue text into `{...}` tag blocks and plain text.

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub name: String,
    pub arg: String,
}

impl Tag {
    pub fn render(&self) -> String {
        format!("\\{}{}", self.name, self.arg)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    Text(String),
    /// A `{...}` block. Non-tag content (comments) is kept as a `Tag` with empty name.
    Block(Vec<Tag>),
}

/// Tag names, longest first so that prefix matching is greedy-correct.
const NAMES: &[&str] = &[
    "fscx", "fscy", "xbord", "ybord", "xshad", "yshad", "alpha", "iclip", "blur", "bord", "shad", "move", "clip", "fade",
    "fax", "fay", "fad", "frx", "fry", "frz", "fsp", "pbo", "pos", "org", "kf", "ko", "an", "be", "fe", "fn", "fr", "fs",
    "1c", "2c", "3c", "4c", "1a", "2a", "3a", "4a", "K", "k", "q", "r", "t", "p", "a", "b", "c", "i", "s", "u",
];

fn match_name(s: &str) -> Option<&'static str> {
    NAMES.iter().copied().find(|n| s.starts_with(n))
}

/// Parse the inside of a `{...}` block.
pub fn parse_block(inner: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    let mut comment_start = 0;
    let flush_comment = |tags: &mut Vec<Tag>, from: usize, to: usize| {
        if to > from {
            tags.push(Tag { name: String::new(), arg: inner[from..to].to_string() });
        }
    };
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if let Some(name) = match_name(&inner[i + 1..]) {
                flush_comment(&mut tags, comment_start, i);
                let astart = i + 1 + name.len();
                let mut j = astart;
                let mut depth = 0;
                while j < bytes.len() {
                    match bytes[j] {
                        b'(' => depth += 1,
                        b')' => {
                            if depth > 0 {
                                depth -= 1
                            }
                        }
                        b'\\' if depth == 0 => break,
                        _ => {}
                    }
                    j += 1;
                }
                tags.push(Tag { name: name.to_string(), arg: inner[astart..j].to_string() });
                i = j;
                comment_start = j;
                continue;
            }
        }
        i += 1;
    }
    flush_comment(&mut tags, comment_start, bytes.len());
    tags
}

pub fn parse(text: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if open > 0 {
                out.push(Seg::Text(rest[..open].to_string()));
            }
            if let Some(close) = rest[open..].find('}') {
                out.push(Seg::Block(parse_block(&rest[open + 1..open + close])));
                rest = &rest[open + close + 1..];
            } else {
                out.push(Seg::Text(rest[open..].to_string()));
                break;
            }
        } else {
            out.push(Seg::Text(rest.to_string()));
            break;
        }
    }
    out
}

pub fn render(segs: &[Seg]) -> String {
    let mut s = String::new();
    for seg in segs {
        match seg {
            Seg::Text(t) => s.push_str(t),
            Seg::Block(tags) => {
                s.push('{');
                for t in tags {
                    if t.name.is_empty() {
                        s.push_str(&t.arg);
                    } else {
                        s.push_str(&t.render());
                    }
                }
                s.push('}');
            }
        }
    }
    s
}

/// Remove all override blocks, keeping `\N`, `\n`, `\h` literal.
pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth_open = false;
    for c in text.chars() {
        match c {
            '{' if !depth_open => depth_open = true,
            '}' if depth_open => depth_open = false,
            _ if depth_open => {}
            _ => out.push(c),
        }
    }
    out
}

/// Plain visible text: tags removed, `\N`/`\n` → newline, `\h` → space. Used for CPS & length checks.
pub fn visible_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Number of visible characters excluding whitespace/newlines (the common CPS convention).
pub fn visible_len(text: &str) -> usize {
    visible_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

/// Drawing mode check: text following `\p1` is vector drawing, not words.
pub fn has_drawing(text: &str) -> bool {
    parse(text).iter().any(|s| match s {
        Seg::Block(tags) => tags.iter().any(|t| t.name == "p" && t.arg.trim().parse::<i32>().map(|v| v > 0).unwrap_or(false)),
        _ => false,
    })
}

/// Merge adjacent `{..}{..}` blocks (Aegisub "cleanup" behaviour).
pub fn merge_blocks(text: &str) -> String {
    let segs = parse(text);
    let mut out: Vec<Seg> = Vec::new();
    for s in segs {
        match (out.last_mut(), s) {
            (Some(Seg::Block(a)), Seg::Block(b)) => a.extend(b),
            (_, s) => out.push(s),
        }
    }
    render(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_and_roundtrips() {
        let t = "{\\an8\\pos(10,20)\\t(0,500,\\fs60\\1c&H0000FF&)\\fnMy Font}Hi{\\r}there";
        let segs = parse(t);
        assert_eq!(render(&segs), t);
        if let Seg::Block(tags) = &segs[0] {
            let names: Vec<_> = tags.iter().map(|t| t.name.as_str()).collect();
            assert_eq!(names, ["an", "pos", "t", "fn"]);
            assert_eq!(tags[2].arg, "(0,500,\\fs60\\1c&H0000FF&)");
        } else {
            panic!()
        }
    }

    #[test]
    fn fs_vs_fscx() {
        let tags = parse_block("\\fscx120\\fs30\\fsp2");
        let names: Vec<_> = tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["fscx", "fs", "fsp"]);
    }

    #[test]
    fn comments_kept() {
        let t = "{TODO: check\\i1}x";
        assert_eq!(render(&parse(t)), t);
    }

    #[test]
    fn visible() {
        assert_eq!(visible_text("{\\i1}Hello\\Nworld{\\i0}"), "Hello\nworld");
        assert_eq!(visible_len("{\\i1}He llo\\Nw{\\i0}"), 6);
        assert!(has_drawing("{\\p1}m 0 0 l 10 10{\\p0}"));
        assert_eq!(merge_blocks("{\\i1}{\\b1}x"), "{\\i1\\b1}x");
    }
}
