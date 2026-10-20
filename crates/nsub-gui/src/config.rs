//! Persistent user settings in `~/.nightsub/config.ini` (simple `key=value` lines).

use crate::i18n::{self, Lang};
use nsub_core::qc::QcConfig;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub lang: Lang,
    pub dark: bool,
    pub qc: QcConfig,
    pub recent: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { lang: i18n::detect(), dark: true, qc: QcConfig::default(), recent: vec![] }
    }
}

pub fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".nightsub").join("config.ini"))
}

impl Config {
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "lang" => {
                    if let Some(l) = i18n::from_code(v) {
                        c.lang = l
                    }
                }
                "theme" => c.dark = v != "light",
                "qc.max_cps" => c.qc.max_cps = v.parse().unwrap_or(c.qc.max_cps),
                "qc.min_duration_ms" => c.qc.min_duration_ms = v.parse().unwrap_or(c.qc.min_duration_ms),
                "qc.max_duration_ms" => c.qc.max_duration_ms = v.parse().unwrap_or(c.qc.max_duration_ms),
                "qc.max_line_len" => c.qc.max_line_len = v.parse().unwrap_or(c.qc.max_line_len),
                "qc.max_lines" => c.qc.max_lines = v.parse().unwrap_or(c.qc.max_lines),
                "qc.min_gap_ms" => c.qc.min_gap_ms = v.parse().unwrap_or(c.qc.min_gap_ms),
                "recent" => c.recent.push(v.to_string()),
                _ => {}
            }
        }
        c
    }

    pub fn serialize(&self) -> String {
        let mut s = format!(
            "lang={}\ntheme={}\nqc.max_cps={}\nqc.min_duration_ms={}\nqc.max_duration_ms={}\nqc.max_line_len={}\nqc.max_lines={}\nqc.min_gap_ms={}\n",
            i18n::code(self.lang),
            if self.dark { "dark" } else { "light" },
            self.qc.max_cps,
            self.qc.min_duration_ms,
            self.qc.max_duration_ms,
            self.qc.max_line_len,
            self.qc.max_lines,
            self.qc.min_gap_ms
        );
        for r in self.recent.iter().take(10) {
            s.push_str(&format!("recent={r}\n"));
        }
        s
    }

    pub fn load() -> Config {
        path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Config::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, self.serialize());
        }
    }

    pub fn add_recent(&mut self, p: &str) {
        self.recent.retain(|r| r != p);
        self.recent.insert(0, p.to_string());
        self.recent.truncate(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut c = Config::default();
        c.lang = Lang::Tr;
        c.dark = false;
        c.qc.max_cps = 17.5;
        c.add_recent("/a.ass");
        c.add_recent("/b.ass");
        c.add_recent("/a.ass");
        let r = Config::parse(&c.serialize());
        assert_eq!(r.lang, Lang::Tr);
        assert!(!r.dark);
        assert_eq!(r.qc.max_cps, 17.5);
        assert_eq!(r.recent, vec!["/a.ass", "/b.ass"]);
    }
}
