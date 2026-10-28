//! Turkish-aware case mapping (dotted/dotless i) and common fansub text fixes.

pub fn upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            _ => c.to_uppercase().collect(),
        })
        .collect()
}

pub fn lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            _ => c.to_lowercase().collect(),
        })
        .collect()
}

/// Uppercase only the first letter of each sentence/word start.
pub fn title(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if start && c.is_alphabetic() {
            out.push_str(&upper(&c.to_string()));
            start = false;
        } else {
            out.push_str(&lower(&c.to_string()));
            if !c.is_alphanumeric() && c != '\'' && c != '’' {
                start = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn case() {
        assert_eq!(upper("ışık iğne"), "IŞIK İĞNE");
        assert_eq!(lower("IŞIK İĞNE"), "ışık iğne");
        assert_eq!(title("İSTANBUL ışıkları"), "İstanbul Işıkları");
    }
}
