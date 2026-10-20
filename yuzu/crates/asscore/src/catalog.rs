//! Style storage (Aegisub's style catalogs): `<name>.sty` files holding one `Style:` row per line,
//! so a series' styles can be copied into every new episode.

use crate::model::Style;

/// Styles in a catalog file (other lines are ignored).
pub fn parse(text: &str) -> Vec<Style> {
    text.trim_start_matches('\u{feff}').lines().filter_map(crate::ass::parse_style_line).collect()
}

pub fn write(styles: &[Style]) -> String {
    styles.iter().map(|s| format!("{}\n", crate::ass::style_line(s))).collect()
}

/// Insert `s`, or replace the style of the same name in place. True when one was replaced.
pub fn upsert(list: &mut Vec<Style>, s: Style) -> bool {
    match list.iter_mut().find(|x| x.name == s.name) {
        Some(x) => { *x = s; true }
        None => { list.push(s); false }
    }
}

/// A catalog name that is safe as a file name (Aegisub refuses the same characters).
pub fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n != "." && n != ".." && !n.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
}

/// Catalog names (file stems of `*.sty`) in a folder, sorted.
pub fn list(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| { let p = e.path(); (p.extension().map(|x| x.eq_ignore_ascii_case("sty")).unwrap_or(false)).then(|| p.file_stem().map(|s| s.to_string_lossy().to_string())).flatten() })
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

pub fn load(dir: &std::path::Path, name: &str) -> Vec<Style> {
    std::fs::read(dir.join(format!("{}.sty", name))).map(|b| parse(&String::from_utf8_lossy(&b))).unwrap_or_default()
}

pub fn save(dir: &std::path::Path, name: &str, styles: &[Style]) -> std::io::Result<()> {
    if !valid_name(name) { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid catalog name")); }
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(format!("{}.sty", name.trim())), write(styles))
}

/// Move the style at `i` by `delta` places (Aegisub's style list ordering buttons).
pub fn move_style(list: &mut [Style], i: usize, delta: isize) -> usize {
    let j = (i as isize + delta).clamp(0, list.len() as isize - 1) as usize;
    if i < list.len() && j != i { list.swap(i, j); }
    j
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_round_trip_and_upsert() {
        let mut a = Style::default();
        a.name = "Dialogue".into();
        a.fontname = "Gandhi Sans".into();
        a.fontsize = 62.0;
        a.bold = true;
        let mut b = Style::default();
        b.name = "Sign, top".into();
        b.alignment = 8;
        let text = write(&[a.clone(), b.clone()]);
        assert!(text.starts_with("Style: Dialogue,Gandhi Sans,62,"));
        let back = parse(&format!("\u{feff}; yorum\n{}\nbozuk satır\n", text));
        // a comma cannot live in a style name: written as ';' like Aegisub
        b.name = "Sign; top".into();
        assert_eq!(back, vec![a.clone(), b.clone()]);
        let mut list = back;
        let mut a2 = a.clone();
        a2.fontsize = 70.0;
        assert!(upsert(&mut list, a2.clone()));
        assert_eq!(list[0].fontsize, 70.0);
        let mut c = Style::default();
        c.name = "Yeni".into();
        assert!(!upsert(&mut list, c));
        assert_eq!(list.len(), 3);
        assert_eq!(move_style(&mut list, 2, -1), 1);
        assert_eq!(list[1].name, "Yeni");
        assert_eq!(move_style(&mut list, 0, -1), 0);
    }

    #[test]
    fn names_and_files() {
        assert!(valid_name("Sakamoto Days"));
        assert!(!valid_name("a/b") && !valid_name(" ") && !valid_name("x:y") && !valid_name(".."));
        let dir = std::env::temp_dir().join(format!("yuzu_catalog_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Style::default();
        s.name = "Default".into();
        save(&dir, "Seri B", &[s.clone()]).unwrap();
        save(&dir, "a seri", &[]).unwrap();
        assert_eq!(list(&dir), vec!["a seri".to_string(), "Seri B".to_string()]);
        assert_eq!(load(&dir, "Seri B"), vec![s]);
        assert!(load(&dir, "yok").is_empty());
        assert!(save(&dir, "../dışarı", &[]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
