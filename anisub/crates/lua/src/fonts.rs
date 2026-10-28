//! Font-based text measurement, modelled on VSFilter/libass: `\fs` is the cell height
//! (win ascent + win descent), so advances scale by `fontsize / (ascent + descent)`.

use std::path::Path;

pub struct Fonts {
    db: fontdb::Database,
    system_loaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extents {
    pub width: f64,
    pub height: f64,
    pub descent: f64,
    pub extlead: f64,
}

pub struct Spec<'a> {
    pub family: &'a str,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
}

impl Fonts {
    pub fn new() -> Fonts {
        Fonts { db: fontdb::Database::new(), system_loaded: false }
    }

    pub fn add_dir(&mut self, dir: &Path) {
        self.db.load_fonts_dir(dir);
    }

    fn ensure_system(&mut self) {
        if !self.system_loaded {
            self.db.load_system_fonts();
            self.system_loaded = true;
        }
    }

    fn find(&mut self, s: &Spec) -> Option<fontdb::ID> {
        self.ensure_system();
        let q = fontdb::Query {
            families: &[fontdb::Family::Name(s.family.trim_start_matches('@')), fontdb::Family::SansSerif],
            weight: if s.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            stretch: fontdb::Stretch::Normal,
            style: if s.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        self.db.query(&q)
    }

    /// Returns `None` when no usable font exists (caller falls back to an estimate).
    pub fn measure(&mut self, s: &Spec, text: &str) -> Option<Extents> {
        let id = self.find(s)?;
        self.db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            let upem = face.units_per_em() as f64;
            let (asc, desc) = match face.tables().os2 {
                Some(os2) if os2.windows_ascender() != 0 || os2.windows_descender() != 0 => {
                    (os2.windows_ascender() as f64, os2.windows_descender().unsigned_abs() as f64)
                }
                _ => (face.ascender() as f64, face.descender().unsigned_abs() as f64),
            };
            let cell = (asc + desc).max(1.0);
            let k = s.size / cell;
            let mut w = 0.0;
            let mut n = 0usize;
            let fallback = upem * 0.5;
            for ch in text.chars() {
                n += 1;
                let adv = face
                    .glyph_index(ch)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .map(|a| a as f64)
                    .unwrap_or(fallback);
                w += adv * k;
            }
            w += s.spacing * n as f64;
            Some(Extents {
                width: w * s.scale_x / 100.0,
                height: s.size * s.scale_y / 100.0,
                descent: desc * k * s.scale_y / 100.0,
                extlead: face.line_gap() as f64 * k * s.scale_y / 100.0,
            })
        })?
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}
