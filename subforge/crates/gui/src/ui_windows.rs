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
        self.props_window(ctx);
        self.settings_window(ctx);
        self.log_window(ctx);
        self.about_window(ctx);
    }

    // ---------------------------------------------------------------- Lua dialog
    fn lua_dialog_window(&mut self, ctx: &egui::Context) {
        let Some(run) = self.run.as_mut() else { return };
        let title = run.title.clone();
        let mut reply: Option<Option<DialogResult>> = None;
        if let Some(d) = run.dialog.as_mut() {
            egui::Window::new(&title).collapsible(false).resizable(true).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
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
                                        let col = Color::parse(&s.replace('#', "&H00").chars().collect::<String>()).unwrap_or(Color::WHITE);
                                        let mut rgb = [col.r, col.g, col.b];
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
                egui::Window::new("Makro çalışıyor").collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -40.0]).show(ctx, |ui| {
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
                ui.checkbox(&mut self.shift.only_selected, "Yalnızca seçili");
            });
            if ui.button("Uygula").clicked() {
                let idx: Vec<usize> = if self.shift.only_selected { self.selected_vec() } else { (0..self.proj.script.events.len()).collect() };
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
                let before = st.clone();
                let old_name = st.name.clone();
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
            for d in self.settings.script_dirs() {
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
