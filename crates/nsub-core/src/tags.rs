//! Override-tag tokenizer. Splits event text into plain text and `{...}` blocks and
//! parses blocks into tags (no delimiter between tag name and value in ASS, so known
//! names are matched longest-first).

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Text(String),
    /// A `{...}` block. Contains parsed tags and any comment text.
    Block(Vec<Tag>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    /// Tag name without backslash (`""` for comment/garbage text inside the block).
    pub name: String,
    /// Raw parameter text, for `\t(...)`/`\pos(...)` the text inside the parentheses (including them, as written).
    pub value: String,
}

impl Tag {
    pub fn to_ass(&self) -> String {
        if self.name.is_empty() {
            self.value.clone()
        } else {
            format!("\\{}{}", self.name, self.value)
        }
    }
}

const NAMES: &[&str] = &[
    "fscx", "fscy", "fsp", "fs", "fad", "fade", "fn", "fr", "frx", "fry", "frz", "fax", "fay", "bord", "xbord", "ybord", "shad", "xshad",
    "yshad", "blur", "be", "alpha", "1a", "2a", "3a", "4a", "1c", "2c", "3c", "4c", "c", "an", "a", "pos", "move", "org", "clip", "iclip",
    "fe", "kf", "ko", "kt", "k", "K", "q", "r", "t", "b", "i", "u", "s", "p", "pbo", "h", "N", "n", "jitter", "z",
];

fn sorted_names() -> Vec<&'static str> {
    let mut v = NAMES.to_vec();
    v.sort_by_key(|n| std::cmp::Reverse(n.len()));
    v
}

fn split_tag(s: &str, names: &[&'static str]) -> (String, String) {
    for n in names {
        if s.starts_with(n) {
            return (n.to_string(), s[n.len()..].to_string());
        }
    }
    (String::new(), s.to_string())
}

/// Parse the content of a block (without braces) into tags.
pub fn parse_block(inner: &str) -> Vec<Tag> {
    let names = sorted_names();
    let mut tags = Vec::new();
    let mut i = 0;
    let b = inner.as_bytes();
    let mut lead = String::new();
    while i < b.len() && b[i] != b'\\' {
        lead.push(inner[i..].chars().next().unwrap());
        i += inner[i..].chars().next().unwrap().len_utf8();
    }
    if !lead.is_empty() {
        tags.push(Tag { name: String::new(), value: lead });
    }
    while i < b.len() {
        // b[i] == '\\'
        let start = i + 1;
        let mut j = start;
        let mut depth = 0;
        while j < b.len() {
            match b[j] {
                b'(' => depth += 1,
                b')' if depth > 0 => depth -= 1,
                b'\\' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        let body = &inner[start..j];
        let (name, value) = split_tag(body, &names);
        tags.push(Tag { name: if name.is_empty() && !body.is_empty() { String::from("?") } else { name }, value });
        i = j;
    }
    tags
}

pub fn parse(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if open > 0 {
                out.push(Segment::Text(rest[..open].to_string()));
            }
            match rest[open..].find('}') {
                Some(close) => {
                    out.push(Segment::Block(parse_block(&rest[open + 1..open + close])));
                    rest = &rest[open + close + 1..];
                }
                None => {
                    // unterminated: treat as text
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

pub fn serialize(segs: &[Segment]) -> String {
    let mut s = String::new();
    for seg in segs {
        match seg {
            Segment::Text(t) => s.push_str(t),
            Segment::Block(tags) => {
                s.push('{');
                for t in tags {
                    s.push_str(&t.to_ass());
                }
                s.push('}');
            }
        }
    }
    s
}

/// Plain visible text: tags removed, `\N`/`\n` → newline, `\h` → space.
/// Single pass, no intermediate tag parsing (hot path for QC over large files).
pub fn visible_text(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut start = 0; // start of pending plain run
    while i < b.len() {
        match b[i] {
            b'{' => match text[i..].find('}') {
                Some(close) => {
                    push_plain(&mut out, &text[start..i]);
                    i += close + 1;
                    start = i;
                }
                None => break, // unterminated: rest is plain text
            },
            _ => i += 1,
        }
    }
    push_plain(&mut out, &text[start..]);
    out
}

fn push_plain(out: &mut String, t: &str) {
    if !t.contains('\\') {
        out.push_str(t);
        return;
    }
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('N') | Some('n') => {
                    chars.next();
                    out.push('\n');
                    continue;
                }
                Some('h') => {
                    chars.next();
                    out.push(' ');
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
}

/// Remove all `{...}` blocks but keep line-break escapes.
pub fn strip_tags(text: &str) -> String {
    parse(text)
        .into_iter()
        .filter_map(|s| if let Segment::Text(t) = s { Some(t) } else { None })
        .collect()
}

/// True when a `{` or `}` is unbalanced.
pub fn has_unbalanced_braces(text: &str) -> bool {
    let mut open = false;
    for c in text.chars() {
        match c {
            '{' if open => return true,
            '{' => open = true,
            '}' if !open => return true,
            '}' => open = false,
            _ => {}
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_tags() {
        let segs = parse("{\\an8\\fscx120\\t(0,500,\\fs60\\c&HFF&)}Hi{\\i1}x");
        match &segs[0] {
            Segment::Block(t) => {
                assert_eq!(t[0], Tag { name: "an".into(), value: "8".into() });
                assert_eq!(t[1], Tag { name: "fscx".into(), value: "120".into() });
                assert_eq!(t[2].name, "t");
                assert!(t[2].value.contains("\\fs60"));
            }
            _ => panic!(),
        }
        assert_eq!(serialize(&segs), "{\\an8\\fscx120\\t(0,500,\\fs60\\c&HFF&)}Hi{\\i1}x");
    }
    #[test]
    fn visible() {
        assert_eq!(visible_text("{\\b1}A{\\b0}\\Nb\\hc"), "A\nb c");
        assert!(has_unbalanced_braces("{a"));
        assert!(!has_unbalanced_braces("{a}b"));
    }
}
