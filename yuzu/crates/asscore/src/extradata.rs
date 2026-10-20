//! Aegisub extradata: per-line key/value data kept in `[Aegisub Extradata]` and referenced from
//! the start of a line's text as `{=1=2}`. Scripts see it as `line.extra` (ASSFoundation, a-mo,
//! the perspective tools and others store data there).
//!
//! Values are written as `e<escaped>` (`#XX` for control characters, `#`, `,`, `:` and `|`) or
//! `u<uuencoded>` when that is shorter, as Aegisub does.

use crate::model::{Extra, Script};

/// Extradata ids at the start of a text and the text after them.
pub fn split_marker(text: &str) -> (Vec<u32>, &str) {
    let Some(rest) = text.strip_prefix("{=") else { return (vec![], text) };
    let Some(close) = rest.find('}') else { return (vec![], text) };
    let inner = &rest[..close];
    let ids: Option<Vec<u32>> = inner.split('=').map(|x| x.parse().ok()).collect();
    match ids { Some(v) if !v.is_empty() => (v, &rest[close + 1..]), _ => (vec![], text) }
}

/// `{=1=2}text`, or the text as is without ids.
pub fn with_marker(ids: &[u32], text: &str) -> String {
    if ids.is_empty() { return text.to_string(); }
    format!("{{={}}}{}", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("="), text)
}

pub fn inline_encode(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if (c as u32) < 0x20 || matches!(c, '#' | ',' | ':' | '|') { o.push_str(&format!("#{:02X}", c as u32)); } else { o.push(c); }
    }
    o
}

pub fn inline_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' && i + 3 <= b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Aegisub's uuencoding: 3 bytes → 4 characters of 6 bits + 33.
pub fn uuencode(data: &[u8]) -> String {
    let mut o = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let s = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let d = [s[0] >> 2, ((s[0] & 0x3) << 4) | (s[1] >> 4), ((s[1] & 0xF) << 2) | (s[2] >> 6), s[2] & 0x3F];
        for v in d.iter().take(chunk.len() + 1) { o.push((v + 33) as char); }
    }
    o
}

pub fn uudecode(s: &str) -> Vec<u8> {
    let src: Vec<u8> = s.bytes().filter(|&c| c != b'\n' && c != b'\r' && c != 0).map(|c| c.wrapping_sub(33)).collect();
    let mut out = Vec::with_capacity(src.len() * 3 / 4);
    for g in src.chunks(4) {
        let at = |k: usize| *g.get(k).unwrap_or(&0);
        if g.len() > 1 { out.push((at(0) << 2) | (at(1) >> 4)); }
        if g.len() > 2 { out.push(((at(1) & 0xF) << 4) | (at(2) >> 2)); }
        if g.len() > 3 { out.push(((at(2) & 0x3) << 6) | at(3)); }
    }
    out
}

/// A stored value as text (`e…` escaped, `u…` uuencoded).
pub fn decode_value(v: &str) -> String {
    match v.as_bytes().first() {
        Some(b'e') => inline_decode(&v[1..]),
        Some(b'u') => String::from_utf8_lossy(&uudecode(&v[1..])).into_owned(),
        _ => v.to_string(),
    }
}

/// A value as Aegisub writes it: escaped, or uuencoded when escaping grows it a lot.
pub fn encode_value(v: &str) -> String {
    let e = inline_encode(v);
    if 4 * v.len() < 3 * e.len() { format!("u{}", uuencode(v.as_bytes())) } else { format!("e{}", e) }
}

/// The decoded (key, value) pairs a line refers to.
pub fn line_values(extra: &[Extra], text: &str) -> Vec<(String, String)> {
    split_marker(text).0.iter().filter_map(|id| extra.iter().find(|x| x.id == *id)).map(|x| (inline_decode(&x.key), decode_value(&x.value))).collect()
}

/// The id of an entry with this key and value, added when there is none.
pub fn intern(extra: &mut Vec<Extra>, key: &str, value: &str) -> u32 {
    if let Some(x) = extra.iter().find(|x| inline_decode(&x.key) == key && decode_value(&x.value) == value) { return x.id; }
    let id = extra.iter().map(|x| x.id + 1).max().unwrap_or(1);
    extra.push(Extra { id, key: inline_encode(key), value: encode_value(value) });
    id
}

/// Drop entries no line refers to (Aegisub does this on save).
pub fn prune(sc: &mut Script) {
    let used: std::collections::HashSet<u32> = sc.events.iter().flat_map(|e| split_marker(&e.text).0).collect();
    sc.extradata.retain(|x| used.contains(&x.id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers() {
        assert_eq!(split_marker("{=3=12}Hello"), (vec![3, 12], "Hello"));
        assert_eq!(split_marker("{=}x"), (vec![], "{=}x"));
        assert_eq!(split_marker("{\\i1}x"), (vec![], "{\\i1}x"));
        assert_eq!(with_marker(&[1, 2], "a"), "{=1=2}a");
        assert_eq!(with_marker(&[], "a"), "a");
    }

    #[test]
    fn encodings_round_trip() {
        let v = "x: 1, y|2 #3\n";
        assert_eq!(inline_encode(v), "x#3A 1#2C y#7C2 #233#0A");
        assert_eq!(inline_decode(&inline_encode(v)), v);
        for s in ["", "a", "ab", "abc", "abcd", "çğış — 柚子", "\u{1}\u{2}\u{3}\u{4}"] {
            assert_eq!(String::from_utf8(uudecode(&uuencode(s.as_bytes()))).unwrap(), s);
            assert_eq!(decode_value(&encode_value(s)), s);
        }
        // many special characters: uuencoded like Aegisub
        assert!(encode_value("\u{1}\u{2}\u{3}\u{4}\u{5}").starts_with('u'));
        assert!(encode_value("plain text").starts_with('e'));
    }

    #[test]
    fn values_of_a_line_and_interning() {
        let mut ex = vec![Extra { id: 1, key: "a-mo".into(), value: "e{\"x\"#3A1}".into() }];
        assert_eq!(line_values(&ex, "{=1}txt"), vec![("a-mo".to_string(), "{\"x\":1}".to_string())]);
        assert_eq!(intern(&mut ex, "a-mo", "{\"x\":1}"), 1);
        assert_eq!(intern(&mut ex, "k", "v"), 2);
        let mut sc = Script::new();
        sc.extradata = ex;
        sc.events.push(crate::Event::new(0, 1, "Default", "{=2}x"));
        prune(&mut sc);
        assert_eq!(sc.extradata.len(), 1);
        assert_eq!(sc.extradata[0].id, 2);
    }
}
