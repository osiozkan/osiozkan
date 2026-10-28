//! Override-tag tokenizing and karaoke helpers.

use crate::time::Ms;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Contents of a `{...}` block, without the braces.
    Tags(String),
    Text(String),
}

pub fn split_blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if open > 0 {
                out.push(Block::Text(rest[..open].to_string()));
            }
            match rest[open..].find('}') {
                Some(close) => {
                    out.push(Block::Tags(rest[open + 1..open + close].to_string()));
                    rest = &rest[open + close + 1..];
                }
                None => {
                    out.push(Block::Text(rest[open..].to_string()));
                    break;
                }
            }
        } else {
            out.push(Block::Text(rest.to_string()));
            break;
        }
    }
    out
}

pub fn join_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| match b {
            Block::Tags(t) => format!("{{{t}}}"),
            Block::Text(t) => t.clone(),
        })
        .collect()
}

/// Text with all override blocks removed; hard/soft breaks kept as-is.
pub fn strip_tags(text: &str) -> String {
    split_blocks(text)
        .into_iter()
        .filter_map(|b| if let Block::Text(t) = b { Some(t) } else { None })
        .collect()
}

/// Visible text for reading-speed purposes: no tags, `\N` and `\n` become a space-less break,
/// `\h` becomes a space.
pub fn plain_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

const TAG_NAMES: &[&str] = &[
    "fscx", "fscy", "fsp", "fad", "fade", "fax", "fay", "frx", "fry", "frz", "fn", "fe", "fr",
    "fs", "bord", "xbord", "ybord", "shad", "xshad", "yshad", "blur", "be", "clip", "iclip",
    "kf", "ko", "kt", "k", "K", "an", "a", "pos", "move", "org", "alpha", "1a", "2a", "3a", "4a",
    "1c", "2c", "3c", "4c", "c", "b", "i", "u", "s", "q", "r", "t", "p", "pbo", "fsc",
];

/// Split a block's inner text into `(name, argument)` pairs, e.g. `\fs20\t(0,100,\fs30)`
/// gives `[("fs","20"), ("t","(0,100,\\fs30)")]`. Text before the first `\` (comments) is dropped.
pub fn parse_tags(block: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut started = false;
    let flush = |cur: &mut String, out: &mut Vec<(String, String)>| {
        if cur.is_empty() {
            return;
        }
        let name = TAG_NAMES.iter().find(|n| cur.starts_with(**n)).copied();
        match name {
            Some(n) => out.push((n.to_string(), cur[n.len()..].to_string())),
            None => out.push((cur.clone(), String::new())),
        }
        cur.clear();
    };
    for ch in block.chars() {
        match ch {
            '(' => {
                depth += 1;
                if started {
                    cur.push(ch)
                }
            }
            ')' => {
                depth -= 1;
                if started {
                    cur.push(ch)
                }
            }
            '\\' if depth == 0 => {
                flush(&mut cur, &mut out);
                started = true;
            }
            _ => {
                if started {
                    cur.push(ch)
                }
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syllable {
    pub start: Ms,
    pub duration: Ms,
    pub kind: String,
    pub text: String,
    pub tags: String,
}

/// Parse karaoke syllables; times are relative to line start. Durations in `\k` are centiseconds.
pub fn karaoke_syllables(text: &str) -> Vec<Syllable> {
    let mut out: Vec<Syllable> = Vec::new();
    let mut time: Ms = 0;
    let mut pending: Option<Syllable> = None;
    for block in split_blocks(text) {
        match block {
            Block::Tags(t) => {
                let tags = parse_tags(&t);
                let k = tags
                    .iter()
                    .filter(|(n, _)| matches!(n.as_str(), "k" | "K" | "kf" | "ko"))
                    .last()
                    .cloned();
                if let Some((name, arg)) = k {
                    if let Some(p) = pending.take() {
                        out.push(p);
                    }
                    let dur = arg.trim().parse::<f64>().unwrap_or(0.0) * 10.0;
                    let dur = dur as Ms;
                    pending = Some(Syllable {
                        start: time,
                        duration: dur,
                        kind: if name == "K" { "kf".into() } else { name },
                        text: String::new(),
                        tags: t,
                    });
                    time += dur;
                } else if let Some(p) = pending.as_mut() {
                    p.tags.push_str(&format!("\\{}", t.trim_start_matches('\\')));
                }
            }
            Block::Text(s) => {
                if let Some(p) = pending.as_mut() {
                    p.text.push_str(&s)
                } else if !s.is_empty() {
                    pending = Some(Syllable {
                        start: 0,
                        duration: 0,
                        kind: "k".into(),
                        text: s,
                        tags: String::new(),
                    });
                }
            }
        }
    }
    if let Some(p) = pending {
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks() {
        let b = split_blocks("a{\\i1}b{c}d");
        assert_eq!(b.len(), 5);
        assert_eq!(strip_tags("a{\\i1}b{c}d"), "abd");
        assert_eq!(join_blocks(&b), "a{\\i1}b{c}d");
        assert_eq!(plain_text("x\\Ny{\\b1}z"), "x\nyz");
    }
    #[test]
    fn tags() {
        let t = parse_tags("\\fs20\\t(0,100,\\fs30\\1c&HFF&)\\fscx90\\1c&H00FF00&");
        assert_eq!(t[0], ("fs".into(), "20".into()));
        assert_eq!(t[1].0, "t");
        assert_eq!(t[2], ("fscx".into(), "90".into()));
        assert_eq!(t[3], ("1c".into(), "&H00FF00&".into()));
    }
    #[test]
    fn karaoke() {
        let s = karaoke_syllables("{\\k20}ko{\\kf30\\b1}ra{\\k10}ke");
        assert_eq!(s.len(), 3);
        assert_eq!((s[1].start, s[1].duration, s[1].kind.as_str()), (200, 300, "kf"));
        assert_eq!(s[2].text, "ke");
    }
}
