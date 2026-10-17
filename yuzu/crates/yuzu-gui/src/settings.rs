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
}

impl Default for Settings {
    fn default() -> Self {
        Settings { script_dirs: crate::worker::default_script_dirs(), recent: vec![], dark: true, max_cps: 20.0, max_cpl: 42, fps: (24000, 1001), autosave_secs: 60 }
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
        let mut s = Settings::default();
        let Ok(txt) = std::fs::read_to_string(file()) else { return s };
        let mut dirs = Vec::new();
        for l in txt.lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            match k {
                "script_dir" => dirs.push(PathBuf::from(v)),
                "recent" => s.recent.push(PathBuf::from(v)),
                "dark" => s.dark = v == "1",
                "max_cps" => s.max_cps = v.parse().unwrap_or(20.0),
                "max_cpl" => s.max_cpl = v.parse().unwrap_or(42),
                "autosave" => s.autosave_secs = v.parse().unwrap_or(60),
                "fps" => if let Some((n, d)) = v.split_once('/') { if let (Ok(n), Ok(d)) = (n.parse(), d.parse()) { s.fps = (n, d); } },
                _ => {}
            }
        }
        if !dirs.is_empty() { s.script_dirs = dirs; }
        s
    }
    pub fn save(&self) {
        let mut o = String::new();
        for d in &self.script_dirs { o.push_str(&format!("script_dir={}\n", d.display())); }
        for r in self.recent.iter().take(12) { o.push_str(&format!("recent={}\n", r.display())); }
        o.push_str(&format!("dark={}\nmax_cps={}\nmax_cpl={}\nautosave={}\nfps={}/{}\n", self.dark as i32, self.max_cps, self.max_cpl, self.autosave_secs, self.fps.0, self.fps.1));
        let _ = std::fs::create_dir_all(config_dir());
        let _ = std::fs::write(file(), o);
    }
}
