//! Sistem yazı tiplerini tarayıp GDI/VSFilter ile uyumlu metin ölçüsü verir.
//!
//! ASS yazı tipi boyutu "hücre yüksekliği"dir (winAscent+winDescent); bu yüzden ölçek
//! `fontsize / (winAscent + winDescent)` alınır. Çekirdekleme (kerning) uygulanmaz (GDI gibi).
use crate::engine::StyleMetrics;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use ttf_parser::{name_id, Face};

struct FaceInfo {
    family: String,
    bold: bool,
    italic: bool,
    path: PathBuf,
    index: u32,
}

struct FaceData {
    bytes: Vec<u8>,
    index: u32,
    upem: f64,
    cell: f64,
    descent: f64,
    ext_lead: f64,
    adv: HashMap<char, f64>,
}

#[derive(Default)]
pub struct FontDb {
    faces: Vec<FaceInfo>,
    scanned: bool,
    extra_dirs: Vec<PathBuf>,
    loaded: HashMap<usize, Option<FaceData>>,
}

fn default_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["/usr/share/fonts", "/usr/local/share/fonts", "/Library/Fonts", "/System/Library/Fonts", "C:\\Windows\\Fonts"].iter().map(PathBuf::from).collect();
    if let Ok(h) = std::env::var("HOME") {
        v.push(PathBuf::from(&h).join(".fonts"));
        v.push(PathBuf::from(&h).join(".local/share/fonts"));
        v.push(PathBuf::from(&h).join("Library/Fonts"));
    }
    v
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
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

fn family_names(face: &Face) -> Vec<String> {
    let mut out = Vec::new();
    for n in face.names() {
        if (n.name_id == name_id::FAMILY || n.name_id == name_id::TYPOGRAPHIC_FAMILY || n.name_id == name_id::FULL_NAME) && n.is_unicode() {
            if let Some(s) = n.to_string() {
                let s = s.to_lowercase();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

impl FontDb {
    pub fn add_dir(&mut self, d: impl Into<PathBuf>) {
        self.extra_dirs.push(d.into());
        self.scanned = false;
        self.faces.clear();
        self.loaded.clear();
    }

    fn scan(&mut self) {
        if self.scanned {
            return;
        }
        self.scanned = true;
        let mut files = Vec::new();
        for d in default_dirs().iter().chain(self.extra_dirs.iter()) {
            walk(d, &mut files, 0);
        }
        for path in files {
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let n = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
            for index in 0..n {
                let Ok(face) = Face::parse(&bytes, index) else { continue };
                let (bold, italic) = (face.is_bold(), face.is_italic());
                for family in family_names(&face) {
                    self.faces.push(FaceInfo { family, bold, italic, path: path.clone(), index });
                }
            }
        }
    }

    pub fn family_count(&mut self) -> usize {
        self.scan();
        let mut f: Vec<&str> = self.faces.iter().map(|f| f.family.as_str()).collect();
        f.sort_unstable();
        f.dedup();
        f.len()
    }

    fn find(&mut self, family: &str, bold: bool, italic: bool) -> Option<usize> {
        self.scan();
        let fam = family.trim_start_matches('@').to_lowercase();
        let cands: Vec<usize> = (0..self.faces.len()).filter(|&i| self.faces[i].family == fam).collect();
        cands
            .iter()
            .copied()
            .find(|&i| self.faces[i].bold == bold && self.faces[i].italic == italic)
            .or_else(|| cands.iter().copied().find(|&i| self.faces[i].italic == italic))
            .or_else(|| cands.first().copied())
    }

    fn data(&mut self, id: usize) -> Option<&mut FaceData> {
        if !self.loaded.contains_key(&id) {
            let info = &self.faces[id];
            let d = std::fs::read(&info.path).ok().and_then(|bytes| {
                let face = Face::parse(&bytes, info.index).ok()?;
                let upem = face.units_per_em() as f64;
                let (mut asc, mut desc) = (face.ascender() as f64, -(face.descender() as f64));
                // GDI hücre yüksekliği: OS/2 winAscent + winDescent (yoksa hhea).
                if let Some(os2) = face.tables().os2 {
                    let (wa, wd) = (os2.windows_ascender() as f64, os2.windows_descender() as f64);
                    if wa + wd.abs() > 0.0 {
                        asc = wa;
                        desc = wd.abs();
                    }
                }
                if asc + desc <= 0.0 {
                    asc = upem * 0.9;
                    desc = upem * 0.2;
                }
                let lead = face.line_gap() as f64;
                let index = info.index;
                Some(FaceData { bytes, index, upem, cell: asc + desc, descent: desc, ext_lead: lead.max(0.0), adv: HashMap::new() })
            });
            self.loaded.insert(id, d);
        }
        self.loaded.get_mut(&id).and_then(|d| d.as_mut())
    }

    /// (genişlik, yükseklik, descent, extlead) — bulunamazsa `None`.
    pub fn extents(&mut self, st: &StyleMetrics, text: &str) -> Option<(f64, f64, f64, f64)> {
        let id = self.find(&st.fontname, st.bold, st.italic)?;
        let bold_synth = st.bold && !self.faces[id].bold;
        let fd = self.data(id)?;
        let ppu = st.fontsize / fd.cell;
        let face = Face::parse(&fd.bytes, fd.index).ok()?;
        let mut w = 0.0;
        for c in text.chars() {
            let adv = *fd.adv.entry(c).or_insert_with(|| {
                face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map(|a| a as f64).unwrap_or(fd.upem * 0.5)
            });
            w += adv * ppu * st.scale_x / 100.0 + st.spacing;
        }
        if bold_synth {
            w += text.chars().count() as f64 * st.fontsize / 24.0;
        }
        let sy = st.scale_y / 100.0;
        Some((w, st.fontsize * sy, fd.descent * ppu * sy, fd.ext_lead * ppu * sy))
    }
}
