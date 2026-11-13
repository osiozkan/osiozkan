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
        let mut rgb = [c.r(), c.g(), c.b()];
        if crate::color_ui::button(ui, label, &mut rgb) {
            c = Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], c.a());
            *v = from32(c);
            changed = true;
        }
        // Saydamlık (ASS alfa: 0 = opak, 255 = görünmez)
        let mut alpha = 255 - c.a();
        if ui.add(DragValue::new(&mut alpha).range(0..=255).prefix("α ")).on_hover_text("Saydamlık (0 opak, 255 görünmez)").changed() {
            c = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 255 - alpha);
            *v = from32(c);
            changed = true;
        }
        ui.monospace(v.as_str());
    });
    changed
}

/// Kurulu yazı tipi listesinden seçim (arama kutusu yazı tipi alanıdır). Değiştiyse `true`.
pub fn font_picker(ui: &mut egui::Ui, name: &mut String, fonts: Option<&[String]>) -> bool {
    let mut ch = false;
    let Some(fonts) = fonts else {
        ui.add_enabled(false, egui::Button::new("Liste…").small()).on_disabled_hover_text("Yazı tipleri taranıyor…");
        return false;
    };
    ui.menu_button("Liste…", |ui| {
        let q = name.trim().to_lowercase();
        let hits: Vec<&String> = if q.is_empty() { fonts.iter().collect() } else {
            // önce baştan eşleşenler, sonra içerenler
            let mut a: Vec<&String> = fonts.iter().filter(|f| f.to_lowercase().starts_with(&q)).collect();
            a.extend(fonts.iter().filter(|f| { let l = f.to_lowercase(); !l.starts_with(&q) && l.contains(&q) }));
            a
        };
        ui.label(egui::RichText::new(format!("{} / {} yazı tipi (yazı tipi alanına yazarak süzün)", hits.len(), fonts.len())).weak());
        let row_h = ui.text_style_height(&egui::TextStyle::Button) + 4.0;
        egui::ScrollArea::vertical().max_height(320.0).min_scrolled_width(260.0).show_rows(ui, row_h, hits.len(), |ui, range| {
            for f in &hits[range] {
                if ui.selectable_label(f.eq_ignore_ascii_case(name), f.as_str()).clicked() {
                    *name = (*f).clone();
                    ch = true;
                    ui.close();
                }
            }
        });
    });
    // Kurulu değilse uyar (ekli yazı tipleri ayrıca denetlenir: Araçlar → Yazı tipi denetimi)
    let n = name.trim_start_matches('@');
    if !n.is_empty() && !fonts.iter().any(|f| f.eq_ignore_ascii_case(n)) {
        ui.label(egui::RichText::new("kurulu değil").color(crate::theme::BAD)).on_hover_text("Bu sistemde bulunamadı; betiğe ekli (Fonts) değilse izleyicide başka yazı tipiyle görünür");
    }
    ch
}

/// Stili düzenler; değişiklik olursa `true`. `fonts`: kurulu yazı tipi aileleri (taranmadıysa None).
pub fn edit(ui: &mut egui::Ui, s: &mut Style, fonts: Option<&[String]>) -> bool {
    let mut ch = false;
    ui.horizontal(|ui| {
        ui.label("Ad");
        ch |= ui.text_edit_singleline(&mut s.name).changed();
    });
    ui.horizontal(|ui| {
        ui.label("Yazı tipi");
        ch |= ui.add(egui::TextEdit::singleline(&mut s.fontname).desired_width(190.0)).changed();
        ch |= font_picker(ui, &mut s.fontname, fonts);
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
