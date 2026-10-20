//! Override tag parsing: split text into blocks, strip tags, karaoke syllables.

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// Plain text outside `{}`.
    Text(String),
    /// Contents of a `{...}` block (without braces).
    Block(String),
}

pub fn split_parts(text: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        match rest[open..].find('}') {
            Some(close) => {
                cur.push_str(&rest[..open]);
                if !cur.is_empty() { out.push(Part::Text(std::mem::take(&mut cur))); }
                out.push(Part::Block(rest[open + 1..open + close].to_string()));
                rest = &rest[open + close + 1..];
            }
            None => break,
        }
    }
    cur.push_str(rest);
    if !cur.is_empty() { out.push(Part::Text(cur)); }
    out
}

pub fn join_parts(parts: &[Part]) -> String {
    let mut s = String::new();
    for p in parts {
        match p {
            Part::Text(t) => s.push_str(t),
            Part::Block(b) => { s.push('{'); s.push_str(b); s.push('}'); }
        }
    }
    s
}

/// Remove `{}` blocks. Keeps `\N`, `\n`, `\h` as-is.
pub fn strip_tags(text: &str) -> String {
    split_parts(text).into_iter().filter_map(|p| if let Part::Text(t) = p { Some(t) } else { None }).collect()
}

/// Display form: removes blocks and converts `\N`/`\n` to newline, `\h` to nbsp-space.
pub fn display_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Number of visible characters (excluding line breaks).
pub fn visible_len(text: &str) -> usize {
    display_text(text).chars().filter(|c| *c != '\n').count()
}

/// Split a tag block into individual tags (`\pos(1,2)`, `\k20`, ...). Handles nested parentheses and text before first `\`.
pub fn split_block_tags(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut prev_bs = false;
    for c in block.chars() {
        match c {
            '(' => { depth += 1; cur.push(c); }
            ')' => { depth -= 1; cur.push(c); }
            '\\' if depth <= 0 => {
                if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
                cur.push(c);
            }
            _ => cur.push(c),
        }
        prev_bs = c == '\\';
    }
    let _ = prev_bs;
    if !cur.is_empty() { out.push(cur); }
    out
}

/// Split a tag into (name, argument). e.g. `\fscx120` → ("fscx", "120"), `\1c&HFF&` → ("1c", "&HFF&").
pub fn tag_name_arg(tag: &str) -> (String, String) {
    let t = tag.strip_prefix('\\').unwrap_or(tag);
    const NAMES: &[&str] = &[
        "fscx", "fscy", "fsp", "fad", "fade", "fnc", "fn", "fs", "fr", "frx", "fry", "frz", "fax", "fay", "fe",
        "move", "pos", "org", "clip", "iclip", "xbord", "ybord", "xshad", "yshad", "bord", "shad", "blur", "be",
        "1c", "2c", "3c", "4c", "1a", "2a", "3a", "4a", "alpha", "an", "a", "kf", "ko", "kt", "k", "K", "q", "r", "t",
        "pbo", "p", "b", "i", "u", "s", "c",
    ];
    // longest-prefix match
    let mut best = "";
    for n in NAMES { if t.starts_with(n) && n.len() > best.len() { best = n; } }
    if best.is_empty() { return (String::new(), t.to_string()); }
    (best.to_string(), t[best.len()..].to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Syllable {
    pub duration_cs: i64,
    pub kind: String, // "k", "kf", "ko", "K"
    pub text: String,
    /// Non-karaoke tags that appeared in the block alongside the \k tag.
    pub extra_tags: String,
}

/// Parse karaoke syllables from a text line. Text preceding the first \k tag becomes a zero-duration syllable.
pub fn karaoke_syllables(text: &str) -> Vec<Syllable> {
    let mut out: Vec<Syllable> = Vec::new();
    let mut pending_text = String::new();
    for part in split_parts(text) {
        match part {
            Part::Text(t) => {
                if let Some(last) = out.last_mut() { last.text.push_str(&t); } else { pending_text.push_str(&t); }
            }
            Part::Block(b) => {
                let mut k: Option<(String, i64)> = None;
                let mut extra = String::new();
                for tag in split_block_tags(&b) {
                    let (nm, arg) = tag_name_arg(&tag);
                    if matches!(nm.as_str(), "k" | "kf" | "ko" | "K") {
                        let d = arg.trim().parse::<f64>().unwrap_or(0.0) as i64;
                        k = Some((if nm == "K" { "kf".into() } else { nm }, d));
                    } else {
                        extra.push_str(&tag);
                    }
                }
                if let Some((kind, d)) = k {
                    out.push(Syllable { duration_cs: d, kind, text: String::new(), extra_tags: extra });
                } else if let Some(last) = out.last_mut() {
                    // Non-k block mid-syllable: keep inline
                    last.text.push('{'); last.text.push_str(&b); last.text.push('}');
                } else {
                    pending_text.push('{'); pending_text.push_str(&b); pending_text.push('}');
                }
            }
        }
    }
    if !pending_text.is_empty() {
        out.insert(0, Syllable { duration_cs: 0, kind: "k".into(), text: pending_text, extra_tags: String::new() });
    }
    out
}

pub fn build_karaoke(syls: &[Syllable]) -> String {
    let mut s = String::new();
    for y in syls {
        s.push_str(&format!("{{\\{}{}{}}}{}", y.kind, y.duration_cs, y.extra_tags, y.text));
    }
    s
}

/// Auto-split each syllable at spaces/hyphen, distributing duration proportionally by character count.
pub fn split_syllables_by_char(syls: &[Syllable]) -> Vec<Syllable> {
    let mut out = Vec::new();
    for y in syls {
        let chars: Vec<char> = y.text.chars().collect();
        if chars.len() <= 1 { out.push(y.clone()); continue; }
        let n = chars.len() as i64;
        let mut used = 0;
        for (idx, ch) in chars.iter().enumerate() {
            let d = if idx as i64 == n - 1 { y.duration_cs - used } else { y.duration_cs / n };
            used += d;
            out.push(Syllable { duration_cs: d, kind: y.kind.clone(), text: ch.to_string(), extra_tags: if idx == 0 { y.extra_tags.clone() } else { String::new() } });
        }
    }
    out
}

/// Total karaoke duration in centiseconds.
pub fn karaoke_total_cs(text: &str) -> i64 {
    karaoke_syllables(text).iter().map(|s| s.duration_cs).sum()
}

/// Remove all karaoke tags (\k, \kf, \ko, \K) while keeping other tags.
pub fn strip_karaoke(text: &str) -> String {
    let parts = split_parts(text).into_iter().filter_map(|p| match p {
        Part::Text(t) => Some(Part::Text(t)),
        Part::Block(b) => {
            let kept: String = split_block_tags(&b).into_iter().filter(|t| {
                let (n, _) = tag_name_arg(t);
                !matches!(n.as_str(), "k" | "kf" | "ko" | "K")
            }).collect();
            if kept.is_empty() && !b.is_empty() { None } else { Some(Part::Block(kept)) }
        }
    }).collect::<Vec<_>>();
    join_parts(&parts)
}

/// Remove every override block.
pub fn strip_all_blocks(text: &str) -> String { strip_tags(text) }

/// Remove specific tag names across all blocks (e.g. ["blur","be"]).
pub fn strip_tag_names(text: &str, names: &[&str]) -> String {
    let parts: Vec<Part> = split_parts(text).into_iter().filter_map(|p| match p {
        Part::Text(t) => Some(Part::Text(t)),
        Part::Block(b) => {
            let kept: String = split_block_tags(&b).into_iter().filter(|t| {
                let (n, _) = tag_name_arg(t);
                !names.contains(&n.as_str())
            }).collect();
            if kept.is_empty() { None } else { Some(Part::Block(kept)) }
        }
    }).collect();
    join_parts(&parts)
}

/// Merge adjacent `{..}{..}` blocks and drop empty ones ("clean tags").
pub fn clean_tags(text: &str) -> String {
    let mut out: Vec<Part> = Vec::new();
    for p in split_parts(text) {
        match (&p, out.last_mut()) {
            (Part::Block(b), Some(Part::Block(prev))) => prev.push_str(b),
            (Part::Block(b), _) if b.is_empty() => {}
            _ => out.push(p),
        }
    }
    join_parts(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strip_and_display() {
        assert_eq!(strip_tags("{\\i1}Hi{\\i0} there"), "Hi there");
        assert_eq!(display_text("a\\Nb"), "a\nb");
        assert_eq!(visible_len("{\\b1}abc\\Nde"), 5);
    }
    #[test]
    fn tags() {
        assert_eq!(split_block_tags("\\pos(1,2)\\fscx120\\t(0,100,\\fs20)"), vec!["\\pos(1,2)", "\\fscx120", "\\t(0,100,\\fs20)"]);
        assert_eq!(tag_name_arg("\\fscx120"), ("fscx".into(), "120".into()));
        assert_eq!(tag_name_arg("\\1c&HFF&"), ("1c".into(), "&HFF&".into()));
        assert_eq!(tag_name_arg("\\kf25"), ("kf".into(), "25".into()));
    }
    #[test]
    fn karaoke() {
        let s = karaoke_syllables("{\\k20}ka{\\kf30}ra{\\k10}ok{\\i1}e");
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].text, "ka");
        assert_eq!(s[2].text, "ok{\\i1}e".replace("{\\i1}", "{\\i1}"));
        assert_eq!(karaoke_total_cs("{\\k20}ka{\\kf30}ra"), 50);
        assert_eq!(build_karaoke(&karaoke_syllables("{\\k20}ka{\\kf30}ra")), "{\\k20}ka{\\kf30}ra");
        assert_eq!(strip_karaoke("{\\k20\\i1}ka{\\kf30}ra"), "{\\i1}kara");
    }
    #[test]
    fn clean() {
        assert_eq!(clean_tags("{\\i1}{\\b1}x{}y"), "{\\i1\\b1}xy");
    }
}
