//! Font usage analysis and collection (what Aegisub's Fonts Collector does).

use crate::ass::Document;
use crate::fonts::FontDb;
use crate::tags::{split_blocks, Seg};
use regex::Regex;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontKey {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

/// All (family, bold, italic) combinations used by styles referenced from events and by \fn \b \i \r tags,
/// with the number of events using each.
pub fn fonts_used(doc: &Document) -> BTreeMap<FontKey, usize> {
    let tag = Regex::new(r"\\(fn|b|i|r)([^\\}()]*)").unwrap();
    let mut out: BTreeMap<FontKey, usize> = BTreeMap::new();
    let style_key = |name: &str| -> FontKey {
        let s = doc.style(name).or_else(|| doc.style("Default")).or_else(|| doc.styles().next());
        match s {
            Some(s) => FontKey { family: s.fontname.clone(), bold: s.bold, italic: s.italic },
            None => FontKey { family: "Arial".into(), bold: false, italic: false },
        }
    };
    for e in doc.events().filter(|e| !e.comment) {
        let base = style_key(&e.style);
        let mut cur = base.clone();
        let mut seen: Vec<FontKey> = vec![];
        let has_text_with = |k: &FontKey, seen: &mut Vec<FontKey>| {
            if !seen.contains(k) {
                seen.push(k.clone());
            }
        };
        for seg in split_blocks(&e.text) {
            match seg {
                Seg::Tags(t) => {
                    for c in tag.captures_iter(t) {
                        let v = c[2].trim();
                        match &c[1] {
                            "fn" => {
                                if !v.is_empty() {
                                    cur.family = v.to_string()
                                }
                            }
                            "b" => cur.bold = matches!(v, "1") || v.parse::<i32>().map(|w| w >= 600).unwrap_or(false),
                            "i" => cur.italic = v == "1",
                            "r" => cur = if v.is_empty() { base.clone() } else { style_key(v) },
                            _ => {}
                        }
                    }
                }
                Seg::Text(t) => {
                    if !t.trim().is_empty() {
                        has_text_with(&cur, &mut seen);
                    }
                }
            }
        }
        if seen.is_empty() {
            seen.push(base);
        }
        for k in seen {
            *out.entry(k).or_insert(0) += 1;
        }
    }
    out
}

#[derive(Debug)]
pub struct Resolution {
    pub key: FontKey,
    pub uses: usize,
    pub path: Option<PathBuf>,
}

pub fn resolve(doc: &Document, db: &mut FontDb) -> Vec<Resolution> {
    fonts_used(doc)
        .into_iter()
        .map(|(key, uses)| {
            let path = db.lookup(&key.family, key.bold, key.italic).map(|e| e.path);
            Resolution { key, uses, path }
        })
        .collect()
}

/// Copy found font files into `dir`; returns (copied files, missing keys).
pub fn collect(doc: &Document, db: &mut FontDb, dir: &std::path::Path) -> std::io::Result<(Vec<PathBuf>, Vec<FontKey>)> {
    std::fs::create_dir_all(dir)?;
    let mut copied: Vec<PathBuf> = vec![];
    let mut missing = vec![];
    for r in resolve(doc, db) {
        match r.path {
            Some(p) => {
                if !copied.contains(&p) {
                    let dest = dir.join(p.file_name().unwrap());
                    std::fs::copy(&p, &dest)?;
                    copied.push(p);
                }
            }
            None => missing.push(r.key),
        }
    }
    Ok((copied, missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::{Entry, Event, Style};

    #[test]
    fn glyph_check() {
        let mut db = FontDb::new();
        if db.lookup("DejaVu Sans", false, false).is_none() {
            return;
        }
        let mut d = Document::new_default();
        d.entries.push(Entry::Style(Style { name: "Dj".into(), fontname: "DejaVu Sans".into(), ..Style::default() }));
        d.entries.push(Entry::Style(Style { name: "Nope".into(), fontname: "Surely Not Installed Font".into(), ..Style::default() }));
        d.entries.push(Entry::Event(Event { style: "Dj".into(), text: "Şişli ğüöçİı \\N日本".into(), ..Event::default() }));
        d.entries.push(Entry::Event(Event { style: "Nope".into(), text: "x".into(), ..Event::default() }));
        let iss = glyph_issues(&d, &mut db);
        let jp = iss.iter().find(|i| i.family == "DejaVu Sans").expect("CJK is missing in DejaVu");
        assert!(jp.missing.contains(&'日') && !jp.missing.contains(&'Ş'), "{:?}", jp);
        assert!(iss.iter().any(|i| i.family == "Surely Not Installed Font" && !i.font_found));
    }

    #[test]
    fn finds_fonts_from_styles_and_tags() {
        let mut d = Document::new_default();
        d.entries.push(Entry::Style(Style { name: "Alt".into(), fontname: "Impact".into(), bold: true, ..Style::default() }));
        for t in ["plain", "{\\fnComic Sans MS\\i1}fancy{\\r}back", "{\\rAlt}alt", "{\\b0}x"] {
            d.entries.push(Entry::Event(Event { text: t.into(), ..Event::default() }));
        }
        let u = fonts_used(&d);
        let get = |f: &str, b: bool, i: bool| u.get(&FontKey { family: f.into(), bold: b, italic: i }).copied().unwrap_or(0);
        assert_eq!(get("Arial", false, false), 3); // plain, back(after \r), x(b0)
        assert_eq!(get("Comic Sans MS", false, true), 1);
        assert_eq!(get("Impact", true, false), 1);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphIssue {
    /// 1-based event index
    pub event: usize,
    pub family: String,
    pub missing: Vec<char>,
    pub font_found: bool,
}

/// Characters that the effective font of an event cannot render (tofu boxes in the final video).
/// Tracks \fn/\r/\b/\i like `fonts_used`. Fonts that are not installed are reported once per event
/// with `font_found = false` (nothing to check against).
pub fn glyph_issues(doc: &Document, db: &mut FontDb) -> Vec<GlyphIssue> {
    let tag = Regex::new(r"\\(fn|b|i|r)([^\\}()]*)").unwrap();
    let mut out = Vec::new();
    let mut cache: BTreeMap<(FontKey, char), bool> = BTreeMap::new();
    let style_key = |name: &str| -> FontKey {
        let s = doc.style(name).or_else(|| doc.style("Default")).or_else(|| doc.styles().next());
        match s {
            Some(s) => FontKey { family: s.fontname.clone(), bold: s.bold, italic: s.italic },
            None => FontKey { family: "Arial".into(), bold: false, italic: false },
        }
    };
    for (idx, e) in doc.events().enumerate().filter(|(_, e)| !e.comment) {
        if e.text.contains("\\p1") {
            continue;
        }
        let base = style_key(&e.style);
        let mut cur = base.clone();
        let mut missing: BTreeMap<String, Vec<char>> = BTreeMap::new();
        let mut not_found: Vec<String> = vec![];
        for seg in split_blocks(&e.text) {
            match seg {
                Seg::Tags(t) => {
                    for c in tag.captures_iter(t) {
                        let v = c[2].trim();
                        match &c[1] {
                            "fn" if !v.is_empty() => cur.family = v.to_string(),
                            "b" => cur.bold = v == "1" || v.parse::<i32>().map(|w| w >= 600).unwrap_or(false),
                            "i" => cur.italic = v == "1",
                            "r" => cur = if v.is_empty() { base.clone() } else { style_key(v) },
                            _ => {}
                        }
                    }
                }
                Seg::Text(t) => {
                    for ch in t.replace("\\N", "").replace("\\n", "").replace("\\h", "").chars() {
                        if ch.is_whitespace() || ch.is_control() {
                            continue;
                        }
                        let key = (cur.clone(), ch);
                        let has = match cache.get(&key) {
                            Some(h) => *h,
                            None => {
                                let h = db.has_glyph(&cur.family, cur.bold, cur.italic, ch);
                                cache.insert(key, h.unwrap_or(true));
                                h.unwrap_or(true)
                            }
                        };
                        if db.lookup(&cur.family, cur.bold, cur.italic).is_none() {
                            if !not_found.contains(&cur.family) {
                                not_found.push(cur.family.clone());
                            }
                        } else if !has {
                            let v = missing.entry(cur.family.clone()).or_default();
                            if !v.contains(&ch) {
                                v.push(ch);
                            }
                        }
                    }
                }
            }
        }
        for (family, chars) in missing {
            out.push(GlyphIssue { event: idx + 1, family, missing: chars, font_found: true });
        }
        for family in not_found {
            out.push(GlyphIssue { event: idx + 1, family, missing: vec![], font_found: false });
        }
    }
    out
}
