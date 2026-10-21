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
