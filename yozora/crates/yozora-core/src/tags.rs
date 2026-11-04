//! Override etiketi (`{\...}`) ayrıştırma ve metin yardımcıları.

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Text(String),
    /// `{...}` içeriği; `Tag` listesi + ayrıştırılamayan düz yorum parçaları.
    Override(Vec<Tag>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// Etiket adı, ters eğik çizgi olmadan (`fscx`, `1c`, `t`...). Yorum parçası için boş.
    pub name: String,
    pub arg: String,
}

const NAMES: &[&str] = &[
    "fscx", "fscy", "fsp", "fs", "fn", "fade", "fad", "frx", "fry", "frz", "fr", "fax", "fay", "fe", "blur", "be",
    "bord", "xbord", "ybord", "shad", "xshad", "yshad", "iclip", "clip", "move", "pos", "org", "an", "alpha", "a",
    "1a", "2a", "3a", "4a", "1c", "2c", "3c", "4c", "c", "kf", "ko", "kt", "K", "k", "q", "r", "t", "i", "b", "u",
    "s", "p",
];

/// Metni düz metin ve override bloklarına böler.
pub fn parse_blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if let Some(close_rel) = rest[open..].find('}') {
                if open > 0 {
                    out.push(Block::Text(rest[..open].to_string()));
                }
                out.push(Block::Override(parse_tags(&rest[open + 1..open + close_rel])));
                rest = &rest[open + close_rel + 1..];
                continue;
            }
        }
        out.push(Block::Text(rest.to_string()));
        break;
    }
    out
}

pub fn parse_tags(inner: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    let b = inner.as_bytes();
    let mut i = 0;
    let mut comment_start = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            if comment_start < i {
                tags.push(Tag { name: String::new(), arg: inner[comment_start..i].to_string() });
            }
            let rest = &inner[i + 1..];
            let name = NAMES.iter().find(|n| rest.starts_with(**n)).copied();
            let Some(name) = name else {
                // Bilinmeyen etiket: bir sonraki '\' ya da sona kadar yorum say.
                let end = rest.find('\\').map(|p| i + 1 + p).unwrap_or(inner.len());
                tags.push(Tag { name: String::new(), arg: inner[i..end].to_string() });
                i = end;
                comment_start = i;
                continue;
            };
            let arg_start = i + 1 + name.len();
            let mut j = arg_start;
            let mut depth = 0i32;
            while j < b.len() {
                match b[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    b'\\' if depth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            tags.push(Tag { name: name.to_string(), arg: inner[arg_start..j].to_string() });
            i = j;
            comment_start = j;
        } else {
            i += 1;
        }
    }
    if comment_start < inner.len() {
        tags.push(Tag { name: String::new(), arg: inner[comment_start..].to_string() });
    }
    tags
}

pub fn blocks_to_string(blocks: &[Block]) -> String {
    let mut s = String::new();
    for b in blocks {
        match b {
            Block::Text(t) => s.push_str(t),
            Block::Override(tags) => {
                s.push('{');
                for t in tags {
                    if t.name.is_empty() {
                        s.push_str(&t.arg);
                    } else {
                        s.push('\\');
                        s.push_str(&t.name);
                        s.push_str(&t.arg);
                    }
                }
                s.push('}');
            }
        }
    }
    s
}

/// Etiketleri atar. `\N`, `\n`, `\h` korunur (bkz. [`visible_text`] farklı davranır).
pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in parse_blocks(text) {
        if let Block::Text(t) = b {
            out.push_str(&t);
        }
    }
    out
}

/// Ekranda görünen metin: etiketler atılır, çizim (`\p1`) blokları atlanır,
/// `\N`/`\n` yeni satıra, `\h` boşluğa çevrilir.
pub fn visible_text(text: &str) -> String {
    let mut out = String::new();
    let mut drawing = false;
    for b in parse_blocks(text) {
        match b {
            Block::Override(tags) => {
                for t in tags {
                    if t.name == "p" {
                        drawing = t.arg.trim().parse::<f64>().map(|v| v > 0.0).unwrap_or(false);
                    }
                }
            }
            Block::Text(t) => {
                if !drawing {
                    out.push_str(&t);
                }
            }
        }
    }
    out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Okunabilirlik hesabı için karakter sayısı (boşluk ve satır sonları hariç).
pub fn char_count(text: &str) -> usize {
    visible_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

/// Her görünür satırın karakter uzunluğu.
pub fn line_lengths(text: &str) -> Vec<usize> {
    visible_text(text).split('\n').map(|l| l.trim().chars().count()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks() {
        let b = parse_blocks("{\\an8\\t(0,100,\\fscx120)\\c&HFF&}Hello{\\i1} world");
        assert_eq!(b.len(), 4);
        if let Block::Override(t) = &b[0] {
            assert_eq!(t[0].name, "an");
            assert_eq!(t[1].name, "t");
            assert_eq!(t[1].arg, "(0,100,\\fscx120)");
            assert_eq!(t[2].name, "c");
        } else {
            panic!()
        }
        assert_eq!(blocks_to_string(&b), "{\\an8\\t(0,100,\\fscx120)\\c&HFF&}Hello{\\i1} world");
    }
    #[test]
    fn visible() {
        assert_eq!(visible_text("{\\i1}A\\NB\\hC{\\i0}"), "A\nB C");
        assert_eq!(visible_text("{\\p1}m 0 0 l 10 10{\\p0}x"), "x");
        assert_eq!(char_count("{\\b1}ab c\\Nd"), 4);
        assert_eq!(line_lengths("abc\\Nde"), vec![3, 2]);
    }
}
