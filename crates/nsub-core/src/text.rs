//! Text utilities, with Turkish-aware casing (dotted/dotless i).

pub fn upper_tr(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

pub fn lower_tr(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            c => c.to_lowercase().collect(),
        })
        .collect()
}

/// Sentence case: first letter of each sentence upper-case, rest lower-case. Only touches visible text, not tags.
pub fn sentence_case_tr(s: &str) -> String {
    use crate::tags::{parse, serialize, Segment};
    let mut start = true;
    let mut segs = parse(s);
    for seg in &mut segs {
        if let Segment::Text(t) = seg {
            let mut out = String::new();
            let mut chars = t.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' && matches!(chars.peek(), Some('N') | Some('n') | Some('h')) {
                    out.push(c);
                    out.push(chars.next().unwrap());
                    continue;
                }
                if c.is_alphabetic() {
                    if start {
                        out.push_str(&upper_tr(&c.to_string()));
                        start = false;
                    } else {
                        out.push_str(&lower_tr(&c.to_string()));
                    }
                } else {
                    if matches!(c, '.' | '!' | '?' | '…') {
                        start = true;
                    }
                    out.push(c);
                }
            }
            *t = out;
        }
    }
    serialize(&segs)
}

/// Number of characters that count towards reading speed (visible text minus line breaks/spaces).
pub fn reading_chars(s: &str) -> usize {
    crate::tags::visible_text(s).chars().filter(|c| !c.is_whitespace()).count()
}

/// Length of the longest visible line.
pub fn longest_line(s: &str) -> usize {
    crate::tags::visible_text(s).lines().map(|l| l.chars().count()).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turkish_case() {
        assert_eq!(upper_tr("işık ılık"), "İŞIK ILIK");
        assert_eq!(lower_tr("İSTANBUL IŞIK"), "istanbul ışık");
        assert_eq!(sentence_case_tr("{\\i1}İYİ MİSİN? EVET.\\NIŞIK"), "{\\i1}İyi misin? Evet.\\NIşık");
    }
}
