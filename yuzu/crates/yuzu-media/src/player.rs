//! Threaded video/audio player on top of libmpv: frame-exact seeks, playback with A/V sync,
//! precise range playback (play line), live subtitle reloads, frames delivered as RGBA.
//!
//! Two threads per player: one waits for mpv events and keeps [`PlayerState`] current, the
//! other renders frames with mpv's software renderer whenever mpv has a new one. The UI only
//! sends commands and picks up the newest frame — it never blocks on mpv.

use crate::mpv::{Event, Mpv, PropValue, Surface, SwRenderer, FORMAT_DOUBLE, FORMAT_FLAG, FORMAT_INT64};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

#[derive(Clone, Debug, Default)]
pub struct PlayerState {
    pub path: Option<PathBuf>,
    pub loaded: bool,
    /// Presentation time of the frame on screen (ms).
    pub time_ms: f64,
    pub duration_ms: f64,
    pub paused: bool,
    pub eof: bool,
    pub seeking: bool,
    /// Display size (aspect-corrected); 0 for audio-only files.
    pub width: i32,
    pub height: i32,
    pub fps: f64,
    pub error: Option<String>,
    /// When `time_ms` was last reported (mpv reports ~20×/s for audio-only files; the UI
    /// interpolates between reports while playing).
    pub time_updated: Option<std::time::Instant>,
    /// Bumped on every state change; cheap "did anything happen" check for the UI.
    pub serial: u64,
}

impl PlayerState {
    pub fn has_video(&self) -> bool { self.width > 0 && self.height > 0 }
    /// Current position, extrapolated from the last report while playing.
    pub fn now_ms(&self) -> f64 {
        match (self.paused || self.seeking, self.time_updated) {
            (false, Some(t)) => (self.time_ms + t.elapsed().as_secs_f64() * 1000.0).min(self.time_ms + 250.0),
            _ => self.time_ms,
        }
    }
}

pub struct VideoFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub time_ms: f64,
}

struct RenderCtl {
    size: (usize, usize),
    pending: bool,
    force: bool,
    quit: bool,
}

struct Shared {
    state: Mutex<PlayerState>,
    frame: Mutex<Option<VideoFrame>>,
    ctl: Mutex<RenderCtl>,
    cv: Condvar,
    quit: AtomicBool,
    notify: Box<dyn Fn() + Send + Sync>,
    sub_path: PathBuf,
    have_subs: AtomicBool,
    subs_added: AtomicBool,
    /// Range playback end (ms, f64 bits; NaN = none).
    stop_at: AtomicU64,
}

impl Shared {
    fn bump(&self, f: impl FnOnce(&mut PlayerState)) {
        let mut s = self.state.lock().unwrap();
        f(&mut s);
        s.serial += 1;
        drop(s);
        (self.notify)();
    }
    fn stop_at(&self) -> Option<f64> {
        let v = f64::from_bits(self.stop_at.load(Ordering::SeqCst));
        (!v.is_nan()).then_some(v)
    }
    fn wake_render(&self, force: bool) {
        let mut c = self.ctl.lock().unwrap();
        c.pending = true;
        c.force |= force;
        self.cv.notify_all();
    }
}

pub struct Player {
    mpv: Arc<Mpv>,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Options Yuzu always wants: no user config/scripts/OSD, paused, precise seeks, our subtitles only.
pub fn default_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("config", "no"), ("load-scripts", "no"), ("ytdl", "no"), ("terminal", "no"), ("idle", "yes"),
        ("osd-level", "0"), ("input-default-bindings", "no"), ("input-vo-keyboard", "no"),
        ("pause", "yes"), ("keep-open", "always"), ("hr-seek", "yes"), ("hr-seek-framedrop", "no"),
        ("sub-auto", "no"), ("sid", "no"), ("sub-fix-timing", "no"), ("audio-display", "no"),
        ("audio-client-name", "Yuzu"), ("hwdec", "no"), ("demuxer-max-back-bytes", "64MiB"),
    ]
}

impl Player {
    /// Start a player; `notify` is called (from player threads) whenever state or frame changed.
    pub fn new(extra_options: &[(&str, &str)], notify: Box<dyn Fn() + Send + Sync>) -> Result<Player, String> {
        let mut opts = default_options();
        for (k, v) in extra_options {
            opts.retain(|(ok, _)| ok != k);
            opts.push((k, v));
        }
        let mpv = Arc::new(Mpv::new(&opts)?);
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let sub_path = std::env::temp_dir().join(format!("yuzu-live-{}-{}.ass", std::process::id(), id));
        let shared = Arc::new(Shared {
            state: Mutex::new(PlayerState { paused: true, ..Default::default() }),
            frame: Mutex::new(None),
            ctl: Mutex::new(RenderCtl { size: (640, 360), pending: false, force: false, quit: false }),
            cv: Condvar::new(),
            quit: AtomicBool::new(false),
            notify,
            sub_path,
            have_subs: AtomicBool::new(false),
            subs_added: AtomicBool::new(false),
            stop_at: AtomicU64::new(f64::NAN.to_bits()),
        });
        for (i, (name, fmt)) in [("time-pos", FORMAT_DOUBLE), ("pause", FORMAT_FLAG), ("duration", FORMAT_DOUBLE), ("eof-reached", FORMAT_FLAG),
            ("dwidth", FORMAT_INT64), ("dheight", FORMAT_INT64), ("container-fps", FORMAT_DOUBLE), ("seeking", FORMAT_FLAG)].iter().enumerate() {
            mpv.observe(i as u64 + 1, name, *fmt)?;
        }
        mpv.request_log("error");
        let mut threads = Vec::new();
        // events
        {
            let (mpv, sh) = (mpv.clone(), shared.clone());
            threads.push(std::thread::Builder::new().name("mpv-events".into()).spawn(move || event_loop(&mpv, &sh)).map_err(|e| e.to_string())?);
        }
        // rendering
        {
            let sh = shared.clone();
            let sh_cb = shared.clone();
            let renderer = SwRenderer::new(mpv.clone(), Box::new(move || sh_cb.wake_render(false)))?;
            let mpv2 = mpv.clone();
            threads.push(std::thread::Builder::new().name("mpv-render".into()).spawn(move || render_loop(renderer, &mpv2, &sh)).map_err(|e| e.to_string())?);
        }
        Ok(Player { mpv, shared, threads })
    }

    pub fn state(&self) -> PlayerState { self.shared.state.lock().unwrap().clone() }

    /// The newest rendered frame not yet taken.
    pub fn take_frame(&self) -> Option<VideoFrame> { self.shared.frame.lock().unwrap().take() }

    pub fn open(&self, path: &Path) {
        self.shared.subs_added.store(false, Ordering::SeqCst);
        self.clear_stop();
        let p = path.to_string_lossy().to_string();
        self.shared.bump(|s| { *s = PlayerState { path: Some(path.to_path_buf()), paused: true, serial: s.serial, ..Default::default() }; });
        if let Err(e) = self.mpv.command(&["loadfile", &p, "replace"]) { self.shared.bump(|s| s.error = Some(e)); }
    }

    pub fn close(&self) {
        let _ = self.mpv.command(&["stop"]);
        self.shared.subs_added.store(false, Ordering::SeqCst);
        *self.shared.frame.lock().unwrap() = None;
        self.shared.bump(|s| { *s = PlayerState { paused: true, serial: s.serial, ..Default::default() }; });
    }

    /// Show these subtitles (complete ASS text) on the video; cheap to call repeatedly.
    pub fn set_subtitles(&self, ass: &str) -> Result<(), String> {
        std::fs::write(&self.shared.sub_path, ass).map_err(|e| e.to_string())?;
        self.shared.have_subs.store(true, Ordering::SeqCst);
        if !self.state().loaded { return Ok(()); }
        if self.shared.subs_added.swap(true, Ordering::SeqCst) {
            self.mpv.command_async(&["sub-reload"]);
        } else {
            self.mpv.command_async(&["sub-add", &self.shared.sub_path.to_string_lossy(), "select", "yuzu"]);
        }
        Ok(())
    }

    fn clear_stop(&self) {
        if self.shared.stop_at().is_some() {
            self.shared.stop_at.store(f64::NAN.to_bits(), Ordering::SeqCst);
            let _ = self.mpv.set_property("end", "none");
        }
    }

    /// Frame-exact seek (the frame shown is the one whose presentation time is `ms`, or the
    /// one before it). `exact = false` jumps to the nearest keyframe (fast scrubbing).
    pub fn seek(&self, ms: f64, exact: bool) {
        self.clear_stop();
        let t = format!("{:.6}", ms.max(0.0) / 1000.0);
        self.mpv.command_async(&["seek", &t, if exact { "absolute+exact" } else { "absolute+keyframes" }]);
        self.shared.bump(|s| { s.time_ms = ms.max(0.0); s.eof = false; });
    }

    pub fn set_pause(&self, pause: bool) {
        if !pause { self.clear_stop(); }
        let _ = self.mpv.set_property("pause", if pause { "yes" } else { "no" });
    }

    pub fn toggle_pause(&self) {
        let st = self.state();
        // at the real end of the file start over; after a range playback just continue
        if st.paused && st.eof && self.shared.stop_at().is_none() {
            let _ = self.mpv.command(&["seek", "0", "absolute+exact"]);
        }
        self.set_pause(!st.paused);
    }

    /// Play [start, end) and pause on the last frame before `end` (audio stops there too).
    /// The end point stays in force until the next seek / play / step (mpv reports EOF a few
    /// frames before it actually stops, so it cannot be cleared from the event thread).
    pub fn play_range(&self, start_ms: f64, end_ms: f64) {
        let t = format!("{:.6}", start_ms.max(0.0) / 1000.0);
        let _ = self.mpv.set_property("pause", "yes");
        let _ = self.mpv.command(&["seek", &t, "absolute+exact"]);
        self.shared.stop_at.store(end_ms.to_bits(), Ordering::SeqCst);
        let _ = self.mpv.set_property("end", &format!("{:.6}", end_ms.max(0.0) / 1000.0));
        let _ = self.mpv.set_property("pause", "no");
    }

    pub fn frame_step(&self, forward: bool) {
        self.clear_stop();
        self.mpv.command_async(&[if forward { "frame-step" } else { "frame-back-step" }]);
    }

    pub fn set_volume(&self, percent: f64) { let _ = self.mpv.set_property("volume", &format!("{:.0}", percent.clamp(0.0, 150.0))); }
    pub fn set_speed(&self, speed: f64) { let _ = self.mpv.set_property("speed", &format!("{:.3}", speed.clamp(0.1, 4.0))); }

    /// Size (pixels) the UI will display frames at; the current frame is re-rendered when it changes.
    pub fn set_output_size(&self, w: usize, h: usize) {
        let (w, h) = (w.clamp(16, 3840), h.clamp(16, 2160));
        let mut c = self.shared.ctl.lock().unwrap();
        if c.size != (w, h) {
            c.size = (w, h);
            c.pending = true;
            c.force = true;
            self.shared.cv.notify_all();
        }
    }

    /// Ask for the current frame again (e.g. after the widget was hidden).
    pub fn redraw(&self) { self.shared.wake_render(true); }

    pub fn mpv(&self) -> &Mpv { &self.mpv }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::SeqCst);
        {
            let mut c = self.shared.ctl.lock().unwrap();
            c.quit = true;
            self.shared.cv.notify_all();
        }
        let _ = self.mpv.command(&["quit"]);
        self.mpv.wakeup();
        for t in self.threads.drain(..) { let _ = t.join(); }
        let _ = std::fs::remove_file(&self.shared.sub_path);
    }
}

fn event_loop(mpv: &Mpv, sh: &Shared) {
    while !sh.quit.load(Ordering::SeqCst) {
        let Some(ev) = mpv.wait_event(0.25) else { continue };
        match ev {
            Event::Shutdown => break,
            Event::FileLoaded => {
                if sh.have_subs.load(Ordering::SeqCst) && !sh.subs_added.swap(true, Ordering::SeqCst) {
                    mpv.command_async(&["sub-add", &sh.sub_path.to_string_lossy(), "select", "yuzu"]);
                }
                // display size comes with the first decoded frame; the stream size is known right away
                let (w, h, fps, dur) = (mpv.get_i64("dwidth").or_else(|| mpv.get_i64("width")).unwrap_or(0), mpv.get_i64("dheight").or_else(|| mpv.get_i64("height")).unwrap_or(0), mpv.get_f64("container-fps").unwrap_or(0.0), mpv.get_f64("duration").unwrap_or(0.0));
                sh.bump(|s| { s.loaded = true; s.error = None; s.width = w as i32; s.height = h as i32; s.fps = fps; s.duration_ms = dur * 1000.0; });
                sh.wake_render(true);
            }
            Event::EndFile { reason, error } => {
                if reason == 4 { sh.bump(|s| { s.loaded = false; s.error = Some(format!("mpv could not play the file (error {})", error)); }); }
            }
            Event::Log { level, prefix, text } => {
                if level == "error" || level == "fatal" { sh.bump(|s| s.error = Some(format!("{}: {}", prefix, text.trim()))); }
            }
            Event::Property { id, value, .. } => {
                sh.bump(|s| match (id, value) {
                    (1, PropValue::Double(t)) => { s.time_ms = t * 1000.0; s.time_updated = Some(std::time::Instant::now()); }
                    (2, PropValue::Flag(p)) => s.paused = p,
                    (3, PropValue::Double(d)) => s.duration_ms = d * 1000.0,
                    (4, PropValue::Flag(e)) => s.eof = e,
                    (5, PropValue::Int(w)) => s.width = w as i32,
                    (6, PropValue::Int(h)) => s.height = h as i32,
                    (7, PropValue::Double(f)) => s.fps = f,
                    (8, PropValue::Flag(b)) => s.seeking = b,
                    _ => {}
                });
            }
            _ => {}
        }
    }
}

fn render_loop(renderer: SwRenderer, mpv: &Mpv, sh: &Shared) {
    let mut surf: Option<Surface> = None;
    loop {
        let (size, force) = {
            let mut c = sh.ctl.lock().unwrap();
            while !c.pending && !c.quit { c = sh.cv.wait(c).unwrap(); }
            if c.quit { break; }
            c.pending = false;
            let f = c.force;
            c.force = false;
            (c.size, f)
        };
        let fresh = renderer.update();
        if !fresh && !force { continue; }
        if !sh.state.lock().unwrap().loaded && !fresh { continue; }
        let s = match &mut surf {
            Some(s) if (s.width, s.height) == size => s,
            _ => { surf = Some(Surface::new(size.0, size.1)); surf.as_mut().unwrap() }
        };
        if renderer.render(s).is_err() { continue; }
        let t = mpv.get_f64("time-pos").unwrap_or(0.0) * 1000.0;
        *sh.frame.lock().unwrap() = Some(VideoFrame { width: s.width, height: s.height, rgba: s.to_rgba(), time_ms: t });
        (sh.notify)();
    }
    drop(renderer);
}
