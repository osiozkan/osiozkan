//! Kalıcı ayarlar (~/.config/osisub/settings.json).
use osisub_core::qc::Rules;
use osisub_core::timing::PostOptions;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<String>,
    pub max_cps: f64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub min_gap_ms: i64,
    pub snap_frames: bool,
    pub snap_keyframes: bool,
    pub lead_in_ms: i64,
    pub lead_out_ms: i64,
    pub adjacent_gap_ms: i64,
    pub fps: f64,
    pub light_theme: bool,
    pub zoom: f32,
    pub glossary_path: String,
    pub dict_aff: String,
    pub dict_dic: String,
    /// Lua makro kısayolları: (tuş birleşimi "Ctrl+Shift+K", betik adı, makro adı)
    pub macro_hotkeys: Vec<MacroHotkey>,
    /// Izgara sütunları (Aegisub gibi): katman, efekt, kenar boşlukları
    pub grid_layer: bool,
    pub grid_effect: bool,
    pub grid_margins: bool,
    /// Elle yüklenen (otomatik yükleme klasörü dışındaki) betikler; açılışta yeniden yüklenir
    pub scripts: Vec<String>,
    /// Kurulu Aegisub'ın automation/autoload betiklerini de yükle
    pub aegisub_autoload: bool,
    /// Oynatma hızı (0.25–2; perde korunur) ve ses seviyesi (0–2)
    pub play_speed: f64,
    pub volume: f64,
    /// Uygulanan varsayılan kısayol sürümü (yeni varsayılanlar bir kez eklenir; kullanıcı silerse geri gelmez)
    pub hotkey_defaults: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MacroHotkey {
    pub keys: String,
    pub script: String,
    pub name: String,
}

impl Default for Settings {
    fn default() -> Self {
        let r = Rules::default();
        let p = PostOptions::default();
        Settings {
            recent: vec![],
            max_cps: r.max_cps,
            max_line_len: r.max_line_len,
            max_lines: r.max_lines,
            min_duration_ms: r.min_duration_ms,
            max_duration_ms: r.max_duration_ms,
            min_gap_ms: r.min_gap_ms,
            snap_frames: true,
            snap_keyframes: true,
            lead_in_ms: p.lead_in_ms,
            lead_out_ms: p.lead_out_ms,
            adjacent_gap_ms: p.adjacent_gap_ms,
            fps: 23.976,
            light_theme: false,
            zoom: 1.0,
            glossary_path: String::new(),
            dict_aff: String::new(),
            dict_dic: String::new(),
            macro_hotkeys: vec![],
            grid_layer: true,
            grid_effect: true,
            grid_margins: false,
            scripts: vec![],
            aegisub_autoload: true,
            play_speed: 1.0,
            volume: 1.0,
            hotkey_defaults: 0,
        }
    }
}

fn path() -> Option<PathBuf> {
    osisub_core::paths::config_dir().map(|d| d.join("settings.json"))
}

impl Settings {
    pub fn load() -> Settings {
        path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(t) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, t);
            }
        }
    }

    pub fn push_recent(&mut self, p: &str) {
        self.recent.retain(|x| x != p);
        self.recent.insert(0, p.to_string());
        self.recent.truncate(10);
    }

    pub fn rules(&self) -> Rules {
        Rules { min_duration_ms: self.min_duration_ms, max_duration_ms: self.max_duration_ms, max_cps: self.max_cps, max_line_len: self.max_line_len, max_lines: self.max_lines, min_gap_ms: self.min_gap_ms }
    }
}
