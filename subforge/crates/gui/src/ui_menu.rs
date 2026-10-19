use crate::state::App;
use crate::theme;
use eframe::egui::{self, Key, Modifiers};
use subforge_app::automation::{self, RunContext, RunMsg};
use subforge_core::timing::SortKey;
use subforge_core::tools::{self, Case};

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_close(ctx);
        self.handle_drops(ctx);
        self.pump_run(ctx);
        self.refresh_qc();
        self.refresh_reference();
        self.autosave();
        self.shortcuts(ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| self.menus(ui, ctx));
        });
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.proj.script.events.len();
                    let (e, w) = self.issues.iter().fold((0, 0), |a, i| match i.severity {
                        subforge_core::qc::Severity::Error => (a.0 + 1, a.1),
                        subforge_core::qc::Severity::Warning => (a.0, a.1 + 1),
                        _ => a,
                    });
                    ui.label(format!("{n} satır · {} seçili", self.sel.len()));
                    ui.separator();
                    if ui.link(egui::RichText::new(format!("uyarı {w} · hata {e}")).color(if e > 0 { theme::ERR } else if w > 0 { theme::WARN } else { theme::OK })).clicked() {
                        self.win.qc = true;
                    }
                    ui.separator();
                    ui.label(format!("{:.3} fps", self.fps()));
                });
            });
        });

        self.edit_panel(ctx);
        if self.show_video && self.video.is_some() {
            self.video_panel(ctx);
        }
        if self.show_wave && self.wave.is_some() {
            self.wave_panel(ctx);
        }
        egui::CentralPanel::default().show(ctx, |ui| self.grid(ui));

        self.windows(ctx);
        self.ctx_repaint_if_busy(ctx);
        let title = format!("{} — SubForge", self.proj.title());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }
}

impl App {
    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.proj.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.confirm_close {
            egui::Window::new("Kaydedilmemiş değişiklikler").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("Kaydedilmemiş değişiklikler var. Ne yapılsın?");
                ui.horizontal(|ui| {
                    if ui.button("Kaydet ve çık").clicked() {
                        self.save();
                        if !self.proj.dirty {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        self.confirm_close = false;
                    }
                    if ui.button("Kaydetmeden çık").clicked() {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("İptal").clicked() {
                        self.confirm_close = false;
                    }
                });
            });
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(p) = f.path {
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                match ext.as_str() {
                    "ass" | "ssa" | "srt" | "vtt" => self.open_path(&p.to_string_lossy()),
                    "lua" => {
                        self.settings.automation_dirs.push(p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default());
                        self.refresh_scripts();
                        self.win.automation = true;
                    }
                    _ => self.load_video(p),
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let typing = ctx.wants_keyboard_input();
        let k = |m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));
        if k(cmd, Key::S) {
            self.save();
        }
        if k(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            self.save_as();
        }
        if k(cmd, Key::O) {
            self.open_dialog();
        }
        if k(cmd, Key::N) {
            self.proj = subforge_app::project::Project::new();
            self.select_only(0);
            self.bump();
        }
        if k(cmd, Key::F) || k(cmd, Key::H) {
            self.win.find = true;
        }
        if k(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || k(cmd, Key::Y) {
            self.do_redo();
        } else if k(cmd, Key::Z) && !typing {
            self.do_undo();
        }
        if !typing {
            if k(Modifiers::NONE, Key::Delete) {
                self.delete_selected();
            }
            if k(cmd, Key::D) {
                self.duplicate_selected();
            }
            if k(cmd, Key::A) {
                self.sel = (0..self.proj.script.events.len()).collect();
            }
            if k(Modifiers::NONE, Key::ArrowDown) {
                if let Some(a) = self.active {
                    self.goto_line((a + 1).min(self.proj.script.events.len().saturating_sub(1)));
                }
            }
            if k(Modifiers::NONE, Key::ArrowUp) {
                if let Some(a) = self.active {
                    self.goto_line(a.saturating_sub(1));
                }
            }
        }
        if k(Modifiers::ALT, Key::ArrowDown) {
            if let Some(a) = self.active {
                self.goto_line((a + 1).min(self.proj.script.events.len().saturating_sub(1)));
            }
        }
        if k(Modifiers::ALT, Key::ArrowUp) {
            if let Some(a) = self.active {
                self.goto_line(a.saturating_sub(1));
            }
        }
        if k(cmd, Key::Enter) {
            // commit & next, Aegisub-style
            if let Some(a) = self.active {
                if a + 1 >= self.proj.script.events.len() {
                    self.insert_line(false);
                } else {
                    self.goto_line(a + 1);
                }
                self.text_focus = true;
            }
        }
        if k(cmd, Key::I) {
            self.insert_line(false);
        }
    }

    fn menus(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.menu_button("Dosya", |ui| {
            if ui.add(egui::Button::new("Yeni").shortcut_text("Ctrl+N")).clicked() {
                self.proj = subforge_app::project::Project::new();
                self.select_only(0);
                self.bump();
                ui.close_menu();
            }
            if ui.add(egui::Button::new("Aç…").shortcut_text("Ctrl+O")).clicked() {
                self.open_dialog();
                ui.close_menu();
            }
            ui.menu_button("Son dosyalar", |ui| {
                let recent = self.settings.recent.clone();
                if recent.is_empty() {
                    ui.label("(boş)");
                }
                for r in recent {
                    if ui.button(&r).clicked() {
                        self.open_path(&r);
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            if ui.add(egui::Button::new("Kaydet").shortcut_text("Ctrl+S")).clicked() {
                self.save();
                ui.close_menu();
            }
            if ui.add(egui::Button::new("Farklı kaydet…").shortcut_text("Ctrl+Shift+S")).clicked() {
                self.save_as();
                ui.close_menu();
            }
            if ui.button("Filtrelerle dışa aktar…").clicked() {
                self.win.filters = true;
                ui.close_menu();
            }
            ui.menu_button("Dışa aktar", |ui| {
                if ui.button("SRT…").clicked() {
                    self.export("srt");
                    ui.close_menu();
                }
                if ui.button("WebVTT…").clicked() {
                    self.export("vtt");
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button("Video aç…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "avi", "webm", "mov", "m2ts", "ts"]).pick_file() {
                    self.load_video(p);
                }
                ui.close_menu();
            }
            if ui.button("Ses aç…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Ses", &["wav", "flac", "mp3", "aac", "ogg", "opus", "mka", "m4a"]).pick_file() {
                    self.load_audio(p);
                }
                ui.close_menu();
            }
            if ui.button("Referans altyazı aç… (çeviri için)").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", &["ass", "ssa", "srt", "vtt"]).pick_file() {
                    self.load_reference(&p.to_string_lossy());
                }
                ui.close_menu();
            }
            if self.reference.is_some() && ui.button("Referansı kapat").clicked() {
                self.reference = None;
                self.ref_texts.clear();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Çıkış").clicked() {
                _ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Düzenle", |ui| {
            let undo = self.proj.undo_label().map(|l| format!("Geri al: {l}")).unwrap_or_else(|| "Geri al".into());
            if ui.add_enabled(self.proj.can_undo(), egui::Button::new(undo).shortcut_text("Ctrl+Z")).clicked() {
                self.do_undo();
                ui.close_menu();
            }
            let redo = self.proj.redo_label().map(|l| format!("Yinele: {l}")).unwrap_or_else(|| "Yinele".into());
            if ui.add_enabled(self.proj.can_redo(), egui::Button::new(redo).shortcut_text("Ctrl+Y")).clicked() {
                self.do_redo();
                ui.close_menu();
            }
            ui.separator();
            if ui.add(egui::Button::new("Satır ekle (sonrasına)").shortcut_text("Ctrl+I")).clicked() {
                self.insert_line(false);
                ui.close_menu();
            }
            if ui.button("Satır ekle (öncesine)").clicked() {
                self.insert_line(true);
                ui.close_menu();
            }
            if ui.add(egui::Button::new("Çoğalt").shortcut_text("Ctrl+D")).clicked() {
                self.duplicate_selected();
                ui.close_menu();
            }
            if ui.add(egui::Button::new("Sil").shortcut_text("Del")).clicked() {
                self.delete_selected();
                ui.close_menu();
            }
            if ui.button("Birleştir").clicked() {
                self.join_selected();
                ui.close_menu();
            }
            if ui.button("Yorum satırı yap/geri al").clicked() {
                self.toggle_comment();
                ui.close_menu();
            }
            ui.separator();
            if ui.add(egui::Button::new("Bul / Değiştir…").shortcut_text("Ctrl+F")).clicked() {
                self.win.find = true;
                ui.close_menu();
            }
            if ui.button("Satırları seç…").clicked() {
                self.win.select = true;
                ui.close_menu();
            }
        });
        ui.menu_button("Zamanlama", |ui| {
            if ui.button("Kaydır…").clicked() {
                self.win.shift = true;
                ui.close_menu();
            }
            if ui.button("İki noktalı eşitleme…").clicked() {
                self.win.retime = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Boşlukları kapat (zincirle)").clicked() {
                let gap = self.shift.chain_gap;
                let idx: Vec<usize> = (0..self.proj.script.events.len()).filter(|&i| !self.proj.script.events[i].is_comment()).collect();
                let n = self.proj.edit("Chain", |s| subforge_core::timing::chain(&mut s.events, &idx, gap, 0));
                self.bump();
                self.set_status(format!("{n} boşluk kapatıldı"));
                ui.close_menu();
            }
            if ui.button("Çakışmaları düzelt").clicked() {
                let n = self.proj.edit("Fix overlaps", |s| subforge_core::timing::fix_overlaps(&mut s.events));
                self.bump();
                self.set_status(format!("{n} çakışma düzeltildi"));
                ui.close_menu();
            }
            if ui.button("Karelere hizala").clicked() {
                let fps = subforge_core::time::Fps::from_f64(self.fps());
                let idx: Vec<usize> = (0..self.proj.script.events.len()).collect();
                self.proj.edit("Snap to frames", |s| subforge_core::timing::snap_to_frames(&mut s.events, &idx, fps));
                self.bump();
                ui.close_menu();
            }
            if ui.button("Anahtar karelere hizala").clicked() {
                let keys = self.video.as_ref().map(|v| v.keyframes.clone()).unwrap_or_default();
                if keys.is_empty() {
                    self.set_status("Anahtar kare yok (önce video açın)");
                } else {
                    let idx = self.selected_vec();
                    let n = self.proj.edit("Snap to keyframes", |s| subforge_core::timing::snap_to_keyframes(&mut s.events, &idx, &keys, 250, 250, 250, 250));
                    self.bump();
                    self.set_status(format!("{n} zaman anahtar kareye hizalandı"));
                }
                ui.close_menu();
            }
        });
        ui.menu_button("Altyazı", |ui| {
            for (name, key) in [("Başlangıca göre sırala", SortKey::Start), ("Bitişe göre sırala", SortKey::End), ("Stile göre sırala", SortKey::Style), ("Konuşmacıya göre sırala", SortKey::Actor), ("Katmana göre sırala", SortKey::Layer)] {
                if ui.button(name).clicked() {
                    self.proj.sort(key);
                    self.bump();
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Çözünürlüğü yeniden örnekle…").clicked() {
                let (x, y) = self.proj.script.play_res();
                self.rs.0 = x;
                self.rs.1 = y;
                self.win.resample = true;
                ui.close_menu();
            }
            if ui.button("Stil yöneticisi…").clicked() {
                self.win.styles = true;
                ui.close_menu();
            }
            if ui.button("Betik özellikleri…").clicked() {
                self.win.props = true;
                ui.close_menu();
            }
        });
        ui.menu_button("Otomasyon", |ui| {
            if ui.button("Betik yöneticisi…").clicked() {
                self.win.automation = true;
                ui.close_menu();
            }
            if ui.button("Betikleri yeniden tara").clicked() {
                self.refresh_scripts();
                ui.close_menu();
            }
            ui.separator();
            let scripts = self.scripts.clone();
            let mut any = false;
            for s in &scripts {
                for (mi, (name, desc)) in s.macros.iter().enumerate() {
                    any = true;
                    if ui.button(name).on_hover_text(desc).clicked() {
                        self.run_macro(s.path.clone(), mi, name.clone());
                        ui.close_menu();
                    }
                }
            }
            if !any {
                ui.label("Yüklü makro yok. Lua betiklerini şuraya koyun:");
                ui.monospace(subforge_app::settings::config_dir().join("automation").display().to_string());
            }
        });
        ui.menu_button("Araçlar", |ui| {
            if ui.button("Kalite kontrol…").clicked() {
                self.win.qc = true;
                ui.close_menu();
            }
            if ui.button("Yazı tipi toplayıcı…").clicked() {
                self.font_list = subforge_app::fonts::resolve(&self.proj.script, &[]);
                self.win.fonts = true;
                ui.close_menu();
            }
            ui.menu_button("Büyük/küçük harf (Türkçe)", |ui| {
                for (label, case) in [("BÜYÜK HARF", Case::Upper), ("küçük harf", Case::Lower), ("Baş Harfler Büyük", Case::Title), ("Cümle düzeni", Case::Sentence)] {
                    if ui.button(label).clicked() {
                        self.map_selected("Change case", move |t| tools::change_case(t, case));
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("Metin temizliği (seçili satırlar)", |ui| {
                if ui.button("Yaygın hataları düzelt (boşluk, …, tire)").clicked() {
                    self.map_selected("Fix common errors", |t| tools::fix_common(t, &tools::FixOptions::default()));
                    ui.close_menu();
                }
                if ui.button("İşitme engelli metnini sil ([..], (..), ♪)").clicked() {
                    self.map_selected("Remove HI", |t| tools::strip_hearing_impaired(t));
                    ui.close_menu();
                }
                if ui.button("Tüm etiketleri sil").clicked() {
                    self.map_selected("Remove tags", |t| tools::remove_all_tags(t));
                    ui.close_menu();
                }
                if ui.button("Bitişik etiket bloklarını birleştir").clicked() {
                    self.map_selected("Merge blocks", |t| tools::merge_blocks(t));
                    ui.close_menu();
                }
                if ui.button("Satır sonlarını kaldır").clicked() {
                    self.map_selected("Unbreak", |t| tools::unbreak(t));
                    ui.close_menu();
                }
                let max = self.settings.qc.max_line_chars;
                if ui.button(format!("Satırları dengeli böl (≤ {max} karakter)")).clicked() {
                    self.map_selected("Balance lines", move |t| tools::balance_lines(t, max));
                    ui.close_menu();
                }
            });
            ui.menu_button("Karaoke", |ui| {
                if ui.button("Karaoke düzenleyici…").clicked() {
                    self.win.kara = true;
                    self.kara_for = None;
                    ui.close_menu();
                }
                if ui.button("Karakter başına \\k ver (satır süresine yay)").clicked() {
                    let idx = self.selected_vec();
                    self.proj.edit("Kara split chars", |s| {
                        for &i in &idx {
                            let cs = (s.events[i].end - s.events[i].start) / 10;
                            s.events[i].text = tools::kara_split_chars(&s.events[i].text, cs);
                        }
                    });
                    self.bump();
                    ui.close_menu();
                }
                if ui.button("\\k toplamını satır süresine ölçekle").clicked() {
                    let idx = self.selected_vec();
                    self.proj.edit("Kara scale", |s| {
                        for &i in &idx {
                            let cs = (s.events[i].end - s.events[i].start) / 10;
                            s.events[i].text = tools::kara_scale_to(&s.events[i].text, cs);
                        }
                    });
                    self.bump();
                    ui.close_menu();
                }
            });
            if ui.button("Seçili satırları \\N noktalarında ayır").clicked() {
                let idx = self.selected_vec();
                self.proj.edit("Split on breaks", |s| {
                    for &i in idx.iter().rev() {
                        let parts = tools::split_on_breaks(&s.events[i]);
                        if parts.len() > 1 {
                            s.events.splice(i..=i, parts);
                        }
                    }
                });
                self.bump();
                self.clamp_selection();
                ui.close_menu();
            }
            ui.menu_button("Stiller", |ui| {
                if ui.button("Kullanılmayan stilleri sil").clicked() {
                    let un = tools::unused_styles(&self.proj.script);
                    let n = un.len();
                    self.proj.edit("Remove unused styles", |s| s.styles.retain(|st| !un.contains(&st.name)));
                    self.bump();
                    self.set_status(format!("{n} stil silindi"));
                    ui.close_menu();
                }
                if ui.button("Başka dosyadan stil içe aktar…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass", "ssa"]).pick_file() {
                        if let Ok((other, _)) = subforge_core::load_path(&p.to_string_lossy()) {
                            let n = self.proj.edit("Import styles", |s| tools::import_styles(s, &other, &[], true));
                            self.bump();
                            self.set_status(format!("{n} stil içe aktarıldı"));
                        }
                    }
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button("Bozuk karakterleri onar (mojibake)").clicked() {
                let cfg = subforge_core::qc::QcConfig { check_overlaps: false, check_whitespace: false, check_empty: false, ..self.settings.qc.clone() };
                let n = self.proj.edit("Fix encoding", |s| subforge_core::qc::autofix(s, &cfg));
                self.bump();
                self.set_status(format!("{n} düzeltme"));
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Ayarlar…").clicked() {
                self.win.settings = true;
                ui.close_menu();
            }
        });
        ui.menu_button("Görünüm", |ui| {
            ui.checkbox(&mut self.show_video, "Video");
            ui.checkbox(&mut self.show_wave, "Ses dalga biçimi");
            if ui.button("Betik günlüğü").clicked() {
                self.win.log = true;
                ui.close_menu();
            }
        });
        ui.menu_button("Yardım", |ui| {
            if ui.button("Klavye kısayolları").clicked() {
                self.win.keys = true;
                ui.close_menu();
            }
            if ui.button("SubForge hakkında").clicked() {
                self.win.about = true;
                ui.close_menu();
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Aç").clicked() {
                self.open_dialog();
            }
            if ui.button("Kaydet").clicked() {
                self.save();
            }
            ui.separator();
            if ui.add_enabled(self.proj.can_undo(), egui::Button::new("Geri")).on_hover_text("Geri al").clicked() {
                self.do_undo();
            }
            if ui.add_enabled(self.proj.can_redo(), egui::Button::new("Yinele")).on_hover_text("Yinele").clicked() {
                self.do_redo();
            }
            ui.separator();
            if ui.button("+ Satır").clicked() {
                self.insert_line(false);
            }
            if ui.button("Çoğalt").clicked() {
                self.duplicate_selected();
            }
            if ui.button("Birleştir").clicked() {
                self.join_selected();
            }
            ui.separator();
            if ui.button("Bul").clicked() {
                self.win.find = true;
            }
            if ui.button("Kaydır").clicked() {
                self.win.shift = true;
            }
            if ui.button("Kalite").clicked() {
                self.win.qc = true;
            }
            if ui.button("Stiller").clicked() {
                self.win.styles = true;
            }
            ui.separator();
            let busy = self.run.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                ui.menu_button("Makrolar ▾", |ui| {
                    let scripts = self.scripts.clone();
                    let mut any = false;
                    for s in &scripts {
                        for (mi, (name, desc)) in s.macros.iter().enumerate() {
                            any = true;
                            if ui.button(name).on_hover_text(desc).clicked() {
                                self.run_macro(s.path.clone(), mi, name.clone());
                                ui.close_menu();
                            }
                        }
                    }
                    if !any {
                        ui.label("Makro bulunamadı — Otomasyon ▸ Betik yöneticisi");
                    }
                });
            });
            if let Some(r) = &self.run {
                ui.spinner();
                ui.label(format!("{}…", r.title));
            }
        });
    }

    /// Apply a text transform to the selected lines as one undo step.
    pub fn map_selected(&mut self, label: &str, f: impl Fn(&str) -> String) {
        let idx = self.selected_vec();
        let mut changed = 0;
        self.proj.edit(label, |s| {
            for &i in &idx {
                let new = f(&s.events[i].text);
                if new != s.events[i].text {
                    s.events[i].text = new;
                    changed += 1;
                }
            }
        });
        self.bump();
        self.set_status(format!("{changed} satır değişti"));
    }

    pub fn run_context(&self) -> RunContext {
        RunContext {
            fps: self.video.as_ref().and_then(|v| v.info.fps),
            video_size: self.video.as_ref().map(|v| (v.info.width, v.info.height)).or(Some(self.proj.script.play_res())),
            script_path: self.proj.path.clone(),
            video_path: self.video.as_ref().map(|v| v.path.clone()),
            audio_path: self.wave.as_ref().map(|w| w.path.clone()),
            keyframes: self.video.as_ref().map(|v| v.keyframes.clone()).unwrap_or_default(),
            include_dirs: self.settings.include_dirs(),
        }
    }

    pub fn run_macro(&mut self, path: std::path::PathBuf, idx: usize, name: String) {
        if self.run.is_some() {
            self.set_status("Başka bir makro çalışıyor");
            return;
        }
        let sel = self.selected_vec();
        let active = self.active.unwrap_or(0);
        let ctx = self.run_context();
        let handle = automation::spawn_macro(path, idx, self.proj.script.clone(), sel.clone(), active, ctx);
        self.log_line(format!("▶ {name}"));
        self.run = Some(crate::state::RunState { export_to: None, handle, title: name, progress: None, task: String::new(), dialog: None });
    }

    fn pump_run(&mut self, ctx: &egui::Context) {
        let Some(run) = &mut self.run else { return };
        let mut finished = None;
        let mut logs = vec![];
        while let Ok(m) = run.handle.rx.try_recv() {
            match m {
                RunMsg::Log(_, s) => logs.push(s),
                RunMsg::Progress(p) => run.progress = Some(p),
                RunMsg::Task(t) => run.task = t,
                RunMsg::Dialog(d, tx) => {
                    let mut values = std::collections::HashMap::new();
                    for f in &d.fields {
                        if let Some(v) = &f.value {
                            if !f.name.is_empty() {
                                values.insert(f.name.clone(), v.clone());
                            }
                        }
                    }
                    run.dialog = Some(crate::state::LuaDialogState { dialog: d, values, reply: tx });
                }
                RunMsg::Done(r) => finished = Some(r),
            }
        }
        for l in logs {
            self.log_line(l);
        }
        if finished.is_some() {
            ctx.request_repaint();
        }
        if let Some(r) = finished {
            let finished_run = self.run.take();
            let name = finished_run.as_ref().map(|r| r.title.clone()).unwrap_or_default();
            let export_to = finished_run.and_then(|r| r.export_to);
            match r {
                Ok(res) if export_to.is_some() => {
                    let path = export_to.unwrap();
                    match subforge_core::save_path(&res.script, &path) {
                        Ok(()) => self.set_status(format!("Filtrelerle dışa aktarıldı: {path}")),
                        Err(e) => self.set_status(format!("Dışa aktarma hatası: {e}")),
                    }
                }
                Ok(res) => {
                    let sel = automation::selection_to_events(&res);
                    let new_script = res.script;
                    self.proj.edit(&format!("Makro: {name}"), |s| *s = new_script);
                    self.bump();
                    self.clamp_selection();
                    if let Some(sel) = sel {
                        if !sel.is_empty() {
                            self.sel = sel.iter().copied().collect();
                            self.active = Some(sel[0]);
                        }
                    }
                    self.set_status(format!("Makro tamamlandı: {name}"));
                }
                Err(subforge_lua::ScriptError::Cancelled) => self.set_status(format!("Makro iptal edildi: {name}")),
                Err(subforge_lua::ScriptError::Failed(e)) => {
                    self.log_line(format!("HATA ({name}): {e}"));
                    self.win.log = true;
                    self.set_status(format!("Makro hata verdi: {name}"));
                }
            }
        }
    }
}
