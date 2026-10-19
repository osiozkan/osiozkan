//! What a running Automation script can ask of its host (CLI, GUI, tests).
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum DialogValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Debug, Clone, Default)]
pub struct DialogField {
    pub class: String,
    pub name: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub hint: String,
    /// default/initial value (text for edit/textbox/color, number for intedit…)
    pub value: Option<DialogValue>,
    pub items: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Dialog {
    pub fields: Vec<DialogField>,
    /// labels of buttons; empty ⇒ default OK/Cancel
    pub buttons: Vec<String>,
    /// optional mapping `ok`/`cancel`/`apply`/`close`/`help` → button label
    pub button_ids: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct DialogResult {
    /// label of the clicked button (None = default OK button was used)
    pub button: Option<String>,
    pub values: HashMap<String, DialogValue>,
}

pub trait Host {
    /// level 0 = fatal, 1 = error, 2 = warning, 3 = hint, 4+ = debug
    fn log(&mut self, level: i32, msg: &str) {
        let tag = match level {
            0 => "FATAL",
            1 => "ERROR",
            2 => "WARN",
            3 => "HINT",
            _ => "DEBUG",
        };
        eprint!("[{tag}] {msg}");
        if !msg.ends_with('\n') {
            eprintln!();
        }
    }
    fn progress_set(&mut self, _percent: f64) {}
    fn progress_task(&mut self, _msg: &str) {}
    fn progress_title(&mut self, _msg: &str) {}
    fn is_cancelled(&self) -> bool {
        false
    }
    /// Show a dialog. Return None when the user cancels.
    fn dialog(&mut self, dialog: &Dialog) -> Option<DialogResult>;
    fn open_dialog(&mut self, _title: &str, _dir: &str, _file: &str, _wildcard: &str, _multiple: bool) -> Option<Vec<String>> {
        None
    }
    fn save_dialog(&mut self, _title: &str, _dir: &str, _file: &str, _wildcard: &str) -> Option<String> {
        None
    }
    fn fps(&self) -> Option<f64> {
        None
    }
    fn keyframes(&self) -> Option<Vec<i64>> {
        None
    }
    fn video_size(&self) -> Option<(i32, i32)> {
        None
    }
    fn script_path(&self) -> Option<PathBuf> {
        None
    }
    fn video_path(&self) -> Option<PathBuf> {
        None
    }
    fn audio_path(&self) -> Option<PathBuf> {
        None
    }
    fn audio_selection(&self) -> Option<(i64, i64)> {
        None
    }
    fn cursor_ms(&self) -> Option<i64> {
        None
    }
    /// Clipboard text (`aegisub.clipboard.get`). The default is a process-local buffer;
    /// GUI hosts should override both methods to reach the system clipboard.
    fn clipboard_get(&self) -> Option<String> {
        FALLBACK_CLIPBOARD.with(|c| c.borrow().clone())
    }
    fn clipboard_set(&mut self, text: &str) -> bool {
        FALLBACK_CLIPBOARD.with(|c| *c.borrow_mut() = Some(text.to_string()));
        true
    }
}

thread_local! {
    static FALLBACK_CLIPBOARD: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Non-interactive host used by the CLI and tests: dialogs return their
/// defaults, optionally overridden by `answers` (field name → text).
#[derive(Default)]
pub struct Headless {
    pub answers: HashMap<String, String>,
    pub button: Option<String>,
    pub fps: Option<f64>,
    pub keyframes: Vec<i64>,
    pub video_size: Option<(i32, i32)>,
    pub script_path: Option<PathBuf>,
    pub quiet: bool,
    pub log_buf: Vec<String>,
}

impl Host for Headless {
    fn log(&mut self, level: i32, msg: &str) {
        self.log_buf.push(msg.to_string());
        if !self.quiet {
            let tag = ["FATAL", "ERROR", "WARN", "HINT"].get(level as usize).copied().unwrap_or("DEBUG");
            eprint!("[{tag}] {msg}");
            if !msg.ends_with('\n') {
                eprintln!();
            }
        }
    }

    fn dialog(&mut self, d: &Dialog) -> Option<DialogResult> {
        let mut values = HashMap::new();
        for f in &d.fields {
            if f.name.is_empty() || matches!(f.class.as_str(), "label") {
                continue;
            }
            let mut v = f.value.clone();
            if let Some(ans) = self.answers.get(&f.name) {
                v = Some(match f.class.as_str() {
                    "intedit" | "floatedit" | "slider" => DialogValue::Num(ans.parse().unwrap_or(0.0)),
                    "checkbox" => DialogValue::Bool(matches!(ans.as_str(), "1" | "true" | "yes" | "on")),
                    _ => DialogValue::Str(ans.clone()),
                });
            }
            let v = v.unwrap_or(match f.class.as_str() {
                "intedit" | "floatedit" | "slider" => DialogValue::Num(f.min.unwrap_or(0.0).max(0.0).min(f.max.unwrap_or(f64::MAX))),
                "checkbox" => DialogValue::Bool(false),
                "dropdown" => DialogValue::Str(f.items.first().cloned().unwrap_or_default()),
                _ => DialogValue::Str(String::new()),
            });
            values.insert(f.name.clone(), v);
        }
        let button = self.button.clone().or_else(|| d.button_ids.get("ok").cloned()).or_else(|| d.buttons.first().cloned());
        Some(DialogResult { button, values })
    }

    fn fps(&self) -> Option<f64> {
        self.fps
    }
    fn keyframes(&self) -> Option<Vec<i64>> {
        if self.keyframes.is_empty() {
            None
        } else {
            Some(self.keyframes.clone())
        }
    }
    fn video_size(&self) -> Option<(i32, i32)> {
        self.video_size
    }
    fn script_path(&self) -> Option<PathBuf> {
        self.script_path.clone()
    }
}
