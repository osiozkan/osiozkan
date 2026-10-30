//! Geçersiz kılma (override) etiketi yardımcıları.

#[derive(Debug, Clone, PartialEq)]
pub enum Block<'a> {
    Tags(&'a str),
    Text(&'a str),
}

/// Metni `{...}` blokları ve düz metin olarak ayırır. Kapanmamış `{` düz metin sayılır.
pub fn split_blocks(text: &str) -> Vec<Block<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if let Some(close_rel) = rest[open..].find('}') {
                if open > 0 {
                    out.push(Block::Text(&rest[..open]));
                }
                out.push(Block::Tags(&rest[open + 1..open + close_rel]));
                rest = &rest[open + close_rel + 1..];
                continue;
            }
        }
        out.push(Block::Text(rest));
        break;
    }
    out
}

/// Etiketleri atar; `\N`/`\n` yeni satıra, `\h` boşluğa dönüşür.
pub fn visible_text(text: &str) -> String {
    let mut s = String::new();
    for b in split_blocks(text) {
        if let Block::Text(t) = b {
            s.push_str(t);
        }
    }
    s.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Okunabilir karakter sayısı (satır sonları ve boşluk dahil değil: CPS standardı).
pub fn char_count(text: &str) -> usize {
    visible_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

pub fn balanced_braces(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if !(0..=1).contains(&depth) {
            return false;
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips() {
        assert_eq!(visible_text("{\\an8}Selam{\\i1} dünya\\Nİki"), "Selam dünya\nİki");
        assert_eq!(char_count("{\\b1}a b{\\b0}"), 2);
        assert!(balanced_braces("{\\i1}x"));
        assert!(!balanced_braces("{\\i1x"));
        assert!(!balanced_braces("{{a}}"));
    }
}
