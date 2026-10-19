use crate::state::App;
use crate::theme;
use eframe::egui::{self, Color32, ColorImage, TextureOptions};
use subforge_app::media;
use subforge_core::time::{format_ass, Fps};

impl App {
    /// Request a frame render in the background (latest request wins).
    fn request_frame(&mut self) {
        let Some(v) = self.video.as_mut() else { return };
        if v.pending.is_some() || v.play.is_some() {
            return;
        }
        let ms = v.want_ms.max(0);
        if v.shown_ms == ms && v.shown_version == self.version {
            return;
        }
        // Make the burned-in preview reflect the current script.
        let ass_path = if v.burn {
            let text = media::preview_script(&self.proj.script, ms - 50, ms + 50).to_ass();
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
        self.pump_playback(ctx);
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
        let mut vis_action: Option<(u8, f32, f32, f32, f32, f64, f64)> = None;
        let mut set_start = false;
        let mut toggle_play = false;
        let mut play_line = false;
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
                let (pw, ph) = self.proj.script.play_res();
                let to_screen = |x: f64, y: f64| egui::pos2(r.rect.left() + (x / pw as f64) as f32 * r.rect.width(), r.rect.top() + (y / ph as f64) as f32 * r.rect.height());
                // overlay of the active line's positioning tags
                if let Some(e) = self.active.and_then(|i| self.proj.script.events.get(i)) {
                    let painter = ui.painter_at(r.rect);
                    let nums = |v: &str| -> Vec<f64> { v.trim().trim_matches(|c| c == '(' || c == ')').split(',').filter_map(|x| x.trim().parse().ok()).collect() };
                    let acc = theme::ACCENT;
                    if let Some(p) = subforge_core::tools::get_override(&e.text, "pos").map(|v| nums(&v)).filter(|n| n.len() == 2) {
                        let c = to_screen(p[0], p[1]);
                        painter.line_segment([c - egui::vec2(8.0, 0.0), c + egui::vec2(8.0, 0.0)], egui::Stroke::new(1.5_f32, acc));
                        painter.line_segment([c - egui::vec2(0.0, 8.0), c + egui::vec2(0.0, 8.0)], egui::Stroke::new(1.5_f32, acc));
                    }
                    if let Some(o) = subforge_core::tools::get_override(&e.text, "org").map(|v| nums(&v)).filter(|n| n.len() == 2) {
                        painter.circle_stroke(to_screen(o[0], o[1]), 6.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(120, 200, 255)));
                    }
                    if let Some(c) = subforge_core::tools::get_override(&e.text, "clip").map(|v| nums(&v)).filter(|n| n.len() == 4) {
                        painter.rect_stroke(egui::Rect::from_two_pos(to_screen(c[0], c[1]), to_screen(c[2], c[3])), 0.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(120, 255, 160)), egui::StrokeKind::Inside);
                    }
                }
                if v.vmode != 0 {
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    let rel_of = |p: egui::Pos2| ((p.x - r.rect.left()) / r.rect.width()).clamp(0.0, 1.0);
                    let rel_y = |p: egui::Pos2| ((p.y - r.rect.top()) / r.rect.height()).clamp(0.0, 1.0);
                    if r.drag_started() || r.clicked() {
                        if let Some(p) = r.interact_pointer_pos() {
                            let (base_a, base_b) = self.active.and_then(|i| self.proj.script.events.get(i)).map(|e| {
                                let f = |n: &str, d: f64| subforge_core::tools::get_override(&e.text, n).and_then(|v| v.trim().parse().ok()).unwrap_or(d);
                                (f("frz", 0.0), f("fscx", 100.0))
                            }).unwrap_or((0.0, 100.0));
                            v.drag0 = Some((rel_of(p), rel_y(p), base_a, base_b));
                        }
                    }
                    if let Some(p) = r.interact_pointer_pos().filter(|_| r.dragged() || r.clicked()) {
                        if let Some((x0, y0, ba, bb)) = v.drag0 {
                            vis_action = Some((v.vmode, x0, y0, rel_of(p), rel_y(p), ba, bb));
                        }
                    }
                    if r.drag_stopped() {
                        v.drag0 = None;
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
                let playing = v.play.is_some();
                if ui.button(if playing { "Durdur (Boşluk)" } else { "Oynat (Boşluk)" }).clicked() {
                    toggle_play = true;
                }
                if ui.button("Satırı oynat").on_hover_text("Aktif satırı 0,5 sn öncesinden bitişine kadar oynat").clicked() {
                    play_line = true;
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
            ui.spacing_mut().slider_width = (avail_w - 24.0).max(100.0);
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
            ui.horizontal_wrapped(|ui| {
                ui.label("Görsel araç:");
                for (m, label, tip) in [(0u8, "Kapalı", ""), (1, "Konum", "tıkla/sürükle = \\pos"), (2, "Merkez", "tıkla = \\org"), (3, "Kırp", "sürükle = \\clip dikdörtgen"), (4, "Döndür", "yatay sürükle = \\frz"), (5, "Ölçek", "yatay sürükle = \\fscx/\\fscy")] {
                    ui.selectable_value(&mut v.vmode, m, label).on_hover_text(tip);
                }
            });
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
            if (step != 0 || seek.is_some()) && v.play.is_some() {
                if let Some(mut p) = v.play.take() {
                    p.stop();
                }
            }
            if step != 0 {
                let f = fps.frame_at(v.want_ms) + step;
                v.want_ms = fps.start_ms(f.max(0));
            }
            if let Some(s) = seek {
                v.want_ms = s.clamp(0, v.info.duration_ms.max(0));
            }
        }
        // Space toggles playback when not typing
        if !ctx.wants_keyboard_input() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
            toggle_play = true;
        }
        if toggle_play {
            self.toggle_playback(None);
        }
        if play_line {
            if let Some(e) = self.active_event() {
                let (a, b) = ((e.start - 500).max(0), e.end + 300);
                self.toggle_playback(Some((a, b)));
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
            if let Some((mode, x0, y0, x1, y1, base_rot, base_scale)) = vis_action {
                let (w, h) = self.proj.script.play_res();
                let sx = |r: f32| (r * w as f32).round() as i32;
                let sy = |r: f32| (r * h as f32).round() as i32;
                let sel = self.selected_vec();
                let targets = if sel.contains(&i) { sel } else { vec![i] };
                let (tag, value, msg): (&str, String, String) = match mode {
                    1 => ("pos", format!("({},{})", sx(x1), sy(y1)), format!("\\pos({},{})", sx(x1), sy(y1))),
                    2 => ("org", format!("({},{})", sx(x1), sy(y1)), format!("\\org({},{})", sx(x1), sy(y1))),
                    3 => ("clip", format!("({},{},{},{})", sx(x0.min(x1)), sy(y0.min(y1)), sx(x0.max(x1)), sy(y0.max(y1))), "\\clip".to_string()),
                    4 => {
                        let deg = (base_rot - ((x1 - x0) as f64) * 360.0).round();
                        ("frz", format!("{deg}"), format!("\\frz{deg}"))
                    }
                    _ => {
                        let sc = (base_scale * (1.0 + (x1 - x0) as f64 * 2.0)).max(1.0).round();
                        ("fscx", format!("{sc}"), format!("\\fscx{sc} \\fscy{sc}"))
                    }
                };
                self.proj.edit_coalesced(&format!("vis:{mode}"), |s| {
                    for &t in &targets {
                        let mut text = subforge_core::tools::set_override(&s.events[t].text, tag, &value);
                        if mode == 5 {
                            text = subforge_core::tools::set_override(&text, "fscy", &value);
                        }
                        s.events[t].text = text;
                    }
                });
                self.bump();
                self.set_status(msg);
            }
        }
    }
}

#[allow(dead_code)]
fn _c() -> Color32 {
    Color32::WHITE
}

impl App {
    pub fn toggle_playback(&mut self, range: Option<(i64, i64)>) {
        let from = range.map(|r| r.0).or(self.video.as_ref().map(|v| v.want_ms)).unwrap_or(0);
        let text = media::preview_script(&self.proj.script, from - 100, from + 180_000).to_ass();
        let _ = std::fs::write(&self.tmp_ass, text);
        let tmp = self.tmp_ass.clone();
        let Some(v) = self.video.as_mut() else { return };
        if v.play.is_some() && range.is_none() {
            if let Some(mut p) = v.play.take() {
                p.stop();
            }
            return;
        }
        if let Some(mut p) = v.play.take() {
            p.stop();
        }
        let (start, end) = match range {
            Some((a, b)) => (a, Some(b)),
            None => (v.want_ms, None),
        };
        let fps = v.info.fps.unwrap_or(24.0);
        let audio = self.wave.is_some();
        match media::start_playback(&v.path, start, end, v.width.clamp(160, 960), v.info.width, v.info.height, fps, if v.burn { Some(tmp.as_path()) } else { None }, audio) {
            Ok(p) => {
                v.play = Some(p);
                v.play_until = end;
            }
            Err(e) => v.error = Some(e),
        }
    }

    fn pump_playback(&mut self, ctx: &egui::Context) {
        let Some(v) = self.video.as_mut() else { return };
        let Some(p) = v.play.as_mut() else { return };
        let mut last = None;
        let mut ended = false;
        loop {
            match p.rx.try_recv() {
                Ok(f) => last = Some(f),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        if let Some(f) = last {
            let img = ColorImage::from_rgba_unmultiplied([f.width, f.height], &f.rgba);
            v.tex = Some(ctx.load_texture("video", img, TextureOptions::LINEAR));
        }
        let now = p.now_ms();
        v.want_ms = now;
        if v.play_until.map(|u| now > u).unwrap_or(false) {
            ended = true;
        }
        if ended {
            if let Some(mut p) = v.play.take() {
                p.stop();
            }
            v.shown_ms = -1;
        } else {
            ctx.request_repaint();
        }
        // keep the audio view following the playhead
        let want = v.want_ms;
        if let Some(w) = self.wave.as_mut() {
            let width_ms = w.ms_per_px * 900.0;
            if (want as f64) > w.start_ms + width_ms * 0.9 || (want as f64) < w.start_ms {
                w.start_ms = (want as f64 - width_ms * 0.1).max(0.0);
            }
        }
    }
}
