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
                ui.image((t.id(), size * scale));
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
        // buttons: set start/end from video position
        if let (Some(i), Some(v)) = (self.active, self.video.as_ref()) {
            let _ = (i, v);
        }
    }
}

#[allow(dead_code)]
fn _c() -> Color32 {
    Color32::WHITE
}
