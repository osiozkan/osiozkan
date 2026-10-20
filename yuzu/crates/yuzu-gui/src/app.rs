use crate::commands::{self, Keymap, When};
use crate::i18n::{self, t, tf};
use crate::luadialog::{self, LuaDialogState};
use crate::palette::{self, Palette, Pick};
use crate::prefs::{self, Prefs};
use crate::settings::Settings;
use crate::worker::{self, Cmd, Msg, ScriptInfo, Worker};
use crate::media::{self, Drag, LineSpan, MediaMsg, MediaState};
use crate::preview;
use asscore::karaoke::{KaraLine, Split};
use asscore::{ass, ops, subrip, tags, time, Event, Script, Style};
use eframe::egui::{self, Color32, Key, KeyboardShortcut, RichText};
use egui_extras::{Column, TableBuilder};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use yuzu_lua::{DialogResult, ProjectInfo};

#[derive(Clone)]
struct Snapshot {
    script: Script,
    selection: BTreeSet<usize>,
    active: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum Panel { Find, Shift, Styles, Qc, Props, Console, Post, Resample, About, Karaoke, Fonts, Fix, Reference, Compare, Sync, Glossary, SelectLines, MultiReplace, Translate, StyleAssist, Scripts, Spell }

pub struct App {
    doc: Script,
    path: Option<PathBuf>,
    dirty: bool,
    selection: BTreeSet<usize>,
    active: usize,
    anchor: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit_key: u64,
    /// Bumped on every document change (live subtitles on the video follow it).
    doc_gen: u64,
    scroll_to: Option<usize>,
    settings: Settings,
    open: Vec<Panel>,
    // find / replace
    find: String,
    repl: String,
    find_opts: ops::FindOpts,
    find_msg: String,
    // shift
    shift_ms: String,
    shift_frames: bool,
    shift_which: u8,
    /// 0 all lines, 1 selected lines, 2 from the first selected line onward
    shift_scope: u8,
    // time buffers
    buf_start: String,
    buf_end: String,
    buf_dur: String,
    buf_key: (usize, i64, i64),
    // style manager
    style_sel: usize,
    // qc
    qc_cfg: ops::QcConfig,
    qc_issues: Vec<ops::Issue>,
    qc_by_line: std::collections::HashMap<usize, u8>,
    // post processor
    pp: ops::PostProc,
    resample_xy: (i32, i32),
    // automation
    worker: Worker,
    scripts: Vec<ScriptInfo>,
    console: Vec<(i32, String)>,
    lua_dialog: Option<(LuaDialogState, Sender<DialogResult>, String)>,
    busy: Option<String>,
    progress: f32,
    pre_run: Option<Snapshot>,
    last_status: String,
    last_autosave: std::time::Instant,
    text_cursor: Option<(usize, usize)>,
    show_preview: bool,
    show_text_stripped: bool,
    ctx: egui::Context,
    quit_confirm: bool,
    media: MediaState,
    wave_drag: Option<Drag>,
    /// Last drawn timing view (for keyboard scrolling and tests).
    wave_rect: Option<egui::Rect>,
    last_active_seen: usize,
    wave_height: f32,
    kara: Option<(usize, KaraLine)>,
    kara_markup: String,
    kara_text: String,
    tap: Option<TapState>,
    tap_latency_ms: i64,
    /// Karaoke syllables on the timing view.
    kara_audio: bool,
    encoding: String,
    bom: bool,
    fonts_report: Vec<String>,
    osi: tools::OsiTools,
    spell: spell::SpellState,
    edit: editing::EditTools,
    auto: automation::AutoState,
    fixes: asscore::fixes::CommonFixes,
    fix_preview: Vec<(String, usize)>,
    vis: visual::Visual,
    /// Last drawn video frame (visual tools; tests).
    video_rect: Option<egui::Rect>,
    keymap: Keymap,
    palette: Option<Palette>,
    prefs: Option<Prefs>,
}

/// Parts of the active line the timing keys play (Aegisub: S/R, Q, W, E, D; T = to the end).
#[derive(PartialEq, Clone, Copy)]
enum PlayPart { Line, Before, After, First, Last, ToEnd }

struct TapState {
    line: usize,
    started: std::time::Instant,
    start_ms: i64,
    taps: Vec<i64>,
}

fn fps_of(s: &Settings) -> time::Fps { time::Fps { num: s.fps.0, den: s.fps.1 } }

fn setup_fonts(ctx: &egui::Context) {
    // Fallbacks after egui's own fonts: a symbol font (arrows, check marks, box drawing) and a
    // CJK font, each the first one installed.
    const SYMBOLS: &[&str] = &[
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Apple Symbols.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    const CJK: &[&str] = &[
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();
    let mut any = false;
    for (name, list) in [("symbols", SYMBOLS), ("cjk", CJK)] {
        if let Some(data) = list.iter().find_map(|p| std::fs::read(p).ok()) {
            fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(data)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(name.into());
            }
            any = true;
        }
    }
    if any { ctx.set_fonts(fonts); }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<String>, video: Option<String>) -> App {
        App::with_context(&cc.egui_ctx, file, video)
    }

    /// Build the app on a bare egui context (also used by the headless UI tests).
    pub fn with_context(egui_ctx: &egui::Context, file: Option<String>, video: Option<String>) -> App {
        let settings = Settings::load();
        i18n::set_lang(i18n::Lang::from_code(&settings.lang));
        setup_fonts(egui_ctx);
        egui_ctx.set_visuals(if settings.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
        egui_ctx.set_zoom_factor(settings.ui_scale);
        let keymap = Keymap::with_overrides(&settings.shortcuts);
        let worker = worker::spawn(egui_ctx.clone());
        let mut app = App {
            doc: Script::new(),
            path: None,
            dirty: false,
            selection: BTreeSet::new(),
            active: 0,
            anchor: 0,
            undo: vec![],
            redo: vec![],
            last_edit_key: 0,
            doc_gen: 0,
            scroll_to: None,
            qc_cfg: ops::QcConfig { max_cps: settings.max_cps, max_cpl: settings.max_cpl, ..Default::default() },
            settings,
            open: vec![],
            find: String::new(),
            repl: String::new(),
            find_opts: ops::FindOpts::default(),
            find_msg: String::new(),
            shift_ms: "0".into(),
            shift_frames: false,
            shift_which: 0,
            shift_scope: 1,
            buf_start: String::new(),
            buf_end: String::new(),
            buf_dur: String::new(),
            buf_key: (usize::MAX, 0, 0),
            style_sel: 0,
            qc_issues: vec![],
            qc_by_line: Default::default(),
            pp: ops::PostProc::default(),
            resample_xy: (1920, 1080),
            worker,
            scripts: vec![],
            console: vec![],
            lua_dialog: None,
            busy: None,
            progress: 0.0,
            pre_run: None,
            last_status: t("Ready").into(),
            last_autosave: std::time::Instant::now(),
            text_cursor: None,
            show_preview: true,
            show_text_stripped: false,
            ctx: egui_ctx.clone(),
            quit_confirm: false,
            media: MediaState::new(egui_ctx.clone()),
            wave_drag: None,
            wave_rect: None,
            last_active_seen: usize::MAX,
            wave_height: 150.0,
            kara: None,
            kara_markup: String::new(),
            kara_text: String::new(),
            tap: None,
            tap_latency_ms: 0,
            kara_audio: false,
            fonts_report: vec![],
            osi: tools::OsiTools::default(),
            spell: spell::SpellState::default(),
            edit: editing::EditTools::default(),
            auto: automation::AutoState::default(),
            fixes: asscore::fixes::CommonFixes::default(),
            fix_preview: vec![],
            encoding: "UTF-8".into(),
            bom: true,
            vis: visual::Visual::default(),
            video_rect: None,
            keymap,
            palette: None,
            prefs: None,
        };
        app.apply_media_settings();
        app.doc.events.push(Event::new(0, 2000, "Default", ""));
        app.reload_scripts();
        if let Some(f) = file { app.open_path(PathBuf::from(f)); }
        if let Some(v) = video { app.media.open(PathBuf::from(v)); }
        app
    }

    /// Copy the video/audio defaults from the settings into the live state.
    fn apply_media_settings(&mut self) {
        self.media.set_volume(self.settings.volume);
        self.media.snap_keyframes = self.settings.snap_keyframes;
        self.media.follow = self.settings.follow;
        self.media.show_spectrum = self.settings.show_spectrum;
        self.tap_latency_ms = self.settings.tap_latency;
        self.qc_cfg.max_cps = self.settings.max_cps;
        self.qc_cfg.max_cpl = self.settings.max_cpl;
    }

    /// Apply a saved preferences dialog.
    fn apply_prefs(&mut self, ctx: &egui::Context, p: Prefs) {
        let dirs_changed = p.s.script_dirs != self.settings.script_dirs;
        let recent = std::mem::take(&mut self.settings.recent);
        let palette_recent = std::mem::take(&mut self.settings.palette_recent);
        self.settings = p.s;
        self.settings.recent = recent;
        self.settings.palette_recent = palette_recent;
        self.keymap = p.keymap;
        self.settings.shortcuts = self.keymap.overrides();
        i18n::set_lang(i18n::Lang::from_code(&self.settings.lang));
        ctx.set_visuals(if self.settings.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
        ctx.set_zoom_factor(self.settings.ui_scale);
        self.apply_media_settings();
        self.run_qc();
        self.settings.save();
        if dirs_changed { self.reload_scripts(); }
        self.last_status = t("Settings saved").into();
    }

    // ------------------------------------------------------------ document plumbing

    fn snapshot(&self) -> Snapshot {
        Snapshot { script: self.doc.clone(), selection: self.selection.clone(), active: self.active }
    }

    fn touch(&mut self) { self.doc_gen = self.doc_gen.wrapping_add(1); }

    fn push_undo(&mut self) {
        self.touch();
        let s = self.snapshot();
        self.undo.push(s);
        if self.undo.len() > 300 { self.undo.remove(0); }
        // cap total undo memory (~400 MB) so huge scripts stay responsive
        let per = self.doc.events.len() * 220 + self.doc.events.iter().map(|e| e.text.len()).sum::<usize>();
        while self.undo.len() > 2 && self.undo.len() * per > 400_000_000 { self.undo.remove(0); }
        self.redo.clear();
        self.dirty = true;
        self.last_edit_key = 0;
    }

    /// Start an edit: only push an undo snapshot when the edit key changes (coalesces typing).
    fn begin_edit(&mut self, key: u64) {
        self.touch();
        if key == 0 || key != self.last_edit_key {
            let k = key;
            self.push_undo();
            self.last_edit_key = k;
        }
        self.dirty = true;
    }

    fn do_undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let cur = self.snapshot();
            self.redo.push(cur);
            self.restore(s);
        }
    }

    fn do_redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let cur = self.snapshot();
            self.undo.push(cur);
            self.restore(s);
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.touch();
        self.doc = s.script;
        self.selection = s.selection;
        self.active = s.active;
        self.dirty = true;
        self.last_edit_key = 0;
        self.buf_key = (usize::MAX, 0, 0);
        self.clamp_sel();
    }

    fn clamp_sel(&mut self) {
        let n = self.doc.events.len();
        if n == 0 { self.selection.clear(); self.active = 0; return; }
        self.active = self.active.min(n - 1);
        self.selection.retain(|&i| i < n);
        if self.selection.is_empty() { self.selection.insert(self.active); }
    }

    fn sel_vec(&self) -> Vec<usize> { self.selection.iter().copied().collect() }

    fn select_only(&mut self, i: usize) {
        self.selection.clear();
        self.selection.insert(i);
        self.active = i;
        self.anchor = i;
    }

    fn open_path(&mut self, p: PathBuf) {
        match std::fs::read(&p) {
            Ok(b) => {
                let dec = asscore::encoding::decode(&b);
                self.encoding = dec.encoding.clone();
                self.bom = dec.had_bom || dec.encoding == "UTF-8";
                let txt = dec.text;
                let mut sc = asscore::formats::load_any(&p.to_string_lossy(), &txt, &asscore::formats::FormatOptions { fps: fps_of(&self.settings).as_f64(), ..Default::default() }).unwrap_or_else(|| asscore::load_str(&p.to_string_lossy(), &txt));
                if sc.events.is_empty() { sc.events.push(Event::new(0, 2000, "Default", "")); }
                self.doc = sc;
                self.touch();
                self.path = Some(p.clone());
                self.undo.clear();
                self.redo.clear();
                self.dirty = false;
                self.select_only(0);
                self.scroll_to = Some(0);
                self.settings.recent.retain(|r| r != &p);
                self.settings.recent.insert(0, p.clone());
                self.settings.save();
                let (x, y) = self.doc.res();
                self.resample_xy = (x, y);
                self.last_status = tf!("Opened {}", p.display());
                self.autoload_glossary();
                self.run_qc();
                self.restore_project(&p);
            }
            Err(e) => self.last_status = tf!("Open failed: {}", e),
        }
    }

    fn garbage_set(&mut self, key: &str, value: &str) {
        if let Some(e) = self.doc.garbage.iter_mut().find(|(k, _)| k == key) { e.1 = value.to_string(); } else { self.doc.garbage.push((key.to_string(), value.to_string())); }
    }

    fn garbage_get(&self, key: &str) -> Option<String> {
        self.doc.garbage.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    /// Store a media path relative to the script when possible (Aegisub project garbage convention).
    fn rel_media_path(&self, media: &std::path::Path, script: &std::path::Path) -> String {
        script.parent().and_then(|d| media.strip_prefix(d).ok()).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| media.to_string_lossy().to_string())
    }

    fn sync_garbage(&mut self, script: &std::path::Path) {
        if let Some(v) = self.media.video.clone() { let r = self.rel_media_path(&v, script); self.garbage_set("Video File", &r); self.garbage_set("Video Position", &self.settings_frame().to_string()); }
        if let Some(a) = self.media.audio.clone() { if self.media.video.as_ref() != Some(&a) { let r = self.rel_media_path(&a, script); self.garbage_set("Audio File", &r); } }
        self.garbage_set("Active Line", &self.active.to_string());
    }

    fn settings_frame(&self) -> i64 { fps_of(&self.settings).frame_at(self.media.pos_ms) }

    fn restore_project(&mut self, script: &std::path::Path) {
        let resolve = |s: &str| -> Option<PathBuf> {
            let p = PathBuf::from(s);
            let full = if p.is_absolute() { p } else { script.parent()?.join(p) };
            full.exists().then_some(full)
        };
        if let Some(v) = self.garbage_get("Video File").and_then(|s| resolve(&s)) {
            self.media.open(v);
            if let Some(f) = self.garbage_get("Video Position").and_then(|s| s.parse::<i64>().ok()) { let t = fps_of(&self.settings).ms_from_frame(f); self.media.seek(t); }
        }
        if let Some(a) = self.garbage_get("Audio File").and_then(|s| resolve(&s)) { self.media.open_audio(a); }
        if let Some(l) = self.garbage_get("Active Line").and_then(|s| s.parse::<usize>().ok()) { if l < self.doc.events.len() { self.select_only(l); self.scroll_to = Some(l); } }
    }

    fn save_to(&mut self, p: PathBuf) {
        let lower = p.to_string_lossy().to_ascii_lowercase();
        if !lower.ends_with(".srt") && !lower.ends_with(".vtt") { self.sync_garbage(&p); }
        let out = if lower.ends_with(".srt") { subrip::write_srt(&self.doc) } else if lower.ends_with(".vtt") { subrip::write_vtt(&self.doc) } else { ass::write(&self.doc) };
        let bytes = asscore::encoding::encode(&out, &self.encoding, self.bom && !lower.ends_with(".vtt"));
        match std::fs::write(&p, bytes) {
            Ok(_) => {
                if !lower.ends_with(".srt") && !lower.ends_with(".vtt") { self.path = Some(p.clone()); self.dirty = false; }
                self.last_status = tf!("Saved {}", p.display());
            }
            Err(e) => self.last_status = tf!("Save failed: {}", e),
        }
    }

    fn save(&mut self) {
        match self.path.clone() {
            Some(p) if !p.to_string_lossy().to_lowercase().ends_with(".srt") && !p.to_string_lossy().to_lowercase().ends_with(".vtt") => self.save_to(p),
            _ => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Advanced SubStation Alpha", &["ass"]).set_file_name("subtitles.ass").save_file() {
            self.save_to(p);
        }
    }

    fn run_qc(&mut self) {
        self.qc_issues = ops::qc(&self.doc, &self.qc_cfg);
        if self.osi.glossary.is_some() {
            for (i, m, _) in self.glossary_issues() { self.qc_issues.push(ops::Issue { index: i, severity: ops::Severity::Warning, code: "glossary", message: m }); }
            self.qc_issues.sort_by_key(|x| x.index);
        }
        self.qc_by_line.clear();
        for i in &self.qc_issues {
            let sev = match i.severity { ops::Severity::Error => 3, ops::Severity::Warning => 2, ops::Severity::Info => 1 };
            let e = self.qc_by_line.entry(i.index).or_insert(0);
            *e = (*e).max(sev);
        }
    }

    fn insert_after_active(&mut self) {
        self.push_undo();
        let (start, style) = self.doc.events.get(self.active).map(|e| (e.end, e.style.clone())).unwrap_or((0, "Default".into()));
        let ev = Event::new(start, start + 2000, &style, "");
        let at = (self.active + 1).min(self.doc.events.len());
        self.doc.events.insert(at, ev);
        self.select_only(at);
        self.scroll_to = Some(at);
    }

    fn insert_before_active(&mut self) {
        self.push_undo();
        let (end, style) = self.doc.events.get(self.active).map(|e| (e.start, e.style.clone())).unwrap_or((2000, "Default".into()));
        let ev = Event::new((end - 2000).max(0), end, &style, "");
        let at = self.active.min(self.doc.events.len());
        self.doc.events.insert(at, ev);
        self.select_only(at);
        self.scroll_to = Some(at);
    }

    fn delete_selected(&mut self) {
        if self.selection.is_empty() { return; }
        self.push_undo();
        let idx = self.sel_vec();
        let first = idx[0];
        ops::delete_events(&mut self.doc, &idx);
        if self.doc.events.is_empty() { self.doc.events.push(Event::new(0, 2000, "Default", "")); }
        let n = self.doc.events.len();
        self.select_only(first.min(n - 1));
    }

    fn duplicate_selected(&mut self) {
        self.push_undo();
        let idx = self.sel_vec();
        ops::duplicate_events(&mut self.doc, &idx);
        let first = idx[0];
        self.select_only(first + 1);
    }

    fn merge_selected(&mut self) {
        let idx = self.sel_vec();
        if idx.len() < 2 { return; }
        self.push_undo();
        ops::merge_events(&mut self.doc, &idx, "\\N");
        self.select_only(idx[0]);
    }

    fn reload_scripts(&mut self) {
        let (files, include) = worker::scan_scripts(&self.settings.script_dirs);
        let _ = self.worker.tx.send(Cmd::Load { files, include });
    }

    fn project_info(&self) -> ProjectInfo {
        let fps = self.fps();
        ProjectInfo {
            script_path: self.path.clone(),
            video_path: self.media.video.clone(),
            audio_path: self.media.audio.clone(),
            fps: Some(self.settings.fps),
            video_size: self.media.info.as_ref().filter(|i| i.has_video).map(|i| (i.width, i.height)),
            keyframes: self.media.keyframes.clone(),
            video_position: fps.frame_at(self.media.pos_ms),
            active_row: self.active as i64,
            data_dir: Some(crate::settings::config_dir()),
            user_dir: Some(crate::settings::config_dir()),
            ..Default::default()
        }
    }

    fn run_macro(&mut self, script: usize, macro_idx: usize, name: &str) {
        if self.busy.is_some() { return; }
        self.pre_run = Some(self.snapshot());
        self.busy = Some(name.to_string());
        self.progress = 0.0;
        let _ = self.worker.tx.send(Cmd::Run {
            script,
            macro_idx,
            doc: Box::new(self.doc.clone()),
            sel: self.sel_vec(),
            active: Some(self.active),
            project: self.project_info(),
        });
    }

    fn poll_worker(&mut self) {
        while let Ok(m) = self.worker.rx.try_recv() {
            match m {
                Msg::Loaded(v) => {
                    let errs = v.iter().filter(|s| s.error.is_some()).count();
                    for s in &v { if let Some(e) = &s.error { self.console.push((1, format!("{}: {}\n", s.path.display(), e))); } }
                    self.last_status = format!("{}{}", tf!("{} automation script(s) loaded", v.len() - errs), if errs > 0 { tf!(", {} failed (see Console)", errs) } else { String::new() });
                    self.scripts = v;
                    self.reset_script_watch();
                }
                Msg::Log(l, s) => self.console.push((l, s)),
                Msg::Dialog(spec, reply) => {
                    let title = self.busy.clone().unwrap_or_else(|| "Automation".into());
                    self.lua_dialog = Some((LuaDialogState::new(spec), reply, title));
                }
                Msg::FileDialog { save, title, default, reply } => {
                    let mut d = rfd::FileDialog::new().set_title(&title);
                    if !default.is_empty() { d = d.set_file_name(&default); }
                    let r = if save { d.save_file() } else { d.pick_file() };
                    let _ = reply.send(r.map(|p| p.to_string_lossy().to_string()));
                }
                Msg::Progress { kind, value, text } => {
                    if kind == "set" { self.progress = (value / 100.0) as f32; }
                    else if kind == "task" || kind == "title" || kind == "status" { self.last_status = text; }
                }
                Msg::Done(r) => {
                    self.busy = None;
                    self.lua_dialog = None;
                    match r {
                        Ok((doc, out)) => {
                            if out.cancelled {
                                self.last_status = t("Macro cancelled").into();
                            } else {
                                if let Some(pre) = self.pre_run.take() { self.undo.push(pre); self.redo.clear(); }
                                self.doc = *doc;
                                self.touch();
                                self.dirty = true;
                                if self.doc.events.is_empty() { self.doc.events.push(Event::new(0, 2000, "Default", "")); }
                                if let Some(sel) = out.selection { self.selection = sel.into_iter().collect(); }
                                if let Some(a) = out.active { self.active = a; self.selection.insert(a); }
                                self.clamp_sel();
                                self.buf_key = (usize::MAX, 0, 0);
                                self.last_status = t("Macro finished").into();
                                self.run_qc();
                            }
                        }
                        Err(e) => {
                            self.console.push((1, format!("{}\n", e)));
                            self.last_status = t("Macro failed (see Console)").into();
                            if !self.open.contains(&Panel::Console) { self.open.push(Panel::Console); }
                        }
                    }
                }
                Msg::States(token, st) => self.got_macro_states(token, st),
                Msg::Unavailable(name) => {
                    self.busy = None;
                    self.pre_run = None;
                    self.last_status = tf!("{} is not available for the current selection", name);
                }
            }
        }
    }

    fn toggle(&mut self, p: Panel) {
        if let Some(i) = self.open.iter().position(|x| *x == p) { self.open.remove(i); } else { self.open.push(p); }
    }
    fn is_open(&self, p: Panel) -> bool { self.open.contains(&p) }
}

#[path = "tools.rs"]
mod tools;

#[path = "editing.rs"]
mod editing;

#[path = "visual.rs"]
mod visual;

#[path = "spell.rs"]
mod spell;

#[path = "automation.rs"]
mod automation;

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

fn fmt_cps(e: &Event) -> String {
    let c = ops::cps(e);
    if c.is_finite() { format!("{:.1}", c) } else { "∞".into() }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}

impl App {
    /// One UI frame (everything `eframe::App::update` does).
    pub fn frame(&mut self, ctx: &egui::Context) {
        self.poll_worker();
        self.poll_sync();
        self.watch_scripts();
        self.handle_shortcuts(ctx);
        if self.settings.autosave_secs > 0 && self.dirty && self.last_autosave.elapsed().as_secs() >= self.settings.autosave_secs {
            self.last_autosave = std::time::Instant::now();
            let dir = crate::settings::config_dir().join("autosave");
            let _ = std::fs::create_dir_all(&dir);
            let name = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
            let _ = std::fs::write(dir.join(format!("{}.autosave.ass", name)), ass::write(&self.doc));
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.quit_confirm {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_confirm = true;
        }

        let title = format!("{}{} — Yuzu", self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| t("Untitled").into()), if self.dirty { " *" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        self.media_tick(ctx);
        self.menu_bar(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.last_status);
                if let Some(b) = &self.busy {
                    ui.separator();
                    ui.spinner();
                    ui.label(tf!("Running: {}", b));
                    ui.add(egui::ProgressBar::new(self.progress).desired_width(160.0));
                    if ui.button(t("Cancel")).clicked() { self.worker.cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let d = &self.doc;
                    let (x, y) = d.res();
                    ui.label(tf!("{} lines · {}×{} · {} fps · {} · QC {}", d.events.len(), x, y, format!("{:.3}", fps_of(&self.settings).as_f64()), self.encoding, self.qc_issues.len()));
                });
            });
        });
        egui::TopBottomPanel::bottom("editor").resizable(true).min_height(190.0).default_height(240.0).show(ctx, |ui| self.editor_panel(ui));
        if self.media.video.is_some() || self.media.audio.is_some() {
            egui::TopBottomPanel::bottom("wave").resizable(true).min_height(90.0).default_height(self.wave_height).show(ctx, |ui| {
                self.wave_height = ui.available_height();
                self.wave_panel(ui);
            });
        }
        if self.show_preview {
            egui::SidePanel::right("preview").resizable(true).default_width(420.0).min_width(260.0).show(ctx, |ui| {
                if self.media.has_video() {
                    self.video_panel(ui);
                } else {
                    ui.heading(t("Preview"));
                    let sz = egui::Vec2::new(ui.available_width(), (ui.available_height() - 6.0).min(ui.available_width() * 0.75));
                    preview::show(ui, &self.doc, self.doc.events.get(self.active), sz);
                }
                ui.separator();
                self.info_box(ui);
            });
        }
        egui::CentralPanel::default().show(ctx, |ui| self.grid(ui));
        self.windows(ctx);
        self.palette_window(ctx);
        if let Some(mut p) = self.prefs.take() {
            match prefs::show(ctx, &mut p) {
                prefs::Action::None => self.prefs = Some(p),
                prefs::Action::Close => {}
                prefs::Action::Save => self.apply_prefs(ctx, p),
            }
        }
        if self.quit_confirm {
            egui::Window::new(t("Unsaved changes")).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(t("Save changes before closing?"));
                ui.horizontal(|ui| {
                    if ui.button(t("Save")).clicked() { self.save(); if !self.dirty { self.quit_confirm = false; self.dirty = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); } }
                    if ui.button(t("Discard")).clicked() { self.dirty = false; self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    if ui.button(t("Cancel")).clicked() { self.quit_confirm = false; }
                });
            });
        }
    }
}

impl App {
    /// Keys go through the keymap (see `commands`). Bindings with more modifiers are tried
    /// first so Ctrl+Shift+Z is not taken by Ctrl+Z; single keys wait while a text box is edited.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.palette.is_some() || self.prefs.as_ref().map(|p| p.recording.is_some()).unwrap_or(false) { return; }
        let typing = ctx.wants_keyboard_input();
        self.clipboard_events(ctx, typing);
        let mut binds: Vec<(KeyboardShortcut, &'static str, When)> = Vec::new();
        for (id, keys) in &self.keymap.bindings {
            let Some(cmd) = commands::find(id) else { continue };
            for k in keys { binds.push((*k, *id, commands::when_of(cmd, k))); }
        }
        let weight = |k: &KeyboardShortcut| k.modifiers.shift as u8 + k.modifiers.alt as u8 + (k.modifiers.command || k.modifiers.ctrl) as u8;
        binds.sort_by_key(|(k, _, _)| std::cmp::Reverse(weight(k)));
        let mut fired = Vec::new();
        for (k, id, when) in binds {
            if when == When::NotTyping && typing { continue; }
            // Space marks syllables while karaoke tapping is running
            if self.tap.is_some() && k.logical_key == Key::Space && k.modifiers.is_none() { continue; }
            if ctx.input_mut(|i| i.consume_shortcut(&k)) { fired.push(id); }
        }
        for id in fired { self.run_command(ctx, id); }
    }

    fn show_panel(&mut self, p: Panel) {
        if !self.is_open(p) { self.open.push(p); }
    }

    fn new_document(&mut self) {
        self.doc = Script::new();
        self.touch();
        self.doc.events.push(Event::new(0, 2000, "Default", ""));
        self.path = None; self.undo.clear(); self.redo.clear(); self.dirty = false; self.select_only(0);
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter(t("Subtitles"), &["ass", "ssa", "srt", "vtt", "sub", "sbv", "lrc", "ttml", "dfxp", "xml", "smi", "sami", "json", "csv", "txt"]).pick_file() { self.open_path(p); }
    }

    /// Load a reference subtitle (command line `--reference`).
    pub fn open_reference(&mut self, p: PathBuf) { self.set_reference(p); }

    fn load_reference_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter(t("Subtitles"), &["ass", "ssa", "srt", "vtt", "sbv", "ttml", "dfxp", "sub"]).pick_file() { self.set_reference(p); }
    }

    fn for_selected_text(&mut self, f: impl Fn(&str) -> String) {
        self.push_undo();
        for i in self.sel_vec() { let t = f(&self.doc.events[i].text); self.doc.events[i].text = t; }
        self.run_qc();
    }

    fn step_line(&mut self, delta: i64) {
        let n = (self.active as i64 + delta).clamp(0, self.doc.events.len().saturating_sub(1) as i64) as usize;
        self.select_only(n);
        self.scroll_to = Some(n);
    }

    /// Run a command by id (menus, shortcuts and the palette all end up here).
    pub(crate) fn run_command(&mut self, ctx: &egui::Context, id: &str) {
        match id {
            "new" => self.new_document(),
            "open" => self.open_dialog(),
            "save" => self.save(),
            "save_as" => self.save_as(),
            "export_srt" => if let Some(p) = rfd::FileDialog::new().add_filter("SubRip", &["srt"]).set_file_name("subtitles.srt").save_file() { self.save_to(p); },
            "export_vtt" => if let Some(p) = rfd::FileDialog::new().add_filter("WebVTT", &["vtt"]).set_file_name("subtitles.vtt").save_file() { self.save_to(p); },
            "load_reference" => self.load_reference_dialog(),
            "close_reference" => self.close_reference(),
            "reference" => self.show_panel(Panel::Reference),
            "compare" => self.show_panel(Panel::Compare),
            "sync_speech" => { self.osi.sync_use_ref = false; self.osi.sync_plan = None; self.show_panel(Panel::Sync); }
            "sync_ref" => { self.osi.sync_use_ref = true; self.osi.sync_plan = None; self.show_panel(Panel::Sync); }
            "glossary" => { self.autoload_glossary(); self.show_panel(Panel::Glossary); }
            "spell" => self.show_panel(Panel::Spell),
            "toggle_ref_column" => self.osi.show_ref_column = !self.osi.show_ref_column,
            "properties" => self.show_panel(Panel::Props),
            "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "undo" => self.do_undo(),
            "redo" => self.do_redo(),
            "insert_after" => self.insert_after_active(),
            "insert_before" => self.insert_before_active(),
            "duplicate" => self.duplicate_selected(),
            "join" => self.merge_selected(),
            "delete" => self.delete_selected(),
            "find" => self.show_panel(Panel::Find),
            "copy_lines" => self.copy_lines(),
            "cut_lines" => self.cut_lines(),
            // the clipboard text arrives as a paste event in the next frame
            "paste_lines" => { self.edit.pending_paste = Some(false); ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste); }
            "paste_over" => { self.edit.pending_paste = Some(true); ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste); }
            "select_lines" => self.show_panel(Panel::SelectLines),
            "multi_replace" => { self.load_rules(); self.show_panel(Panel::MultiReplace); }
            "translate_assist" => { self.start_translation(); self.show_panel(Panel::Translate); }
            "style_assist" => { self.edit.sa_filter.clear(); self.edit.sa_focus = true; self.show_panel(Panel::StyleAssist); }
            "select_all" => self.selection = (0..self.doc.events.len()).collect(),
            "prev_line" | "prev_line_key" => self.step_line(-1),
            "next_line" | "next_line_key" => self.step_line(1),
            "commit_next" | "commit" => self.next_line_for_timing(),
            "palette" => self.palette = Some(Palette::new()),
            "prefs" => self.prefs = Some(Prefs::new(&self.settings, &self.keymap)),
            "shortcuts" => { let mut p = Prefs::new(&self.settings, &self.keymap); p.tab = 2; self.prefs = Some(p); }
            "shift" => self.toggle(Panel::Shift),
            "snap_frames" => {
                self.push_undo();
                let (idx, fps) = (self.sel_vec(), self.fps());
                ops::snap_to_frames(&mut self.doc, &idx, fps);
                self.buf_key = (usize::MAX, 0, 0);
            }
            "post_processor" => self.toggle(Panel::Post),
            "snap_keyframes_sel" => {
                self.push_undo();
                let kf = self.media.keyframe_line_times(self.fps());
                let pp = ops::PostProc { lead_in: 0, lead_out: 0, keyframe_snap_before: 300, keyframe_snap_after: 300, ..Default::default() };
                let idx = self.sel_vec();
                ops::post_process(&mut self.doc, &idx, &pp, &kf);
                self.buf_key = (usize::MAX, 0, 0);
            }
            "sort_start" => { self.push_undo(); self.doc.sort_by_start(); }
            "sort_end" => { self.push_undo(); self.doc.sort_by_end(); }
            "sort_style" => { self.push_undo(); self.doc.sort_by_style(); }
            "play_line" | "play_line_f" => self.play_part(PlayPart::Line),
            "play_before" => self.play_part(PlayPart::Before),
            "play_after" => self.play_part(PlayPart::After),
            "play_first" => self.play_part(PlayPart::First),
            "play_last" => self.play_part(PlayPart::Last),
            "play_to_end" => self.play_part(PlayPart::ToEnd),
            "play_end_f" => self.play_active_line(-500, 500),
            "stop" => self.media.stop(),
            "play_pause" | "play_pause_key" => self.toggle_play(),
            "speech_lines" => self.speech_lines(),
            "set_start" | "start_at_playhead" => self.set_line_time_from_pos(true),
            "set_end" | "end_at_playhead" => self.set_line_time_from_pos(false),
            "scroll_left" | "scroll_right" => {
                let span = self.wave_rect.map(|r| r.width() as f64).unwrap_or(900.0) * self.media.ms_per_px;
                self.media.view_start += if id == "scroll_left" { -span * 0.25 } else { span * 0.25 };
            }
            "styles" => self.toggle(Panel::Styles),
            "qc" => { self.run_qc(); self.show_panel(Panel::Qc); }
            "resample" => self.toggle(Panel::Resample),
            "fix" => self.toggle(Panel::Fix),
            "netflix_qc" => {
                self.qc_issues = asscore::fixes::qc_netflix(&self.doc, &asscore::fixes::NetflixConfig::default());
                self.qc_by_line.clear();
                for i in &self.qc_issues { let e = self.qc_by_line.entry(i.index).or_insert(0); *e = (*e).max(2); }
                self.show_panel(Panel::Qc);
            }
            "fonts" => self.toggle(Panel::Fonts),
            "karaoke" => self.toggle(Panel::Karaoke),
            "clean_tags" => self.for_selected_text(tags::clean_tags),
            "strip_tags" => self.for_selected_text(tags::strip_tags),
            "strip_karaoke" => self.for_selected_text(tags::strip_karaoke),
            "dedupe" => {
                self.push_undo();
                let n = ops::dedupe(&mut self.doc);
                self.last_status = tf!("Removed {} duplicate line(s)", n);
                self.clamp_sel();
            }
            "trim" => {
                self.push_undo();
                for e in self.doc.events.iter_mut() { e.text = e.text.trim().replace("  ", " "); }
            }
            "reload_scripts" => self.reload_scripts(),
            "add_script_folder" => if let Some(d) = rfd::FileDialog::new().pick_folder() { self.settings.script_dirs.push(d); self.settings.save(); self.reload_scripts(); },
            "console" => self.toggle(Panel::Console),
            "automation_manager" => self.show_panel(Panel::Scripts),
            "open_video" => if let Some(p) = rfd::FileDialog::new().add_filter(t("Video / audio"), &["mkv", "mp4", "avi", "webm", "m2ts", "ts", "mov", "m4v", "flv", "mka", "mp3", "aac", "flac", "wav", "ogg", "opus", "m4a"]).pick_file() { self.media.open(p); },
            "open_audio" => if let Some(p) = rfd::FileDialog::new().add_filter(t("Audio / video"), &["mka", "mp3", "aac", "flac", "wav", "ogg", "opus", "m4a", "ac3", "mkv", "mp4"]).pick_file() { self.media.open_audio(p); },
            "close_media" => self.media.close(),
            "frame_prev" => self.step_frame(-1),
            "frame_next" => self.step_frame(1),
            "seek_back" => self.seek_rel(-1000),
            "seek_fwd" => self.seek_rel(1000),
            "goto_start" => { let t = self.doc.events.get(self.active).map(|e| e.start).unwrap_or(0); self.goto_ms(t); }
            "goto_end" => { let t = self.doc.events.get(self.active).map(|e| e.end).unwrap_or(0); self.goto_ms(t); }
            "toggle_spectrum" => self.media.show_spectrum = !self.media.show_spectrum,
            "toggle_snap" => self.media.snap_keyframes = !self.media.snap_keyframes,
            "toggle_follow" => self.media.follow = !self.media.follow,
            "toggle_preview" => self.show_preview = !self.show_preview,
            "toggle_dark" => {
                self.settings.dark = !self.settings.dark;
                ctx.set_visuals(if self.settings.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
                self.settings.save();
            }
            "about" => self.toggle(Panel::About),
            _ => {}
        }
    }

    /// "Commit": go to the next line; at the last line add a new one starting where this ends.
    fn next_line_for_timing(&mut self) {
        let n = self.active + 1;
        if n >= self.doc.events.len() { self.insert_after_active(); } else { self.select_only(n); self.scroll_to = Some(n); }
    }

    /// "Label (key, key)" for tooltips, following the user's bindings.
    fn cmd_hint(&self, id: &str) -> String {
        let Some(c) = commands::find(id) else { return String::new() };
        let keys: Vec<String> = self.keymap.get(id).iter().map(commands::display).collect();
        if keys.is_empty() { t(c.label).to_string() } else { format!("{} ({})", t(c.label), keys.join(", ")) }
    }

    /// Menu entry for a command: translated label with its current shortcut on the right.
    fn menu_cmd(&mut self, ui: &mut egui::Ui, id: &'static str) { self.menu_cmd_if(ui, id, true); }

    fn menu_cmd_if(&mut self, ui: &mut egui::Ui, id: &'static str, enabled: bool) {
        let Some(c) = commands::find(id) else { return };
        let mut b = egui::Button::new(t(c.label));
        if let Some(k) = self.keymap.label(id) { b = b.shortcut_text(k); }
        if ui.add_enabled(enabled, b).clicked() {
            ui.close();
            let ctx = ui.ctx().clone();
            self.run_command(&ctx, id);
        }
    }

    /// Menu entry for an on/off command.
    fn menu_toggle(&mut self, ui: &mut egui::Ui, id: &'static str, on: bool) {
        let Some(c) = commands::find(id) else { return };
        let mut v = on;
        if ui.checkbox(&mut v, t(c.label)).clicked() {
            let ctx = ui.ctx().clone();
            self.run_command(&ctx, id);
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(t("File"), |ui| {
                    self.menu_cmd(ui, "new");
                    self.menu_cmd(ui, "open");
                    ui.menu_button(t("Open recent"), |ui| {
                        for r in self.settings.recent.clone().into_iter().take(10) {
                            if ui.button(r.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()).on_hover_text(r.display().to_string()).clicked() { ui.close(); self.open_path(r); }
                        }
                    });
                    ui.separator();
                    self.menu_cmd(ui, "save");
                    self.menu_cmd(ui, "save_as");
                    ui.menu_button(t("Export as…"), |ui| {
                        for f in asscore::formats::Format::ALL {
                            if f == asscore::formats::Format::Ass { continue; }
                            if ui.button(f.name()).clicked() {
                                ui.close();
                                if let Some(p) = rfd::FileDialog::new().set_file_name(format!("subtitles.{}", f.extension())).save_file() {
                                    let out = asscore::formats::save_any(f, &self.doc, &asscore::formats::FormatOptions { fps: fps_of(&self.settings).as_f64(), ..Default::default() });
                                    let _ = std::fs::write(&p, asscore::encoding::encode(&out, &self.encoding, self.bom));
                                    self.last_status = tf!("Exported {}", p.display());
                                }
                            }
                        }
                    });
                    self.menu_cmd(ui, "export_srt");
                    self.menu_cmd(ui, "export_vtt");
                    ui.separator();
                    ui.menu_button(tf!("Save encoding: {}", self.encoding), |ui| {
                        for (label, name) in asscore::encoding::COMMON_ENCODINGS {
                            if ui.radio(self.encoding.eq_ignore_ascii_case(label), *name).clicked() { self.encoding = label.to_string(); ui.close(); }
                        }
                        ui.checkbox(&mut self.bom, t("Write byte-order mark"));
                    });
                    self.menu_cmd(ui, "load_reference");
                    if self.osi.reference.is_some() { self.menu_cmd(ui, "close_reference"); }
                    self.menu_cmd(ui, "properties");
                    ui.separator();
                    self.menu_cmd(ui, "quit");
                });
                ui.menu_button(t("Edit"), |ui| {
                    let (u, r) = (!self.undo.is_empty(), !self.redo.is_empty());
                    self.menu_cmd_if(ui, "undo", u);
                    self.menu_cmd_if(ui, "redo", r);
                    ui.separator();
                    for id in ["insert_after", "insert_before", "duplicate", "join", "delete"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    for id in ["copy_lines", "cut_lines", "paste_lines", "paste_over"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    for id in ["find", "multi_replace", "select_lines", "select_all"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    self.menu_cmd(ui, "palette");
                    self.menu_cmd(ui, "prefs");
                });
                ui.menu_button(t("Timing"), |ui| {
                    for id in ["shift", "snap_frames", "snap_keyframes_sel", "post_processor"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    for id in ["sort_start", "sort_end", "sort_style"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    ui.menu_button(t("Frame rate"), |ui| {
                        for (n, d, l) in [(24000, 1001, "23.976"), (24, 1, "24"), (25, 1, "25"), (30000, 1001, "29.970"), (30, 1, "30"), (60000, 1001, "59.940"), (60, 1, "60")] {
                            if ui.radio(self.settings.fps == (n, d), l).clicked() { self.settings.fps = (n, d); self.settings.save(); ui.close(); }
                        }
                    });
                    ui.menu_button(t("Timing keys"), |ui| {
                        for c in commands::COMMANDS.iter().filter(|c| c.group == "Timing keys") { self.menu_cmd(ui, c.id); }
                    });
                });
                ui.menu_button(t("Tools"), |ui| {
                    for id in ["styles", "style_assist", "qc", "netflix_qc", "fix", "spell", "resample", "fonts", "karaoke"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    for id in ["clean_tags", "strip_tags", "strip_karaoke", "dedupe", "trim"] { self.menu_cmd(ui, id); }
                });
                ui.menu_button(t("Translation"), |ui| {
                    self.menu_cmd(ui, "translate_assist");
                    ui.separator();
                    self.menu_cmd(ui, "load_reference");
                    let has_ref = self.osi.reference.is_some();
                    self.menu_cmd_if(ui, "reference", has_ref);
                    let on = self.osi.show_ref_column;
                    if has_ref { self.menu_toggle(ui, "toggle_ref_column", on); }
                    ui.separator();
                    for id in ["compare", "sync_ref", "sync_speech", "glossary"] { self.menu_cmd(ui, id); }
                });
                ui.menu_button(t("Automation"), |ui| {
                    // the menu is open: ask which macros apply to the current selection
                    self.request_macro_states();
                    if self.scripts.is_empty() { ui.label(t("No scripts loaded")); }
                    let scripts: Vec<(usize, String, Vec<(usize, String, String)>, Option<String>)> = self.scripts.iter().enumerate()
                        .map(|(i, s)| (i, s.name.clone(), s.macros.iter().map(|m| (m.index, m.name.clone(), m.description.clone())).collect(), s.error.clone())).collect();
                    let mut groups: Vec<(String, Vec<(usize, usize, String, String)>)> = Vec::new();
                    for (si, name, macros, err) in scripts {
                        if let Some(e) = err { ui.add_enabled(false, egui::Button::new(format!("⚠ {}", name))).on_disabled_hover_text(e.lines().next().unwrap_or("").to_string()); continue; }
                        for (mi, mn, md) in macros {
                            let (g, label) = match mn.split_once('/') { Some((g, l)) => (g.to_string(), l.to_string()), None => (String::new(), mn.clone()) };
                            match groups.iter_mut().find(|(n, _)| *n == g) { Some(e) => e.1.push((si, mi, label, md)), None => groups.push((g, vec![(si, mi, label, md)])) }
                        }
                    }
                    let mut run: Option<(usize, usize, String)> = None;
                    let busy = self.busy.is_some();
                    // greyed when the macro's validation says no; checked when is_active says yes
                    let item = |app: &App, ui: &mut egui::Ui, si: usize, mi: usize, label: &str, md: &str| -> bool {
                        let st = app.macro_state(si, mi);
                        let enabled = !busy && st.map(|s| s.enabled).unwrap_or(true);
                        let text = if st.and_then(|s| s.checked) == Some(true) { format!("✔ {}", label) } else { label.to_string() };
                        ui.add_enabled(enabled, egui::Button::new(text)).on_hover_text(md).on_disabled_hover_text(t("Not available for the current selection")).clicked()
                    };
                    for (g, items) in groups.iter().filter(|(g, _)| !g.is_empty()) {
                        ui.menu_button(g, |ui| {
                            for (si, mi, label, md) in items {
                                if item(self, ui, *si, *mi, label, md) { ui.close(); run = Some((*si, *mi, format!("{}/{}", g, label))); }
                            }
                        });
                    }
                    for (_, items) in groups.iter().filter(|(g, _)| g.is_empty()) {
                        for (si, mi, label, md) in items {
                            if item(self, ui, *si, *mi, label, md) { ui.close(); run = Some((*si, *mi, label.clone())); }
                        }
                    }
                    if let Some((si, mi, n)) = run { self.run_macro(si, mi, &n); }
                    ui.separator();
                    for id in ["automation_manager", "reload_scripts", "add_script_folder", "console"] { self.menu_cmd(ui, id); }
                });
                ui.menu_button(t("Video"), |ui| {
                    let has_media = self.media.video.is_some() || self.media.audio.is_some();
                    self.menu_cmd(ui, "open_video");
                    self.menu_cmd(ui, "open_audio");
                    self.menu_cmd_if(ui, "close_media", has_media);
                    ui.separator();
                    for id in ["speech_lines", "set_start", "set_end", "play_line_f", "play_end_f", "play_pause", "frame_prev", "frame_next", "seek_back", "seek_fwd"] { self.menu_cmd(ui, id); }
                    ui.separator();
                    self.menu_cmd(ui, "goto_start");
                    self.menu_cmd(ui, "goto_end");
                    ui.separator();
                    let (sp, sn, fo) = (self.media.show_spectrum, self.media.snap_keyframes, self.media.follow);
                    self.menu_toggle(ui, "toggle_spectrum", sp);
                    self.menu_toggle(ui, "toggle_snap", sn);
                    self.menu_toggle(ui, "toggle_follow", fo);
                });
                ui.menu_button(t("View"), |ui| {
                    let (pv, dk) = (self.show_preview, self.settings.dark);
                    self.menu_toggle(ui, "toggle_preview", pv);
                    self.menu_toggle(ui, "toggle_dark", dk);
                });
                ui.menu_button(t("Help"), |ui| {
                    self.menu_cmd(ui, "shortcuts");
                    self.menu_cmd(ui, "about");
                });
            });
        });
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        let n = self.doc.events.len();
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        // cell texts must not be selectable, or they swallow the click meant for the row
        ui.style_mut().interaction.selectable_labels = false;
        let show_ref = self.osi.reference.is_some() && self.osi.show_ref_column;
        let max_cpl = self.qc_cfg.max_cpl;
        let mut tb = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(46.0))
            .column(Column::exact(34.0))
            .column(Column::exact(86.0))
            .column(Column::exact(86.0))
            .column(Column::exact(54.0))
            .column(Column::exact(44.0))
            .column(Column::exact(56.0))
            .column(Column::initial(100.0).clip(true))
            .column(Column::initial(80.0).clip(true));
        if show_ref { tb = tb.column(Column::initial(280.0).clip(true)); }
        tb = tb.column(Column::remainder().clip(true));
        if let Some(t) = self.scroll_to.take() { tb = tb.scroll_to_row(t, Some(egui::Align::Center)); }
        tb.header(22.0, |mut h| {
            for head in ["#", "L", "Start", "End", "Dur", "CPS", "CPL", "Style", "Actor"] { h.col(|ui| { ui.strong(t(head)); }); }
            if show_ref { h.col(|ui| { ui.strong(tf!("Reference ({} s)", format!("{:+.2}", self.osi.ref_offset_ms as f64 / 1000.0))); }); }
            h.col(|ui| { ui.strong(t("Text")); });
        })
        .body(|body| {
            body.rows(22.0, n, |mut row| {
                let i = row.index();
                let e = &self.doc.events[i];
                row.set_selected(self.selection.contains(&i));
                let issue = self.qc_by_line.get(&i).copied().unwrap_or(0);
                let dim = e.comment;
                let tint = |s: String| -> RichText {
                    let mut t = RichText::new(s);
                    if dim { t = t.color(Color32::GRAY).italics(); }
                    t
                };
                row.col(|ui| { ui.label(tint(format!("{}", i + 1))); });
                row.col(|ui| { ui.label(tint(if e.comment { "#".into() } else { e.layer.to_string() })); });
                row.col(|ui| { ui.label(tint(time::format_ass(e.start))); });
                row.col(|ui| { ui.label(tint(time::format_ass(e.end))); });
                row.col(|ui| { ui.label(tint(format!("{:.2}", e.duration() as f64 / 1000.0))); });
                row.col(|ui| {
                    let c = ops::cps(e);
                    let mut t = RichText::new(fmt_cps(e));
                    if c > self.qc_cfg.max_cps { t = t.color(Color32::from_rgb(240, 110, 90)); }
                    ui.label(t);
                });
                row.col(|ui| {
                    // Osi's rule: one line over the limit warns, a part of a split line over it is an error
                    let parts = ops::cpl_parts(&e.text);
                    let mut t = RichText::new(ops::cpl_label(&parts));
                    match ops::cpl_status(&parts, max_cpl) {
                        ops::CplStatus::Warn => t = t.color(Color32::from_rgb(235, 190, 80)),
                        ops::CplStatus::Error => t = t.color(Color32::from_rgb(255, 90, 90)).strong(),
                        ops::CplStatus::Ok => if dim { t = t.color(Color32::GRAY) },
                    }
                    ui.label(t);
                });
                row.col(|ui| { ui.label(tint(e.style.clone())); });
                row.col(|ui| { ui.label(tint(e.actor.clone())); });
                if show_ref {
                    row.col(|ui| { if let Some(r) = self.reference_text(i) { ui.label(RichText::new(r).color(Color32::from_rgb(150, 170, 200))); } });
                }
                row.col(|ui| {
                    // the extradata marker ({=1}) is not shown, as in Aegisub
                    let body = asscore::extradata::split_marker(&e.text).1;
                    let txt = if self.show_text_stripped { tags::display_text(body).replace('\n', " ⏎ ") } else { body.to_string() };
                    let mut t = RichText::new(txt);
                    if issue >= 3 { t = t.color(Color32::from_rgb(255, 90, 90)); } else if issue == 2 { t = t.color(Color32::from_rgb(235, 190, 80)); } else if dim { t = t.color(Color32::GRAY); }
                    ui.label(t);
                });
                let resp = row.response();
                if resp.clicked() { clicked = Some((i, resp.ctx.input(|i| i.modifiers))); }
            });
        });
        if let Some((i, m)) = clicked {
            if m.shift {
                let (a, b) = (self.anchor.min(i), self.anchor.max(i));
                if !m.command { self.selection.clear(); }
                for k in a..=b { self.selection.insert(k); }
                self.active = i;
            } else if m.command {
                if !self.selection.remove(&i) { self.selection.insert(i); }
                self.active = i;
                self.anchor = i;
            } else {
                self.select_only(i);
            }
        }
    }

    fn info_box(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_text_stripped, t("Show lines without tags"));
        if let Some(e) = self.doc.events.get(self.active) {
            ui.label(tf!("{} chars · {} CPS · {} ms", tags::visible_len(&e.text), format!("{:.1}", ops::cps(e)), e.duration()));
            let karaoke = tags::karaoke_syllables(&e.text);
            if karaoke.iter().any(|s| s.duration_cs > 0) {
                ui.label(tf!("Karaoke: {} syllables, {}s", karaoke.len(), format!("{:.2}", karaoke.iter().map(|s| s.duration_cs).sum::<i64>() as f64 / 100.0)));
            }
        }
        for is in self.qc_issues.iter().filter(|i| i.index == self.active) {
            let c = match is.severity { ops::Severity::Error => Color32::from_rgb(255, 90, 90), ops::Severity::Warning => Color32::from_rgb(235, 190, 80), _ => Color32::GRAY };
            ui.colored_label(c, i18n::message(&is.message));
        }
    }

    fn editor_panel(&mut self, ui: &mut egui::Ui) {
        if self.doc.events.is_empty() { return; }
        let i = self.active.min(self.doc.events.len() - 1);
        // refresh time buffers
        let (s, e) = (self.doc.events[i].start, self.doc.events[i].end);
        if self.buf_key != (i, s, e) {
            self.buf_start = time::format_ass(s);
            self.buf_end = time::format_ass(e);
            self.buf_dur = format!("{:.2}", (e - s) as f64 / 1000.0);
            self.buf_key = (i, s, e);
        }
        let styles: Vec<String> = self.doc.styles.iter().map(|s| s.name.clone()).collect();
        let multi = self.selection.len() > 1;
        ui.horizontal(|ui| {
            let mut comment = self.doc.events[i].comment;
            if ui.checkbox(&mut comment, t("Comment")).changed() {
                self.begin_edit(0);
                for k in self.sel_vec() { self.doc.events[k].comment = comment; }
            }
            ui.label(t("Layer"));
            let mut layer = self.doc.events[i].layer;
            if ui.add(egui::DragValue::new(&mut layer).range(0..=999)).changed() {
                self.begin_edit(1_000_000 + i as u64);
                for k in self.sel_vec() { self.doc.events[k].layer = layer; }
            }
            ui.label(t("Style"));
            let mut cur = self.doc.events[i].style.clone();
            egui::ComboBox::from_id_salt("estyle").selected_text(&cur).show_ui(ui, |ui| {
                for s in &styles { ui.selectable_value(&mut cur, s.clone(), s); }
            });
            if cur != self.doc.events[i].style {
                self.begin_edit(0);
                for k in self.sel_vec() { self.doc.events[k].style = cur.clone(); }
            }
            ui.label(t("Actor"));
            let mut actor = self.doc.events[i].actor.clone();
            if ui.add(egui::TextEdit::singleline(&mut actor).desired_width(90.0)).changed() {
                self.begin_edit(2_000_000 + i as u64);
                for k in self.sel_vec() { self.doc.events[k].actor = actor.clone(); }
            }
            ui.label(t("Effect"));
            let mut eff = self.doc.events[i].effect.clone();
            if ui.add(egui::TextEdit::singleline(&mut eff).desired_width(90.0)).changed() {
                self.begin_edit(3_000_000 + i as u64);
                for k in self.sel_vec() { self.doc.events[k].effect = eff.clone(); }
            }
            ui.label(t("Margins"));
            for (lbl, which) in [("L", 0), ("R", 1), ("V", 2)] {
                let mut v = match which { 0 => self.doc.events[i].margin_l, 1 => self.doc.events[i].margin_r, _ => self.doc.events[i].margin_v };
                if ui.add(egui::DragValue::new(&mut v).range(0..=9999).prefix(format!("{} ", lbl))).changed() {
                    self.begin_edit(4_000_000 + which as u64 + (i as u64) * 10);
                    for k in self.sel_vec() { let ev = &mut self.doc.events[k]; match which { 0 => ev.margin_l = v, 1 => ev.margin_r = v, _ => ev.margin_v = v } }
                }
            }
            if multi { ui.weak(tf!("({} lines selected)", self.selection.len())); }
        });
        ui.horizontal(|ui| {
            let mut changed_time = false;
            let w = 90.0;
            ui.label(t("Start"));
            let r1 = ui.add(egui::TextEdit::singleline(&mut self.buf_start).desired_width(w));
            ui.label(t("End"));
            let r2 = ui.add(egui::TextEdit::singleline(&mut self.buf_end).desired_width(w));
            ui.label(t("Duration"));
            let r3 = ui.add(egui::TextEdit::singleline(&mut self.buf_dur).desired_width(60.0));
            if r1.lost_focus() { if let Some(t) = time::parse_ass(&self.buf_start) { self.begin_edit(0); self.doc.events[i].start = t.max(0); changed_time = true; } }
            if r2.lost_focus() { if let Some(t) = time::parse_ass(&self.buf_end) { self.begin_edit(0); self.doc.events[i].end = t.max(0); changed_time = true; } }
            if r3.lost_focus() { if let Ok(d) = self.buf_dur.trim().parse::<f64>() { self.begin_edit(0); let st = self.doc.events[i].start; self.doc.events[i].end = st + (d * 1000.0).round() as i64; changed_time = true; } }
            if changed_time { self.buf_key = (usize::MAX, 0, 0); }
            ui.separator();
            for (label, hint, open, close) in [("B", "Bold", "{\\b1}", "{\\b0}"), ("I", "Italic", "{\\i1}", "{\\i0}"), ("U", "Underline", "{\\u1}", "{\\u0}"), ("S", "Strikeout", "{\\s1}", "{\\s0}")] {
                if ui.button(RichText::new(label).strong()).on_hover_text(t(hint)).clicked() { self.wrap_text(open, close); }
            }
            if ui.button("\\N").on_hover_text(t("Insert line break")).clicked() { self.wrap_text("\\N", ""); }
            if ui.button("{\\an8}").on_hover_text(t("Top-center alignment")).clicked() { self.prefix_tag("{\\an8}"); }
            ui.separator();
            if ui.button(t("Split")).on_hover_text(t("Split line at cursor, dividing time proportionally")).clicked() { self.split_at_cursor(); }
        });
        if let Some(r) = self.reference_text(i) {
            ui.horizontal(|ui| { ui.weak(t("Reference:")); ui.label(RichText::new(r).italics()); });
        }
        // text box
        let id = egui::Id::new("text_edit");
        // the extradata marker stays out of the text box and is put back on every edit
        let full = self.doc.events[i].text.clone();
        let (ids, body) = asscore::extradata::split_marker(&full);
        let mlen = full.chars().count() - body.chars().count();
        let mut text = body.to_string();
        let out = egui::TextEdit::multiline(&mut text).id(id).desired_width(f32::INFINITY).desired_rows(4).font(egui::TextStyle::Monospace).show(ui);
        if let Some(r) = out.cursor_range { let (a, b) = (r.primary.index + mlen, r.secondary.index + mlen); self.text_cursor = Some((a.min(b), a.max(b))); }
        if out.response.changed() {
            self.begin_edit(5_000_000 + i as u64);
            let text = asscore::extradata::with_marker(&ids, &text);
            for k in self.sel_vec().into_iter().filter(|k| !multi || *k == i) { self.doc.events[k].text = text.clone(); }
            self.run_qc_line(i);
        }
    }

    fn run_qc_line(&mut self, _i: usize) {
        // cheap incremental: only recompute when the doc is small enough to be instant
        if self.doc.events.len() < 3000 { self.run_qc(); }
    }

    fn wrap_text(&mut self, open: &str, close: &str) {
        let i = self.active;
        if i >= self.doc.events.len() { return; }
        self.begin_edit(0);
        let (a, b) = self.text_cursor.unwrap_or((self.doc.events[i].text.chars().count(), self.doc.events[i].text.chars().count()));
        let chars: Vec<char> = self.doc.events[i].text.chars().collect();
        let (a, b) = (a.min(chars.len()), b.min(chars.len()));
        let mut s: String = chars[..a].iter().collect();
        s.push_str(open);
        s.extend(&chars[a..b]);
        s.push_str(close);
        s.extend(&chars[b..]);
        self.doc.events[i].text = s;
        self.ctx.request_repaint();
    }

    fn prefix_tag(&mut self, tag: &str) {
        let i = self.active;
        if i >= self.doc.events.len() { return; }
        self.begin_edit(0);
        let full = self.doc.events[i].text.clone();
        let (ids, t) = asscore::extradata::split_marker(&full);
        let body = if t.starts_with('{') && !tag.is_empty() {
            // merge into first block
            let inner = &tag[1..tag.len() - 1];
            format!("{{{}{}", inner, &t[1..])
        } else { format!("{}{}", tag, t) };
        self.doc.events[i].text = asscore::extradata::with_marker(&ids, &body);
    }

    fn split_at_cursor(&mut self) {
        let i = self.active;
        if i >= self.doc.events.len() { return; }
        let e = self.doc.events[i].clone();
        let total = e.text.chars().count().max(1);
        let off = self.text_cursor.map(|c| c.0).unwrap_or(total / 2).min(total);
        let at = e.start + ((e.end - e.start) as f64 * off as f64 / total as f64).round() as i64;
        self.push_undo();
        ops::split_event(&mut self.doc, i, at.clamp(e.start, e.end), off);
        self.select_only(i + 1);
    }

    /// Command palette: commands and Automation macros; runs the choice.
    fn palette_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.palette.take() else { return };
        let mut entries: Vec<palette::Entry> = commands::COMMANDS.iter().map(|c| palette::Entry {
            label: t(c.label).to_string(), group: t(c.group).to_string(), shortcut: self.keymap.label(c.id).unwrap_or_default(), pick: Pick::Command(c.id),
        }).collect();
        for (si, s) in self.scripts.iter().enumerate() {
            for m in &s.macros {
                entries.push(palette::Entry { label: m.name.replace('/', " › "), group: t("Automation macro").to_string(), shortcut: String::new(), pick: Pick::Macro { script: si, macro_idx: m.index, name: m.name.clone() } });
            }
        }
        match palette::show(ctx, &mut st, &entries, &self.settings.palette_recent, self.doc.events.len()) {
            None => self.palette = Some(st),
            Some(None) => {}
            Some(Some(pick)) => {
                palette::remember(&mut self.settings.palette_recent, &pick);
                self.settings.save();
                match pick {
                    Pick::Command(id) => self.run_command(ctx, id),
                    Pick::Macro { script, macro_idx, name } => self.run_macro(script, macro_idx, &name),
                    Pick::Line(n) => { self.select_only(n); self.scroll_to = Some(n); }
                }
            }
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        // Lua dialog
        if let Some((st, _, title)) = self.lua_dialog.as_mut() {
            let t = title.clone();
            if let Some(btn) = luadialog::show(ctx, st, &t) {
                if let Some((st, reply, _)) = self.lua_dialog.take() {
                    let cancel = st.spec.cancel.as_deref() == Some(btn.as_str());
                    let r = if cancel && !st.spec.default_buttons && st.spec.cancel.is_none() { DialogResult::Cancelled }
                        else if cancel { DialogResult::Cancelled }
                        else { DialogResult::Ok { button: btn, values: st.values.into_iter().collect() } };
                    let _ = reply.send(r);
                }
            }
        }
        let panels = self.open.clone();
        for p in panels {
            let mut open = true;
            match p {
                Panel::Find => { egui::Window::new(t("Find & replace")).open(&mut open).default_width(420.0).show(ctx, |ui| self.find_ui(ui)); }
                Panel::Shift => { egui::Window::new(t("Shift times")).open(&mut open).show(ctx, |ui| self.shift_ui(ui)); }
                Panel::Styles => { egui::Window::new(t("Styles manager")).open(&mut open).default_width(720.0).show(ctx, |ui| self.styles_ui(ui)); }
                Panel::Qc => { egui::Window::new(t("Quality check")).open(&mut open).default_width(560.0).default_height(420.0).show(ctx, |ui| self.qc_ui(ui)); }
                Panel::Props => { egui::Window::new(t("Script properties")).open(&mut open).show(ctx, |ui| self.props_ui(ui)); }
                Panel::Console => { egui::Window::new(t("Automation console")).open(&mut open).default_width(640.0).default_height(300.0).show(ctx, |ui| self.console_ui(ui)); }
                Panel::Post => { egui::Window::new(t("Timing post-processor")).open(&mut open).show(ctx, |ui| self.post_ui(ui)); }
                Panel::Resample => { egui::Window::new(t("Resample resolution")).open(&mut open).show(ctx, |ui| self.resample_ui(ui)); }
                Panel::Fix => { egui::Window::new(t("Fix common errors")).open(&mut open).default_width(420.0).show(ctx, |ui| self.fix_ui(ui)); }
                Panel::Reference => { egui::Window::new(t("Reference")).open(&mut open).default_width(560.0).show(ctx, |ui| self.reference_ui(ui)); }
                Panel::Compare => { egui::Window::new(t("Compare with the source")).open(&mut open).default_width(860.0).default_height(560.0).show(ctx, |ui| self.compare_ui(ui)); }
                Panel::Sync => { egui::Window::new(t("Piecewise sync")).open(&mut open).default_width(640.0).show(ctx, |ui| self.sync_ui(ui)); }
                Panel::Spell => { egui::Window::new(t("Spell check")).open(&mut open).default_width(560.0).default_height(440.0).show(ctx, |ui| self.spell_ui(ui)); }
                Panel::Glossary => { egui::Window::new(t("Term check")).open(&mut open).default_width(620.0).show(ctx, |ui| self.glossary_ui(ui)); }
                Panel::SelectLines => { egui::Window::new(t("Select lines")).open(&mut open).default_width(520.0).show(ctx, |ui| self.select_lines_ui(ui)); }
                Panel::MultiReplace => { egui::Window::new(t("Multiple replace")).open(&mut open).default_width(860.0).show(ctx, |ui| self.multi_replace_ui(ui)); }
                Panel::Translate => { egui::Window::new(t("Translation assistant")).open(&mut open).default_width(640.0).show(ctx, |ui| self.translate_ui(ui)); }
                Panel::StyleAssist => { egui::Window::new(t("Styling assistant")).open(&mut open).default_width(460.0).show(ctx, |ui| self.style_assist_ui(ui)); }
                Panel::Scripts => { egui::Window::new(t("Automation manager")).open(&mut open).default_width(760.0).show(ctx, |ui| self.automation_manager_ui(ui)); }
                Panel::Fonts => { egui::Window::new(t("Fonts")).open(&mut open).default_width(620.0).show(ctx, |ui| self.fonts_ui(ui)); }
                Panel::Karaoke => { egui::Window::new(t("Karaoke editor")).open(&mut open).default_width(760.0).show(ctx, |ui| self.karaoke_ui(ui)); }
                Panel::About => { egui::Window::new(t("About Yuzu")).open(&mut open).show(ctx, |ui| {
                    ui.heading(t("Yuzu 柚子"));
                    ui.label(t("Anime subtitle editor — Aegisub-compatible Lua automation, Subtitle Edit style tooling."));
                    ui.label(tf!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.separator();
                    ui.label(tf!("Video / audio: {}", self.media.engine_description()));
                    ui.label(tf!("ffmpeg / ffprobe: {}", if yuzu_media::ffmpeg_available() { t("found") } else { t("not found (needed for waveform, keyframes and export)") }));
                }); }
            }
            if !open { self.open.retain(|x| *x != p); }
        }
        self.paste_over_window(ctx);
    }

    fn find_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("find").num_columns(2).show(ui, |ui| {
            ui.label(t("Find")); ui.text_edit_singleline(&mut self.find); ui.end_row();
            ui.label(t("Replace")); ui.text_edit_singleline(&mut self.repl); ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.find_opts.regex, t("Regex"));
            ui.checkbox(&mut self.find_opts.case_sensitive, t("Match case"));
            ui.checkbox(&mut self.find_opts.whole_word, t("Whole word"));
            ui.checkbox(&mut self.find_opts.skip_comments, t("Skip comments"));
        });
        ui.horizontal(|ui| {
            ui.label(t("In"));
            egui::ComboBox::from_id_salt("ffield").selected_text(t(["Text (raw)", "Style", "Actor", "Effect", "Text (no tags)"][self.find_opts.field as usize])).show_ui(ui, |ui| {
                for (k, l) in ["Text (raw)", "Style", "Actor", "Effect", "Text (no tags)"].iter().enumerate() { ui.selectable_value(&mut self.find_opts.field, k as u8, t(l)); }
            });
        });
        ui.horizontal(|ui| {
            if ui.button(t("Find next")).clicked() {
                match ops::find_all(&self.doc, &self.find, &self.find_opts) {
                    Ok(hits) => {
                        let next = hits.iter().copied().find(|&h| h > self.active).or(hits.first().copied());
                        match next { Some(h) => { self.select_only(h); self.scroll_to = Some(h); self.find_msg = tf!("{} match(es)", hits.len()); } None => self.find_msg = t("No matches").into() }
                    }
                    Err(e) => self.find_msg = e,
                }
            }
            if ui.button(t("Select all matches")).clicked() {
                match ops::find_all(&self.doc, &self.find, &self.find_opts) {
                    Ok(hits) => { self.find_msg = tf!("{} match(es)", hits.len()); if let Some(&f) = hits.first() { self.selection = hits.into_iter().collect(); self.active = f; self.scroll_to = Some(f); } }
                    Err(e) => self.find_msg = e,
                }
            }
            if ui.button(t("Replace all")).clicked() {
                self.push_undo();
                let only = if self.selection.len() > 1 { Some(self.sel_vec()) } else { None };
                match ops::replace_all(&mut self.doc, &self.find, &self.repl, &self.find_opts, only.as_deref()) {
                    Ok(n) => { self.find_msg = tf!("{} replacement(s)", n); self.run_qc(); }
                    Err(e) => self.find_msg = e,
                }
            }
        });
        ui.label(&self.find_msg);
        ui.weak(t("With several lines selected, Replace all applies to the selection only."));
    }

    fn shift_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("Amount"));
            ui.add(egui::TextEdit::singleline(&mut self.shift_ms).desired_width(110.0)).on_hover_text(t("A whole number is milliseconds (frames in frame mode); 1.5 or 0:00:01.50 is a time. Negative shifts earlier."));
            ui.radio_value(&mut self.shift_frames, false, t("ms"));
            ui.radio_value(&mut self.shift_frames, true, t("frames"));
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.shift_which, 0, t("Start & end"));
            ui.radio_value(&mut self.shift_which, 1, t("Start only"));
            ui.radio_value(&mut self.shift_which, 2, t("End only"));
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.shift_scope, 1, t("Selected lines"));
            ui.radio_value(&mut self.shift_scope, 0, t("All lines"));
            ui.radio_value(&mut self.shift_scope, 2, t("From selection onward")).on_hover_text(t("shifts the selected line and everything after it"));
        });
        if ui.button(t("Apply")).clicked() { self.apply_shift(); }
    }

    /// A whole number is milliseconds (or frames); anything else is a time ("1.5", "0:00:01.50").
    fn apply_shift(&mut self) {
        let txt = self.shift_ms.trim().replace(',', ".");
        let whole = !txt.is_empty() && txt.trim_start_matches(['-', '+']).chars().all(|c| c.is_ascii_digit());
        let v = if self.shift_frames || whole { txt.trim_start_matches('+').parse::<i64>().ok() } else { time::parse_ass(&txt) };
        let Some(v) = v else { self.last_status = t("Could not read the amount").into(); return };
        self.push_undo();
        let first = self.selection.iter().next().copied().unwrap_or(self.active);
        let idx: Vec<usize> = match self.shift_scope { 1 => self.sel_vec(), 2 => (first..self.doc.events.len()).collect(), _ => (0..self.doc.events.len()).collect() };
        if self.shift_frames { ops::shift_frames(&mut self.doc, &idx, v, fps_of(&self.settings), self.shift_which); } else { ops::shift(&mut self.doc, &idx, v, self.shift_which); }
        self.buf_key = (usize::MAX, 0, 0);
        self.last_status = tf!("{} lines shifted", idx.len());
    }

    fn qc_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("Max CPS")); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_cps).range(5.0..=60.0));
            ui.label(t("Max line length")); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_cpl).range(10..=200));
            ui.label(t("Min dur (ms)")); ui.add(egui::DragValue::new(&mut self.qc_cfg.min_duration_ms).range(0..=5000));
            ui.label(t("Max dur (ms)")); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_duration_ms).range(500..=60000));
            if ui.button(t("Run")).clicked() { self.settings.max_cps = self.qc_cfg.max_cps; self.settings.max_cpl = self.qc_cfg.max_cpl; self.settings.save(); self.run_qc(); }
        });
        ui.separator();
        let mut goto = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for is in &self.qc_issues {
                let (c, tag) = match is.severity { ops::Severity::Error => (Color32::from_rgb(255, 90, 90), t("ERR")), ops::Severity::Warning => (Color32::from_rgb(235, 190, 80), t("WARN")), _ => (Color32::GRAY, t("info")) };
                ui.horizontal(|ui| {
                    if ui.link(format!("#{}", is.index + 1)).clicked() { goto = Some(is.index); }
                    ui.colored_label(c, tag);
                    ui.label(i18n::message(&is.message));
                });
            }
            if self.qc_issues.is_empty() { ui.label(t("No issues found.")); }
        });
        if let Some(g) = goto { self.select_only(g); self.scroll_to = Some(g); }
    }

    fn props_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        egui::Grid::new("props").num_columns(2).show(ui, |ui| {
            for key in ["Title", "Original Script", "Original Translation", "Original Editing", "Original Timing", "Synch Point", "Script Updated By", "Update Details", "PlayResX", "PlayResY", "WrapStyle", "ScaledBorderAndShadow", "YCbCr Matrix"] {
                let mut v = self.doc.info_get(key).unwrap_or("").to_string();
                ui.label(t(key));
                if ui.text_edit_singleline(&mut v).changed() { changed = true; self.doc.info_set(key, &v); }
                ui.end_row();
            }
        });
        if changed { self.dirty = true; self.touch(); }
    }

    fn console_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button(t("Clear")).clicked() { self.console.clear(); }
        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for (l, s) in &self.console {
                let c = match l { 0 | 1 => Color32::from_rgb(255, 120, 120), 2 => Color32::from_rgb(235, 190, 80), _ => ui.visuals().text_color() };
                ui.add(egui::Label::new(RichText::new(s.trim_end()).monospace().color(c)));
            }
        });
    }

    fn post_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("pp").num_columns(2).show(ui, |ui| {
            ui.label(t("Lead-in (ms)")); ui.add(egui::DragValue::new(&mut self.pp.lead_in).range(0..=2000)); ui.end_row();
            ui.label(t("Lead-out (ms)")); ui.add(egui::DragValue::new(&mut self.pp.lead_out).range(0..=2000)); ui.end_row();
            ui.label(t("Join gaps up to (ms)")); ui.add(egui::DragValue::new(&mut self.pp.join_gap_threshold).range(0..=2000)); ui.end_row();
            ui.label(t("Minimum gap (ms)")); ui.add(egui::DragValue::new(&mut self.pp.min_gap).range(0..=500)); ui.end_row();
        });
        if ui.button(t("Apply to selected lines")).clicked() {
            self.push_undo();
            let idx = self.sel_vec();
            ops::post_process(&mut self.doc, &idx, &self.pp, &[]);
            self.buf_key = (usize::MAX, 0, 0);
        }
    }

    fn resample_ui(&mut self, ui: &mut egui::Ui) {
        let (cx, cy) = self.doc.res();
        ui.label(tf!("Current: {}×{}", cx, cy));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.resample_xy.0).range(16..=16384));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut self.resample_xy.1).range(16..=16384));
            for (w, h) in [(1280, 720), (1920, 1080), (3840, 2160)] { if ui.small_button(format!("{}p", h)).clicked() { self.resample_xy = (w, h); } }
        });
        if ui.button(t("Resample")).clicked() {
            self.push_undo();
            ops::resample(&mut self.doc, self.resample_xy.0, self.resample_xy.1);
        }
    }
}

// ---------------------------------------------------------------- media integration
impl App {
    fn fps(&self) -> time::Fps { fps_of(&self.settings) }

    fn goto_ms(&mut self, ms: i64) {
        self.media.seek(ms);
    }

    fn step_frame(&mut self, n: i64) {
        let f = self.fps();
        self.media.step(n, f);
    }

    fn seek_rel(&mut self, d: i64) {
        let t = self.media.pos_ms + d;
        self.media.seek(t);
    }

    /// ASS text for the ffmpeg fallback (libmpv gets live subtitles separately).
    fn fallback_ass(&self, t: i64) -> Option<String> {
        if self.media.uses_mpv() { None } else { self.ass_from(t, None) }
    }

    fn toggle_play(&mut self) {
        let ass = self.fallback_ass(self.media.pos_ms);
        self.media.toggle_play(|| ass);
    }

    /// Play part of the active line: `which` = line, before (500 ms before the start),
    /// after (500 ms after the end), first / last 500 ms.
    fn play_part(&mut self, which: PlayPart) {
        let Some(e) = self.doc.events.get(self.active).cloned() else { return };
        let (s, en) = (e.start, e.end.max(e.start));
        let (start, end) = match which {
            PlayPart::Line => (s, en),
            PlayPart::Before => ((s - 500).max(0), s),
            PlayPart::After => (en, en + 500),
            PlayPart::First => (s, (s + 500).min(en)),
            PlayPart::Last => ((en - 500).max(s), en),
            PlayPart::ToEnd => (self.media.pos_ms, en),
        };
        let ass = self.fallback_ass(start);
        self.media.play_range(start, end, || ass);
    }

    fn play_active_line(&mut self, pre: i64, post: i64) {
        let Some(e) = self.doc.events.get(self.active).cloned() else { return };
        let (start, end) = if pre < 0 { ((e.end + pre).max(e.start.min(e.end)), e.end + post) } else { (e.start, e.end + post) };
        let ass = self.fallback_ass(start);
        self.media.play_range(start, end, || ass);
    }

    /// F1 / F2: with video the time is placed so the line starts on (or ends after) exactly the
    /// frame on screen, even after rounding to centiseconds; audio-only uses the playhead as is.
    fn set_line_time_from_pos(&mut self, start: bool) {
        if self.active >= self.doc.events.len() { return; }
        self.begin_edit(0);
        let t = self.media.pos_ms;
        let t = if self.media.has_video() {
            let f = self.fps();
            let frame = f.frame_at(t);
            if start { f.line_start(frame) } else { f.line_end(frame) }
        } else { t };
        let sel = self.sel_vec();
        for k in sel {
            if start { self.doc.events[k].start = t; } else { self.doc.events[k].end = t; }
        }
        self.buf_key = (usize::MAX, 0, 0);
    }

    /// Mini ASS covering events overlapping [t, ∞), shifted so that `t` is time 0 (ffmpeg resets timestamps after -ss).
    fn ass_from(&self, t: i64, only_at: Option<i64>) -> Option<String> {
        let mut sc = Script::default();
        sc.info = self.doc.info.clone();
        sc.styles = self.doc.styles.clone();
        for e in self.doc.events.iter().filter(|e| !e.comment && e.end > t) {
            if let Some(at) = only_at { if e.start > at { continue; } }
            let mut e = e.clone();
            e.start = (e.start - t).max(-3_600_000);
            e.end -= t;
            // negative starts are not representable in ASS: clamp and keep effect timing by leaving as 0
            if e.start < 0 { e.start = 0; }
            sc.events.push(e);
        }
        Some(ass::write(&sc))
    }

    fn media_tick(&mut self, ctx: &egui::Context) {
        for m in self.media.poll(ctx) {
            if let MediaMsg::Info(Ok(i)) = m {
                if i.fps_num > 0 { self.settings.fps = (i.fps_num, i.fps_den); self.settings.save(); }
                if self.doc.info_get("PlayResX").is_none() && i.width > 0 {
                    self.doc.info_set("PlayResX", &i.width.to_string());
                    self.doc.info_set("PlayResY", &i.height.to_string());
                }
                self.last_status = tf!("Video: {}×{} @ {} fps, {}s", i.width, i.height, format!("{:.3}", i.fps_num as f64 / i.fps_den as f64), format!("{:.1}", i.duration_ms as f64 / 1000.0));
            }
        }
        // follow active line
        if self.last_active_seen != self.active {
            self.last_active_seen = self.active;
            if self.media.follow && (self.media.video.is_some() || self.media.audio.is_some()) {
                if let Some(e) = self.doc.events.get(self.active) {
                    if !self.media.playing { let t = e.start; self.media.seek(t); }
                    let w = 900.0 * self.media.ms_per_px;
                    if (e.start as f64) < self.media.view_start || (e.end as f64) > self.media.view_start + w { self.media.view_start = e.start as f64 - w * 0.2; }
                }
            }
        }
        // libmpv: keep the subtitles on the video in step with the document
        if self.media.uses_mpv() {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            self.doc_gen.hash(&mut h);
            self.doc.info.hash(&mut h);
            self.doc.styles.len().hash(&mut h);
            let gen = h.finish();
            let doc = &self.doc;
            self.media.sync_subtitles(gen, || ass::write(doc));
        }
        // ffmpeg fallback: refresh the still frame when position or visible subtitles change
        if !self.media.uses_mpv() && self.media.info.as_ref().map(|i| i.has_video).unwrap_or(false) && !self.media.playing {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            self.media.pos_ms.hash(&mut h);
            for e in self.doc.events.iter().filter(|e| !e.comment && e.start <= self.media.pos_ms + 1 && e.end > self.media.pos_ms) {
                (e.start, e.end, &e.text, &e.style, e.layer, e.margin_l, e.margin_r, e.margin_v).hash(&mut h);
            }
            for s in &self.doc.styles { ass::style_line(s).hash(&mut h); }
            self.doc.info.hash(&mut h);
            let key = h.finish();
            if key != self.media.last_render_key && !self.media.pending {
                self.media.last_render_key = key;
                let ass = self.ass_from(self.media.pos_ms, Some(self.media.pos_ms));
                self.media.request_frame(ass, 960);
            }
        }
        if self.media.playing { ctx.request_repaint(); }
    }

    fn video_panel(&mut self, ui: &mut egui::Ui) {
        let fps = self.fps();
        let avail = ui.available_width();
        let max_h = (ui.available_height() - 96.0).max(120.0);
        // display size follows the panel; with libmpv the frame is rendered at exactly that size
        let (vw, vh) = match (&self.media.player, &self.media.info) {
            (Some(p), _) if p.state().has_video() => { let st = p.state(); (st.width.max(1) as f32, st.height.max(1) as f32) }
            (_, Some(i)) if i.width > 0 => (i.width as f32, i.height as f32),
            _ => (16.0, 9.0),
        };
        let (mut w, mut h) = (avail, avail * vh / vw);
        if h > max_h { h = max_h; w = h * vw / vh; }
        let ppp = ui.ctx().pixels_per_point();
        self.media.set_display_size(w * ppp, h * ppp);
        if let Some(t) = &self.media.texture {
            let img = ui.add(egui::Image::new((t.id(), egui::Vec2::new(w, h))).sense(egui::Sense::click_and_drag()));
            self.video_rect = Some(img.rect);
            self.visual_overlay(ui, &img);
        } else {
            let (r, _) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::hover());
            ui.painter().rect_filled(r, 2.0, Color32::from_rgb(12, 12, 16));
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, if self.media.pending { "Rendering frame…" } else { "Loading video…" }, egui::FontId::proportional(14.0), Color32::GRAY);
        }
        if let Some(e) = &self.media.error { ui.colored_label(Color32::from_rgb(255, 120, 120), e); }
        self.visual_toolbar(ui);
        let dur = self.media.duration().max(1);
        ui.horizontal(|ui| {
            if ui.button("⏮").on_hover_text(self.cmd_hint("frame_prev")).clicked() { self.step_frame(-1); }
            if ui.button(if self.media.playing { "⏸" } else { "▶" }).on_hover_text(self.cmd_hint("play_pause")).clicked() { self.toggle_play(); }
            if ui.button("⏭").on_hover_text(self.cmd_hint("frame_next")).clicked() { self.step_frame(1); }
            ui.label(RichText::new(tf!("{}  ·  frame {}", time::format_ass(self.media.pos_ms), fps.frame_at(self.media.pos_ms))).monospace());
            if self.media.keyframes.binary_search(&self.media.pos_ms).is_ok() || self.media.keyframes.iter().any(|&k| fps.frame_at(k) == fps.frame_at(self.media.pos_ms)) {
                ui.colored_label(Color32::from_rgb(230, 200, 60), "KF").on_hover_text(t("This frame is a keyframe"));
            }
        });
        let mut pos = self.media.pos_ms as f64;
        ui.spacing_mut().slider_width = (avail - 20.0).max(100.0);
        if ui.add(egui::Slider::new(&mut pos, 0.0..=dur as f64).show_value(false)).changed() {
            self.media.seek(pos as i64);
        }
    }

    fn wave_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.small_button(t("Start=⏱ F1")).on_hover_text(t("Set line start to the video frame / playhead")).clicked() { self.set_line_time_from_pos(true); }
            if ui.small_button(t("End=⏱ F2")).on_hover_text(t("Set line end to the video frame / playhead")).clicked() { self.set_line_time_from_pos(false); }
            if ui.small_button(t("▶ line")).on_hover_text(self.cmd_hint("play_line")).clicked() { self.play_part(PlayPart::Line); }
            if ui.small_button("◀500").on_hover_text(self.cmd_hint("play_before")).clicked() { self.play_part(PlayPart::Before); }
            if ui.small_button("500▶").on_hover_text(self.cmd_hint("play_after")).clicked() { self.play_part(PlayPart::After); }
            if ui.small_button("|500").on_hover_text(self.cmd_hint("play_first")).clicked() { self.play_part(PlayPart::First); }
            if ui.small_button("500|").on_hover_text(self.cmd_hint("play_last")).clicked() { self.play_part(PlayPart::Last); }
            if ui.small_button("■").on_hover_text(self.cmd_hint("stop")).clicked() { self.media.stop(); }
            ui.separator();
            ui.selectable_value(&mut self.media.show_spectrum, false, t("Waveform"));
            ui.selectable_value(&mut self.media.show_spectrum, true, t("Spectrum")).on_hover_text(if self.media.spectrum.is_some() { "Mel spectrogram: speech onsets and syllables stand out" } else { "computed after the audio is decoded" });
            ui.toggle_value(&mut self.kara_audio, t("Karaoke")).on_hover_text(t("Show the active line's syllables: drag a boundary to retime, click a syllable to hear it"));
            ui.separator();
            ui.label(t("Zoom"));
            let mut z = (self.media.ms_per_px).ln();
            if ui.add(egui::Slider::new(&mut z, 0.25_f64.ln()..=400.0_f64.ln()).show_value(false)).changed() { self.media.ms_per_px = z.exp(); }
            ui.label("🔊");
            let mut v = self.media.volume;
            if ui.add(egui::Slider::new(&mut v, 0.0..=150.0).show_value(false)).changed() { self.media.set_volume(v); }
            if self.media.peaks_loading { ui.spinner(); ui.label(t("decoding audio…")); }
            if !yuzu_media::ffmpeg_available() { ui.colored_label(Color32::from_rgb(255, 160, 80), "ffmpeg/ffprobe not found in PATH"); }
        });
        let spans: Vec<(i64, i64, String, bool, bool)> = {
            let (a, b) = (self.media.view_start as i64 - 10_000, (self.media.view_start + (ui.available_width() as f64 * self.media.ms_per_px)) as i64 + 10_000);
            self.doc.events.iter().enumerate().filter(|(_, e)| e.end >= a && e.start <= b).map(|(i, e)| (e.start, e.end, tags::display_text(&e.text).replace('\n', " "), i == self.active, self.selection.contains(&i))).collect()
        };
        let spans_ref: Vec<LineSpan> = spans.iter().map(|(s, e, l, a, sel)| LineSpan { start: *s, end: *e, label: l, active: *a, selected: *sel }).collect();
        let h = ui.available_height().max(60.0);
        let mut drag = self.wave_drag;
        let fps = self.fps();
        let snap = self.media.keyframe_line_times(fps);
        let video_fps = self.media.has_video().then_some(fps);
        // karaoke mode: the active line's syllables at absolute times
        let kara: Vec<(i64, i64, String)> = match (self.kara_audio, self.doc.events.get(self.active)) {
            (true, Some(e)) => {
                let k = KaraLine::from_text(&e.text);
                let mut t0 = e.start;
                if k.total_cs() > 0 { k.syls.iter().enumerate().map(|(j, sy)| { let a = t0; t0 += sy.duration_cs * 10; (a, t0, k.plain(j)) }).collect() } else { vec![] }
            }
            _ => vec![],
        };
        let kara_ref: Vec<media::KaraSpan> = kara.iter().map(|(a, b, l)| media::KaraSpan { start: *a, end: *b, label: l }).collect();
        let ev = media::waveform(ui, &mut self.media, &spans_ref, h, &mut drag, &snap, video_fps, &kara_ref);
        self.wave_rect = Some(ev.rect);
        self.wave_drag = drag;
        let i = self.active;
        if ev.drag_begin { self.last_edit_key = 0; self.begin_edit(8_000_000 + i as u64); }
        if let Some(t) = ev.set_start {
            if i < self.doc.events.len() {
                if !ev.drag_begin && self.wave_drag.is_none() { self.begin_edit(0); } else { self.begin_edit(8_000_000 + i as u64); }
                self.doc.events[i].start = t;
                if self.doc.events[i].end < t { self.doc.events[i].end = t; }
                self.buf_key = (usize::MAX, 0, 0);
            }
        }
        if let Some(t) = ev.set_end {
            if i < self.doc.events.len() {
                if self.wave_drag.is_none() { self.begin_edit(0); } else { self.begin_edit(8_000_000 + i as u64); }
                self.doc.events[i].end = t.max(self.doc.events[i].start);
                self.buf_key = (usize::MAX, 0, 0);
            }
        }
        if let Some(t) = ev.seek { self.media.seek(t); }
        if let Some((k, t)) = ev.kara_move {
            // the boundary after syllable k moves; its neighbours share the change (the total stays)
            if let (Some(&(_, b, _)), true) = (kara.get(k), i < self.doc.events.len()) {
                let delta_cs = ((t - b) as f64 / 10.0).round() as i64;
                if delta_cs != 0 {
                    let mut model = KaraLine::from_text(&self.doc.events[i].text);
                    model.move_boundary(k, delta_cs);
                    let txt = model.to_text();
                    if txt != self.doc.events[i].text { self.begin_edit(8_000_000 + i as u64); self.doc.events[i].text = txt; }
                }
            }
        }
        if let Some(k) = ev.kara_play {
            if let Some(&(a, b, _)) = kara.get(k) { let ass = self.fallback_ass(a); self.media.play_range(a, b, || ass); }
        }
        if ev.drag_end { self.last_edit_key = 0; }
    }
}

// ---------------------------------------------------------------- karaoke editor
impl App {
    fn karaoke_ui(&mut self, ui: &mut egui::Ui) {
        let i = self.active;
        if i >= self.doc.events.len() { return; }
        // (re)load model when the active line changed externally
        let stale = match &self.kara { Some((li, _)) => *li != i || self.kara_text != self.doc.events[i].text, None => true };
        if stale { let k = KaraLine::from_text(&self.doc.events[i].text); self.kara_markup = k.markup(); self.kara_text = self.doc.events[i].text.clone(); self.kara = Some((i, k)); }
        let ev = self.doc.events[i].clone();
        let (mut model, mut changed) = (self.kara.take().unwrap().1, false);
        ui.label(tf!("Line {} · {} – {}  ({}s)", i + 1, time::format_ass(ev.start), time::format_ass(ev.end), format!("{:.2}", ev.duration() as f64 / 1000.0)));
        ui.horizontal(|ui| {
            ui.label(t("Syllable markup ( | splits )"));
            let r = ui.add(egui::TextEdit::singleline(&mut self.kara_markup).desired_width(420.0).font(egui::TextStyle::Monospace));
            if r.lost_focus() && self.kara_markup != model.markup() { model.apply_markup(&self.kara_markup); changed = true; }
        });
        ui.horizontal(|ui| {
            ui.label(t("Auto split:"));
            for (l, st) in [("char", Split::Char), ("word", Split::Word), ("romaji", Split::Romaji), ("kana", Split::Kana)] {
                if ui.button(t(l)).clicked() { model.resplit(st); changed = true; }
            }
            ui.separator();
            ui.label(t("Effect:"));
            for k in ["k", "kf", "ko"] { if ui.button(format!("\\{}", k)).clicked() { model.set_kind(k); changed = true; } }
            ui.separator();
            if ui.button(t("Even")).clicked() { let t = model.total_cs().max(ev.duration() / 10); model.distribute_evenly(t); changed = true; }
            if ui.button(t("Fit to line")).on_hover_text(t("Scale durations to the line's length")).clicked() { model.fit_total(ev.duration() / 10); changed = true; }
        });
        // syllable strip
        ui.separator();
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                let n = model.syls.len();
                for k in 0..n {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(if model.plain(k).is_empty() { "∅".to_string() } else { model.plain(k) }).size(18.0));
                        let mut d = model.syls[k].duration_cs;
                        if ui.add(egui::DragValue::new(&mut d).range(0..=100000).suffix(" cs")).changed() {
                            let delta = d - model.syls[k].duration_cs;
                            if k + 1 < n && ui.input(|i| !i.modifiers.shift) { model.move_boundary(k, delta); } else { model.syls[k].duration_cs = d; }
                            changed = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("✂").on_hover_text(t("Split in half")).clicked() { let c = model.plain(k).chars().count() / 2; model.split(k, c.max(1)); changed = true; }
                            if k + 1 < n && ui.small_button("⛓").on_hover_text(t("Join with next")).clicked() { model.join(k); changed = true; }
                        });
                    });
                    ui.separator();
                }
            });
        });
        // Total + tap timing
        let total = model.total_cs();
        let line_cs = ev.duration() / 10;
        ui.horizontal(|ui| {
            let col = if total == line_cs { ui.visuals().text_color() } else { Color32::from_rgb(235, 190, 80) };
            ui.colored_label(col, tf!("Total {} cs / line {} cs", total, line_cs));
            ui.separator();
            ui.label(t("Tap latency"));
            ui.add(egui::DragValue::new(&mut self.tap_latency_ms).range(0..=400).suffix(" ms"));
            let tapping = self.tap.is_some();
            if !tapping {
                if ui.button(t("● Tap timing")).on_hover_text(t("Plays the line; press Space at the end of each syllable")).clicked() {
                    let ass = self.fallback_ass(ev.start);
                    self.media.play_range(ev.start, ev.end + 400, || ass);
                    self.tap = Some(TapState { line: i, started: std::time::Instant::now(), start_ms: ev.start, taps: vec![] });
                }
            } else if ui.button(t("■ Cancel tapping")).clicked() { self.tap = None; self.media.stop(); }
        });
        if self.tap.as_ref().map(|t| t.line != i).unwrap_or(false) { self.tap = None; self.media.stop(); }
        if let Some(t) = &mut self.tap {
            let n = model.syls.len();
            ui.colored_label(Color32::from_rgb(255, 120, 120), tf!("Tapping… {}/{} — press Space", t.taps.len(), n));
            if ui.input(|inp| inp.key_pressed(Key::Space)) {
                // libmpv reports where playback really is; the ffmpeg path only has the wall clock
                let heard = if self.media.uses_mpv() { self.media.pos_ms } else { t.start_ms + t.started.elapsed().as_millis() as i64 };
                let now = heard - self.tap_latency_ms;
                t.taps.push(now.clamp(t.start_ms, ev.end + 400));
            }
            if t.taps.len() >= n {
                let (start, taps) = (t.start_ms, t.taps.clone());
                model.apply_taps(start, &taps);
                changed = true;
                self.tap = None;
                self.media.stop();
            }
            ui.ctx().request_repaint();
        }
        if changed {
            self.begin_edit(0);
            self.doc.events[i].text = model.to_text();
            self.kara_text = self.doc.events[i].text.clone();
            self.kara_markup = model.markup();
            self.run_qc();
        }
        self.kara = Some((i, model));
        ui.weak(t("Tip: drag a syllable's duration to move the boundary with its neighbour (hold Shift to change only that syllable)."));
    }
}

// ---------------------------------------------------------------- fonts
impl App {
    fn font_db(&self) -> fontdb::Database {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut dirs = yuzu_fonts::user_font_dirs();
        if let Some(d) = self.path.as_ref().and_then(|p| yuzu_fonts::script_dir(p)) { dirs.push(d); }
        yuzu_fonts::scan_dirs(&mut db, &dirs);
        db
    }

    fn fonts_ui(&mut self, ui: &mut egui::Ui) {
        let uses = yuzu_fonts::used_fonts(&self.doc);
        let db = self.font_db();
        let located = yuzu_fonts::locate_all(&uses, &db);
        egui::Grid::new("fontgrid").striped(true).show(ui, |ui| {
            ui.strong(t("Font")); ui.strong("B/I"); ui.strong(t("Lines")); ui.strong(t("File")); ui.end_row();
            for (u, l) in uses.iter().zip(&located) {
                ui.label(&u.family);
                ui.label(format!("{}{}", if u.bold { "B" } else { "" }, if u.italic { "I" } else { "" }));
                ui.label(u.lines.to_string());
                match &l.file {
                    Some(f) if l.exact => { ui.label(f.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()); }
                    Some(f) => { ui.colored_label(Color32::from_rgb(235, 190, 80), tf!("substitute: {}", f.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())); }
                    None => { ui.colored_label(Color32::from_rgb(255, 90, 90), "MISSING"); }
                }
                ui.end_row();
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(t("Copy fonts to folder…")).clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    let opts = yuzu_fonts::Options { dest_dir: Some(d), ..Default::default() };
                    self.fonts_report = match yuzu_fonts::collect_located(&located, &opts) { Ok(r) => r.listing().lines().map(String::from).collect(), Err(e) => vec![e.to_string()] };
                }
            }
            if ui.button(t("Save as .zip…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name("fonts.zip").save_file() {
                    let opts = yuzu_fonts::Options { zip_path: Some(p), ..Default::default() };
                    self.fonts_report = match yuzu_fonts::collect_located(&located, &opts) { Ok(r) => r.listing().lines().map(String::from).collect(), Err(e) => vec![e.to_string()] };
                }
            }
            if ui.button(t("Embed used fonts into script")).clicked() {
                self.push_undo();
                let mut out = vec![];
                for l in located.iter().filter(|l| l.exact) {
                    if let Some(f) = &l.file { match yuzu_fonts::embed(&mut self.doc, &f.path) { Ok(n) => out.push(tf!("embedded {}", n)), Err(e) => out.push(format!("{}: {}", f.path.display(), e)) } }
                }
                self.fonts_report = out;
            }
        });
        let emb = yuzu_fonts::extract_embedded(&self.doc);
        if !emb.is_empty() {
            ui.separator();
            ui.label(tf!("{} embedded font(s):", emb.len()));
            let mut remove = None;
            for (n, d) in &emb { ui.horizontal(|ui| { ui.label(format!("{} ({} KB)", n, d.len() / 1024)); if ui.small_button(t("remove")).clicked() { remove = Some(n.clone()); } if ui.small_button(t("extract…")).clicked() { if let Some(p) = rfd::FileDialog::new().set_file_name(n).save_file() { let _ = std::fs::write(p, d); } } }); }
            if let Some(n) = remove { self.push_undo(); yuzu_fonts::remove_embedded(&mut self.doc, &n); }
        }
        for l in &self.fonts_report { ui.label(l); }
    }
}

// ---------------------------------------------------------------- common error fixes
impl App {
    fn fix_ui(&mut self, ui: &mut egui::Ui) {
        use asscore::fixes::{Lang, QuoteStyle};
        let f = &mut self.fixes;
        ui.horizontal(|ui| {
            ui.label(t("Language"));
            for (l, n) in [(Lang::English, "English"), (Lang::Turkish, "Türkçe"), (Lang::Other, t("Other"))] { ui.selectable_value(&mut f.lang, l, n); }
        });
        egui::Grid::new("fixg").num_columns(2).show(ui, |ui| {
            ui.checkbox(&mut f.overlaps, t("Fix overlaps")); ui.checkbox(&mut f.short_durations, t("Fix too-short durations")); ui.end_row();
            ui.checkbox(&mut f.long_durations, t("Fix too-long durations")); ui.checkbox(&mut f.double_spaces, t("Double spaces")); ui.end_row();
            ui.checkbox(&mut f.missing_spaces, t("Missing spaces")); ui.checkbox(&mut f.unneeded_spaces, t("Unneeded spaces")); ui.end_row();
            ui.checkbox(&mut f.all_caps, t("ALL CAPS to sentence case")); ui.checkbox(&mut f.i_confusion, t("I / l / İ / ı confusion")); ui.end_row();
            ui.checkbox(&mut f.dashes, t("Dialogue dashes")); ui.checkbox(&mut f.ellipsis, t("Ellipsis")); ui.end_row();
            ui.checkbox(&mut f.hearing_impaired, t("Remove [sound] / ♪ / NAME:")); ui.checkbox(&mut f.auto_break, t("Auto-balance line breaks")); ui.end_row();
            ui.checkbox(&mut f.merge_duplicates, t("Merge duplicate lines")); ui.checkbox(&mut f.empty_lines, t("Remove empty lines")); ui.end_row();
        });
        let mut curly = f.quotes == Some(QuoteStyle::Curly);
        if ui.checkbox(&mut curly, t("Curly quotes")).changed() { f.quotes = if curly { Some(QuoteStyle::Curly) } else { None }; }
        let scope_sel = self.selection.len() > 1;
        ui.horizontal(|ui| {
            if ui.button(if scope_sel { "Apply to selection" } else { "Apply to all lines" }).clicked() {
                let sel = if scope_sel { self.sel_vec() } else { vec![] };
                self.push_undo();
                let fixes = self.fixes.clone();
                let res = asscore::fixes::fix_common_errors(&mut self.doc, &sel, &fixes);
                self.fix_preview = res.into_iter().map(|(n, c)| (n.to_string(), c.len())).filter(|(_, c)| *c > 0).collect();
                self.clamp_sel();
                self.run_qc();
                self.buf_key = (usize::MAX, 0, 0);
            }
        });
        for (n, c) in &self.fix_preview { ui.label(tf!("{}: {} change(s)", i18n::message(n), c)); }
        if self.fix_preview.is_empty() { ui.weak(t("Undo (Ctrl+Z) reverts the whole run.")); }
    }
}
