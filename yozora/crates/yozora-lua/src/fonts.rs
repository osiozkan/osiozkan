//! Yazı tipi ölçümü (`aegisub.text_extents`). Sistem yazı tiplerini fontdb ile bulur;
//! bulunamazsa yaklaşık genişlik tablosuna düşer.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub struct StyleMetrics<'a> {
    pub fontname: &'a str,
    pub fontsize: f64,
    pub bold: bool,
    pub italic: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
}

static DB: OnceLock<fontdb::Database> = OnceLock::new();
static CACHE: OnceLock<Mutex<HashMap<(String, bool, bool), Option<fontdb::ID>>>> = OnceLock::new();

fn db() -> &'static fontdb::Database {
    DB.get_or_init(|| {
        let mut d = fontdb::Database::new();
        d.load_system_fonts();
        if let Ok(extra) = std::env::var("YOZORA_FONT_DIRS") {
            for dir in std::env::split_paths(&extra) {
                d.load_fonts_dir(dir);
            }
        }
        d
    })
}

fn find(name: &str, bold: bool, italic: bool) -> Option<fontdb::ID> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (name.to_string(), bold, italic);
    if let Some(v) = cache.lock().ok()?.get(&key) {
        return *v;
    }
    let q = fontdb::Query {
        families: &[fontdb::Family::Name(name), fontdb::Family::SansSerif],
        weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
        stretch: fontdb::Stretch::Normal,
        style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
    };
    let id = db().query(&q);
    cache.lock().ok()?.insert(key, id);
    id
}

fn approx_advance(c: char) -> f64 {
    let u = c as u32;
    match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.28,
        'm' | 'w' | 'M' | 'W' => 0.85,
        _ if (0x1100..=0x11FF).contains(&u)
            || (0x2E80..=0xA4CF).contains(&u)
            || (0xAC00..=0xD7A3).contains(&u)
            || (0xF900..=0xFAFF).contains(&u)
            || (0xFF00..=0xFF60).contains(&u) => 1.0,
        _ if c.is_ascii_uppercase() => 0.68,
        _ => 0.52,
    }
}

/// (genişlik, yükseklik, descent, extlead) — Aegisub ile aynı sıra, stil ölçek ve aralık dahil.
pub fn extents(st: &StyleMetrics, text: &str) -> (f64, f64, f64, f64) {
    let size = st.fontsize;
    let sx = st.scale_x / 100.0;
    let sy = st.scale_y / 100.0;
    let nchars = text.chars().count() as f64;
    let id = find(st.fontname, st.bold, st.italic);
    if let Some(id) = id {
        let r = db().with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            let upem = face.units_per_em() as f64;
            let (asc, desc) = match face.tables().os2 {
                Some(os2) if os2.windows_ascender() != 0 || os2.windows_descender() != 0 => {
                    (os2.windows_ascender() as f64, (os2.windows_descender() as f64).abs())
                }
                _ => (face.ascender() as f64, (face.descender() as f64).abs()),
            };
            let cell = (asc + desc).max(1.0);
            let k = size / cell;
            let mut w = 0.0;
            for ch in text.chars() {
                let adv = face
                    .glyph_index(ch)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .map(|a| a as f64)
                    .unwrap_or(upem * approx_advance(ch));
                w += adv * k;
            }
            let lead = (face.line_gap() as f64).max(0.0) * k;
            Some((w, desc * k, lead))
        });
        if let Some(Some((w, d, l))) = r {
            return ((w + st.spacing * nchars) * sx, size * sy, d * sy, l * sy);
        }
    }
    let w: f64 = text.chars().map(|c| approx_advance(c) * size).sum();
    ((w + st.spacing * nchars) * sx, size * sy, size * 0.2 * sy, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_is_monotonic() {
        let st = StyleMetrics { fontname: "NoSuchFontXYZ", fontsize: 40.0, bold: false, italic: false, scale_x: 100.0, scale_y: 100.0, spacing: 0.0 };
        let (a, h, _, _) = extents(&st, "abc");
        let (b, _, _, _) = extents(&st, "abcdef");
        assert!(b > a && h == 40.0);
        let st2 = StyleMetrics { scale_x: 200.0, ..st };
        assert!((extents(&st2, "abc").0 - a * 2.0).abs() < 1e-6);
    }
}
