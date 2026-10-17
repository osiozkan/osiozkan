//! Text measurement for `aegisub.text_extents` (VSFilter/libass convention: font cell height == fontsize).

use fontdb::{Database, Family, Query, Stretch, Style as FStyle, Weight, ID};
use std::cell::RefCell;
use std::collections::HashMap;

pub struct FontReq<'a> {
    pub family: &'a str,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
}

thread_local! {
    static DB: RefCell<Option<Database>> = const { RefCell::new(None) };
    static ADV: RefCell<HashMap<(ID, char), f64>> = RefCell::new(HashMap::new());
    static MET: RefCell<HashMap<ID, (f64, f64, f64, f64)>> = RefCell::new(HashMap::new());
}

pub fn add_font_dir(dir: &str) {
    with_db(|db| db.load_fonts_dir(dir));
}

fn with_db<R>(f: impl FnOnce(&mut Database) -> R) -> R {
    DB.with(|d| {
        let mut d = d.borrow_mut();
        if d.is_none() {
            let mut db = Database::new();
            db.load_system_fonts();
            *d = Some(db);
        }
        f(d.as_mut().unwrap())
    })
}

fn find(req: &FontReq) -> Option<ID> {
    with_db(|db| {
        let fam = [Family::Name(req.family)];
        let q = Query {
            families: &fam,
            weight: if req.bold { Weight::BOLD } else { Weight::NORMAL },
            stretch: Stretch::Normal,
            style: if req.italic { FStyle::Italic } else { FStyle::Normal },
        };
        db.query(&q).or_else(|| {
            let fam = [Family::SansSerif];
            db.query(&Query { families: &fam, ..q })
        })
    })
}

/// (units_per_em, ascender, descender(positive), line_gap) in font units
fn face_metrics(id: ID) -> Option<(f64, f64, f64, f64)> {
    if let Some(m) = MET.with(|m| m.borrow().get(&id).copied()) {
        return Some(m);
    }
    let r = with_db(|db| {
        db.with_face_data(id, |data, idx| {
            ttf_parser::Face::parse(data, idx).ok().map(|f| {
                (f.units_per_em() as f64, f.ascender() as f64, -(f.descender() as f64), f.line_gap() as f64)
            })
        })
        .flatten()
    })?;
    MET.with(|m| m.borrow_mut().insert(id, r));
    Some(r)
}

fn advance(id: ID, c: char, upm: f64) -> f64 {
    if let Some(v) = ADV.with(|a| a.borrow().get(&(id, c)).copied()) {
        return v;
    }
    let v = with_db(|db| {
        db.with_face_data(id, |data, idx| {
            let f = ttf_parser::Face::parse(data, idx).ok()?;
            let g = f.glyph_index(c).or_else(|| f.glyph_index('?'))?;
            f.glyph_hor_advance(g).map(|a| a as f64 / upm)
        })
        .flatten()
    })
    .unwrap_or(0.5);
    ADV.with(|a| a.borrow_mut().insert((id, c), v));
    v
}

/// Returns (width, height, descent, external_leading)
pub fn text_extents(req: &FontReq, text: &str) -> (f64, f64, f64, f64) {
    let id = find(req);
    let (upm, asc, desc, gap) = id.and_then(face_metrics).unwrap_or((1000.0, 800.0, 200.0, 0.0));
    let cell = (asc + desc).max(1.0);
    // em size such that ascent+descent == fontsize
    let em = req.size * upm / cell;
    let mut w = 0.0;
    let mut n = 0.0;
    for c in text.chars() {
        if c == '\n' || c == '\r' {
            continue;
        }
        let a = match id {
            Some(id) => advance(id, c, upm),
            None => 0.5,
        };
        w += a * em + req.spacing;
        n += 1.0;
    }
    let _ = n;
    let width = w * req.scale_x / 100.0;
    let height = req.size * req.scale_y / 100.0;
    let descent = req.size * desc / cell * req.scale_y / 100.0;
    let ext = gap * em / upm * req.scale_y / 100.0;
    (width, height, descent, ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widths_scale() {
        let r = FontReq { family: "Arial", size: 40.0, bold: false, italic: false, scale_x: 100.0, scale_y: 100.0, spacing: 0.0 };
        let (w1, h, d, _) = text_extents(&r, "Hello");
        let r2 = FontReq { scale_x: 200.0, ..r };
        let (w2, _, _, _) = text_extents(&r2, "Hello");
        assert!((w2 - 2.0 * w1).abs() < 1e-6);
        assert!(w1 > 0.0 && h == 40.0 && d > 0.0 && d < 40.0);
    }
}
