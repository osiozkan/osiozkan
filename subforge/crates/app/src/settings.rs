//! Persistent user settings (~/.config/subforge/settings.json).
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use subforge_core::qc::QcConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<String>,
    pub automation_dirs: Vec<String>,
    pub qc: QcConfig,
    pub dark: bool,
    pub ui_scale: f32,
    pub autosave_secs: u64,
    pub default_fps: f64,
    pub grid_font: f32,
    pub lang: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            recent: vec![],
            automation_dirs: vec![],
            qc: QcConfig::default(),
            dark: true,
            ui_scale: 1.0,
            autosave_secs: 60,
            default_fps: 23.976,
            grid_font: 14.0,
            lang: "tr".into(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("SUBFORGE_CONFIG") {
        return PathBuf::from(d);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("APPDATA").map(PathBuf::from))
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("subforge")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(config_dir().join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let d = config_dir();
        let _ = std::fs::create_dir_all(&d);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(d.join("settings.json"), s);
        }
    }

    pub fn push_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(12);
    }

    /// Roots that may contain `autoload/` and `include/` (SubForge's own plus any
    /// Aegisub installation/profile folders found on this machine, and user-added ones).
    pub fn automation_roots(&self) -> Vec<PathBuf> {
        let mut v = vec![config_dir().join("automation")];
        let home = std::env::var("HOME").map(PathBuf::from).ok();
        let mut guesses: Vec<PathBuf> = vec![];
        if let Ok(a) = std::env::var("APPDATA") {
            guesses.push(PathBuf::from(a).join("Aegisub").join("automation"));
        }
        if let Some(h) = &home {
            guesses.push(h.join(".aegisub").join("automation"));
            guesses.push(h.join("Library/Application Support/Aegisub/automation"));
            guesses.push(h.join(".config/Aegisub/automation"));
        }
        guesses.push(PathBuf::from("C:\\Program Files\\Aegisub\\automation"));
        guesses.push(PathBuf::from("C:\\Program Files (x86)\\Aegisub\\automation"));
        guesses.push(PathBuf::from("/usr/share/aegisub/automation"));
        guesses.push(PathBuf::from("/Applications/Aegisub.app/Contents/SharedSupport/automation"));
        v.extend(guesses.into_iter().filter(|p| p.is_dir()));
        v.extend(self.automation_dirs.iter().map(PathBuf::from));
        v
    }

    /// Folders scanned for `*.lua` scripts: each root, `<root>/autoload`.
    pub fn script_dirs(&self) -> Vec<PathBuf> {
        let mut out = vec![];
        for r in self.automation_roots() {
            out.push(r.clone());
            out.push(r.join("autoload"));
        }
        out.dedup();
        out
    }

    /// Search path for `include()` / `require`: each root, `<root>/include`.
    pub fn include_dirs(&self) -> Vec<PathBuf> {
        let mut out = vec![];
        for r in self.automation_roots() {
            out.push(r.clone());
            out.push(r.join("include"));
        }
        out
    }
}
