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
    /// ikinci (ek) sözlük, ör. en_US — boşsa yok
    pub dict2_aff: String,
    pub dict2_dic: String,
    /// Lua makro kısayolları: (tuş birleşimi "Ctrl+Shift+K", betik adı, makro adı)
    pub macro_hotkeys: Vec<MacroHotkey>,
    /// Izgara sütunları (Aegisub gibi): katman, efekt, kenar boşlukları
    pub grid_layer: bool,
    pub grid_effect: bool,
    pub grid_margins: bool,
    /// Izgara sütun sırası (kimlikler; `#` başta, Metin sonda sabit) ve gizlenen sütunlar
    pub grid_order: Vec<String>,
    pub grid_hidden: Vec<String>,
    /// Elle yüklenen (otomatik yükleme klasörü dışındaki) betikler; açılışta yeniden yüklenir
    pub scripts: Vec<String>,
    /// Kurulu Aegisub'ın automation/autoload betiklerini de yükle
    pub aegisub_autoload: bool,
    /// Oynatma hızı (0.25–2; perde korunur) ve ses seviyesi (0–2)
    pub play_speed: f64,
    pub volume: f64,
    /// Uygulanan varsayılan kısayol sürümü (yeni varsayılanlar bir kez eklenir; kullanıcı silerse geri gelmez)
    pub hotkey_defaults: u32,
    /// Makine çevirisi: sağlayıcı ("deepl" | "google" | "libre" | "claude"), anahtar, sunucu, diller, bağlam
    pub mt_provider: String,
    pub mt_key: String,
    pub mt_url: String,
    pub mt_target: String,
    pub mt_source: String,
    pub mt_context: String,
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
            dict2_aff: String::new(),
            dict2_dic: String::new(),
            macro_hotkeys: vec![],
            grid_layer: true,
            grid_effect: true,
            grid_margins: false,
            grid_order: vec![],
            grid_hidden: vec!["len".into()],
            scripts: vec![],
            aegisub_autoload: true,
            play_speed: 1.0,
            volume: 1.0,
            hotkey_defaults: 0,
            mt_provider: "deepl".into(),
            mt_key: String::new(),
            mt_url: "http://localhost:5000".into(),
            mt_target: "tr".into(),
            mt_source: String::new(),
            mt_context: String::new(),
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

/// Izgaranın taşınabilir/gizlenebilir sütunları (`#` ve Metin her zaman görünür).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridCol {
    Layer,
    Start,
    End,
    Dur,
    Cps,
    Len,
    Style,
    Actor,
    Effect,
    Margins,
}

impl GridCol {
    pub const ALL: [GridCol; 10] = [GridCol::Layer, GridCol::Start, GridCol::End, GridCol::Dur, GridCol::Cps, GridCol::Len, GridCol::Style, GridCol::Actor, GridCol::Effect, GridCol::Margins];

    pub fn id(self) -> &'static str {
        match self {
            GridCol::Layer => "layer",
            GridCol::Start => "start",
            GridCol::End => "end",
            GridCol::Dur => "dur",
            GridCol::Cps => "cps",
            GridCol::Len => "len",
            GridCol::Style => "style",
            GridCol::Actor => "actor",
            GridCol::Effect => "effect",
            GridCol::Margins => "margins",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GridCol::Layer => "Katman",
            GridCol::Start => "Başlangıç",
            GridCol::End => "Bitiş",
            GridCol::Dur => "Süre",
            GridCol::Cps => "KSS (okuma hızı)",
            GridCol::Len => "Uzunluk (en uzun satır)",
            GridCol::Style => "Stil",
            GridCol::Actor => "Konuşan",
            GridCol::Effect => "Efekt",
            GridCol::Margins => "Kenar boşlukları (Sol/Sağ/Dik)",
        }
    }

    fn from_id(s: &str) -> Option<GridCol> {
        GridCol::ALL.into_iter().find(|c| c.id() == s)
    }
}

impl Settings {
    /// Kayıtlı sıraya göre tüm sütunlar (bilinmeyenler atılır, eksikler sona eklenir).
    pub fn grid_columns(&self) -> Vec<GridCol> {
        let mut v: Vec<GridCol> = Vec::new();
        for id in &self.grid_order {
            if let Some(c) = GridCol::from_id(id) {
                if !v.contains(&c) {
                    v.push(c);
                }
            }
        }
        // Kayıtta olmayanlar (ör. yeni sürümde eklenen sütun) sona
        for c in GridCol::ALL {
            if !v.contains(&c) {
                v.push(c);
            }
        }
        v
    }

    pub fn grid_visible(&self, c: GridCol) -> bool {
        match c {
            GridCol::Layer => self.grid_layer,
            GridCol::Effect => self.grid_effect,
            GridCol::Margins => self.grid_margins,
            _ => !self.grid_hidden.iter().any(|h| h == c.id()),
        }
    }

    pub fn set_grid_visible(&mut self, c: GridCol, on: bool) {
        match c {
            GridCol::Layer => self.grid_layer = on,
            GridCol::Effect => self.grid_effect = on,
            GridCol::Margins => self.grid_margins = on,
            _ => {
                self.grid_hidden.retain(|h| h != c.id());
                if !on {
                    self.grid_hidden.push(c.id().into());
                }
            }
        }
    }

    /// Sütunu `delta` (−1 / +1) kadar taşır.
    pub fn move_grid_column(&mut self, c: GridCol, delta: i32) {
        let mut v = self.grid_columns();
        let Some(i) = v.iter().position(|x| *x == c) else { return };
        let j = i as i32 + delta;
        if j < 0 || j as usize >= v.len() {
            return;
        }
        v.swap(i, j as usize);
        self.grid_order = v.iter().map(|c| c.id().to_string()).collect();
    }
}

#[cfg(test)]
mod grid_tests {
    use super::*;

    #[test]
    fn column_order_and_visibility() {
        let mut s = Settings::default();
        assert_eq!(s.grid_columns(), GridCol::ALL.to_vec());
        assert!(!s.grid_visible(GridCol::Len) && s.grid_visible(GridCol::Layer) && !s.grid_visible(GridCol::Margins));
        s.move_grid_column(GridCol::Actor, -1);
        s.move_grid_column(GridCol::Actor, -1);
        s.move_grid_column(GridCol::Layer, -1); // zaten başta
        let ids: Vec<&str> = s.grid_columns().iter().map(|c| c.id()).collect();
        assert_eq!(ids, ["layer", "start", "end", "dur", "cps", "actor", "len", "style", "effect", "margins"]);
        // eski/bozuk ayar: bilinmeyen ve eksik kimlikler
        s.grid_order = vec!["style".into(), "yok".into(), "start".into()];
        let ids: Vec<&str> = s.grid_columns().iter().map(|c| c.id()).collect();
        assert_eq!(ids, ["style", "start", "layer", "end", "dur", "cps", "len", "actor", "effect", "margins"]);
        s.set_grid_visible(GridCol::Cps, false);
        s.set_grid_visible(GridCol::Len, true);
        assert!(!s.grid_visible(GridCol::Cps) && s.grid_visible(GridCol::Len));
    }
}
