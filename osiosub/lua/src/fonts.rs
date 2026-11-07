//! Real font metrics (system fonts) for `aegisub.text_extents`, GDI/VSFilter-style:
//! font size maps to the cell height (ascent+descent), no kerning.
use fontdb::{Database, Family, Query, Stretch, Style as FStyle, Weight};
use osiosub_core::ass::Style;
use std::sync::{Mutex, OnceLock};

fn db() -> &'static Mutex<Database> {
    static DB: OnceLock<Mutex<Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut d = Database::new();
        d.load_system_fonts();
        for dir in ["fonts", "automation/fonts", "."] {
            if std::path::Path::new(dir).is_dir() { d.load_fonts_dir(dir); }
        }
        Mutex::new(d)
    })
}

/// Add a font directory (e.g. the folder next to the subtitle file).
pub fn add_font_dir(p: &std::path::Path) { db().lock().unwrap().load_fonts_dir(p); }

pub fn has_font(name: &str) -> bool {
    let d = db().lock().unwrap();
    d.query(&Query { families: &[Family::Name(name)], ..Default::default() }).is_some()
}

/// Returns (width, height, descent, external_leading) or None if the font can't be found.
pub fn extents(style: &Style, text: &str) -> Option<(f64, f64, f64, f64)> {
    let d = db().lock().unwrap();
    let name = style.fontname.trim_start_matches('@');
    let q = Query {
        families: &[Family::Name(name), Family::SansSerif],
        weight: if style.bold { Weight::BOLD } else { Weight::NORMAL },
        stretch: Stretch::Normal,
        style: if style.italic { FStyle::Italic } else { FStyle::Normal },
    };
    let id = d.query(&q)?;
    d.with_face_data(id, |data, idx| {
        let face = ttf_parser::Face::parse(data, idx).ok()?;
        let upm = face.units_per_em() as f64;
        let (asc, desc) = match face.tables().os2 {
            Some(os2) => (os2.windows_ascender() as f64, -(os2.windows_descender() as f64)),
            None => (face.ascender() as f64, -(face.descender() as f64)),
        };
        let cell = (asc + desc).max(1.0);
        let scale = style.fontsize / cell;
        let _ = upm;
        let mut w = 0.0;
        let mut n = 0usize;
        for c in text.chars() {
            if c == '\n' { continue; }
            let adv = face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or((upm * 0.5) as u16) as f64;
            w += adv * scale;
            n += 1;
        }
        w = w * style.scale_x / 100.0 + style.spacing * n as f64;
        let lg = face.line_gap() as f64 * scale;
        Some((w, style.fontsize * style.scale_y / 100.0, desc * scale * style.scale_y / 100.0, lg))
    })?
}
