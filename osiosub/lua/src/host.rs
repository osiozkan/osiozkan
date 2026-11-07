//! Interface between the Lua runtime and the application (dialogs, metrics, video).
use osiosub_core::ass::Style;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum DlgVal { Str(String), Num(f64), Bool(bool) }

#[derive(Clone, Debug, Default)]
pub struct DialogControl {
    pub class: String,
    pub name: String,
    pub x: i32, pub y: i32, pub width: i32, pub height: i32,
    pub label: String,
    pub text: String,
    pub hint: String,
    pub value: Option<DlgVal>,
    pub min: Option<f64>, pub max: Option<f64>, pub step: Option<f64>,
    pub items: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DialogSpec {
    pub controls: Vec<DialogControl>,
    pub buttons: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DialogResult {
    /// Pressed button label; `None` = cancelled.
    pub button: Option<String>,
    pub values: BTreeMap<String, DlgVal>,
}

pub trait Host {
    /// Show a dialog. Default: "press first button with the defaults".
    fn dialog(&mut self, spec: &DialogSpec) -> DialogResult {
        let mut values = BTreeMap::new();
        for c in &spec.controls {
            if c.name.is_empty() { continue; }
            let v = match c.class.as_str() {
                "checkbox" => Some(c.value.clone().unwrap_or(DlgVal::Bool(false))),
                "dropdown" => Some(c.value.clone().unwrap_or_else(|| DlgVal::Str(c.items.first().cloned().unwrap_or_default()))),
                "label" => None,
                "intedit" | "floatedit" | "slider" => Some(c.value.clone().unwrap_or(DlgVal::Num(c.min.unwrap_or(0.0)))),
                _ => Some(c.value.clone().unwrap_or_else(|| DlgVal::Str(c.text.clone()))),
            };
            if let Some(v) = v { values.insert(c.name.clone(), v); }
        }
        let button = spec.buttons.first().cloned().or(Some("OK".into()));
        DialogResult { button, values }
    }
    /// Returns (width, height, descent, external_leading) in script pixels.
    fn text_extents(&self, style: &Style, text: &str) -> (f64, f64, f64, f64) {
        crate::fonts::extents(style, text).unwrap_or_else(|| approx_extents(style, text))
    }
    fn log(&mut self, msg: &str) { eprint!("{}", msg); }
    fn video_size(&self) -> Option<(i32, i32)> { None }
    fn fps(&self) -> Option<f64> { None }
    fn keyframes(&self) -> Vec<i64> { vec![] }
    fn clipboard_get(&self) -> Option<String> { None }
    fn clipboard_set(&mut self, _s: &str) -> bool { false }
    fn progress(&mut self, _percent: f64, _task: &str, _title: &str) {}
}

pub struct DefaultHost;
impl Host for DefaultHost {}

/// Rough metrics when no font engine is available.
pub fn approx_extents(style: &Style, text: &str) -> (f64, f64, f64, f64) {
    let fs = style.fontsize;
    let mut w = 0.0;
    for c in text.chars() {
        let f = match c {
            ' ' => 0.28, 'i' | 'l' | 'j' | '.' | ',' | '\'' | '!' | ':' | ';' | '|' => 0.26,
            'm' | 'w' | 'M' | 'W' => 0.85,
            c if c.is_uppercase() => 0.66,
            c if (c as u32) > 0x2E80 => 1.0,
            _ => 0.52,
        };
        w += fs * f * if style.bold { 1.06 } else { 1.0 };
    }
    w = w * style.scale_x / 100.0 + style.spacing * text.chars().count() as f64;
    (w, fs * style.scale_y / 100.0, fs * 0.2, 0.0)
}
