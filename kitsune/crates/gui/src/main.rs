use eframe::egui;
use egui_extras::{Column, TableBuilder};
use kitsune_core::{check, load_file, save_file, tags, time, Document, Event};
use kitsune_lua::Engine;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 780.0]).with_title("Kitsune"),
        ..Default::default()
    };
    eframe::run_native("Kitsune", opts, Box::new(|_| Ok(Box::new(App::new()))))
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Styles,
    Problems,
    Automation,
}

struct App {
    doc: Document,
    path_input: String,
    path: Option<PathBuf>,
    dirty: bool,
    sel: BTreeSet<usize>,
    active: Option<usize>,
    last_clicked: Option<usize>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    status: String,
    tab: Tab,
    shift_ms: String,
    find: String,
    replace: String,
    issues: Vec<check::Issue>,
    engine: Option<Engine>,
    script_input: String,
    log: String,
    edit_row: Option<usize>,
}

impl App {
    fn new() -> Self {
        let mut doc = Document::new();
        doc.events.push(Event { start: 1000, end: 3500, text: "Kitsune'ye hoş geldiniz".into(), ..Event::default() });
        App {
            doc, path_input: String::new(), path: None, dirty: false, sel: BTreeSet::from([0]), active: Some(0),
            last_clicked: Some(0), undo: vec![], redo: vec![], status: "Hazır".into(), tab: Tab::Styles,
            shift_ms: "0".into(), find: String::new(), replace: String::new(), issues: vec![],
            engine: Engine::new().ok(), script_input: String::new(), log: String::new(), edit_row: None,
        }
    }

    fn push_undo(&mut self) {
        self.edit_row = None;
        self.undo.push(self.doc.clone());
        if self.undo.len() > 200 { self.undo.remove(0); }
        self.redo.clear();
        self.dirty = true;
    }
    fn do_undo(&mut self) {
        if let Some(d) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, d));
            self.fix_sel();
        }
    }
    fn do_redo(&mut self) {
        if let Some(d) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, d));
            self.fix_sel();
        }
    }
    fn fix_sel(&mut self) {
        let n = self.doc.events.len();
        self.sel.retain(|&i| i < n);
        if self.active.map_or(false, |a| a >= n) { self.active = n.checked_sub(1); }
    }

    fn open(&mut self, p: PathBuf) {
        match load_file(&p) {
            Ok(d) => {
                self.status = format!("{} açıldı ({} satır)", p.display(), d.events.len());
                self.doc = d;
                self.path = Some(p);
                self.undo.clear();
                self.redo.clear();
                self.dirty = false;
                self.sel = BTreeSet::from([0]);
                self.active = if self.doc.events.is_empty() { None } else { Some(0) };
                self.issues.clear();
            }
            Err(e) => self.status = e,
        }
    }
    fn save(&mut self, p: PathBuf) {
        match save_file(&self.doc, &p) {
            Ok(()) => { self.status = format!("Kaydedildi: {}", p.display()); self.path = Some(p); self.dirty = false; }
            Err(e) => self.status = e,
        }
    }

    fn targets(&self) -> Vec<usize> {
        if self.sel.is_empty() { (0..self.doc.events.len()).collect() } else { self.sel.iter().copied().collect() }
    }

    fn run_macro(&mut self, i: usize) {
        let Some(engine) = &self.engine else { return };
        let before = self.doc.clone();
        let sel: Vec<usize> = self.sel.iter().copied().collect();
        match engine.run(i, &mut self.doc, &sel, self.active) {
            Ok(out) => {
                self.log = out.logs.join("\n");
                if out.cancelled {
                    self.status = "Makro iptal edildi".into();
                } else {
                    self.undo.push(before);
                    self.redo.clear();
                    self.dirty = true;
                    self.sel = out.selection.into_iter().collect();
                    self.active = out.active;
                    self.fix_sel();
                    self.status = "Makro tamamlandı".into();
                }
            }
            Err(e) => { self.log = e; self.doc = before; self.status = "Makro hata verdi".into(); }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) { self.do_undo(); }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y)) { self.do_redo(); }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            if let Some(p) = self.path.clone() { self.save(p); }
        }

        egui::Panel::top("menu").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Yeni").clicked() { *self = App { engine: self.engine.take(), ..App::new() }; }
                ui.label("Dosya:");
                ui.add(egui::TextEdit::singleline(&mut self.path_input).desired_width(320.0).hint_text("/yol/dosya.ass"));
                if ui.button("Aç").clicked() && !self.path_input.is_empty() { self.open(PathBuf::from(self.path_input.clone())); }
                if ui.button("Kaydet").clicked() {
                    let p = if !self.path_input.is_empty() { Some(PathBuf::from(&self.path_input)) } else { self.path.clone() };
                    if let Some(p) = p { self.save(p); }
                }
                ui.separator();
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶ Geri al")).clicked() { self.do_undo(); }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷ Yinele")).clicked() { self.do_redo(); }
                ui.separator();
                ui.label("Kaydır (ms):");
                ui.add(egui::TextEdit::singleline(&mut self.shift_ms).desired_width(70.0));
                if ui.button("Uygula").clicked() {
                    if let Ok(ms) = self.shift_ms.trim().parse::<i64>() {
                        self.push_undo();
                        for i in self.targets() { let e = &mut self.doc.events[i]; e.start = (e.start + ms).max(0); e.end = (e.end + ms).max(0); }
                    }
                }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.find).desired_width(110.0).hint_text("Bul"));
                ui.add(egui::TextEdit::singleline(&mut self.replace).desired_width(110.0).hint_text("Değiştir"));
                if ui.button("Değiştir").clicked() && !self.find.is_empty() {
                    self.push_undo();
                    let (f, r) = (self.find.clone(), self.replace.clone());
                    let mut n = 0;
                    for i in self.targets() { let e = &mut self.doc.events[i]; n += e.text.matches(&f).count(); e.text = e.text.replace(&f, &r); }
                    self.status = format!("{} değişiklik", n);
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.separator();
                ui.label(format!("{} satır, {} seçili{}", self.doc.events.len(), self.sel.len(), if self.dirty { " •değişti" } else { "" }));
            });
        });

        egui::Panel::right("side").default_size(340.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Styles, "Stiller");
                ui.selectable_value(&mut self.tab, Tab::Problems, "Sorunlar");
                ui.selectable_value(&mut self.tab, Tab::Automation, "Otomasyon (Lua)");
            });
            ui.separator();
            match self.tab {
                Tab::Styles => self.styles_ui(ui),
                Tab::Problems => self.problems_ui(ui),
                Tab::Automation => self.automation_ui(ui),
            }
        });

        egui::Panel::bottom("editor").min_size(170.0).show(ui, |ui| self.editor_ui(ui));
        egui::CentralPanel::default().show(ui, |ui| self.grid_ui(ui));
    }
}

impl App {
    fn grid_ui(&mut self, ui: &mut egui::Ui) {
        let n = self.doc.events.len();
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .column(Column::exact(44.0))
            .column(Column::exact(86.0))
            .column(Column::exact(86.0))
            .column(Column::exact(48.0))
            .column(Column::exact(100.0))
            .column(Column::exact(44.0))
            .column(Column::remainder().clip(true))
            .header(22.0, |mut h| {
                for t in ["#", "Başla", "Bitiş", "CPS", "Stil", "Kat.", "Metin"] { h.col(|ui| { ui.strong(t); }); }
            })
            .body(|body| {
                body.rows(22.0, n, |mut row| {
                    let i = row.index();
                    row.set_selected(self.sel.contains(&i));
                    let e = &self.doc.events[i];
                    let dur = e.duration().max(1) as f64 / 1000.0;
                    let cps = tags::visible_chars(&e.text) as f64 / dur;
                    let bad = self.issues.iter().any(|x| x.index == i);
                    let (s, en, cl, st, ly, tx) = (time::format_ass(e.start), time::format_ass(e.end), e.is_comment(), e.style.clone(), e.layer, tags::plain(&e.text).replace('\n', " ⏎ "));
                    row.col(|ui| { let t = format!("{}", i + 1); ui.label(if bad { egui::RichText::new(t).color(egui::Color32::LIGHT_RED) } else { egui::RichText::new(t) }); });
                    row.col(|ui| { ui.monospace(s); });
                    row.col(|ui| { ui.monospace(en); });
                    row.col(|ui| { let c = if cps > 20.0 { egui::Color32::LIGHT_RED } else { ui.visuals().text_color() }; ui.label(egui::RichText::new(format!("{:.1}", cps)).color(c)); });
                    row.col(|ui| { ui.label(st); });
                    row.col(|ui| { ui.label(ly.to_string()); });
                    row.col(|ui| { let t = egui::RichText::new(tx); ui.label(if cl { t.weak().italics() } else { t }); });
                    if row.response().clicked() { clicked = Some((i, row.response().ctx.input(|i| i.modifiers))); }
                });
            });
        if let Some((i, m)) = clicked {
            if m.shift {
                let a = self.last_clicked.unwrap_or(i);
                self.sel = (a.min(i)..=a.max(i)).collect();
            } else if m.command {
                if !self.sel.remove(&i) { self.sel.insert(i); }
                self.last_clicked = Some(i);
            } else {
                self.sel = BTreeSet::from([i]);
                self.last_clicked = Some(i);
            }
            self.active = Some(i);
        }
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("＋ Satır ekle").clicked() {
                self.push_undo();
                let at = self.active.map_or(self.doc.events.len(), |a| a + 1);
                let (s, st) = self.active.map_or((0, "Default".to_string()), |a| (self.doc.events[a].end, self.doc.events[a].style.clone()));
                self.doc.events.insert(at, Event { start: s, end: s + 2000, style: st, ..Event::default() });
                self.active = Some(at);
                self.sel = BTreeSet::from([at]);
            }
            if ui.button("Çoğalt").clicked() {
                if let Some(a) = self.active {
                    self.push_undo();
                    let e = self.doc.events[a].clone();
                    self.doc.events.insert(a + 1, e);
                    self.active = Some(a + 1);
                    self.sel = BTreeSet::from([a + 1]);
                }
            }
            if ui.button("Sil").clicked() && !self.sel.is_empty() {
                self.push_undo();
                for &i in self.sel.clone().iter().rev() { self.doc.events.remove(i); }
                self.sel.clear();
                self.active = self.active.and_then(|a| a.min(self.doc.events.len()).checked_sub(1).or(Some(0))).filter(|_| !self.doc.events.is_empty());
            }
            if ui.button("Birleştir").clicked() && self.sel.len() > 1 {
                self.push_undo();
                let idx: Vec<usize> = self.sel.iter().copied().collect();
                let text = idx.iter().map(|&i| self.doc.events[i].text.clone()).collect::<Vec<_>>().join("\\N");
                let end = idx.iter().map(|&i| self.doc.events[i].end).max().unwrap();
                self.doc.events[idx[0]].text = text;
                self.doc.events[idx[0]].end = end;
                for &i in idx[1..].iter().rev() { self.doc.events.remove(i); }
                self.sel = BTreeSet::from([idx[0]]);
                self.active = Some(idx[0]);
            }
            if ui.button("Zamana göre sırala").clicked() { self.push_undo(); self.doc.sort_by_start(); }
        });
        let Some(a) = self.active.filter(|&a| a < self.doc.events.len()) else { ui.label("Satır seçilmedi"); return };
        let before = self.doc.events[a].clone();
        let mut e = before.clone();
        let (mut s, mut en) = (time::format_ass(e.start), time::format_ass(e.end));
        let styles: Vec<String> = self.doc.styles.iter().map(|s| s.name.clone()).collect();
        ui.horizontal(|ui| {
            let mut cm = e.is_comment();
            if ui.checkbox(&mut cm, "Yorum").changed() { e.kind = if cm { kitsune_core::EventKind::Comment } else { kitsune_core::EventKind::Dialogue }; }
            ui.label("Başla");
            if ui.add(egui::TextEdit::singleline(&mut s).desired_width(80.0)).lost_focus() { if let Some(v) = time::parse_ass(&s) { e.start = v; } }
            ui.label("Bitiş");
            if ui.add(egui::TextEdit::singleline(&mut en).desired_width(80.0)).lost_focus() { if let Some(v) = time::parse_ass(&en) { e.end = v; } }
            ui.label(format!("Süre {:.2}s", e.duration() as f64 / 1000.0));
            egui::ComboBox::from_id_salt("style").selected_text(e.style.clone()).show_ui(ui, |ui| {
                for n in &styles { ui.selectable_value(&mut e.style, n.clone(), n); }
            });
            ui.label("Kat.");
            ui.add(egui::DragValue::new(&mut e.layer));
            ui.label("Aktör");
            ui.add(egui::TextEdit::singleline(&mut e.actor).desired_width(90.0));
            ui.label("Efekt");
            ui.add(egui::TextEdit::singleline(&mut e.effect).desired_width(90.0));
        });
        ui.add(egui::TextEdit::multiline(&mut e.text).desired_width(f32::INFINITY).desired_rows(4).font(egui::TextStyle::Monospace));
        let dur = e.duration().max(1) as f64 / 1000.0;
        ui.label(format!("{} karakter · {:.1} cps · en uzun satır {}", tags::visible_chars(&e.text), tags::visible_chars(&e.text) as f64 / dur, tags::max_line_len(&e.text)));
        if e != before {
            // Aynı satırdaki ardışık düzenlemeler tek geri-alma adımında birleşir.
            if self.edit_row != Some(a) {
                self.undo.push(self.doc.clone());
                self.redo.clear();
            }
            self.edit_row = Some(a);
            self.doc.events[a] = e;
            self.dirty = true;
        }
    }

    fn styles_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let before = self.doc.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for s in &mut self.doc.styles {
                egui::CollapsingHeader::new(s.name.clone()).id_salt(format!("st-{}", s.name)).show(ui, |ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut s.fontname)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut s.fontsize).prefix("boyut ")).changed();
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut s.bold, "B").changed();
                        changed |= ui.checkbox(&mut s.italic, "I").changed();
                        changed |= ui.checkbox(&mut s.underline, "U").changed();
                    });
                    changed |= ui.add(egui::DragValue::new(&mut s.outline).prefix("çerçeve ").speed(0.1)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut s.shadow).prefix("gölge ").speed(0.1)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut s.alignment).range(1..=9).prefix("hiza ")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut s.margin_v).prefix("dikey kenar ")).changed();
                    ui.label(format!("Renkler: {} {} {} {}", s.primary, s.secondary, s.outline_color, s.back));
                });
            }
            if ui.button("＋ Stil ekle").clicked() {
                let mut s = kitsune_core::Style::default();
                s.name = format!("Stil{}", self.doc.styles.len() + 1);
                self.doc.styles.push(s);
                changed = true;
            }
        });
        if changed { self.undo.push(before); self.redo.clear(); self.dirty = true; }
    }

    fn problems_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("Denetle").clicked() {
            self.issues = check::run(&self.doc, &check::Rules::default());
            self.status = format!("{} sorun bulundu", self.issues.len());
        }
        let mut go = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in &self.issues {
                if ui.selectable_label(false, format!("#{} {}", i.index + 1, i.message)).clicked() { go = Some(i.index); }
            }
        });
        if let Some(i) = go { self.sel = BTreeSet::from([i]); self.active = Some(i); }
    }

    fn automation_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.script_input).desired_width(220.0).hint_text("betik.lua yolu"));
            if ui.button("Yükle").clicked() {
                if let Some(e) = &self.engine {
                    self.status = match e.load_script(std::path::Path::new(&self.script_input)) {
                        Ok(n) => format!("{} makro yüklendi", n),
                        Err(er) => er,
                    };
                }
            }
        });
        let macros = self.engine.as_ref().map(|e| e.macros()).unwrap_or_default();
        let mut run = None;
        for (i, m) in macros.iter().enumerate() {
            if ui.button(&m.name).on_hover_text(&m.description).clicked() { run = Some(i); }
        }
        if let Some(i) = run { self.run_macro(i); }
        ui.separator();
        ui.label("Günlük:");
        ui.add(egui::TextEdit::multiline(&mut self.log).desired_width(f32::INFINITY).desired_rows(8).interactive(false));
    }
}
