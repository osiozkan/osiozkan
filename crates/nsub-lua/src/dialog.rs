//! Data types for Aegisub-style dialogs, so a host (GUI) can show them.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    None,
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Clone, Debug)]
pub struct Control {
    pub class: String,
    pub name: String,
    pub label: String,
    pub hint: String,
    pub value: Val,
    pub items: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default)]
pub struct DialogRequest {
    pub controls: Vec<Control>,
    /// Empty means the default OK / Cancel pair.
    pub buttons: Vec<String>,
    /// Aegisub button ids: `ok` / `cancel` labels (or None).
    pub ok_id: Option<String>,
    pub cancel_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DialogResponse {
    /// Pressed button label; `None` = dialog cancelled/closed.
    pub button: Option<String>,
    pub values: HashMap<String, Val>,
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub fraction: Option<f64>,
    pub task: Option<String>,
    pub title: Option<String>,
}

pub type DialogHandler = std::rc::Rc<dyn Fn(DialogRequest) -> DialogResponse>;
pub type ProgressHandler = std::rc::Rc<dyn Fn(Progress)>;
