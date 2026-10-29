use crate::dialog_ui::{self, DialogState};
use crate::media_ui::Media;
use crate::settings::Settings;
use crate::{style_ui, theme};
use eframe::egui::{self, text::LayoutJob, Align, Color32, Key, KeyboardShortcut, Modifiers, RichText, TextEdit, TextFormat};
use egui_extras::{Column, TableBuilder};
use osisub_core::ass::{Entry, Style};
use osisub_core::qc::{self, Issue, Kind, Rules};
use osisub_core::search::{self, Field};
use osisub_core::timing::{self, PostOptions};
use osisub_core::time;
use osisub_doc::worker::{Cmd, Event, Worker};
use osisub_doc::Document;
use osisub_lua::{MacroInfo, MacroKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Translate,
    Styles,
    Karaoke,
    Spell,
    Quality,
    Automation,
    Info,
}

struct MacroEntry {
    info: MacroInfo,
    script: String,
}

pub struct App {
    doc: Document,
    worker: Worker,
    macros: Vec<MacroEntry>,
    running: Option<String>,
    progress: f32,
    dialog: Option<(String, DialogState)>,
    log: String,
    status: String,
    tab: Tab,
    filter: String,
    rows: Vec<usize>,
    rows_key: (u64, String),
    rules: Rules,
    issues: Vec<Issue>,
    issue_map: HashMap<usize, Vec<Kind>>,
    issues_key: u64,
    qc_rx: Option<std::sync::mpsc::Receiver<(u64, Vec<Issue>)>>,
    qc_seen_rev: u64,
    qc_changed_at: std::time::Instant,
    start_buf: String,
    end_buf: String,
    style_sel: usize,
    scroll_to: Option<usize>,
    anchor: usize,
    timing_open: bool,
    shift_ms: i64,
    shift_all: bool,
    fps: f64,
    quit_confirm: bool,
    shot: Option<PathBuf>,
    shot_frame: u32,
    shot_requested: bool,
    shot_delay: Option<f32>,
    started: std::time::Instant,
    shot_done: Arc<AtomicBool>,
    media: Media,
    show_media: bool,
    find_open: bool,
    find: search::Options,
    find_selection_only: bool,
    resample_open: bool,
    res_x: i32,
    res_y: i32,
    post: PostOptions,
    post_selection_only: bool,
    last_autosave: std::time::Instant,
    settings: Settings,
    reference: Option<osisub_core::Script>,
    ref_lines: Vec<(i64, i64, String)>,
    fonts_open: bool,
    fixes_open: bool,
    stats_open: bool,
    export_open: bool,
    export_sel: Vec<usize>,
    export_job: Option<ExportJob>,
    tap: Option<TapState>,
    tap_latency_ms: i64,
    backed_up: std::collections::HashSet<PathBuf>,
    theme_dirty: bool,
    goto_open: bool,
    goto_focus: bool,
    tr_last_active: usize,
    find_focus: bool,
    goto_buf: String,
    import_styles: Option<Vec<(Style, bool)>>,
    autospell: bool,
    spell: Option<std::sync::Arc<osisub_spell::Speller>>,
    spell_load_rx: Option<std::sync::mpsc::Receiver<Result<osisub_spell::Speller, String>>>,
    spell_status: String,
    misses: Vec<osisub_spell::Miss>,
    miss_rx: Option<std::sync::mpsc::Receiver<Vec<osisub_spell::Miss>>>,
    miss_sel: Option<usize>,
    suggestions: Vec<String>,
    spell_ignore: std::collections::HashSet<String>,
    vad_open: bool,
    vad: osisub_media::VadOptions,
    vad_skip_existing: bool,
    autoplay: bool,
    play_wait: u32,
    loaded_scripts: Vec<PathBuf>,
    fixes: osisub_core::fixes::Fixes,
    fixes_selection_only: bool,
    min_dur_on: bool,
    min_gap_on: bool,
    font_report: Option<Vec<(osisub_core::fonts_used::FontUse, Vec<PathBuf>)>>,
    font_rx: Option<std::sync::mpsc::Receiver<Vec<(osisub_core::fonts_used::FontUse, Vec<PathBuf>)>>>,
}

struct ExportJob {
    queue: Vec<usize>,
    script: osisub_core::Script,
    path: PathBuf,
}

struct TapState {
    line: usize,
    /// Her hecenin başlangıcı (ms, satır başına göre).
    taps: Vec<i64>,
}

enum RowAction {
    InsertBefore,
    InsertAfter,
    Duplicate,
    Delete,
    ToggleComment,
    Merge,
    Copy,
    Cut,
    Paste,
    SetStyle(String),
}

fn autoload_dir() -> Option<PathBuf> {
    osisub_core::paths::config_dir().map(|d| d.join("automation/autoload"))
}

/// Düzenleme kutusu renklendirmesi: etiket blokları soluk, `\\N` vurgulu, yazım hataları kırmızı altçizgili.
fn editor_job(text: &str, sp: Option<&osisub_spell::Speller>) -> LayoutJob {
    let parts = osisub_core::tags::split(text);
    let visible: String = parts.iter().filter_map(|p| if let osisub_core::tags::Part::Text(t) = p { Some(*t) } else { None }).collect();
    let bad: Vec<(usize, usize)> = match sp {
        Some(sp) => osisub_spell::word_spans(&visible).into_iter().filter(|&(a, b)| osisub_spell::should_flag(sp, &visible[a..b])).collect(),
        None => vec![],
    };
    let font = egui::FontId::monospace(14.0);
    let normal = TextFormat { color: Color32::from_gray(225), font_id: font.clone(), ..Default::default() };
    let tag = TextFormat { color: theme::TAG_COLOR, font_id: font.clone(), ..Default::default() };
    let nl = TextFormat { color: theme::ACCENT, font_id: font.clone(), ..Default::default() };
    let wrong = TextFormat { underline: egui::Stroke::new(1.5, theme::BAD), ..normal.clone() };
    let mut job = LayoutJob::default();
    let mut off = 0usize;
    for p in parts {
        match p {
            osisub_core::tags::Part::Block(b) => job.append(&format!("{{{}}}", b), 0.0, tag.clone()),
            osisub_core::tags::Part::Text(t) => {
                // Hata sınırlarında ve \N konumlarında böl.
                let mut cuts: Vec<usize> = vec![0, t.len()];
                for &(a, b) in &bad {
                    for x in [a, b] { if x > off && x < off + t.len() { cuts.push(x - off); } }
                }
                let mut i = 0;
                while let Some(j) = t[i..].find("\\N") { cuts.push(i + j); cuts.push(i + j + 2); i += j + 2; }
                cuts.sort_unstable();
                cuts.dedup();
                for w in cuts.windows(2) {
                    let seg = &t[w[0]..w[1]];
                    if seg.is_empty() { continue; }
                    let fmt = if seg == "\\N" { &nl } else if bad.iter().any(|&(a, b)| off + w[0] >= a && off + w[1] <= b) { &wrong } else { &normal };
                    job.append(seg, 0.0, fmt.clone());
                }
                off += t.len();
            }
        }
    }
    job
}

mod grid;
mod tabs;
mod windows;

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: crate::Args) -> App {
        let crate::Args { file, scripts, screenshot: shot, tab, media, open_win, line, reference, shot_delay } = args;
        theme::setup(&cc.egui_ctx);
        let mut include = vec![];
        if let Some(d) = autoload_dir().and_then(|d| d.parent().map(|p| p.join("include"))) {
            include.push(d);
        }
        let worker = Worker::spawn(include, vec![]);
        let mut status = String::from("Hazır");
        let doc = match file {
            Some(p) => match Document::open(&p) {
                Ok(d) => d,
                Err(e) => {
                    status = e;
                    Document::new(osisub_core::Script::new_default())
                }
            },
            None => Document::new(osisub_core::Script::new_default()),
        };
        let mut all = scripts;
        if let Some(d) = autoload_dir() {
            if let Ok(rd) = std::fs::read_dir(d) {
                let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "lua")).collect();
                v.sort();
                all.extend(v);
            }
        }
        let mut loaded_scripts = vec![];
        for s in all {
            loaded_scripts.push(s.clone());
            worker.send(Cmd::Load(s));
        }
        let mut app = App {
            doc,
            worker,
            macros: vec![],
            running: None,
            progress: 0.0,
            dialog: None,
            log: String::new(),
            status,
            tab: match tab.as_deref() { Some("quality") => Tab::Quality, Some("translate") => Tab::Translate, Some("karaoke") => Tab::Karaoke, Some("spell") => Tab::Spell, Some("automation") => Tab::Automation, Some("info") => Tab::Info, _ => Tab::Styles },
            filter: String::new(),
            rows: vec![],
            rows_key: (u64::MAX, String::new()),
            rules: Rules::default(),
            issues: vec![],
            issue_map: HashMap::new(),
            issues_key: u64::MAX,
            qc_rx: None,
            qc_seen_rev: u64::MAX,
            qc_changed_at: std::time::Instant::now(),
            start_buf: String::new(),
            end_buf: String::new(),
            style_sel: 0,
            scroll_to: None,
            anchor: 0,
            timing_open: false,
            shift_ms: 500,
            shift_all: false,
            fps: 23.976,
            quit_confirm: false,
            shot,
            shot_frame: 0,
            shot_requested: false,
            shot_delay,
            started: std::time::Instant::now(),
            shot_done: Arc::new(AtomicBool::new(false)),
            media: Media::new(),
            show_media: false,
            find_open: false,
            find: search::Options::default(),
            find_selection_only: false,
            resample_open: false,
            res_x: 1920,
            res_y: 1080,
            post: PostOptions::default(),
            post_selection_only: true,
            last_autosave: std::time::Instant::now(),
            settings: Settings::load(),
            reference: None,
            ref_lines: vec![],
            fonts_open: false,
            fixes_open: false,
            stats_open: false,
            export_open: false,
            export_sel: vec![],
            export_job: None,
            tap: None,
            tap_latency_ms: 200,
            backed_up: Default::default(),
            theme_dirty: true,
            goto_open: false,
            goto_focus: false,
            tr_last_active: usize::MAX,
            find_focus: false,
            goto_buf: String::new(),
            import_styles: None,
            autospell: false,
            spell: None,
            spell_load_rx: None,
            spell_status: String::new(),
            misses: vec![],
            miss_rx: None,
            miss_sel: None,
            suggestions: vec![],
            spell_ignore: Default::default(),
            vad_open: false,
            vad: osisub_media::VadOptions::default(),
            vad_skip_existing: true,
            autoplay: false,
            play_wait: 0,
            loaded_scripts,
            fixes: osisub_core::fixes::Fixes::default(),
            fixes_selection_only: false,
            min_dur_on: false,
            min_gap_on: false,
            font_report: None,
            font_rx: None,
        };
        app.apply_settings();
        app.auto_load_dictionary();
        if let Some(r) = reference {
            if let Ok(sc) = osisub_core::io::load(&r) {
                app.ref_lines = { let mut v: Vec<(i64, i64, String)> = sc.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, osisub_core::tags::visible_text(&d.text).replace('\n', " ⏎ "))).collect(); v.sort_by_key(|x| x.0); v };
                app.reference = Some(sc);
            }
        }
        if let Some(first) = app.first_row() {
            app.doc.select(vec![first], first);
        }
        if let Some(n) = line {
            if let Some(&r) = app.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect::<Vec<_>>().get(n.saturating_sub(1)) {
                app.doc.select(vec![r], r);
                app.scroll_to = Some(r);
            }
        }
        match open_win.as_deref() {
            Some("find") => app.find_open = true,
            Some("timing") => app.timing_open = true,
            Some("resample") => app.resample_open = true,
            Some("fixes") => app.fixes_open = true,
            Some("stats") => app.stats_open = true,
            Some("vad") => app.vad_open = true,
            Some("spell") => { app.tab = Tab::Spell; app.autospell = true; }
            Some("pos") => app.media.tool = crate::media_ui::VideoTool::Pos,
            Some("clip") => app.media.tool = crate::media_ui::VideoTool::Clip,
            Some("rotate") => app.media.tool = crate::media_ui::VideoTool::Rotate,
            Some("scale") => app.media.tool = crate::media_ui::VideoTool::Scale,
            Some("spectrum") => app.media.spectrum = true,
            Some("play") => app.autoplay = true,
            Some("fonts") => { app.fonts_open = true; app.start_font_scan(); }
            _ => {}
        }
        if let Some(m) = media {
            app.media.open(m, &cc.egui_ctx);
            app.show_media = true;
        }
        app
    }

    fn apply_settings(&mut self) {
        self.rules = self.settings.rules();
        self.media.snap = self.settings.snap_frames;
        self.media.snap_kf = self.settings.snap_keyframes;
        self.post.lead_in_ms = self.settings.lead_in_ms;
        self.post.lead_out_ms = self.settings.lead_out_ms;
        self.post.adjacent_gap_ms = self.settings.adjacent_gap_ms;
        self.fps = self.settings.fps;
        self.theme_dirty = true;
    }

    fn save_settings(&mut self) {
        let s = &mut self.settings;
        s.max_cps = self.rules.max_cps;
        s.max_line_len = self.rules.max_line_len;
        s.max_lines = self.rules.max_lines;
        s.min_duration_ms = self.rules.min_duration_ms;
        s.max_duration_ms = self.rules.max_duration_ms;
        s.min_gap_ms = self.rules.min_gap_ms;
        s.snap_frames = self.media.snap;
        s.snap_keyframes = self.media.snap_kf;
        s.lead_in_ms = self.post.lead_in_ms;
        s.lead_out_ms = self.post.lead_out_ms;
        s.adjacent_gap_ms = self.post.adjacent_gap_ms;
        s.fps = self.fps;
        s.save();
    }

    // ------------------------------------------------------------ yazım denetimi

    fn first_row(&self) -> Option<usize> {
        self.doc.script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_)))
    }

    fn title(&self) -> String {
        let name = self.doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Adsız".into());
        format!("{}{} — osisub", name, if self.doc.is_dirty() { " •" } else { "" })
    }

    // ------------------------------------------------------------ dosya

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", &["ass", "ssa", "srt", "vtt", "lrc", "sub", "sbv", "ttml", "dfxp"]).pick_file() {
            self.open_path(p);
        }
    }

    fn open_path(&mut self, p: PathBuf) {
        match Document::open(&p) {
            Ok(d) => {
                self.doc = d;
                if let Some(f) = self.first_row() {
                    self.doc.select(vec![f], f);
                }
                self.status = format!("Açıldı: {}", p.display());
                self.settings.push_recent(&p.to_string_lossy());
                self.save_settings();
            }
            Err(e) => self.status = format!("Hata: {}", e),
        }
    }

    fn save(&mut self, as_new: bool) {
        let target = if as_new || self.doc.path.is_none() {
            rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).add_filter("LRC", &["lrc"]).add_filter("MicroDVD", &["sub"]).add_filter("SBV", &["sbv"]).add_filter("TTML", &["ttml"]).add_filter("Düz metin", &["txt"]).add_filter("CSV", &["csv"]).save_file()
        } else {
            self.doc.path.clone()
        };
        if let Some(p) = target {
            // Güvenlik: mevcut dosyanın oturumdaki ilk kaydında .bak yedeği al.
            if p.is_file() && self.backed_up.insert(p.clone()) {
                let mut bak = p.clone().into_os_string();
                bak.push(".bak");
                let _ = std::fs::copy(&p, PathBuf::from(bak));
            }
            match self.doc.save_as(p.clone()) {
                Ok(()) => { self.status = format!("Kaydedildi: {}", p.display()); self.settings.push_recent(&p.to_string_lossy()); }
                Err(e) => self.status = format!("Hata: {}", e),
            }
        }
    }

    // ------------------------------------------------------------ Lua

    fn run_macro(&mut self, id: usize) {
        if self.running.is_some() {
            return;
        }
        let Some(m) = self.macros.iter().find(|m| m.info.id == id) else { return };
        self.running = Some(m.info.name.clone());
        let mut props = vec![];
        if let Some(p) = &self.doc.path {
            props.push(("script_file".to_string(), p.to_string_lossy().to_string()));
            if let Some(d) = p.parent() {
                props.push(("script_dir".to_string(), d.to_string_lossy().to_string()));
            }
        }
        if let Some(p) = &self.media.path {
            props.push(("video_file".to_string(), p.to_string_lossy().to_string()));
            props.push(("audio_file".to_string(), p.to_string_lossy().to_string()));
        }
        self.sync_media_state();
        if let Some(f) = self.media.fps() { props.push(("video_position".to_string(), osisub_core::time::frame_from_ms(self.media.playhead, f).to_string())); }
        // Aegisub gibi: seçim boşsa etkin satır seçili sayılır.
        let mut selection = self.doc.selection.clone();
        if selection.is_empty() && self.doc.active < self.doc.script.entries.len() { selection.push(self.doc.active); }
        self.worker.send(Cmd::Run { id, script: self.doc.script.clone(), selection, active: self.doc.active, props });
    }

    fn load_lua(&mut self, p: PathBuf) {
        self.loaded_scripts.push(p.clone());
        self.worker.send(Cmd::Load(p));
    }

    /// Motoru sıfırlayıp tüm betikleri yeniden yükler (betik geliştirirken değişiklikleri almak için).
    fn reload_scripts(&mut self) {
        self.dialog = None;
        self.running = None;
        self.tap = None;
        let mut include = vec![];
        if let Some(d) = autoload_dir().and_then(|d| d.parent().map(|p| p.join("include"))) { include.push(d); }
        self.worker = Worker::spawn(include, vec![]);
        self.macros.clear();
        let list = self.loaded_scripts.clone();
        for p in list { self.worker.send(Cmd::Load(p)); }
        self.log.push_str("— betikler yeniden yüklendi —\n");
    }

    fn sync_media_state(&mut self) {
        if let Ok(mut m) = self.worker.media.lock() {
            m.fps = self.media.fps();
            m.video_size = self.media.info.as_ref().filter(|i| i.has_video).map(|i| (i.width as i32, i.height as i32));
            m.keyframes = self.media.fps().map(|f| self.media.keyframes.iter().map(|&ms| osisub_core::time::frame_from_ms(ms, f)).collect()).unwrap_or_default();
            m.audio_selection = match self.doc.script.entries.get(self.doc.active) {
                Some(Entry::Dialogue(d)) => Some((d.start_time, d.end_time)),
                _ => None,
            };
        }
    }

    fn pump_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.worker.events.try_recv() {
            match ev {
                Event::Loaded(Ok((info, ms))) => {
                    for m in ms {
                        self.macros.push(MacroEntry { info: m, script: info.name.clone() });
                    }
                    self.status = format!("Betik yüklendi: {}", info.name);
                }
                Event::Loaded(Err(e)) => {
                    self.log.push_str(&format!("Betik yüklenemedi: {}\n", e));
                    self.status = "Betik yüklenemedi (günlüğe bakın)".into();
                    self.tab = Tab::Automation;
                }
                Event::Dialog(spec) => {
                    let title = self.running.clone().unwrap_or_else(|| "Betik".into());
                    self.dialog = Some((title, DialogState::new(spec)));
                }
                Event::Progress { fraction, .. } => self.progress = fraction as f32,
                Event::Log(m) => self.log.push_str(&m),
                Event::Valid { .. } => {}
                Event::Done(r) => {
                    let name = self.running.take().unwrap_or_default();
                    self.progress = 0.0;
                    match r {
                        Ok(done) if self.export_job.is_some() => {
                            if done.cancelled {
                                self.export_job = None;
                                self.status = format!("{}: iptal edildi; dışa aktarma durdu", name);
                            } else {
                                let mut job = self.export_job.take().unwrap();
                                job.script = done.script;
                                if job.queue.is_empty() {
                                    match osisub_core::io::save(&job.script, &job.path) {
                                        Ok(()) => self.status = format!("Dışa aktarıldı: {}", job.path.display()),
                                        Err(e) => self.status = format!("Hata: {}", e),
                                    }
                                } else {
                                    let next = job.queue.remove(0);
                                    self.export_job = Some(job);
                                    self.run_export_step(next);
                                }
                            }
                        }
                        Ok(done) => {
                            let cancelled = done.cancelled;
                            self.doc.apply_run_done(done, &name);
                            self.status = if cancelled { format!("{}: iptal edildi", name) } else { format!("{}: tamamlandı", name) };
                        }
                        Err((msg, log)) => {
                            self.export_job = None;
                            self.log.push_str(&log);
                            self.log.push_str(&format!("HATA ({}): {}\n", name, msg));
                            self.status = format!("{}: hata (günlüğe bakın)", name);
                            self.tab = Tab::Automation;
                        }
                    }
                }
            }
        }
        if self.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    // ------------------------------------------------------------ önbellekler

    fn refresh_caches(&mut self) {
        let key = (self.doc.rev, self.filter.clone());
        if self.rows_key != key {
            let f = self.filter.to_lowercase();
            self.rows = self
                .doc
                .script
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| match e {
                    Entry::Dialogue(d) if f.is_empty() || osisub_core::tags::strip_tags(&d.text).to_lowercase().contains(&f) || d.actor.to_lowercase().contains(&f) || d.style.to_lowercase().contains(&f) => Some(i),
                    _ => None,
                })
                .collect();
            self.rows_key = key;
        }
        if self.qc_seen_rev != self.doc.rev {
            self.qc_seen_rev = self.doc.rev;
            self.qc_changed_at = std::time::Instant::now();
        }
        if let Some(rx) = &self.qc_rx {
            if let Ok((rev, issues)) = rx.try_recv() {
                self.qc_rx = None;
                if rev == self.doc.rev {
                    self.set_issues(issues, rev);
                }
            }
        }
        if self.issues_key != self.doc.rev {
            if self.doc.script.entries.len() < 5000 {
                let issues = qc::check(&self.doc.script, &self.rules);
                self.set_issues(issues, self.doc.rev);
            } else if self.qc_rx.is_none() && self.qc_changed_at.elapsed() >= std::time::Duration::from_millis(300) {
                // Büyük belge: düzenleme durunca arka planda hesapla.
                let (tx, rx) = std::sync::mpsc::channel();
                self.qc_rx = Some(rx);
                let (script, rules, rev) = (self.doc.script.clone(), self.rules.clone(), self.doc.rev);
                std::thread::spawn(move || { let _ = tx.send((rev, qc::check(&script, &rules))); });
            }
        }
    }

    fn set_issues(&mut self, issues: Vec<Issue>, rev: u64) {
        self.issue_map.clear();
        for i in &issues {
            self.issue_map.entry(i.entry).or_default().push(i.kind.clone());
        }
        self.issues = issues;
        self.issues_key = rev;
    }

    fn recheck(&mut self) {
        self.issues_key = u64::MAX;
    }

    // ------------------------------------------------------------ kısayollar

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input();
        let k = |m: Modifiers, key: Key| KeyboardShortcut::new(m, key);
        // Esc: küçük araç pencerelerini kapat (metin kutusu Esc'yi tüketmeden önce yakala).
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.find_open = false;
            self.goto_open = false;
            self.timing_open = false;
            self.resample_open = false;
            self.fixes_open = false;
            self.stats_open = false;
            self.vad_open = false;
            self.export_open = false;
            self.fonts_open = false;
        }
        if !typing {
            let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
            for ev in events {
                match ev {
                    egui::Event::Copy => { let t = self.doc.copy_lines(); if !t.is_empty() { ctx.copy_text(t); } }
                    egui::Event::Cut => { let t = self.doc.cut_lines(); if !t.is_empty() { ctx.copy_text(t); } }
                    egui::Event::Paste(t) => { let n = self.doc.paste_lines(&t); if n > 0 { self.status = format!("{} satır yapıştırıldı", n); self.scroll_to = Some(self.doc.active); } }
                    _ => {}
                }
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            match ext.as_str() {
                "ass" | "ssa" | "srt" | "vtt" | "lrc" | "sbv" | "ttml" | "dfxp" => { if self.doc.is_dirty() { self.status = "Kaydedilmemiş değişiklik var; önce kaydedin".into(); } else { self.open_path(p); } }
                "lua" => self.load_lua(p),
                _ => { self.media.open(p, ctx); self.show_media = true; }
            }
        }
        let (open, save, save_as, undo, redo, redo2, dup, ins, merge) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&k(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::Y)),
                i.consume_shortcut(&k(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::D)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::Enter)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::M)),
            )
        });
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) { self.media.toggle_play(&self.doc, ctx); }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F6)) { self.media.set_active_start(&mut self.doc); }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F7)) { self.media.set_active_end(&mut self.doc); }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::H))) { self.find_open = true; self.find_focus = true; }
        let (alt_up, alt_down) = ctx.input_mut(|i| (i.consume_key(Modifiers::ALT, Key::ArrowUp), i.consume_key(Modifiers::ALT, Key::ArrowDown)));
        if (alt_up || alt_down) && !self.rows.is_empty() {
            let pos = self.rows.iter().position(|&r| r == self.doc.active).unwrap_or(0);
            let np = if alt_up { pos.saturating_sub(1) } else { (pos + 1).min(self.rows.len() - 1) };
            let r = self.rows[np];
            self.doc.select(vec![r], r);
            self.anchor = np;
            self.scroll_to = Some(r);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::G))) { self.goto_open = true; self.goto_focus = true; }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus))) { self.settings.zoom = (self.settings.zoom + 0.1).min(2.5); self.theme_dirty = true; }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus))) { self.settings.zoom = (self.settings.zoom - 0.1).max(0.6); self.theme_dirty = true; }
        if self.tap.is_some() && !typing && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
            self.record_tap();
        }
        if !typing {
            let (prev, next) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Comma), i.consume_key(Modifiers::NONE, Key::Period)));
            if prev || next { self.media.step_frames(if next { 1 } else { -1 }); }
        }
        if open { self.open_dialog(); }
        if save_as { self.save(true); } else if save { self.save(false); }
        if undo || redo || redo2 {
            if undo { self.doc.undo(); } else { self.doc.redo(); }
            self.scroll_to = Some(self.doc.active);
        }
        if dup { self.doc.duplicate_selected(); }
        if ins { self.doc.insert_after_active(); self.scroll_to = Some(self.doc.active); }
        if merge { self.doc.merge_selected(); }
        if !typing {
            let (up, down, pgup, pgdn, del, all) = ctx.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::PageUp),
                    i.consume_key(Modifiers::NONE, Key::PageDown),
                    i.consume_key(Modifiers::NONE, Key::Delete),
                    i.consume_shortcut(&k(Modifiers::COMMAND, Key::A)),
                )
            });
            let step = if pgup || pgdn { 10 } else { 1 };
            if (up || pgup || down || pgdn) && !self.rows.is_empty() {
                let pos = self.rows.iter().position(|&r| r == self.doc.active).unwrap_or(0);
                let np = if up || pgup { pos.saturating_sub(step) } else { (pos + step).min(self.rows.len() - 1) };
                let r = self.rows[np];
                self.doc.select(vec![r], r);
                self.anchor = np;
                self.scroll_to = Some(r);
            }
            if del { self.doc.delete_selected(); }
            if all { let v = self.rows.clone(); self.doc.select(v, self.doc.active); }
        }
    }

    // ------------------------------------------------------------ çizim

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Dosya", |ui| {
                if ui.add(egui::Button::new("Yeni").shortcut_text("")).clicked() {
                    self.doc = Document::new(osisub_core::Script::new_default());
                    ui.close();
                }
                if ui.add(egui::Button::new("Aç…").shortcut_text("Ctrl+O")).clicked() { ui.close(); self.open_dialog(); }
                ui.menu_button("Son açılanlar", |ui| {
                    let recent = self.settings.recent.clone();
                    if recent.is_empty() { ui.label(RichText::new("Yok").weak()); }
                    for r in recent {
                        if ui.button(&r).clicked() { ui.close(); self.open_path(PathBuf::from(&r)); }
                    }
                });
                if ui.button("Referans altyazı aç…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", &["ass", "ssa", "srt", "vtt", "lrc", "sub", "sbv", "ttml", "dfxp"]).pick_file() {
                        match osisub_core::io::load(&p) {
                            Ok(s) => { self.ref_lines = { let mut v: Vec<(i64, i64, String)> = s.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, osisub_core::tags::visible_text(&d.text).replace('\n', " ⏎ "))).collect(); v.sort_by_key(|x| x.0); v }; self.reference = Some(s); self.status = format!("Referans: {}", p.display()); }
                            Err(e) => self.status = format!("Hata: {}", e),
                        }
                    }
                }
                if self.reference.is_some() && ui.button("Referansı kapat").clicked() { self.reference = None; self.ref_lines.clear(); ui.close(); }
                ui.separator();
                if ui.add(egui::Button::new("Kaydet").shortcut_text("Ctrl+S")).clicked() { ui.close(); self.save(false); }
                if ui.add(egui::Button::new("Farklı kaydet…").shortcut_text("Ctrl+Shift+S")).clicked() { ui.close(); self.save(true); }
                if ui.button("Dışa aktar (filtreler)…").clicked() { ui.close(); self.export_open = true; }
                ui.separator();
                if ui.button("Çıkış").clicked() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close); }
            });
            ui.menu_button("Düzen", |ui| {
                if ui.add_enabled(self.doc.can_undo(), egui::Button::new(format!("Geri al{}", self.doc.undo_desc().map(|d| format!(": {}", d)).unwrap_or_default())).shortcut_text("Ctrl+Z")).clicked() { self.doc.undo(); ui.close(); }
                if ui.add_enabled(self.doc.can_redo(), egui::Button::new("Yinele").shortcut_text("Ctrl+Y")).clicked() { self.doc.redo(); ui.close(); }
                ui.separator();
                if ui.add(egui::Button::new("Satır ekle").shortcut_text("Ctrl+Enter")).clicked() { self.doc.insert_after_active(); self.scroll_to = Some(self.doc.active); ui.close(); }
                if ui.add(egui::Button::new("Çoğalt").shortcut_text("Ctrl+D")).clicked() { self.doc.duplicate_selected(); ui.close(); }
                if ui.add(egui::Button::new("Sil").shortcut_text("Del")).clicked() { self.doc.delete_selected(); ui.close(); }
                if ui.add(egui::Button::new("Seçilenleri birleştir").shortcut_text("Ctrl+M")).clicked() { self.doc.merge_selected(); ui.close(); }
                if ui.button("Satırları dengele").clicked() { self.doc.balance_selected(self.rules.max_line_len); ui.close(); }
            });
            ui.menu_button("Zamanlama", |ui| {
                if ui.button("Kaydır / kareye oturt…").clicked() { self.timing_open = true; ui.close(); }
                if ui.button("Son-işleyici (lead-in/out, anahtar kare)…").clicked() { self.timing_open = true; ui.close(); }
                if ui.add_enabled(self.media.waveform().is_some(), egui::Button::new("Sesten satır oluştur…")).clicked() { self.vad_open = true; ui.close(); }
                ui.menu_button("Sırala", |ui| {
                    for (k, l) in [(qc::SortKey::Start, "Başlangıç zamanına göre"), (qc::SortKey::End, "Bitiş zamanına göre"), (qc::SortKey::Style, "Stile göre"), (qc::SortKey::Actor, "Konuşana göre"), (qc::SortKey::Effect, "Efekte göre"), (qc::SortKey::Layer, "Katmana göre"), (qc::SortKey::Text, "Metne göre")] {
                        if ui.button(l).clicked() {
                            let mut s = self.doc.script.clone();
                            qc::sort_by(&mut s, k);
                            self.doc.apply_new_script("Sırala", s);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Görünüm", |ui| {
                let mut light = self.settings.light_theme;
                if ui.checkbox(&mut light, "Açık tema").changed() { self.settings.light_theme = light; self.theme_dirty = true; }
                if ui.add(egui::Button::new("Yakınlaştır").shortcut_text("Ctrl +")).clicked() { self.settings.zoom = (self.settings.zoom + 0.1).min(2.5); self.theme_dirty = true; }
                if ui.add(egui::Button::new("Uzaklaştır").shortcut_text("Ctrl -")).clicked() { self.settings.zoom = (self.settings.zoom - 0.1).max(0.6); self.theme_dirty = true; }
                if ui.button("Yakınlaştırmayı sıfırla").clicked() { self.settings.zoom = 1.0; self.theme_dirty = true; }
                ui.separator();
                if ui.add(egui::Button::new("Zamana git…").shortcut_text("Ctrl+G")).clicked() { self.goto_open = true; self.goto_focus = true; ui.close(); }
            });
            ui.menu_button("Video", |ui| {
                if ui.button("Video / ses aç…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Ortam", &["mkv", "mp4", "avi", "webm", "mov", "mka", "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"]).pick_file() {
                        self.media.open(p, ui.ctx());
                        self.show_media = true;
                    }
                }
                ui.checkbox(&mut self.show_media, "Medya panelini göster");
                ui.checkbox(&mut self.media.spectrum, "Spektrum göster");
                ui.checkbox(&mut self.media.snap, "Karelere yapış");
                ui.checkbox(&mut self.media.snap_kf, "Anahtar karelere yapış");
                ui.menu_button("Video aracı", |ui| {
                    use crate::media_ui::VideoTool as T;
                    for (t, l) in [(T::None, "Yok"), (T::Pos, "Konum (\\pos sürükle)"), (T::Clip, "Kırpma (\\clip dikdörtgen; sağ tık = kaldır)"), (T::Rotate, "Döndürme (\\frz; Shift = 15°)"), (T::Scale, "Ölçek (\\fscx/\\fscy)")] {
                        if ui.radio_value(&mut self.media.tool, t, l).clicked() { ui.close(); }
                    }
                });
                if ui.add_enabled(self.media.is_open(), egui::Button::new("Satırı oynat / durdur").shortcut_text("F5")).clicked() { self.media.toggle_play(&self.doc, ui.ctx()); ui.close(); }
            });
            ui.menu_button("Otomasyon", |ui| {
                if self.macros.is_empty() { ui.label(RichText::new("Yüklü makro yok").weak()); }
                let list: Vec<(usize, String, String)> = self.macros.iter().filter(|m| m.info.kind == MacroKind::Macro).map(|m| (m.info.id, m.info.name.clone(), m.info.description.clone())).collect();
                for (id, name, desc) in list {
                    if ui.add_enabled(self.running.is_none(), egui::Button::new(&name)).on_hover_text(desc).clicked() { ui.close(); self.run_macro(id); }
                }
                ui.separator();
                if ui.button("Lua betiği yükle…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Lua", &["lua"]).pick_file() { self.load_lua(p); }
                }
                if ui.button("Betikleri yeniden yükle").clicked() { ui.close(); self.reload_scripts(); }
            });
            ui.menu_button("Araçlar", |ui| {
                if ui.add(egui::Button::new("Bul / Değiştir…").shortcut_text("Ctrl+F")).clicked() { self.find_open = true; self.find_focus = true; ui.close(); }
                if ui.button("İstatistikler…").clicked() { self.stats_open = true; ui.close(); }
                if ui.button("Sık hataları düzelt…").clicked() { self.fixes_open = true; ui.close(); }
                if ui.button("Yazı tipi denetimi…").clicked() { self.fonts_open = true; self.start_font_scan(); ui.close(); }
                if ui.button("Çözünürlüğü yeniden örnekle…").clicked() {
                    let (x, y) = self.doc.script.res();
                    self.res_x = x; self.res_y = y; self.resample_open = true; ui.close();
                }
                ui.separator();
                if ui.button("Kalite kontrolü göster").clicked() { self.tab = Tab::Quality; ui.close(); }
                if ui.button("Metni temizle (etiket + boşluk)").clicked() {
                    let mut s = self.doc.script.clone();
                    for d in s.dialogues_mut() { d.text = qc::clean_text(&d.text); }
                    self.doc.apply_new_script("Metni temizle", s);
                    ui.close();
                }
            });
        });
    }

    fn autosave(&mut self) {
        if !self.doc.is_dirty() || self.last_autosave.elapsed().as_secs() < 60 { return; }
        self.last_autosave = std::time::Instant::now();
        let Some(base) = osisub_core::paths::data_dir() else { return };
        let dir = base.join("autosave");
        if std::fs::create_dir_all(&dir).is_err() { return; }
        let name = self.doc.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "adsiz".into());
        let _ = std::fs::write(dir.join(format!("{}.autosave.ass", name)), self.doc.script.to_ass_string());
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.shot_frame += 1;
        ctx.request_repaint();
        let delay_ok = self.shot_delay.map_or(true, |d| self.started.elapsed().as_secs_f32() >= d);
        if self.shot_frame >= 12 && delay_ok && !self.shot_requested && (!self.media.is_open() || self.media.ready() || self.shot_frame > 900) && self.font_rx.is_none() && !self.autoplay && self.play_wait == 0 && !self.autospell && self.miss_rx.is_none() && self.spell_load_rx.is_none() {
            self.shot_requested = true;
            let done = self.shot_done.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                let mut buf = Vec::with_capacity((w * h * 4) as usize);
                for p in &img.pixels { buf.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]); }
                if let Some(im) = image::RgbaImage::from_raw(w, h, buf) { let _ = im.save(&path); }
                done.store(true, Ordering::SeqCst);
            })));
        }
        if self.shot_done.load(Ordering::SeqCst) { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.theme_dirty {
            self.theme_dirty = false;
            theme::apply(&ctx, self.settings.light_theme);
            ctx.set_zoom_factor(self.settings.zoom);
        }
        if std::env::var_os("OSISUB_DEBUG_INPUT").is_some() {
            ctx.input(|i| for e in &i.events { if !matches!(e, egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_)) { eprintln!("girdi: {:?}", e); } });
        }
        self.pump_events(&ctx);
        self.pump_spell(&ctx);
        self.refresh_caches();
        if self.issues_key != self.doc.rev { ctx.request_repaint_after(std::time::Duration::from_millis(150)); }
        self.shortcuts(&ctx);
        self.refresh_caches(); // kısayollar belgeyi değiştirmiş olabilir
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

        if ctx.input(|i| i.viewport().close_requested()) && self.doc.is_dirty() && !self.quit_confirm && self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_confirm = true;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let (w, h) = self.doc.script.res();
                    ui.label(RichText::new(format!("{}×{}  |  seçili {}  |  {} sorun", w, h, self.doc.selection.len(), self.issues.len())).weak());
                });
            });
        });
        if self.show_media || self.media.is_open() {
            egui::Panel::top("media").resizable(true).default_size(280.0).show(ui, |ui| self.media.ui(ui, &mut self.doc));
        }
        if self.tap.is_some() && !self.media.is_playing() { self.finish_tap(); }
        egui::Panel::bottom("editor").resizable(true).default_size(190.0).show(ui, |ui| self.editor(ui));
        egui::Panel::right("side").resizable(true).default_size(380.0).show(ui, |ui| self.side(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| self.grid(ui));

        self.timing_window(&ctx);
        self.find_window(&ctx);
        self.fonts_window(&ctx);
        self.fixes_window(&ctx);
        self.stats_window(&ctx);
        self.export_window(&ctx);
        self.vad_window(&ctx);
        self.goto_window(&ctx);
        self.import_window(&ctx);
        self.resample_window(&ctx);
        self.autosave();
        if let Some((title, st)) = self.dialog.as_mut() {
            if let Some(res) = dialog_ui::show(&ctx, title, st) {
                self.worker.reply_dialog(res);
                self.dialog = None;
            }
        }
        if self.quit_confirm {
            egui::Window::new("Kaydedilmemiş değişiklikler").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.label("Kaydedilmemiş değişiklikler var.");
                ui.horizontal(|ui| {
                    if ui.button("Kaydet ve çık").clicked() { self.save(false); if !self.doc.is_dirty() { self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); } }
                    if ui.button("Kaydetmeden çık").clicked() { self.doc = Document::new(osisub_core::Script::new_default()); self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    if ui.button("İptal").clicked() { self.quit_confirm = false; }
                });
            });
        }
        if self.autoplay && self.media.ready() {
            self.autoplay = false;
            self.play_wait = 40;
            self.media.toggle_play(&self.doc, &ctx);
        }
        if self.autospell && self.spell.is_some() && self.miss_rx.is_none() {
            self.autospell = false;
            self.start_spellcheck();
        }
        if self.play_wait > 0 { self.play_wait -= 1; ctx.request_repaint(); }
        self.handle_screenshot(&ctx);
    }
}
