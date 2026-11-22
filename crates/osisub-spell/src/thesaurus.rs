//! Eş anlamlılar sözlüğü (MyThes `.idx` + `.dat`; LibreOffice/Aegisub biçimi).
//!
//! `.idx`: 1. satır kodlama, 2. satır girdi sayısı, sonra `sözcük|bayt_konumu`.
//! `.dat`: 1. satır kodlama; konumda `sözcük|anlam_sayısı`, ardından her anlam için
//! `(tür)|eş1|eş2|…` satırları.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Meaning {
    /// Ör. "(noun)" / "(isim)"; ilk eş anlamlı genellikle anlamın kendisidir.
    pub pos: String,
    pub words: Vec<String>,
}

pub struct Thesaurus {
    /// (küçük harfli sözcük, konum), sözcüğe göre sıralı
    index: Vec<(String, usize)>,
    dat: Vec<u8>,
    latin: Option<&'static [char; 96]>,
    pub name: String,
}

/// ISO-8859-9 (Türkçe) 0xA0..0xFF; ISO-8859-1'den yalnızca 6 harf farklı.
fn latin_table(enc: &str) -> Option<&'static [char; 96]> {
    static L1: std::sync::OnceLock<[char; 96]> = std::sync::OnceLock::new();
    static L9: std::sync::OnceLock<[char; 96]> = std::sync::OnceLock::new();
    let e = enc.trim().to_ascii_uppercase().replace('_', "-");
    let base = || {
        let mut t = ['\0'; 96];
        for (i, c) in t.iter_mut().enumerate() { *c = char::from_u32(0xA0 + i as u32).unwrap(); }
        t
    };
    if e.contains("8859-9") || e.contains("1254") {
        Some(L9.get_or_init(|| {
            let mut t = base();
            for (b, c) in [(0xD0, 'Ğ'), (0xDD, 'İ'), (0xDE, 'Ş'), (0xF0, 'ğ'), (0xFD, 'ı'), (0xFE, 'ş')] { t[b - 0xA0] = c; }
            t
        }))
    } else if e.contains("8859-1") || e.contains("1252") || e.contains("LATIN1") {
        Some(L1.get_or_init(base))
    } else {
        None
    }
}

fn decode(bytes: &[u8], latin: Option<&[char; 96]>) -> String {
    match latin {
        None => String::from_utf8_lossy(bytes).into_owned(),
        Some(t) => bytes.iter().map(|&b| if b >= 0xA0 { t[(b - 0xA0) as usize] } else { b as char }).collect(),
    }
}

fn lower(s: &str) -> String {
    osisub_core::qc::tr_lower(s)
}

impl Thesaurus {
    pub fn load(idx: &Path, dat: &Path) -> Result<Thesaurus, String> {
        let ib = std::fs::read(idx).map_err(|e| format!("{}: {}", idx.display(), e))?;
        let dat_b = std::fs::read(dat).map_err(|e| format!("{}: {}", dat.display(), e))?;
        let mut lines = ib.split(|&b| b == b'\n');
        let enc = String::from_utf8_lossy(lines.next().unwrap_or_default()).trim().to_string();
        let latin = latin_table(&enc);
        let _count = lines.next();
        let mut index: Vec<(String, usize)> = lines
            .filter_map(|l| {
                let l = decode(l, latin);
                let (w, off) = l.trim_end_matches('\r').rsplit_once('|')?;
                Some((lower(w), off.trim().parse().ok()?))
            })
            .collect();
        index.sort_by(|a, b| a.0.cmp(&b.0));
        let name = dat.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        Ok(Thesaurus { index, dat: dat_b, latin, name })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn line_at(&self, pos: usize) -> Option<(String, usize)> {
        if pos >= self.dat.len() { return None; }
        let rest = &self.dat[pos..];
        let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
        Some((decode(&rest[..end], self.latin).trim_end_matches('\r').to_string(), pos + end + 1))
    }

    /// Sözcüğün anlamları (harf duyarsız, Türkçe i/İ). Bulunamazsa boş.
    pub fn lookup(&self, word: &str) -> Vec<Meaning> {
        let key = lower(word.trim());
        let Ok(i) = self.index.binary_search_by(|e| e.0.as_str().cmp(&key)) else { return vec![] };
        let Some((head, mut pos)) = self.line_at(self.index[i].1) else { return vec![] };
        let n: usize = head.rsplit_once('|').and_then(|(_, n)| n.trim().parse().ok()).unwrap_or(0);
        let mut out = vec![];
        for _ in 0..n.min(256) {
            let Some((l, next)) = self.line_at(pos) else { break };
            pos = next;
            let mut parts = l.split('|');
            let pos_tag = parts.next().unwrap_or("").trim().to_string();
            let words: Vec<String> = parts.map(|w| strip_note(w).to_string()).filter(|w| !w.is_empty()).collect();
            if !words.is_empty() { out.push(Meaning { pos: pos_tag, words }); }
        }
        out
    }
}

/// "glad (related term)" → "glad": değiştirirken açıklama metne girmesin.
fn strip_note(w: &str) -> &str {
    let w = w.trim();
    match w.rfind(" (") {
        Some(i) if w.ends_with(')') && i > 0 => w[..i].trim_end(),
        _ => w,
    }
}

/// `th_<dil>_v2` / `th_<dil>` eş anlamlılar sözlüğünü arar (`/usr/share/mythes`, ayar klasörü…).
pub fn find_thesaurus(lang: &str, extra_dirs: &[PathBuf]) -> Option<(PathBuf, PathBuf)> {
    let mut dirs: Vec<PathBuf> = extra_dirs.to_vec();
    dirs.extend(["/usr/share/mythes", "/usr/share/myspell", "/usr/share/hunspell", "/usr/share/myspell/dicts"].iter().map(PathBuf::from));
    if let Some(c) = osisub_core::paths::config_dir() {
        dirs.push(c.join("dict"));
    }
    for d in dirs {
        for stem in [format!("th_{}_v2", lang), format!("th_{}", lang)] {
            let (idx, dat) = (d.join(format!("{}.idx", stem)), d.join(format!("{}.dat", stem)));
            if idx.is_file() && dat.is_file() {
                return Some((idx, dat));
            }
        }
    }
    None
}

/// Ham satır metninde (etiketler/`\N` dahil) karakter konumundaki sözcük: (baş, son) karakter aralığı.
/// Etiket bloğunun içindeyse ya da sözcük yoksa `None`. Kesme işareti içteyse sözcüğe dahildir (Saki'yi).
pub fn word_at(text: &str, at: usize) -> Option<(usize, usize)> {
    let c: Vec<char> = text.chars().collect();
    // etiket bloğu içi mi?
    let mut depth_open: Option<usize> = None;
    for (i, &ch) in c.iter().enumerate().take(at.min(c.len())) {
        if ch == '{' { depth_open = Some(i); }
        if ch == '}' { depth_open = None; }
    }
    if depth_open.is_some() { return None; }
    let is_w = |i: usize| -> bool {
        let ch = c[i];
        if ch.is_alphanumeric() { return true; }
        // iç kesme işareti / tire: iki yanı harf
        (ch == '\'' || ch == '’' || ch == '-') && i > 0 && i + 1 < c.len() && c[i - 1].is_alphabetic() && c[i + 1].is_alphabetic()
    };
    let mut p = at.min(c.len());
    if p == c.len() || !is_w(p) {
        if p > 0 && is_w(p - 1) { p -= 1; } else { return None; }
    }
    let (mut s, mut e) = (p, p + 1);
    while s > 0 && is_w(s - 1) && c[s - 1] != '{' { s -= 1; }
    while e < c.len() && is_w(e) { e += 1; }
    // \N, \n, \h kaçışı sözcüğe yapışmasın
    if s > 0 && c[s - 1] == '\\' && matches!(c[s], 'N' | 'n' | 'h') {
        s += 1;
        if s >= e || at < s { return None; }
    }
    if c[s..e].iter().all(|ch| ch.is_numeric()) { return None; }
    Some((s, e))
}

/// Değiştirilen sözcüğün harf biçimini korur: "IŞIK"→"AYDINLIK", "Işık"→"Aydınlık" (Türkçe i/İ).
pub fn match_case(orig: &str, repl: &str) -> String {
    let letters: Vec<char> = orig.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return osisub_core::qc::tr_upper(repl);
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        let mut it = repl.chars();
        if let Some(f) = it.next() {
            if f.is_lowercase() {
                return osisub_core::qc::tr_upper(&f.to_string()) + it.as_str();
            }
        }
    }
    repl.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn harf_bicimi() {
        assert_eq!(super::match_case("IŞIK", "aydınlık"), "AYDINLIK");
        assert_eq!(super::match_case("Işık", "ince"), "İnce");
        assert_eq!(super::match_case("ışık", "Nur"), "Nur");
        assert_eq!(super::match_case("A", "bir"), "Bir");
    }

    use super::*;

    fn write_th(dir: &Path, enc: &str, entries: &[(&str, &[&str])]) -> (PathBuf, PathBuf) {
        // entries: (sözcük, ["(tür)|a|b", ...])
        let mut dat: Vec<u8> = format!("{}\n", enc).into_bytes();
        let mut idx = format!("{}\n{}\n", enc, entries.len());
        for (w, ms) in entries {
            idx.push_str(&format!("{}|{}\n", w, dat.len()));
            dat.extend(format!("{}|{}\n", w, ms.len()).bytes());
            for m in *ms { dat.extend(m.bytes()); dat.push(b'\n'); }
        }
        let (i, d) = (dir.join("th_xx_XX.idx"), dir.join("th_xx_XX.dat"));
        std::fs::write(&i, idx).unwrap();
        std::fs::write(&d, dat).unwrap();
        (i, d)
    }

    #[test]
    fn utf8_arama_turkce_harf() {
        let dir = std::env::temp_dir().join(format!("osisub-th-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (i, d) = write_th(&dir, "UTF-8", &[("ışık", &["(isim)|ışık|aydınlık|nur"]), ("iyi", &["(sıfat)|iyi|güzel", "(zarf)|iyice"])]);
        let t = Thesaurus::load(&i, &d).unwrap();
        assert_eq!(t.len(), 2);
        let m = t.lookup("IŞIK");
        assert_eq!(m, vec![Meaning { pos: "(isim)".into(), words: vec!["ışık".into(), "aydınlık".into(), "nur".into()] }]);
        assert_eq!(t.lookup("İyi").len(), 2);
        assert!(t.lookup("yok").is_empty());
    }

    #[test]
    fn latin9_kodlama() {
        let dir = std::env::temp_dir().join(format!("osisub-th9-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // "şık" ISO-8859-9: FE FD 6B
        let mut dat = b"ISO8859-9\n".to_vec();
        let off = dat.len();
        dat.extend(b"\xFE\xFDk|1\n(s)|\xFE\xFDk|zarif\n");
        std::fs::write(dir.join("th_t.dat"), &dat).unwrap();
        let mut idx = b"ISO8859-9\n1\n".to_vec();
        idx.extend(b"\xFE\xFDk|");
        idx.extend(off.to_string().bytes());
        idx.push(b'\n');
        std::fs::write(dir.join("th_t.idx"), &idx).unwrap();
        let t = Thesaurus::load(&dir.join("th_t.idx"), &dir.join("th_t.dat")).unwrap();
        assert_eq!(t.lookup("şık")[0].words, vec!["şık".to_string(), "zarif".to_string()]);
    }

    #[test]
    fn bozuk_dosya_paniklemez() {
        let dir = std::env::temp_dir().join(format!("osisub-thb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.idx"), "UTF-8\n2\nkelime|99999\nbozuk\nbaşka|x\n").unwrap();
        std::fs::write(dir.join("b.dat"), "UTF-8\n").unwrap();
        let t = Thesaurus::load(&dir.join("b.idx"), &dir.join("b.dat")).unwrap();
        assert!(t.lookup("kelime").is_empty());
    }

    #[test]
    fn imlecteki_sozcuk() {
        let t = "{\\an8}Merhaba\\NSaki'yi gördün mü?";
        let c: Vec<char> = t.chars().collect();
        let s = |r: Option<(usize, usize)>| r.map(|(a, b)| c[a..b].iter().collect::<String>());
        assert_eq!(s(word_at(t, 2)), None); // etiket içi
        assert_eq!(s(word_at(t, 8)), Some("Merhaba".into()));
        assert_eq!(s(word_at(t, 13)), Some("Merhaba".into())); // sözcük sonu
        assert_eq!(s(word_at(t, 15)), Some("Saki'yi".into())); // \N sonrası, N alınmaz
        assert_eq!(s(word_at(t, 14)), None); // N'nin üstü
        assert_eq!(s(word_at(t, t.chars().count())), None); // '?' sonrası
        assert_eq!(s(word_at("abc 123", 5)), None);
        assert_eq!(s(word_at("", 0)), None);
    }

    #[test]
    fn gercek_en_us() {
        let Some((i, d)) = find_thesaurus("en_US", &[]) else { return };
        let t = Thesaurus::load(&i, &d).unwrap();
        let m = t.lookup("Happy");
        assert!(m.iter().any(|m| m.words.iter().any(|w| w == "glad")), "{:?}", m);
        assert!(m.iter().all(|m| m.words.iter().all(|w| !w.contains("term)"))), "{:?}", m);
    }
}
