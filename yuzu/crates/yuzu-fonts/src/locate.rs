//! Map used fonts to concrete font files.

use crate::used::FontUse;
use fontdb::{Database, Family, Query, Source, Stretch, Style, Weight};
use std::path::{Path, PathBuf};

/// A concrete font file on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFile {
    pub path: PathBuf,
    /// Face index inside the file (non-zero only for collections).
    pub index: u32,
    /// Family name the database reports for this face.
    pub face_family: String,
    /// True for TrueType/OpenType collections (`.ttc`/`.otc` or `ttcf` magic).
    pub is_collection: bool,
}

/// Result of resolving one [`FontUse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// `None` when nothing usable was found (or the face only exists in memory).
    pub file: Option<FontFile>,
    /// True if the requested family (or PostScript name) matched; false for a substitute.
    pub exact: bool,
}

impl Located {
    /// No file at all.
    pub fn is_missing(&self) -> bool {
        self.file.is_none()
    }
    /// A file was found but for a different family than requested.
    pub fn is_fallback(&self) -> bool {
        self.file.is_some() && !self.exact
    }
}

fn file_of(db: &Database, id: fontdb::ID) -> Option<FontFile> {
    let face = db.face(id)?;
    let path = match &face.source {
        Source::File(p) => p.clone(),
        Source::SharedFile(p, _) => p.clone(),
        Source::Binary(_) => return None,
    };
    let ext_coll = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "ttc" | "otc"))
        .unwrap_or(false);
    let magic_coll = || {
        use std::io::Read;
        let mut b = [0u8; 4];
        std::fs::File::open(&path).and_then(|mut f| f.read_exact(&mut b)).is_ok() && &b == b"ttcf"
    };
    Some(FontFile {
        is_collection: ext_coll || face.index > 0 || magic_coll(),
        index: face.index,
        face_family: face.families.first().map(|f| f.0.clone()).unwrap_or_default(),
        path,
    })
}

/// Resolve a font use against `db`. Family matching is case-insensitive and also accepts the
/// PostScript name. If the family is absent, a sans-serif substitute is returned with
/// `exact == false` (the caller decides whether that counts as found).
pub fn locate(u: &FontUse, db: &Database) -> Located {
    let want = u.family.to_lowercase();
    let target = if u.bold { 700i32 } else { 400 };
    let mut best: Option<(i32, fontdb::ID)> = None;
    for f in db.faces() {
        let hit = f.families.iter().any(|(n, _)| n.to_lowercase() == want) || f.post_script_name.to_lowercase() == want;
        if !hit {
            continue;
        }
        let mut score = (f.weight.0 as i32 - target).abs();
        if (f.style != Style::Normal) != u.italic {
            score += 1000;
        }
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, f.id));
        }
    }
    let mut out = Located { family: u.family.clone(), bold: u.bold, italic: u.italic, file: None, exact: false };
    if let Some((_, id)) = best {
        out.file = file_of(db, id);
        out.exact = true;
        return out;
    }
    let q = Query {
        families: &[Family::SansSerif],
        weight: Weight(target as u16),
        stretch: Stretch::Normal,
        style: if u.italic { Style::Italic } else { Style::Normal },
    };
    out.file = db.query(&q).and_then(|id| file_of(db, id));
    out
}

/// [`locate`] every font in `uses`.
pub fn locate_all(uses: &[FontUse], db: &Database) -> Vec<Located> {
    uses.iter().map(|u| locate(u, db)).collect()
}

/// Load every font from the given directories (missing ones are skipped). Returns the number of
/// faces added. Use for the folder next to the `.ass` and for user font folders.
pub fn scan_dirs(db: &mut Database, dirs: &[PathBuf]) -> usize {
    let before = db.len();
    for d in dirs {
        if d.is_dir() {
            db.load_fonts_dir(d);
        }
    }
    db.len() - before
}

/// Per-user font folders for the current platform that exist on disk.
pub fn user_font_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(h) = env("HOME") {
        v.push(h.join(".fonts"));
        v.push(h.join(".local/share/fonts"));
        v.push(h.join("Library/Fonts"));
    }
    if let Some(x) = env("XDG_DATA_HOME") {
        v.push(x.join("fonts"));
    }
    if let Some(l) = env("LOCALAPPDATA") {
        v.push(l.join("Microsoft/Windows/Fonts"));
    }
    v.retain(|p| p.is_dir());
    v.dedup();
    v
}

/// Convenience: the directory containing `script_path`, if any (for [`scan_dirs`]).
pub fn script_dir(script_path: &Path) -> Option<PathBuf> {
    script_path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_path_buf() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fu(f: &str) -> FontUse {
        FontUse { family: f.into(), bold: false, italic: false, chars: Default::default(), lines: 1 }
    }

    #[test]
    fn empty_db_is_missing() {
        let db = Database::new();
        let l = locate(&fu("Arial"), &db);
        assert!(l.is_missing());
        assert!(!l.exact && !l.is_fallback());
    }

    #[test]
    fn scan_missing_dir_adds_nothing() {
        let mut db = Database::new();
        assert_eq!(scan_dirs(&mut db, &[PathBuf::from("/definitely/not/here")]), 0);
    }

    #[test]
    fn system_fonts_when_available() {
        let mut db = Database::new();
        db.load_system_fonts();
        let Some(face) = db.faces().find(|f| matches!(f.source, Source::File(_))) else {
            eprintln!("no system fonts; skipping");
            return;
        };
        let fam = face.families[0].0.to_uppercase();
        let l = locate(&fu(&fam), &db);
        assert!(l.exact, "case-insensitive exact match expected");
        assert!(l.file.is_some());
        let l = locate(&fu("No Such Font Family 12345"), &db);
        assert!(!l.exact);
    }
}
