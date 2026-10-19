//! Turkish text helpers: locale-correct casing and mojibake repair.

pub fn upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

pub fn lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            c => c.to_lowercase().collect(),
        })
        .collect()
}

/// Capitalize first letter of each word, Turkish-aware.
pub fn title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if start {
                out.push_str(&upper(&c.to_string()));
            } else {
                out.push_str(&lower(&c.to_string()));
            }
            start = false;
        } else {
            out.push(c);
            start = !(c == '\'' || c == '’');
        }
    }
    out
}

const MOJIBAKE_MARKERS: &[&str] = &[
    "Ã§", "Ã¼", "Ã¶", "Ä±", "ÅŸ", "ÄŸ", "Ä°", "Ã‡", "Ãœ", "Ã–", "Åž", "Ä\u{9e}", "Å\u{9f}", "Å\u{178}", "Ä\u{b1}", "â€™", "â€œ", "â€¦",
];

pub fn looks_like_mojibake(s: &str) -> bool {
    MOJIBAKE_MARKERS.iter().any(|m| s.contains(m))
}

/// Repair UTF-8 text that was decoded as windows-1252/1254 and re-saved.
/// Returns None if nothing to fix or the result isn't clean.
pub fn fix_mojibake(s: &str) -> Option<String> {
    if !looks_like_mojibake(s) {
        return None;
    }
    let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(s);
    let st = std::str::from_utf8(&bytes).ok()?;
    if st.contains('\u{FFFD}') {
        return None;
    }
    Some(st.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn casing() {
        assert_eq!(upper("istanbul ışık"), "İSTANBUL IŞIK");
        assert_eq!(lower("İSTANBUL IŞIK"), "istanbul ışık");
        assert_eq!(title("ankara'nın iNCİ"), "Ankara'nın İnci");
    }
    #[test]
    fn mojibake() {
        let broken = "Ã§ok gÃ¼zel";
        assert_eq!(fix_mojibake(broken).as_deref(), Some("çok güzel"));
        assert!(fix_mojibake("çok güzel").is_none());
    }
}
