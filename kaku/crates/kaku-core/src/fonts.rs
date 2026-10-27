//! Real text metrics from installed fonts (used by `aegisub.text_extents` and layout tools).
//!
//! ASS font sizes denote the font *cell height* (ascent + descent), not the em size, so the
//! scale factor is `fontsize / (ascent + descent)` – the same convention as VSFilter/libass.

use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extents {
    pub width: f64,
    pub height: f64,
    pub descent: f64,
    pub extlead: f64,
}

pub struct FontBook {
    db: Database,
    cache: HashMap<(String, bool, bool), Option<fontdb::ID>>,
}

impl FontBook {
    pub fn system() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        FontBook { db, cache: HashMap::new() }
    }
    pub fn add_dir(&mut self, dir: &std::path::Path) {
        self.db.load_fonts_dir(dir);
        self.cache.clear();
    }
    pub fn has_family(&self, name: &str) -> bool {
        self.db.faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
    }
    fn face(&mut self, family: &str, bold: bool, italic: bool) -> Option<fontdb::ID> {
        let key = (family.trim_start_matches('@').to_string(), bold, italic);
        if let Some(v) = self.cache.get(&key) {
            return *v;
        }
        let fam = [Family::Name(&key.0)];
        let q = Query {
            families: &fam,
            weight: if bold { Weight::BOLD } else { Weight::NORMAL },
            stretch: Stretch::Normal,
            style: if italic { Style::Italic } else { Style::Normal },
        };
        // Only accept an exact family match; otherwise caller falls back to heuristics.
        let id = if self.has_family(&key.0) { self.db.query(&q) } else { None };
        self.cache.insert(key, id);
        id
    }

    /// File path of the best matching face (None for embedded/binary sources or missing families).
    pub fn face_path(&mut self, family: &str, bold: bool, italic: bool) -> Option<std::path::PathBuf> {
        let id = self.face(family, bold, italic)?;
        match &self.db.face(id)?.source {
            fontdb::Source::File(p) => Some(p.clone()),
            fontdb::Source::SharedFile(p, _) => Some(p.clone()),
            _ => None,
        }
    }

    /// Characters of `chars` that the matching face has no glyph for.
    pub fn missing_glyphs(&mut self, family: &str, bold: bool, italic: bool, chars: &std::collections::BTreeSet<char>) -> Option<Vec<char>> {
        let id = self.face(family, bold, italic)?;
        self.db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            Some(chars.iter().copied().filter(|c| !c.is_control() && !c.is_whitespace() && face.glyph_index(*c).is_none()).collect::<Vec<_>>())
        })?
    }

    /// Returns `None` when the family is not installed.
    pub fn measure(&mut self, family: &str, bold: bool, italic: bool, size: f64, scale_x: f64, scale_y: f64, spacing: f64, text: &str) -> Option<Extents> {
        let id = self.face(family, bold, italic)?;
        self.db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            let asc = face.ascender() as f64;
            let desc = -(face.descender() as f64);
            let cell = (asc + desc).max(1.0);
            let k = size / cell;
            let mut w = 0.0;
            for ch in text.chars() {
                let adv = face.glyph_index(ch).and_then(|g| face.glyph_hor_advance(g)).unwrap_or((face.units_per_em() / 2) as u16) as f64;
                w += adv * k + spacing;
            }
            Some(Extents { width: w * scale_x / 100.0, height: size * scale_y / 100.0, descent: desc * k * scale_y / 100.0, extlead: face.line_gap() as f64 * k * scale_y / 100.0 })
        })?
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontUse {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub chars: std::collections::BTreeSet<char>,
    pub lines: usize,
}

fn is_bold_arg(a: &str) -> bool {
    match a.trim().parse::<i32>() {
        Ok(v) => v == 1 || v >= 600,
        Err(_) => false,
    }
}

/// Every (family, bold, italic) combination used by styles and `\fn`/`\b`/`\i`/`\r` overrides, with the characters rendered in it.
pub fn used_fonts(script: &crate::ass::Script) -> Vec<FontUse> {
    use crate::tags::{self, Seg};
    use std::collections::BTreeMap;
    let styles: BTreeMap<&str, &crate::ass::Style> = script.styles().map(|s| (s.name.as_str(), s)).collect();
    let mut map: BTreeMap<(String, bool, bool), FontUse> = BTreeMap::new();
    let mut touch = |fam: &str, b: bool, i: bool, text: Option<&str>, line_first: bool| {
        let e = map.entry((fam.to_string(), b, i)).or_insert_with(|| FontUse { family: fam.to_string(), bold: b, italic: i, chars: Default::default(), lines: 0 });
        if line_first {
            e.lines += 1;
        }
        if let Some(t) = text {
            e.chars.extend(t.chars());
        }
    };
    for st in script.styles() {
        touch(&st.fontname, st.bold, st.italic, None, false);
    }
    for l in script.lines() {
        if l.is_comment() {
            continue;
        }
        let Some(base) = styles.get(l.style.as_str()).or_else(|| styles.get("Default")) else { continue };
        let (mut fam, mut b, mut i) = (base.fontname.clone(), base.bold, base.italic);
        let mut drawing = false;
        let mut seen: std::collections::HashSet<(String, bool, bool)> = Default::default();
        for seg in tags::parse(&l.text) {
            match seg {
                Seg::Block(ts) => {
                    for t in ts {
                        match t.name.as_str() {
                            "fn" => fam = t.arg.trim().to_string(),
                            "b" => b = is_bold_arg(&t.arg) || (t.arg.trim().is_empty() && base.bold),
                            "i" => i = t.arg.trim() == "1" || (t.arg.trim().is_empty() && base.italic),
                            "r" => {
                                let target = if t.arg.trim().is_empty() { Some(*base) } else { styles.get(t.arg.trim()).copied() };
                                if let Some(st) = target {
                                    fam = st.fontname.clone();
                                    b = st.bold;
                                    i = st.italic;
                                }
                            }
                            "p" => drawing = t.arg.trim().parse::<i32>().map(|v| v > 0).unwrap_or(false),
                            _ => {}
                        }
                    }
                }
                Seg::Text(t) if !drawing => {
                    let vis = t.replace("\\N", "").replace("\\n", "").replace("\\h", " ");
                    let first = seen.insert((fam.clone(), b, i));
                    touch(&fam.trim_start_matches('@').to_string(), b, i, Some(&vis), first);
                }
                _ => {}
            }
        }
    }
    map.into_values().collect()
}

#[derive(Debug, Clone)]
pub struct FontStatus {
    pub used: FontUse,
    pub path: Option<std::path::PathBuf>,
    pub installed: bool,
    pub missing_glyphs: Vec<char>,
}

pub fn check_fonts(script: &crate::ass::Script) -> Vec<FontStatus> {
    used_fonts(script)
        .into_iter()
        .map(|u| {
            BOOK.with(|b| {
                let mut b = b.borrow_mut();
                let book = b.get_or_insert_with(FontBook::system);
                let installed = book.has_family(u.family.trim_start_matches('@'));
                let path = book.face_path(&u.family, u.bold, u.italic);
                let missing = if installed { book.missing_glyphs(&u.family, u.bold, u.italic, &u.chars).unwrap_or_default() } else { vec![] };
                FontStatus { used: u, path, installed, missing_glyphs: missing }
            })
        })
        .collect()
}

/// Copy the font files used by `script` into `dest`. Returns (copied file names, families not found).
pub fn collect_fonts(script: &crate::ass::Script, dest: &std::path::Path) -> std::io::Result<(Vec<String>, Vec<String>)> {
    std::fs::create_dir_all(dest)?;
    let mut copied: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for st in check_fonts(script) {
        match st.path {
            Some(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if !copied.contains(&name) {
                    std::fs::copy(&p, dest.join(&name))?;
                    copied.push(name);
                }
            }
            None => {
                if !missing.contains(&st.used.family) {
                    missing.push(st.used.family.clone());
                }
            }
        }
    }
    Ok((copied, missing))
}

thread_local! {
    static BOOK: RefCell<Option<FontBook>> = const { RefCell::new(None) };
}

/// Measure using the process-wide (lazily loaded) system font book.
pub fn measure(family: &str, bold: bool, italic: bool, size: f64, sx: f64, sy: f64, spacing: f64, text: &str) -> Option<Extents> {
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        let book = b.get_or_insert_with(FontBook::system);
        book.measure(family, bold, italic, size, sx, sy, spacing, text)
    })
}

pub fn add_font_dir(dir: &std::path::Path) {
    BOOK.with(|b| b.borrow_mut().get_or_insert_with(FontBook::system).add_dir(dir));
}

pub fn has_family(name: &str) -> bool {
    BOOK.with(|b| b.borrow_mut().get_or_insert_with(FontBook::system).has_family(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn used_fonts_tracks_overrides() {
        use crate::ass::{Line, Script};
        let mut sc = Script::new_default();
        sc.push_line(Line::new(0, 1000, "{\\fnComic Neue\\b1}Hi{\\r} plain \\Nx"));
        let u = used_fonts(&sc);
        let comic = u.iter().find(|f| f.family == "Comic Neue").unwrap();
        assert!(comic.bold && comic.chars.contains(&'H'));
        let arial = u.iter().find(|f| f.family == "Arial" && !f.bold).unwrap();
        assert!(arial.chars.contains(&'p') && arial.chars.contains(&'x'));
    }

    #[test]
    fn collect_copies_and_reports() {
        use crate::ass::{Line, Script, Style};
        if !has_family("DejaVu Sans") {
            return;
        }
        let mut sc = Script::new_default();
        sc.entries.push(crate::ass::Entry::Style(Style { name: "A".into(), fontname: "DejaVu Sans".into(), ..Default::default() }));
        sc.push_line(Line { style: "A".into(), text: "ok".into(), ..Default::default() });
        sc.push_line(Line { text: "{\\fnNo Such Font Zzz}x".into(), ..Default::default() });
        let dir = std::env::temp_dir().join(format!("kaku-collect-{}", std::process::id()));
        let (copied, missing) = collect_fonts(&sc, &dir).unwrap();
        assert!(copied.iter().any(|c| c.contains("DejaVuSans")));
        assert!(missing.contains(&"No Such Font Zzz".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn measures_dejavu_if_present() {
        if !has_family("DejaVu Sans") {
            return;
        }
        let a = measure("DejaVu Sans", false, false, 48.0, 100.0, 100.0, 0.0, "Hello").unwrap();
        let b = measure("DejaVu Sans", false, false, 96.0, 100.0, 100.0, 0.0, "Hello").unwrap();
        assert!((b.width / a.width - 2.0).abs() < 1e-6);
        assert!(a.width > 50.0 && a.width < 200.0);
        assert!(measure("No Such Font Zzz", false, false, 48.0, 100.0, 100.0, 0.0, "x").is_none());
    }
}
