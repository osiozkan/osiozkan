//! Stil düzenleyici paneli.
use eframe::egui::{self, Color32, DragValue};
use osisub_core::ass::Style;
use osisub_core::color::Color;

fn to32(s: &str) -> Color32 {
    let c = Color::parse(s).unwrap_or(Color::WHITE);
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, 255 - c.a)
}

fn from32(c: Color32) -> String {
    Color { r: c.r(), g: c.g(), b: c.b(), a: 255 - c.a() }.to_style_string()
}

fn color_row(ui: &mut egui::Ui, label: &str, v: &mut String) -> bool {
    let mut c = to32(v);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.color_edit_button_srgba(&mut c).changed() {
            *v = from32(c);
            changed = true;
        }
        ui.monospace(v.as_str());
    });
    changed
}

/// Stili düzenler; değişiklik olursa `true`.
pub fn edit(ui: &mut egui::Ui, s: &mut Style) -> bool {
    let mut ch = false;
    ui.horizontal(|ui| {
        ui.label("Ad");
        ch |= ui.text_edit_singleline(&mut s.name).changed();
    });
    ui.horizontal(|ui| {
        ui.label("Yazı tipi");
        ch |= ui.text_edit_singleline(&mut s.fontname).changed();
        ui.label("Boyut");
        ch |= ui.add(DragValue::new(&mut s.fontsize).range(1.0..=500.0)).changed();
    });
    ui.horizontal(|ui| {
        ch |= ui.checkbox(&mut s.bold, "Kalın").changed();
        ch |= ui.checkbox(&mut s.italic, "İtalik").changed();
        ch |= ui.checkbox(&mut s.underline, "Altı çizili").changed();
        ch |= ui.checkbox(&mut s.strikeout, "Üstü çizili").changed();
    });
    ch |= color_row(ui, "Birincil  ", &mut s.color1);
    ch |= color_row(ui, "İkincil   ", &mut s.color2);
    ch |= color_row(ui, "Kenar     ", &mut s.color3);
    ch |= color_row(ui, "Gölge     ", &mut s.color4);
    ui.horizontal(|ui| {
        ui.label("Kenar");
        ch |= ui.add(DragValue::new(&mut s.outline).speed(0.1).range(0.0..=50.0)).changed();
        ui.label("Gölge");
        ch |= ui.add(DragValue::new(&mut s.shadow).speed(0.1).range(0.0..=50.0)).changed();
        let mut opaque = s.borderstyle == 3;
        if ui.checkbox(&mut opaque, "Opak kutu").changed() {
            s.borderstyle = if opaque { 3 } else { 1 };
            ch = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Ölçek X/Y");
        ch |= ui.add(DragValue::new(&mut s.scale_x).range(1.0..=1000.0)).changed();
        ch |= ui.add(DragValue::new(&mut s.scale_y).range(1.0..=1000.0)).changed();
        ui.label("Aralık");
        ch |= ui.add(DragValue::new(&mut s.spacing).speed(0.1)).changed();
        ui.label("Açı");
        ch |= ui.add(DragValue::new(&mut s.angle).speed(0.5)).changed();
    });
    ui.horizontal(|ui| {
        ui.label("Hizalama");
        egui::Grid::new("align_grid").spacing([2.0, 2.0]).show(ui, |ui| {
            for row in [[7, 8, 9], [4, 5, 6], [1, 2, 3]] {
                for a in row {
                    if ui.selectable_label(s.align == a, a.to_string()).clicked() {
                        s.align = a;
                        ch = true;
                    }
                }
                ui.end_row();
            }
        });
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Kenar boşluğu S/R/V");
                ch |= ui.add(DragValue::new(&mut s.margin_l)).changed();
                ch |= ui.add(DragValue::new(&mut s.margin_r)).changed();
                let before = s.margin_t;
                if ui.add(DragValue::new(&mut s.margin_t)).changed() {
                    s.margin_b = s.margin_t;
                    ch = true;
                }
                let _ = before;
            });
        });
    });
    ch
}
