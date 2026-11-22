//! `aegisub.dialog.display` için arayüzden bağımsız iletişim kutusu modeli.
use mlua::{Table, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum DV {
    Bool(bool),
    Num(f64),
    Str(String),
}

#[derive(Clone, Debug, Default)]
pub struct Control {
    pub class: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub label: String,
    pub text: String,
    pub value: Option<DV>,
    pub items: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub hint: String,
}

#[derive(Clone, Debug, Default)]
pub struct DialogSpec {
    pub controls: Vec<Control>,
    /// Boşsa Aegisub varsayılanı: OK / Cancel.
    pub buttons: Vec<String>,
    pub ok: Option<String>,
    pub cancel: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DialogResult {
    /// `None` = iptal.
    pub button: Option<String>,
    pub values: BTreeMap<String, DV>,
}

fn s(t: &Table, k: &str) -> String {
    match t.get::<Value>(k) {
        Ok(Value::String(v)) => v.to_str().map(|x| x.to_string()).unwrap_or_default(),
        Ok(Value::Number(n)) => n.to_string(),
        Ok(Value::Integer(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn n(t: &Table, k: &str) -> Option<f64> {
    match t.get::<Value>(k) {
        Ok(Value::Number(n)) => Some(n),
        Ok(Value::Integer(n)) => Some(n as f64),
        _ => None,
    }
}

fn dv(v: Value) -> Option<DV> {
    match v {
        Value::Boolean(b) => Some(DV::Bool(b)),
        Value::Number(x) => Some(DV::Num(x)),
        Value::Integer(x) => Some(DV::Num(x as f64)),
        Value::String(x) => Some(DV::Str(x.to_str().ok()?.to_string())),
        _ => None,
    }
}

impl DialogSpec {
    pub fn from_lua(dialog: &Table, buttons: Option<&Table>, ids: Option<&Table>) -> mlua::Result<DialogSpec> {
        let mut spec = DialogSpec::default();
        for item in dialog.sequence_values::<Table>() {
            let t = item?;
            let class = s(&t, "class");
            let mut c = Control {
                class: class.clone(),
                name: s(&t, "name"),
                x: n(&t, "x").unwrap_or(0.0) as i32,
                y: n(&t, "y").unwrap_or(0.0) as i32,
                width: n(&t, "width").unwrap_or(1.0) as i32,
                height: n(&t, "height").unwrap_or(1.0) as i32,
                label: s(&t, "label"),
                text: s(&t, "text"),
                value: dv(t.get::<Value>("value").unwrap_or(Value::Nil)),
                min: n(&t, "min"),
                max: n(&t, "max"),
                step: n(&t, "step"),
                hint: s(&t, "hint"),
                items: vec![],
            };
            if let Ok(Value::Table(items)) = t.get::<Value>("items") {
                c.items = items.sequence_values::<String>().filter_map(|x| x.ok()).collect();
            }
            // Sınıfa göre tür ve varsayılanlar (Aegisub auto4_lua_dialog.cpp)
            let raw_value = t.get::<Value>("value").unwrap_or(Value::Nil);
            let has_text = !matches!(t.get::<Value>("text"), Ok(Value::Nil) | Err(_));
            match class.as_str() {
                // value dizeye çevrilir; ikisi de varsa text kazanır
                "edit" | "textbox" => c.value = Some(DV::Str(if has_text { c.text.clone() } else { s(&t, "value") })),
                "intedit" | "floatedit" => {
                    let int = class == "intedit";
                    let mut v = match raw_value { Value::Integer(x) => x as f64, Value::Number(x) => x, _ => 0.0 };
                    // min >= max: sınırsız
                    if let (Some(lo), Some(hi)) = (c.min, c.max) {
                        if lo >= hi { c.min = None; c.max = None; }
                    }
                    if int { v = v.trunc(); c.min = c.min.map(f64::trunc); c.max = c.max.map(f64::trunc); }
                    if let Some(lo) = c.min { v = v.max(lo); }
                    if let Some(hi) = c.max { v = v.min(hi); }
                    c.value = Some(DV::Num(v));
                }
                // yalnızca gerçek boolean kabul edilir (value=1 → false)
                "checkbox" => c.value = Some(DV::Bool(matches!(raw_value, Value::Boolean(true)))),
                "dropdown" | "alpha" => c.value = Some(DV::Str(s(&t, "value"))),
                "color" | "coloralpha" => c.value = Some(DV::Str(match s(&t, "value") { v if v.is_empty() => "&H000000&".into(), v => v })),
                _ => {}
            }
            spec.controls.push(c);
        }
        if let Some(b) = buttons {
            spec.buttons = b.sequence_values::<String>().filter_map(|x| x.ok()).collect();
        }
        if let Some(i) = ids {
            let g = |k: &str| match i.get::<Value>(k) {
                Ok(Value::String(v)) => v.to_str().ok().map(|x| x.to_string()),
                _ => None,
            };
            spec.ok = g("ok");
            spec.cancel = g("cancel");
        }
        Ok(spec)
    }

    /// Hiç kullanıcı girdisi olmadan (betik/CLI) varsayılan sonuç.
    pub fn defaults(&self) -> DialogResult {
        let mut values = BTreeMap::new();
        for c in &self.controls {
            if !c.name.is_empty() {
                if let Some(v) = &c.value {
                    values.insert(c.name.clone(), v.clone());
                }
            }
        }
        // varsayılan düğme: ok kimliği, yoksa iptal olmayan ilk düğme
        let button = self.ok.clone().filter(|o| self.buttons.contains(o)).or_else(|| self.buttons.iter().find(|b| Some(*b) != self.cancel.as_ref()).cloned()).or(Some("OK".into()));
        DialogResult { button, values }
    }
}
