//! Persistent user settings (`~/.config/subforge/settings.json`): Automation autoload/include
//! directories and user-added scripts, so the owner's Lua scripts are there on every launch.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Settings {
    /// directories scanned for `*.lua` macros at startup (Aegisub's automation/autoload equivalent)
    #[serde(default)]
    pub autoload_dirs: Vec<String>,
    /// extra `include()` / `require` search directories (Aegisub's automation/include)
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// individually added scripts
    #[serde(default)]
    pub scripts: Vec<String>,
    /// extra directories searched for fonts
    #[serde(default)]
    pub font_dirs: Vec<String>,
}

pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SUBFORGE_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        return Path::new(&x).join("subforge");
    }
    if let Some(a) = std::env::var_os("APPDATA") {
        return Path::new(&a).join("subforge");
    }
    std::env::var_os("HOME").map(|h| Path::new(&h).join(".config/subforge")).unwrap_or_else(|| PathBuf::from(".subforge"))
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(config_dir().join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("settings.json"), serde_json::to_string_pretty(self).unwrap())
    }

    /// All include dirs including the built-in `<config>/include`.
    pub fn all_include_dirs(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.include_dirs.iter().map(PathBuf::from).collect();
        v.push(config_dir().join("include"));
        for d in &self.autoload_dirs {
            v.push(Path::new(d).join("include"));
        }
        v
    }

    /// Script files from autoload dirs (+ `<config>/autoload`) and individually added scripts.
    pub fn script_files(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.autoload_dirs.iter().map(PathBuf::from).collect();
        dirs.push(config_dir().join("autoload"));
        let mut out: Vec<PathBuf> = Vec::new();
        for d in dirs {
            if let Ok(rd) = std::fs::read_dir(&d) {
                let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().map(|x| x.eq_ignore_ascii_case("lua")).unwrap_or(false)).collect();
                v.sort();
                out.extend(v);
            }
        }
        for s in &self.scripts {
            let p = PathBuf::from(s);
            if p.is_file() && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_scan() {
        let tmp = std::env::temp_dir().join(format!("sf-settings-{}", std::process::id()));
        let auto = tmp.join("autoload");
        std::fs::create_dir_all(&auto).unwrap();
        std::fs::write(auto.join("a.lua"), "x=1").unwrap();
        std::fs::write(auto.join("b.txt"), "no").unwrap();
        // SAFETY: test-only env mutation, single test touches it
        unsafe { std::env::set_var("SUBFORGE_CONFIG_DIR", tmp.join("cfg")) };
        let s = Settings { autoload_dirs: vec![auto.display().to_string()], scripts: vec![auto.join("a.lua").display().to_string()], ..Default::default() };
        s.save().unwrap();
        let l = Settings::load();
        assert_eq!(l, s);
        let files = l.script_files();
        assert_eq!(files.len(), 1, "{:?}", files); // a.lua only once
        assert!(l.all_include_dirs().iter().any(|p| p.ends_with("include")));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
