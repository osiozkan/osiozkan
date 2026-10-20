//! Video preview and audio waveform panels (ffmpeg-backed; see `nsub-media`).

use crate::i18n::tr;
use crate::state::App;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use nsub_media::{Frame, Peaks, VideoInfo};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

type Key = (i64, u64, bool, u32, u32);

struct Req {
    key: Key,
    video: Option<PathBuf>,
    time_ms: i64,
    size: (u32, u32),
    ass: Option<PathBuf>,
}

struct Resp {
    key: Key,
    frame: Result<Frame, String>,
}

pub struct Media {
    pub video: Option<(PathBuf, VideoInfo)>,
    pub audio: Option<PathBuf>,
    pub peaks: Arc<Mutex<Peaks>>,
    pub peaks_loading: Arc<AtomicBool>,
    pub time_ms: i64,
    pub show_subs: bool,
    pub follow_selection: bool,
    /// Visible span of the waveform in ms.
    pub wave_span_ms: i64,
    pub wave_center_ms: i64,
    pub error: Option<String>,
    tx: Sender<Req>,
    rx: Receiver<Resp>,
    tex: Option<egui::TextureHandle>,
    tex_key: Option<Key>,
    requested: Option<Key>,
    tmp_dir: PathBuf,
    written_rev: Option<u64>,
    drag: Option<Marker>,
}

#[derive(Clone, Copy, PartialEq)]
enum Marker {
    Start,
    End,
}

impl Media {
    pub fn new(ctx: egui::Context) -> Media {
        let (tx, req_rx) = channel::<Req>();
        let (resp_tx, rx) = channel::<Resp>();
        std::thread::Builder::new()
            .name("frame-worker".into())
            .spawn(move || {
                while let Ok(mut req) = req_rx.recv() {
                    while let Ok(newer) = req_rx.try_recv() {
                        req = newer; // latest request wins
                    }
                    let frame = match (&req.video, &req.ass) {
                        (Some(v), a) => nsub_media::grab_frame(v, req.time_ms, req.size, a.as_deref(), None),
                        (None, Some(a)) => nsub_media::render_on_blank(a, req.time_ms, req.size, 0x1e1e1e),
                        (None, None) => Err("nothing to show".into()),
                    };
                    let _ = resp_tx.send(Resp { key: req.key, frame });
                    ctx.request_repaint();
                }
            })
            .expect("spawn frame worker");
        let tmp_dir = std::env::temp_dir().join(format!("nightsub-preview-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        Media {
            video: None,
            audio: None,
            peaks: Arc::new(Mutex::new(Peaks::default())),
            peaks_loading: Arc::new(AtomicBool::new(false)),
            time_ms: 0,
            show_subs: true,
            follow_selection: true,
            wave_span_ms: 12_000,
            wave_center_ms: 0,
            error: None,
            tx,
            rx,
            tex: None,
            tex_key: None,
            requested: None,
            tmp_dir,
            written_rev: None,
            drag: None,
        }
    }

    pub fn fps(&self) -> Option<f64> {
        self.video.as_ref().map(|v| v.1.fps).filter(|f| *f > 0.0)
    }

    pub fn load_video(&mut self, path: PathBuf, ctx: &egui::Context) {
        match nsub_media::probe(&path) {
            Ok(info) => {
                self.video = Some((path.clone(), info));
                self.error = None;
                self.tex_key = None;
                self.requested = None;
                if self.audio.is_none() {
                    self.load_audio(path, ctx);
                }
            }
            Err(e) => self.error = Some(format!("Video: {e}")),
        }
    }

    pub fn load_audio(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.audio = Some(path.clone());
        let peaks = self.peaks.clone();
        let loading = self.peaks_loading.clone();
        *peaks.lock().unwrap() = Peaks::default();
        loading.store(true, Ordering::Relaxed);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let p2 = peaks.clone();
            let c2 = ctx.clone();
            let res = nsub_media::audio_peaks(&path, 100, move |partial| {
                *p2.lock().unwrap() = partial.clone();
                c2.request_repaint();
            });
            if let Ok(p) = res {
                *peaks.lock().unwrap() = p;
            }
            loading.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }

    pub fn seek(&mut self, ms: i64) {
        let max = self.video.as_ref().map(|v| v.1.duration_ms).filter(|d| *d > 0).unwrap_or(i64::MAX);
        self.time_ms = ms.clamp(0, max);
    }

    pub fn step_frames(&mut self, n: i64) {
        match self.fps() {
            Some(fps) => {
                let fr = nsub_core::time::FrameRate::new(fps);
                let f = fr.frame_from_ms(self.time_ms) + n;
                self.seek(fr.ms_from_frame(f.max(0)) + 1);
            }
            None => self.seek(self.time_ms + n * 40),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(r) = self.rx.try_recv() {
            match r.frame {
                Ok(f) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
                    match &mut self.tex {
                        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                        None => self.tex = Some(ctx.load_texture("video", img, egui::TextureOptions::LINEAR)),
                    }
                    self.tex_key = Some(r.key);
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.tex_key = Some(r.key);
                }
            }
        }
    }

    fn request(&mut self, app: &App, size: (u32, u32)) {
        let want_subs = self.show_subs;
        let key: Key = (self.time_ms, app.doc.rev, want_subs, size.0, size.1);
        if self.tex_key == Some(key) || self.requested == Some(key) {
            return;
        }
        let mut ass = None;
        if want_subs {
            let path = self.tmp_dir.join(format!("preview-{}.ass", app.doc.rev));
            if self.written_rev != Some(app.doc.rev) {
                if std::fs::write(&path, app.doc.script.to_ass_string()).is_ok() {
                    if let Some(old) = self.written_rev {
                        let _ = std::fs::remove_file(self.tmp_dir.join(format!("preview-{old}.ass")));
                    }
                    self.written_rev = Some(app.doc.rev);
                }
            }
            ass = Some(path);
        }
        if self.video.is_none() && ass.is_none() {
            return;
        }
        self.requested = Some(key);
        let _ = self.tx.send(Req { key, video: self.video.as_ref().map(|v| v.0.clone()), time_ms: self.time_ms, size, ass });
    }

    /// Video tab.
    pub fn video_panel(&mut self, ui: &mut egui::Ui, app: &mut App) {
        self.poll(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.show_subs, tr("Subtitles"));
            ui.checkbox(&mut self.follow_selection, tr("Follow selection"));
        });
        let aspect = match &self.video {
            Some((_, i)) => i.width as f32 / i.height as f32,
            None => {
                let (w, h) = app.doc.script.resolution();
                w as f32 / h.max(1) as f32
            }
        };
        let avail = ui.available_width().min(960.0);
        let size = ui.available_size_before_wrap();
        let _ = size;
        let (w, h) = (avail, avail / aspect);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        let px = ui.ctx().pixels_per_point();
        let req = (((w * px) as u32).max(64) & !1, ((h * px) as u32).max(36) & !1);
        self.request(app, req);
        if let Some(t) = &self.tex {
            ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        if self.video.is_none() && !self.show_subs {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "No video loaded\n(File → Open video…)", egui::FontId::proportional(14.0), Color32::GRAY);
        }
        if let Some(e) = &self.error {
            ui.colored_label(Color32::from_rgb(230, 110, 110), e);
        }
        let dur = self.video.as_ref().map(|v| v.1.duration_ms).filter(|d| *d > 0).unwrap_or_else(|| app.doc.script.events.iter().map(|e| e.end).max().unwrap_or(60_000) + 5000);
        let mut t = self.time_ms as f64;
        ui.spacing_mut().slider_width = (w - 20.0).max(50.0);
        if ui.add(egui::Slider::new(&mut t, 0.0..=dur as f64).show_value(false)).changed() {
            self.seek(t as i64);
        }
        ui.horizontal_wrapped(|ui| {
            ui.monospace(nsub_core::time::format_ass(self.time_ms));
            if let Some(f) = self.fps() {
                ui.monospace(format!("frame {}", nsub_core::time::FrameRate::new(f).frame_from_ms(self.time_ms)));
            }
            if ui.small_button(tr("−1s")).clicked() { self.seek(self.time_ms - 1000) }
            if ui.small_button(tr("◀ frame")).clicked() { self.step_frames(-1) }
            if ui.small_button(tr("frame ▶")).clicked() { self.step_frames(1) }
            if ui.small_button(tr("+1s")).clicked() { self.seek(self.time_ms + 1000) }
        });
        if let Some(a) = app.active.and_then(|a| app.doc.script.events.get(a)) {
            let (s, e) = (a.start, a.end);
            ui.horizontal(|ui| {
                if ui.small_button(tr("→ start")).clicked() { self.seek(s) }
                if ui.small_button(tr("→ end")).clicked() { self.seek(e) }
                if ui.small_button(tr("set start = here")).clicked() {
                    let a = app.active.unwrap();
                    app.doc.checkpoint();
                    app.doc.script.events[a].start = self.time_ms;
                }
                if ui.small_button(tr("set end = here")).clicked() {
                    let a = app.active.unwrap();
                    app.doc.checkpoint();
                    app.doc.script.events[a].end = self.time_ms;
                }
            });
        }
    }

    /// Audio waveform with draggable start/end markers for the active line.
    pub fn waveform(&mut self, ui: &mut egui::Ui, app: &mut App) {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ui.available_height().max(60.0)), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(16, 18, 24));
        let span = self.wave_span_ms.max(500);
        let start = self.wave_center_ms - span / 2;
        let end = start + span;
        let x_of = |t: i64| rect.left() + rect.width() * ((t - start) as f32 / span as f32);
        let t_of = |x: f32| start + (((x - rect.left()) / rect.width()) * span as f32) as i64;

        // other lines as faint bars
        for (i, e) in app.doc.script.events.iter().enumerate() {
            if e.end < start || e.start > end || e.comment {
                continue;
            }
            let r = Rect::from_min_max(Pos2::new(x_of(e.start).max(rect.left()), rect.top()), Pos2::new(x_of(e.end).min(rect.right()), rect.bottom()));
            let active = app.sel.contains(&i);
            painter.rect_filled(r, 0.0, if active { Color32::from_rgba_unmultiplied(70, 140, 220, 60) } else { Color32::from_rgba_unmultiplied(120, 120, 120, 25) });
        }

        let cols = rect.width() as usize;
        let mid = rect.center().y;
        let amp = rect.height() * 0.45;
        {
            let pk = self.peaks.lock().unwrap();
            if pk.per_second > 0 {
                for (c, (lo, hi)) in pk.window(start.max(0), end, cols).into_iter().enumerate() {
                    let x = rect.left() + c as f32 + 0.5;
                    painter.line_segment([Pos2::new(x, mid - hi * amp), Pos2::new(x, mid - lo * amp)], Stroke::new(1.0_f32, Color32::from_rgb(110, 190, 240)));
                }
            }
        }
        if self.peaks_loading.load(Ordering::Relaxed) {
            painter.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, "decoding audio…", egui::FontId::proportional(11.0), Color32::GRAY);
        } else if self.audio.is_none() {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "No audio (File → Open audio/video…)", egui::FontId::proportional(13.0), Color32::GRAY);
        }
        // playhead
        painter.line_segment([Pos2::new(x_of(self.time_ms), rect.top()), Pos2::new(x_of(self.time_ms), rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(240, 200, 80)));

        // active line markers
        let active = app.active.filter(|&a| a < app.doc.script.events.len());
        if let Some(a) = active {
            let (s, e) = (app.doc.script.events[a].start, app.doc.script.events[a].end);
            for (t, col) in [(s, Color32::from_rgb(90, 220, 120)), (e, Color32::from_rgb(230, 90, 90))] {
                painter.line_segment([Pos2::new(x_of(t), rect.top()), Pos2::new(x_of(t), rect.bottom())], Stroke::new(2.0_f32, col));
            }
            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (ds, de) = ((p.x - x_of(s)).abs(), (p.x - x_of(e)).abs());
                    self.drag = if ds < 8.0 && ds <= de { Some(Marker::Start) } else if de < 8.0 { Some(Marker::End) } else { None };
                    if self.drag.is_some() {
                        app.doc.checkpoint();
                    }
                }
            }
            if let (Some(m), true) = (self.drag, resp.dragged()) {
                if let Some(p) = resp.interact_pointer_pos() {
                    let t = t_of(p.x).max(0);
                    let ev = &mut app.doc.script.events[a];
                    match m {
                        Marker::Start => ev.start = t.min(ev.end),
                        Marker::End => ev.end = t.max(ev.start),
                    }
                    app.doc.rev += 1;
                    app.doc.dirty = true;
                }
            }
            if resp.drag_stopped() {
                if self.drag.take().is_some() {
                    app.refresh_qc();
                }
            }
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.seek(t_of(p.x));
            }
        }
        if resp.hovered() {
            let (scroll, zoom, ctrl) = ui.input(|i| (i.raw_scroll_delta, i.zoom_delta(), i.modifiers.command));
            if zoom != 1.0 {
                self.wave_span_ms = ((self.wave_span_ms as f32 / zoom) as i64).clamp(1000, 120_000);
            } else if ctrl && scroll.y != 0.0 {
                self.wave_span_ms = ((self.wave_span_ms as f32 * (1.0 - scroll.y * 0.002)) as i64).clamp(1000, 120_000);
            } else if scroll.y != 0.0 || scroll.x != 0.0 {
                self.wave_center_ms += ((scroll.y + scroll.x) * self.wave_span_ms as f32 * -0.002) as i64;
            }
        }
    }

    /// Call every frame after the active line may have changed.
    pub fn on_active_changed(&mut self, app: &App) {
        if let Some(e) = app.active.and_then(|a| app.doc.script.events.get(a)) {
            self.wave_center_ms = (e.start + e.end) / 2;
            if self.follow_selection {
                self.seek(e.start + 1);
            }
        }
    }
}
