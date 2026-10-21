//! System font discovery and text metrics (GDI/VSFilter-compatible sizing: the ASS font size is the
//! full cell height, i.e. `winAscent + winDescent`, mapped onto `size`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct FontEntry {
    pub path: PathBuf,
    pub index: u32,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Default)]
pub struct FontDb {
    entries: Vec<FontEntry>,
    scanned: bool,
    extra_dirs: Vec<PathBuf>,
    data: HashMap<PathBuf, Rc<Vec<u8>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extents {
    pub width: f64,
    pub height: f64,
    pub descent: f64,
    pub extlead: f64,
}

fn default_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    for d in ["/usr/share/fonts", "/usr/local/share/fonts", "/Library/Fonts", "/System/Library/Fonts", "C:\\Windows\\Fonts"] {
        v.push(PathBuf::from(d));
    }
    if let Ok(h) = std::env::var("HOME") {
        for d in [".fonts", ".local/share/fonts", "Library/Fonts"] {
            v.push(Path::new(&h).join(d));
        }
    }
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        v.push(Path::new(&l).join("Microsoft/Windows/Fonts"));
    }
    v
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out, depth + 1);
        } else if let Some(ext) = p.extension().map(|x| x.to_string_lossy().to_lowercase()) {
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                out.push(p);
            }
        }
    }
}

fn names(face: &ttf_parser::Face, id: u16) -> Vec<String> {
    face.names().into_iter().filter(|n| n.name_id == id).filter_map(|n| n.to_string()).collect()
}

impl FontDb {
    pub fn new() -> FontDb {
        FontDb::default()
    }

    pub fn add_dir(&mut self, dir: impl Into<PathBuf>) {
        self.extra_dirs.push(dir.into());
        self.scanned = false;
    }

    fn scan(&mut self) {
        if self.scanned {
            return;
        }
        self.scanned = true;
        self.entries.clear();
        let mut files = Vec::new();
        for d in default_dirs().iter().chain(self.extra_dirs.iter()) {
            walk(d, &mut files, 0);
        }
        for p in files {
            let Ok(bytes) = std::fs::read(&p) else { continue };
            let n = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
            for index in 0..n {
                let Ok(face) = ttf_parser::Face::parse(&bytes, index) else { continue };
                let mut fams = names(&face, 1);
                fams.extend(names(&face, 16));
                fams.extend(names(&face, 4)); // full name: ASS styles sometimes use it
                fams.sort();
                fams.dedup();
                for family in fams {
                    self.entries.push(FontEntry { path: p.clone(), index, family, bold: face.is_bold(), italic: face.is_italic() });
                }
            }
            if n == 1 {
                // keep memory bounded: only cache on demand
            }
        }
    }

    pub fn families(&mut self) -> Vec<String> {
        self.scan();
        let mut f: Vec<String> = self.entries.iter().map(|e| e.family.clone()).collect();
        f.sort_by_key(|s| s.to_lowercase());
        f.dedup();
        f
    }

    pub fn lookup(&mut self, family: &str, bold: bool, italic: bool) -> Option<FontEntry> {
        self.scan();
        let fam = family.trim_start_matches('@').to_lowercase();
        let cands: Vec<&FontEntry> = self.entries.iter().filter(|e| e.family.to_lowercase() == fam).collect();
        if cands.is_empty() {
            return None;
        }
        cands.iter().find(|e| e.bold == bold && e.italic == italic).or_else(|| cands.iter().find(|e| e.bold == bold)).or(cands.first()).map(|e| (*e).clone())
    }

    fn bytes(&mut self, p: &Path) -> Option<Rc<Vec<u8>>> {
        if let Some(b) = self.data.get(p) {
            return Some(b.clone());
        }
        let b = Rc::new(std::fs::read(p).ok()?);
        self.data.insert(p.to_path_buf(), b.clone());
        Some(b)
    }

    /// Measure `text` for a style. Falls back to a heuristic when the font is not installed.
    pub fn extents(&mut self, family: &str, size: f64, bold: bool, italic: bool, scale_x: f64, scale_y: f64, spacing: f64, text: &str) -> Extents {
        let entry = self.lookup(family, bold, italic);
        let data = entry.as_ref().and_then(|e| self.bytes(&e.path).map(|b| (b, e.index)));
        if let Some((bytes, index)) = data {
            if let Ok(face) = ttf_parser::Face::parse(&bytes, index) {
                let upem = face.units_per_em() as f64;
                let (asc, desc) = os2_metrics(&face);
                let cell = (asc + desc).max(1.0);
                let k = size / cell;
                let mut w = 0.0;
                let mut n = 0usize;
                for c in text.chars() {
                    let adv = face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map(|a| a as f64).unwrap_or_else(|| upem * crate::automation::approx_char_width(c));
                    w += adv * k + spacing;
                    n += 1;
                }
                let _ = n;
                let lead = face.line_gap() as f64 * k;
                return Extents { width: w * scale_x / 100.0, height: size * scale_y / 100.0, descent: desc * k * scale_y / 100.0, extlead: lead * scale_y / 100.0 };
            }
        }
        let w: f64 = text.chars().map(|c| crate::automation::approx_char_width(c) * size + spacing).sum();
        Extents { width: w * scale_x / 100.0, height: size * 1.2 * scale_y / 100.0, descent: size * 0.22 * scale_y / 100.0, extlead: 0.0 }
    }
}

fn os2_metrics(face: &ttf_parser::Face) -> (f64, f64) {
    // GDI uses winAscent/winDescent; fall back to hhea when OS/2 is missing.
    if let Some(os2) = face.tables().os2 {
        let (a, d) = (os2.windows_ascender() as f64, -(os2.windows_descender() as f64));
        if a + d > 0.0 {
            return (a, d);
        }
    }
    let (a, d) = (face.ascender() as f64, -(face.descender() as f64));
    (a.max(1.0), d.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measures_known_font() {
        let mut db = FontDb::new();
        let Some(_) = db.lookup("DejaVu Sans", false, false) else { return }; // skip where not installed
        let a = db.extents("DejaVu Sans", 48.0, false, false, 100.0, 100.0, 0.0, "Hello");
        let b = db.extents("DejaVu Sans", 48.0, false, false, 100.0, 100.0, 0.0, "Hello Hello");
        assert!(a.width > 80.0 && a.width < 200.0, "{:?}", a);
        assert!(b.width > a.width * 2.0);
        assert_eq!(a.height, 48.0);
        let s = db.extents("DejaVu Sans", 48.0, false, false, 50.0, 100.0, 0.0, "Hello");
        assert!((s.width - a.width / 2.0).abs() < 0.01);
        let missing = db.extents("No Such Font 123", 48.0, false, false, 100.0, 100.0, 0.0, "Hello");
        assert!(missing.width > 0.0);
    }
}
