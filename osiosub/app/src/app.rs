use crate::worker::{self, FromWorker, ToWorker, Worker};
use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};
use egui_file_dialog::FileDialog;
use osiosub_core::{
    ass::{AssFile, Event, Style},
    color::Color,
    qc::{self, Issue, QcConfig, Severity},
    search::{self, Field, SearchOpts},
    tags, textfmt, time, timing, turkish,
    undo::History,
};
use osiosub_lua::{DialogResult, DialogSpec, DlgVal, MacroInfo};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
enum FileAction { Open, SaveAs, Export, LoadScript }

#[derive(Default)]
struct FindState { open: bool, pattern: String, with: String, regex: bool, case: bool, word: bool, skip_tags: bool, field: u8, msg: String }

#[derive(Default)]
struct ShiftState { open: bool, ms: String, start: bool, end: bool, selected_only: bool }

pub struct OsioApp {
    file: AssFile,
    path: Option<PathBuf>,
    dirty: bool,
    rev: u64,
    hist: History,
    sel: BTreeSet<usize>,
    active: Option<usize>,
    anchor: Option<usize>,
    status: String,
    issues: Vec<Issue>,
    issues_rev: u64,
    qc_cfg: QcConfig,
    show_qc: bool,
    show_styles: bool,
    style_idx: usize,
    show_props: bool,
    show_log: bool,
    log: String,
    find: FindState,
    shift: ShiftState,
    fd: FileDialog,
    fd_action: FileAction,
    worker: Worker,
    macros: Vec<MacroInfo>,
    running: bool,
    dialog: Option<(DialogSpec, BTreeMap<String, DlgVal>)>,
    time_buf: (Option<usize>, String, String),
    scroll_to: Option<usize>,
    export_ext: &'static str,
}

impl OsioApp {
    pub fn new(cc: &eframe::CreationContext<'_>, arg: Option<String>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = OsioApp {
            file: AssFile::new_default(), path: None, dirty: false, rev: 0, hist: History::new(200),
            sel: BTreeSet::new(), active: None, anchor: None, status: "Hazır".into(), issues: vec![], issues_rev: u64::MAX,
            qc_cfg: QcConfig::default(), show_qc: false, show_styles: false, style_idx: 0, show_props: false, show_log: false,
            log: String::new(), find: FindState { skip_tags: true, ..Default::default() }, shift: ShiftState { start: true, end: true, ..Default::default() },
            fd: FileDialog::new(), fd_action: FileAction::Open, worker: worker::spawn(cc.egui_ctx.clone()), macros: vec![],
            running: false, dialog: None, time_buf: (None, String::new(), String::new()), scroll_to: None, export_ext: "srt",
        };
        if let Some(p) = arg { app.open_path(PathBuf::from(p)); }
        // auto-load scripts from ./automation/autoload if present
        if let Ok(rd) = std::fs::read_dir("automation/autoload") {
            for e in rd.flatten() { if e.path().extension().map_or(false, |x| x == "lua") { let _ = app.worker.tx.send(ToWorker::Load(e.path())); } }
        }
        app
    }

    fn touch(&mut self) { self.dirty = true; self.rev += 1; }
    fn snapshot(&mut self, label: &str) { self.hist.record(label, &self.file); }

    fn open_path(&mut self, p: PathBuf) {
        match std::fs::read(&p) {
            Ok(b) => match textfmt::open_any(&p.to_string_lossy(), &String::from_utf8_lossy(&b)) {
                Ok(f) => {
                    self.file = f; self.hist = History::new(200); self.sel.clear(); self.active = if self.file.events.is_empty() { None } else { Some(0) };
                    if let Some(a) = self.active { self.sel.insert(a); }
                    self.status = format!("Açıldı: {} ({} satır)", p.display(), self.file.events.len());
                    let is_ass = p.extension().map_or(false, |e| e.eq_ignore_ascii_case("ass") || e.eq_ignore_ascii_case("ssa"));
                    self.path = if is_ass { Some(p) } else { None };
                    self.dirty = !is_ass; self.rev += 1;
                }
                Err(e) => self.status = format!("Hata: {e}"),
            },
            Err(e) => self.status = format!("Açılamadı: {e}"),
        }
    }

    fn save_to(&mut self, p: PathBuf) {
        let l = p.to_string_lossy().to_ascii_lowercase();
        let s = if l.ends_with(".srt") { textfmt::to_srt(&self.file) } else if l.ends_with(".vtt") { textfmt::to_vtt(&self.file) } else if l.ends_with(".txt") { textfmt::to_txt(&self.file) } else { self.file.to_string() };
        match std::fs::write(&p, s) {
            Ok(_) => { self.status = format!("Kaydedildi: {}", p.display()); if l.ends_with(".ass") || l.ends_with(".ssa") { self.path = Some(p); self.dirty = false; } }
            Err(e) => self.status = format!("Kaydedilemedi: {e}"),
        }
    }

    fn save(&mut self) {
        match self.path.clone() { Some(p) => self.save_to(p), None => { self.fd_action = FileAction::SaveAs; self.fd.save_file(); } }
    }

    fn selected_vec(&self) -> Vec<usize> { self.sel.iter().cloned().collect() }

    fn select_only(&mut self, i: usize) { self.sel.clear(); self.sel.insert(i); self.active = Some(i); self.anchor = Some(i); }

    fn insert_event(&mut self, at: usize, ev: Event) {
        self.snapshot("Satır ekle");
        let at = at.min(self.file.events.len());
        self.file.events.insert(at, ev);
        self.select_only(at); self.scroll_to = Some(at); self.touch();
    }

    fn new_event_after(&self, i: Option<usize>) -> Event {
        match i.and_then(|i| self.file.events.get(i)) {
            Some(e) => Event { start: e.end, end: e.end + 2000, style: e.style.clone(), layer: e.layer, actor: e.actor.clone(), ..Default::default() },
            None => Event::default(),
        }
    }

    fn delete_selected(&mut self) {
        if self.sel.is_empty() { return; }
        self.snapshot("Sil");
        let first = *self.sel.iter().next().unwrap();
        let del = self.sel.clone();
        let mut i = 0;
        self.file.events.retain(|_| { let k = !del.contains(&i); i += 1; k });
        self.sel.clear();
        if !self.file.events.is_empty() { self.select_only(first.min(self.file.events.len() - 1)); } else { self.active = None; }
        self.touch();
    }

    fn duplicate_selected(&mut self) {
        if self.sel.is_empty() { return; }
        self.snapshot("Çoğalt");
        let idx = self.selected_vec();
        let last = *idx.last().unwrap();
        let copies: Vec<Event> = idx.iter().map(|&i| self.file.events[i].clone()).collect();
        for (k, c) in copies.into_iter().enumerate() { self.file.events.insert(last + 1 + k, c); }
        self.touch();
    }

    fn run_macro(&mut self, idx: usize) {
        if self.running { self.status = "Bir makro zaten çalışıyor".into(); return; }
        self.running = true;
        self.log.clear();
        let _ = self.worker.tx.send(ToWorker::Run { idx, file: self.file.clone(), sel: self.selected_vec(), active: self.active.unwrap_or(0) });
    }

    fn poll_worker(&mut self) {
        while let Ok(m) = self.worker.rx.try_recv() {
            match m {
                FromWorker::Loaded(r, all) => {
                    self.macros = all;
                    match r { Ok(n) => self.status = format!("{n} makro yüklendi"), Err(e) => { self.log.push_str(&format!("Script hatası: {e}\n")); self.show_log = true; } }
                }
                FromWorker::Log(s) => { self.log.push_str(&s); self.show_log = true; }
                FromWorker::Dialog(spec) => {
                    let mut vals = BTreeMap::new();
                    for c in &spec.controls {
                        if c.name.is_empty() { continue; }
                        let v = match c.class.as_str() {
                            "checkbox" => DlgVal::Bool(matches!(c.value, Some(DlgVal::Bool(true)))),
                            "dropdown" => c.value.clone().unwrap_or_else(|| DlgVal::Str(c.items.first().cloned().unwrap_or_default())),
                            "intedit" | "floatedit" | "slider" => c.value.clone().unwrap_or(DlgVal::Num(c.min.unwrap_or(0.0))),
                            "label" => continue,
                            _ => c.value.clone().unwrap_or_else(|| DlgVal::Str(c.text.clone())),
                        };
                        vals.insert(c.name.clone(), v);
                    }
                    self.dialog = Some((spec, vals));
                }
                FromWorker::Done(res) => {
                    self.running = false;
                    if let Some(e) = res.error { self.log.push_str(&format!("Hata: {e}\n")); self.show_log = true; self.status = "Makro hatası".into(); }
                    else if res.cancelled { self.status = "Makro iptal edildi".into(); }
                    else if let Some(f) = res.file {
                        if f != self.file { self.snapshot("Makro"); self.file = f; self.touch(); }
                        if let Some(s) = res.selection { self.sel = s.into_iter().filter(|&i| i < self.file.events.len()).collect(); }
                        if let Some(a) = res.active { self.active = Some(a.min(self.file.events.len().saturating_sub(1))); }
                        else if let Some(&a) = self.sel.iter().next() { self.active = Some(a); }
                        self.status = "Makro tamamlandı".into();
                    }
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut as K, Modifiers as M};
        let c = ctx.input_mut(|i| (
            i.consume_shortcut(&K::new(M::COMMAND, Key::Z)), i.consume_shortcut(&K::new(M::COMMAND, Key::Y)),
            i.consume_shortcut(&K::new(M::COMMAND, Key::S)), i.consume_shortcut(&K::new(M::COMMAND, Key::O)),
            i.consume_shortcut(&K::new(M::COMMAND, Key::F)), i.consume_shortcut(&K::new(M::COMMAND, Key::D)),
            i.consume_shortcut(&K::new(M::COMMAND, Key::Delete)),
        ));
        if c.0 { if self.hist.undo(&mut self.file).is_some() { self.touch(); } }
        if c.1 { if self.hist.redo(&mut self.file).is_some() { self.touch(); } }
        if c.2 { self.save(); }
        if c.3 { self.fd_action = FileAction::Open; self.fd.pick_file(); }
        if c.4 { self.find.open = true; }
        if c.5 { self.duplicate_selected(); }
        if c.6 { self.delete_selected(); }
    }

    // ---------------- UI pieces ----------------
    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Dosya", |ui| {
                if ui.button("Yeni").clicked() { self.file = AssFile::new_default(); self.path = None; self.hist = History::new(200); self.sel.clear(); self.active = None; self.touch(); self.dirty = false; ui.close(); }
                if ui.button("Aç…  (Ctrl+O)").clicked() { self.fd_action = FileAction::Open; self.fd.pick_file(); ui.close(); }
                if ui.button("Kaydet  (Ctrl+S)").clicked() { self.save(); ui.close(); }
                if ui.button("Farklı kaydet…").clicked() { self.fd_action = FileAction::SaveAs; self.fd.save_file(); ui.close(); }
                ui.separator();
                for (label, ext) in [("SRT olarak dışa aktar…", "srt"), ("WebVTT olarak dışa aktar…", "vtt"), ("Düz metin (TXT)…", "txt")] {
                    if ui.button(label).clicked() { self.export_ext = ext; self.fd_action = FileAction::Export; self.fd.save_file(); ui.close(); }
                }
                ui.separator();
                if ui.button("Betik özellikleri…").clicked() { self.show_props = true; ui.close(); }
            });
            ui.menu_button("Düzen", |ui| {
                if ui.add_enabled(self.hist.can_undo(), egui::Button::new("Geri al  (Ctrl+Z)")).clicked() { if self.hist.undo(&mut self.file).is_some() { self.touch(); } ui.close(); }
                if ui.add_enabled(self.hist.can_redo(), egui::Button::new("Yinele  (Ctrl+Y)")).clicked() { if self.hist.redo(&mut self.file).is_some() { self.touch(); } ui.close(); }
                ui.separator();
                if ui.button("Bul / Değiştir  (Ctrl+F)").clicked() { self.find.open = true; ui.close(); }
                if ui.button("Tümünü seç").clicked() { self.sel = (0..self.file.events.len()).collect(); ui.close(); }
            });
            ui.menu_button("Altyazı", |ui| {
                if ui.button("Öncesine satır ekle").clicked() { let at = self.active.unwrap_or(0); let mut e = self.new_event_after(at.checked_sub(1)); if at == 0 { e = Event::default(); } self.insert_event(at, e); ui.close(); }
                if ui.button("Sonrasına satır ekle").clicked() { let at = self.active.map(|a| a + 1).unwrap_or(self.file.events.len()); let e = self.new_event_after(self.active); self.insert_event(at, e); ui.close(); }
                if ui.button("Çoğalt  (Ctrl+D)").clicked() { self.duplicate_selected(); ui.close(); }
                if ui.button("Sil  (Ctrl+Del)").clicked() { self.delete_selected(); ui.close(); }
                ui.separator();
                if ui.button("Ortadan ikiye böl").clicked() {
                    if let Some(a) = self.active { let e = &self.file.events[a]; let mid = (e.start + e.end) / 2; self.snapshot("Böl"); if timing::split(&mut self.file, a, mid, None) { self.touch(); } }
                    ui.close();
                }
                if ui.button("Seçilileri birleştir").clicked() {
                    let v = self.selected_vec();
                    if v.len() >= 2 { self.snapshot("Birleştir"); timing::merge(&mut self.file, v[0], *v.last().unwrap(), "\\N"); self.select_only(v[0]); self.touch(); }
                    ui.close();
                }
                if ui.button("Yorum satırına çevir/geri al").clicked() { self.snapshot("Yorum"); for &i in &self.sel.clone() { let e = &mut self.file.events[i]; e.comment = !e.comment; } self.touch(); ui.close(); }
                ui.separator();
                if ui.button("Stil yöneticisi…").clicked() { self.show_styles = true; ui.close(); }
            });
            ui.menu_button("Zamanlama", |ui| {
                if ui.button("Kaydır…").clicked() { self.shift.open = true; ui.close(); }
                if ui.button("Zamana göre sırala").clicked() { self.snapshot("Sırala"); timing::sort_by_start(&mut self.file); self.touch(); ui.close(); }
                if ui.button("Küçük boşlukları kapat (<100ms)").clicked() { self.snapshot("Boşluk"); let n = timing::close_gaps(&mut self.file, 100); self.status = format!("{n} boşluk kapatıldı"); self.touch(); ui.close(); }
                if ui.button("Min. 2 kare (84ms) boşluk bırak").clicked() { self.snapshot("Boşluk"); let n = timing::enforce_gap(&mut self.file, 84, 500); self.status = format!("{n} satır kısaltıldı"); self.touch(); ui.close(); }
            });
            ui.menu_button("Araçlar", |ui| {
                if ui.button("Kalite kontrol (QC)").clicked() { self.show_qc = true; ui.close(); }
                ui.separator();
                let sel = self.selected_vec();
                if ui.button("Türkçe: BÜYÜK HARF").clicked() { self.map_text(&sel, |t| tags::map_plain(t, turkish::to_upper)); ui.close(); }
                if ui.button("Türkçe: küçük harf").clicked() { self.map_text(&sel, |t| tags::map_plain(t, turkish::to_lower)); ui.close(); }
                if ui.button("Türkçe: Cümle düzeni").clicked() { self.map_text(&sel, |t| tags::map_plain(t, turkish::sentence_case)); ui.close(); }
                if ui.button("Tipografi düzelt (… , boşluk)").clicked() { self.map_text(&sel, |t| tags::map_plain(t, turkish::fix_typography)); ui.close(); }
                if ui.button("Etiketleri temizle").clicked() { self.map_text(&sel, tags::strip_tags); ui.close(); }
            });
            ui.menu_button("Otomasyon", |ui| {
                if ui.button("Betik yükle (.lua)…").clicked() { self.fd_action = FileAction::LoadScript; self.fd.pick_file(); ui.close(); }
                if ui.button("Günlüğü göster").clicked() { self.show_log = true; ui.close(); }
                ui.separator();
                if self.macros.is_empty() { ui.label(RichText::new("(yüklü makro yok)").weak()); }
                let list: Vec<(usize, MacroInfo)> = self.macros.iter().cloned().enumerate().collect();
                for (i, m) in list {
                    if ui.add_enabled(!self.running, egui::Button::new(&m.name)).on_hover_text(&m.description).clicked() { self.run_macro(i); ui.close(); }
                }
            });
        });
    }

    fn map_text(&mut self, sel: &[usize], f: impl Fn(&str) -> String) {
        if sel.is_empty() { return; }
        self.snapshot("Metin dönüşümü");
        for &i in sel { if let Some(e) = self.file.events.get_mut(i) { e.text = f(&e.text); } }
        self.touch();
    }

    fn grid(&mut self, ui: &mut egui::Ui) {
        if self.issues_rev != self.rev { self.issues = qc::check(&self.file, &self.qc_cfg); self.issues_rev = self.rev; }
        let bad: BTreeMap<usize, Severity> = self.issues.iter().fold(BTreeMap::new(), |mut m, i| { let e = m.entry(i.line).or_insert(i.severity); if i.severity as u8 > *e as u8 { *e = i.severity; } m });
        let n = self.file.events.len();
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut tb = TableBuilder::new(ui).striped(true).resizable(true).sense(egui::Sense::click())
            .column(Column::exact(46.0)).column(Column::exact(86.0)).column(Column::exact(86.0)).column(Column::exact(56.0))
            .column(Column::exact(44.0)).column(Column::exact(90.0)).column(Column::exact(80.0)).column(Column::remainder().clip(true))
            .min_scrolled_height(0.0);
        if let Some(r) = self.scroll_to.take() { tb = tb.scroll_to_row(r, Some(egui::Align::Center)); }
        tb.header(20.0, |mut h| {
            for t in ["#", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Aktör", "Metin"] { h.col(|ui| { ui.strong(t); }); }
        }).body(|body| {
            body.rows(20.0, n, |mut row| {
                let i = row.index();
                let e = &self.file.events[i];
                row.set_selected(self.sel.contains(&i));
                let dur = e.end - e.start;
                let cps = qc::cps(&e.text, dur);
                let sev = bad.get(&i);
                let txt = tags::visible_text(&e.text).replace('\n', "  |  ");
                let dim = e.comment;
                let mk = |s: String| { let t = RichText::new(s); if dim { t.weak().italics() } else { t } };
                row.col(|ui| { ui.label(mk(format!("{}", i + 1))); });
                row.col(|ui| { ui.label(mk(time::fmt_ass(e.start))); });
                row.col(|ui| { ui.label(mk(time::fmt_ass(e.end))); });
                row.col(|ui| { ui.label(mk(format!("{:.2}", dur as f64 / 1000.0))); });
                row.col(|ui| { let mut t = RichText::new(if cps.is_finite() { format!("{:.1}", cps) } else { "∞".into() }); if cps > self.qc_cfg.max_cps { t = t.color(Color32::from_rgb(255, 120, 90)); } ui.label(t); });
                row.col(|ui| { ui.label(mk(e.style.clone())); });
                row.col(|ui| { ui.label(mk(e.actor.clone())); });
                row.col(|ui| {
                    let mut t = RichText::new(txt);
                    if dim { t = t.weak().italics(); }
                    match sev { Some(Severity::Error) => t = t.color(Color32::from_rgb(255, 100, 100)), Some(Severity::Warning) => t = t.color(Color32::from_rgb(255, 205, 100)), _ => {} }
                    ui.label(t);
                });
                let r = row.response();
                if r.clicked() { clicked = Some((i, ui_mods(&r))); }
            });
        });
        if let Some((i, m)) = clicked {
            if m.shift && self.anchor.is_some() {
                let a = self.anchor.unwrap();
                let (lo, hi) = (a.min(i), a.max(i));
                if !m.command { self.sel.clear(); }
                for k in lo..=hi { self.sel.insert(k); }
                self.active = Some(i);
            } else if m.command {
                if !self.sel.remove(&i) { self.sel.insert(i); }
                self.active = Some(i); self.anchor = Some(i);
            } else { self.select_only(i); }
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.active.filter(|&a| a < self.file.events.len()) else { ui.label(RichText::new("Satır seçilmedi — Altyazı ▸ Sonrasına satır ekle").weak()); return; };
        if self.time_buf.0 != Some(a) || !ui.memory(|m| m.focused().is_some()) {
            self.time_buf = (Some(a), time::fmt_ass(self.file.events[a].start), time::fmt_ass(self.file.events[a].end));
        }
        let styles: Vec<String> = self.file.styles.iter().map(|s| s.name.clone()).collect();
        let mut ev = self.file.events[a].clone();
        let mut changed = false; let mut started = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut ev.comment, "Yorum").changed().then(|| { changed = true; started = true; });
            ui.label("Katman"); let r = ui.add(egui::DragValue::new(&mut ev.layer)); if r.changed() { changed = true; } if r.drag_started() { started = true; }
            ui.label("Stil");
            egui::ComboBox::from_id_salt("stylebox").selected_text(&ev.style).show_ui(ui, |ui| { for s in &styles { if ui.selectable_value(&mut ev.style, s.clone(), s).changed() { changed = true; started = true; } } });
            ui.label("Aktör"); let r = ui.add(egui::TextEdit::singleline(&mut ev.actor).desired_width(90.0)); if r.changed() { changed = true; } if r.gained_focus() { started = true; }
            ui.label("Efekt"); let r = ui.add(egui::TextEdit::singleline(&mut ev.effect).desired_width(90.0)); if r.changed() { changed = true; } if r.gained_focus() { started = true; }
            for (l, v) in [("Sol", &mut ev.margin_l), ("Sağ", &mut ev.margin_r), ("Dik", &mut ev.margin_v)] { ui.label(l); let r = ui.add(egui::DragValue::new(v)); if r.changed() { changed = true; } if r.drag_started() { started = true; } }
        });
        ui.horizontal(|ui| {
            ui.label("Başlangıç");
            let r1 = ui.add(egui::TextEdit::singleline(&mut self.time_buf.1).desired_width(90.0));
            ui.label("Bitiş");
            let r2 = ui.add(egui::TextEdit::singleline(&mut self.time_buf.2).desired_width(90.0));
            if r1.lost_focus() || r2.lost_focus() {
                if let Some(s) = time::parse_ass(&self.time_buf.1) { if s != ev.start { ev.start = s; changed = true; started = true; } }
                if let Some(e) = time::parse_ass(&self.time_buf.2) { if e != ev.end { ev.end = e; changed = true; started = true; } }
            }
            let mut dur = (ev.end - ev.start) as f64 / 1000.0;
            ui.label("Süre"); if ui.add(egui::DragValue::new(&mut dur).speed(0.01).suffix(" s").fixed_decimals(2)).changed() { ev.end = ev.start + (dur * 1000.0).round() as i64; changed = true; started = true; }
            let cps = qc::cps(&ev.text, ev.end - ev.start);
            let lens = tags::line_lengths(&ev.text);
            let col = if cps > self.qc_cfg.max_cps { Color32::from_rgb(255, 120, 90) } else { Color32::LIGHT_GREEN };
            ui.label(RichText::new(format!("CPS {:.1}", cps)).color(col));
            ui.label(format!("Karakter: {}", lens.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(" / ")));
            if ui.button("+ \\N").on_hover_text("İmleç yerine satır sonu (\\N) ekle — metin kutusunda Ctrl+Enter").clicked() { ev.text.push_str("\\N"); changed = true; started = true; }
        });
        let mut shown = ev.text.replace("\\N", "\n");
        let r = ui.add(egui::TextEdit::multiline(&mut shown).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).id_salt("textedit"));
        if r.gained_focus() { started = true; }
        if r.changed() { ev.text = shown.replace("\r", "").replace('\n', "\\N"); changed = true; }
        if started && changed { self.hist.record("Düzenle", &self.file); }
        if changed { self.file.events[a] = ev; self.touch(); }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        // Find / replace
        if self.find.open {
            let mut open = true;
            egui::Window::new("Bul / Değiştir").open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| { ui.label("Bul:    "); ui.text_edit_singleline(&mut self.find.pattern); });
                ui.horizontal(|ui| { ui.label("Yerine:"); ui.text_edit_singleline(&mut self.find.with); });
                ui.horizontal(|ui| { ui.checkbox(&mut self.find.regex, "Regex"); ui.checkbox(&mut self.find.case, "Büyük/küçük"); ui.checkbox(&mut self.find.word, "Tam kelime"); ui.checkbox(&mut self.find.skip_tags, "Etiketleri atla"); });
                ui.horizontal(|ui| { ui.label("Alan:"); ui.radio_value(&mut self.find.field, 0, "Metin"); ui.radio_value(&mut self.find.field, 1, "Aktör"); ui.radio_value(&mut self.find.field, 2, "Stil"); ui.radio_value(&mut self.find.field, 3, "Efekt"); });
                let opts = SearchOpts { pattern: self.find.pattern.clone(), regex: self.find.regex, case_sensitive: self.find.case, whole_word: self.find.word, skip_tags: self.find.skip_tags,
                    field: [Field::Text, Field::Actor, Field::Style, Field::Effect][self.find.field as usize] };
                ui.horizontal(|ui| {
                    if ui.button("Sonrakini bul").clicked() {
                        match search::find_all(&self.file, &opts) {
                            Ok(v) => { let from = self.active.map(|a| a + 1).unwrap_or(0); match v.iter().find(|&&i| i >= from).or(v.first()) { Some(&i) => { self.select_only(i); self.scroll_to = Some(i); self.find.msg = format!("{} eşleşme", v.len()); } None => self.find.msg = "Bulunamadı".into() } }
                            Err(e) => self.find.msg = format!("Regex hatası: {e}"),
                        }
                    }
                    if ui.button("Tümünü seç").clicked() { if let Ok(v) = search::find_all(&self.file, &opts) { self.find.msg = format!("{} satır seçildi", v.len()); self.sel = v.into_iter().collect(); } }
                    if ui.button("Tümünü değiştir").clicked() {
                        self.snapshot("Değiştir");
                        match search::replace_all(&mut self.file, &opts, &self.find.with) { Ok(n) => { self.find.msg = format!("{n} satır değişti"); if n > 0 { self.touch(); } } Err(e) => self.find.msg = format!("Regex hatası: {e}") }
                    }
                });
                ui.label(&self.find.msg);
            });
            self.find.open = open;
        }
        // Shift
        if self.shift.open {
            let mut open = true;
            egui::Window::new("Zaman kaydır").open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| { ui.label("Miktar (ms, ± ):"); ui.text_edit_singleline(&mut self.shift.ms); });
                ui.horizontal(|ui| { ui.checkbox(&mut self.shift.start, "Başlangıç"); ui.checkbox(&mut self.shift.end, "Bitiş"); ui.checkbox(&mut self.shift.selected_only, "Yalnız seçili"); });
                if ui.button("Uygula").clicked() {
                    if let Ok(d) = self.shift.ms.trim().parse::<i64>() {
                        self.snapshot("Kaydır");
                        let idx: Vec<usize> = if self.shift.selected_only { self.selected_vec() } else { (0..self.file.events.len()).collect() };
                        timing::shift(&mut self.file, &idx, d, self.shift.start, self.shift.end); self.touch();
                        self.status = format!("{} satır kaydırıldı", idx.len());
                    } else { self.status = "Geçersiz sayı".into(); }
                }
            });
            self.shift.open = open;
        }
        // QC
        if self.show_qc {
            let mut open = true;
            egui::Window::new("Kalite kontrol").open(&mut open).default_width(560.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Maks CPS"); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_cps).speed(0.1));
                    ui.label("Satır uz."); ui.add(egui::DragValue::new(&mut self.qc_cfg.max_line_len));
                    ui.label("Min süre ms"); ui.add(egui::DragValue::new(&mut self.qc_cfg.min_dur_ms));
                    if ui.button("Yenile").clicked() { self.issues_rev = u64::MAX; }
                });
                ui.label(format!("{} sorun", self.issues.len()));
                let mut goto = None;
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    for it in &self.issues {
                        let c = match it.severity { Severity::Error => Color32::from_rgb(255, 100, 100), Severity::Warning => Color32::from_rgb(255, 205, 100), Severity::Info => Color32::GRAY };
                        if ui.add(egui::Label::new(RichText::new(format!("#{}  [{}] {}", it.line + 1, it.code, it.message)).color(c)).sense(egui::Sense::click())).clicked() { goto = Some(it.line); }
                    }
                });
                if let Some(g) = goto { self.select_only(g); self.scroll_to = Some(g); }
            });
            self.show_qc = open;
        }
        // Styles
        if self.show_styles { self.styles_window(ctx); }
        // Properties
        if self.show_props {
            let mut open = true;
            egui::Window::new("Betik özellikleri").open(&mut open).show(ctx, |ui| {
                let mut changed = false;
                let (mut rx, mut ry) = self.file.play_res();
                ui.horizontal(|ui| { ui.label("PlayResX"); changed |= ui.add(egui::DragValue::new(&mut rx)).changed(); ui.label("PlayResY"); changed |= ui.add(egui::DragValue::new(&mut ry)).changed(); });
                let mut title = self.file.info_get("Title").unwrap_or("").to_string();
                ui.horizontal(|ui| { ui.label("Başlık"); if ui.text_edit_singleline(&mut title).changed() { self.file.info_set("Title", &title); self.touch(); } });
                if changed { self.file.info_set("PlayResX", &rx.to_string()); self.file.info_set("PlayResY", &ry.to_string()); self.touch(); }
            });
            self.show_props = open;
        }
        // Log
        if self.show_log {
            let mut open = true;
            egui::Window::new("Otomasyon günlüğü").open(&mut open).default_width(480.0).show(ctx, |ui| {
                if ui.button("Temizle").clicked() { self.log.clear(); }
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| { ui.add(egui::Label::new(RichText::new(&self.log).monospace()).wrap()); });
            });
            self.show_log = open;
        }
        // Lua dialog
        if self.dialog.is_some() { self.lua_dialog(ctx); }
    }

    fn styles_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("Stil yöneticisi").open(&mut open).default_width(520.0).show(ctx, |ui| {
            let mut changed = false; let mut snap = false;
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("sty").selected_text(self.file.styles.get(self.style_idx).map(|s| s.name.as_str()).unwrap_or("-")).show_ui(ui, |ui| {
                    for (i, s) in self.file.styles.iter().enumerate() { ui.selectable_value(&mut self.style_idx, i, &s.name); }
                });
                if ui.button("Yeni").clicked() { self.snapshot("Stil ekle"); let mut s = self.file.styles.get(self.style_idx).cloned().unwrap_or_default(); s.name = format!("{} kopya", s.name); self.file.styles.push(s); self.style_idx = self.file.styles.len() - 1; changed = true; }
                if ui.button("Sil").clicked() && self.file.styles.len() > 1 { self.snapshot("Stil sil"); self.file.styles.remove(self.style_idx); self.style_idx = 0; changed = true; }
            });
            if self.style_idx >= self.file.styles.len() { self.style_idx = 0; }
            let old_name = self.file.styles[self.style_idx].name.clone();
            let mut s: Style = self.file.styles[self.style_idx].clone();
            egui::Grid::new("stygrid").num_columns(4).show(ui, |ui| {
                ui.label("Ad"); changed |= ui.text_edit_singleline(&mut s.name).changed(); ui.label("Yazı tipi"); changed |= ui.text_edit_singleline(&mut s.fontname).changed(); ui.end_row();
                ui.label("Boyut"); changed |= ui.add(egui::DragValue::new(&mut s.fontsize).speed(0.5)).changed();
                ui.label("Kalınlık/İtalik"); ui.horizontal(|ui| { changed |= ui.checkbox(&mut s.bold, "B").changed(); changed |= ui.checkbox(&mut s.italic, "I").changed(); changed |= ui.checkbox(&mut s.underline, "U").changed(); changed |= ui.checkbox(&mut s.strikeout, "S").changed(); }); ui.end_row();
                for (lbl, c) in [("Ana renk", &mut s.primary), ("İkincil", &mut s.secondary), ("Kenar", &mut s.outline_color), ("Gölge", &mut s.back)] {
                    ui.label(lbl);
                    let mut rgba = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 255 - c.a());
                    if ui.color_edit_button_srgba(&mut rgba).changed() { *c = Color::rgba(rgba.r(), rgba.g(), rgba.b(), 255 - rgba.a()); changed = true; }
                    if lbl == "İkincil" || lbl == "Gölge" { ui.end_row(); }
                }
                ui.label("Kenar kalınlığı"); changed |= ui.add(egui::DragValue::new(&mut s.outline).speed(0.1)).changed();
                ui.label("Gölge"); changed |= ui.add(egui::DragValue::new(&mut s.shadow).speed(0.1)).changed(); ui.end_row();
                ui.label("Hizalama (numpad)"); changed |= ui.add(egui::DragValue::new(&mut s.alignment).range(1..=9)).changed();
                ui.label("Aralık"); changed |= ui.add(egui::DragValue::new(&mut s.spacing).speed(0.1)).changed(); ui.end_row();
                ui.label("Kenar boşluğu L/R/V"); ui.horizontal(|ui| { changed |= ui.add(egui::DragValue::new(&mut s.margin_l)).changed(); changed |= ui.add(egui::DragValue::new(&mut s.margin_r)).changed(); changed |= ui.add(egui::DragValue::new(&mut s.margin_v)).changed(); });
                ui.label("Ölçek X/Y %"); ui.horizontal(|ui| { changed |= ui.add(egui::DragValue::new(&mut s.scale_x)).changed(); changed |= ui.add(egui::DragValue::new(&mut s.scale_y)).changed(); }); ui.end_row();
            });
            let _ = &mut snap;
            if changed {
                if s != self.file.styles[self.style_idx] {
                    if !ui.memory(|m| m.focused().is_some()) || true { self.hist.record("Stil", &self.file); }
                    if s.name != old_name { for e in &mut self.file.events { if e.style == old_name { e.style = s.name.clone(); } } }
                    self.file.styles[self.style_idx] = s; self.touch();
                }
            }
        });
        self.show_styles = open;
    }

    fn lua_dialog(&mut self, ctx: &egui::Context) {
        let (spec, mut vals) = self.dialog.take().unwrap();
        let mut result: Option<Option<String>> = None;
        egui::Window::new("Otomasyon").collapsible(false).resizable(true).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            let mut rows: BTreeMap<i32, Vec<&osiosub_lua::DialogControl>> = BTreeMap::new();
            for c in &spec.controls { rows.entry(c.y).or_default().push(c); }
            for (_, mut cs) in rows {
                cs.sort_by_key(|c| c.x);
                ui.horizontal(|ui| {
                    for c in cs {
                        let w = (c.width.max(1) as f32) * 70.0;
                        match c.class.as_str() {
                            "label" => { ui.label(&c.label); }
                            "checkbox" => { if let Some(DlgVal::Bool(b)) = vals.get_mut(&c.name) { ui.checkbox(b, &c.label); } }
                            "edit" | "textbox" | "coloralpha" | "color" | "alpha" => {
                                if let Some(DlgVal::Str(s)) = vals.get_mut(&c.name) {
                                    if c.class == "textbox" { ui.add(egui::TextEdit::multiline(s).desired_width(w).desired_rows(c.height.max(3) as usize)); }
                                    else { ui.add(egui::TextEdit::singleline(s).desired_width(w)); }
                                }
                            }
                            "intedit" | "floatedit" | "slider" => {
                                if let Some(DlgVal::Num(n)) = vals.get_mut(&c.name) {
                                    let mut d = egui::DragValue::new(n);
                                    if let (Some(a), Some(b)) = (c.min, c.max) { d = d.range(a..=b); }
                                    if c.class == "intedit" { d = d.fixed_decimals(0); } else { d = d.speed(c.step.unwrap_or(0.1)); }
                                    ui.add(d);
                                }
                            }
                            "dropdown" => {
                                if let Some(DlgVal::Str(s)) = vals.get_mut(&c.name) {
                                    egui::ComboBox::from_id_salt(&c.name).selected_text(s.clone()).width(w).show_ui(ui, |ui| { for it in &c.items { ui.selectable_value(s, it.clone(), it); } });
                                }
                            }
                            _ => { ui.label(format!("[{}]", c.class)); }
                        }
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                let btns = if spec.buttons.is_empty() { vec!["OK".to_string(), "Cancel".to_string()] } else { spec.buttons.clone() };
                for b in &btns { if ui.button(b).clicked() { result = Some(Some(b.clone())); } }
            });
        });
        match result {
            Some(btn) => {
                // "Cancel"-like buttons return false in Aegisub
                let cancel = btn.as_deref().map_or(true, |b| b.eq_ignore_ascii_case("cancel") || b.eq_ignore_ascii_case("iptal"));
                let _ = self.worker.reply.send(DialogResult { button: if cancel { None } else { btn }, values: vals });
            }
            None => self.dialog = Some((spec, vals)),
        }
    }
}

fn ui_mods(r: &egui::Response) -> egui::Modifiers { r.ctx.input(|i| i.modifiers) }

impl eframe::App for OsioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_worker();
        self.handle_shortcuts(&ctx);
        self.fd.update(&ctx);
        if let Some(p) = self.fd.take_picked() {
            match self.fd_action {
                FileAction::Open => self.open_path(p),
                FileAction::SaveAs => { let mut p = p; if p.extension().is_none() { p.set_extension("ass"); } self.save_to(p) }
                FileAction::Export => { let mut p = p; if p.extension().is_none() { p.set_extension(self.export_ext); } self.save_to(p) }
                FileAction::LoadScript => { let _ = self.worker.tx.send(ToWorker::Load(p)); }
            }
        }
        // dropped files
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped { if p.extension().map_or(false, |e| e == "lua") { let _ = self.worker.tx.send(ToWorker::Load(p)); } else { self.open_path(p); } }

        let title = format!("{}{} — OsioSub", self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or("Adsız".into()), if self.dirty { " *" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::Panel::top("menu").show_inside(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if self.running { ui.spinner(); ui.label("makro çalışıyor…"); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n_err = self.issues.iter().filter(|i| i.severity == Severity::Error).count();
                    if ui.small_button(format!("QC: {} sorun ({} hata)", self.issues.len(), n_err)).clicked() { self.show_qc = true; }
                    ui.label(format!("{} satır · {} seçili", self.file.events.len(), self.sel.len()));
                });
            });
        });
        egui::Panel::bottom("editor").resizable(true).min_size(150.0).show_inside(ui, |ui| self.editor(ui));
        egui::CentralPanel::default().show_inside(ui, |ui| self.grid(ui));
        self.windows(&ctx);
    }
}
