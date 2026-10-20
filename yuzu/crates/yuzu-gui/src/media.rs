//! Video / audio state and the timing view.
//!
//! Playback goes through libmpv when it is installed (frame-exact seeking, audio in sync, the
//! subtitles being edited drawn live by libass); otherwise the older ffmpeg command-line path
//! renders still frames and plays through ffplay. Audio is decoded once to 16 kHz PCM for the
//! waveform and the spectrum.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2};
use std::io::Read;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use yuzu_media::audio::{self, Spectrogram};
use yuzu_media::player::Player;
use yuzu_media::{Frame, MediaInfo};

pub struct RenderReq {
    pub video: PathBuf,
    pub time_ms: i64,
    pub w: i32,
    pub h: i32,
    pub ass: Option<String>,
}

pub enum MediaMsg {
    Info(Result<MediaInfo, String>),
    Keyframes(Vec<i64>),
    Pcm(Vec<i16>),
    PcmDone(Option<String>),
    Spectrum(Spectrogram),
    Frame(Result<Frame, String>),
    PlayFrame { frame: Frame, time_ms: i64 },
    PlayEnded,
}

pub struct MediaState {
    pub video: Option<PathBuf>,
    pub audio: Option<PathBuf>,
    pub info: Option<MediaInfo>,
    /// Keyframe presentation times (ms).
    pub keyframes: Vec<i64>,
    pub pcm: Vec<i16>,
    pub peaks: Vec<(i8, i8)>,
    pub spectrum: Option<Spectrogram>,
    pub show_spectrum: bool,
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
    pub volume: f64,
    pub player: Option<Player>,
    pub player_error: Option<String>,
    pub tx: Sender<MediaMsg>,
    pub rx: Receiver<MediaMsg>,
    req_tx: Sender<RenderReq>,
    pub pending: bool,
    decode_cancel: Arc<AtomicBool>,
    play_gen: Arc<AtomicU64>,
    play_child: Option<Child>,
    audio_child: Option<Child>,
    pub last_render_key: u64,
    pub video_size: (i32, i32),
    sub_gen: u64,
    sub_changed: Option<Instant>,
    /// A seek asked for before libmpv finished opening the file.
    pending_seek: Option<i64>,
    spec_tex: Option<TextureHandle>,
    spec_key: (i64, i64, u32, u32, usize),
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
                let mut n = 0u64;
                while let Ok(mut r) = req_rx.recv() {
                    while let Ok(next) = req_rx.try_recv() { r = next; } // coalesce
                    n += 1;
                    let mut tmp = None;
                    if let Some(a) = &r.ass {
                        let p = std::env::temp_dir().join(format!("yuzu-prev-{}-{}.ass", std::process::id(), n));
                        if std::fs::write(&p, a).is_ok() { tmp = Some(p); }
                    }
                    let f = yuzu_media::render_frame(&r.video, r.time_ms, r.w, r.h, tmp.as_deref());
                    if let Some(p) = tmp { let _ = std::fs::remove_file(p); }
                    let _ = tx.send(MediaMsg::Frame(f));
                    ctx.request_repaint();
                }
            });
        }
        MediaState {
            video: None, audio: None, info: None, keyframes: vec![], pcm: vec![], peaks: vec![], spectrum: None, show_spectrum: false,
            peaks_loading: false, bin_ms: 10, pos_ms: 0, texture: None, error: None, playing: false, view_start: 0.0, ms_per_px: 20.0,
            snap_keyframes: true, follow: true, volume: 100.0, player: None, player_error: None,
            tx, rx, req_tx, pending: false, decode_cancel: Arc::new(AtomicBool::new(false)), play_gen: Arc::new(AtomicU64::new(0)),
            play_child: None, audio_child: None, last_render_key: 0, video_size: (0, 0), sub_gen: u64::MAX, sub_changed: None,
            pending_seek: None, spec_tex: None, spec_key: (0, 0, 0, 0, 0), ctx,
        }
    }

    /// Lazily start libmpv; None (and a remembered reason) when it is not installed.
    fn ensure_player(&mut self) -> Option<&Player> {
        if self.player.is_none() && self.player_error.is_none() && std::env::var_os("YUZU_NO_MPV").is_none() {
            let ctx = self.ctx.clone();
            match Player::new(&[], Box::new(move || ctx.request_repaint())) {
                Ok(p) => { p.set_volume(self.volume); self.player = Some(p); }
                Err(e) => self.player_error = Some(e),
            }
        }
        self.player.as_ref()
    }

    /// True when playback and frames come from libmpv.
    pub fn uses_mpv(&self) -> bool { self.player.is_some() }

    /// One line for the About box: which engine plays video and why.
    pub fn engine_description(&self) -> String {
        match (&self.player, &self.player_error) {
            (Some(p), _) => format!("libmpv {} (frame-exact playback, live subtitles)", p.mpv().get_string("mpv-version").unwrap_or_default().trim_start_matches("mpv ")),
            (None, Some(e)) => format!("ffmpeg fallback — libmpv unavailable: {}", e),
            (None, None) if yuzu_media::mpv::available() => "libmpv (starts with the first video)".into(),
            (None, None) => format!("ffmpeg fallback — {}", yuzu_media::mpv::load_error().unwrap_or_default()),
        }
    }

    pub fn has_video(&self) -> bool {
        match &self.player {
            Some(p) => p.state().has_video() || self.info.as_ref().map(|i| i.has_video).unwrap_or(false),
            None => self.info.as_ref().map(|i| i.has_video).unwrap_or(false),
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
        if let Some(p) = self.ensure_player() { p.open(&path); }
        self.load_audio(path);
    }

    /// Use a separate audio file for the waveform (and for playback when libmpv is used).
    pub fn open_audio(&mut self, path: PathBuf) {
        self.audio = Some(path.clone());
        let video = self.video.clone();
        if let Some(p) = self.ensure_player() {
            match video {
                Some(_) => { let _ = p.mpv().command(&["audio-add", &path.to_string_lossy(), "select"]); }
                None => p.open(&path),
            }
        }
        self.load_audio(path);
    }

    pub fn load_audio(&mut self, path: PathBuf) {
        self.decode_cancel.store(true, Ordering::Relaxed);
        self.decode_cancel = Arc::new(AtomicBool::new(false));
        self.pcm.clear();
        self.peaks.clear();
        self.spectrum = None;
        self.spec_key = (0, 0, 0, 0, 0);
        self.peaks_loading = true;
        let (tx, ctx, cancel) = (self.tx.clone(), self.ctx.clone(), self.decode_cancel.clone());
        std::thread::spawn(move || {
            let mut all: Vec<i16> = Vec::new();
            let mut pending: Vec<i16> = Vec::new();
            let mut last = Instant::now();
            let r = audio::decode(&path, |s| {
                all.extend_from_slice(s);
                pending.extend_from_slice(s);
                if last.elapsed() > Duration::from_millis(200) {
                    let _ = tx.send(MediaMsg::Pcm(std::mem::take(&mut pending)));
                    ctx.request_repaint();
                    last = Instant::now();
                }
                !cancel.load(Ordering::Relaxed)
            });
            if cancel.load(Ordering::Relaxed) { return; }
            if !pending.is_empty() { let _ = tx.send(MediaMsg::Pcm(pending)); }
            let _ = tx.send(MediaMsg::PcmDone(r.err()));
            ctx.request_repaint();
            if !all.is_empty() {
                let sp = audio::spectrogram(&all, audio::RATE, 10, 128);
                if !cancel.load(Ordering::Relaxed) { let _ = tx.send(MediaMsg::Spectrum(sp)); ctx.request_repaint(); }
            }
        });
    }

    pub fn close(&mut self) {
        self.stop();
        self.pending_seek = None;
        if let Some(p) = &self.player { p.close(); }
        self.decode_cancel.store(true, Ordering::Relaxed);
        self.video = None; self.audio = None; self.info = None; self.keyframes.clear(); self.pcm.clear(); self.peaks.clear();
        self.spectrum = None; self.texture = None; self.error = None;
        self.pos_ms = 0; self.view_start = 0.0; self.peaks_loading = false; self.sub_gen = u64::MAX;
    }

    pub fn duration(&self) -> i64 {
        let from_player = self.player.as_ref().map(|p| p.state().duration_ms as i64).unwrap_or(0);
        self.info.as_ref().map(|i| i.duration_ms).unwrap_or(0).max(from_player).max(self.pcm.len() as i64 * 1000 / audio::RATE as i64)
    }

    // ------------------------------------------------------------ transport

    /// Move the playhead (frame-exact with libmpv).
    pub fn seek(&mut self, ms: i64) {
        let ms = ms.max(0);
        self.stop_ffmpeg();
        self.pos_ms = ms;
        if let Some(p) = &self.player {
            if p.state().loaded { p.seek(ms as f64, true); } else { self.pending_seek = Some(ms); }
        }
    }

    pub fn toggle_play(&mut self, ass: impl FnOnce() -> Option<String>) {
        if let Some(p) = &self.player { p.toggle_pause(); return; }
        if self.playing { self.stop(); return; }
        self.ffmpeg_play(ass(), None, 800);
    }

    /// Play [start, end) and stop there (the "play line" family).
    pub fn play_range(&mut self, start: i64, end: i64, ass: impl FnOnce() -> Option<String>) {
        let start = start.max(0);
        if let Some(p) = &self.player {
            self.pos_ms = start;
            p.play_range(start as f64, end.max(start + 1) as f64);
            return;
        }
        self.stop();
        self.pos_ms = start;
        self.ffmpeg_play(ass(), Some(end), 800);
    }

    pub fn stop(&mut self) {
        if let Some(p) = &self.player { p.set_pause(true); }
        self.stop_ffmpeg();
    }

    /// Step whole frames (frame-step / frame-back-step with libmpv, frame arithmetic otherwise).
    pub fn step(&mut self, n: i64, fps: asscore::time::Fps) {
        if let Some(p) = &self.player {
            if n == 1 || n == -1 { p.frame_step(n > 0); return; }
            let f = fps.frame_at(self.pos_ms) + n;
            let t = fps.ms_from_frame(f.max(0));
            self.pos_ms = t;
            p.seek(t as f64, true);
            return;
        }
        self.stop_ffmpeg();
        let f = fps.frame_at(self.pos_ms) + n;
        self.pos_ms = fps.ms_from_frame(f.max(0));
    }

    pub fn set_volume(&mut self, v: f64) {
        self.volume = v;
        if let Some(p) = &self.player { p.set_volume(v); }
    }

    /// Keep libmpv's subtitles in step with the document. `gen` changes on every edit; the
    /// file is rewritten at most every 120 ms while typing.
    pub fn sync_subtitles(&mut self, gen: u64, make: impl FnOnce() -> String) {
        let Some(p) = &self.player else { return };
        if gen == self.sub_gen { self.sub_changed = None; return; }
        let since = *self.sub_changed.get_or_insert_with(Instant::now);
        let first = self.sub_gen == u64::MAX;
        if first || since.elapsed() >= Duration::from_millis(120) || self.playing {
            if p.set_subtitles(&make()).is_ok() { self.sub_gen = gen; self.sub_changed = None; }
        } else {
            self.ctx.request_repaint_after(Duration::from_millis(130));
        }
    }

    /// Size (physical pixels) the video is displayed at.
    pub fn set_display_size(&self, w: f32, h: f32) {
        if let Some(p) = &self.player { p.set_output_size(w.round().max(16.0) as usize, h.round().max(16.0) as usize); }
    }

    // ------------------------------------------------------------ ffmpeg fallback

    /// Ask the ffmpeg render worker for a still frame at `pos_ms` with these subtitles.
    pub fn request_frame(&mut self, ass: Option<String>, w: i32) {
        let (Some(v), Some(info)) = (self.video.clone(), self.info.clone()) else { return };
        if !info.has_video || info.width == 0 { return; }
        let w = (w.clamp(160, 1280) / 2) * 2;
        let h = ((w as f64 * info.height as f64 / info.width as f64) as i32 / 2) * 2;
        self.video_size = (info.width, info.height);
        self.pending = true;
        let _ = self.req_tx.send(RenderReq { video: v, time_ms: self.pos_ms, w, h, ass });
    }

    fn stop_ffmpeg(&mut self) {
        if self.player.is_none() { self.playing = false; }
        self.play_gen.fetch_add(1, Ordering::SeqCst);
        if let Some(mut c) = self.play_child.take() { let _ = c.kill(); let _ = c.wait(); }
        if let Some(mut c) = self.audio_child.take() { let _ = c.kill(); let _ = c.wait(); }
    }

    fn ffmpeg_play(&mut self, ass: Option<String>, end_ms: Option<i64>, out_w: i32) {
        self.stop_ffmpeg();
        let (Some(v), Some(info)) = (self.video.clone(), self.info.clone().filter(|i| i.has_video)) else {
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
            let t0 = Instant::now();
            let mut n = 0u64;
            let mut buf = vec![0u8; need];
            loop {
                if gens.load(Ordering::SeqCst) != gen { break; }
                if so.read_exact(&mut buf).is_err() { break; }
                let t = start + (n as f64 / fps * 1000.0) as i64;
                let due = Duration::from_millis((t - start) as u64);
                if let Some(e) = end_ms { if t >= e { break; } }
                let el = t0.elapsed();
                if el < due { std::thread::sleep(due - el); } else if el > due + Duration::from_millis(250) { n += 1; continue; }
                let _ = tx.send(MediaMsg::PlayFrame { frame: Frame { width: w as usize, height: h as usize, rgba: buf.clone() }, time_ms: t });
                ctx.request_repaint();
                n += 1;
            }
            if gens.load(Ordering::SeqCst) == gen { let _ = tx.send(MediaMsg::PlayEnded); ctx.request_repaint(); }
        });
    }

    // ------------------------------------------------------------ per-frame update

    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<MediaMsg> {
        let mut passthrough = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                MediaMsg::Info(r) => match r {
                    Ok(i) => { self.error = None; self.info = Some(i.clone()); passthrough.push(MediaMsg::Info(Ok(i))); }
                    Err(e) => if self.player.is_none() { self.error = Some(e) },
                },
                MediaMsg::Keyframes(k) => self.keyframes = k,
                MediaMsg::Pcm(p) => {
                    self.pcm.extend_from_slice(&p);
                    let per = (audio::RATE as i64 * self.bin_ms / 1000) as usize;
                    let done = self.peaks.len() * per;
                    let full = self.pcm.len() / per * per;
                    if full > done { let new = audio::peaks(&self.pcm[done..full], audio::RATE, self.bin_ms); self.peaks.extend(new); }
                }
                MediaMsg::PcmDone(err) => {
                    self.peaks_loading = false;
                    if let Some(e) = err { if self.pcm.is_empty() && !self.has_video() { self.error = Some(e); } }
                }
                MediaMsg::Spectrum(s) => { self.spectrum = Some(s); self.spec_key = (0, 0, 0, 0, 0); }
                MediaMsg::Frame(frame) => {
                    self.pending = false;
                    match frame {
                        Ok(f) => self.set_texture(ctx, f.width, f.height, &f.rgba),
                        Err(e) => self.error = Some(e),
                    }
                }
                MediaMsg::PlayFrame { frame, time_ms } => {
                    if self.playing { self.pos_ms = time_ms; self.set_texture(ctx, frame.width, frame.height, &frame.rgba); }
                }
                MediaMsg::PlayEnded => { self.playing = false; if let Some(mut c) = self.audio_child.take() { let _ = c.kill(); let _ = c.wait(); } }
            }
        }
        if let Some(p) = &self.player {
            let st = p.state();
            if st.loaded { if let Some(t) = self.pending_seek.take() { p.seek(t as f64, true); } }
            let st = p.state();
            self.playing = st.loaded && !st.paused;
            if st.loaded { self.pos_ms = st.now_ms().round() as i64; }
            if let Some(e) = &st.error { self.error = Some(e.clone()); }
            if let Some(f) = p.take_frame() { self.set_texture(ctx, f.width, f.height, &f.rgba); }
            if self.playing { ctx.request_repaint(); }
        }
        passthrough
    }

    fn set_texture(&mut self, ctx: &egui::Context, w: usize, h: usize, rgba: &[u8]) {
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba);
        match &mut self.texture {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("video", img, egui::TextureOptions::LINEAR)),
        }
    }

    /// Snap targets: keyframe times converted to line times that start exactly on the keyframe.
    pub fn keyframe_line_times(&self, fps: asscore::time::Fps) -> Vec<i64> {
        self.keyframes.iter().map(|&k| fps.line_start(fps.frame_at(k))).collect()
    }
}

/// Which handle of the active line is being dragged (Kara: the boundary after syllable k).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drag { Start, End, Kara(usize) }

pub struct WaveEvents {
    /// Where the view was drawn (screen points).
    pub rect: Rect,
    pub seek: Option<i64>,
    pub set_start: Option<i64>,
    pub set_end: Option<i64>,
    pub drag_begin: bool,
    pub drag_end: bool,
    /// Karaoke mode: boundary k (after syllable k) dragged to this time.
    pub kara_move: Option<(usize, i64)>,
    /// Karaoke mode: syllable clicked (play it).
    pub kara_play: Option<usize>,
}

/// A karaoke syllable of the active line on the timing view (absolute times).
pub struct KaraSpan<'a> { pub start: i64, pub end: i64, pub label: &'a str }

pub struct LineSpan<'a> { pub start: i64, pub end: i64, pub label: &'a str, pub active: bool, pub selected: bool }

/// Spectrum colour map (dark blue → cyan → yellow → white).
fn spec_color(v: u8) -> Color32 {
    const STOPS: [(f32, [f32; 3]); 5] = [(0.0, [6.0, 8.0, 20.0]), (0.3, [30.0, 40.0, 140.0]), (0.55, [20.0, 170.0, 200.0]), (0.8, [240.0, 220.0, 60.0]), (1.0, [255.0, 255.0, 255.0])];
    let x = v as f32 / 255.0;
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if x <= b {
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            return Color32::from_rgb((ca[0] + (cb[0] - ca[0]) * t) as u8, (ca[1] + (cb[1] - ca[1]) * t) as u8, (ca[2] + (cb[2] - ca[2]) * t) as u8);
        }
    }
    Color32::WHITE
}

fn draw_spectrum(ui: &egui::Ui, m: &mut MediaState, rect: Rect, p: &egui::Painter) -> bool {
    let Some(sp) = &m.spectrum else { return false };
    let ppp = ui.ctx().pixels_per_point();
    let (w, h) = ((rect.width() * ppp).round().max(1.0) as usize, (rect.height() * ppp).round().max(1.0) as usize);
    let key = (m.view_start.round() as i64, (m.ms_per_px * 1000.0).round() as i64, w as u32, h as u32, sp.columns());
    if m.spec_tex.is_none() || key != m.spec_key {
        let mut img = egui::ColorImage::filled([w, h], Color32::from_rgb(6, 8, 20));
        let ms_per_dev_px = m.ms_per_px / ppp as f64;
        let cols = sp.columns();
        let lut: Vec<Color32> = (0..=255u8).map(spec_color).collect();
        let mut col = vec![0u8; sp.bands];
        for x in 0..w {
            let t0 = m.view_start + x as f64 * ms_per_dev_px;
            let t1 = t0 + ms_per_dev_px;
            if t1 < 0.0 { continue; }
            let c0 = (t0 / sp.hop_ms as f64).floor().max(0.0) as usize;
            let c1 = ((t1 / sp.hop_ms as f64).floor() as usize).max(c0 + 1).min(cols);
            if c0 >= cols { break; }
            col.iter_mut().for_each(|v| *v = 0);
            for c in c0..c1 { for (b, v) in sp.column(c).iter().enumerate() { col[b] = col[b].max(*v); } }
            for y in 0..h {
                let b = ((h - 1 - y) * sp.bands / h).min(sp.bands - 1);
                img.pixels[y * w + x] = lut[col[b] as usize];
            }
        }
        match &mut m.spec_tex {
            Some(t) => t.set(img, egui::TextureOptions::NEAREST),
            None => m.spec_tex = Some(ui.ctx().load_texture("spectrum", img, egui::TextureOptions::NEAREST)),
        }
        m.spec_key = key;
    }
    if let Some(t) = &m.spec_tex {
        p.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    true
}

/// The timing view. Mouse (Aegisub style): left click sets the start, right click the end,
/// dragging an edge moves it, Ctrl+click or middle click moves the playhead; wheel scrolls,
/// Ctrl+wheel zooms around the pointer. Shift inverts keyframe snapping.
pub fn waveform(ui: &mut egui::Ui, m: &mut MediaState, spans: &[LineSpan], height: f32, drag: &mut Option<Drag>, snap: &[i64], fps: Option<asscore::time::Fps>, kara: &[KaraSpan]) -> WaveEvents {
    let (resp, p) = ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::click_and_drag());
    let rect = resp.rect;
    let mut ev = WaveEvents { rect, seek: None, set_start: None, set_end: None, drag_begin: false, drag_end: false, kara_move: None, kara_play: None };
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
            let new = (m.ms_per_px * f).clamp(0.25, 400.0);
            if let Some(pp) = ui.input(|i| i.pointer.hover_pos()) { m.view_start = anchor - (pp.x - rect.left()) as f64 * new; }
            m.ms_per_px = new;
        } else if zoom != 1.0 {
            m.ms_per_px = (m.ms_per_px / zoom as f64).clamp(0.25, 400.0);
        } else if scroll != Vec2::ZERO {
            m.view_start -= (scroll.x + scroll.y) as f64 * m.ms_per_px * 1.5;
        }
    }
    // keep the playhead in view while playing
    if m.playing {
        let ph = m.pos_ms as f64;
        let span = w * m.ms_per_px;
        if ph > m.view_start + span * 0.9 || ph < m.view_start { m.view_start = ph - span * 0.1; }
    }
    m.view_start = m.view_start.max(-w * m.ms_per_px * 0.1);

    let spectrum = m.show_spectrum && draw_spectrum(ui, m, rect, &p);

    // line spans
    for s in spans {
        let (x0, x1) = (ms_to_x(s.start as f64, m), ms_to_x(s.end as f64, m));
        if x1 < rect.left() || x0 > rect.right() { continue; }
        let col = if s.active { Color32::from_rgba_unmultiplied(80, 160, 255, if spectrum { 40 } else { 60 }) } else if s.selected { Color32::from_rgba_unmultiplied(160, 160, 255, 36) } else { Color32::from_rgba_unmultiplied(255, 255, 255, if spectrum { 8 } else { 14 }) };
        let r = Rect::from_min_max(Pos2::new(x0.max(rect.left()), rect.top()), Pos2::new(x1.min(rect.right()), rect.bottom()));
        p.rect_filled(r, 0.0, col);
        let (sc, ec) = if s.active { (Color32::from_rgb(255, 120, 90), Color32::from_rgb(90, 200, 255)) } else { (Color32::from_gray(90), Color32::from_gray(90)) };
        p.line_segment([Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())], Stroke::new(if s.active { 2.0_f32 } else { 1.0 }, sc));
        p.line_segment([Pos2::new(x1, rect.top()), Pos2::new(x1, rect.bottom())], Stroke::new(if s.active { 2.0_f32 } else { 1.0 }, ec));
        if x1 - x0 > 40.0 {
            p.with_clip_rect(r).text(Pos2::new(x0.max(rect.left()) + 3.0, rect.top() + 14.0), egui::Align2::LEFT_TOP, s.label, egui::FontId::proportional(11.0), Color32::from_gray(200));
        }
    }

    // waveform
    if !spectrum {
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
    }

    // video frame ticks when zoomed in far enough to see single frames
    if let Some(f) = fps {
        if m.ms_per_px <= 2.0 {
            let (a, b) = (f.frame_at(m.view_start.max(0.0) as i64), f.frame_at((m.view_start + w * m.ms_per_px) as i64) + 1);
            for fr in a..=b {
                let x = ms_to_x(f.ms_from_frame(fr) as f64, m);
                if x >= rect.left() && x <= rect.right() { p.line_segment([Pos2::new(x, rect.bottom() - 5.0), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, Color32::from_gray(110))); }
            }
        }
    }
    // keyframes
    for &k in &m.keyframes {
        let x = ms_to_x(k as f64, m);
        if x >= rect.left() && x <= rect.right() {
            p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(230, 200, 60, 70)));
            p.line_segment([Pos2::new(x, rect.bottom() - 10.0), Pos2::new(x, rect.bottom())], Stroke::new(2.0_f32, Color32::from_rgb(230, 200, 60)));
        }
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
        p.text(Pos2::new(x + 2.0, rect.top() + 2.0), egui::Align2::LEFT_TOP, format!("{}:{:02}{}", s / 60, s % 60, if step_ms < 1000.0 { format!(".{:01}", ((t as i64 % 1000) / 100)) } else { String::new() }), egui::FontId::proportional(10.0), Color32::from_gray(150));
    }
    // karaoke syllables of the active line: boundaries and labels
    let kara_col = Color32::from_rgb(255, 160, 60);
    for (k, sy) in kara.iter().enumerate() {
        let (x0, x1) = (ms_to_x(sy.start as f64, m), ms_to_x(sy.end as f64, m));
        if x1 < rect.left() || x0 > rect.right() { continue; }
        if k > 0 {
            let mut y = rect.top() + 26.0;
            while y < rect.bottom() { p.line_segment([Pos2::new(x0, y), Pos2::new(x0, (y + 6.0).min(rect.bottom()))], Stroke::new(1.5_f32, kara_col)); y += 10.0; }
        }
        let r = Rect::from_min_max(Pos2::new(x0.max(rect.left()) + 1.0, rect.top() + 24.0), Pos2::new(x1.min(rect.right()) - 1.0, rect.top() + 44.0));
        if r.width() > 4.0 {
            let clip = p.with_clip_rect(r);
            let galley = clip.layout_no_wrap(sy.label.to_string(), egui::FontId::proportional(14.0), kara_col);
            let at = Pos2::new((x0 + x1) / 2.0 - galley.size().x / 2.0, rect.top() + 34.0 - galley.size().y / 2.0);
            clip.rect_filled(Rect::from_min_size(at - Vec2::new(3.0, 1.0), galley.size() + Vec2::new(6.0, 2.0)), 3.0, Color32::from_rgba_unmultiplied(10, 10, 14, 200));
            clip.galley(at, galley, kara_col);
        }
    }

    // playhead
    let px = ms_to_x(m.pos_ms as f64, m);
    if px >= rect.left() && px <= rect.right() { p.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(1.5_f32, Color32::from_rgb(255, 90, 90))); }

    // interaction
    let active = spans.iter().find(|s| s.active);
    let near = |x: f32, hx: f32| (x - hx).abs() < 7.0;
    let snap_t = |t: i64, m: &MediaState, invert: bool| -> i64 {
        if m.snap_keyframes != invert {
            if let Some(&k) = snap.iter().min_by_key(|&&k| (k - t).abs()) { if ((k - t).abs() as f64) < 8.0 * m.ms_per_px { return k; } }
        }
        t
    };
    let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
    // a syllable boundary (not the line's own edges) under the pointer
    let kara_hit = |x: f32, m: &MediaState| kara.iter().enumerate().skip(1).find(|(_, sy)| near(x, ms_to_x(sy.start as f64, m))).map(|(k, _)| k - 1);
    if resp.drag_started_by(egui::PointerButton::Primary) && !ctrl {
        if let (Some(a), Some(hp)) = (active, ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())) {
            let (x0, x1) = (ms_to_x(a.start as f64, m), ms_to_x(a.end as f64, m));
            // a karaoke boundary first; else grab the nearer edge; beyond the end means "drag the end"
            *drag = Some(match kara_hit(hp.x, m) {
                Some(k) if !near(hp.x, x0) && !near(hp.x, x1) => Drag::Kara(k),
                _ => if near(hp.x, x1) && !near(hp.x, x0) { Drag::End } else if near(hp.x, x0) { Drag::Start } else if hp.x > x1 { Drag::End } else { Drag::Start },
            });
            ev.drag_begin = true;
        }
    }
    // the release frame repeats the last position; sending it again would be a separate undo step
    let released = resp.drag_stopped();
    if let (Some(d), Some(pp), false) = (*drag, resp.interact_pointer_pos(), released) {
        match d {
            Drag::Start => ev.set_start = Some(snap_t(x_to_ms(pp.x, m).round() as i64, m, shift).max(0)),
            Drag::End => ev.set_end = Some(snap_t(x_to_ms(pp.x, m).round() as i64, m, shift).max(0)),
            Drag::Kara(k) => ev.kara_move = Some((k, x_to_ms(pp.x, m).round() as i64)),
        }
    }
    if resp.drag_stopped() { if drag.is_some() { ev.drag_end = true; } *drag = None; }
    if let Some(hp) = resp.hover_pos() {
        if let Some(a) = active {
            let (x0, x1) = (ms_to_x(a.start as f64, m), ms_to_x(a.end as f64, m));
            if near(hp.x, x0) || near(hp.x, x1) || kara_hit(hp.x, m).is_some() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }
        }
    }
    if let Some(pp) = resp.interact_pointer_pos() {
        let t = x_to_ms(pp.x, m).max(0.0).round() as i64;
        // karaoke mode: a click on a syllable plays it instead of moving the start
        let syl = (!ctrl).then(|| kara.iter().position(|sy| t >= sy.start && t < sy.end)).flatten();
        if resp.clicked() && drag.is_none() {
            if ctrl { ev.seek = Some(t); } else if let Some(k) = syl { ev.kara_play = Some(k); } else { ev.set_start = Some(snap_t(t, m, shift)); }
        }
        if resp.secondary_clicked() { ev.set_end = Some(snap_t(t, m, shift)); }
        if resp.middle_clicked() { ev.seek = Some(t); }
    }
    ev
}
