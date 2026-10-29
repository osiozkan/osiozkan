//! `aegisub.dialog.display` iletişim kutularını egui ile çizer.
use eframe::egui::{self, DragValue, TextEdit};
use osisub_lua::{DialogResult, DialogSpec, DV};
use std::collections::BTreeMap;

pub struct DialogState {
    pub spec: DialogSpec,
    pub values: BTreeMap<String, DV>,
}

impl DialogState {
    pub fn new(spec: DialogSpec) -> Self {
        let values = spec.defaults().values;
        DialogState { spec, values }
    }
}

/// Düğmeye basılırsa `Some(sonuç)` döner.
pub fn show(ctx: &egui::Context, title: &str, st: &mut DialogState) -> Option<DialogResult> {
    let mut result = None;
    egui::Window::new(title).collapsible(false).resizable(true).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        let mut rows: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
        for (i, c) in st.spec.controls.iter().enumerate() {
            rows.entry(c.y).or_default().push(i);
        }
        for (_, mut idxs) in rows {
            idxs.sort_by_key(|&i| st.spec.controls[i].x);
            ui.horizontal(|ui| {
                for i in idxs {
                    let c = st.spec.controls[i].clone();
                    let w = (c.width.max(1) as f32 * 70.0).min(520.0);
                    let name = c.name.clone();
                    match c.class.as_str() {
                        "label" => {
                            ui.label(&c.label);
                        }
                        "edit" => {
                            if let Some(DV::Str(s)) = st.values.get_mut(&name) {
                                ui.add(TextEdit::singleline(s).desired_width(w).hint_text(&c.hint));
                            }
                        }
                        "textbox" => {
                            if let Some(DV::Str(s)) = st.values.get_mut(&name) {
                                ui.add(TextEdit::multiline(s).desired_width(w).desired_rows(c.height.max(2) as usize * 2));
                            }
                        }
                        "intedit" | "floatedit" | "alpha" => {
                            if let Some(DV::Num(n)) = st.values.get_mut(&name) {
                                let mut d = DragValue::new(n);
                                if c.class == "intedit" || c.class == "alpha" {
                                    d = d.fixed_decimals(0);
                                }
                                if let (Some(a), Some(b)) = (c.min, c.max) {
                                    d = d.range(a..=b);
                                }
                                if let Some(s) = c.step {
                                    d = d.speed(s);
                                }
                                ui.add(d);
                            }
                        }
                        "checkbox" => {
                            if let Some(DV::Bool(b)) = st.values.get_mut(&name) {
                                ui.checkbox(b, &c.label);
                            }
                        }
                        "dropdown" => {
                            if let Some(DV::Str(s)) = st.values.get_mut(&name) {
                                egui::ComboBox::from_id_salt(format!("dd{}", i)).selected_text(s.clone()).width(w).show_ui(ui, |ui| {
                                    for it in &c.items {
                                        ui.selectable_value(s, it.clone(), it);
                                    }
                                });
                            }
                        }
                        "color" | "coloralpha" => {
                            if let Some(DV::Str(s)) = st.values.get_mut(&name) {
                                let col = osisub_core::color::Color::parse(s).unwrap_or(osisub_core::color::Color::BLACK);
                                let mut c32 = egui::Color32::from_rgba_unmultiplied(col.r, col.g, col.b, 255 - col.a);
                                if ui.color_edit_button_srgba(&mut c32).changed() {
                                    let nc = osisub_core::color::Color { r: c32.r(), g: c32.g(), b: c32.b(), a: 255 - c32.a() };
                                    *s = if c.class == "color" { nc.to_tag_string() } else { nc.to_style_string() };
                                }
                                ui.monospace(s.as_str());
                            }
                        }
                        _ => {
                            if !c.label.is_empty() {
                                ui.label(&c.label);
                            }
                        }
                    }
                }
            });
        }
        ui.separator();
        ui.horizontal(|ui| {
            let buttons: Vec<String> = if st.spec.buttons.is_empty() { vec!["OK".into(), "Cancel".into()] } else { st.spec.buttons.clone() };
            for b in buttons {
                if ui.button(&b).clicked() {
                    let is_cancel = match &st.spec.cancel {
                        Some(c) => *c == b,
                        None => b.eq_ignore_ascii_case("cancel") || b.eq_ignore_ascii_case("iptal"),
                    };
                    result = Some(DialogResult { button: if is_cancel { None } else { Some(b) }, values: st.values.clone() });
                }
            }
        });
    });
    result
}
