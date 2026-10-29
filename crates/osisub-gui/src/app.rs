use crate::dialog_ui::{self, DialogState};
use crate::{style_ui, theme};
use eframe::egui::{self, text::LayoutJob, Align, Color32, Key, KeyboardShortcut, Modifiers, RichText, TextEdit, TextFormat};
use egui_extras::{Column, TableBuilder};
use osisub_core::ass::{Entry, Style};
use osisub_core::qc::{self, Issue, Kind, Rules};
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
    start_buf: String,
    end_buf: String,
    buf_for: (usize, u64),
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
    shot_done: Arc<AtomicBool>,
}

fn autoload_dir() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).ok().or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("osisub/automation/autoload"))
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, scripts: Vec<PathBuf>, shot: Option<PathBuf>, tab: Option<String>) -> App {
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
        for s in all {
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
            tab: match tab.as_deref() { Some("quality") => Tab::Quality, Some("automation") => Tab::Automation, Some("info") => Tab::Info, _ => Tab::Styles },
            filter: String::new(),
            rows: vec![],
            rows_key: (u64::MAX, String::new()),
            rules: Rules::default(),
            issues: vec![],
            issue_map: HashMap::new(),
            issues_key: u64::MAX,
            start_buf: String::new(),
            end_buf: String::new(),
            buf_for: (usize::MAX, u64::MAX),
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
            shot_done: Arc::new(AtomicBool::new(false)),
        };
        if let Some(first) = app.first_row() {
            app.doc.select(vec![first], first);
        }
        app
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
        if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", &["ass", "ssa", "srt", "vtt"]).pick_file() {
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
            }
            Err(e) => self.status = format!("Hata: {}", e),
        }
    }

    fn save(&mut self, as_new: bool) {
        let target = if as_new || self.doc.path.is_none() {
            rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).save_file()
        } else {
            self.doc.path.clone()
        };
        if let Some(p) = target {
            match self.doc.save_as(p.clone()) {
                Ok(()) => self.status = format!("Kaydedildi: {}", p.display()),
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
        self.worker.send(Cmd::Run { id, script: self.doc.script.clone(), selection: self.doc.selection.clone(), active: self.doc.active, props });
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
                        Ok(done) => {
                            let cancelled = done.cancelled;
                            self.doc.apply_run_done(done, &name);
                            self.status = if cancelled { format!("{}: iptal edildi", name) } else { format!("{}: tamamlandı", name) };
                        }
                        Err((msg, log)) => {
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
        if self.issues_key != self.doc.rev {
            self.issues = qc::check(&self.doc.script, &self.rules);
            self.issue_map.clear();
            for i in &self.issues {
                self.issue_map.entry(i.entry).or_default().push(i.kind.clone());
            }
            self.issues_key = self.doc.rev;
        }
    }

    fn recheck(&mut self) {
        self.issues_key = u64::MAX;
    }

    // ------------------------------------------------------------ kısayollar

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input();
        let k = |m: Modifiers, key: Key| KeyboardShortcut::new(m, key);
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
                if ui.add(egui::Button::new("Kaydet").shortcut_text("Ctrl+S")).clicked() { ui.close(); self.save(false); }
                if ui.add(egui::Button::new("Farklı kaydet…").shortcut_text("Ctrl+Shift+S")).clicked() { ui.close(); self.save(true); }
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
                if ui.button("Zamana göre sırala").clicked() {
                    let mut s = self.doc.script.clone();
                    qc::sort_by_start(&mut s);
                    self.doc.apply_new_script("Sırala", s);
                    ui.close();
                }
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
                    if let Some(p) = rfd::FileDialog::new().add_filter("Lua", &["lua"]).pick_file() { self.worker.send(Cmd::Load(p)); }
                }
            });
            ui.menu_button("Araçlar", |ui| {
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
            .column(Column::initial(80.0).clip(true))
            .column(Column::remainder().clip(true));
        if let Some(target) = self.scroll_to.take() {
            if let Some(pos) = self.rows.iter().position(|&r| r == target) {
                table = table.scroll_to_row(pos, Some(Align::Center));
            }
        }
        let rows = self.rows.clone();
        let mut clicked: Option<usize> = None;
        table
            .header(22.0, |mut h| {
                for t in ["#", "Başlangıç", "Bitiş", "Süre", "KSS", "Stil", "Konuşan", "Metin"] {
                    h.col(|ui| { ui.strong(t); });
                }
            })
            .body(|body| {
                body.rows(24.0, rows.len(), |mut row| {
                    let pos = row.index();
                    let ei = rows[pos];
                    let Entry::Dialogue(d) = &self.doc.script.entries[ei] else { return };
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
                    row.col(|ui| { ui.add(egui::Label::new(Self::highlighted(&d.text, dim)).truncate()); });
                    if row.response().clicked() { clicked = Some(pos); }
                });
            });
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
        let eid = egui::Id::new(id);
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
        let r = ui.add(TextEdit::multiline(&mut nd.text).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).hint_text("Altyazı metni — satır sonu için \\N"));
        let _ = r;
        let cps = qc::cps(&nd);
        let vis = osisub_core::tags::visible_text(&nd.text);
        let longest = osisub_core::tags::max_line_len(&nd.text);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} karakter", osisub_core::tags::visible_len(&nd.text))).weak());
            ui.label(RichText::new(format!("{} satır", vis.lines().count().max(1))).weak());
            ui.label(RichText::new(format!("en uzun satır {}", longest)).color(if longest > self.rules.max_line_len { theme::BAD } else { Color32::GRAY }));
            ui.label(RichText::new(format!("{} kar/sn", if cps.is_finite() { format!("{:.1}", cps) } else { "∞".into() })).color(if cps > self.rules.max_cps { theme::BAD } else { Color32::GRAY }));
        });
        if nd != d {
            let key = if nd.text != d.text { format!("text:{}", at) } else if nd.start_time != d.start_time || nd.end_time != d.end_time { format!("time:{}", at) } else { String::new() };
            let desc = if nd.text != d.text { "Metin düzenle" } else { "Satırı düzenle" };
            self.doc.edit_keyed(desc, if key.is_empty() { None } else { Some(&key) }, |tx| tx.replace(at, Entry::Dialogue(nd)));
        }
    }

    fn side(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (t, l) in [(Tab::Styles, "Stiller"), (Tab::Quality, "Kalite"), (Tab::Automation, "Otomasyon"), (Tab::Info, "Bilgi")] {
                ui.selectable_value(&mut self.tab, t, l);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
            Tab::Styles => self.styles_tab(ui),
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
        ui.horizontal(|ui| {
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
            let mut apply = |app: &mut App, name: &str, f: &dyn Fn(&mut osisub_core::Script)| {
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
                if let Entry::Dialogue(d) = &self.doc.script.entries[is.entry] {
                    let txt = format!("{}  {}  {}", time::format_ass(d.start_time), is.message, "");
                    if ui.selectable_label(self.doc.active == is.entry, txt).clicked() { jump = Some(is.entry); }
                }
            }
        });
        if let Some(e) = jump { self.doc.select(vec![e], e); self.scroll_to = Some(e); }
    }

    fn automation_tab(&mut self, ui: &mut egui::Ui) {
        if ui.button("Lua betiği yükle…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Lua", &["lua"]).pick_file() { self.worker.send(Cmd::Load(p)); }
        }
        if let Some(r) = &self.running { ui.horizontal(|ui| { ui.spinner(); ui.label(format!("Çalışıyor: {}", r)); if self.progress > 0.0 { ui.add(egui::ProgressBar::new(self.progress).desired_width(100.0)); } }); }
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

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.shot_frame += 1;
        ctx.request_repaint();
        if self.shot_frame == 12 {
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pump_events(&ctx);
        self.refresh_caches();
        self.shortcuts(&ctx);
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
        egui::Panel::bottom("editor").resizable(true).default_size(190.0).show(ui, |ui| self.editor(ui));
        egui::Panel::right("side").resizable(true).default_size(380.0).show(ui, |ui| self.side(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| self.grid(ui));

        self.timing_window(&ctx);
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
        self.handle_screenshot(&ctx);
    }
}
