//! Platforma uygun yapılandırma/veri dizinleri.
use std::path::PathBuf;

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Ayarlar, sözlükler, stil kataloğu, otomatik yüklenen betikler: `<yapılandırma>/osisub`.
/// Windows: %APPDATA%\osisub · macOS/Linux: $XDG_CONFIG_HOME ya da ~/.config altında.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(x).join("osisub"));
    }
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") {
            return Some(PathBuf::from(a).join("osisub"));
        }
    }
    home().map(|h| h.join(".config/osisub"))
}

/// Otomatik kayıt vb.: `<veri>/osisub`.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(x).join("osisub"));
    }
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("LOCALAPPDATA") {
            return Some(PathBuf::from(a).join("osisub"));
        }
    }
    home().map(|h| h.join(".local/share/osisub"))
}

/// Kurulu Aegisub'ın kullanıcı otomasyon klasörleri (var olanlar): betikleri ve
/// DependencyControl modülleri (`include/`) osisub'dan da kullanılabilsin diye.
/// Windows: %APPDATA%\Aegisub\automation · macOS: ~/Library/Application Support/Aegisub/automation ·
/// Linux: ~/.aegisub/automation. `OSISUB_AEGISUB_DIR` ile değiştirilebilir (boş = kapalı).
pub fn aegisub_automation_dirs() -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(x) = std::env::var_os("OSISUB_AEGISUB_DIR") {
        if !x.is_empty() {
            out.push(PathBuf::from(x));
        }
    } else {
        if let Some(a) = std::env::var_os("APPDATA") {
            out.push(PathBuf::from(a).join("Aegisub/automation"));
        }
        if let Some(h) = home() {
            out.push(h.join(".aegisub/automation"));
            out.push(h.join("Library/Application Support/Aegisub/automation"));
        }
    }
    out.retain(|p| p.is_dir());
    out
}

/// Lua `require`/`include` arama klasörleri: osisub'ın `automation/include`'u, sonra Aegisub'ınki.
pub fn automation_include_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = config_dir().map(|d| d.join("automation/include")).into_iter().collect();
    v.extend(aegisub_automation_dirs().into_iter().map(|d| d.join("include")).filter(|d| d.is_dir()));
    v
}

/// Aegisub `[Aegisub Project Garbage]` → `Automation Scripts:` değerini çözer (Aegisub
/// LocalScriptManager ile aynı). Ayraç `|`; önekler: `~` = altyazı dosyasının klasörüne göre,
/// `$` = otomasyon tabanına göre (Aegisub'ın `?user/automation`'ı), `/` = mutlak yol. Diğerleri atlanır.
pub fn decode_automation_scripts(value: &str, script_dir: Option<&std::path::Path>, automation_base: Option<&std::path::Path>) -> Vec<PathBuf> {
    let mut out = vec![];
    for item in value.split('|').map(str::trim).filter(|s| !s.is_empty()) {
        let rest = &item[1..];
        let rel = rest.trim_start_matches(['/', '\\']);
        let p = match item.as_bytes()[0] {
            b'~' => match script_dir { Some(d) => d.join(rel), None => continue },
            b'$' => match automation_base { Some(d) => d.join(rel), None => continue },
            // "/" + mutlak yol: Linux'ta "//a/b.lua", Windows'ta "/C:\a.lua"
            b'/' => PathBuf::from(if rest.starts_with('/') { rest } else if cfg!(windows) { rest } else { item }),
            _ => continue,
        };
        out.push(p);
    }
    out
}

/// `decode_automation_scripts`'in tersi: altyazı klasöründeki betikler `~` ile göreli, diğerleri `/` ile mutlak.
pub fn encode_automation_scripts(paths: &[PathBuf], script_dir: Option<&std::path::Path>) -> String {
    paths
        .iter()
        .map(|p| match script_dir.and_then(|d| p.strip_prefix(d).ok()) {
            Some(r) => format!("~/{}", r.to_string_lossy().replace('\\', "/")),
            None => format!("/{}", p.to_string_lossy()),
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automation_scripts_field() {
        let sd = PathBuf::from("/proj/ep01");
        let base = PathBuf::from("/home/u/.aegisub/automation");
        let v = decode_automation_scripts("~/scripts/a.lua|//abs/b.moon|$autoload/c.lua| |?bad", Some(&sd), Some(&base));
        assert_eq!(v, vec![sd.join("scripts/a.lua"), PathBuf::from("/abs/b.moon"), base.join("autoload/c.lua")]);
        let enc = encode_automation_scripts(&[sd.join("scripts/a.lua"), PathBuf::from("/abs/b.moon")], Some(&sd));
        assert_eq!(enc, "~/scripts/a.lua|//abs/b.moon");
        assert_eq!(decode_automation_scripts(&enc, Some(&sd), None), vec![sd.join("scripts/a.lua"), PathBuf::from("/abs/b.moon")]);
    }
    #[test]
    fn aegisub_dir_override() {
        let tmp = std::env::temp_dir().join(format!("osisub-aegi-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("include")).unwrap();
        std::env::set_var("OSISUB_AEGISUB_DIR", &tmp);
        assert_eq!(aegisub_automation_dirs(), vec![tmp.clone()]);
        assert!(automation_include_dirs().contains(&tmp.join("include")));
        std::env::set_var("OSISUB_AEGISUB_DIR", "");
        assert!(aegisub_automation_dirs().is_empty());
        std::env::remove_var("OSISUB_AEGISUB_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn dirs_end_with_osisub() {
        if let Some(d) = config_dir() {
            assert!(d.ends_with("osisub"));
        }
        if let Some(d) = data_dir() {
            assert!(d.ends_with("osisub"));
        }
    }
}
