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

#[cfg(test)]
mod tests {
    use super::*;
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
