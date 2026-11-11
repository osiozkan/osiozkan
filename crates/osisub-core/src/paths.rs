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

#[cfg(test)]
mod tests {
    use super::*;
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
