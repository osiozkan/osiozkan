//! Font-backed text measurement (GDI-style: advance widths without kerning, em size = fontsize).

use nsub_core::metrics::{Extents, HeuristicMetrics, TextMetrics};
use nsub_core::Style;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

pub struct FontMetrics {
    db: fontdb::Database,
    cache: RefCell<HashMap<(String, bool, bool), Option<fontdb::ID>>>,
    fallback: HeuristicMetrics,
}

impl FontMetrics {
    /// Load system fonts.
    pub fn system() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        FontMetrics { db, cache: RefCell::new(HashMap::new()), fallback: HeuristicMetrics }
    }

    /// Add a directory of fonts (e.g. the folder next to the script, like Aegisub's fonts collector).
    pub fn add_dir(&mut self, dir: &Path) {
        self.db.load_fonts_dir(dir);
        self.cache.borrow_mut().clear();
    }

    pub fn face_count(&self) -> usize {
        self.db.len()
    }

    fn lookup(&self, style: &Style) -> Option<fontdb::ID> {
        let key = (style.fontname.trim_start_matches('@').to_lowercase(), style.bold, style.italic);
        if let Some(v) = self.cache.borrow().get(&key) {
            return *v;
        }
        let q = fontdb::Query {
            families: &[fontdb::Family::Name(&style.fontname.trim_start_matches('@').to_string()), fontdb::Family::SansSerif],
            weight: if style.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            stretch: fontdb::Stretch::Normal,
            style: if style.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        let id = self.db.query(&q);
        self.cache.borrow_mut().insert(key, id);
        id
    }

    /// True when the exact family exists (not just the sans-serif fallback).
    pub fn has_family(&self, name: &str) -> bool {
        let n = name.trim_start_matches('@').to_lowercase();
        self.db.faces().any(|f| f.families.iter().any(|(fam, _)| fam.to_lowercase() == n))
    }
}

impl TextMetrics for FontMetrics {
    fn extents(&self, style: &Style, text: &str) -> Extents {
        let Some(id) = self.lookup(style) else { return self.fallback.extents(style, text) };
        self.db
            .with_face_data(id, |data, index| {
                let Ok(face) = ttf_parser::Face::parse(data, index) else { return None };
                let upem = face.units_per_em() as f64;
                let k = style.fontsize / upem;
                let mut w = 0.0;
                let mut n = 0.0;
                for c in text.chars() {
                    let adv = face
                        .glyph_index(c)
                        .and_then(|g| face.glyph_hor_advance(g))
                        .map(|a| a as f64)
                        .unwrap_or(upem * 0.5);
                    w += adv * k;
                    n += 1.0;
                }
                let (asc, desc) = match face.tables().os2 {
                    Some(os2) if os2.windows_ascender() != 0 || os2.windows_descender() != 0 => {
                        (os2.windows_ascender() as f64, os2.windows_descender().unsigned_abs() as f64)
                    }
                    _ => (face.ascender() as f64, face.descender().unsigned_abs() as f64),
                };
                let gap = face.line_gap().max(0) as f64;
                // Aegisub scales the cell so that the em square equals fontsize.
                let sy = style.scale_y / 100.0;
                Some(Extents {
                    width: (w + n * style.spacing) * style.scale_x / 100.0,
                    height: (asc + desc) * k * sy,
                    descent: desc * k * sy,
                    extlead: gap * k * sy,
                })
            })
            .flatten()
            .unwrap_or_else(|| self.fallback.extents(style, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wider_text_is_wider_and_scales_with_size() {
        let m = FontMetrics::system();
        if m.face_count() == 0 {
            return; // no fonts on this machine
        }
        let mut st = Style::default();
        st.fontname = "DejaVu Sans".into();
        let a = m.extents(&st, "Hello");
        let b = m.extents(&st, "Hello Hello");
        assert!(b.width > a.width * 1.9 && b.width < a.width * 2.5);
        st.fontsize *= 2.0;
        let c = m.extents(&st, "Hello");
        assert!((c.width / a.width - 2.0).abs() < 0.01);
        assert!(a.height > st.fontsize / 2.0 * 0.9);
        st.scale_x = 50.0;
        assert!((m.extents(&st, "Hello").width / c.width - 0.5).abs() < 0.01);
    }
}

// ---------------------------------------------------------------------------------------------
// Fonts collector

use nsub_core::tags::{parse, Segment};
use nsub_core::Script;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontUse {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

/// Every (family, bold, italic) combination that actually renders text, following `\fn \b \i \r` state per event.
pub fn used_fonts(script: &Script) -> BTreeSet<FontUse> {
    let mut out = BTreeSet::new();
    for e in script.events.iter().filter(|e| !e.comment) {
        let base = script.style_or_default(&e.style);
        let mut cur = FontUse { family: base.fontname.trim_start_matches('@').to_string(), bold: base.bold, italic: base.italic };
        let mut drawing = false;
        for seg in parse(&e.text) {
            match seg {
                Segment::Block(tags) => {
                    for t in tags {
                        match t.name.as_str() {
                            "fn" => cur.family = t.value.trim().trim_start_matches('@').to_string(),
                            "b" => {
                                let v: i32 = t.value.trim().parse().unwrap_or(-1);
                                cur.bold = if v == -1 { base.bold } else { v == 1 || v >= 700 };
                            }
                            "i" => {
                                let v: i32 = t.value.trim().parse().unwrap_or(-1);
                                cur.italic = if v == -1 { base.italic } else { v == 1 };
                            }
                            "r" => {
                                let name = t.value.trim();
                                let st = if name.is_empty() { base.clone() } else { script.style_or_default(name) };
                                cur = FontUse { family: st.fontname.trim_start_matches('@').to_string(), bold: st.bold, italic: st.italic };
                            }
                            "p" => drawing = t.value.trim().parse::<f64>().unwrap_or(0.0) > 0.0,
                            _ => {}
                        }
                    }
                }
                Segment::Text(t) => {
                    if !drawing && !t.trim().is_empty() {
                        out.insert(cur.clone());
                    }
                }
            }
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct CollectReport {
    pub found: Vec<(FontUse, PathBuf)>,
    pub missing: Vec<FontUse>,
}

impl FontMetrics {
    /// Locate the font file that would render `u` (None if the family isn't installed).
    pub fn find_file(&self, u: &FontUse) -> Option<PathBuf> {
        if !self.has_family(&u.family) {
            return None;
        }
        let q = fontdb::Query {
            families: &[fontdb::Family::Name(&u.family)],
            weight: if u.bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
            stretch: fontdb::Stretch::Normal,
            style: if u.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        let id = self.db.query(&q)?;
        match &self.db.face(id)?.source {
            fontdb::Source::File(p) => Some(p.clone()),
            fontdb::Source::SharedFile(p, _) => Some(p.clone()),
            fontdb::Source::Binary(_) => None,
        }
    }

    pub fn report(&self, script: &Script) -> CollectReport {
        let mut r = CollectReport::default();
        for u in used_fonts(script) {
            match self.find_file(&u) {
                Some(p) => r.found.push((u, p)),
                None => r.missing.push(u),
            }
        }
        r
    }
}

/// Copy the font files used by `script` into `dir`. Returns the report plus the number of files copied.
pub fn collect_fonts(fm: &FontMetrics, script: &Script, dir: &Path) -> std::io::Result<(CollectReport, usize)> {
    std::fs::create_dir_all(dir)?;
    let rep = fm.report(script);
    let mut seen = std::collections::HashSet::new();
    let mut n = 0;
    for (_, p) in &rep.found {
        if seen.insert(p.clone()) {
            if let Some(name) = p.file_name() {
                std::fs::copy(p, dir.join(name))?;
                n += 1;
            }
        }
    }
    Ok((rep, n))
}

#[cfg(test)]
mod collector_tests {
    use super::*;
    use nsub_core::Event;

    #[test]
    fn tracks_font_state() {
        let mut s = Script::new();
        s.styles[0].fontname = "Arial".into();
        s.events.push(Event { text: "plain {\\fnVerdana\\b1}bold verdana{\\r} back {\\i1}it{\\p1}m 0 0 l 1 1{\\p0}".into(), ..Event::default() });
        let u = used_fonts(&s);
        let names: Vec<(String, bool, bool)> = u.iter().map(|f| (f.family.clone(), f.bold, f.italic)).collect();
        assert!(names.contains(&("Arial".into(), false, false)));
        assert!(names.contains(&("Verdana".into(), true, false)));
        assert!(names.contains(&("Arial".into(), false, true)));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn finds_and_copies_installed_font() {
        let fm = FontMetrics::system();
        if !fm.has_family("DejaVu Sans") {
            return;
        }
        let mut s = Script::new();
        s.styles[0].fontname = "DejaVu Sans".into();
        s.events.push(Event { text: "x".into(), ..Event::default() });
        s.events.push(Event { text: "{\\fnNoSuchFontXYZ}y".into(), ..Event::default() });
        let dir = std::env::temp_dir().join(format!("nsub-fonts-{}", std::process::id()));
        let (rep, n) = collect_fonts(&fm, &s, &dir).unwrap();
        assert_eq!(n, 1);
        assert_eq!(rep.missing.len(), 1);
        assert_eq!(rep.missing[0].family, "NoSuchFontXYZ");
        assert!(std::fs::read_dir(&dir).unwrap().count() == 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
