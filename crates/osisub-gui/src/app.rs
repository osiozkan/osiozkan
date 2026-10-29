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
    export_open: bool,
    export_sel: Vec<usize>,
    export_job: Option<ExportJob>,
    tap: Option<TapState>,
    tap_latency_ms: i64,
    backed_up: std::collections::HashSet<PathBuf>,
    theme_dirty: bool,
    goto_open: bool,
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
    let base = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).ok().or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("osisub/automation/autoload"))
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

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: crate::Args) -> App {
        let crate::Args { file, scripts, screenshot: shot, tab, media, open_win, line, reference } = args;
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
            tab: match tab.as_deref() { Some("quality") => Tab::Quality, Some("karaoke") => Tab::Karaoke, Some("spell") => Tab::Spell, Some("automation") => Tab::Automation, Some("info") => Tab::Info, _ => Tab::Styles },
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
            export_open: false,
            export_sel: vec![],
            export_job: None,
            tap: None,
            tap_latency_ms: 200,
            backed_up: Default::default(),
            theme_dirty: true,
            goto_open: false,
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
            }
        }
        match open_win.as_deref() {
            Some("find") => app.find_open = true,
            Some("timing") => app.timing_open = true,
            Some("resample") => app.resample_open = true,
            Some("fixes") => app.fixes_open = true,
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

    fn user_dict_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config/osisub/dict"))
    }

    fn user_words_path() -> Option<PathBuf> {
        Self::user_dict_dir().map(|d| d.join("kullanici_sozcukleri.txt"))
    }

    fn auto_load_dictionary(&mut self) {
        let (aff, dic) = (PathBuf::from(&self.settings.dict_aff), PathBuf::from(&self.settings.dict_dic));
        if aff.is_file() && dic.is_file() {
            self.load_dictionary(aff, dic);
        } else if let Some((a, d)) = osisub_spell::find_dictionary("tr_TR", &Self::user_dict_dir().into_iter().collect::<Vec<_>>()) {
            self.load_dictionary(a, d);
        }
    }

    fn load_dictionary(&mut self, aff: PathBuf, dic: PathBuf) {
        self.settings.dict_aff = aff.to_string_lossy().to_string();
        self.settings.dict_dic = dic.to_string_lossy().to_string();
        self.spell_status = "Sözlük yükleniyor…".into();
        let (tx, rx) = std::sync::mpsc::channel();
        self.spell_load_rx = Some(rx);
        std::thread::spawn(move || {
            let r = osisub_spell::Speller::load(&aff, &dic).map(|mut s| {
                if let Some(p) = Self::user_words_path() {
                    if let Ok(t) = std::fs::read_to_string(p) { for w in t.lines().filter(|l| !l.trim().is_empty()) { s.add_word(w.trim()); } }
                }
                s
            });
            let _ = tx.send(r);
        });
    }

    fn pump_spell(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.spell_load_rx {
            match rx.try_recv() {
                Ok(Ok(s)) => { self.spell_status = format!("Sözlük yüklü: {}", s.name); self.spell = Some(std::sync::Arc::new(s)); self.spell_load_rx = None; }
                Ok(Err(e)) => { self.spell_status = format!("Sözlük yüklenemedi: {}", e); self.spell_load_rx = None; }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            }
        }
        if let Some(rx) = &self.miss_rx {
            if let Ok(m) = rx.try_recv() { self.misses = m; self.miss_rx = None; self.miss_sel = None; self.suggestions.clear(); }
            else { ctx.request_repaint_after(std::time::Duration::from_millis(150)); }
        }
    }

    fn start_spellcheck(&mut self) {
        let Some(sp) = self.spell.clone() else { return };
        let script = self.doc.script.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.miss_rx = Some(rx);
        let ignore = self.spell_ignore.clone();
        std::thread::spawn(move || {
            let m: Vec<_> = osisub_spell::check_script(&sp, &script, None).into_iter().filter(|m| !ignore.contains(&osisub_core::qc::tr_lower(&m.word))).collect();
            let _ = tx.send(m);
        });
    }

    fn spell_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(if self.spell_status.is_empty() { "Sözlük yüklü değil".to_string() } else { self.spell_status.clone() });
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Sözlük seç (.dic)…").clicked() {
                if let Some(dic) = rfd::FileDialog::new().add_filter("Hunspell sözlüğü", &["dic"]).pick_file() {
                    let aff = dic.with_extension("aff");
                    if aff.is_file() { self.load_dictionary(aff, dic); } else { self.spell_status = "Aynı klasörde .aff dosyası yok".into(); }
                }
            }
            if ui.button("tr_TR indir").on_hover_text("LibreOffice sözlüklerinden ~/.config/osisub/dict içine indirir (curl gerekir)").clicked() {
                if let Some(dir) = Self::user_dict_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    let ok = ["tr_TR.aff", "tr_TR.dic"].iter().all(|f| std::process::Command::new("curl").args(["-sSfL", "-o"]).arg(dir.join(f)).arg(format!("https://raw.githubusercontent.com/LibreOffice/dictionaries/master/tr_TR/{}", f)).status().map(|s| s.success()).unwrap_or(false));
                    if ok { self.load_dictionary(dir.join("tr_TR.aff"), dir.join("tr_TR.dic")); } else { self.spell_status = "İndirme başarısız (curl/ağ)".into(); }
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.spell.is_some() && self.miss_rx.is_none(), egui::Button::new("Denetle")).clicked() { self.start_spellcheck(); }
            if self.miss_rx.is_some() { ui.spinner(); }
            ui.label(RichText::new(format!("{} hata", self.misses.len())).strong());
        });
        ui.separator();
        let misses = self.misses.clone();
        let mut jump: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("misses").max_height(220.0).show_rows(ui, 20.0, misses.len(), |ui, range| {
            for i in range {
                let m = &misses[i];
                if ui.selectable_label(self.miss_sel == Some(i), format!("{}  (satır {})", m.word, self.rows.iter().position(|&r| r == m.entry).map(|p| p + 1).unwrap_or(m.entry + 1))).clicked() { jump = Some(i); }
            }
        });
        if let Some(i) = jump {
            self.miss_sel = Some(i);
            let e = misses[i].entry;
            self.doc.select(vec![e], e);
            self.scroll_to = Some(e);
            self.suggestions = self.spell.as_ref().map(|s| s.suggest(&misses[i].word, 8)).unwrap_or_default();
        }
        if let Some(i) = self.miss_sel.filter(|&i| i < misses.len()) {
            let m = misses[i].clone();
            ui.separator();
            ui.label(RichText::new(&m.word).strong());
            ui.horizontal_wrapped(|ui| {
                for sug in self.suggestions.clone() {
                    if ui.button(&sug).clicked() {
                        let at = m.entry;
                        if let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() {
                            let nt = osisub_spell::replace_word(&d.text, &m.word, &sug, false);
                            self.doc.edit("Yazım düzelt", |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
                        }
                        self.misses.remove(i);
                        self.miss_sel = None;
                    }
                }
                if self.suggestions.is_empty() { ui.label(RichText::new("öneri yok").weak()); }
            });
            ui.horizontal(|ui| {
                if ui.button("Yoksay").clicked() {
                    let w = osisub_core::qc::tr_lower(&m.word);
                    self.spell_ignore.insert(w.clone());
                    self.misses.retain(|x| osisub_core::qc::tr_lower(&x.word) != w);
                    self.miss_sel = None;
                }
                if ui.button("Sözlüğe ekle").clicked() {
                    if let Some(p) = Self::user_words_path() {
                        if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
                        let mut t = std::fs::read_to_string(&p).unwrap_or_default();
                        t.push_str(&format!("{}\n", m.word));
                        let _ = std::fs::write(&p, t);
                    }
                    let w = osisub_core::qc::tr_lower(&m.word);
                    self.spell_ignore.insert(w.clone());
                    self.misses.retain(|x| osisub_core::qc::tr_lower(&x.word) != w);
                    self.miss_sel = None;
                }
            });
        }
    }

    fn catalog_path() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config/osisub/styles.ass"))
    }

    fn catalog_styles() -> Vec<Style> {
        Self::catalog_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| osisub_core::Script::parse(&t).styles().cloned().collect()).unwrap_or_default()
    }

    fn save_catalog(styles: &[Style]) {
        if let Some(p) = Self::catalog_path() {
            if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
            let mut s = osisub_core::Script::new_default();
            s.entries.retain(|e| !matches!(e, Entry::Style(_)));
            s.entries.extend(styles.iter().cloned().map(Entry::Style));
            let _ = std::fs::write(p, s.to_ass_string());
        }
    }

    fn add_styles(&mut self, list: Vec<Style>) {
        if list.is_empty() { return; }
        let at = self.doc.script.entries.iter().rposition(|e| matches!(e, Entry::Style(_))).map(|i| i + 1).unwrap_or(self.doc.script.entries.len());
        let mut items = vec![];
        let mut taken: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        for mut st in list {
            if taken.contains(&st.name) {
                let base = st.name.clone();
                st.name = (2..).map(|n| format!("{} ({})", base, n)).find(|n| !taken.contains(n)).unwrap();
            }
            taken.push(st.name.clone());
            items.push(Entry::Style(st));
        }
        let n = items.len();
        self.doc.edit("Stil ekle", |tx| tx.insert(at, items));
        self.status = format!("{} stil eklendi", n);
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        let Some(list) = self.import_styles.as_mut() else { return };
        let mut done: Option<bool> = None;
        egui::Window::new("Stilleri içe aktar").collapsible(false).show(ctx, |ui| {
            for (st, on) in list.iter_mut() { ui.checkbox(on, &st.name); }
            ui.horizontal(|ui| {
                if ui.button("İçe aktar").clicked() { done = Some(true); }
                if ui.button("İptal").clicked() { done = Some(false); }
            });
        });
        if let Some(go) = done {
            let chosen: Vec<Style> = self.import_styles.take().unwrap_or_default().into_iter().filter(|(_, on)| *on && go).map(|(s, _)| s).collect();
            self.add_styles(chosen);
        }
    }

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

    fn export_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_open;
        egui::Window::new("Dışa aktar — filtreler").open(&mut open).collapsible(false).show(ctx, |ui| {
            let mut filters: Vec<(usize, String, String, i64)> = self.macros.iter().filter(|m| m.info.kind == MacroKind::Filter).map(|m| (m.info.id, m.info.name.clone(), m.info.description.clone(), m.info.priority)).collect();
            filters.sort_by_key(|f| -f.3);
            if filters.is_empty() { ui.label(RichText::new("Yüklü filtre yok (Lua: aegisub.register_filter)").weak()); }
            for (id, name, desc, prio) in &filters {
                let mut on = self.export_sel.contains(id);
                if ui.checkbox(&mut on, format!("{}  (öncelik {})", name, prio)).on_hover_text(desc).changed() {
                    if on { self.export_sel.push(*id); } else { self.export_sel.retain(|x| x != id); }
                }
            }
            ui.label(RichText::new("Seçili filtreler öncelik sırasıyla (büyükten küçüğe) çalışır; belge değişmez, sonuç dosyaya yazılır.").weak().small());
            if ui.add_enabled(self.running.is_none(), egui::Button::new("Dışa aktar…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).add_filter("Düz metin", &["txt"]).save_file() {
                    let mut queue: Vec<usize> = filters.iter().filter(|f| self.export_sel.contains(&f.0)).map(|f| f.0).collect();
                    if queue.is_empty() {
                        match osisub_core::io::save(&self.doc.script, &p) { Ok(()) => self.status = format!("Dışa aktarıldı: {}", p.display()), Err(e) => self.status = format!("Hata: {}", e) }
                    } else {
                        let first = queue.remove(0);
                        self.export_job = Some(ExportJob { queue, script: self.doc.script.clone(), path: p });
                        self.run_export_step(first);
                    }
                }
            }
        });
        self.export_open = open;
    }

    fn run_export_step(&mut self, id: usize) {
        let Some(script) = self.export_job.as_ref().map(|j| j.script.clone()) else { return };
        let name = self.macros.iter().find(|m| m.info.id == id).map(|m| m.info.name.clone()).unwrap_or_default();
        self.running = Some(name);
        self.sync_media_state();
        self.worker.send(Cmd::Run { id, script, selection: vec![], active: 0, props: vec![] });
    }

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
        self.worker.send(Cmd::Run { id, script: self.doc.script.clone(), selection: self.doc.selection.clone(), active: self.doc.active, props });
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
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::H))) { self.find_open = true; }
        let (alt_up, alt_down) = ctx.input_mut(|i| (i.consume_key(Modifiers::ALT, Key::ArrowUp), i.consume_key(Modifiers::ALT, Key::ArrowDown)));
        if (alt_up || alt_down) && !self.rows.is_empty() {
            let pos = self.rows.iter().position(|&r| r == self.doc.active).unwrap_or(0);
            let np = if alt_up { pos.saturating_sub(1) } else { (pos + 1).min(self.rows.len() - 1) };
            let r = self.rows[np];
            self.doc.select(vec![r], r);
            self.anchor = np;
            self.scroll_to = Some(r);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::G))) { self.goto_open = true; }
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
                if ui.add(egui::Button::new("Zamana git…").shortcut_text("Ctrl+G")).clicked() { self.goto_open = true; ui.close(); }
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
                if ui.add(egui::Button::new("Bul / Değiştir…").shortcut_text("Ctrl+F")).clicked() { self.find_open = true; ui.close(); }
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

    fn highlighted(text: &str, dim: bool) -> LayoutJob {
        let mut job = LayoutJob::default();
        let base = if dim { Color32::from_gray(120) } else { Color32::from_gray(225) };
        for p in osisub_core::tags::split(text) {
            match p {
                osisub_core::tags::Part::Block(b) => job.append(&format!("{{{}}}", b), 0.0, TextFormat { color: theme::TAG_COLOR, font_id: egui::FontId::monospace(11.0), ..Default::default() }),
                osisub_core::tags::Part::Text(t) => job.append(&t.replace("\\N", " ⏎ ").replace("\\n", " ⏎ "), 0.0, TextFormat { color: base, ..Default::default() }),
            }
        }
        job
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Ara:");
            ui.add(TextEdit::singleline(&mut self.filter).hint_text("Süz (metin, stil, konuşan)…").desired_width(260.0));
            if !self.filter.is_empty() && ui.small_button("x").clicked() { self.filter.clear(); }
            ui.label(RichText::new(format!("{} / {} satır", self.rows.len(), self.doc.script.dialogues().count())).weak());
        });
        let mods = ui.input(|i| i.modifiers);
        let has_ref = !self.ref_lines.is_empty();
        let avail_w = ui.available_width();
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(Align::Center))
            .column(Column::exact(44.0))
            .column(Column::exact(86.0))
            .column(Column::exact(86.0))
            .column(Column::exact(52.0))
            .column(Column::exact(44.0))
            .column(Column::initial(90.0).clip(true))
            .column(Column::initial(80.0).clip(true));
        if has_ref {
            // Sabit sütunlar ≈ 482 px; kalanı Referans ve Metin eşit paylaşsın.
            let w = ((avail_w - 482.0 - 16.0) / 2.0).max(120.0);
            table = table.column(Column::initial(w).clip(true).resizable(true));
        }
        table = table.column(Column::remainder().clip(true));
        if let Some(target) = self.scroll_to.take() {
            if let Some(pos) = self.rows.iter().position(|&r| r == target) {
                table = table.scroll_to_row(pos, Some(Align::Center));
            }
        }
        let rows = self.rows.clone();
        let mut clicked: Option<usize> = None;
        let mut action: Option<RowAction> = None;
        let style_names: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        let sel_snapshot = self.doc.selection.clone();
        let mut ctx_row: Option<usize> = None;
        table
            .header(22.0, |mut h| {
                let titles: Vec<&str> = if has_ref { vec!["#", "Başlangıç", "Bitiş", "Süre", "KSS", "Stil", "Konuşan", "Referans", "Metin"] } else { vec!["#", "Başlangıç", "Bitiş", "Süre", "KSS", "Stil", "Konuşan", "Metin"] };
                for t in titles {
                    h.col(|ui| { ui.strong(t); });
                }
            })
            .body(|body| {
                body.rows(24.0, rows.len(), |mut row| {
                    let pos = row.index();
                    let ei = rows[pos];
                    let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(ei) else { return };
                    let selected = self.doc.selection.binary_search(&ei).is_ok();
                    row.set_selected(selected);
                    let flags = self.issue_map.get(&ei);
                    let cps = qc::cps(d);
                    let dim = d.comment;
                    let col = |s: String, c: Option<Color32>| {
                        let mut t = RichText::new(s).monospace();
                        if let Some(c) = c { t = t.color(c); } else if dim { t = t.color(Color32::from_gray(120)); }
                        t
                    };
                    let overlap = flags.map_or(false, |f| f.contains(&Kind::Overlap));
                    row.col(|ui| { ui.label(col((pos + 1).to_string(), None)); });
                    row.col(|ui| { ui.label(col(time::format_ass(d.start_time), if overlap { Some(theme::BAD) } else { None })); });
                    row.col(|ui| { ui.label(col(time::format_ass(d.end_time), None)); });
                    row.col(|ui| { ui.label(col(format!("{:.2}", d.duration() as f64 / 1000.0), flags.and_then(|f| f.iter().find(|k| matches!(k, Kind::TooShort | Kind::TooLong | Kind::NegativeDuration)).map(|_| theme::WARN)))); });
                    row.col(|ui| { ui.label(col(if cps.is_finite() { format!("{:.0}", cps) } else { "∞".into() }, flags.and_then(|f| f.contains(&Kind::ReadingSpeed).then_some(theme::BAD)))); });
                    row.col(|ui| { ui.label(RichText::new(&d.style).color(if flags.map_or(false, |f| f.contains(&Kind::UnknownStyle)) { theme::BAD } else { Color32::from_gray(180) })); });
                    row.col(|ui| { ui.label(RichText::new(&d.actor).weak()); });
                    if has_ref {
                        let refs = &self.ref_lines;
                        let end = refs.partition_point(|r| r.0 < d.end_time);
                        let best = refs[..end].iter().rev().take(8).filter(|r| r.1 > d.start_time).max_by_key(|r| r.1.min(d.end_time) - r.0.max(d.start_time));
                        row.col(|ui| { if let Some(r) = best { ui.add(egui::Label::new(RichText::new(&r.2).italics().color(theme::ACCENT)).truncate()); } });
                    }
                    row.col(|ui| { ui.add(egui::Label::new(Self::highlighted(&d.text, dim)).truncate()); });
                    let resp = row.response();
                    if resp.clicked() { clicked = Some(pos); }
                    if resp.secondary_clicked() && sel_snapshot.binary_search(&ei).is_err() { ctx_row = Some(ei); }
                    resp.context_menu(|ui| {
                        if ui.button("Önce satır ekle").clicked() { action = Some(RowAction::InsertBefore); ui.close(); }
                        if ui.button("Sonra satır ekle").clicked() { action = Some(RowAction::InsertAfter); ui.close(); }
                        if ui.button("Çoğalt").clicked() { action = Some(RowAction::Duplicate); ui.close(); }
                        if ui.button("Sil").clicked() { action = Some(RowAction::Delete); ui.close(); }
                        if ui.button("Birleştir").clicked() { action = Some(RowAction::Merge); ui.close(); }
                        if ui.button("Yorum yap / kaldır").clicked() { action = Some(RowAction::ToggleComment); ui.close(); }
                        ui.separator();
                        if ui.button("Kopyala").clicked() { action = Some(RowAction::Copy); ui.close(); }
                        if ui.button("Kes").clicked() { action = Some(RowAction::Cut); ui.close(); }
                        if ui.button("Yapıştır (pano)").clicked() { action = Some(RowAction::Paste); ui.close(); }
                        ui.menu_button("Stil", |ui| {
                            for n in &style_names { if ui.button(n).clicked() { action = Some(RowAction::SetStyle(n.clone())); ui.close(); } }
                        });
                    });
                });
            });
        if let Some(ei) = ctx_row { self.doc.select(vec![ei], ei); }
        if let Some(a) = action {
            match a {
                RowAction::InsertBefore => { self.doc.insert_before_active(); }
                RowAction::InsertAfter => { self.doc.insert_after_active(); }
                RowAction::Duplicate => self.doc.duplicate_selected(),
                RowAction::Delete => self.doc.delete_selected(),
                RowAction::ToggleComment => self.doc.toggle_comment_selected(),
                RowAction::Merge => { self.doc.merge_selected(); }
                RowAction::Copy => { let t = self.doc.copy_lines(); ui.ctx().copy_text(t); }
                RowAction::Cut => { let t = self.doc.cut_lines(); ui.ctx().copy_text(t); }
                RowAction::Paste => self.status = "Yapıştırmak için Ctrl+V kullanın".into(),
                RowAction::SetStyle(n) => self.doc.set_style_selected(&n),
            }
            self.scroll_to = Some(self.doc.active);
        }
        if let Some(pos) = clicked {
            let ei = rows[pos];
            if mods.shift {
                let (a, b) = (self.anchor.min(pos), self.anchor.max(pos));
                self.doc.select(rows[a..=b].to_vec(), ei);
            } else if mods.command {
                let mut s = self.doc.selection.clone();
                match s.binary_search(&ei) { Ok(i) => { s.remove(i); } Err(i) => s.insert(i, ei) }
                self.doc.select(s, ei);
                self.anchor = pos;
            } else {
                self.doc.select(vec![ei], ei);
                self.anchor = pos;
            }
        }
    }

    fn time_field(ui: &mut egui::Ui, id: &str, buf: &mut String, current: i64) -> Option<i64> {
        let eid = egui::Id::unique(id);
        let focused = ui.memory(|m| m.has_focus(eid));
        if !focused { *buf = time::format_ass(current); }
        let r = ui.add(TextEdit::singleline(buf).id(eid).desired_width(92.0).font(egui::TextStyle::Monospace));
        if r.changed() { time::parse_ass(buf) } else { None }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let at = self.doc.active;
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() else {
            ui.label(RichText::new("Düzenlemek için bir satır seçin").weak());
            return;
        };
        let styles: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        let mut nd = d.clone();
        ui.horizontal(|ui| {
            ui.checkbox(&mut nd.comment, "Yorum");
            ui.label("Katman");
            ui.add(egui::DragValue::new(&mut nd.layer));
            ui.label("Başlangıç");
            if let Some(t) = Self::time_field(ui, "start_f", &mut self.start_buf, d.start_time) { nd.start_time = t; }
            ui.label("Bitiş");
            if let Some(t) = Self::time_field(ui, "end_f", &mut self.end_buf, d.end_time) { nd.end_time = t; }
            ui.label(RichText::new(format!("{:.3} sn", d.duration() as f64 / 1000.0)).monospace());
            ui.label("Stil");
            egui::ComboBox::from_id_salt("style_cb").selected_text(nd.style.clone()).show_ui(ui, |ui| {
                for s in &styles { ui.selectable_value(&mut nd.style, s.clone(), s); }
            });
            ui.label("Konuşan");
            ui.add(TextEdit::singleline(&mut nd.actor).desired_width(90.0));
        });
        ui.horizontal(|ui| {
            ui.label("Kenar boşluğu");
            ui.add(egui::DragValue::new(&mut nd.margin_l));
            ui.add(egui::DragValue::new(&mut nd.margin_r));
            let mut v = nd.margin_t;
            if ui.add(egui::DragValue::new(&mut v)).changed() { nd.margin_t = v; nd.margin_b = v; }
            ui.label("Efekt");
            ui.add(TextEdit::singleline(&mut nd.effect).desired_width(120.0));
        });
        let sp = self.spell.clone();
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = editor_job(buf.as_str(), sp.as_deref());
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        ui.add(TextEdit::multiline(&mut nd.text).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).layouter(&mut layouter).hint_text("Altyazı metni — satır sonu için \\N"));
        let cps = qc::cps(&nd);
        let vis = osisub_core::tags::visible_text(&nd.text);
        let longest = osisub_core::tags::max_line_len(&nd.text);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} karakter", osisub_core::tags::visible_len(&nd.text))).weak());
            ui.label(RichText::new(format!("{} satır", vis.lines().count().max(1))).weak());
            ui.label(RichText::new(format!("en uzun satır {}", longest)).color(if longest > self.rules.max_line_len { theme::BAD } else { Color32::GRAY }));
            ui.label(RichText::new(format!("{} kar/sn", if cps.is_finite() { format!("{:.1}", cps) } else { "∞".into() })).color(if cps > self.rules.max_cps { theme::BAD } else { Color32::GRAY }));
        });
        if let Some(r) = &self.reference {
            let (a, b) = (d.start_time, d.end_time);
            let mut best: Vec<(i64, String)> = r.dialogues().filter(|x| !x.comment && x.end_time > a && x.start_time < b).map(|x| ((x.end_time.min(b) - x.start_time.max(a)), osisub_core::tags::visible_text(&x.text).replace('\n', " ⏎ "))).collect();
            best.sort_by_key(|x| -x.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Referans:").color(theme::ACCENT));
                if best.is_empty() { ui.label(RichText::new("(eşleşen satır yok)").weak()); }
                for (_, t) in best.iter().take(3) { ui.label(RichText::new(t).italics()); }
            });
        }
        if nd != d {
            let key = if nd.text != d.text { format!("text:{}", at) } else if nd.start_time != d.start_time || nd.end_time != d.end_time { format!("time:{}", at) } else { String::new() };
            let desc = if nd.text != d.text { "Metin düzenle" } else { "Satırı düzenle" };
            self.doc.edit_keyed(desc, if key.is_empty() { None } else { Some(&key) }, |tx| tx.replace(at, Entry::Dialogue(nd)));
        }
    }

    fn side(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (t, l) in [(Tab::Styles, "Stiller"), (Tab::Karaoke, "Karaoke"), (Tab::Spell, "Yazım"), (Tab::Quality, "Kalite"), (Tab::Automation, "Otomasyon"), (Tab::Info, "Bilgi")] {
                ui.selectable_value(&mut self.tab, t, l);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
            Tab::Styles => self.styles_tab(ui),
            Tab::Karaoke => self.karaoke_tab(ui),
            Tab::Spell => self.spell_tab(ui),
            Tab::Quality => self.quality_tab(ui),
            Tab::Automation => self.automation_tab(ui),
            Tab::Info => self.info_tab(ui),
        });
    }

    fn styles_tab(&mut self, ui: &mut egui::Ui) {
        let idxs: Vec<usize> = self.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Style(_))).map(|(i, _)| i).collect();
        if idxs.is_empty() { ui.label("Stil yok"); }
        self.style_sel = self.style_sel.min(idxs.len().saturating_sub(1));
        ui.horizontal_wrapped(|ui| {
            for (n, &i) in idxs.iter().enumerate() {
                if let Entry::Style(s) = &self.doc.script.entries[i] { ui.selectable_value(&mut self.style_sel, n, &s.name); }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Yeni").clicked() {
                let at = idxs.last().map_or(self.doc.script.entries.len(), |i| i + 1);
                let mut s = Style::default();
                s.name = format!("Stil{}", idxs.len() + 1);
                self.doc.edit("Stil ekle", |tx| tx.insert(at, vec![Entry::Style(s)]));
                self.style_sel = idxs.len();
            }
            if ui.button("Çoğalt").clicked() {
                if let Some(&i) = idxs.get(self.style_sel) {
                    if let Entry::Style(s) = self.doc.script.entries[i].clone() {
                        let mut c = s; c.name.push_str(" kopya");
                        self.doc.edit("Stil çoğalt", |tx| tx.insert(i + 1, vec![Entry::Style(c)]));
                        self.style_sel += 1;
                    }
                }
            }
            if ui.button("Kataloğa kaydet").on_hover_text("~/.config/osisub/styles.ass").clicked() {
                if let Some(&i) = idxs.get(self.style_sel) {
                    if let Entry::Style(st) = self.doc.script.entries[i].clone() {
                        let mut cat = Self::catalog_styles();
                        cat.retain(|c| c.name != st.name);
                        cat.push(st);
                        Self::save_catalog(&cat);
                        self.status = "Stil kataloğa kaydedildi".into();
                    }
                }
            }
            ui.menu_button("Katalogdan ekle", |ui| {
                let cat = Self::catalog_styles();
                if cat.is_empty() { ui.label(RichText::new("Katalog boş").weak()); }
                for c in cat { if ui.button(&c.name).clicked() { self.add_styles(vec![c]); ui.close(); } }
            });
            if ui.button("Dosyadan içe aktar…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass", "ssa"]).pick_file() {
                    match osisub_core::io::load(&p) {
                        Ok(sc) => self.import_styles = Some(sc.styles().cloned().map(|s| (s, true)).collect()),
                        Err(e) => self.status = format!("Hata: {}", e),
                    }
                }
            }
            if ui.button("Sil").clicked() && idxs.len() > 1 {
                if let Some(&i) = idxs.get(self.style_sel) { self.doc.edit("Stil sil", |tx| tx.remove(i, 1)); }
            }
        });
        ui.separator();
        if let Some(&i) = idxs.get(self.style_sel) {
            if let Entry::Style(s) = self.doc.script.entries[i].clone() {
                let mut ns = s.clone();
                if style_ui::edit(ui, &mut ns) && ns != s {
                    // Ad değişirse diyalogları da güncelle.
                    let rename = (ns.name != s.name).then(|| (s.name.clone(), ns.name.clone()));
                    let key = format!("style:{}", i);
                    self.doc.edit_keyed("Stil düzenle", Some(&key), |tx| {
                        tx.replace(i, Entry::Style(ns));
                        if let Some((old, new)) = rename {
                            let affected: Vec<usize> = tx.script().entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(d) if d.style == old)).map(|(k, _)| k).collect();
                            for k in affected { tx.dialogue_mut(k, |d| d.style = new.clone()); }
                        }
                    });
                }
            }
        }
    }

    fn start_tap(&mut self, ctx: &egui::Context) {
        if !self.media.is_open() { self.status = "Önce ses/video açın".into(); return; }
        if !matches!(self.doc.script.entries.get(self.doc.active), Some(Entry::Dialogue(_))) { return; }
        if self.media.is_playing() { self.media.toggle_play(&self.doc, ctx); }
        self.tap = Some(TapState { line: self.doc.active, taps: vec![] });
        self.media.toggle_play(&self.doc, ctx);
        self.status = "Tuşla zamanlama: her hecenin başlangıcında Boşluk'a basın".into();
    }

    fn record_tap(&mut self) {
        let Some(t) = self.tap.as_mut() else { return };
        if !self.media.is_playing() { return; }
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(t.line) else { return };
        let ms = (self.media.playhead - d.start_time - self.tap_latency_ms).max(0);
        t.taps.push(ms);
        let n = osisub_core::karaoke::parse(&d.text).len();
        if t.taps.len() >= n { self.finish_tap(); }
    }

    fn finish_tap(&mut self) {
        use osisub_core::karaoke as k;
        let Some(t) = self.tap.take() else { return };
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(t.line).cloned() else { return };
        let syls = k::parse(&d.text);
        let n = syls.len().min(t.taps.len());
        if n == 0 { return; }
        let syls = k::apply_taps(&syls, &t.taps, d.duration() / 10);
        let text = k::build(&syls);
        let at = t.line;
        self.doc.edit("Tuşla karaoke zamanlama", |tx| tx.dialogue_mut(at, |dd| dd.text = text));
        self.status = format!("{} hece zamanlandı", n);
    }

    fn karaoke_tab(&mut self, ui: &mut egui::Ui) {
        use osisub_core::karaoke as k;
        let at = self.doc.active;
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() else {
            ui.label(RichText::new("Bir satır seçin").weak());
            return;
        };
        let line_cs = d.duration() / 10;
        let mut syls = k::parse(&d.text);
        let has_k = syls.iter().any(|s| s.dur > 0) || d.text.contains("\\k");
        let mut new_syls: Option<Vec<k::Syl>> = None;
        ui.horizontal_wrapped(|ui| {
            match &self.tap {
                None => {
                    if ui.add_enabled(self.media.is_open(), egui::Button::new("Tuşla zamanla (Boşluk)")).on_hover_text("Satırı oynatır; her hecenin başında Boşluk'a basın").clicked() { self.start_tap(ui.ctx()); }
                }
                Some(t) => {
                    ui.label(RichText::new(format!("Kayıt: {} / {}", t.taps.len(), syls.len())).color(theme::ACCENT));
                    if ui.button("Bitir").clicked() { self.finish_tap(); }
                    if ui.button("İptal").clicked() { self.tap = None; }
                }
            }
            ui.label("Gecikme (ms)");
            ui.add(egui::DragValue::new(&mut self.tap_latency_ms).range(0..=1000).speed(5)).on_hover_text("İnsan tepkisi + ses başlatma gecikmesi için düzeltme");
        });

        if !has_k {
            ui.label("Bu satırda karaoke etiketi yok.");
            if ui.button("Sözcüklere böl ve süreyi dağıt").clicked() {
                let plain = osisub_core::tags::strip_tags(&d.text);
                let mut v: Vec<k::Syl> = vec![];
                let mut cur = String::new();
                for ch in plain.chars() {
                    cur.push(ch);
                    if ch == ' ' { v.push(k::Syl { dur: 0, kind: "kf".into(), text: std::mem::take(&mut cur) }); }
                }
                if !cur.is_empty() { v.push(k::Syl { dur: 0, kind: "kf".into(), text: cur }); }
                k::distribute(&mut v, line_cs);
                new_syls = Some(v);
            }
        } else {
            let tot = k::total(&syls);
            ui.horizontal(|ui| {
                ui.label(format!("{} hece • toplam {} cs / satır {} cs", syls.len(), tot, line_cs));
                if tot != line_cs { ui.label(RichText::new("(süre uyuşmuyor)").color(theme::WARN)); }
            });
            ui.horizontal(|ui| {
                if ui.button("Süreye ölçekle").clicked() { let mut v = syls.clone(); k::scale_to(&mut v, line_cs); new_syls = Some(v); }
                if ui.button("Harfe göre dağıt").clicked() { let mut v = syls.clone(); k::distribute(&mut v, line_cs); new_syls = Some(v); }
                if ui.button("Etiketleri kaldır").clicked() { self.doc.edit("Karaoke etiketlerini kaldır", |tx| tx.dialogue_mut(at, |dd| { let v = k::parse(&dd.text); dd.text = v.iter().map(|s| s.text.clone()).collect(); })); }
            });
            // Çubuk
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::hover());
            let total = tot.max(1) as f32;
            let px_cs = width / total;
            let p = ui.painter_at(rect);
            let mut x = rect.left();
            let mut bounds: Vec<f32> = vec![];
            for (i, s) in syls.iter().enumerate() {
                let w = s.dur as f32 * px_cs;
                let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()));
                p.rect_filled(r.shrink(0.5), 3.0, if i % 2 == 0 { Color32::from_rgb(0x3a, 0x2a, 0x44) } else { Color32::from_rgb(0x2a, 0x3a, 0x4a) });
                p.text(r.center(), egui::Align2::CENTER_CENTER, s.visible().trim().to_string(), egui::FontId::proportional(13.0), Color32::from_gray(230));
                x += w;
                bounds.push(x);
            }
            for (i, &bx) in bounds.iter().enumerate().take(syls.len().saturating_sub(1)) {
                let hit = egui::Rect::from_center_size(egui::pos2(bx, rect.center().y), egui::vec2(10.0, rect.height()));
                let r = ui.interact(hit, egui::Id::unique(("kb", i)), egui::Sense::drag());
                if r.hovered() || r.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }
                p.line_segment([egui::pos2(bx, rect.top()), egui::pos2(bx, rect.bottom())], egui::Stroke::new(2.0, if r.dragged() { theme::ACCENT } else { Color32::from_gray(150) }));
                if r.dragged() {
                    if let Some(pos) = r.interact_pointer_pos() {
                        let target = ((pos.x - rect.left()) / px_cs).round() as i64;
                        let cum: i64 = syls[..=i].iter().map(|s| s.dur).sum();
                        let mut v = syls.clone();
                        k::move_boundary(&mut v, i, target - cum);
                        new_syls = Some(v);
                    }
                }
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Hece metnine | yazarak böl; sınırları çubukta sürükle.").weak().small());
            // Hece listesi
            let mut join_at: Option<usize> = None;
            for i in 0..syls.len() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).weak());
                    let mut t = syls[i].text.clone();
                    if ui.add(TextEdit::singleline(&mut t).desired_width(150.0)).changed() {
                        let mut v = syls.clone();
                        if let Some(p) = t.find('|') {
                            let before = t[..p].chars().count();
                            v[i].text = t.replace('|', "");
                            k::split(&mut v, i, before);
                        } else {
                            v[i].text = t;
                        }
                        new_syls = Some(v);
                    }
                    let mut dur = syls[i].dur;
                    if ui.add(egui::DragValue::new(&mut dur).range(0..=100000).suffix(" cs")).changed() {
                        let mut v = syls.clone();
                        v[i].dur = dur;
                        new_syls = Some(v);
                    }
                    let mut kind = syls[i].kind.clone();
                    egui::ComboBox::from_id_salt(("kk", i)).selected_text(format!("\\{}", kind)).width(60.0).show_ui(ui, |ui| {
                        for kk in ["k", "kf", "ko"] { ui.selectable_value(&mut kind, kk.to_string(), format!("\\{}", kk)); }
                    });
                    if kind != syls[i].kind { let mut v = syls.clone(); v[i].kind = kind; new_syls = Some(v); }
                    if i + 1 < syls.len() && ui.small_button("Birleştir ↓").clicked() { join_at = Some(i); }
                });
            }
            if let Some(i) = join_at { k::join(&mut syls, i); new_syls = Some(syls.clone()); }
        }
        if let Some(v) = new_syls {
            let text = k::build(&v);
            if text != d.text {
                let key = format!("kara:{}", at);
                self.doc.edit_keyed("Karaoke", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = text));
            }
        }
    }

    fn quality_tab(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        egui::Grid::new("rules").num_columns(2).show(ui, |ui| {
            ui.label("Azami kar/sn"); changed |= ui.add(egui::DragValue::new(&mut self.rules.max_cps).range(5.0..=60.0)).changed(); ui.end_row();
            ui.label("Azami satır uzunluğu"); changed |= ui.add(egui::DragValue::new(&mut self.rules.max_line_len).range(10..=120)).changed(); ui.end_row();
            ui.label("Azami satır sayısı"); changed |= ui.add(egui::DragValue::new(&mut self.rules.max_lines).range(1..=6)).changed(); ui.end_row();
            ui.label("En kısa süre (ms)"); changed |= ui.add(egui::DragValue::new(&mut self.rules.min_duration_ms).speed(10)).changed(); ui.end_row();
            ui.label("En uzun süre (ms)"); changed |= ui.add(egui::DragValue::new(&mut self.rules.max_duration_ms).speed(50)).changed(); ui.end_row();
            ui.label("En küçük boşluk (ms)"); changed |= ui.add(egui::DragValue::new(&mut self.rules.min_gap_ms).speed(10)).changed(); ui.end_row();
        });
        if changed { self.recheck(); }
        ui.horizontal_wrapped(|ui| {
            let apply = |app: &mut App, name: &str, f: &dyn Fn(&mut osisub_core::Script)| {
                let mut s = app.doc.script.clone();
                f(&mut s);
                app.doc.apply_new_script(name, s);
            };
            if ui.button("Çakışmaları düzelt").clicked() { apply(self, "Çakışmaları düzelt", &|s| { qc::fix_overlaps(s); }); }
            if ui.button("Yinelenenleri sil").clicked() { apply(self, "Yinelenenleri sil", &|s| { qc::remove_duplicates(s); }); }
            if ui.button("Boşları sil").clicked() { apply(self, "Boşları sil", &|s| { qc::remove_empty(s); }); }
        });
        ui.separator();
        ui.label(RichText::new(format!("{} sorun", self.issues.len())).strong());
        let issues = self.issues.clone();
        let mut jump = None;
        egui::ScrollArea::vertical().id_salt("issues").show_rows(ui, 20.0, issues.len(), |ui, range| {
            for i in range {
                let is = &issues[i];
                if let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(is.entry) {
                    let txt = format!("{}  {}  {}", time::format_ass(d.start_time), is.message, "");
                    if ui.selectable_label(self.doc.active == is.entry, txt).clicked() { jump = Some(is.entry); }
                }
            }
        });
        if let Some(e) = jump { self.doc.select(vec![e], e); self.scroll_to = Some(e); }
    }

    fn automation_tab(&mut self, ui: &mut egui::Ui) {
        if ui.button("Lua betiği yükle…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Lua", &["lua"]).pick_file() { self.load_lua(p); }
        }
        if ui.button("Betikleri yeniden yükle").clicked() { self.reload_scripts(); }
        if let Some(r) = self.running.clone() { ui.horizontal(|ui| { ui.spinner(); ui.label(format!("Çalışıyor: {}", r)); if ui.small_button("Durdur").on_hover_text("Betik motorunu sıfırlar (takılan betikler için)").clicked() { self.reload_scripts(); } if self.progress > 0.0 { ui.add(egui::ProgressBar::new(self.progress).desired_width(100.0)); } }); }
        let list: Vec<(usize, String, String, String, MacroKind)> = self.macros.iter().map(|m| (m.info.id, m.script.clone(), m.info.name.clone(), m.info.description.clone(), m.info.kind.clone())).collect();
        for (id, script, name, desc, kind) in list {
            ui.horizontal(|ui| {
                let enabled = self.running.is_none() && kind == MacroKind::Macro;
                if ui.add_enabled(enabled, egui::Button::new("Çalıştır")).clicked() { self.run_macro(id); }
                ui.vertical(|ui| { ui.strong(&name); ui.label(RichText::new(format!("{} — {}", script, desc)).weak().small()); });
            });
        }
        ui.separator();
        ui.horizontal(|ui| { ui.strong("Günlük"); if ui.small_button("Temizle").clicked() { self.log.clear(); } });
        ui.add(TextEdit::multiline(&mut self.log.as_str()).desired_width(f32::INFINITY).desired_rows(12).font(egui::TextStyle::Monospace));
    }

    fn info_tab(&mut self, ui: &mut egui::Ui) {
        let idxs: Vec<usize> = self.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Info { .. })).map(|(i, _)| i).collect();
        for i in idxs {
            if let Entry::Info { key, value } = self.doc.script.entries[i].clone() {
                let mut v = value.clone();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&key).strong());
                    ui.add(TextEdit::singleline(&mut v).desired_width(f32::INFINITY));
                });
                if v != value {
                    let k = format!("info:{}", i);
                    self.doc.edit_keyed("Betik bilgisi", Some(&k), |tx| tx.replace(i, Entry::Info { key, value: v }));
                }
            }
        }
    }

    fn timing_window(&mut self, ctx: &egui::Context) {
        let mut open = self.timing_open;
        egui::Window::new("Zamanlama").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Kaydır (ms)");
                ui.add(egui::DragValue::new(&mut self.shift_ms).speed(10));
                ui.checkbox(&mut self.shift_all, "tüm satırlar");
                if ui.button("Uygula").clicked() {
                    if self.shift_all {
                        let mut s = self.doc.script.clone();
                        qc::shift(&mut s, self.shift_ms, None);
                        self.doc.apply_new_script("Kaydır", s);
                    } else {
                        self.doc.shift_selected(self.shift_ms);
                    }
                }
            });
            ui.separator();
            ui.strong("Son-işleyici");
            egui::Grid::new("post_grid").num_columns(2).show(ui, |ui| {
                ui.label("Lead-in (ms)"); ui.add(egui::DragValue::new(&mut self.post.lead_in_ms).speed(5)); ui.end_row();
                ui.label("Lead-out (ms)"); ui.add(egui::DragValue::new(&mut self.post.lead_out_ms).speed(5)); ui.end_row();
                ui.label("Bitişik satır eşiği (ms)"); ui.add(egui::DragValue::new(&mut self.post.adjacent_gap_ms).speed(5)); ui.end_row();
                ui.label("Boşluk eğilimi (0–1)"); ui.add(egui::DragValue::new(&mut self.post.adjacent_bias).speed(0.05).range(0.0..=1.0)); ui.end_row();
                ui.label("Anahtar kare: başl. önce/sonra"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.post.kf_start_before).speed(5)); ui.add(egui::DragValue::new(&mut self.post.kf_start_after).speed(5)); }); ui.end_row();
                ui.label("Anahtar kare: bitiş önce/sonra"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.post.kf_end_before).speed(5)); ui.add(egui::DragValue::new(&mut self.post.kf_end_after).speed(5)); }); ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.post_selection_only, "Yalnızca seçim");
                ui.label(RichText::new(format!("{} anahtar kare", self.media.keyframes.len())).weak());
                if ui.button("Uygula").clicked() {
                    let mut o = self.post.clone();
                    o.keyframes = self.media.keyframes.clone();
                    o.only = if self.post_selection_only { Some(self.doc.selection.clone()) } else { None };
                    let mut s = self.doc.script.clone();
                    timing::post_process(&mut s, &o);
                    self.doc.apply_new_script("Zamanlama son-işleyici", s);
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("FPS");
                ui.add(egui::DragValue::new(&mut self.fps).speed(0.001).range(1.0..=240.0));
                if ui.button("Kareye oturt").clicked() {
                    let mut s = self.doc.script.clone();
                    qc::snap_to_frames(&mut s, self.fps);
                    self.doc.apply_new_script("Kareye oturt", s);
                }
            });
        });
        self.timing_open = open;
    }

    fn find_window(&mut self, ctx: &egui::Context) {
        let mut open = self.find_open;
        egui::Window::new("Bul / Değiştir").open(&mut open).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("find_grid").num_columns(2).show(ui, |ui| {
                ui.label("Bul");
                let r = ui.add(TextEdit::singleline(&mut self.find.pattern).desired_width(280.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { self.find_next(); }
                ui.end_row();
                ui.label("Değiştir");
                ui.add(TextEdit::singleline(&mut self.find.replacement).desired_width(280.0));
                ui.end_row();
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.find.regex, "Düzenli ifade");
                ui.checkbox(&mut self.find.match_case, "Büyük/küçük harf");
                ui.checkbox(&mut self.find.whole_word, "Tam sözcük");
                ui.checkbox(&mut self.find.ignore_tags, "Etiketleri yok say");
                ui.checkbox(&mut self.find.skip_comments, "Yorumları atla");
                ui.checkbox(&mut self.find_selection_only, "Yalnızca seçim");
            });
            ui.horizontal(|ui| {
                ui.label("Alanlar:");
                for (f, l) in [(Field::Text, "Metin"), (Field::Actor, "Konuşan"), (Field::Effect, "Efekt"), (Field::Style, "Stil")] {
                    let mut on = self.find.fields.contains(&f);
                    if ui.checkbox(&mut on, l).changed() {
                        if on { self.find.fields.push(f); } else { self.find.fields.retain(|x| *x != f); }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Sonrakini bul").clicked() { self.find_next(); }
                if ui.button("Tümünü seç").clicked() { self.find_select_all(); }
                if ui.button("Tümünü değiştir").clicked() { self.find_replace_all(); }
            });
        });
        self.find_open = open;
    }

    fn find_opts(&self) -> search::Options {
        let mut o = self.find.clone();
        if self.find_selection_only { o.only = Some(self.doc.selection.clone()); }
        o
    }

    fn find_next(&mut self) {
        let o = self.find_opts();
        match search::find_all(&self.doc.script, &o) {
            Ok(hits) if !hits.is_empty() => {
                let cur = self.doc.active;
                let h = hits.iter().find(|h| h.entry > cur).unwrap_or(&hits[0]).entry;
                self.doc.select(vec![h], h);
                self.scroll_to = Some(h);
                self.status = format!("{} eşleşme", hits.len());
            }
            Ok(_) => self.status = "Eşleşme yok".into(),
            Err(e) => self.status = format!("Arama hatası: {}", e),
        }
    }

    fn find_select_all(&mut self) {
        let o = self.find_opts();
        match search::find_all(&self.doc.script, &o) {
            Ok(hits) => {
                let mut v: Vec<usize> = hits.iter().map(|h| h.entry).collect();
                v.dedup();
                self.status = format!("{} satır seçildi", v.len());
                let a = v.first().copied().unwrap_or(self.doc.active);
                self.doc.select(v, a);
                self.scroll_to = Some(a);
            }
            Err(e) => self.status = format!("Arama hatası: {}", e),
        }
    }

    fn find_replace_all(&mut self) {
        let o = self.find_opts();
        let mut s = self.doc.script.clone();
        match search::replace_all(&mut s, &o) {
            Ok(n) => {
                self.doc.apply_new_script("Bul/Değiştir", s);
                self.status = format!("{} alan değiştirildi", n);
            }
            Err(e) => self.status = format!("Arama hatası: {}", e),
        }
    }

    fn resample_window(&mut self, ctx: &egui::Context) {
        let mut open = self.resample_open;
        egui::Window::new("Çözünürlüğü yeniden örnekle").open(&mut open).collapsible(false).show(ctx, |ui| {
            let (ox, oy) = self.doc.script.res();
            ui.label(format!("Mevcut: {}×{}", ox, oy));
            ui.horizontal(|ui| {
                ui.label("Yeni");
                ui.add(egui::DragValue::new(&mut self.res_x).range(16..=16384));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.res_y).range(16..=16384));
                for (x, y, l) in [(1280, 720, "720p"), (1920, 1080, "1080p"), (384, 288, "384×288")] {
                    if ui.small_button(l).clicked() { self.res_x = x; self.res_y = y; }
                }
            });
            ui.label(RichText::new("Stiller, kenar boşlukları, \\pos/\\move/\\org/\\clip/\\fs/\\bord/... ve çizimler ölçeklenir.").weak().small());
            if ui.button("Uygula").clicked() {
                let mut s = self.doc.script.clone();
                timing::resample(&mut s, self.res_x, self.res_y);
                self.doc.apply_new_script("Çözünürlüğü yeniden örnekle", s);
            }
        });
        self.resample_open = open;
    }

    fn goto_window(&mut self, ctx: &egui::Context) {
        let mut open = self.goto_open;
        egui::Window::new("Zamana git").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label("Zaman (H:MM:SS.cc, MM:SS ya da saniye):");
            let r = ui.add(TextEdit::singleline(&mut self.goto_buf).desired_width(160.0));
            r.request_focus();
            if ui.button("Git").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                let t = time::parse_ass(&self.goto_buf).or_else(|| self.goto_buf.trim().parse::<f64>().ok().map(|s| (s * 1000.0) as i64));
                match t {
                    Some(t) => {
                        // en yakın satır (başlangıcı t'ye en yakın/içeren)
                        let best = self.rows.iter().copied().min_by_key(|&i| match &self.doc.script.entries[i] { Entry::Dialogue(d) => if d.start_time <= t && t < d.end_time { 0 } else { (d.start_time - t).abs().min((d.end_time - t).abs()) }, _ => i64::MAX });
                        if let Some(b) = best { self.doc.select(vec![b], b); self.scroll_to = Some(b); }
                        self.media.playhead = t;
                        self.goto_open = false;
                    }
                    None => self.status = "Geçersiz zaman".into(),
                }
            }
        });
        if !open { self.goto_open = false; }
    }

    fn vad_window(&mut self, ctx: &egui::Context) {
        let mut open = self.vad_open;
        egui::Window::new("Sesten satır oluştur").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label("Konuşma bölgelerini bulup boş altyazı satırları oluşturur (zamanlama iskeleti).");
            egui::Grid::new("vad_grid").num_columns(2).show(ui, |ui| {
                ui.label("Eşik payı (dB)"); ui.add(egui::DragValue::new(&mut self.vad.margin_db).speed(0.5).range(3.0..=40.0)); ui.end_row();
                ui.label("Mutlak taban (dBFS)"); ui.add(egui::DragValue::new(&mut self.vad.floor_db).speed(0.5).range(-90.0..=-10.0)); ui.end_row();
                ui.label("En kısa konuşma (ms)"); ui.add(egui::DragValue::new(&mut self.vad.min_speech_ms).speed(10)); ui.end_row();
                ui.label("En kısa sessizlik (ms)"); ui.add(egui::DragValue::new(&mut self.vad.min_silence_ms).speed(10)); ui.end_row();
                ui.label("Kenar payı (ms)"); ui.add(egui::DragValue::new(&mut self.vad.pad_ms).speed(5)); ui.end_row();
            });
            ui.checkbox(&mut self.vad_skip_existing, "Zaten satır olan bölgeleri atla");
            if ui.button("Oluştur").clicked() {
                if let Some(w) = self.media.waveform() {
                    let segs = osisub_media::detect_speech(&w, &self.vad);
                    let existing: Vec<(i64, i64)> = self.doc.script.dialogues().map(|d| (d.start_time, d.end_time)).collect();
                    let style = self.doc.script.styles().next().map(|s| s.name.clone()).unwrap_or_else(|| "Default".into());
                    let mut s = self.doc.script.clone();
                    let mut n = 0;
                    for (a, b) in segs {
                        let covered = existing.iter().any(|&(x, y)| (y.min(b) - x.max(a)) * 2 > (b - a));
                        if self.vad_skip_existing && covered { continue; }
                        s.entries.push(Entry::Dialogue(osisub_core::Dialogue { start_time: a, end_time: b, style: style.clone(), ..Default::default() }));
                        n += 1;
                    }
                    qc::sort_by_start(&mut s);
                    self.doc.apply_new_script("Sesten satır oluştur", s);
                    self.status = format!("{} satır oluşturuldu", n);
                }
            }
        });
        self.vad_open = open;
    }

    fn fixes_window(&mut self, ctx: &egui::Context) {
        let mut open = self.fixes_open;
        egui::Window::new("Sık hataları düzelt").open(&mut open).collapsible(false).show(ctx, |ui| {
            let f = &mut self.fixes;
            ui.checkbox(&mut f.mojibake, "Bozuk Türkçe karakterleri onar (Ã¼ → ü, Ý → İ, þ → ş …)");
            ui.checkbox(&mut f.double_spaces, "Çift boşlukları tek yap");
            ui.checkbox(&mut f.edge_spaces, "Satır başı/sonu boşluklarını sil");
            ui.checkbox(&mut f.space_before_punct, "Noktalamadan önceki boşluğu sil");
            ui.checkbox(&mut f.space_after_punct, "Noktalamadan sonra boşluk ekle");
            ui.checkbox(&mut f.ellipsis, "Üç noktayı … yap");
            ui.checkbox(&mut f.dialogue_dash, "Diyalog çizgisinden sonra boşluk (-Merhaba → - Merhaba)");
            ui.checkbox(&mut f.sentence_case, "Cümle başlarını büyük harf yap (Türkçe i/İ, ı/I)");
            ui.checkbox(&mut f.remove_hi, "İşitme engelli notlarını sil ([ağlar], (müzik), AD: ön ekleri)");
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.min_dur_on, "En kısa süre (ms)");
                let mut v = f.min_duration_ms.unwrap_or(833);
                ui.add_enabled(self.min_dur_on, egui::DragValue::new(&mut v).speed(10));
                f.min_duration_ms = self.min_dur_on.then_some(v);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.min_gap_on, "En küçük boşluk (ms)");
                let mut v = f.min_gap_ms.unwrap_or(84);
                ui.add_enabled(self.min_gap_on, egui::DragValue::new(&mut v).speed(5));
                f.min_gap_ms = self.min_gap_on.then_some(v);
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.fixes_selection_only, "Yalnızca seçim");
                if ui.button("Uygula").clicked() {
                    let mut o = self.fixes.clone();
                    o.only = if self.fixes_selection_only { Some(self.doc.selection.clone()) } else { None };
                    let mut s = self.doc.script.clone();
                    let rep = osisub_core::fixes::apply(&mut s, &o);
                    self.doc.apply_new_script("Sık hataları düzelt", s);
                    self.status = format!("{} satır değişti, {} zamanlama düzeltildi", rep.changed_lines, rep.timing_changes);
                }
            });
        });
        self.fixes_open = open;
    }

    fn start_font_scan(&mut self) {
        let used = osisub_core::fonts_used::used_fonts(&self.doc.script);
        let (tx, rx) = std::sync::mpsc::channel();
        self.font_rx = Some(rx);
        self.font_report = None;
        std::thread::spawn(move || {
            let mut db = osisub_lua::fonts::FontDb::default();
            let rep: Vec<_> = used.into_iter().map(|u| { let f = db.files_for(&u.family); (u, f) }).collect();
            let _ = tx.send(rep);
        });
    }

    fn fonts_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.font_rx {
            if let Ok(r) = rx.try_recv() { self.font_report = Some(r); self.font_rx = None; }
            else { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
        }
        let mut open = self.fonts_open;
        egui::Window::new("Yazı tipi denetimi").open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            if ui.button("Yenile").clicked() { self.start_font_scan(); }
            match &self.font_report {
                None => { ui.horizontal(|ui| { ui.spinner(); ui.label("Sistem yazı tipleri taranıyor…"); }); }
                Some(rep) => {
                    let missing = rep.iter().filter(|(_, f)| f.is_empty()).count();
                    ui.label(if missing == 0 { RichText::new("Tüm yazı tipleri kurulu").color(Color32::from_rgb(0x7a, 0xd9, 0x8a)) } else { RichText::new(format!("{} yazı tipi eksik", missing)).color(theme::BAD) });
                    egui::Grid::new("fonts_grid").striped(true).show(ui, |ui| {
                        ui.strong("Aile"); ui.strong("Stil"); ui.strong("Satır"); ui.strong("Durum"); ui.end_row();
                        for (u, files) in rep {
                            ui.label(&u.family);
                            ui.label(format!("{}{}", if u.bold { "kalın " } else { "" }, if u.italic { "italik" } else { "" }));
                            ui.label(u.lines.to_string());
                            if files.is_empty() { ui.label(RichText::new("EKSİK").color(theme::BAD)); } else { ui.label(RichText::new("kurulu").color(Color32::from_rgb(0x7a, 0xd9, 0x8a))).on_hover_text(files.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n")); }
                            ui.end_row();
                        }
                    });
                    if ui.button("Kurulu yazı tiplerini klasöre kopyala…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let mut n = 0;
                            for (_, files) in rep { for f in files { if let Some(name) = f.file_name() { if std::fs::copy(f, dir.join(name)).is_ok() { n += 1; } } } }
                            self.status = format!("{} yazı tipi dosyası kopyalandı", n);
                        }
                    }
                }
            }
        });
        self.fonts_open = open;
    }

    fn autosave(&mut self) {
        if !self.doc.is_dirty() || self.last_autosave.elapsed().as_secs() < 60 { return; }
        self.last_autosave = std::time::Instant::now();
        let Some(base) = std::env::var("XDG_DATA_HOME").map(PathBuf::from).ok().or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local/share"))) else { return };
        let dir = base.join("osisub/autosave");
        if std::fs::create_dir_all(&dir).is_err() { return; }
        let name = self.doc.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "adsiz".into());
        let _ = std::fs::write(dir.join(format!("{}.autosave.ass", name)), self.doc.script.to_ass_string());
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.shot_frame += 1;
        ctx.request_repaint();
        if self.shot_frame >= 12 && !self.shot_requested && (!self.media.is_open() || self.media.ready() || self.shot_frame > 900) && self.font_rx.is_none() && !self.autoplay && self.play_wait == 0 && !self.autospell && self.miss_rx.is_none() && self.spell_load_rx.is_none() {
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
