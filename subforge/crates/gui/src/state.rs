use crate::theme;
use eframe::egui::{self, TextureHandle};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use subforge_app::automation::{self, RunHandle, ScriptEntry};
use subforge_app::media::{self, Frame, MediaInfo, SharedWaveform};
use subforge_app::project::Project;
use subforge_app::search::SearchOpts;
use subforge_app::settings::Settings;
use subforge_core::qc::{self, Issue, Severity};
use subforge_lua::{Dialog, DialogResult, DialogValue};

pub struct LuaDialogState {
    pub dialog: Dialog,
    pub values: HashMap<String, DialogValue>,
    pub reply: std::sync::mpsc::Sender<Option<DialogResult>>,
}

pub struct RunState {
    pub export_to: Option<String>,
    pub handle: RunHandle,
    pub title: String,
    pub progress: Option<f64>,
    pub task: String,
    pub dialog: Option<LuaDialogState>,
}

#[derive(Default)]
pub struct Windows {
    pub find: bool,
    pub shift: bool,
    pub styles: bool,
    pub qc: bool,
    pub automation: bool,
    pub props: bool,
    pub settings: bool,
    pub about: bool,
    pub log: bool,
    pub retime: bool,
    pub filters: bool,
    pub resample: bool,
    pub keys: bool,
    pub kara: bool,
    pub console: bool,
    pub spell: bool,
    pub select: bool,
    pub fonts: bool,
}

pub struct ShiftState {
    pub ms: i64,
    pub frames: i64,
    pub use_frames: bool,
    pub start: bool,
    pub end: bool,
    pub scope: u8, // 0 = selected, 1 = all, 2 = from active line onward
    pub chain_gap: i64,
    pub lead_in: i64,
    pub lead_out: i64,
    pub rt: [String; 4],
}

impl Default for ShiftState {
    fn default() -> Self {
        ShiftState { ms: 0, frames: 0, use_frames: false, start: true, end: true, scope: 0, chain_gap: 300, lead_in: 0, lead_out: 0, rt: Default::default() }
    }
}

pub struct VideoState {
    pub path: PathBuf,
    pub info: MediaInfo,
    pub tex: Option<TextureHandle>,
    pub pending: Option<Receiver<Result<Frame, String>>>,
    pub shown_ms: i64,
    pub shown_version: u64,
    pub want_ms: i64,
    pub burn: bool,
    pub keyframes: Vec<i64>,
    pub error: Option<String>,
    pub width: i32,
    pub vmode: u8, // 0 off, 1 pos, 2 org, 3 clip, 4 rotate, 5 scale
    pub drag0: Option<(f32, f32, f64, f64)>,
    pub play: Option<media::Playback>,
    pub play_until: Option<i64>,
}

pub struct WaveState {
    pub path: PathBuf,
    pub wave: SharedWaveform,
    pub start_ms: f64,
    pub ms_per_px: f64,
    pub sel: Option<(i64, i64)>,
    pub drag: Option<WaveDrag>,
    pub player: Option<std::process::Child>,
    pub snap_keys: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum WaveDrag {
    NewSel(i64),
    Start(usize),
    End(usize),
    Move(usize, i64),
}

pub struct App {
    pub settings: Settings,
    pub proj: Project,
    pub sel: BTreeSet<usize>,
    pub active: Option<usize>,
    pub anchor: usize,
    pub scroll_to: Option<usize>,
    pub issues: Vec<Issue>,
    pub issue_sev: HashMap<usize, Severity>,
    pub version: u64,
    pub qc_version: u64,
    pub qc_rx: Option<(u64, Receiver<Vec<Issue>>)>,
    pub seen_version: u64,
    pub changed_at: std::time::Instant,
    pub win: Windows,
    pub find: SearchOpts,
    pub find_msg: String,
    pub shift: ShiftState,
    pub status: String,
    pub video: Option<VideoState>,
    pub wave: Option<WaveState>,
    pub scripts: Vec<ScriptEntry>,
    pub run: Option<RunState>,
    pub log: Vec<String>,
    pub style_sel: usize,
    pub start_buf: String,
    pub end_buf: String,
    pub dur_buf: String,
    pub buf_for: Option<(usize, u64)>,
    pub confirm_close: bool,
    pub allow_close: bool,
    pub last_autosave: std::time::Instant,
    pub show_video: bool,
    pub show_wave: bool,
    pub wave_h: f32,
    pub vad: subforge_app::vad::VadOpts,
    pub text_focus: bool,
    pub tmp_ass: PathBuf,
    pub reference: Option<Vec<(i64, i64, String)>>,
    pub ref_path: Option<String>,
    pub ref_texts: Vec<String>,
    pub ref_version: u64,
    pub filter_sel: std::collections::HashSet<(usize, usize)>,
    pub console_code: String,
    pub speller: Option<subforge_app::spell::Speller>,
    pub spell_lang: String,
    pub spell_results: Vec<subforge_app::spell::Misspelling>,
    pub spell_ignore: std::collections::HashSet<String>,
    pub spell_sel: usize,
    pub spell_sugg: Vec<String>,
    pub spell_fix: String,
    pub spell_dl: Option<Receiver<Result<(), String>>>,
    pub spell_msg: String,
    pub kara_syls: Vec<(i64, String, String, String)>,
    pub kara_for: Option<(usize, u64)>,
    pub kara_sel: usize,
    pub rs: (i32, i32, bool, bool),
    pub sel_opts: subforge_app::search::SelectOpts,
    pub font_list: Vec<subforge_app::fonts::FontUse>,
    pub style_prev: Option<(u64, TextureHandle)>,
    pub style_prev_rx: Option<(u64, Receiver<Result<Frame, String>>)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, arg: Option<String>) -> App {
        let settings = Settings::load();
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, settings.dark, settings.ui_scale);
        let scripts = automation::scan_with(&settings.script_dirs(), &settings.include_dirs());
        let mut app = App {
            proj: Project::new(),
            sel: BTreeSet::from([0]),
            active: Some(0),
            anchor: 0,
            scroll_to: None,
            issues: vec![],
            issue_sev: HashMap::new(),
            version: 1,
            qc_version: 0,
            qc_rx: None,
            seen_version: 0,
            changed_at: std::time::Instant::now(),
            win: Windows::default(),
            find: SearchOpts::default(),
            find_msg: String::new(),
            shift: ShiftState::default(),
            status: "Hazır — bir altyazı dosyası açın veya sürükleyip bırakın.".into(),
            video: None,
            wave: None,
            scripts,
            run: None,
            log: vec![],
            style_sel: 0,
            start_buf: String::new(),
            end_buf: String::new(),
            dur_buf: String::new(),
            buf_for: None,
            confirm_close: false,
            allow_close: false,
            last_autosave: std::time::Instant::now(),
            show_video: true,
            show_wave: true,
            wave_h: 230.0,
            vad: Default::default(),
            text_focus: false,
            tmp_ass: std::env::temp_dir().join(format!("subforge-preview-{}.ass", std::process::id())),
            reference: None,
            ref_path: None,
            ref_texts: vec![],
            ref_version: 0,
            filter_sel: Default::default(),
            speller: None,
            spell_lang: "tr_TR".into(),
            spell_results: vec![],
            spell_ignore: Default::default(),
            spell_sel: 0,
            spell_sugg: vec![],
            spell_fix: String::new(),
            spell_dl: None,
            spell_msg: String::new(),
            console_code: "-- subs, sel (seçili satır indeksleri), active kullanılabilir\nfor _, i in ipairs(sel) do\n  local l = subs[i]\n  aegisub.log(\"%d: %s\\n\", i, l.text)\nend\n".to_string(),
            kara_syls: vec![],
            kara_for: None,
            kara_sel: 0,
            rs: (1920, 1080, true, false),
            sel_opts: Default::default(),
            font_list: vec![],
            style_prev: None,
            style_prev_rx: None,
            settings,
        };
        if let Some(a) = arg {
            app.open_path(&a);
        }
        if let Ok(spec) = std::env::var("SUBFORGE_RUN") {
            if let Some((path, idx)) = spec.rsplit_once('|') {
                app.run_macro(PathBuf::from(path), idx.parse().unwrap_or(0), "test".into());
            }
        }
        if std::env::var("SUBFORGE_AUTOPLAY").is_ok() {
            app.toggle_playback(None);
        }
        // handy for screenshots/tests: SUBFORGE_OPEN=styles,qc,find,...
        if let Ok(list) = std::env::var("SUBFORGE_OPEN") {
            for w in list.split(',') {
                match w.trim() {
                    "styles" => app.win.styles = true,
                    "qc" => app.win.qc = true,
                    "find" => app.win.find = true,
                    "shift" => app.win.shift = true,
                    "automation" => app.win.automation = true,
                    "props" => app.win.props = true,
                    "settings" => app.win.settings = true,
                    "about" => app.win.about = true,
                    "retime" => app.win.retime = true,
                    "resample" => app.win.resample = true,
                    "select" => app.win.select = true,
                    "fonts" => app.win.fonts = true,
                    "spell" => app.win.spell = true,
                    "kara" => app.win.kara = true,
                    "console" => app.win.console = true,
                    "keys" => app.win.keys = true,
                    _ => {}
                }
            }
        }
        app
    }

    pub fn bump(&mut self) {
        self.version += 1;
    }

    pub fn selected_vec(&self) -> Vec<usize> {
        self.sel.iter().copied().filter(|&i| i < self.proj.script.events.len()).collect()
    }

    pub fn select_only(&mut self, i: usize) {
        self.sel.clear();
        self.sel.insert(i);
        self.active = Some(i);
        self.anchor = i;
    }

    pub fn clamp_selection(&mut self) {
        let n = self.proj.script.events.len();
        self.sel.retain(|&i| i < n);
        if n == 0 {
            self.active = None;
        } else if self.active.map(|a| a >= n).unwrap_or(true) {
            let a = self.sel.iter().next().copied().unwrap_or(n - 1);
            self.active = Some(a);
            self.sel.insert(a);
        }
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    /// Run QC on a worker thread, debounced so typing stays smooth even on 100k-line scripts.
    pub fn refresh_qc(&mut self) {
        if self.seen_version != self.version {
            self.seen_version = self.version;
            self.changed_at = std::time::Instant::now();
        }
        if let Some((ver, rx)) = &self.qc_rx {
            if let Ok(issues) = rx.try_recv() {
                let ver = *ver;
                self.qc_rx = None;
                self.qc_version = ver;
                self.issues = issues;
                self.issue_sev.clear();
                for i in &self.issues {
                    let e = self.issue_sev.entry(i.event).or_insert(i.severity);
                    if i.severity > *e {
                        *e = i.severity;
                    }
                }
            }
            return;
        }
        let quiet = self.changed_at.elapsed() >= std::time::Duration::from_millis(if self.proj.script.events.len() > 20_000 { 600 } else { 120 });
        if self.qc_version != self.version && quiet {
            let script = self.proj.script.clone();
            let cfg = self.settings.qc.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.qc_rx = Some((self.version, rx));
            std::thread::spawn(move || {
                let _ = tx.send(qc::check(&script, &cfg));
            });
        }
    }

    pub fn load_reference(&mut self, path: &str) {
        match subforge_core::load_path(path) {
            Ok((sc, _)) => {
                let mut v: Vec<(i64, i64, String)> = sc
                    .events
                    .iter()
                    .filter(|e| !e.is_comment())
                    .map(|e| (e.start, e.end, subforge_core::tags::visible_text(&e.text).replace('\n', " / ")))
                    .filter(|(_, _, t)| !t.trim().is_empty())
                    .collect();
                v.sort_by_key(|x| (x.0, x.1));
                self.set_status(format!("Referans altyazı: {path} ({} satır)", v.len()));
                self.reference = Some(v);
                self.ref_path = Some(path.to_string());
                self.ref_version = 0;
            }
            Err(e) => self.set_status(format!("Referans açılamadı: {e}")),
        }
    }

    /// For each event, the reference line with the greatest time overlap.
    pub fn refresh_reference(&mut self) {
        let Some(r) = &self.reference else { return };
        if self.ref_version == self.version && self.ref_texts.len() == self.proj.script.events.len() {
            return;
        }
        self.ref_version = self.version;
        self.ref_texts = self
            .proj
            .script
            .events
            .iter()
            .map(|e| {
                let hi = r.partition_point(|x| x.0 < e.end);
                let lo = hi.saturating_sub(12);
                let mut best: Option<(i64, usize)> = None;
                for (k, x) in r[lo..hi].iter().enumerate() {
                    let ov = x.1.min(e.end) - x.0.max(e.start);
                    if ov > 0 && best.map(|b| ov > b.0).unwrap_or(true) {
                        best = Some((ov, lo + k));
                    }
                }
                best.map(|(_, i)| r[i].2.clone()).unwrap_or_default()
            })
            .collect();
    }

    // ---------------------------------------------------------- files
    pub fn open_path(&mut self, path: &str) {
        match Project::open(path) {
            Ok(p) => {
                self.proj = p;
                self.sel.clear();
                self.sel.insert(0);
                self.active = Some(0);
                self.anchor = 0;
                self.scroll_to = Some(0);
                self.bump();
                self.settings.push_recent(path);
                self.settings.save();
                let det = self.proj.encoding.clone();
                self.set_status(format!("Açıldı: {path}  ({det}, {} satır)", self.proj.script.events.len()));
                // auto-load media referenced by the script
                if let Some(v) = self.proj.script.garbage_get("Video File") {
                    let vp = self.resolve_media(&v, path);
                    if vp.exists() {
                        self.load_video(vp);
                    }
                }
                if let Some(a) = self.proj.script.garbage_get("Active Line").and_then(|a| a.parse::<usize>().ok()) {
                    self.goto_line(a);
                }
                if let Some(v) = self.proj.script.garbage_get("Audio File") {
                    let ap = self.resolve_media(&v, path);
                    if ap.exists() && self.wave.is_none() {
                        self.load_audio(ap);
                    }
                }
            }
            Err(e) => self.set_status(format!("Açılamadı: {e}")),
        }
    }

    fn resolve_media(&self, v: &str, script_path: &str) -> PathBuf {
        let p = PathBuf::from(v.trim_start_matches("?video/").trim_start_matches("?audio/"));
        if p.is_absolute() || p.exists() {
            p
        } else {
            std::path::Path::new(script_path).parent().map(|d| d.join(&p)).unwrap_or(p)
        }
    }

    /// Remember media and position in the script like Aegisub's Project Garbage.
    pub fn sync_garbage(&mut self, script_path: Option<&std::path::Path>) {
        let rel = |p: &std::path::Path| -> String {
            script_path.and_then(|sp| sp.parent()).and_then(|d| p.strip_prefix(d).ok()).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string_lossy().to_string())
        };
        if let Some(v) = &self.video {
            let (name, pos) = (rel(&v.path), (v.want_ms as f64 / 1000.0 * v.info.fps.unwrap_or(24.0)) as i64);
            self.proj.script.garbage_set("Video File", &name);
            self.proj.script.garbage_set("Video Position", &pos.to_string());
        }
        if let Some(w) = &self.wave {
            let name = rel(&w.path);
            self.proj.script.garbage_set("Audio File", &name);
        }
        if let Some(a) = self.active {
            self.proj.script.garbage_set("Active Line", &a.to_string());
        }
    }

    pub fn save(&mut self) {
        if self.proj.path.is_none() {
            return self.save_as();
        }
        let p = self.proj.path.clone();
        self.sync_garbage(p.as_deref());
        match self.proj.save() {
            Ok(()) => {
                let p = self.proj.path.clone().unwrap();
                self.set_status(format!("Kaydedildi: {}", p.display()));
            }
            Err(e) => self.set_status(format!("Kaydedilemedi: {e}")),
        }
    }

    pub fn save_as(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass"]).set_file_name("altyazi.ass").save_file() {
            let ps = p.to_string_lossy().to_string();
            self.sync_garbage(Some(&p));
            match self.proj.save_as(&ps) {
                Ok(()) => {
                    self.settings.push_recent(&ps);
                    self.settings.save();
                    self.set_status(format!("Kaydedildi: {ps}"));
                }
                Err(e) => self.set_status(format!("Kaydedilemedi: {e}")),
            }
        }
    }

    pub fn export(&mut self, ext: &str) {
        if let Some(p) = rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(format!("altyazi.{ext}")).save_file() {
            let ps = p.to_string_lossy().to_string();
            match subforge_core::save_path(&self.proj.script, &ps) {
                Ok(()) => self.set_status(format!("Dışa aktarıldı: {ps}")),
                Err(e) => self.set_status(format!("Hata: {e}")),
            }
        }
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", &["ass", "ssa", "srt", "vtt"]).pick_file() {
            self.open_path(&p.to_string_lossy());
        }
    }

    pub fn load_video(&mut self, path: PathBuf) {
        match media::probe(&path) {
            Ok(info) => {
                if !info.has_video {
                    // audio-only: just use as audio
                    self.load_audio(path);
                    return;
                }
                let keyframes = media::keyframes(&path).unwrap_or_default();
                if info.has_audio && self.wave.is_none() {
                    self.load_audio(path.clone());
                }
                if let Some(f) = info.fps {
                    self.settings.default_fps = f;
                }
                self.set_status(format!("Video: {} ({}x{}, {:.3} fps)", path.display(), info.width, info.height, info.fps.unwrap_or(0.0)));
                let want_ms = self.active_event().map(|e| e.start).unwrap_or(0);
                self.video = Some(VideoState { path, info, tex: None, pending: None, shown_ms: -1, shown_version: 0, want_ms, burn: true, keyframes, error: None, width: 640, vmode: 0, drag0: None, play: None, play_until: None });
            }
            Err(e) => self.set_status(format!("Video açılamadı: {e}")),
        }
    }

    pub fn load_audio(&mut self, path: PathBuf) {
        let wave = media::spawn_waveform(&path, 10);
        self.wave = Some(WaveState { path, wave, start_ms: 0.0, ms_per_px: 20.0, sel: None, drag: None, player: None, snap_keys: false });
    }

    pub fn fps(&self) -> f64 {
        self.video.as_ref().and_then(|v| v.info.fps).unwrap_or(self.settings.default_fps)
    }

    pub fn autosave(&mut self) {
        if self.settings.autosave_secs == 0 || !self.proj.dirty || self.last_autosave.elapsed().as_secs() < self.settings.autosave_secs {
            return;
        }
        self.last_autosave = std::time::Instant::now();
        let dir = subforge_app::settings::config_dir().join("autosave");
        let _ = std::fs::create_dir_all(&dir);
        let name = self.proj.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        let _ = subforge_core::save_path(&self.proj.script, &dir.join(format!("{name}.autosave.ass")).to_string_lossy());
    }

    // ---------------------------------------------------------- edit ops with bookkeeping
    pub fn do_undo(&mut self) {
        if self.proj.undo() {
            self.bump();
            self.clamp_selection();
            self.set_status("Geri alındı");
        }
    }

    pub fn do_redo(&mut self) {
        if self.proj.redo() {
            self.bump();
            self.clamp_selection();
            self.set_status("Yinelendi");
        }
    }

    pub fn insert_line(&mut self, before: bool) {
        let at = self.active.unwrap_or(0);
        let i = if before && !self.proj.script.events.is_empty() { self.proj.insert_before(at) } else if self.proj.script.events.is_empty() { self.proj.insert_after(None) } else { self.proj.insert_after(Some(at)) };
        self.select_only(i);
        self.scroll_to = Some(i);
        self.bump();
    }

    pub fn delete_selected(&mut self) {
        let v = self.selected_vec();
        if v.is_empty() {
            return;
        }
        let first = v[0];
        self.proj.delete(&v);
        self.bump();
        let n = self.proj.script.events.len();
        if n > 0 {
            self.select_only(first.min(n - 1));
        } else {
            self.sel.clear();
            self.active = None;
        }
    }

    pub fn duplicate_selected(&mut self) {
        let v = self.selected_vec();
        let out = self.proj.duplicate(&v);
        self.bump();
        if let Some(&l) = out.last() {
            self.sel = out.iter().copied().collect();
            self.active = Some(l);
            self.scroll_to = Some(l);
        }
    }

    pub fn join_selected(&mut self) {
        let v = self.selected_vec();
        if let Some(i) = self.proj.join(&v, "\\N") {
            self.select_only(i);
            self.bump();
        }
    }

    pub fn toggle_comment(&mut self) {
        let v = self.selected_vec();
        self.proj.toggle_comment(&v);
        self.bump();
    }

    pub fn goto_line(&mut self, i: usize) {
        if i < self.proj.script.events.len() {
            self.select_only(i);
            self.scroll_to = Some(i);
            if let Some(e) = self.proj.script.events.get(i) {
                if let Some(v) = &mut self.video {
                    v.want_ms = e.start;
                }
                if let Some(w) = &mut self.wave {
                    // keep the line in view
                    let width_ms = w.ms_per_px * 900.0;
                    if (e.start as f64) < w.start_ms || (e.end as f64) > w.start_ms + width_ms {
                        w.start_ms = (e.start as f64 - width_ms * 0.15).max(0.0);
                    }
                }
            }
        }
    }

    pub fn active_event(&self) -> Option<&subforge_core::Event> {
        self.active.and_then(|i| self.proj.script.events.get(i))
    }

    pub fn log_line(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 2000 {
            self.log.drain(..500);
        }
    }

    pub fn refresh_scripts(&mut self) {
        self.scripts = automation::scan_with(&self.settings.script_dirs(), &self.settings.include_dirs());
    }

    pub fn ctx_repaint_if_busy(&self, ctx: &egui::Context) {
        let wave_busy = self.wave.as_ref().map(|w| !w.wave.lock().map(|w| w.done).unwrap_or(true)).unwrap_or(false);
        let video_busy = self.video.as_ref().map(|v| v.pending.is_some()).unwrap_or(false);
        if self.run.is_some() || wave_busy || video_busy || self.qc_rx.is_some() || self.qc_version != self.version {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
