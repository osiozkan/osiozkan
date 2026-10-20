use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Settings {
    pub script_dirs: Vec<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub dark: bool,
    pub max_cps: f64,
    pub max_cpl: usize,
    pub fps: (u32, u32),
    pub autosave_secs: u64,
    /// Interface language code ("tr", "en").
    pub lang: String,
    /// Interface zoom (egui zoom factor).
    pub ui_scale: f32,
    /// Shortcut overrides: command id → "Ctrl+A|Ctrl+B" ("" = none).
    pub shortcuts: Vec<(String, String)>,
    pub volume: f64,
    pub snap_keyframes: bool,
    pub follow: bool,
    pub show_spectrum: bool,
    pub tap_latency: i64,
    pub palette_recent: Vec<String>,
    /// Fields "Paste over" overwrites (`asscore::lines::field` bits).
    pub paste_over_fields: u16,
    /// Style storage shown in the styles manager.
    pub style_catalog: String,
    /// Reload automation scripts when a script or library file changes.
    pub auto_reload: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            script_dirs: crate::worker::default_script_dirs(), recent: vec![], dark: true, max_cps: 20.0, max_cpl: 42, fps: (24000, 1001), autosave_secs: 60,
            lang: "tr".into(), ui_scale: 1.0, shortcuts: vec![], volume: 100.0, snap_keyframes: true, follow: true, show_spectrum: false, tap_latency: 80,
            palette_recent: vec![], paste_over_fields: asscore::lines::field::TEXT, style_catalog: "Default".into(), auto_reload: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") { return PathBuf::from(x).join("yuzu"); }
    if let Some(h) = std::env::var_os("HOME") { return PathBuf::from(h).join(".config/yuzu"); }
    if let Some(a) = std::env::var_os("APPDATA") { return PathBuf::from(a).join("Yuzu"); }
    PathBuf::from(".yuzu")
}

fn file() -> PathBuf { config_dir().join("settings.txt") }

impl Settings {
    pub fn load() -> Settings {
        let Ok(txt) = std::fs::read_to_string(file()) else { return Settings::default() };
        Settings::parse(&txt)
    }

    pub fn parse(txt: &str) -> Settings {
        let mut s = Settings::default();
        let mut dirs = Vec::new();
        let b = |v: &str| v == "1";
        for l in txt.lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            match k {
                "script_dir" => dirs.push(PathBuf::from(v)),
                "recent" => s.recent.push(PathBuf::from(v)),
                "dark" => s.dark = b(v),
                "max_cps" => s.max_cps = v.parse().unwrap_or(20.0),
                "max_cpl" => s.max_cpl = v.parse().unwrap_or(42),
                "autosave" => s.autosave_secs = v.parse().unwrap_or(60),
                "fps" => if let Some((n, d)) = v.split_once('/') { if let (Ok(n), Ok(d)) = (n.parse(), d.parse()) { s.fps = (n, d); } },
                "lang" => s.lang = v.trim().to_string(),
                "ui_scale" => s.ui_scale = v.parse::<f32>().unwrap_or(1.0).clamp(0.5, 3.0),
                "shortcut" => if let Some((id, keys)) = v.split_once('=') { s.shortcuts.push((id.to_string(), keys.to_string())); },
                "volume" => s.volume = v.parse().unwrap_or(100.0),
                "snap_keyframes" => s.snap_keyframes = b(v),
                "follow" => s.follow = b(v),
                "spectrum" => s.show_spectrum = b(v),
                "tap_latency" => s.tap_latency = v.parse().unwrap_or(80),
                "palette_recent" => s.palette_recent.push(v.to_string()),
                "paste_over_fields" => s.paste_over_fields = v.parse().unwrap_or(asscore::lines::field::TEXT),
                "style_catalog" => s.style_catalog = v.to_string(),
                "auto_reload" => s.auto_reload = b(v),
                _ => {}
            }
        }
        if !dirs.is_empty() { s.script_dirs = dirs; }
        s
    }

    pub fn to_text(&self) -> String {
        let mut o = String::new();
        for d in &self.script_dirs { o.push_str(&format!("script_dir={}\n", d.display())); }
        for r in self.recent.iter().take(12) { o.push_str(&format!("recent={}\n", r.display())); }
        o.push_str(&format!("dark={}\nmax_cps={}\nmax_cpl={}\nautosave={}\nfps={}/{}\n", self.dark as i32, self.max_cps, self.max_cpl, self.autosave_secs, self.fps.0, self.fps.1));
        o.push_str(&format!("lang={}\nui_scale={}\nvolume={}\nsnap_keyframes={}\nfollow={}\nspectrum={}\ntap_latency={}\n",
            self.lang, self.ui_scale, self.volume, self.snap_keyframes as i32, self.follow as i32, self.show_spectrum as i32, self.tap_latency));
        for (id, keys) in &self.shortcuts { o.push_str(&format!("shortcut={}={}\n", id, keys)); }
        for r in self.palette_recent.iter().take(20) { o.push_str(&format!("palette_recent={}\n", r)); }
        o.push_str(&format!("paste_over_fields={}\nstyle_catalog={}\nauto_reload={}\n", self.paste_over_fields, self.style_catalog, self.auto_reload as i32));
        o
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        let _ = std::fs::write(file(), self.to_text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut s = Settings::default();
        s.lang = "en".into();
        s.ui_scale = 1.25;
        s.shortcuts = vec![("qc".into(), "Ctrl+Shift+Q".into()), ("delete".into(), String::new())];
        s.palette_recent = vec!["save".into(), "macro:Foo/Bar".into()];
        s.show_spectrum = true;
        s.volume = 55.0;
        s.paste_over_fields = 3;
        s.style_catalog = "Seri A".into();
        s.auto_reload = false;
        let p = Settings::parse(&s.to_text());
        assert!(!p.auto_reload);
        assert_eq!((p.paste_over_fields, p.style_catalog.as_str()), (3, "Seri A"));
        assert_eq!(p.lang, "en");
        assert_eq!(p.ui_scale, 1.25);
        assert_eq!(p.shortcuts, s.shortcuts);
        assert_eq!(p.palette_recent, s.palette_recent);
        assert!(p.show_spectrum);
        assert_eq!(p.volume, 55.0);
    }
}
