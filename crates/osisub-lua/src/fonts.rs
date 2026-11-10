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

/// Dosya kimliği: boyut + değişiklik zamanı (sn). Değişmeyen dosya yeniden okunmaz.
fn file_stamp(p: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(p).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((m.len(), t))
}

fn parse_faces(path: &Path) -> Vec<FaceInfo> {
    let mut out = vec![];
    let Ok(bytes) = std::fs::read(path) else { return out };
    let n = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    for index in 0..n {
        let Ok(face) = Face::parse(&bytes, index) else { continue };
        let (bold, italic) = (face.is_bold(), face.is_italic());
        for family in family_names(&face) {
            out.push(FaceInfo { family, bold, italic, path: path.to_path_buf(), index });
        }
    }
    out
}

/// Yazı tipi dosyalarını tarar; `cache` verilirse (yol, boyut, zaman) eşleşen dosyalar için
/// önbellekteki aile adları kullanılır ve önbellek güncellenir. Biçim: TSV, satır başına bir yüz.
fn scan_files(files: &[PathBuf], cache: Option<&Path>) -> Vec<FaceInfo> {
    // önbellek: yol -> (boyut, zaman, yüzler)
    let mut old: HashMap<PathBuf, (u64, u64, Vec<FaceInfo>)> = HashMap::new();
    if let Some(c) = cache {
        if let Ok(t) = std::fs::read_to_string(c) {
            for l in t.lines().skip(1) {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() != 7 {
                    continue;
                }
                let (Ok(size), Ok(mtime)) = (f[1].parse::<u64>(), f[2].parse::<u64>()) else { continue };
                let path = PathBuf::from(f[0]);
                let e = old.entry(path.clone()).or_insert((size, mtime, vec![]));
                // "-" = yüzü olmayan (bozuk) dosya
                if f[6] != "-" {
                    e.2.push(FaceInfo { family: f[6].to_string(), bold: f[4] == "1", italic: f[5] == "1", path, index: f[3].parse().unwrap_or(0) });
                }
            }
        }
    }
    let mut faces = vec![];
    let mut out = String::from("# osisub yazı tipi dizini v1\n");
    let mut changed = old.len() != files.len();
    for path in files {
        let stamp = file_stamp(path).unwrap_or((0, 0));
        let got = match old.remove(path) {
            Some((s, m, f)) if (s, m) == stamp => f,
            _ => {
                changed = true;
                parse_faces(path)
            }
        };
        if cache.is_some() {
            if got.is_empty() {
                out.push_str(&format!("{}\t{}\t{}\t0\t0\t0\t-\n", path.display(), stamp.0, stamp.1));
            }
            for x in &got {
                out.push_str(&format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\n", path.display(), stamp.0, stamp.1, x.index, x.bold as u8, x.italic as u8, x.family.replace(['\t', '\n'], " ")));
            }
        }
        faces.extend(got);
    }
    if let (Some(c), true) = (cache, changed) {
        if let Some(d) = c.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = c.with_extension("tmp");
        if std::fs::write(&tmp, out).is_ok() {
            let _ = std::fs::rename(&tmp, c);
        }
    }
    faces
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
        let cache = if std::env::var_os("OSISUB_NO_FONT_CACHE").is_some() { None } else { osisub_core::paths::data_dir().map(|d| d.join("font-index.tsv")) };
        self.faces = scan_files(&files, cache.as_deref());
    }

    /// Kurulu bir yazı tipi ailesi var mı (büyük/küçük harf duyarsız, tam adlar dahil).
    pub fn has_family(&mut self, family: &str) -> bool {
        self.scan();
        let f = family.trim_start_matches('@').to_lowercase();
        self.faces.iter().any(|x| x.family == f)
    }

    /// Ailenin dosya yolları (yinelenenler ayıklanmış).
    pub fn files_for(&mut self, family: &str) -> Vec<PathBuf> {
        self.scan();
        let f = family.trim_start_matches('@').to_lowercase();
        let mut v: Vec<PathBuf> = self.faces.iter().filter(|x| x.family == f).map(|x| x.path.clone()).collect();
        v.sort();
        v.dedup();
        v
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_cache_matches_full_scan() {
        let mut files = vec![];
        for d in default_dirs() {
            walk(&d, &mut files, 0);
        }
        files.truncate(40);
        if files.is_empty() {
            return; // ortamda yazı tipi yok
        }
        let dir = std::env::temp_dir().join(format!("osisub-fontcache-{}", std::process::id()));
        let c = dir.join("idx.tsv");
        let key = |v: &[FaceInfo]| { let mut k: Vec<String> = v.iter().map(|f| format!("{}|{}|{}|{}|{}", f.path.display(), f.index, f.bold, f.italic, f.family)).collect(); k.sort(); k };
        let full = scan_files(&files, None);
        let first = scan_files(&files, Some(&c));
        assert!(c.is_file());
        // önbellekten okuma: dosyaları "okunamaz" yapamayız; önbelleği elle işaretleyip kullanıldığını doğrula
        let t = std::fs::read_to_string(&c).unwrap();
        let fam = first.first().map(|f| f.family.clone()).unwrap_or_default();
        std::fs::write(&c, t.replacen(&format!("\t{}\n", fam), "\tönbellek-ailesi\n", 1)).unwrap();
        let cached = scan_files(&files, Some(&c));
        assert_eq!(key(&full), key(&first));
        assert!(cached.iter().any(|f| f.family == "önbellek-ailesi"), "önbellek kullanılmadı");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

