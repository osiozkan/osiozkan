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

    /// Yazı tipinde glifi olmayan karakterler; yazı tipi bulunamazsa `None`. (libass eksik glifleri
    /// başka bir yazı tipinden çizer — görünüm tutarsızlaşır; VSFilter kutu/boşluk gösterir.)
    pub fn missing_chars(&mut self, family: &str, bold: bool, italic: bool, chars: impl IntoIterator<Item = char>) -> Option<Vec<char>> {
        let id = self.find(family, bold, italic)?;
        let fd = self.data(id)?;
        let face = Face::parse(&fd.bytes, fd.index).ok()?;
        Some(chars.into_iter().filter(|&c| face.glyph_index(c).is_none()).collect())
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
            // Aegisub/VSFilter/libass: harf aralığı da yatay ölçekle çarpılır
            w += (adv * ppu + st.spacing) * st.scale_x / 100.0;
        }
        if bold_synth {
            w += text.chars().count() as f64 * st.fontsize / 24.0;
        }
        let sy = st.scale_y / 100.0;
        Some((w, st.fontsize * sy, fd.descent * ppu * sy, fd.ext_lead * ppu * sy))
    }
}

/// ttf-parser konturunu ASS çizim komutlarına çevirir (y aşağı, birim piksel).
struct ShapeBuilder {
    out: String,
    ox: f64,
    oy: f64,
    sx: f64,
    sy: f64,
    last: (f64, f64),
}

impl ShapeBuilder {
    fn p(&self, x: f32, y: f32) -> (f64, f64) {
        (self.ox + x as f64 * self.sx, self.oy - y as f64 * self.sy)
    }
    fn num(v: f64) -> String {
        let r = (v * 100.0).round() / 100.0;
        let s = format!("{r}");
        if s == "-0" { "0".into() } else { s }
    }
    fn push(&mut self, cmd: &str, pts: &[(f64, f64)]) {
        if !self.out.is_empty() {
            self.out.push(' ');
        }
        self.out.push_str(cmd);
        for (x, y) in pts {
            self.out.push(' ');
            self.out.push_str(&Self::num(*x));
            self.out.push(' ');
            self.out.push_str(&Self::num(*y));
        }
        if let Some(l) = pts.last() {
            self.last = *l;
        }
    }
}

impl ttf_parser::OutlineBuilder for ShapeBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.push("m", &[p]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.push("l", &[p]);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // ikinci derece → kübik Bezier
        let (p0, q, p3) = (self.last, self.p(x1, y1), self.p(x, y));
        let c1 = (p0.0 + 2.0 / 3.0 * (q.0 - p0.0), p0.1 + 2.0 / 3.0 * (q.1 - p0.1));
        let c2 = (p3.0 + 2.0 / 3.0 * (q.0 - p3.0), p3.1 + 2.0 / 3.0 * (q.1 - p3.1));
        self.push("b", &[c1, c2, p3]);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let pts = [self.p(x1, y1), self.p(x2, y2), self.p(x, y)];
        self.push("b", &pts);
    }
    fn close(&mut self) {}
}

impl FontDb {
    /// Metni ASS çizimine (`\p1`) çevirir: (0,0) = metin hücresinin sol üstü (taban çizgisi
    /// `ascent` kadar aşağıda). Dönüş: (çizim, genişlik, yükseklik). Yazı tipi yoksa `None`.
    /// Çekirdekleme yok (GDI/VSFilter gibi); `\fsp` her karakterden sonra eklenir.
    pub fn text_shape(&mut self, st: &StyleMetrics, text: &str) -> Option<(String, f64, f64)> {
        self.text_shape_at(st, text, 0.0, 0.0)
    }

    /// `text_shape` gibi, ama çizim (ox, oy) kadar kaydırılmış.
    pub fn text_shape_at(&mut self, st: &StyleMetrics, text: &str, ox: f64, oy: f64) -> Option<(String, f64, f64)> {
        let id = self.find(&st.fontname, st.bold, st.italic)?;
        let fd = self.data(id)?;
        let ppu = st.fontsize / fd.cell;
        let (sx, sy) = (ppu * st.scale_x / 100.0, ppu * st.scale_y / 100.0);
        let face = Face::parse(&fd.bytes, fd.index).ok()?;
        let asc = fd.cell - fd.descent;
        let mut b = ShapeBuilder { out: String::new(), ox: 0.0, oy: oy + asc * sy, sx, sy, last: (0.0, 0.0) };
        let mut pen = ox;
        for c in text.chars() {
            let g = face.glyph_index(c);
            b.ox = pen;
            if let Some(g) = g {
                face.outline_glyph(g, &mut b);
            }
            let adv = g.and_then(|g| face.glyph_hor_advance(g)).map(|a| a as f64).unwrap_or(fd.upem * 0.5);
            pen += adv * sx + st.spacing * st.scale_x / 100.0;
        }
        Some((b.out, pen - ox, st.fontsize * st.scale_y / 100.0))
    }
}

/// Satırın %100 ölçekteki kutusu (genişlik, yükseklik): `\\p` çizimiyse noktaların sınırı (0'dan),
/// değilse yazı tipi ölçüsüyle metin hücresi (çok satırda en geniş satır × satır sayısı).
pub fn line_box(db: &mut FontDb, script: &osisub_core::Script, d: &osisub_core::ass::Dialogue) -> Result<(f64, f64), String> {
    use osisub_core::overrides;
    let st = script.style_by_name(&d.style).cloned().unwrap_or_default();
    let get = |n: &str| overrides::get_initial(&d.text, n);
    if get("p").and_then(|t| t.number()).unwrap_or(0.0) > 0.0 {
        let draw = d.text.rsplit('}').next().unwrap_or("");
        let pts = osisub_core::drawing::points(&osisub_core::drawing::parse(draw));
        let w = pts.iter().map(|p| p.2 .0).fold(0.0, f64::max);
        let h = pts.iter().map(|p| p.2 .1).fold(0.0, f64::max);
        return if w > 0.0 && h > 0.0 { Ok((w, h)) } else { Err("Boş çizim".into()) };
    }
    let fontname = get("fn").map(|t| t.arg.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or(st.fontname.clone());
    let m = StyleMetrics {
        fontname: fontname.clone(),
        fontsize: get("fs").and_then(|t| t.number()).unwrap_or(st.fontsize),
        bold: get("b").and_then(|t| t.number()).map_or(st.bold, |v| v != 0.0),
        italic: get("i").and_then(|t| t.number()).map_or(st.italic, |v| v != 0.0),
        scale_x: 100.0,
        scale_y: 100.0,
        spacing: get("fsp").and_then(|t| t.number()).unwrap_or(st.spacing),
    };
    let text = osisub_core::tags::visible_text(&d.text);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut w: f64 = 0.0;
    for l in &lines {
        w = w.max(db.extents(&m, l).map(|e| e.0).ok_or(format!("Yazı tipi bulunamadı: {fontname}"))?);
    }
    Ok((w, m.fontsize * lines.len() as f64))
}

/// Bir altyazı satırını aynı görünümde `\\p1` çizimine çevirir (tipografide kırpma/şekil için).
/// Yalnızca baştaki etiket bloğu desteklenir; metin ortasında yazı tipi değişimi varsa hata.
/// Sonuç `\\an7\\pos` ile konumlanır, `\\fscx/\\fscy` 100'e sabitlenir, döndürme varsa `\\org`
/// eski hizalama noktasına konur (görünüm aynı kalır).
pub fn line_to_drawing(db: &mut FontDb, script: &osisub_core::Script, d: &osisub_core::ass::Dialogue) -> Result<String, String> {
    use osisub_core::overrides::{self, Seg};
    let segs = overrides::parse(&d.text);
    let mut lead: Vec<osisub_core::overrides::Tag> = vec![];
    let mut text = String::new();
    let mut seen_text = false;
    for s in &segs {
        match s {
            Seg::Block(t) if !seen_text => lead.extend(t.iter().cloned()),
            Seg::Block(t) => {
                if t.iter().any(|x| !x.name.is_empty()) {
                    return Err("Metin ortasında etiket var; yalnızca baştaki etiket bloğu desteklenir".into());
                }
            }
            Seg::Text(t) => {
                seen_text = true;
                text.push_str(t);
            }
        }
    }
    if lead.iter().any(|t| t.name == "p" && t.number().unwrap_or(0.0) > 0.0) {
        return Err("Satır zaten çizim".into());
    }
    if lead.iter().any(|t| t.name == "move") {
        return Err("\\move'lu satır desteklenmiyor (önce \\pos'a çevirin)".into());
    }
    let st = script.style_by_name(&d.style).cloned().unwrap_or_default();
    let get = |n: &str| lead.iter().rev().find(|t| t.name == n);
    let num = |n: &str, def: f64| get(n).and_then(|t| t.number()).unwrap_or(def);
    let fontname = get("fn").map(|t| t.arg.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or(st.fontname.clone());
    let m = StyleMetrics {
        fontname: fontname.clone(),
        fontsize: num("fs", st.fontsize),
        bold: get("b").and_then(|t| t.number()).map_or(st.bold, |v| v != 0.0),
        italic: get("i").and_then(|t| t.number()).map_or(st.italic, |v| v != 0.0),
        scale_x: num("fscx", st.scale_x),
        scale_y: num("fscy", st.scale_y),
        spacing: num("fsp", st.spacing),
    };
    let (px, py) = osisub_core::motion::line_position(script, d);
    let an = num("an", st.align as f64) as i32;
    let an = if (1..=9).contains(&an) { an } else { 2 };
    let hfrac = [0.0, 0.5, 1.0][((an - 1) % 3) as usize];
    let vfrac = [1.0, 0.5, 0.0][((an - 1) / 3) as usize];
    let lines: Vec<String> = text.replace("\\n", "\\N").split("\\N").map(|l| l.replace("\\h", "\u{a0}")).collect();
    let line_h = m.fontsize * m.scale_y / 100.0;
    let widths: Vec<f64> = lines.iter().map(|l| db.extents(&m, l).map(|e| e.0).ok_or(format!("Yazı tipi bulunamadı: {fontname}"))).collect::<Result<_, _>>()?;
    let w = widths.iter().cloned().fold(0.0, f64::max);
    let h = line_h * lines.len() as f64;
    let mut draw = String::new();
    for (k, l) in lines.iter().enumerate() {
        let ox = (w - widths[k]) * hfrac;
        let (sh, _, _) = db.text_shape_at(&m, l, ox, k as f64 * line_h).ok_or(format!("Yazı tipi bulunamadı: {fontname}"))?;
        if !sh.is_empty() {
            if !draw.is_empty() {
                draw.push(' ');
            }
            draw.push_str(&sh);
        }
    }
    let (x7, y7) = (px - w * hfrac, py - h * vfrac);
    let fmt = |v: f64| {
        let s = format!("{:.2}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let rotated = lead.iter().any(|t| matches!(t.name.as_str(), "frz" | "fr" | "frx" | "fry")) || st.angle != 0.0;
    let has_org = lead.iter().any(|t| t.name == "org");
    let mut keep: Vec<String> = lead
        .iter()
        .filter(|t| !matches!(t.name.as_str(), "fn" | "fs" | "b" | "i" | "fscx" | "fscy" | "fsp" | "an" | "a" | "pos" | "p" | "q"))
        .map(|t| t.to_ass())
        .collect();
    keep.retain(|t| !t.is_empty());
    let mut head = format!("\\an7\\pos({},{})\\fscx100\\fscy100", fmt(x7), fmt(y7));
    if rotated && !has_org {
        head.push_str(&format!("\\org({},{})", fmt(px), fmt(py)));
    }
    Ok(format!("{{{}{}\\p1}}{}", head, keep.join(""), draw))
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

    #[test]
    fn spacing_scales_with_fscx_and_shape_matches_extents() {
        let mut db = FontDb::default();
        let fam = ["dejavu sans", "liberation sans", "arial", "noto sans"].into_iter().find(|f| db.has_family(f));
        let Some(fam) = fam else { return }; // ortamda yazı tipi yok
        let st = |sx: f64, fsp: f64| StyleMetrics { fontname: fam.into(), fontsize: 50.0, scale_x: sx, scale_y: 100.0, spacing: fsp, ..Default::default() };
        let w0 = db.extents(&st(100.0, 0.0), "Abc").unwrap().0;
        let w = db.extents(&st(50.0, 6.0), "Abc").unwrap().0;
        // libass/Aegisub: (ilerleme + aralık) × fscx
        assert!((w - (w0 + 3.0 * 6.0) * 0.5).abs() < 0.01, "{w} vs {w0}");
        let (shape, sw, sh) = db.text_shape(&st(50.0, 6.0), "Abc").unwrap();
        assert!((sw - w).abs() < 0.01 && sh == 50.0);
        assert!(shape.starts_with("m ") && shape.contains(" b ") || shape.contains(" l "));
    }
}
