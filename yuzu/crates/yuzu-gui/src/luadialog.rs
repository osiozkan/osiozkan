//! Renders an Aegisub `aegisub.dialog.display` spec as an egui window.

use eframe::egui::{self, Color32};
use std::collections::BTreeMap;
use yuzu_lua::{DValue, DialogControl, DialogSpec};

pub struct LuaDialogState {
    pub spec: DialogSpec,
    pub values: BTreeMap<String, DValue>,
}

impl LuaDialogState {
    pub fn new(spec: DialogSpec) -> Self {
        let values = yuzu_lua::default_dialog_values(&spec).into_iter().collect();
        LuaDialogState { spec, values }
    }
}

fn parse_color(s: &str) -> Color32 {
    // "#RRGGBB", "#RRGGBBAA" or "&HAABBGGRR"
    let t = s.trim();
    if let Some(h) = t.strip_prefix('#') {
        if h.len() >= 6 {
            let v = u32::from_str_radix(&h[..6], 16).unwrap_or(0);
            return Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        }
    }
    if let Some(c) = asscore::Color::parse(t) { return Color32::from_rgb(c.r, c.g, c.b); }
    Color32::WHITE
}

/// Returns Some(button) when a button was pressed (None → keep open).
pub fn show(ctx: &egui::Context, st: &mut LuaDialogState, title: &str) -> Option<String> {
    let mut pressed = None;
    let mut ctrls: Vec<&DialogControl> = st.spec.controls.iter().collect();
    ctrls.sort_by_key(|c| (c.y, c.x));
    let spec = st.spec.clone();
    egui::Window::new(title).collapsible(false).resizable(true).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        let mut rows: Vec<Vec<&DialogControl>> = Vec::new();
        for c in &ctrls {
            match rows.last_mut() {
                Some(r) if r[0].y == c.y => r.push(c),
                _ => rows.push(vec![c]),
            }
        }
        egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
            for r in rows {
                ui.horizontal_wrapped(|ui| {
                    for c in r {
                        control(ui, c, &mut st.values);
                    }
                });
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            for b in &spec.buttons {
                if ui.button(b).clicked() { pressed = Some(b.clone()); }
            }
        });
    });
    pressed
}

fn control(ui: &mut egui::Ui, c: &DialogControl, vals: &mut BTreeMap<String, DValue>) {
    let width = (c.width as f32 * 70.0).clamp(60.0, 520.0);
    match c.class.as_str() {
        "label" => { ui.label(&c.label); }
        "hyperlink" => { ui.hyperlink_to(&c.label, &c.text); }
        "image" => {}
        "checkbox" => {
            let mut v = matches!(vals.get(&c.name), Some(DValue::Bool(true)));
            if ui.checkbox(&mut v, &c.label).on_hover_text(&c.hint).changed() { vals.insert(c.name.clone(), DValue::Bool(v)); }
        }
        "intedit" => {
            let mut v = match vals.get(&c.name) { Some(DValue::Num(n)) => *n, _ => 0.0 };
            let (lo, hi) = (c.min.unwrap_or(-2_147_483_648.0), c.max.unwrap_or(2_147_483_647.0));
            if ui.add(egui::DragValue::new(&mut v).range(lo..=hi).speed(1.0).max_decimals(0)).on_hover_text(&c.hint).changed() { vals.insert(c.name.clone(), DValue::Num(v.round())); }
        }
        "floatedit" => {
            let mut v = match vals.get(&c.name) { Some(DValue::Num(n)) => *n, _ => 0.0 };
            let (lo, hi) = (c.min.unwrap_or(f64::MIN), c.max.unwrap_or(f64::MAX));
            let speed = c.step.unwrap_or(0.1);
            if ui.add(egui::DragValue::new(&mut v).range(lo..=hi).speed(speed)).on_hover_text(&c.hint).changed() { vals.insert(c.name.clone(), DValue::Num(v)); }
        }
        "slider" => {
            let mut v = match vals.get(&c.name) { Some(DValue::Num(n)) => *n, _ => 0.0 };
            if ui.add(egui::Slider::new(&mut v, c.min.unwrap_or(0.0)..=c.max.unwrap_or(100.0))).changed() { vals.insert(c.name.clone(), DValue::Num(v)); }
        }
        "dropdown" => {
            let mut cur = match vals.get(&c.name) { Some(DValue::Str(s)) => s.clone(), _ => String::new() };
            egui::ComboBox::from_id_salt(("dd", &c.name, c.x, c.y)).selected_text(cur.clone()).width(width.min(300.0)).show_ui(ui, |ui| {
                for it in &c.items { ui.selectable_value(&mut cur, it.clone(), it); }
            });
            vals.insert(c.name.clone(), DValue::Str(cur));
        }
        "textbox" => {
            let mut s = match vals.get(&c.name) { Some(DValue::Str(s)) => s.clone(), _ => String::new() };
            let h = (c.height as f32 * 24.0).max(48.0);
            if ui.add_sized([width.max(240.0), h], egui::TextEdit::multiline(&mut s)).on_hover_text(&c.hint).changed() { vals.insert(c.name.clone(), DValue::Str(s)); }
        }
        "color" | "coloralpha" | "alpha" => {
            let s = match vals.get(&c.name) { Some(DValue::Str(s)) => s.clone(), _ => "#FFFFFF".into() };
            let col = parse_color(&s);
            let mut rgb = [col.r(), col.g(), col.b()];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                vals.insert(c.name.clone(), DValue::Str(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])));
            }
        }
        _ => {
            // edit and unknown text-like controls
            let mut s = match vals.get(&c.name) { Some(DValue::Str(s)) => s.clone(), _ => String::new() };
            if ui.add_sized([width.max(120.0), 20.0], egui::TextEdit::singleline(&mut s)).on_hover_text(&c.hint).changed() { vals.insert(c.name.clone(), DValue::Str(s)); }
        }
    }
}
