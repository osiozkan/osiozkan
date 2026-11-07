//! Turkish-aware text helpers (dotted/dotless I) plus common typography fixes.

pub fn to_upper(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        'i' => vec!['İ'],
        'ı' => vec!['I'],
        c => c.to_uppercase().collect(),
    }).collect()
}

pub fn to_lower(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        'I' => vec!['ı'],
        'İ' => vec!['i'],
        c => c.to_lowercase().collect(),
    }).collect()
}

/// Capitalise first letter of each word, Turkish rules.
pub fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if start { out.extend(to_upper(&c.to_string()).chars()); } else { out.extend(to_lower(&c.to_string()).chars()); }
            start = false;
        } else {
            out.push(c);
            start = !(c == '\'' || c == '’');
        }
    }
    out
}

/// Sentence case: first alphabetic char after `. ! ? ` or at start is upper.
pub fn sentence_case(s: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if cap { out.push_str(&to_upper(&c.to_string())); } else { out.push_str(&to_lower(&c.to_string())); }
            cap = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?' | '…') { cap = true; }
        }
    }
    out
}

/// Typography cleanup suited to Turkish subtitles. Operates on plain text only.
pub fn fix_typography(s: &str) -> String {
    let mut t = s.to_string();
    while t.contains("  ") { t = t.replace("  ", " "); }
    t = t.replace("...", "…");
    for p in [" ,", " .", " !", " ?", " ;", " :", " …"] {
        t = t.replace(p, &p[1..]);
    }
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn case() {
        assert_eq!(to_upper("içki ışık"), "İÇKİ IŞIK");
        assert_eq!(to_lower("IŞIK İNCE"), "ışık ince");
        assert_eq!(title_case("ıSPARTA'nın iSİMLERİ"), "Isparta'nın İsimleri");
        assert_eq!(sentence_case("MERHABA. nasılsın"), "Merhaba. Nasılsın");
    }
    #[test]
    fn typo() { assert_eq!(fix_typography("a  b ,c..."), "a b,c…"); }
}
