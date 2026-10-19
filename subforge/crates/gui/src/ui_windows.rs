use crate::state::App;
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use subforge_app::search;
use subforge_core::color::Color;
use subforge_core::qc::{self, Severity};
use subforge_core::time::{parse_time, Fps};
use subforge_core::timing;
use subforge_lua::{DialogResult, DialogValue};

fn c32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, 255 - c.a)
}

fn from_c32(c: Color32) -> Color {
    Color { r: c.r(), g: c.g(), b: c.b(), a: 255 - c.a() }
}

impl App {
    pub fn windows(&mut self, ctx: &egui::Context) {
        self.lua_dialog_window(ctx);
        self.find_window(ctx);
        self.shift_window(ctx);
        self.retime_window(ctx);
        self.styles_window(ctx);
        self.qc_window(ctx);
        self.automation_window(ctx);
        self.filters_window(ctx);
        self.resample_window(ctx);
        self.select_window(ctx);
        self.fonts_window(ctx);
        self.props_window(ctx);
        self.settings_window(ctx);
        self.log_window(ctx);
        self.about_window(ctx);
        self.keys_window(ctx);
        self.kara_window(ctx);
        self.console_window(ctx);
        self.spell_window(ctx);
    }

    // ---------------------------------------------------------------- Lua dialog
    fn lua_dialog_window(&mut self, ctx: &egui::Context) {
        let Some(run) = self.run.as_mut() else { return };
        let title = run.title.clone();
        let mut reply: Option<Option<DialogResult>> = None;
        if let Some(d) = run.dialog.as_mut() {
            egui::Modal::new(egui::Id::new("lua_dialog")).show(ctx, |ui| {
                ui.heading(&title);
                // group fields by row (y), order by x
                let mut rows: std::collections::BTreeMap<i32, Vec<usize>> = Default::default();
                for (i, f) in d.dialog.fields.iter().enumerate() {
                    rows.entry(f.y).or_default().push(i);
                }
                for (_, mut idxs) in rows {
                    idxs.sort_by_key(|&i| d.dialog.fields[i].x);
                    ui.horizontal(|ui| {
                        for i in idxs {
                            let f = d.dialog.fields[i].clone();
                            let w = (f.width.max(1) as f32) * 90.0;
                            match f.class.as_str() {
                                "label" => {
                                    ui.label(&f.label);
                                }
                                "edit" | "color" | "coloralpha" | "alpha" => {
                                    let mut s = match d.values.get(&f.name) {
                                        Some(DialogValue::Str(s)) => s.clone(),
                                        _ => String::new(),
                                    };
                                    if f.class == "color" || f.class == "coloralpha" {
                                        let hex = s.trim().trim_start_matches('#');
                                        let mut rgb = [255u8, 255, 255];
                                        if hex.len() >= 6 {
                                            for k in 0..3 {
                                                rgb[k] = u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap_or(255);
                                            }
                                        }
                                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                                            s = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                                        }
                                    }
                                    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(w.max(120.0))).on_hover_text(&f.hint).changed() || f.class.starts_with("color") {
                                        d.values.insert(f.name.clone(), DialogValue::Str(s));
                                    }
                                }
                                "textbox" => {
                                    let mut s = match d.values.get(&f.name) {
                                        Some(DialogValue::Str(s)) => s.clone(),
                                        _ => String::new(),
                                    };
                                    if ui.add(egui::TextEdit::multiline(&mut s).desired_width(w.max(240.0)).desired_rows((f.height.max(2) * 2) as usize)).changed() {
                                        d.values.insert(f.name.clone(), DialogValue::Str(s));
                                    }
                                }
                                "intedit" | "floatedit" | "slider" => {
                                    let mut v = match d.values.get(&f.name) {
                                        Some(DialogValue::Num(n)) => *n,
                                        _ => 0.0,
                                    };
                                    let mut dv = egui::DragValue::new(&mut v);
                                    if f.class == "intedit" {
                                        dv = dv.speed(1.0).fixed_decimals(0);
                                    } else {
                                        dv = dv.speed(f.step.unwrap_or(0.1));
                                    }
                                    if let (Some(a), Some(b)) = (f.min, f.max) {
                                        if b > a {
                                            dv = dv.range(a..=b);
                                        }
                                    }
                                    if ui.add(dv).on_hover_text(&f.hint).changed() {
                                        d.values.insert(f.name.clone(), DialogValue::Num(v));
                                    }
                                }
                                "checkbox" => {
                                    let mut b = matches!(d.values.get(&f.name), Some(DialogValue::Bool(true)));
                                    if ui.checkbox(&mut b, &f.label).on_hover_text(&f.hint).changed() {
                                        d.values.insert(f.name.clone(), DialogValue::Bool(b));
                                    }
                                }
                                "dropdown" => {
                                    let mut cur = match d.values.get(&f.name) {
                                        Some(DialogValue::Str(s)) => s.clone(),
                                        _ => f.items.first().cloned().unwrap_or_default(),
                                    };
                                    egui::ComboBox::from_id_salt(format!("dd{i}")).selected_text(&cur).show_ui(ui, |ui| {
                                        for it in &f.items {
                                            ui.selectable_value(&mut cur, it.clone(), it);
                                        }
                                    });
                                    d.values.insert(f.name.clone(), DialogValue::Str(cur));
                                }
                                other => {
                                    ui.label(format!("({other})"));
                                }
                            }
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let buttons: Vec<String> = if d.dialog.buttons.is_empty() { vec!["OK".into(), "İptal".into()] } else { d.dialog.buttons.clone() };
                    let cancel_label = d.dialog.button_ids.get("cancel").cloned();
                    for (bi, b) in buttons.iter().enumerate() {
                        let is_cancel = cancel_label.as_deref() == Some(b.as_str()) || (cancel_label.is_none() && (bi == 1 && d.dialog.buttons.is_empty() || b.eq_ignore_ascii_case("cancel") || b.eq_ignore_ascii_case("iptal")));
                        if ui.button(b).clicked() {
                            reply = Some(if is_cancel { None } else { Some(DialogResult { button: if d.dialog.buttons.is_empty() { None } else { Some(b.clone()) }, values: d.values.clone() }) });
                        }
                    }
                });
            });
        }
        if let Some(r) = reply {
            if let Some(d) = run.dialog.take() {
                let _ = d.reply.send(r);
            }
        }
        // progress/cancel bar
        if let Some(run) = self.run.as_mut() {
            if run.dialog.is_none() {
                egui::Modal::new(egui::Id::new("lua_progress")).show(ctx, |ui| {
                    ui.heading("Makro çalışıyor");
                    ui.label(&run.title);
                    if !run.task.is_empty() {
                        ui.label(&run.task);
                    }
                    match run.progress {
                        Some(p) => {
                            ui.add(egui::ProgressBar::new((p / 100.0) as f32).show_percentage());
                        }
                        None => {
                            ui.add(egui::ProgressBar::new(0.0).animate(true));
                        }
                    }
                    if ui.button("İptal").clicked() {
                        run.handle.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            }
        }
    }

    // ---------------------------------------------------------------- Find / replace
    fn find_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.find;
        egui::Window::new("Bul / Değiştir").open(&mut open).resizable(false).show(ctx, |ui| {
            egui::Grid::new("find_grid").num_columns(2).show(ui, |ui| {
                ui.label("Bul");
                let r = ui.text_edit_singleline(&mut self.find.query);
                ui.end_row();
                ui.label("Değiştir");
                ui.text_edit_singleline(&mut self.find.replace);
                ui.end_row();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.find_next();
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.find.match_case, "Büyük/küçük harf");
                ui.checkbox(&mut self.find.regex, "Regex");
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.find.in_text, "Metin");
                ui.checkbox(&mut self.find.in_actor, "Konuşmacı");
                ui.checkbox(&mut self.find.in_style, "Stil");
                ui.checkbox(&mut self.find.in_effect, "Efekt");
            });
            ui.checkbox(&mut self.find.only_selected, "Yalnızca seçili satırlarda");
            ui.horizontal(|ui| {
                if ui.button("Sonrakini bul").clicked() {
                    self.find_next();
                }
                if ui.button("Tümünü değiştir").clicked() {
                    let sel = self.selected_vec();
                    let o = self.find.clone();
                    let res = self.proj.edit("Replace all", |s| search::replace_all(s, &o, &sel));
                    match res {
                        Ok(n) => {
                            self.bump();
                            self.find_msg = format!("{n} satır değiştirildi");
                        }
                        Err(e) => self.find_msg = e,
                    }
                }
                if ui.button("Tümünü seç").clicked() {
                    let sel = self.selected_vec();
                    match search::find_all(&self.proj.script, &self.find, &sel) {
                        Ok(v) => {
                            self.find_msg = format!("{} eşleşme", v.len());
                            if let Some(&f) = v.first() {
                                self.sel = v.iter().copied().collect();
                                self.active = Some(f);
                                self.scroll_to = Some(f);
                            }
                        }
                        Err(e) => self.find_msg = e,
                    }
                }
            });
            ui.label(&self.find_msg);
        });
        self.win.find = open;
    }

    fn find_next(&mut self) {
        let sel = self.selected_vec();
        match search::find_next(&self.proj.script, &self.find, self.active, &sel) {
            Ok(Some(i)) => {
                self.goto_line(i);
                self.find_msg = format!("Satır {}", i + 1);
            }
            Ok(None) => self.find_msg = "Bulunamadı".into(),
            Err(e) => self.find_msg = e,
        }
    }

    // ---------------------------------------------------------------- Shift
    fn shift_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.shift;
        egui::Window::new("Zamanları kaydır").open(&mut open).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.shift.use_frames, false, "Milisaniye");
                ui.radio_value(&mut self.shift.use_frames, true, "Kare");
            });
            if self.shift.use_frames {
                ui.add(egui::DragValue::new(&mut self.shift.frames).suffix(" kare").speed(0.2));
            } else {
                ui.add(egui::DragValue::new(&mut self.shift.ms).suffix(" ms").speed(5.0));
            }
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.shift.start, "Başlangıç");
                ui.checkbox(&mut self.shift.end, "Bitiş");
            });
            egui::ComboBox::from_id_salt("shift_scope").selected_text(["Seçili satırlar", "Tüm satırlar", "Aktif satırdan sona kadar"][self.shift.scope as usize]).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.shift.scope, 0, "Seçili satırlar");
                ui.selectable_value(&mut self.shift.scope, 1, "Tüm satırlar");
                ui.selectable_value(&mut self.shift.scope, 2, "Aktif satırdan sona kadar");
            });
            if ui.button("Uygula").clicked() {
                let n = self.proj.script.events.len();
                let idx: Vec<usize> = match self.shift.scope {
                    0 => self.selected_vec(),
                    1 => (0..n).collect(),
                    _ => (self.active.unwrap_or(0)..n).collect(),
                };
                let fps = Fps::from_f64(self.fps());
                let (use_frames, frames, ms, st, en) = (self.shift.use_frames, self.shift.frames, self.shift.ms, self.shift.start, self.shift.end);
                self.proj.edit("Shift times", |s| {
                    if use_frames {
                        timing::shift_frames(&mut s.events, &idx, frames, fps);
                    } else {
                        timing::shift(&mut s.events, &idx, ms, st, en);
                    }
                });
                self.bump();
            }
            ui.separator();
            ui.label("Zincirle / tampon");
            ui.horizontal(|ui| {
                ui.label("Maks. boşluk");
                ui.add(egui::DragValue::new(&mut self.shift.chain_gap).suffix(" ms"));
                if ui.button("Boşlukları kapat").clicked() {
                    let idx: Vec<usize> = self.selected_vec();
                    let idx = if idx.len() < 2 { (0..self.proj.script.events.len()).collect() } else { idx };
                    let g = self.shift.chain_gap;
                    let n = self.proj.edit("Chain", |s| timing::chain(&mut s.events, &idx, g, 0));
                    self.bump();
                    self.set_status(format!("{n} boşluk kapatıldı"));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Başa");
                ui.add(egui::DragValue::new(&mut self.shift.lead_in).suffix(" ms"));
                ui.label("Sona");
                ui.add(egui::DragValue::new(&mut self.shift.lead_out).suffix(" ms"));
                if ui.button("Uygula").clicked() {
                    let idx = self.selected_vec();
                    let (a, b) = (self.shift.lead_in, self.shift.lead_out);
                    self.proj.edit("Lead in/out", |s| timing::lead(&mut s.events, &idx, a, b));
                    self.bump();
                }
            });
        });
        self.win.shift = open;
    }

    fn retime_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.retime;
        egui::Window::new("İki noktalı eşitleme").open(&mut open).resizable(false).show(ctx, |ui| {
            ui.label("Altyazıdaki iki bilinen zamanı videodaki gerçek zamanlarına eşler (sürüklenmeyi düzeltir).");
            egui::Grid::new("rt").num_columns(3).show(ui, |ui| {
                ui.label("");
                ui.label("Eski (altyazı)");
                ui.label("Yeni (video)");
                ui.end_row();
                ui.label("1. nokta");
                ui.text_edit_singleline(&mut self.shift.rt[0]);
                ui.text_edit_singleline(&mut self.shift.rt[1]);
                ui.end_row();
                ui.label("2. nokta");
                ui.text_edit_singleline(&mut self.shift.rt[2]);
                ui.text_edit_singleline(&mut self.shift.rt[3]);
                ui.end_row();
            });
            ui.label("Biçim: 0:01:23.45");
            if ui.button("Uygula").clicked() {
                let t: Vec<Option<i64>> = self.shift.rt.iter().map(|s| parse_time(s)).collect();
                if let [Some(a1), Some(b1), Some(a2), Some(b2)] = t[..] {
                    let idx: Vec<usize> = (0..self.proj.script.events.len()).collect();
                    self.proj.edit("Linear retime", |s| timing::linear_retime(&mut s.events, &idx, a1, b1, a2, b2));
                    self.bump();
                    self.set_status("Eşitleme uygulandı");
                } else {
                    self.set_status("Zaman biçimi geçersiz");
                }
            }
        });
        self.win.retime = open;
    }

    // ---------------------------------------------------------------- Styles
    fn styles_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.styles;
        egui::Window::new("Stil yöneticisi").open(&mut open).default_size([640.0, 460.0]).show(ctx, |ui| {
            let n = self.proj.script.styles.len();
            if n == 0 {
                if ui.button("Stil ekle").clicked() {
                    self.proj.add_style(subforge_core::Style::default());
                    self.bump();
                }
                return;
            }
            self.style_sel = self.style_sel.min(n - 1);
            ui.horizontal(|ui| {
                egui::ScrollArea::vertical().id_salt("style_list").max_width(170.0).show(ui, |ui| {
                    ui.vertical(|ui| {
                        for i in 0..n {
                            let name = self.proj.script.styles[i].name.clone();
                            if ui.selectable_label(self.style_sel == i, name).clicked() {
                                self.style_sel = i;
                            }
                        }
                        ui.separator();
                        if ui.button("+ Yeni").clicked() {
                            let mut s = self.proj.script.styles[self.style_sel].clone();
                            s.name = format!("{} kopya", s.name);
                            self.proj.add_style(s);
                            self.style_sel = self.proj.script.styles.len() - 1;
                            self.bump();
                        }
                        if ui.add_enabled(n > 1, egui::Button::new("Sil")).clicked() {
                            let i = self.style_sel;
                            self.proj.edit("Delete style", |s| {
                                s.styles.remove(i);
                            });
                            self.bump();
                        }
                        ui.separator();
                        ui.label("Stil deposu");
                        let store = subforge_app::settings::config_dir().join("style_storage.ass");
                        if ui.button("Depoya kaydet").on_hover_text("Seçili stili ortak depoya ekler").clicked() {
                            let mut sc = subforge_core::load_path(&store.to_string_lossy()).map(|x| x.0).unwrap_or_else(|_| {
                                let mut s = subforge_core::Script::new();
                                s.styles.clear();
                                s
                            });
                            let st = self.proj.script.styles[self.style_sel].clone();
                            subforge_core::tools::import_styles(&mut sc, &subforge_core::Script { styles: vec![st], ..subforge_core::Script::new() }, &[], true);
                            let _ = std::fs::create_dir_all(subforge_app::settings::config_dir());
                            let _ = subforge_core::save_path(&sc, &store.to_string_lossy());
                            self.status = "Stil depoya kaydedildi".into();
                        }
                        ui.menu_button("Depodan getir", |ui| {
                            match subforge_core::load_path(&store.to_string_lossy()) {
                                Ok((sc, _)) if !sc.styles.is_empty() => {
                                    for st in &sc.styles {
                                        if ui.button(&st.name).clicked() {
                                            let other = subforge_core::Script { styles: vec![st.clone()], ..subforge_core::Script::new() };
                                            self.proj.edit("Import style", |s| subforge_core::tools::import_styles(s, &other, &[], true));
                                            self.bump();
                                            ui.close_menu();
                                        }
                                    }
                                }
                                _ => {
                                    ui.label("Depo boş");
                                }
                            }
                        });
                        ui.separator();
                        if ui.button("Yukarı taşı").clicked() && self.style_sel > 0 {
                            let i = self.style_sel;
                            self.proj.edit("Move style", |s| s.styles.swap(i, i - 1));
                            self.style_sel -= 1;
                            self.bump();
                        }
                    });
                });
                ui.separator();
                let i = self.style_sel;
                let mut st = self.proj.script.styles[i].clone();
                self.style_preview(ctx, &st);
                let before = st.clone();
                let old_name = st.name.clone();
                ui.vertical(|ui| {
                    if let Some((_, tex)) = &self.style_prev {
                        let w = ui.available_width().min(420.0);
                        ui.image((tex.id(), egui::vec2(w, w * tex.size_vec2().y / tex.size_vec2().x)));
                    }
                });
                egui::ScrollArea::vertical().id_salt("style_edit").show(ui, |ui| {
                    egui::Grid::new("style_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label("Ad");
                        ui.text_edit_singleline(&mut st.name);
                        ui.end_row();
                        ui.label("Yazı tipi");
                        ui.text_edit_singleline(&mut st.fontname);
                        ui.end_row();
                        ui.label("Boyut");
                        ui.add(egui::DragValue::new(&mut st.fontsize).range(1.0..=500.0));
                        ui.end_row();
                        for (label, col) in [("Ana renk", &mut st.color1), ("İkincil renk", &mut st.color2), ("Kenar rengi", &mut st.color3), ("Gölge rengi", &mut st.color4)] {
                            ui.label(label);
                            let mut c = c32(*col);
                            ui.horizontal(|ui| {
                                if ui.color_edit_button_srgba_unmultiplied(unsafe_arr(&mut c)).changed() {}
                            });
                            *col = from_c32(c);
                            ui.end_row();
                        }
                        ui.label("Biçim");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut st.bold, "Kalın");
                            ui.checkbox(&mut st.italic, "İtalik");
                            ui.checkbox(&mut st.underline, "Altı çizili");
                            ui.checkbox(&mut st.strikeout, "Üstü çizili");
                        });
                        ui.end_row();
                        ui.label("Ölçek X / Y %");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut st.scale_x));
                            ui.add(egui::DragValue::new(&mut st.scale_y));
                        });
                        ui.end_row();
                        ui.label("Harf aralığı / Açı");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut st.spacing).speed(0.1));
                            ui.add(egui::DragValue::new(&mut st.angle));
                        });
                        ui.end_row();
                        ui.label("Kenar / Gölge");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut st.outline).speed(0.1).range(0.0..=100.0));
                            ui.add(egui::DragValue::new(&mut st.shadow).speed(0.1).range(0.0..=100.0));
                        });
                        ui.end_row();
                        ui.label("Kenar stili");
                        egui::ComboBox::from_id_salt("bs").selected_text(if st.borderstyle == 3 { "Opak kutu" } else { "Kenar + gölge" }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut st.borderstyle, 1, "Kenar + gölge");
                            ui.selectable_value(&mut st.borderstyle, 3, "Opak kutu");
                        });
                        ui.end_row();
                        ui.label("Hizalama (numpad)");
                        ui.horizontal(|ui| {
                            egui::Grid::new("align").spacing([2.0, 2.0]).show(ui, |ui| {
                                for row in [[7, 8, 9], [4, 5, 6], [1, 2, 3]] {
                                    for a in row {
                                        if ui.selectable_label(st.align == a, format!("{a}")).clicked() {
                                            st.align = a;
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                        ui.end_row();
                        ui.label("Kenar boşluğu L / R / T / B");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut st.margin_l));
                            ui.add(egui::DragValue::new(&mut st.margin_r));
                            ui.add(egui::DragValue::new(&mut st.margin_t));
                            ui.add(egui::DragValue::new(&mut st.margin_b));
                        });
                        ui.end_row();
                        ui.label("Kodlama");
                        ui.add(egui::DragValue::new(&mut st.encoding));
                        ui.end_row();
                    });
                });
                if st != before {
                    let new_name = st.name.clone();
                    self.proj.edit_coalesced(&format!("style:{i}"), |s| {
                        s.styles[i] = st;
                        if new_name != old_name {
                            for e in s.events.iter_mut().filter(|e| e.style == old_name) {
                                e.style = new_name.clone();
                            }
                        }
                    });
                    self.bump();
                }
            });
        });
        self.win.styles = open;
    }

    /// Keep the style preview texture in sync with the style being edited (async libass render).
    fn style_preview(&mut self, ctx: &egui::Context, st: &subforge_core::Style) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{st:?}").hash(&mut h);
        let key = h.finish();
        if let Some((k, rx)) = &self.style_prev_rx {
            if let Ok(r) = rx.try_recv() {
                let k = *k;
                self.style_prev_rx = None;
                if let Ok(f) = r {
                    let img = egui::ColorImage::from_rgba_unmultiplied([f.width, f.height], &f.rgba);
                    self.style_prev = Some((k, ctx.load_texture("style_prev", img, egui::TextureOptions::LINEAR)));
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
                return;
            }
        }
        if self.style_prev.as_ref().map(|(k, _)| *k) != Some(key) && self.style_prev_rx.is_none() {
            let st = st.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.style_prev_rx = Some((key, rx));
            std::thread::spawn(move || {
                let _ = tx.send(subforge_app::media::render_style_preview(&st, "Örnek metin — Sample Text 日本語", 420, 120));
            });
        }
    }

    // ---------------------------------------------------------------- QC
    fn qc_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.qc;
        let mut jump = None;
        egui::Window::new("Kalite kontrol").open(&mut open).default_size([760.0, 520.0]).show(ctx, |ui| {
            let mut cfg_changed = false;
            ui.horizontal_wrapped(|ui| {
                let q = &mut self.settings.qc;
                ui.label("Maks. kar/sn");
                cfg_changed |= ui.add(egui::DragValue::new(&mut q.max_cps).speed(0.5)).changed();
                ui.label("Maks. satır uzunluğu");
                cfg_changed |= ui.add(egui::DragValue::new(&mut q.max_line_chars)).changed();
                ui.label("Maks. satır");
                cfg_changed |= ui.add(egui::DragValue::new(&mut q.max_lines)).changed();
                ui.label("Min. süre ms");
                cfg_changed |= ui.add(egui::DragValue::new(&mut q.min_duration_ms).speed(10.0)).changed();
                ui.label("Min. boşluk ms");
                cfg_changed |= ui.add(egui::DragValue::new(&mut q.min_gap_ms).speed(10.0)).changed();
            });
            if cfg_changed {
                self.qc_version = 0; self.qc_rx = None;
                self.settings.save();
            }
            ui.horizontal(|ui| {
                if ui.button("Otomatik düzelt").on_hover_text("Bozuk karakterler, boşluklar, çakışmalar, yinelenen/boş satırlar").clicked() {
                    let cfg = self.settings.qc.clone();
                    let n = self.proj.edit("QC autofix", |s| qc::autofix(s, &cfg));
                    self.bump();
                    self.clamp_selection();
                    self.set_status(format!("{n} düzeltme uygulandı"));
                }
                ui.label(format!("{} sorun", self.issues.len()));
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for is in &self.issues {
                    let (c, t) = match is.severity {
                        Severity::Error => (theme::ERR, "HATA"),
                        Severity::Warning => (theme::WARN, "UYARI"),
                        Severity::Info => (Color32::GRAY, "BİLGİ"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t).color(c).small());
                        if ui.link(format!("#{}", is.event + 1)).clicked() {
                            jump = Some(is.event);
                        }
                        ui.label(format!("{:?}: {}", is.kind, is.message));
                    });
                }
            });
        });
        self.win.qc = open;
        if let Some(i) = jump {
            self.goto_line(i);
        }
    }

    // ---------------------------------------------------------------- Automation manager
    fn automation_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.automation;
        let mut run: Option<(std::path::PathBuf, usize, String)> = None;
        egui::Window::new("Otomasyon betikleri").open(&mut open).default_size([640.0, 420.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Klasör ekle…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.settings.automation_dirs.push(d.to_string_lossy().to_string());
                        self.settings.save();
                        self.refresh_scripts();
                    }
                }
                if ui.button("Yeniden tara").clicked() {
                    self.refresh_scripts();
                }
            });
            ui.label("Aegisub Automation 4 (.lua) betikleri doğrudan çalışır. Klasörler:");
            for d in self.settings.automation_roots() {
                ui.monospace(d.display().to_string());
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in &self.scripts {
                    ui.group(|ui| {
                        ui.label(RichText::new(&s.name).strong());
                        ui.monospace(s.path.display().to_string());
                        if !s.description.is_empty() {
                            ui.label(&s.description);
                        }
                        if let Some(e) = &s.error {
                            ui.colored_label(theme::ERR, format!("Yüklenemedi: {e}"));
                        }
                        for (mi, (name, desc)) in s.macros.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.button("Çalıştır").clicked() {
                                    run = Some((s.path.clone(), mi, name.clone()));
                                }
                                ui.label(name);
                                ui.label(RichText::new(desc).weak());
                            });
                        }
                        for (name, desc) in &s.filters {
                            ui.label(RichText::new(format!("dışa aktarma filtresi: {name} — {desc}")).weak());
                        }
                    });
                }
            });
        });
        self.win.automation = open;
        if let Some((p, i, n)) = run {
            self.run_macro(p, i, n);
        }
    }

    fn filters_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.filters;
        let mut go = false;
        egui::Window::new("Dışa aktarma filtreleri").open(&mut open).default_size([520.0, 360.0]).show(ctx, |ui| {
            ui.label("Seçili filtreler öncelik sırasıyla (yüksekten düşüğe) uygulanır; proje değişmez, sonuç dosyaya yazılır.");
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                let mut rows: Vec<(usize, usize, String, String)> = vec![];
                for (si, s) in self.scripts.iter().enumerate() {
                    for (fi, (n, d)) in s.filters.iter().enumerate() {
                        rows.push((si, fi, n.clone(), d.clone()));
                    }
                }
                if rows.is_empty() {
                    ui.label("Yüklü dışa aktarma filtresi yok.");
                }
                for (si, fi, n, d) in rows {
                    let mut on = self.filter_sel.contains(&(si, fi));
                    if ui.checkbox(&mut on, &n).on_hover_text(&d).changed() {
                        if on {
                            self.filter_sel.insert((si, fi));
                        } else {
                            self.filter_sel.remove(&(si, fi));
                        }
                    }
                }
            });
            if ui.add_enabled(!self.filter_sel.is_empty() && self.run.is_none(), egui::Button::new("Dışa aktar…")).clicked() {
                go = true;
            }
        });
        self.win.filters = open;
        if go {
            if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).set_file_name("export.ass").save_file() {
                let items: Vec<(std::path::PathBuf, usize)> = {
                    let mut v: Vec<(usize, usize)> = self.filter_sel.iter().copied().collect();
                    let prio = |&(si, fi): &(usize, usize)| self.scripts.get(si).and_then(|s| s.filter_prio.get(fi)).copied().unwrap_or(0);
                    v.sort_by_key(|k| (std::cmp::Reverse(prio(k)), k.0, k.1));
                    v.into_iter().filter_map(|(si, fi)| self.scripts.get(si).map(|s| (s.path.clone(), fi))).collect()
                };
                let ctx = self.run_context();
                let handle = subforge_app::automation::spawn_filters(items, self.proj.script.clone(), ctx);
                self.run = Some(crate::state::RunState { export_to: Some(p.to_string_lossy().to_string()), handle, title: "Dışa aktarma filtreleri".into(), progress: None, task: String::new(), dialog: None });
            }
        }
    }

    fn resample_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.resample;
        egui::Window::new("Çözünürlüğü yeniden örnekle").open(&mut open).resizable(false).show(ctx, |ui| {
            let (ox, oy) = self.proj.script.play_res();
            ui.label(format!("Şimdiki çözünürlük: {ox}×{oy}"));
            ui.horizontal(|ui| {
                ui.label("Yeni");
                ui.add(egui::DragValue::new(&mut self.rs.0).range(16..=16384));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.rs.1).range(16..=16384));
                if let Some(v) = &self.video {
                    if ui.button("Videoya eşitle").clicked() {
                        self.rs.0 = v.info.width;
                        self.rs.1 = v.info.height;
                    }
                }
            });
            ui.checkbox(&mut self.rs.2, "Yazı boyutu, kenar, gölge ve aralıkları ölçekle");
            ui.checkbox(&mut self.rs.3, "En-boy oranı değişimini ortala (ör. 4:3 ve 16:9 arası)");
            if ui.button("Uygula").clicked() {
                let r = subforge_core::resample::Resample { new_x: self.rs.0, new_y: self.rs.1, scale_sizes: self.rs.2, aspect_shift: self.rs.3 };
                let n = self.proj.edit("Resample resolution", |s| subforge_core::resample::resample(s, r));
                self.bump();
                self.set_status(format!("{n} satırın etiketleri ölçeklendi"));
            }
        });
        self.win.resample = open;
    }

    fn select_window(&mut self, ctx: &egui::Context) {
        use subforge_app::search::{select_lines, SelectMode};
        let mut open = self.win.select;
        let mut go = false;
        egui::Window::new("Satırları seç").open(&mut open).resizable(false).show(ctx, |ui| {
            let o = &mut self.sel_opts;
            ui.horizontal(|ui| {
                ui.label("Ara");
                ui.text_edit_singleline(&mut o.query);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.in_text, "Metin");
                ui.checkbox(&mut o.in_actor, "Konuşmacı");
                ui.checkbox(&mut o.in_style, "Stil");
                ui.checkbox(&mut o.in_effect, "Efekt");
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.regex, "Regex");
                ui.checkbox(&mut o.match_case, "Büyük/küçük harf");
                ui.checkbox(&mut o.invert, "Eşleşmeyenler");
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.only_dialogue, "Yalnızca diyalog");
                ui.checkbox(&mut o.only_comments, "Yalnızca yorum");
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.mode, SelectMode::Replace, "Yeni seçim");
                ui.radio_value(&mut o.mode, SelectMode::Add, "Ekle");
                ui.radio_value(&mut o.mode, SelectMode::Subtract, "Çıkar");
                ui.radio_value(&mut o.mode, SelectMode::Intersect, "Kesişim");
            });
            if ui.button("Seç").clicked() {
                go = true;
            }
        });
        self.win.select = open;
        if go {
            let cur = self.selected_vec();
            match select_lines(&self.proj.script, &self.sel_opts, &cur) {
                Ok(v) => {
                    self.set_status(format!("{} satır seçildi", v.len()));
                    if let Some(&f) = v.first() {
                        self.sel = v.iter().copied().collect();
                        self.active = Some(f);
                        self.scroll_to = Some(f);
                    } else {
                        self.sel.clear();
                    }
                }
                Err(e) => self.set_status(e),
            }
        }
    }

    fn fonts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.fonts;
        egui::Window::new("Yazı tipi toplayıcı").open(&mut open).default_size([520.0, 360.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Yeniden tara").clicked() {
                    self.font_list = subforge_app::fonts::resolve(&self.proj.script, &[]);
                }
                let missing = self.font_list.iter().filter(|f| f.path.is_none()).count();
                ui.label(format!("{} yazı tipi, {} eksik", self.font_list.len(), missing));
                if ui.add_enabled(!self.font_list.is_empty(), egui::Button::new("Klasöre kopyala…")).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        match subforge_app::fonts::collect_to(&self.font_list, &d) {
                            Ok((n, miss)) => self.set_status(format!("{n} yazı tipi dosyası kopyalandı, eksik: {}", if miss.is_empty() { "yok".into() } else { miss.join(", ") })),
                            Err(e) => self.set_status(format!("Kopyalanamadı: {e}")),
                        }
                    }
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for f in &self.font_list {
                    ui.horizontal(|ui| {
                        let style = format!("{}{}", if f.key.bold { " Kalın" } else { "" }, if f.key.italic { " İtalik" } else { "" });
                        match &f.path {
                            Some(p) => {
                                ui.colored_label(theme::OK, "var");
                                ui.label(format!("{}{style}", f.key.family));
                                ui.label(RichText::new(p.display().to_string()).weak().small());
                            }
                            None => {
                                ui.colored_label(theme::ERR, "YOK");
                                ui.label(format!("{}{style}", f.key.family));
                            }
                        }
                    });
                }
            });
        });
        self.win.fonts = open;
    }

    fn props_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.props;
        egui::Window::new("Betik özellikleri").open(&mut open).resizable(false).show(ctx, |ui| {
            let mut info = self.proj.script.info.clone();
            let before = info.clone();
            egui::Grid::new("props").num_columns(2).show(ui, |ui| {
                for (k, v) in info.iter_mut().filter(|(k, _)| k != ";") {
                    ui.label(k.as_str());
                    ui.add(egui::TextEdit::singleline(v).desired_width(260.0));
                    ui.end_row();
                }
            });
            if info != before {
                self.proj.edit_coalesced("props", |s| s.info = info);
                self.bump();
            }
            ui.separator();
            if ui.button("Çözünürlüğü videoya eşitle").clicked() {
                if let Some(v) = &self.video {
                    let (w, h) = (v.info.width, v.info.height);
                    self.proj.edit("Set resolution", |s| {
                        s.info_set("PlayResX", &w.to_string());
                        s.info_set("PlayResY", &h.to_string());
                    });
                    self.bump();
                }
            }
        });
        self.win.props = open;
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.settings;
        egui::Window::new("Ayarlar").open(&mut open).resizable(false).show(ctx, |ui| {
            let mut changed = false;
            changed |= ui.checkbox(&mut self.settings.dark, "Koyu tema").changed();
            changed |= ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.75..=2.0).text("Arayüz ölçeği")).changed();
            ui.add(egui::Slider::new(&mut self.settings.grid_font, 10.0..=24.0).text("Liste yazı boyutu"));
            ui.add(egui::DragValue::new(&mut self.settings.autosave_secs).suffix(" sn otomatik kayıt (0 = kapalı)"));
            ui.add(egui::DragValue::new(&mut self.settings.default_fps).speed(0.001).suffix(" varsayılan fps"));
            if changed {
                crate::theme::apply(ctx, self.settings.dark, self.settings.ui_scale);
            }
            if ui.button("Kaydet").clicked() {
                self.settings.save();
            }
        });
        self.win.settings = open;
    }

    fn log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.log;
        egui::Window::new("Betik günlüğü").open(&mut open).default_size([560.0, 300.0]).show(ctx, |ui| {
            if ui.button("Temizle").clicked() {
                self.log.clear();
            }
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for l in &self.log {
                    ui.monospace(l.trim_end());
                }
            });
        });
        self.win.log = open;
    }

    fn kara_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.kara;
        let Some(idx) = self.active.filter(|&i| i < self.proj.script.events.len()) else {
            self.win.kara = false;
            return;
        };
        if self.kara_for != Some((idx, self.version)) {
            self.kara_syls = subforge_core::tools::kara_parse(&self.proj.script.events[idx].text);
            self.kara_sel = self.kara_sel.min(self.kara_syls.len().saturating_sub(1));
            self.kara_for = Some((idx, self.version));
        }
        let line_cs = (self.proj.script.events[idx].end - self.proj.script.events[idx].start) / 10;
        let mut apply = false;
        egui::Window::new(format!("Karaoke düzenleyici — satır {}", idx + 1)).open(&mut open).default_size([620.0, 380.0]).show(ctx, |ui| {
            let total: i64 = self.kara_syls.iter().map(|s| s.0).sum();
            ui.horizontal(|ui| {
                ui.label(format!("Toplam {total} cs / satır {line_cs} cs"));
                if ui.button("Satır süresine ölçekle").clicked() && total > 0 {
                    let text = subforge_core::tools::kara_build(&self.kara_syls);
                    let scaled = subforge_core::tools::kara_scale_to(&text, line_cs);
                    self.kara_syls = subforge_core::tools::kara_parse(&scaled);
                }
                if ui.button("Eşit dağıt").clicked() && !self.kara_syls.is_empty() {
                    let n = self.kara_syls.len() as i64;
                    let per = (line_cs / n).max(1);
                    for s in self.kara_syls.iter_mut() {
                        s.0 = per;
                    }
                }
            });
            ui.horizontal(|ui| {
                let n = self.kara_syls.len();
                if ui.add_enabled(n > 0, egui::Button::new("Seçiliyi ikiye böl")).clicked() {
                    let sel = self.kara_sel.min(n - 1);
                    let (d, k, o, t) = self.kara_syls[sel].clone();
                    let chars: Vec<char> = t.chars().collect();
                    if chars.len() > 1 {
                        let h = chars.len() / 2;
                        let (a, b): (String, String) = (chars[..h].iter().collect(), chars[h..].iter().collect());
                        self.kara_syls[sel] = (d / 2, k.clone(), o, a);
                        self.kara_syls.insert(sel + 1, (d - d / 2, k, String::new(), b));
                    }
                }
                if ui.add_enabled(n > 1 && self.kara_sel + 1 < n, egui::Button::new("Sonrakiyle birleştir")).clicked() {
                    let sel = self.kara_sel;
                    let next = self.kara_syls.remove(sel + 1);
                    self.kara_syls[sel].0 += next.0;
                    self.kara_syls[sel].3.push_str(&next.3);
                }
                if ui.add_enabled(n > 0, egui::Button::new("Sil")).clicked() {
                    self.kara_syls.remove(self.kara_sel.min(n - 1));
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("kara_grid").num_columns(5).striped(true).show(ui, |ui| {
                    ui.label("#");
                    ui.label("cs");
                    ui.label("tür");
                    ui.label("metin");
                    ui.label("ek etiket");
                    ui.end_row();
                    for (i, s) in self.kara_syls.iter_mut().enumerate() {
                        if ui.selectable_label(self.kara_sel == i, format!("{}", i + 1)).clicked() {
                            self.kara_sel = i;
                        }
                        ui.add(egui::DragValue::new(&mut s.0).range(0..=100000));
                        egui::ComboBox::from_id_salt(format!("kk{i}")).selected_text(s.1.as_str()).width(50.0).show_ui(ui, |ui| {
                            for k in ["k", "kf", "ko"] {
                                ui.selectable_value(&mut s.1, k.to_string(), k);
                            }
                        });
                        ui.add(egui::TextEdit::singleline(&mut s.3).desired_width(160.0));
                        ui.add(egui::TextEdit::singleline(&mut s.2).desired_width(100.0));
                        ui.end_row();
                    }
                });
            });
            ui.separator();
            if ui.button("Satıra uygula").clicked() {
                apply = true;
            }
        });
        self.win.kara = open;
        if apply {
            let text = subforge_core::tools::kara_build(&self.kara_syls);
            self.proj.edit("Karaoke edit", |s| s.events[idx].text = text);
            self.bump();
            self.kara_for = None;
        }
    }

    fn console_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.console;
        let mut run = false;
        egui::Window::new("Lua konsolu").open(&mut open).default_size([620.0, 420.0]).show(ctx, |ui| {
            ui.label("Makro gövdesi olarak çalışır: subs, sel, active. Çıktı için aegisub.log(...) kullanın.");
            ui.add(egui::TextEdit::multiline(&mut self.console_code).code_editor().desired_rows(12).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.add_enabled(self.run.is_none(), egui::Button::new("Çalıştır")).clicked() {
                    run = true;
                }
                if ui.button("Günlüğü göster").clicked() {
                    self.win.log = true;
                }
            });
        });
        self.win.console = open;
        if run {
            let code = format!("script_name = \"console\"\naegisub.register_macro(\"console\", \"\", function(subs, sel, active)\n{}\nend)\n", self.console_code);
            let path = std::env::temp_dir().join(format!("subforge-console-{}.lua", std::process::id()));
            if std::fs::write(&path, code).is_ok() {
                self.run_macro(path, 0, "Lua konsolu".into());
            }
        }
    }

    fn spell_window(&mut self, ctx: &egui::Context) {
        use subforge_app::spell;
        let mut open = self.win.spell;
        let mut goto = None;
        let mut replace: Option<(String, String)> = None;
        let mut ignore: Option<String> = None;
        let mut add: Option<String> = None;
        if let Some(rx) = &self.spell_dl {
            if let Ok(r) = rx.try_recv() {
                self.spell_dl = None;
                self.spell_msg = match r {
                    Ok(()) => "Sözlük indirildi.".into(),
                    Err(e) => format!("İndirilemedi: {e}"),
                };
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        egui::Window::new("Yazım denetimi").open(&mut open).default_size([560.0, 460.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Dil");
                egui::ComboBox::from_id_salt("spell_lang").selected_text(self.spell_lang.clone()).show_ui(ui, |ui| {
                    for l in ["tr_TR", "en_US"] {
                        if ui.selectable_value(&mut self.spell_lang, l.to_string(), l).clicked() {
                            self.speller = None;
                            self.spell_results.clear();
                        }
                    }
                });
            });
            if !spell::available(&self.spell_lang) {
                ui.label("Bu dil için sözlük kurulu değil (LibreOffice sözlükleri, internetten indirilir; tr_TR ≈ 38 MB).");
                let busy = self.spell_dl.is_some();
                if ui.add_enabled(!busy, egui::Button::new("Sözlüğü indir")).clicked() {
                    let lang = self.spell_lang.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.spell_dl = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(spell::download(&lang));
                    });
                }
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("İndiriliyor…");
                    });
                }
                ui.label(&self.spell_msg);
                return;
            }
            ui.horizontal(|ui| {
                if ui.button("Denetle").clicked() {
                    if self.speller.is_none() {
                        match spell::Speller::load(&self.spell_lang) {
                            Ok(s) => self.speller = Some(s),
                            Err(e) => self.spell_msg = e,
                        }
                    }
                    if let Some(sp) = &self.speller {
                        self.spell_results = spell::check_script(sp, &self.proj.script, &self.spell_ignore);
                        self.spell_sel = 0;
                        self.spell_sugg.clear();
                        self.spell_msg = format!("{} yanlış yazılmış sözcük", self.spell_results.len());
                    }
                }
                ui.label(&self.spell_msg);
            });
            ui.separator();
            ui.columns(2, |cols| {
                egui::ScrollArea::vertical().id_salt("spell_list").show(&mut cols[0], |ui| {
                    for (i, m) in self.spell_results.iter().enumerate() {
                        if ui.selectable_label(self.spell_sel == i, format!("{}  ({})", m.word, m.events.len())).clicked() {
                            self.spell_sel = i;
                            self.spell_fix = m.word.clone();
                            self.spell_sugg = self.speller.as_ref().map(|s| s.suggest(&m.word, 8)).unwrap_or_default();
                            goto = m.events.first().copied();
                        }
                    }
                });
                let ui = &mut cols[1];
                if let Some(m) = self.spell_results.get(self.spell_sel).cloned() {
                    ui.label(RichText::new(&m.word).strong().size(18.0));
                    ui.label(format!("{} satırda", m.events.len()));
                    ui.horizontal_wrapped(|ui| {
                        for s in self.spell_sugg.clone() {
                            if ui.button(&s).clicked() {
                                self.spell_fix = s;
                            }
                        }
                    });
                    ui.text_edit_singleline(&mut self.spell_fix);
                    ui.horizontal(|ui| {
                        if ui.button("Tümünü düzelt").clicked() {
                            replace = Some((m.word.clone(), self.spell_fix.clone()));
                        }
                        if ui.button("Yoksay").clicked() {
                            ignore = Some(m.word.clone());
                        }
                        if ui.button("Sözlüğe ekle").clicked() {
                            add = Some(m.word.clone());
                        }
                    });
                }
            });
        });
        self.win.spell = open;
        if let Some(i) = goto {
            self.goto_line(i);
        }
        if let Some((from, to)) = replace {
            if !to.is_empty() && to != from {
                let events: Vec<usize> = self.spell_results.iter().find(|m| m.word == from).map(|m| m.events.clone()).unwrap_or_default();
                self.proj.edit("Spelling fix", |s| {
                    for &i in &events {
                        s.events[i].text = spell::replace_word(&s.events[i].text, &from, &to);
                    }
                });
                self.bump();
                self.spell_results.retain(|m| m.word != from);
                self.spell_sel = self.spell_sel.min(self.spell_results.len().saturating_sub(1));
            }
        }
        if let Some(w) = ignore {
            self.spell_ignore.insert(w.clone());
            self.spell_results.retain(|m| m.word != w);
            self.spell_sel = self.spell_sel.min(self.spell_results.len().saturating_sub(1));
        }
        if let Some(w) = add {
            if let Some(sp) = self.speller.as_mut() {
                sp.add_user_word(&w);
            }
            self.spell_results.retain(|m| m.word != w);
            self.spell_sel = self.spell_sel.min(self.spell_results.len().saturating_sub(1));
        }
    }

    fn keys_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.keys;
        egui::Window::new("Klavye kısayolları").open(&mut open).resizable(false).show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                for (k, d) in [
                    ("Ctrl+O / S / Shift+S", "Aç / Kaydet / Farklı kaydet"),
                    ("Ctrl+Z / Ctrl+Y", "Geri al / Yinele"),
                    ("Ctrl+F", "Bul / Değiştir"),
                    ("Ctrl+I", "Yeni satır ekle"),
                    ("Ctrl+D", "Satırı çoğalt"),
                    ("Delete", "Seçili satırları sil"),
                    ("Ctrl+A", "Tümünü seç"),
                    ("Yukarı/Aşağı ok (Alt ile de)", "Önceki / sonraki satır"),
                    ("Ctrl+Enter", "Satırı bitir, sonrakine geç (sonda yeni satır ekler)"),
                    ("Boşluk", "Videoyu oynat / durdur (yazı yazarken değil)"),
                    ("[  /  ]", "Aktif satırın başlangıç / bitişini video konumuna ayarla"),
                    ("Dalga biçimi: sürükle", "Ses seçimi · turuncu çizgileri sürükle = başlangıç/bitiş"),
                    ("Enter (dalga)", "Ses seçimini aktif satıra uygula"),
                    ("Alt+sürükle (dalga)", "Aktif satırı kaydır"),
                    ("Ctrl+tekerlek (dalga)", "Yakınlaştır / uzaklaştır"),
                    ("Tıkla / Ctrl+tıkla / Shift+tıkla", "Satır seçimi"),
                ] {
                    ui.monospace(k);
                    ui.label(d);
                    ui.end_row();
                }
            });
        });
        self.win.keys = open;
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.win.about;
        egui::Window::new("SubForge hakkında").open(&mut open).resizable(false).show(ctx, |ui| {
            ui.heading(RichText::new("SubForge").color(theme::ACCENT));
            ui.label(format!("Sürüm {}", env!("CARGO_PKG_VERSION")));
            ui.label("Anime altyazı çalışma tezgâhı — Aegisub Automation 4 (Lua) uyumlu.");
            ui.label("ASS · SRT · WebVTT · Türkçe karakter ve kodlama onarımı · Kalite kontrol · Ses dalga biçimi · libass önizleme");
        });
        self.win.about = open;
    }
}

fn unsafe_arr(c: &mut Color32) -> &mut [u8; 4] {
    // Color32 is #[repr(C)] [u8; 4] — color_edit wants a plain array.
    unsafe { &mut *(c as *mut Color32 as *mut [u8; 4]) }
}

#[allow(dead_code)]
fn _unused(_: HashMap<String, String>) {}
