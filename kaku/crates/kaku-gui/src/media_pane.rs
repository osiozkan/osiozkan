//! Video preview + audio waveform pane. Heavy work (ffmpeg) runs on background threads.

use egui::{Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2};
use kaku_media::{MediaInfo, Waveform};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

pub enum Event {
    Seek(i64),
    /// (line index, new start)
    SetStart(usize, i64),
    SetEnd(usize, i64),
    DragBegin,
    Select(usize),
}

pub struct LineSpan {
    pub idx: usize,
    pub start: i64,
    pub end: i64,
    pub active: bool,
    pub selected: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Start,
    End,
}

pub struct MediaPane {
    pub path: Option<PathBuf>,
    pub info: Option<MediaInfo>,
    pub wave: Option<Waveform>,
    pub keyframes: Vec<i64>,
    pub status: String,
    pub playhead: i64,
    pub view_start: i64,
    pub ms_per_px: f32,
    pub snap_keyframes: bool,
    load_rx: Option<Receiver<Result<(MediaInfo, Waveform, Vec<i64>), String>>>,
    frame_rx: Option<Receiver<Result<Vec<u8>, String>>>,
    tex: Option<TextureHandle>,
    pub frame_wanted: Option<(i64, String)>,
    frame_busy: bool,
    drag: Option<(usize, Handle)>,
    play: Option<std::process::Child>,
    pub follow: bool,
}

impl MediaPane {
    pub fn new() -> Self {
        MediaPane {
            path: None,
            info: None,
            wave: None,
            keyframes: vec![],
            status: String::new(),
            playhead: 0,
            view_start: 0,
            ms_per_px: 20.0,
            snap_keyframes: true,
            load_rx: None,
            frame_rx: None,
            tex: None,
            frame_wanted: None,
            frame_busy: false,
            drag: None,
            play: None,
            follow: true,
        }
    }

    pub fn load(&mut self, path: PathBuf, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.load_rx = Some(rx);
        self.path = Some(path.clone());
        self.info = None;
        self.wave = None;
        self.keyframes.clear();
        self.status = "Loading media…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = (|| {
                let info = kaku_media::probe(&path).map_err(|e| e.to_string())?;
                let wave = kaku_media::waveform(&path, info.audio.first().map(|a| a.index), 10).unwrap_or_default();
                let kf = if info.has_video { kaku_media::keyframes(&path).unwrap_or_default() } else { vec![] };
                Ok((info, wave, kf))
            })();
            let _ = tx.send(r);
            ctx.request_repaint();
        });
    }

    /// Request a frame at `ms`, burning `ass_text` (the current script) in.
    pub fn request_frame(&mut self, ms: i64, ass_text: String) {
        self.frame_wanted = Some((ms, ass_text));
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.load_rx {
            if let Ok(r) = rx.try_recv() {
                self.load_rx = None;
                match r {
                    Ok((info, wave, kf)) => {
                        self.status = format!("{} · {} keyframes · {} audio stream(s)", if info.has_video { format!("{}x{}", info.width, info.height) } else { "audio only".into() }, kf.len(), info.audio.len());
                        self.info = Some(info);
                        self.wave = Some(wave);
                        self.keyframes = kf;
                    }
                    Err(e) => self.status = format!("Media error: {e}"),
                }
            }
        }
        if let Some(rx) = &self.frame_rx {
            if let Ok(r) = rx.try_recv() {
                self.frame_rx = None;
                self.frame_busy = false;
                if let Ok(png) = r {
                    if let Ok(img) = image::load_from_memory_with_format(&png, image::ImageFormat::Png) {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let ci = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        match &mut self.tex {
                            Some(t) => t.set(ci, Default::default()),
                            None => self.tex = Some(ctx.load_texture("frame", ci, Default::default())),
                        }
                    }
                }
            }
        }
        if !self.frame_busy {
            if let (Some((ms, text)), Some(path), Some(info)) = (self.frame_wanted.take(), self.path.clone(), self.info.as_ref()) {
                if info.has_video {
                    let (tx, rx) = channel();
                    self.frame_rx = Some(rx);
                    self.frame_busy = true;
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let tmp = std::env::temp_dir().join(format!("kaku-preview-{}.ass", std::process::id()));
                        let r = std::fs::write(&tmp, text).map_err(|e| e.to_string()).and_then(|_| kaku_media::render_frame(&path, ms, Some(&tmp), None, 960).map_err(|e| e.to_string()));
                        let _ = tx.send(r);
                        ctx.request_repaint();
                    });
                }
            }
        }
        if let Some(c) = &mut self.play {
            if matches!(c.try_wait(), Ok(Some(_))) {
                self.play = None;
            }
        }
    }

    pub fn play(&mut self, a: i64, b: i64) {
        self.stop();
        if let Some(p) = &self.path {
            match kaku_media::play_range(p, a, b) {
                Ok(c) => self.play = Some(c),
                Err(e) => self.status = e.to_string(),
            }
        }
    }
    pub fn stop(&mut self) {
        if let Some(mut c) = self.play.take() {
            let _ = c.kill();
        }
    }
    pub fn playing(&self) -> bool {
        self.play.is_some()
    }

    pub fn video_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(t) = &self.tex {
            let w = ui.available_width();
            let sz = t.size_vec2();
            let h = w * sz.y / sz.x.max(1.0);
            ui.image((t.id(), Vec2::new(w, h)));
        } else if self.info.as_ref().map_or(false, |i| i.has_video) {
            ui.label("Rendering frame…");
        }
    }

    pub fn waveform_ui(&mut self, ui: &mut egui::Ui, spans: &[LineSpan]) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(wave) = &self.wave else {
            ui.label(if self.load_rx.is_some() { "Decoding audio…" } else { "No audio loaded (Media ▸ Open video/audio…)" });
            return events;
        };
        let height = ui.available_height().max(60.0);
        let (resp, painter) = ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, Color32::from_gray(18));
        let mpp = self.ms_per_px.max(0.5);
        // zoom with ctrl+scroll, pan with scroll
        if resp.hovered() {
            let (scroll, zoom, ctrl, pointer) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command, i.pointer.hover_pos()));
            if (zoom - 1.0).abs() > 1e-3 || (ctrl && scroll.y != 0.0) {
                let f = if ctrl && scroll.y != 0.0 { (1.0 - scroll.y * 0.002).clamp(0.5, 2.0) } else { 1.0 / zoom };
                if let Some(p) = pointer {
                    let t_under = self.view_start as f32 + (p.x - rect.left()) * mpp;
                    self.ms_per_px = (mpp * f).clamp(0.5, 200.0);
                    self.view_start = (t_under - (p.x - rect.left()) * self.ms_per_px) as i64;
                }
            } else if scroll.x != 0.0 || scroll.y != 0.0 {
                self.view_start -= ((scroll.x + scroll.y) * mpp) as i64;
            }
        }
        let mpp = self.ms_per_px;
        self.view_start = self.view_start.clamp(0, (wave.duration_ms() - 1000).max(0));
        let t0 = self.view_start;
        let x_of = |t: i64| rect.left() + (t - t0) as f32 / mpp;
        let t_of = |x: f32| t0 + ((x - rect.left()) * mpp) as i64;
        let mid = rect.center().y;
        let amp = rect.height() * 0.5 - 2.0;
        // waveform
        let px = rect.width() as usize;
        for x in 0..px {
            let a = t0 + (x as f32 * mpp) as i64;
            let (lo, hi) = wave.range(a, a + mpp.ceil() as i64);
            let (l, h) = (lo as f32 / 32768.0, hi as f32 / 32768.0);
            painter.line_segment([Pos2::new(rect.left() + x as f32, mid - h * amp), Pos2::new(rect.left() + x as f32, mid - l * amp)], Stroke::new(1.0, Color32::from_rgb(70, 150, 120)));
        }
        // keyframes
        let (vis_a, vis_b) = (t0, t0 + (rect.width() * mpp) as i64);
        for &k in self.keyframes.iter().filter(|&&k| k >= vis_a && k <= vis_b) {
            let x = x_of(k);
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 255, 90)));
        }
        // line spans
        let edge = 5.0;
        let pointer = resp.hover_pos();
        let mut hover_handle: Option<(usize, Handle)> = None;
        for s in spans.iter().filter(|s| s.end >= vis_a && s.start <= vis_b) {
            let r = Rect::from_min_max(Pos2::new(x_of(s.start), rect.top()), Pos2::new(x_of(s.end), rect.bottom()));
            let (fill, stroke) = if s.active { (Color32::from_rgba_unmultiplied(255, 200, 60, 50), Color32::from_rgb(255, 200, 60)) } else if s.selected { (Color32::from_rgba_unmultiplied(100, 160, 255, 40), Color32::from_rgb(100, 160, 255)) } else { (Color32::from_rgba_unmultiplied(255, 255, 255, 14), Color32::from_gray(110)) };
            painter.rect_filled(r, 0.0, fill);
            painter.line_segment([r.left_top(), r.left_bottom()], Stroke::new(2.0, stroke));
            painter.line_segment([r.right_top(), r.right_bottom()], Stroke::new(2.0, stroke));
            if let Some(p) = pointer {
                if (p.x - r.left()).abs() <= edge && s.active {
                    hover_handle = Some((s.idx, Handle::Start));
                } else if (p.x - r.right()).abs() <= edge && s.active {
                    hover_handle = Some((s.idx, Handle::End));
                }
            }
        }
        // playhead
        let ph = x_of(self.playhead);
        painter.line_segment([Pos2::new(ph, rect.top()), Pos2::new(ph, rect.bottom())], Stroke::new(1.5, Color32::from_rgb(255, 80, 80)));
        if hover_handle.is_some() || self.drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.drag_started() {
            if let Some(h) = hover_handle {
                self.drag = Some(h);
                events.push(Event::DragBegin);
            }
        }
        if let (Some((idx, h)), true) = (self.drag, resp.dragged()) {
            if let Some(p) = pointer.or(resp.interact_pointer_pos()) {
                let mut t = t_of(p.x).max(0);
                if self.snap_keyframes && !ui.input(|i| i.modifiers.alt) {
                    if let Some(&k) = self.keyframes.iter().min_by_key(|&&k| (k - t).abs()) {
                        if ((k - t).abs() as f32) / mpp < 6.0 {
                            t = k;
                        }
                    }
                }
                events.push(if h == Handle::Start { Event::SetStart(idx, t) } else { Event::SetEnd(idx, t) });
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
        if resp.clicked() && self.drag.is_none() && hover_handle.is_none() {
            if let Some(p) = resp.interact_pointer_pos() {
                let t = t_of(p.x).max(0);
                self.playhead = t;
                events.push(Event::Seek(t));
                if let Some(s) = spans.iter().find(|s| t >= s.start && t < s.end) {
                    events.push(Event::Select(s.idx));
                }
            }
        }
        events
    }
}
