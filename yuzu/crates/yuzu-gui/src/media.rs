//! Video / audio state: background ffmpeg workers and the waveform view.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2};
use std::io::Read;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use yuzu_media::{MediaInfo, Frame};

pub struct RenderReq {
    pub id: u64,
    pub video: PathBuf,
    pub time_ms: i64,
    pub w: i32,
    pub h: i32,
    pub ass: Option<String>,
}

pub enum MediaMsg {
    Info(Result<MediaInfo, String>),
    Keyframes(Vec<i64>),
    Peaks(Vec<(i8, i8)>),
    PeaksDone,
    Frame { id: u64, frame: Result<Frame, String>, time_ms: i64 },
    PlayFrame { frame: Frame, time_ms: i64 },
    PlayEnded,
}

pub struct MediaState {
    pub video: Option<PathBuf>,
    pub audio: Option<PathBuf>,
    pub info: Option<MediaInfo>,
    pub keyframes: Vec<i64>,
    pub peaks: Vec<(i8, i8)>,
    pub peaks_loading: bool,
    pub bin_ms: i64,
    pub pos_ms: i64,
    pub texture: Option<TextureHandle>,
    pub error: Option<String>,
    pub playing: bool,
    pub view_start: f64,
    pub ms_per_px: f64,
    pub snap_keyframes: bool,
    pub follow: bool,
    pub tx: Sender<MediaMsg>,
    pub rx: Receiver<MediaMsg>,
    req_tx: Sender<RenderReq>,
    next_id: u64,
    pub pending: bool,
    peaks_cancel: Arc<AtomicBool>,
    play_gen: Arc<AtomicU64>,
    play_child: Option<Child>,
    audio_child: Option<Child>,
    pub last_render_key: u64,
    pub video_size: (i32, i32),
    ctx: egui::Context,
}

impl MediaState {
    pub fn new(ctx: egui::Context) -> MediaState {
        let (tx, rx) = channel();
        let (req_tx, req_rx) = channel::<RenderReq>();
        {
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                while let Ok(mut r) = req_rx.recv() {
                    while let Ok(n) = req_rx.try_recv() { r = n; } // coalesce
                    let mut tmp = None;
                    if let Some(a) = &r.ass {
                        let p = std::env::temp_dir().join(format!("yuzu-prev-{}-{}.ass", std::process::id(), r.id));
                        if std::fs::write(&p, a).is_ok() { tmp = Some(p); }
                    }
                    let f = yuzu_media::render_frame(&r.video, r.time_ms, r.w, r.h, tmp.as_deref());
                    if let Some(p) = tmp { let _ = std::fs::remove_file(p); }
                    let _ = tx.send(MediaMsg::Frame { id: r.id, frame: f, time_ms: r.time_ms });
                    ctx.request_repaint();
                }
            });
        }
        MediaState {
            video: None, audio: None, info: None, keyframes: vec![], peaks: vec![], peaks_loading: false, bin_ms: 10,
            pos_ms: 0, texture: None, error: None, playing: false, view_start: 0.0, ms_per_px: 20.0, snap_keyframes: true, follow: true,
            tx, rx, req_tx, next_id: 1, pending: false, peaks_cancel: Arc::new(AtomicBool::new(false)), play_gen: Arc::new(AtomicU64::new(0)),
            play_child: None, audio_child: None, last_render_key: 0, video_size: (0, 0), ctx,
        }
    }

    pub fn open(&mut self, path: PathBuf) {
        self.close();
        self.video = Some(path.clone());
        self.audio = Some(path.clone());
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let p2 = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(MediaMsg::Info(yuzu_media::probe(&p2)));
            ctx.request_repaint();
            if let Ok(k) = yuzu_media::keyframes(&p2) { let _ = tx.send(MediaMsg::Keyframes(k)); ctx.request_repaint(); }
        });
        self.load_peaks(path);
    }

    pub fn load_peaks(&mut self, path: PathBuf) {
        self.peaks_cancel.store(true, Ordering::Relaxed);
        self.peaks_cancel = Arc::new(AtomicBool::new(false));
        self.peaks.clear();
        self.peaks_loading = true;
        let (tx, ctx, cancel, bin) = (self.tx.clone(), self.ctx.clone(), self.peaks_cancel.clone(), self.bin_ms);
        std::thread::spawn(move || {
            let mut last = std::time::Instant::now();
            let mut acc: Vec<(i8, i8)> = Vec::new();
            let _ = yuzu_media::peaks(&path, bin, |b| {
                acc.extend_from_slice(b);
                if last.elapsed().as_millis() > 250 {
                    let _ = tx.send(MediaMsg::Peaks(std::mem::take(&mut acc)));
                    ctx.request_repaint();
                    last = std::time::Instant::now();
                }
                !cancel.load(Ordering::Relaxed)
            });
            if !acc.is_empty() { let _ = tx.send(MediaMsg::Peaks(acc)); }
            let _ = tx.send(MediaMsg::PeaksDone);
            ctx.request_repaint();
        });
    }

    pub fn close(&mut self) {
        self.stop();
        self.peaks_cancel.store(true, Ordering::Relaxed);
        self.video = None; self.audio = None; self.info = None; self.keyframes.clear(); self.peaks.clear(); self.texture = None; self.error = None;
        self.pos_ms = 0; self.view_start = 0.0; self.peaks_loading = false;
    }

    pub fn duration(&self) -> i64 {
        self.info.as_ref().map(|i| i.duration_ms).unwrap_or(0).max(self.peaks.len() as i64 * self.bin_ms)
    }

    /// Ask the render worker for a frame at `pos_ms` with current subtitles.
    pub fn request_frame(&mut self, ass: Option<String>, w: i32) {
        let (Some(v), Some(info)) = (self.video.clone(), self.info.clone()) else { return };
        if !info.has_video || info.width == 0 { return; }
        let w = (w.clamp(160, 1280) / 2) * 2;
        let h = ((w as f64 * info.height as f64 / info.width as f64) as i32 / 2) * 2;
        self.video_size = (info.width, info.height);
        let id = self.next_id;
        self.next_id += 1;
        self.pending = true;
        let _ = self.req_tx.send(RenderReq { id, video: v, time_ms: self.pos_ms, w, h, ass });
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<MediaMsg> {
        let mut passthrough = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                MediaMsg::Info(r) => match r {
                    Ok(i) => { self.error = None; self.info = Some(i.clone()); passthrough.push(MediaMsg::Info(Ok(i))); }
                    Err(e) => self.error = Some(e),
                },
                MediaMsg::Keyframes(k) => self.keyframes = k,
                MediaMsg::Peaks(p) => self.peaks.extend(p),
                MediaMsg::PeaksDone => self.peaks_loading = false,
                MediaMsg::Frame { frame, .. } => {
                    self.pending = false;
                    match frame {
                        Ok(f) => self.set_texture(ctx, f),
                        Err(e) => self.error = Some(e),
                    }
                }
                MediaMsg::PlayFrame { frame, time_ms } => {
                    if self.playing { self.pos_ms = time_ms; self.set_texture(ctx, frame); }
                }
                MediaMsg::PlayEnded => { self.playing = false; self.stop_audio(); }
                other => passthrough.push(other),
            }
        }
        passthrough
    }

    fn set_texture(&mut self, ctx: &egui::Context, f: Frame) {
        let img = egui::ColorImage::from_rgba_unmultiplied([f.width, f.height], &f.rgba);
        match &mut self.texture {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("video", img, egui::TextureOptions::LINEAR)),
        }
    }

    pub fn stop_audio(&mut self) {
        if let Some(mut c) = self.audio_child.take() { let _ = c.kill(); let _ = c.wait(); }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.play_gen.fetch_add(1, Ordering::SeqCst);
        if let Some(mut c) = self.play_child.take() { let _ = c.kill(); let _ = c.wait(); }
        self.stop_audio();
    }

    /// Play video from pos (with audio) until `end_ms` if given.
    pub fn play(&mut self, ass: Option<String>, end_ms: Option<i64>, out_w: i32) {
        self.stop();
        let (Some(v), Some(info)) = (self.video.clone(), self.info.clone()) else {
            // audio-only
            if let Some(a) = self.audio.clone() {
                self.playing = true;
                self.audio_child = yuzu_media::play_audio(&a, self.pos_ms, end_ms).ok();
            }
            return;
        };
        let fps = info.fps_num as f64 / info.fps_den as f64;
        let w = (out_w.clamp(160, 960) / 2) * 2;
        let h = ((w as f64 * info.height.max(1) as f64 / info.width.max(1) as f64) as i32 / 2) * 2;
        let mut tmp = None;
        if let Some(a) = &ass {
            let p = std::env::temp_dir().join(format!("yuzu-play-{}.ass", std::process::id()));
            if std::fs::write(&p, a).is_ok() { tmp = Some(p); }
        }
        let start = self.pos_ms;
        let mut child = match yuzu_media::spawn_playback(&v, start, w, h, fps, tmp.as_deref()) {
            Ok(c) => c,
            Err(e) => { self.error = Some(e); return; }
        };
        let Some(mut so) = child.stdout.take() else { return };
        self.play_child = Some(child);
        if info.has_audio { self.audio_child = yuzu_media::play_audio(&v, start, end_ms).ok(); }
        self.playing = true;
        let gen = self.play_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, ctx, gens) = (self.tx.clone(), self.ctx.clone(), self.play_gen.clone());
        std::thread::spawn(move || {
            let need = (w * h * 4) as usize;
            let t0 = std::time::Instant::now();
            let mut n = 0u64;
            let mut buf = vec![0u8; need];
            loop {
                if gens.load(Ordering::SeqCst) != gen { break; }
                if so.read_exact(&mut buf).is_err() { break; }
                let t = start + (n as f64 / fps * 1000.0) as i64;
                let due = std::time::Duration::from_millis((t - start) as u64);
                if let Some(e) = end_ms { if t >= e { break; } }
                let el = t0.elapsed();
                if el < due { std::thread::sleep(due - el); }
                else if el > due + std::time::Duration::from_millis(250) { n += 1; continue; } // behind: drop frame
                let _ = tx.send(MediaMsg::PlayFrame { frame: Frame { width: w as usize, height: h as usize, rgba: buf.clone() }, time_ms: t });
                ctx.request_repaint();
                n += 1;
            }
            if gens.load(Ordering::SeqCst) == gen { let _ = tx.send(MediaMsg::PlayEnded); ctx.request_repaint(); }
        });
    }
}

/// Which handle of the active line is being dragged.
#[derive(Clone, Copy, PartialEq)]
pub enum Drag { Start, End, Whole }

pub struct WaveEvents {
    pub seek: Option<i64>,
    pub set_start: Option<i64>,
    pub set_end: Option<i64>,
    pub drag_begin: bool,
    pub drag_end: bool,
}

pub struct LineSpan<'a> { pub start: i64, pub end: i64, pub label: &'a str, pub active: bool, pub selected: bool }

pub fn waveform(ui: &mut egui::Ui, m: &mut MediaState, spans: &[LineSpan], height: f32, drag: &mut Option<Drag>) -> WaveEvents {
    let mut ev = WaveEvents { seek: None, set_start: None, set_end: None, drag_begin: false, drag_end: false };
    let (resp, p) = ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::click_and_drag());
    let rect = resp.rect;
    p.rect_filled(rect, 0.0, Color32::from_rgb(14, 16, 22));
    let w = rect.width() as f64;
    let ms_to_x = |t: f64, m: &MediaState| rect.left() + ((t - m.view_start) / m.ms_per_px) as f32;
    let x_to_ms = |x: f32, m: &MediaState| m.view_start + (x - rect.left()) as f64 * m.ms_per_px;

    // zoom / pan
    if resp.hovered() {
        let (scroll, zoom, ctrl) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command));
        if ctrl && scroll.y != 0.0 {
            let anchor = ui.input(|i| i.pointer.hover_pos()).map(|pp| x_to_ms(pp.x, m)).unwrap_or(m.view_start);
            let f = (1.0 - scroll.y as f64 * 0.003).clamp(0.5, 2.0);
            let new = (m.ms_per_px * f).clamp(0.5, 400.0);
            if let Some(pp) = ui.input(|i| i.pointer.hover_pos()) { m.view_start = anchor - (pp.x - rect.left()) as f64 * new; }
            m.ms_per_px = new;
        } else if zoom != 1.0 {
            m.ms_per_px = (m.ms_per_px / zoom as f64).clamp(0.5, 400.0);
        } else if scroll != Vec2::ZERO {
            m.view_start -= (scroll.x + scroll.y) as f64 * m.ms_per_px * 1.5;
        }
    }
    m.view_start = m.view_start.max(-w * m.ms_per_px * 0.1);

    // line spans
    for s in spans {
        let (x0, x1) = (ms_to_x(s.start as f64, m), ms_to_x(s.end as f64, m));
        if x1 < rect.left() || x0 > rect.right() { continue; }
        let col = if s.active { Color32::from_rgba_unmultiplied(80, 160, 255, 60) } else if s.selected { Color32::from_rgba_unmultiplied(160, 160, 255, 36) } else { Color32::from_rgba_unmultiplied(255, 255, 255, 14) };
        let r = Rect::from_min_max(Pos2::new(x0.max(rect.left()), rect.top()), Pos2::new(x1.min(rect.right()), rect.bottom()));
        p.rect_filled(r, 0.0, col);
        let edge = if s.active { Color32::from_rgb(90, 180, 255) } else { Color32::from_gray(90) };
        p.line_segment([Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())], Stroke::new(if s.active { 2.0_f32 } else { 1.0 }, edge));
        p.line_segment([Pos2::new(x1, rect.top()), Pos2::new(x1, rect.bottom())], Stroke::new(if s.active { 2.0_f32 } else { 1.0 }, edge));
        if x1 - x0 > 40.0 {
            p.with_clip_rect(r).text(Pos2::new(x0.max(rect.left()) + 3.0, rect.top() + 2.0), egui::Align2::LEFT_TOP, s.label, egui::FontId::proportional(11.0), Color32::from_gray(190));
        }
    }

    // peaks
    let mid = rect.center().y;
    let amp = rect.height() / 2.0 - 14.0;
    let bins = &m.peaks;
    let wave = Color32::from_rgb(110, 190, 150);
    let n = rect.width() as i32;
    for px in 0..n {
        let t0 = m.view_start + px as f64 * m.ms_per_px;
        let t1 = t0 + m.ms_per_px;
        if t1 < 0.0 { continue; }
        let b0 = (t0 / m.bin_ms as f64).floor().max(0.0) as usize;
        let b1 = ((t1 / m.bin_ms as f64).ceil() as usize).max(b0 + 1);
        if b0 >= bins.len() { break; }
        let (mut mn, mut mx) = (127i16, -128i16);
        for b in &bins[b0..b1.min(bins.len())] { mn = mn.min(b.0 as i16); mx = mx.max(b.1 as i16); }
        let x = rect.left() + px as f32 + 0.5;
        p.line_segment([Pos2::new(x, mid - mx as f32 / 128.0 * amp), Pos2::new(x, mid - mn as f32 / 128.0 * amp - 1.0)], Stroke::new(1.0_f32, wave));
    }
    p.line_segment([Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)], Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 25)));

    // keyframes
    for &k in &m.keyframes {
        let x = ms_to_x(k as f64, m);
        if x >= rect.left() && x <= rect.right() { p.line_segment([Pos2::new(x, rect.bottom() - 10.0), Pos2::new(x, rect.bottom())], Stroke::new(2.0_f32, Color32::from_rgb(230, 200, 60))); }
    }
    // time ruler
    let step_ms = [100.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 30000.0, 60000.0].into_iter().find(|s| s / m.ms_per_px > 70.0).unwrap_or(120000.0);
    let first = (m.view_start / step_ms).floor() as i64;
    for k in first..first + (w * m.ms_per_px / step_ms) as i64 + 2 {
        let t = k as f64 * step_ms;
        let x = ms_to_x(t, m);
        if t < 0.0 || x < rect.left() || x > rect.right() { continue; }
        p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.top() + 6.0)], Stroke::new(1.0_f32, Color32::from_gray(120)));
        let s = (t / 1000.0) as i64;
        p.text(Pos2::new(x + 2.0, rect.top() + 6.0), egui::Align2::LEFT_TOP, format!("{}:{:02}{}", s / 60, s % 60, if step_ms < 1000.0 { format!(".{:01}", ((t as i64 % 1000) / 100)) } else { String::new() }), egui::FontId::proportional(10.0), Color32::from_gray(140));
    }
    // playhead
    let px = ms_to_x(m.pos_ms as f64, m);
    if px >= rect.left() && px <= rect.right() { p.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(1.5_f32, Color32::from_rgb(255, 90, 90))); }

    // interaction: handles of the active line
    let active = spans.iter().find(|s| s.active);
    let hover = resp.hover_pos();
    let near = |x: f32, hx: f32| (x - hx).abs() < 7.0;
    if resp.drag_started() {
        if let (Some(a), Some(hp)) = (active, resp.interact_pointer_pos()) {
            let (x0, x1) = (ms_to_x(a.start as f64, m), ms_to_x(a.end as f64, m));
            if near(hp.x, x0) { *drag = Some(Drag::Start); ev.drag_begin = true; }
            else if near(hp.x, x1) { *drag = Some(Drag::End); ev.drag_begin = true; }
        }
    }
    if let (Some(d), Some(pp)) = (*drag, resp.interact_pointer_pos()) {
        let mut t = x_to_ms(pp.x, m).round() as i64;
        if m.snap_keyframes || ui.input(|i| i.modifiers.shift) {
            if let Some(&k) = m.keyframes.iter().min_by_key(|&&k| (k - t).abs()) { if ((k - t).abs() as f64) < 8.0 * m.ms_per_px { t = k; } }
        }
        match d { Drag::Start => ev.set_start = Some(t.max(0)), Drag::End => ev.set_end = Some(t.max(0)), Drag::Whole => {} }
    }
    if resp.drag_stopped() { if drag.is_some() { ev.drag_end = true; } *drag = None; }
    if let Some(hp) = hover {
        if let Some(a) = active {
            let (x0, x1) = (ms_to_x(a.start as f64, m), ms_to_x(a.end as f64, m));
            if near(hp.x, x0) || near(hp.x, x1) { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }
        }
    }
    if resp.clicked() && drag.is_none() {
        if let Some(pp) = resp.interact_pointer_pos() { ev.seek = Some(x_to_ms(pp.x, m).max(0.0) as i64); }
    }
    if resp.secondary_clicked() {
        if let Some(pp) = resp.interact_pointer_pos() { ev.set_end = Some(x_to_ms(pp.x, m).max(0.0) as i64); }
    }
    if ui.input(|i| i.modifiers.shift) && resp.clicked() {
        if let Some(pp) = resp.interact_pointer_pos() { ev.seek = None; ev.set_start = Some(x_to_ms(pp.x, m).max(0.0) as i64); }
    }
    ev
}
