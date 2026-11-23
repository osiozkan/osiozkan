//! Stil katalogları (Aegisub "Style Storage"): her katalog bir `.sty` dosyası, her satırı bir
//! `Style: …` satırı (Aegisub `?user/catalog/*.sty` ile aynı biçim). Gösteri başına ayrı katalog
//! tutulabilir; Aegisub'ın katalogları da okunur.
use crate::ass::{style_line, Script, Style};
use std::path::{Path, PathBuf};

/// Katalog dosyasındaki stiller (bozuk satırlar atlanır).
pub fn read(path: &Path) -> Vec<Style> {
    let Ok(raw) = std::fs::read(path) else { return vec![] };
    let text = crate::ass::decode_bytes(&raw);
    let body: String = text.lines().filter(|l| l.trim_start().starts_with("Style:")).map(|l| format!("{}\n", l.trim_start())).collect();
    Script::parse(&format!("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n{body}")).styles().cloned().collect()
}

/// Kataloğu yazar (aynı adlı stillerden sonuncusu kalır).
pub fn write(path: &Path, styles: &[Style]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut seen = std::collections::HashSet::new();
    let mut keep: Vec<&Style> = styles.iter().rev().filter(|s| seen.insert(s.name.clone())).collect();
    keep.reverse();
    let text: String = keep.iter().map(|s| format!("{}\n", style_line(s))).collect();
    std::fs::write(path, text)
}

/// Klasörlerdeki katalogların adları ve yolları (ada göre; aynı ad ilk klasördekidir).
pub fn list(dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = vec![];
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e.eq_ignore_ascii_case("sty"))).collect();
        v.sort();
        for p in v {
            let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if !out.iter().any(|(n, _)| *n == name) {
                out.push((name, p));
            }
        }
    }
    out
}

/// Dosya adında kullanılamayan karakterler `_` olur (Aegisub gibi).
pub fn safe_name(name: &str) -> String {
    name.trim().chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_list() {
        let d = std::env::temp_dir().join(format!("osisub-cat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut a = Style::default();
        a.name = "Dialogue".into();
        a.fontsize = 52.0;
        let mut b = a.clone();
        b.name = "Sign, büyük".into();
        let mut a2 = a.clone();
        a2.fontsize = 60.0;
        let p = d.join(format!("{}.sty", safe_name("Show: S2")));
        write(&p, &[a, b, a2]).unwrap();
        let back = read(&p);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "Sign; büyük"); // virgül → ';' (Aegisub)
        assert_eq!(back[1].fontsize, 60.0);
        std::fs::write(d.join("not.txt"), "x").unwrap();
        let l = list(&[d.clone(), d.join("yok")]);
        assert_eq!(l.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["Show_ S2"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
