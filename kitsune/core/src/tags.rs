//! Override-tag block splitting and helpers.

#[derive(Clone, Debug, PartialEq)]
pub enum Part { Text(String), Tags(String) }

/// Split dialogue text into text runs and `{...}` blocks.
pub fn split(s: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        match rest[i..].find('}') {
            Some(j) => {
                cur.push_str(&rest[..i]);
                if !cur.is_empty() { out.push(Part::Text(std::mem::take(&mut cur))); }
                out.push(Part::Tags(rest[i + 1..i + j].to_string()));
                rest = &rest[i + j + 1..];
            }
            None => break,
        }
    }
    cur.push_str(rest);
    if !cur.is_empty() { out.push(Part::Text(cur)); }
    out
}

/// Remove all `{...}` blocks.
pub fn strip(s: &str) -> String {
    split(s).into_iter().filter_map(|p| if let Part::Text(t) = p { Some(t) } else { None }).collect()
}

/// Visible text: tags removed, `\N`/`\n` -> newline, `\h` -> space.
pub fn plain(s: &str) -> String {
    strip(s).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Characters excluding line breaks and tags (for CPS / line length).
pub fn visible_len(s: &str) -> usize { plain(s).chars().filter(|c| *c != '\n').count() }

/// Insert-or-replace the first occurrence of a simple tag (e.g. "pos") at text start.
pub fn prepend_tag(text: &str, tag: &str) -> String {
    if let Some(rest) = text.strip_prefix('{') {
        if let Some(j) = rest.find('}') { return format!("{{{}{}}}{}", tag, &rest[..j], &rest[j + 1..]); }
    }
    format!("{{{}}}{}", tag, text)
}
