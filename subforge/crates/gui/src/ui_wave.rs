use crate::state::{App, WaveDrag};
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use subforge_core::time::format_ass;

impl App {
    pub fn wave_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("wave").exact_height(self.wave_h).show(ctx, |ui| {
            let mut wave_h = self.wave_h;
            let Some(w) = self.wave.as_mut() else { return };
            let (peaks, done, window_ms, err) = {
                let g = w.wave.lock().unwrap();
                (g.peaks.len(), g.done, g.window_ms, g.error.clone())
            };
            ui.horizontal(|ui| {
                ui.label(format!("Ses: {}", w.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
                if !done {
                    ui.spinner();
                    ui.label(format!("çözülüyor… {:.0} sn", peaks as f64 * window_ms as f64 / 1000.0));
                }
                if let Some(e) = err {
                    ui.colored_label(theme::ERR, e);
                }
                ui.separator();
                if ui.button("−").on_hover_text("Uzaklaş").clicked() {
                    w.ms_per_px = (w.ms_per_px * 1.5).min(500.0);
                }
                if ui.button("+").on_hover_text("Yakınlaş").clicked() {
                    w.ms_per_px = (w.ms_per_px / 1.5).max(1.0);
                }
                ui.checkbox(&mut w.snap_keys, "Anahtar kareye yapış");
                ui.add(egui::Slider::new(&mut wave_h, 120.0..=520.0).text("yükseklik").show_value(false));
                if let Some((a, b)) = w.sel {
                    ui.separator();
                    ui.label(format!("Seçim {} → {} ({} ms)", format_ass(a), format_ass(b), b - a));
                    if ui.button("Çal").clicked() {
                        if let Some(mut c) = w.player.take() {
                            let _ = c.kill();
                        }
                        w.player = subforge_app::media::play_audio(&w.path, a, Some(b));
                    }
                }
            });
            let avail = ui.available_size_before_wrap();
            let (resp, painter) = ui.allocate_painter(egui::vec2(avail.x, (avail.y - 2.0).max(40.0)), Sense::click_and_drag());
            let rect = resp.rect;
            painter.rect_filled(rect, 2.0, Color32::from_rgb(14, 15, 18));
            let ms_per_px = w.ms_per_px;
            let t0 = w.start_ms;
            let x_to_ms = |x: f32| -> i64 { (t0 + (x - rect.left()) as f64 * ms_per_px).round() as i64 };
            let ms_to_x = |ms: i64| -> f32 { rect.left() + ((ms as f64 - t0) / ms_per_px) as f32 };

            // time ruler
            let step_candidates = [100i64, 250, 500, 1000, 2000, 5000, 10000, 30000, 60000];
            let step = step_candidates.iter().copied().find(|s| *s as f64 / ms_per_px > 70.0).unwrap_or(60000);
            let first = (t0 as i64 / step) * step;
            let mut t = first;
            while ms_to_x(t) < rect.right() {
                let x = ms_to_x(t);
                if x >= rect.left() {
                    painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.top() + 8.0)], Stroke::new(1.0, Color32::from_gray(90)));
                    painter.text(Pos2::new(x + 3.0, rect.top() + 1.0), egui::Align2::LEFT_TOP, format_ass(t), egui::FontId::monospace(10.0), Color32::from_gray(130));
                }
                t += step;
            }

            // subtitle blocks
            let mid = rect.center().y;
            for (i, e) in self.proj.script.events.iter().enumerate() {
                if e.end < t0 as i64 || e.start > x_to_ms(rect.right()) {
                    continue;
                }
                let r = Rect::from_min_max(Pos2::new(ms_to_x(e.start).max(rect.left()), rect.bottom() - 22.0), Pos2::new(ms_to_x(e.end).min(rect.right()), rect.bottom() - 2.0));
                let active = Some(i) == self.active;
                let selected = self.sel.contains(&i);
                let fill = if active { theme::ACCENT.linear_multiply(0.55) } else if selected { theme::ACCENT.linear_multiply(0.3) } else { Color32::from_rgb(70, 90, 140).linear_multiply(0.6) };
                painter.rect_filled(r, 2.0, fill);
                painter.rect_stroke(r, 2.0, Stroke::new(1.0, if active { theme::ACCENT } else { Color32::from_gray(80) }), egui::StrokeKind::Inside);
                if r.width() > 30.0 {
                    painter.text(r.left_center() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_CENTER, subforge_core::tags::visible_text(&e.text).replace('\n', " "), egui::FontId::proportional(11.0), Color32::WHITE);
                }
            }

            // waveform
            {
                let g = w.wave.lock().unwrap();
                if !g.peaks.is_empty() {
                    let amp_h = (rect.height() - 34.0) / 2.0;
                    let cy = rect.top() + 14.0 + amp_h;
                    let _ = mid;
                    let mut x = rect.left();
                    while x < rect.right() {
                        let a = (t0 + (x - rect.left()) as f64 * ms_per_px) / g.window_ms as f64;
                        let b = (t0 + (x + 1.0 - rect.left()) as f64 * ms_per_px) / g.window_ms as f64;
                        let (ia, ib) = (a.max(0.0) as usize, (b.max(a + 1.0)) as usize);
                        if ia < g.peaks.len() {
                            let (mut mn, mut mx) = (i16::MAX, i16::MIN);
                            for p in &g.peaks[ia..ib.min(g.peaks.len())] {
                                mn = mn.min(p.0);
                                mx = mx.max(p.1);
                            }
                            if mn <= mx {
                                let y1 = cy - (mx as f32 / 32768.0).min(1.0) * amp_h;
                                let y2 = cy - (mn as f32 / 32768.0).max(-1.0) * amp_h;
                                painter.line_segment([Pos2::new(x, y1), Pos2::new(x, y2.max(y1 + 1.0))], Stroke::new(1.0, Color32::from_rgb(80, 170, 200)));
                            }
                        }
                        x += 1.0;
                    }
                }
            }

            // keyframes
            if let Some(v) = &self.video {
                for &k in &v.keyframes {
                    let x = ms_to_x(k);
                    if x >= rect.left() && x <= rect.right() {
                        painter.line_segment([Pos2::new(x, rect.top() + 10.0), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 220, 120, 60)));
                    }
                }
            }

            // audio selection
            if let Some((a, b)) = w.sel {
                let r = Rect::from_min_max(Pos2::new(ms_to_x(a.min(b)), rect.top()), Pos2::new(ms_to_x(a.max(b)), rect.bottom()));
                painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 36));
            }
            // video cursor
            if let Some(v) = &self.video {
                let x = ms_to_x(v.want_ms);
                painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(235, 90, 90)));
            }

            // active line handles
            let handle_px = 6.0;
            let act = self.active.filter(|&i| i < self.proj.script.events.len());
            if let Some(i) = act {
                let e = &self.proj.script.events[i];
                for x in [ms_to_x(e.start), ms_to_x(e.end)] {
                    painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(2.0, theme::ACCENT));
                }
            }

            // ---- interaction
            if resp.hovered() {
                let (scroll, zoom_mod) = ctx.input(|i| (i.smooth_scroll_delta, i.modifiers.command));
                if zoom_mod && scroll.y != 0.0 {
                    let mouse_ms = resp.hover_pos().map(|p| x_to_ms(p.x)).unwrap_or(0);
                    let f = if scroll.y > 0.0 { 1.0 / 1.15 } else { 1.15 };
                    w.ms_per_px = (w.ms_per_px * f).clamp(1.0, 500.0);
                    if let Some(p) = resp.hover_pos() {
                        w.start_ms = (mouse_ms as f64 - (p.x - rect.left()) as f64 * w.ms_per_px).max(0.0);
                    }
                } else if scroll != egui::Vec2::ZERO {
                    w.start_ms = (w.start_ms - (scroll.x + scroll.y) as f64 * w.ms_per_px * 1.5).max(0.0);
                }
            }
            let mut goto: Option<usize> = None;
            let ptr = resp.interact_pointer_pos();
            if resp.drag_started() {
                if let Some(p) = ptr {
                    let ms = x_to_ms(p.x);
                    w.drag = Some(match act {
                        Some(i) if (p.x - ms_to_x(self.proj.script.events[i].start)).abs() < handle_px => WaveDrag::Start(i),
                        Some(i) if (p.x - ms_to_x(self.proj.script.events[i].end)).abs() < handle_px => WaveDrag::End(i),
                        Some(i) if ctx.input(|inp| inp.modifiers.alt) && ms >= self.proj.script.events[i].start && ms <= self.proj.script.events[i].end => WaveDrag::Move(i, ms - self.proj.script.events[i].start),
                        _ => WaveDrag::NewSel(ms),
                    });
                    if let Some(WaveDrag::Start(_) | WaveDrag::End(_) | WaveDrag::Move(..)) = w.drag {
                        self.proj.edit("Drag timing", |_| {});
                    }
                }
            }
            if resp.dragged() {
                if let (Some(p), Some(d)) = (ptr, w.drag) {
                    let mut ms = x_to_ms(p.x).max(0);
                    if w.snap_keys {
                        if let Some(v) = &self.video {
                            if let Some(&k) = v.keyframes.iter().find(|&&k| (ms_to_x(k) - p.x).abs() < 8.0) {
                                ms = k;
                            }
                        }
                    }
                    match d {
                        WaveDrag::NewSel(a) => w.sel = Some((a.min(ms), a.max(ms))),
                        WaveDrag::Start(i) => {
                            let e = &mut self.proj.script.events[i];
                            e.start = ms.min(e.end);
                            self.version += 1;
                        }
                        WaveDrag::End(i) => {
                            let e = &mut self.proj.script.events[i];
                            e.end = ms.max(e.start);
                            self.version += 1;
                        }
                        WaveDrag::Move(i, off) => {
                            let e = &mut self.proj.script.events[i];
                            let d = e.end - e.start;
                            e.start = (ms - off).max(0);
                            e.end = e.start + d;
                            self.version += 1;
                        }
                    }
                    // autoscroll near edges
                    if p.x > rect.right() - 20.0 {
                        w.start_ms += 8.0 * w.ms_per_px;
                    } else if p.x < rect.left() + 20.0 {
                        w.start_ms = (w.start_ms - 8.0 * w.ms_per_px).max(0.0);
                    }
                }
            }
            if resp.drag_stopped() {
                w.drag = None;
            }
            if resp.clicked() {
                if let Some(p) = ptr {
                    let ms = x_to_ms(p.x);
                    // click on a subtitle block selects it; otherwise seek
                    let hit = self.proj.script.events.iter().position(|e| p.y > rect.bottom() - 24.0 && ms >= e.start && ms <= e.end);
                    if let Some(i) = hit {
                        goto = Some(i);
                    } else if let Some(v) = &mut self.video {
                        v.want_ms = ms;
                    }
                    w.sel = None;
                }
            }
            if resp.hovered() {
                if let Some(p) = resp.hover_pos() {
                    if let Some(i) = act {
                        let e = &self.proj.script.events[i];
                        if (p.x - ms_to_x(e.start)).abs() < handle_px || (p.x - ms_to_x(e.end)).abs() < handle_px {
                            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        }
                    }
                    resp.clone().on_hover_text_at_pointer(format_ass(x_to_ms(p.x)));
                }
            }
            // commit selection to the active line with Enter / button via context menu
            let commit = ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.command) && !ctx.wants_keyboard_input();
            let sel = w.sel;
            resp.context_menu(|ui| {
                if let (Some((a, b)), Some(i)) = (sel, act) {
                    if ui.button("Seçimi aktif satıra uygula").clicked() {
                        self.proj.edit("Set times from audio", |s| {
                            s.events[i].start = a;
                            s.events[i].end = b;
                        });
                        self.version += 1;
                        ui.close_menu();
                    }
                }
            });
            if commit {
                if let (Some((a, b)), Some(i)) = (sel, act) {
                    self.proj.edit("Set times from audio", |s| {
                        s.events[i].start = a;
                        s.events[i].end = b;
                    });
                    self.version += 1;
                    if let Some(w) = self.wave.as_mut() {
                        w.sel = None;
                    }
                }
            }
            if let Some(i) = goto {
                self.goto_line(i);
            }
            self.wave_h = wave_h;
        });
    }
}
