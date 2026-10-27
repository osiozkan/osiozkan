//! Persistent settings in `~/.kaku/config.ini` (key=value, repeated `recent=` lines).

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub recent: Vec<PathBuf>,
    pub max_cps: f64,
    pub max_chars: usize,
    pub autosave_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config { recent: vec![], max_cps: 20.0, max_chars: 42, autosave_secs: 60 }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

pub fn data_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".kaku"))
}

impl Config {
    fn path() -> Option<PathBuf> {
        data_dir().map(|d| d.join("config.ini"))
    }

    pub fn load() -> Config {
        let mut c = Config::default();
        let Some(p) = Self::path() else { return c };
        let Ok(text) = std::fs::read_to_string(p) else { return c };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k.trim() {
                "recent" => c.recent.push(PathBuf::from(v.trim())),
                "max_cps" => c.max_cps = v.trim().parse().unwrap_or(c.max_cps),
                "max_chars" => c.max_chars = v.trim().parse().unwrap_or(c.max_chars),
                "autosave_secs" => c.autosave_secs = v.trim().parse().unwrap_or(c.autosave_secs),
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let mut s = format!("max_cps={}\nmax_chars={}\nautosave_secs={}\n", self.max_cps, self.max_chars, self.autosave_secs);
        for r in &self.recent {
            s.push_str(&format!("recent={}\n", r.display()));
        }
        let _ = std::fs::write(p, s);
    }

    pub fn push_recent(&mut self, p: &std::path::Path) {
        self.recent.retain(|r| r != p);
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(10);
        self.save();
    }
}

/// Autosave file path for a given document name.
pub fn autosave_path(name: &str) -> Option<PathBuf> {
    data_dir().map(|d| d.join("autosave").join(format!("{name}.autosave.ass")))
}
