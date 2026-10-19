use crate::state::App;
use crate::theme;
use eframe::egui::{self, Color32, ColorImage, TextureOptions};
use subforge_app::media;
use subforge_core::time::{format_ass, Fps};

impl App {
    /// Request a frame render in the background (latest request wins).
    fn request_frame(&mut self) {
        let Some(v) = self.video.as_mut() else { return };
        if v.pending.is_some() {
            return;
        }
        let ms = v.want_ms.max(0);
        if v.shown_ms == ms && v.shown_version == self.version {
            return;
        }
        // Make the burned-in preview reflect the current script.
        let ass_path = if v.burn {
            let text = self.proj.script.to_ass();
            let _ = std::fs::write(&self.tmp_ass, text);
            Some(self.tmp_ass.clone())
        } else {
            None
        };
        let (path, w, sw, sh) = (v.path.clone(), v.width, v.info.width, v.info.height);
        let (tx, rx) = std::sync::mpsc::channel();
        v.pending = Some(rx);
        v.shown_ms = ms;
        v.shown_version = self.version;
        std::thread::spawn(move || {
            let _ = tx.send(media::grab_frame(&path, ms, w, sw, sh, ass_path.as_deref(), None));
        });
    }

    pub fn video_panel(&mut self, ctx: &egui::Context) {
        // sync the video cursor to the active line start when it changes
        self.request_frame();
        if let Some(v) = self.video.as_mut() {
            if let Some(rx) = &v.pending {
                if let Ok(res) = rx.try_recv() {
                    v.pending = None;
                    match res {
                        Ok(f) => {
                            let img = ColorImage::from_rgba_unmultiplied([f.width, f.height], &f.rgba);
                            v.tex = Some(ctx.load_texture("video", img, TextureOptions::LINEAR));
                            v.error = None;
                        }
                        Err(e) => v.error = Some(e),
                    }
                }
            }
        }
        let fps = Fps::from_f64(self.fps());
        let mut step: i64 = 0;
        let mut seek: Option<i64> = None;
        let mut pos_click: Option<(f32, f32)> = None;
        let mut set_start = false;
        let mut set_end = false;
        egui::SidePanel::left("video").resizable(true).default_width(520.0).min_width(260.0).show(ctx, |ui| {
            let Some(v) = self.video.as_mut() else { return };
            ui.horizontal(|ui| {
                ui.label(format!("Video: {}", v.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
                ui.checkbox(&mut v.burn, "ASS önizleme");
            });
            let avail_w = ui.available_width();
            let target_w = (avail_w as i32 / 2 * 2).clamp(160, 1920);
            if (v.width - target_w).abs() > 40 {
                v.width = target_w;
                v.shown_ms = -1; // re-render at the new size
            }
            if let Some(t) = &v.tex {
                let size = t.size_vec2();
                let scale = avail_w / size.x;
                let img = egui::Image::new((t.id(), size * scale)).sense(egui::Sense::click_and_drag());
                let r = ui.add(img);
                if v.pos_mode {
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    if let Some(p) = r.interact_pointer_pos().filter(|_| r.dragged() || r.clicked()) {
                        let rel = (p - r.rect.min) / r.rect.size();
                        pos_click = Some((rel.x.clamp(0.0, 1.0), rel.y.clamp(0.0, 1.0)));
                    }
                }
            } else {
                ui.allocate_space(egui::vec2(avail_w, avail_w * 9.0 / 16.0));
            }
            if let Some(e) = &v.error {
                ui.colored_label(theme::ERR, e);
            }
            let frame = fps.frame_at(v.want_ms);
            ui.horizontal(|ui| {
                ui.monospace(format!("{}  kare {}", format_ass(v.want_ms), frame));
                if v.pending.is_some() {
                    ui.spinner();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("−1 sn").clicked() {
                    seek = Some(v.want_ms - 1000);
                }
                if ui.button("−1 kare").clicked() {
                    step = -1;
                }
                if ui.button("+1 kare").clicked() {
                    step = 1;
                }
                if ui.button("+1 sn").clicked() {
                    seek = Some(v.want_ms + 1000);
                }
            });
            let mut ms = v.want_ms as f64;
            let max = v.info.duration_ms.max(1) as f64;
            if ui.add(egui::Slider::new(&mut ms, 0.0..=max).show_value(false)).changed() {
                seek = Some(ms as i64);
            }
            ui.horizontal(|ui| {
                if ui.button("Başlangıç = şimdi  [").on_hover_text("Aktif satırın başlangıcını bu kareye ayarla ( [ )").clicked() {
                    set_start = true;
                }
                if ui.button("Bitiş = şimdi  ]").on_hover_text("Aktif satırın bitişini bu kareye ayarla ( ] )").clicked() {
                    set_end = true;
                }
            });
            ui.checkbox(&mut v.pos_mode, "Konumlandır (videoya tıkla/sürükle = \\pos)");
            ui.horizontal(|ui| {
                if ui.button("Önceki anahtar kare").clicked() {
                    seek = v.keyframes.iter().rev().find(|&&k| k < v.want_ms - 1).copied();
                }
                if ui.button("Sonraki anahtar kare").clicked() {
                    seek = v.keyframes.iter().find(|&&k| k > v.want_ms + 1).copied();
                }
            });
        });
        if let Some(v) = self.video.as_mut() {
            if step != 0 {
                let f = fps.frame_at(v.want_ms) + step;
                v.want_ms = fps.start_ms(f.max(0));
            }
            if let Some(s) = seek {
                v.want_ms = s.clamp(0, v.info.duration_ms.max(0));
            }
        }
        // hotkeys: [ and ] set start/end from the video position
        if !ctx.wants_keyboard_input() {
            ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::OpenBracket) {
                    set_start = true;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::CloseBracket) {
                    set_end = true;
                }
            });
        }
        let now = self.video.as_ref().map(|v| v.want_ms);
        if let (Some(i), Some(now)) = (self.active, now) {
            if (set_start || set_end) && i < self.proj.script.events.len() {
                self.proj.edit("Set time from video", |s| {
                    let e = &mut s.events[i];
                    if set_start {
                        e.start = now;
                        if e.end < e.start {
                            e.end = e.start;
                        }
                    }
                    if set_end {
                        e.end = now.max(e.start);
                    }
                });
                self.bump();
            }
            if let Some((rx, ry)) = pos_click {
                let (w, h) = self.proj.script.play_res();
                let (x, y) = ((rx * w as f32).round() as i32, (ry * h as f32).round() as i32);
                let sel = self.selected_vec();
                let targets = if sel.contains(&i) { sel } else { vec![i] };
                self.proj.edit_coalesced("pos", |s| {
                    for &t in &targets {
                        s.events[t].text = subforge_core::tools::set_override(&s.events[t].text, "pos", &format!("({x},{y})"));
                    }
                });
                self.bump();
                self.set_status(format!("\\pos({x},{y})"));
            }
        }
    }
}

#[allow(dead_code)]
fn _c() -> Color32 {
    Color32::WHITE
}
