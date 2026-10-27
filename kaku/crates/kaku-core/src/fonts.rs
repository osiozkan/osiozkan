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
