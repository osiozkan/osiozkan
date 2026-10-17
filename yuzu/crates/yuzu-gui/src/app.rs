use crate::luadialog::{self, LuaDialogState};
use crate::settings::Settings;
use crate::worker::{self, Cmd, Msg, ScriptInfo, Worker};
use crate::media::{self, Drag, LineSpan, MediaMsg, MediaState};
use crate::preview;
use asscore::{ass, ops, subrip, tags, time, Event, Script, Style};
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
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
enum Panel { Find, Shift, Styles, Qc, Props, Console, Post, Resample, About }

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
    shift_sel_only: bool,
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
    last_active_seen: usize,
    wave_height: f32,
}

fn fps_of(s: &Settings) -> time::Fps { time::Fps { num: s.fps.0, den: s.fps.1 } }

fn setup_fonts(ctx: &egui::Context) {
    // Add a CJK-capable system font as fallback when one is installed.
    let candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    ];
    for p in candidates {
        if let Ok(data) = std::fs::read(p) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("cjk".into(), std::sync::Arc::new(egui::FontData::from_owned(data)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
            ctx.set_fonts(fonts);
            break;
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<String>, video: Option<String>) -> App {
        let settings = Settings::load();
        setup_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(if settings.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
        let worker = worker::spawn(cc.egui_ctx.clone());
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
            shift_sel_only: true,
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
            last_status: "Ready".into(),
            last_autosave: std::time::Instant::now(),
            text_cursor: None,
            show_preview: true,
            show_text_stripped: false,
            ctx: cc.egui_ctx.clone(),
            quit_confirm: false,
            media: MediaState::new(cc.egui_ctx.clone()),
            wave_drag: None,
            last_active_seen: usize::MAX,
            wave_height: 150.0,
        };
        app.doc.events.push(Event::new(0, 2000, "Default", ""));
        app.reload_scripts();
        if let Some(f) = file { app.open_path(PathBuf::from(f)); }
        if let Some(v) = video { app.media.open(PathBuf::from(v)); }
        app
    }

    // ------------------------------------------------------------ document plumbing

    fn snapshot(&self) -> Snapshot {
        Snapshot { script: self.doc.clone(), selection: self.selection.clone(), active: self.active }
    }

    fn push_undo(&mut self) {
        let s = self.snapshot();
        self.undo.push(s);
        if self.undo.len() > 300 { self.undo.remove(0); }
        self.redo.clear();
        self.dirty = true;
        self.last_edit_key = 0;
    }

    /// Start an edit: only push an undo snapshot when the edit key changes (coalesces typing).
    fn begin_edit(&mut self, key: u64) {
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
                let txt = asscore::decode_bytes(&b);
                let mut sc = asscore::load_str(&p.to_string_lossy(), &txt);
                if sc.events.is_empty() { sc.events.push(Event::new(0, 2000, "Default", "")); }
                self.doc = sc;
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
                self.last_status = format!("Opened {}", p.display());
                self.run_qc();
            }
            Err(e) => self.last_status = format!("Open failed: {}", e),
        }
    }

    fn save_to(&mut self, p: PathBuf) {
        let lower = p.to_string_lossy().to_ascii_lowercase();
        let out = if lower.ends_with(".srt") { subrip::write_srt(&self.doc) } else if lower.ends_with(".vtt") { subrip::write_vtt(&self.doc) } else { format!("\u{feff}{}", ass::write(&self.doc)) };
        match std::fs::write(&p, out) {
            Ok(_) => {
                if !lower.ends_with(".srt") && !lower.ends_with(".vtt") { self.path = Some(p.clone()); self.dirty = false; }
                self.last_status = format!("Saved {}", p.display());
            }
            Err(e) => self.last_status = format!("Save failed: {}", e),
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
        ProjectInfo {
            script_path: self.path.clone(),
            fps: Some(self.settings.fps),
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
                    self.last_status = format!("{} automation script(s) loaded{}", v.len() - errs, if errs > 0 { format!(", {} failed (see Console)", errs) } else { String::new() });
                    self.scripts = v;
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
                    else if kind == "task" || kind == "title" { self.last_status = text; }
                }
                Msg::Done(r) => {
                    self.busy = None;
                    self.lua_dialog = None;
                    match r {
                        Ok((doc, out)) => {
                            if out.cancelled {
                                self.last_status = "Macro cancelled".into();
                            } else {
                                if let Some(pre) = self.pre_run.take() { self.undo.push(pre); self.redo.clear(); }
                                self.doc = *doc;
                                self.dirty = true;
                                if self.doc.events.is_empty() { self.doc.events.push(Event::new(0, 2000, "Default", "")); }
                                if let Some(sel) = out.selection { self.selection = sel.into_iter().collect(); }
                                if let Some(a) = out.active { self.active = a; self.selection.insert(a); }
                                self.clamp_sel();
                                self.buf_key = (usize::MAX, 0, 0);
                                self.last_status = "Macro finished".into();
                                self.run_qc();
                            }
                        }
                        Err(e) => {
                            self.console.push((1, format!("{}\n", e)));
                            self.last_status = "Macro failed (see Console)".into();
                            if !self.open.contains(&Panel::Console) { self.open.push(Panel::Console); }
                        }
                    }
                }
                Msg::Valid(..) => {}
            }
        }
    }

    fn toggle(&mut self, p: Panel) {
        if let Some(i) = self.open.iter().position(|x| *x == p) { self.open.remove(i); } else { self.open.push(p); }
    }
    fn is_open(&self, p: Panel) -> bool { self.open.contains(&p) }
}

fn fmt_cps(e: &Event) -> String {
    let c = ops::cps(e);
    if c.is_finite() { format!("{:.1}", c) } else { "∞".into() }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
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

        let title = format!("{}{} — Yuzu", self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into()), if self.dirty { " *" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        self.media_tick(ctx);
        self.menu_bar(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.last_status);
                if let Some(b) = &self.busy {
                    ui.separator();
                    ui.spinner();
                    ui.label(format!("Running: {}", b));
                    ui.add(egui::ProgressBar::new(self.progress).desired_width(160.0));
                    if ui.button("Cancel").clicked() { self.worker.cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let d = &self.doc;
                    let (x, y) = d.res();
                    ui.label(format!("{} lines · {}×{} · {:.3} fps · QC {}", d.events.len(), x, y, fps_of(&self.settings).as_f64(), self.qc_issues.len()));
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
                if self.media.info.as_ref().map(|i| i.has_video).unwrap_or(false) {
                    self.video_panel(ui);
                } else {
                    ui.heading("Preview");
                    let sz = egui::Vec2::new(ui.available_width(), (ui.available_height() - 6.0).min(ui.available_width() * 0.75));
                    preview::show(ui, &self.doc, self.doc.events.get(self.active), sz);
                }
                ui.separator();
                self.info_box(ui);
            });
        }
        egui::CentralPanel::default().show(ctx, |ui| self.grid(ui));
        self.windows(ctx);
        if self.quit_confirm {
            egui::Window::new("Unsaved changes").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("Save changes before closing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() { self.save(); if !self.dirty { self.quit_confirm = false; self.dirty = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); } }
                    if ui.button("Discard").clicked() { self.dirty = false; self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    if ui.button("Cancel").clicked() { self.quit_confirm = false; }
                });
            });
        }
    }
}

impl App {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let editing = ctx.wants_keyboard_input();
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))) { self.do_redo(); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Z))) { self.do_undo(); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Y))) { self.do_redo(); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S))) { self.save(); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O))) {
            if let Some(p) = rfd::FileDialog::new().add_filter("Subtitles", &["ass", "ssa", "srt", "vtt"]).pick_file() { self.open_path(p); }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::F))) { if !self.is_open(Panel::Find) { self.open.push(Panel::Find); } }
        if !editing {
            if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::Delete))) { self.delete_selected(); }
            if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::A))) { self.selection = (0..self.doc.events.len()).collect(); }
            if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::ArrowDown))) {
                let n = (self.active + 1).min(self.doc.events.len() - 1);
                self.select_only(n); self.scroll_to = Some(n);
            }
            if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::ArrowUp))) {
                let n = self.active.saturating_sub(1);
                self.select_only(n); self.scroll_to = Some(n);
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Enter))) {
            // commit + advance
            let n = self.active + 1;
            if n >= self.doc.events.len() { self.insert_after_active(); } else { self.select_only(n); self.scroll_to = Some(n); }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::D))) { self.duplicate_selected(); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::F1))) { self.set_line_time_from_pos(true); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::F2))) { self.set_line_time_from_pos(false); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::F3))) { self.play_active_line(0, 0); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::NONE, Key::F4))) { self.play_active_line(-500, 500); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Space))) { self.toggle_play(); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::ArrowRight))) { self.seek_rel(1000); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::ArrowLeft))) { self.seek_rel(-1000); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::ArrowRight))) { self.step_frame(1); }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(Modifiers::COMMAND, Key::ArrowLeft))) { self.step_frame(-1); }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.doc = Script::new();
                        self.doc.events.push(Event::new(0, 2000, "Default", ""));
                        self.path = None; self.undo.clear(); self.redo.clear(); self.dirty = false; self.select_only(0);
                        ui.close();
                    }
                    if ui.button("Open…  Ctrl+O").clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("Subtitles", &["ass", "ssa", "srt", "vtt"]).pick_file() { self.open_path(p); }
                    }
                    ui.menu_button("Open recent", |ui| {
                        for r in self.settings.recent.clone().into_iter().take(10) {
                            if ui.button(r.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()).on_hover_text(r.display().to_string()).clicked() { ui.close(); self.open_path(r); }
                        }
                    });
                    ui.separator();
                    if ui.button("Save  Ctrl+S").clicked() { ui.close(); self.save(); }
                    if ui.button("Save as…").clicked() { ui.close(); self.save_as(); }
                    if ui.button("Export SRT…").clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("SubRip", &["srt"]).set_file_name("subtitles.srt").save_file() { self.save_to(p); }
                    }
                    if ui.button("Export WebVTT…").clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("WebVTT", &["vtt"]).set_file_name("subtitles.vtt").save_file() { self.save_to(p); }
                    }
                    ui.separator();
                    if ui.button("Script properties…").clicked() { ui.close(); self.open.push(Panel::Props); }
                    ui.separator();
                    if ui.button("Quit").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  Ctrl+Z")).clicked() { ui.close(); self.do_undo(); }
                    if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo  Ctrl+Shift+Z")).clicked() { ui.close(); self.do_redo(); }
                    ui.separator();
                    if ui.button("Insert line after").clicked() { ui.close(); self.insert_after_active(); }
                    if ui.button("Insert line before").clicked() { ui.close(); self.insert_before_active(); }
                    if ui.button("Duplicate  Ctrl+D").clicked() { ui.close(); self.duplicate_selected(); }
                    if ui.button("Join selected").clicked() { ui.close(); self.merge_selected(); }
                    if ui.button("Delete  Del").clicked() { ui.close(); self.delete_selected(); }
                    ui.separator();
                    if ui.button("Find & replace…  Ctrl+F").clicked() { ui.close(); if !self.is_open(Panel::Find) { self.open.push(Panel::Find); } }
                });
                ui.menu_button("Timing", |ui| {
                    if ui.button("Shift times…").clicked() { ui.close(); self.toggle(Panel::Shift); }
                    if ui.button("Snap to frames").clicked() {
                        ui.close();
                        self.push_undo();
                        let idx = self.sel_vec();
                        ops::snap_to_frames(&mut self.doc, &idx, fps_of(&self.settings));
                    }
                    if ui.button("Post-processor…").clicked() { ui.close(); self.toggle(Panel::Post); }
                    ui.separator();
                    if ui.button("Sort by start").clicked() { ui.close(); self.push_undo(); self.doc.sort_by_start(); }
                    if ui.button("Sort by end").clicked() { ui.close(); self.push_undo(); self.doc.sort_by_end(); }
                    if ui.button("Sort by style").clicked() { ui.close(); self.push_undo(); self.doc.sort_by_style(); }
                    ui.separator();
                    ui.menu_button("Frame rate", |ui| {
                        for (n, d, l) in [(24000, 1001, "23.976"), (24, 1, "24"), (25, 1, "25"), (30000, 1001, "29.970"), (30, 1, "30"), (60000, 1001, "59.940"), (60, 1, "60")] {
                            if ui.radio(self.settings.fps == (n, d), l).clicked() { self.settings.fps = (n, d); self.settings.save(); ui.close(); }
                        }
                    });
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Styles manager…").clicked() { ui.close(); self.toggle(Panel::Styles); }
                    if ui.button("Quality check…").clicked() { ui.close(); self.run_qc(); self.toggle(Panel::Qc); }
                    if ui.button("Resample resolution…").clicked() { ui.close(); self.toggle(Panel::Resample); }
                    ui.separator();
                    if ui.button("Clean tags (selected)").clicked() {
                        ui.close();
                        self.push_undo();
                        for i in self.sel_vec() { let t = tags::clean_tags(&self.doc.events[i].text); self.doc.events[i].text = t; }
                    }
                    if ui.button("Strip all tags (selected)").clicked() {
                        ui.close();
                        self.push_undo();
                        for i in self.sel_vec() { self.doc.events[i].text = tags::strip_tags(&self.doc.events[i].text); }
                    }
                    if ui.button("Strip karaoke (selected)").clicked() {
                        ui.close();
                        self.push_undo();
                        for i in self.sel_vec() { let t = tags::strip_karaoke(&self.doc.events[i].text); self.doc.events[i].text = t; }
                    }
                    if ui.button("Remove duplicate lines").clicked() {
                        ui.close();
                        self.push_undo();
                        let n = ops::dedupe(&mut self.doc);
                        self.last_status = format!("Removed {} duplicate line(s)", n);
                        self.clamp_sel();
                    }
                    if ui.button("Trim whitespace (all)").clicked() {
                        ui.close();
                        self.push_undo();
                        for e in self.doc.events.iter_mut() { e.text = e.text.trim().replace("  ", " "); }
                    }
                });
                ui.menu_button("Automation", |ui| {
                    if self.scripts.is_empty() { ui.label("No scripts loaded"); }
                    let scripts: Vec<(usize, String, Vec<(usize, String, String)>, bool)> = self.scripts.iter().enumerate()
                        .map(|(i, s)| (i, s.name.clone(), s.macros.iter().map(|m| (m.index, m.name.clone(), m.description.clone())).collect(), s.error.is_some())).collect();
                    for (si, name, macros, err) in scripts {
                        if err { ui.add_enabled(false, egui::Button::new(format!("⚠ {}", name))); continue; }
                        for (mi, mn, md) in macros {
                            // Aegisub convention: "Submenu/Name"
                            let label = mn.clone();
                            if ui.add_enabled(self.busy.is_none(), egui::Button::new(label)).on_hover_text(md).clicked() {
                                ui.close();
                                self.run_macro(si, mi, &mn);
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Reload scripts").clicked() { ui.close(); self.reload_scripts(); }
                    if ui.button("Add script folder…").clicked() {
                        ui.close();
                        if let Some(d) = rfd::FileDialog::new().pick_folder() { self.settings.script_dirs.push(d); self.settings.save(); self.reload_scripts(); }
                    }
                    if ui.button("Console").clicked() { ui.close(); self.toggle(Panel::Console); }
                });
                ui.menu_button("Video", |ui| {
                    if ui.button("Open video…").clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("Video / audio", &["mkv", "mp4", "avi", "webm", "m2ts", "ts", "mov", "m4v", "flv", "mka", "mp3", "aac", "flac", "wav", "ogg", "opus", "m4a"]).pick_file() { self.media.open(p); }
                    }
                    if ui.button("Open audio only…").clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().pick_file() { self.media.audio = Some(p.clone()); self.media.load_peaks(p); }
                    }
                    if ui.add_enabled(self.media.video.is_some() || self.media.audio.is_some(), egui::Button::new("Close media")).clicked() { ui.close(); self.media.close(); }
                    ui.separator();
                    if ui.button("Set line start from video  F1").clicked() { ui.close(); self.set_line_time_from_pos(true); }
                    if ui.button("Set line end from video  F2").clicked() { ui.close(); self.set_line_time_from_pos(false); }
                    if ui.button("Play line  F3").clicked() { ui.close(); self.play_active_line(0, 0); }
                    if ui.button("Play last 500 ms + 500 ms after end  F4").clicked() { ui.close(); self.play_active_line(-500, 500); }
                    if ui.button("Play / pause  Ctrl+Space").clicked() { ui.close(); self.toggle_play(); }
                    ui.separator();
                    if ui.button("Go to line start").clicked() { ui.close(); self.goto_ms(self.doc.events.get(self.active).map(|e| e.start).unwrap_or(0)); }
                    if ui.button("Go to line end").clicked() { ui.close(); self.goto_ms(self.doc.events.get(self.active).map(|e| e.end).unwrap_or(0)); }
                    ui.separator();
                    ui.checkbox(&mut self.media.snap_keyframes, "Snap waveform drags to keyframes");
                    ui.checkbox(&mut self.media.follow, "Follow active line");
                    if ui.button("Snap selected to keyframes (±300 ms)").clicked() {
                        ui.close();
                        self.push_undo();
                        let kf = self.media.keyframes.clone();
                        let pp = ops::PostProc { lead_in: 0, lead_out: 0, keyframe_snap_before: 300, keyframe_snap_after: 300, ..Default::default() };
                        let idx = self.sel_vec();
                        ops::post_process(&mut self.doc, &idx, &pp, &kf);
                        self.buf_key = (usize::MAX, 0, 0);
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_preview, "Preview panel");
                    if ui.checkbox(&mut self.settings.dark, "Dark theme").changed() { ctx.set_visuals(if self.settings.dark { egui::Visuals::dark() } else { egui::Visuals::light() }); self.settings.save(); }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About Yuzu").clicked() { ui.close(); self.toggle(Panel::About); }
                });
            });
        });
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        let n = self.doc.events.len();
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
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
            .column(Column::initial(100.0).clip(true))
            .column(Column::initial(80.0).clip(true))
            .column(Column::remainder().clip(true));
        if let Some(t) = self.scroll_to.take() { tb = tb.scroll_to_row(t, Some(egui::Align::Center)); }
        tb.header(22.0, |mut h| {
            for t in ["#", "L", "Start", "End", "Dur", "CPS", "Style", "Actor", "Text"] { h.col(|ui| { ui.strong(t); }); }
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
                row.col(|ui| { ui.label(tint(e.style.clone())); });
                row.col(|ui| { ui.label(tint(e.actor.clone())); });
                row.col(|ui| {
                    let txt = if self.show_text_stripped { tags::display_text(&e.text).replace('\n', " ⏎ ") } else { e.text.clone() };
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
        ui.checkbox(&mut self.show_text_stripped, "Show lines without tags");
        if let Some(e) = self.doc.events.get(self.active) {
            ui.label(format!("{} chars · {:.1} CPS · {} ms", tags::visible_len(&e.text), ops::cps(e), e.duration()));
            let karaoke = tags::karaoke_syllables(&e.text);
            if karaoke.iter().any(|s| s.duration_cs > 0) {
                ui.label(format!("Karaoke: {} syllables, {:.2}s", karaoke.len(), karaoke.iter().map(|s| s.duration_cs).sum::<i64>() as f64 / 100.0));
            }
        }
        for is in self.qc_issues.iter().filter(|i| i.index == self.active) {
            let c = match is.severity { ops::Severity::Error => Color32::from_rgb(255, 90, 90), ops::Severity::Warning => Color32::from_rgb(235, 190, 80), _ => Color32::GRAY };
            ui.colored_label(c, &is.message);
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
            if ui.checkbox(&mut comment, "Comment").changed() {
                self.begin_edit(0);
                for k in self.sel_vec() { self.doc.events[k].comment = comment; }
            }
            ui.label("Layer");
            let mut layer = self.doc.events[i].layer;
            if ui.add(egui::DragValue::new(&mut layer).range(0..=999)).changed() {
                self.begin_edit(1_000_000 + i as u64);
                for k in self.sel_vec() { self.doc.events[k].layer = layer; }
            }
            ui.label("Style");
            let mut cur = self.doc.events[i].style.clone();
            egui::ComboBox::from_id_salt("estyle").selected_text(&cur).show_ui(ui, |ui| {
                for s in &styles { ui.selectable_value(&mut cur, s.clone(), s); }
            });
            if cur != self.doc.events[i].style {
                self.begin_edit(0);
                for k in self.sel_vec() { self.doc.events[k].style = cur.clone(); }
            }
            ui.label("Actor");
            let mut actor = self.doc.events[i].actor.clone();
            if ui.add(egui::TextEdit::singleline(&mut actor).desired_width(90.0)).changed() {
                self.begin_edit(2_000_000 + i as u64);
                for k in self.sel_vec() { self.doc.events[k].actor = actor.clone(); }
            }
            ui.label("Effect");
            let mut eff = self.doc.events[i].effect.clone();
            if ui.add(egui::TextEdit::singleline(&mut eff).desired_width(90.0)).changed() {
                self.begin_edit(3_000_000 + i as u64);
                for k in self.sel_vec() { self.doc.events[k].effect = eff.clone(); }
            }
            ui.label("Margins");
            for (lbl, which) in [("L", 0), ("R", 1), ("V", 2)] {
                let mut v = match which { 0 => self.doc.events[i].margin_l, 1 => self.doc.events[i].margin_r, _ => self.doc.events[i].margin_v };
                if ui.add(egui::DragValue::new(&mut v).range(0..=9999).prefix(format!("{} ", lbl))).changed() {
                    self.begin_edit(4_000_000 + which as u64 + (i as u64) * 10);
                    for k in self.sel_vec() { let ev = &mut self.doc.events[k]; match which { 0 => ev.margin_l = v, 1 => ev.margin_r = v, _ => ev.margin_v = v } }
                }
            }
            if multi { ui.weak(format!("({} lines selected)", self.selection.len())); }
        });
        ui.horizontal(|ui| {
            let mut changed_time = false;
            let w = 90.0;
            ui.label("Start");
            let r1 = ui.add(egui::TextEdit::singleline(&mut self.buf_start).desired_width(w));
            ui.label("End");
            let r2 = ui.add(egui::TextEdit::singleline(&mut self.buf_end).desired_width(w));
            ui.label("Duration");
            let r3 = ui.add(egui::TextEdit::singleline(&mut self.buf_dur).desired_width(60.0));
            if r1.lost_focus() { if let Some(t) = time::parse_ass(&self.buf_start) { self.begin_edit(0); self.doc.events[i].start = t.max(0); changed_time = true; } }
            if r2.lost_focus() { if let Some(t) = time::parse_ass(&self.buf_end) { self.begin_edit(0); self.doc.events[i].end = t.max(0); changed_time = true; } }
            if r3.lost_focus() { if let Ok(d) = self.buf_dur.trim().parse::<f64>() { self.begin_edit(0); let st = self.doc.events[i].start; self.doc.events[i].end = st + (d * 1000.0).round() as i64; changed_time = true; } }
            if changed_time { self.buf_key = (usize::MAX, 0, 0); }
            ui.separator();
            for (label, hint, open, close) in [("B", "Bold", "{\\b1}", "{\\b0}"), ("I", "Italic", "{\\i1}", "{\\i0}"), ("U", "Underline", "{\\u1}", "{\\u0}"), ("S", "Strikeout", "{\\s1}", "{\\s0}")] {
                if ui.button(RichText::new(label).strong()).on_hover_text(hint).clicked() { self.wrap_text(open, close); }
            }
            if ui.button("\\N").on_hover_text("Insert line break").clicked() { self.wrap_text("\\N", ""); }
            if ui.button("{\\an8}").on_hover_text("Top-center alignment").clicked() { self.prefix_tag("{\\an8}"); }
            ui.separator();
            if ui.button("Split").on_hover_text("Split line at cursor, dividing time proportionally").clicked() { self.split_at_cursor(); }
        });
        // text box
        let id = egui::Id::new("text_edit");
        let mut text = self.doc.events[i].text.clone();
        let out = egui::TextEdit::multiline(&mut text).id(id).desired_width(f32::INFINITY).desired_rows(4).font(egui::TextStyle::Monospace).show(ui);
        if let Some(r) = out.cursor_range { let (a, b) = (r.primary.index, r.secondary.index); self.text_cursor = Some((a.min(b), a.max(b))); }
        if out.response.changed() {
            self.begin_edit(5_000_000 + i as u64);
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
        let t = self.doc.events[i].text.clone();
        self.doc.events[i].text = if t.starts_with('{') && !tag.is_empty() {
            // merge into first block
            let inner = &tag[1..tag.len() - 1];
            format!("{{{}{}", inner, &t[1..])
        } else { format!("{}{}", tag, t) };
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
                Panel::Find => { egui::Window::new("Find & replace").open(&mut open).default_width(420.0).show(ctx, |ui| self.find_ui(ui)); }
                Panel::Shift => { egui::Window::new("Shift times").open(&mut open).show(ctx, |ui| self.shift_ui(ui)); }
                Panel::Styles => { egui::Window::new("Styles manager").open(&mut open).default_width(560.0).show(ctx, |ui| self.styles_ui(ui)); }
                Panel::Qc => { egui::Window::new("Quality check").open(&mut open).default_width(560.0).default_height(420.0).show(ctx, |ui| self.qc_ui(ui)); }
                Panel::Props => { egui::Window::new("Script properties").open(&mut open).show(ctx, |ui| self.props_ui(ui)); }
                Panel::Console => { egui::Window::new("Automation console").open(&mut open).default_width(640.0).default_height(300.0).show(ctx, |ui| self.console_ui(ui)); }
                Panel::Post => { egui::Window::new("Timing post-processor").open(&mut open).show(ctx, |ui| self.post_ui(ui)); }
                Panel::Resample => { egui::Window::new("Resample resolution").open(&mut open).show(ctx, |ui| self.resample_ui(ui)); }
                Panel::About => { egui::Window::new("About Yuzu").open(&mut open).show(ctx, |ui| {
                    ui.heading("Yuzu 柚子");
                    ui.label("Anime subtitle editor — Aegisub-compatible Lua automation, Subtitle Edit style tooling.");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                }); }
            }
            if !open { self.open.retain(|x| *x != p); }
        }
    }

    fn find_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("find").num_columns(2).show(ui, |ui| {
            ui.label("Find"); ui.text_edit_singleline(&mut self.find); ui.end_row();
            ui.label("Replace"); ui.text_edit_singleline(&mut self.repl); ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.find_opts.regex, "Regex");
            ui.checkbox(&mut self.find_opts.case_sensitive, "Match case");
            ui.checkbox(&mut self.find_opts.whole_word, "Whole word");
            ui.checkbox(&mut self.find_opts.skip_comments, "Skip comments");
        });
        ui.horizontal(|ui| {
            ui.label("In");
            egui::ComboBox::from_id_salt("ffield").selected_text(["Text (raw)", "Style", "Actor", "Effect", "Text (no tags)"][self.find_opts.field as usize]).show_ui(ui, |ui| {
                for (k, l) in ["Text (raw)", "Style", "Actor", "Effect", "Text (no tags)"].iter().enumerate() { ui.selectable_value(&mut self.find_opts.field, k as u8, *l); }
            });
        });
        ui.horizontal(|ui| {
            if ui.button("Find next").clicked() {
                match ops::find_all(&self.doc, &self.find, &self.find_opts) {
                    Ok(hits) => {
                        let next = hits.iter().copied().find(|&h| h > self.active).or(hits.first().copied());
                        match next { Some(h) => { self.select_only(h); self.scroll_to = Some(h); self.find_msg = format!("{} match(es)", hits.len()); } None => self.find_msg = "No matches".into() }
                    }
                    Err(e) => self.find_msg = e,
                }
            }
            if ui.button("Select all matches").clicked() {
                match ops::find_all(&self.doc, &self.find, &self.find_opts) {
                    Ok(hits) => { self.find_msg = format!("{} match(es)", hits.len()); if let Some(&f) = hits.first() { self.selection = hits.into_iter().collect(); self.active = f; self.scroll_to = Some(f); } }
                    Err(e) => self.find_msg = e,
                }
            }
            if ui.button("Replace all").clicked() {
                self.push_undo();
                let only = if self.selection.len() > 1 { Some(self.sel_vec()) } else { None };
                match ops::replace_all(&mut self.doc, &self.find, &self.repl, &self.find_opts, only.as_deref()) {
                    Ok(n) => { self.find_msg = format!("{} replacement(s)", n); self.run_qc(); }
                    Err(e) => self.find_msg = e,
                }
            }
        });
        ui.label(&self.find_msg);
        ui.weak("With several lines selected, Replace all applies to the selection only.");
    }

    fn shift_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Amount");
            ui.text_edit_singleline(&mut self.shift_ms);
            ui.radio_value(&mut self.shift_frames, false, "ms");
            ui.radio_value(&mut self.shift_frames, true, "frames");
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.shift_which, 0, "Start & end");
            ui.radio_value(&mut self.shift_which, 1, "Start only");
            ui.radio_value(&mut self.shift_which, 2, "End only");
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.shift_sel_only, true, "Selected lines");
            ui.radio_value(&mut self.shift_sel_only, false, "All lines");
            ui.radio_value(&mut self.shift_sel_only, false, "From selection onward").on_hover_text("shifts the selected line and everything after it");
        });
        if ui.button("Apply").clicked() {
            let v = if self.shift_frames { self.shift_ms.trim().parse::<i64>().ok() } else { time::parse_ass(&self.shift_ms.replace(',', ".")).or_else(|| self.shift_ms.trim().parse::<i64>().ok()) };
            if let Some(v) = v {
                self.push_undo();
                let idx: Vec<usize> = if self.shift_sel_only { self.sel_vec() } else { (0..self.doc.events.len()).collect() };
                if self.shift_frames { ops::shift_frames(&mut self.doc, &idx, v, fps_of(&self.settings)); } else { ops::shift(&mut self.doc, &idx, v, self.shift_which); }
                self.buf_key = (usize::MAX, 0, 0);
            }
        }
    }

    fn styles_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("sm").selected_text(self.doc.styles.get(self.style_sel).map(|s| s.name.clone()).unwrap_or_default()).show_ui(ui, |ui| {
                for (k, s) in self.doc.styles.iter().enumerate() { ui.selectable_value(&mut self.style_sel, k, &s.name); }
            });
            if ui.button("New").clicked() { self.push_undo(); let mut s = Style::default(); s.name = format!("Style{}", self.doc.styles.len() + 1); self.doc.styles.push(s); self.style_sel = self.doc.styles.len() - 1; }
            if ui.button("Copy").clicked() { if let Some(s) = self.doc.styles.get(self.style_sel).cloned() { self.push_undo(); let mut c = s; c.name.push_str(" copy"); self.doc.styles.push(c); self.style_sel = self.doc.styles.len() - 1; } }
            if ui.add_enabled(self.doc.styles.len() > 1, egui::Button::new("Delete")).clicked() { self.push_undo(); self.doc.styles.remove(self.style_sel); self.style_sel = 0; }
        });
        let sel = self.style_sel.min(self.doc.styles.len().saturating_sub(1));
        if self.doc.styles.is_empty() { return; }
        let mut s = self.doc.styles[sel].clone();
        let old_name = s.name.clone();
        egui::Grid::new("styled").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Name"); ui.text_edit_singleline(&mut s.name); ui.label("Font"); ui.text_edit_singleline(&mut s.fontname); ui.end_row();
            ui.label("Size"); ui.add(egui::DragValue::new(&mut s.fontsize).range(1.0..=999.0)); ui.label("Spacing"); ui.add(egui::DragValue::new(&mut s.spacing).speed(0.1)); ui.end_row();
            ui.label("Scale X/Y"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.scale_x)); ui.add(egui::DragValue::new(&mut s.scale_y)); }); ui.label("Angle"); ui.add(egui::DragValue::new(&mut s.angle)); ui.end_row();
            ui.label("Outline"); ui.add(egui::DragValue::new(&mut s.outline).speed(0.1).range(0.0..=99.0)); ui.label("Shadow"); ui.add(egui::DragValue::new(&mut s.shadow).speed(0.1).range(0.0..=99.0)); ui.end_row();
            ui.label("Alignment"); ui.add(egui::DragValue::new(&mut s.alignment).range(1..=9)); ui.label("Border style"); egui::ComboBox::from_id_salt("bs").selected_text(if s.border_style == 3 { "Opaque box" } else { "Outline + shadow" }).show_ui(ui, |ui| { ui.selectable_value(&mut s.border_style, 1, "Outline + shadow"); ui.selectable_value(&mut s.border_style, 3, "Opaque box"); }); ui.end_row();
            ui.label("Margins L/R/V"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.margin_l)); ui.add(egui::DragValue::new(&mut s.margin_r)); ui.add(egui::DragValue::new(&mut s.margin_v)); }); ui.label("Encoding"); ui.add(egui::DragValue::new(&mut s.encoding)); ui.end_row();
            ui.label("Style"); ui.horizontal(|ui| { ui.checkbox(&mut s.bold, "B"); ui.checkbox(&mut s.italic, "I"); ui.checkbox(&mut s.underline, "U"); ui.checkbox(&mut s.strikeout, "S"); }); ui.end_row();
            for (lbl, col) in [("Primary", &mut s.primary), ("Secondary", &mut s.secondary), ("Outline", &mut s.outline_color), ("Shadow", &mut s.back)] {
                ui.label(lbl);
                ui.horizontal(|ui| {
                    let mut rgb = [col.r, col.g, col.b];
                    if ui.color_edit_button_srgb(&mut rgb).changed() { col.r = rgb[0]; col.g = rgb[1]; col.b = rgb[2]; }
                    let mut a = 255 - col.a as i32;
                    ui.label("opacity"); if ui.add(egui::DragValue::new(&mut a).range(0..=255)).changed() { col.a = (255 - a) as u8; }
                });
                ui.end_row();
            }
        });
        if s != self.doc.styles[sel] {
            self.begin_edit(7_000_000 + sel as u64);
            if s.name != old_name {
                for e in self.doc.events.iter_mut().filter(|e| e.style == old_name) { e.style = s.name.clone(); }
            }
            self.doc.styles[sel] = s;
        }
    }

    fn qc_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Max CPS"); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_cps).range(5.0..=60.0));
            ui.label("Max line length"); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_cpl).range(10..=200));
            ui.label("Min dur (ms)"); ui.add(egui::DragValue::new(&mut self.qc_cfg.min_duration_ms).range(0..=5000));
            ui.label("Max dur (ms)"); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_duration_ms).range(500..=60000));
            if ui.button("Run").clicked() { self.settings.max_cps = self.qc_cfg.max_cps; self.settings.max_cpl = self.qc_cfg.max_cpl; self.settings.save(); self.run_qc(); }
        });
        ui.separator();
        let mut goto = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for is in &self.qc_issues {
                let (c, tag) = match is.severity { ops::Severity::Error => (Color32::from_rgb(255, 90, 90), "ERR"), ops::Severity::Warning => (Color32::from_rgb(235, 190, 80), "WARN"), _ => (Color32::GRAY, "info") };
                ui.horizontal(|ui| {
                    if ui.link(format!("#{}", is.index + 1)).clicked() { goto = Some(is.index); }
                    ui.colored_label(c, tag);
                    ui.label(&is.message);
                });
            }
            if self.qc_issues.is_empty() { ui.label("No issues found."); }
        });
        if let Some(g) = goto { self.select_only(g); self.scroll_to = Some(g); }
    }

    fn props_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        egui::Grid::new("props").num_columns(2).show(ui, |ui| {
            for key in ["Title", "Original Script", "Original Translation", "Original Editing", "Original Timing", "Synch Point", "Script Updated By", "Update Details", "PlayResX", "PlayResY", "WrapStyle", "ScaledBorderAndShadow", "YCbCr Matrix"] {
                let mut v = self.doc.info_get(key).unwrap_or("").to_string();
                ui.label(key);
                if ui.text_edit_singleline(&mut v).changed() { changed = true; self.doc.info_set(key, &v); }
                ui.end_row();
            }
        });
        if changed { self.dirty = true; }
    }

    fn console_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("Clear").clicked() { self.console.clear(); }
        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for (l, s) in &self.console {
                let c = match l { 0 | 1 => Color32::from_rgb(255, 120, 120), 2 => Color32::from_rgb(235, 190, 80), _ => ui.visuals().text_color() };
                ui.add(egui::Label::new(RichText::new(s.trim_end()).monospace().color(c)));
            }
        });
    }

    fn post_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("pp").num_columns(2).show(ui, |ui| {
            ui.label("Lead-in (ms)"); ui.add(egui::DragValue::new(&mut self.pp.lead_in).range(0..=2000)); ui.end_row();
            ui.label("Lead-out (ms)"); ui.add(egui::DragValue::new(&mut self.pp.lead_out).range(0..=2000)); ui.end_row();
            ui.label("Join gaps up to (ms)"); ui.add(egui::DragValue::new(&mut self.pp.join_gap_threshold).range(0..=2000)); ui.end_row();
            ui.label("Minimum gap (ms)"); ui.add(egui::DragValue::new(&mut self.pp.min_gap).range(0..=500)); ui.end_row();
        });
        if ui.button("Apply to selected lines").clicked() {
            self.push_undo();
            let idx = self.sel_vec();
            ops::post_process(&mut self.doc, &idx, &self.pp, &[]);
            self.buf_key = (usize::MAX, 0, 0);
        }
    }

    fn resample_ui(&mut self, ui: &mut egui::Ui) {
        let (cx, cy) = self.doc.res();
        ui.label(format!("Current: {}×{}", cx, cy));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.resample_xy.0).range(16..=16384));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut self.resample_xy.1).range(16..=16384));
            for (w, h) in [(1280, 720), (1920, 1080), (3840, 2160)] { if ui.small_button(format!("{}p", h)).clicked() { self.resample_xy = (w, h); } }
        });
        if ui.button("Resample").clicked() {
            self.push_undo();
            ops::resample(&mut self.doc, self.resample_xy.0, self.resample_xy.1);
        }
    }
}

// ---------------------------------------------------------------- media integration
impl App {
    fn fps(&self) -> time::Fps { fps_of(&self.settings) }

    fn goto_ms(&mut self, ms: i64) {
        self.media.pos_ms = ms.max(0);
        self.media.stop();
    }

    fn step_frame(&mut self, n: i64) {
        let f = self.fps();
        let cur = f.frame_from_ms(self.media.pos_ms);
        self.media.stop();
        self.media.pos_ms = f.start_ms((cur + n).max(0));
    }

    fn seek_rel(&mut self, d: i64) {
        self.media.stop();
        self.media.pos_ms = (self.media.pos_ms + d).max(0);
    }

    fn toggle_play(&mut self) {
        if self.media.playing { self.media.stop(); return; }
        let ass = self.ass_from(self.media.pos_ms, None);
        self.media.play(ass, None, 800);
    }

    fn play_active_line(&mut self, pre: i64, post: i64) {
        let Some(e) = self.doc.events.get(self.active).cloned() else { return };
        let (start, end) = if pre < 0 { ((e.end + pre).max(e.start.min(e.end)), e.end + post) } else { (e.start, e.end + post) };
        self.media.stop();
        self.media.pos_ms = start.max(0);
        let ass = self.ass_from(start, None);
        self.media.play(ass, Some(end), 800);
    }

    fn set_line_time_from_pos(&mut self, start: bool) {
        if self.active >= self.doc.events.len() { return; }
        self.begin_edit(0);
        let t = self.media.pos_ms;
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
                self.last_status = format!("Video: {}×{} @ {:.3} fps, {:.1}s", i.width, i.height, i.fps_num as f64 / i.fps_den as f64, i.duration_ms as f64 / 1000.0);
            }
        }
        // follow active line
        if self.last_active_seen != self.active {
            self.last_active_seen = self.active;
            if self.media.follow && (self.media.video.is_some() || self.media.audio.is_some()) {
                if let Some(e) = self.doc.events.get(self.active) {
                    if !self.media.playing { self.media.pos_ms = e.start; }
                    let w = 900.0 * self.media.ms_per_px;
                    if (e.start as f64) < self.media.view_start || (e.end as f64) > self.media.view_start + w { self.media.view_start = e.start as f64 - w * 0.2; }
                }
            }
        }
        // refresh still frame when position or visible subtitle content changes
        if self.media.info.as_ref().map(|i| i.has_video).unwrap_or(false) && !self.media.playing {
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
        if let Some(t) = &self.media.texture {
            let sz = t.size_vec2();
            let h = avail * sz.y / sz.x;
            let max_h = (ui.available_height() - 90.0).max(120.0);
            let (w, h) = if h > max_h { (max_h * sz.x / sz.y, max_h) } else { (avail, h) };
            ui.image((t.id(), egui::Vec2::new(w, h)));
        } else {
            ui.label(if self.media.pending { "Rendering frame…" } else { "No frame" });
        }
        if let Some(e) = &self.media.error { ui.colored_label(Color32::from_rgb(255, 120, 120), e); }
        let dur = self.media.duration().max(1);
        ui.horizontal(|ui| {
            if ui.button("⏮").on_hover_text("Previous frame (Ctrl+←)").clicked() { self.step_frame(-1); }
            if ui.button(if self.media.playing { "⏸" } else { "▶" }).on_hover_text("Play / pause (Ctrl+Space)").clicked() { self.toggle_play(); }
            if ui.button("⏭").on_hover_text("Next frame (Ctrl+→)").clicked() { self.step_frame(1); }
            ui.label(format!("{}  ·  frame {}", time::format_ass(self.media.pos_ms), fps.frame_from_ms(self.media.pos_ms)));
        });
        let mut pos = self.media.pos_ms as f64;
        ui.spacing_mut().slider_width = (avail - 20.0).max(100.0);
        if ui.add(egui::Slider::new(&mut pos, 0.0..=dur as f64).show_value(false)).changed() {
            self.media.stop();
            self.media.pos_ms = pos as i64;
        }
    }

    fn wave_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.small_button("Start=⏱ F1").on_hover_text("Set line start to the playhead").clicked() { self.set_line_time_from_pos(true); }
            if ui.small_button("End=⏱ F2").on_hover_text("Set line end to the playhead").clicked() { self.set_line_time_from_pos(false); }
            if ui.small_button("▶ line F3").clicked() { self.play_active_line(0, 0); }
            if ui.small_button("▶ end F4").clicked() { self.play_active_line(-500, 500); }
            if ui.small_button("Stop").clicked() { self.media.stop(); }
            ui.separator();
            ui.label("Zoom");
            let mut z = (self.media.ms_per_px).ln();
            if ui.add(egui::Slider::new(&mut z, 0.5_f64.ln()..=400.0_f64.ln()).show_value(false)).changed() { self.media.ms_per_px = z.exp(); }
            if self.media.peaks_loading { ui.spinner(); ui.label("decoding audio…"); }
            if !yuzu_media::ffmpeg_available() { ui.colored_label(Color32::from_rgb(255, 160, 80), "ffmpeg/ffprobe not found in PATH"); }
        });
        let spans: Vec<(i64, i64, String, bool, bool)> = {
            let (a, b) = (self.media.view_start as i64 - 10_000, (self.media.view_start + (ui.available_width() as f64 * self.media.ms_per_px)) as i64 + 10_000);
            self.doc.events.iter().enumerate().filter(|(_, e)| e.end >= a && e.start <= b).map(|(i, e)| (e.start, e.end, tags::display_text(&e.text).replace('\n', " "), i == self.active, self.selection.contains(&i))).collect()
        };
        let spans_ref: Vec<LineSpan> = spans.iter().map(|(s, e, l, a, sel)| LineSpan { start: *s, end: *e, label: l, active: *a, selected: *sel }).collect();
        let h = ui.available_height().max(60.0);
        let mut drag = self.wave_drag;
        let ev = media::waveform(ui, &mut self.media, &spans_ref, h, &mut drag);
        self.wave_drag = drag;
        let i = self.active;
        if ev.drag_begin { self.last_edit_key = 0; self.begin_edit(8_000_000 + i as u64); }
        if let Some(t) = ev.set_start { if i < self.doc.events.len() { if !ev.drag_begin && self.wave_drag.is_none() { self.begin_edit(0); } self.doc.events[i].start = t; self.buf_key = (usize::MAX, 0, 0); } }
        if let Some(t) = ev.set_end { if i < self.doc.events.len() { if self.wave_drag.is_none() { self.begin_edit(0); } self.doc.events[i].end = t; self.buf_key = (usize::MAX, 0, 0); } }
        if let Some(t) = ev.seek { self.media.stop(); self.media.pos_ms = t; }
        if ev.drag_end { self.last_edit_key = 0; }
    }
}
