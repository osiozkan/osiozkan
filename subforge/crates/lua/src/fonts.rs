//! Real font metrics for `aegisub.text_extents` (GDI-compatible semantics:
//! width = Σ advance·size/em + spacing·chars, no kerning), with graceful
//! fallback to the heuristic estimator when no matching font is installed.
use fontdb::{Database, Family, Query, Stretch, Style as FStyle, Weight, ID};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use subforge_core::ass::Style;

struct FontStore {
    db: Database,
    cache: HashMap<(String, bool, bool), Option<ID>>,
}

thread_local! {
    static STORE: RefCell<Option<FontStore>> = const { RefCell::new(None) };
    static EXTRA_DIRS: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Make fonts from `dir` (script folder `fonts/`, user fontsdir) available.
pub fn add_font_dir(dir: PathBuf) {
    EXTRA_DIRS.with(|d| {
        let mut d = d.borrow_mut();
        if !d.contains(&dir) {
            d.push(dir.clone());
        }
    });
    STORE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.db.load_fonts_dir(&dir);
            st.cache.clear();
        }
    });
}

fn with_store<R>(f: impl FnOnce(&mut FontStore) -> R) -> R {
    STORE.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            let mut db = Database::new();
            db.load_system_fonts();
            EXTRA_DIRS.with(|d| d.borrow().iter().for_each(|p| db.load_fonts_dir(p)));
            *s = Some(FontStore { db, cache: HashMap::new() });
        }
        f(s.as_mut().unwrap())
    })
}

/// Metric-compatible substitutes for the usual Windows fonts.
fn substitutes(name: &str) -> &'static [&'static str] {
    match name.to_ascii_lowercase().as_str() {
        "arial" | "helvetica" | "arial unicode ms" => &["Liberation Sans", "Arimo", "Nimbus Sans", "DejaVu Sans"],
        "times new roman" | "times" => &["Liberation Serif", "Tinos", "Nimbus Roman", "DejaVu Serif"],
        "courier new" | "courier" => &["Liberation Mono", "Cousine", "Nimbus Mono PS", "DejaVu Sans Mono"],
        "verdana" | "tahoma" | "segoe ui" | "calibri" => &["DejaVu Sans", "Liberation Sans"],
        _ => &["DejaVu Sans", "Liberation Sans", "Noto Sans"],
    }
}

fn find(store: &mut FontStore, name: &str, bold: bool, italic: bool) -> Option<ID> {
    let key = (name.to_string(), bold, italic);
    if let Some(v) = store.cache.get(&key) {
        return *v;
    }
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if italic { FStyle::Italic } else { FStyle::Normal };
    let lookup = |db: &Database, fam: &str| {
        let fams = [Family::Name(fam)];
        db.query(&Query { families: &fams, weight, stretch: Stretch::Normal, style })
    };
    let mut id = lookup(&store.db, name.trim_start_matches('@'));
    if id.is_none() {
        for s in substitutes(name) {
            id = lookup(&store.db, s);
            if id.is_some() {
                break;
            }
        }
    }
    store.cache.insert(key, id);
    id
}

/// (width, height, descent, external leading) in script pixels, or None when no font is usable.
pub fn text_extents(style: &Style, text: &str) -> Option<(f64, f64, f64, f64)> {
    with_store(|st| {
        let id = find(st, &style.fontname, style.bold, style.italic)?;
        st.db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            let upem = face.units_per_em() as f64;
            let size = style.fontsize;
            let k = size / upem;
            let mut w = 0.0;
            let mut n = 0.0;
            for c in text.chars() {
                if c == '\n' || c == '\r' {
                    continue;
                }
                let adv = face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or((upem * 0.5) as u16);
                w += adv as f64 * k + style.spacing;
                n += 1.0;
            }
            let _ = n;
            let (asc, desc) = match face.tables().os2 {
                Some(os2) => (os2.windows_ascender() as f64, os2.windows_descender().unsigned_abs() as f64),
                None => (face.ascender() as f64, face.descender().unsigned_abs() as f64),
            };
            let height = (asc + desc) * k;
            let descent = desc * k;
            let lead = (face.line_gap() as f64 + face.ascender() as f64 - face.descender() as f64 - (asc + desc)).max(0.0) * k;
            Some((w * style.scale_x / 100.0, height * style.scale_y / 100.0, descent * style.scale_y / 100.0, lead * style.scale_y / 100.0))
        })?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measures_or_falls_back() {
        let st = Style { fontname: "Arial".into(), fontsize: 40.0, ..Style::default() };
        if let Some((w, h, d, _)) = text_extents(&st, "Hello") {
            assert!(w > 40.0 && w < 200.0, "{w}");
            assert!(h > 35.0 && h < 60.0);
            assert!(d > 0.0);
            let (w2, ..) = text_extents(&st, "Hello Hello").unwrap();
            assert!(w2 > w * 1.9);
        }
    }
}
