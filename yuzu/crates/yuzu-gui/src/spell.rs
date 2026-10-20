//! Spell checking panel (Hunspell dictionaries through `yuzu-spell`; Turkish is downloaded on demand).

use super::*;
use std::sync::mpsc::{channel, Receiver};
use yuzu_spell::{self as spell, Misspelling, Speller};

pub(super) struct SpellState {
    pub lang: String,
    pub speller: Option<Speller>,
    pub results: Vec<Misspelling>,
    pub ignore: std::collections::HashSet<String>,
    pub sel: usize,
    pub sugg: Vec<String>,
    pub fix: String,
    pub dl: Option<Receiver<Result<(), String>>>,
    pub msg: String,
}

impl Default for SpellState {
    fn default() -> Self {
        SpellState { lang: "tr_TR".into(), speller: None, results: vec![], ignore: Default::default(), sel: 0, sugg: vec![], fix: String::new(), dl: None, msg: String::new() }
    }
}

impl App {
    pub(super) fn spell_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(rx) = &self.spell.dl {
            match rx.try_recv() {
                Ok(r) => {
                    self.spell.dl = None;
                    self.spell.msg = match r { Ok(()) => t("Dictionary downloaded.").to_string(), Err(e) => tf!("Download failed: {}", e) };
                }
                Err(_) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        ui.horizontal(|ui| {
            ui.label(t("Language"));
            egui::ComboBox::from_id_salt("spell_lang").selected_text(self.spell.lang.clone()).show_ui(ui, |ui| {
                for l in ["tr_TR", "en_US"] {
                    if ui.selectable_value(&mut self.spell.lang, l.to_string(), l).clicked() { self.spell.speller = None; self.spell.results.clear(); }
                }
            });
        });
        if !spell::available(&self.spell.lang) {
            ui.label(t("No dictionary installed for this language (LibreOffice dictionaries, downloaded from the internet; tr_TR ≈ 38 MB)."));
            let busy = self.spell.dl.is_some();
            if ui.add_enabled(!busy, egui::Button::new(t("Download dictionary"))).clicked() {
                let lang = self.spell.lang.clone();
                let (tx, rx) = channel();
                self.spell.dl = Some(rx);
                std::thread::spawn(move || { let _ = tx.send(spell::download(&lang)); });
            }
            if busy { ui.horizontal(|ui| { ui.spinner(); ui.label(t("Downloading…")); }); }
            ui.label(&self.spell.msg);
            return;
        }
        ui.horizontal(|ui| {
            if ui.button(RichText::new(t("Check")).strong()).clicked() {
                if self.spell.speller.is_none() {
                    match Speller::load(&self.spell.lang) { Ok(s) => self.spell.speller = Some(s), Err(e) => self.spell.msg = e }
                }
                if let Some(sp) = &self.spell.speller {
                    self.spell.results = spell::check_script(sp, &self.doc.events, &self.spell.ignore);
                    self.spell.sel = 0;
                    self.spell.sugg.clear();
                    self.spell.msg = tf!("{} misspelled words", self.spell.results.len());
                }
            }
            ui.weak(&self.spell.msg);
        });
        ui.separator();
        let mut goto = None;
        let mut replace: Option<(String, String)> = None;
        let mut ignore: Option<String> = None;
        let mut add: Option<String> = None;
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().id_salt("spell_list").max_height(360.0).show(&mut cols[0], |ui| {
                for (i, m) in self.spell.results.iter().enumerate() {
                    if ui.selectable_label(self.spell.sel == i, format!("{}  ({})", m.word, m.events.len())).clicked() {
                        self.spell.sel = i;
                        self.spell.fix = m.word.clone();
                        self.spell.sugg = self.spell.speller.as_ref().map(|s| s.suggest(&m.word, 8)).unwrap_or_default();
                        goto = m.events.first().copied();
                    }
                }
            });
            let ui = &mut cols[1];
            if let Some(m) = self.spell.results.get(self.spell.sel).cloned() {
                ui.label(RichText::new(&m.word).strong().size(18.0));
                ui.weak(tf!("in {} lines", m.events.len()));
                ui.horizontal_wrapped(|ui| {
                    for s in self.spell.sugg.clone() { if ui.button(&s).clicked() { self.spell.fix = s; } }
                });
                ui.text_edit_singleline(&mut self.spell.fix);
                ui.horizontal(|ui| {
                    if ui.button(t("Fix all")).clicked() { replace = Some((m.word.clone(), self.spell.fix.clone())); }
                    if ui.button(t("Ignore")).clicked() { ignore = Some(m.word.clone()); }
                    if ui.button(t("Add to dictionary")).clicked() { add = Some(m.word.clone()); }
                });
            }
        });
        if let Some(i) = goto { if i < self.doc.events.len() { self.select_only(i); self.scroll_to = Some(i); } }
        if let Some((from, to)) = replace {
            if !to.is_empty() && to != from {
                let rows: Vec<usize> = self.spell.results.iter().find(|m| m.word == from).map(|m| m.events.clone()).unwrap_or_default();
                self.push_undo();
                for r in rows { if let Some(e) = self.doc.events.get_mut(r) { e.text = spell::replace_word(&e.text, &from, &to); } }
                self.spell.results.retain(|m| m.word != from);
                self.spell.sel = self.spell.sel.min(self.spell.results.len().saturating_sub(1));
                self.run_qc();
            }
        }
        if let Some(w) = ignore {
            self.spell.ignore.insert(w.clone());
            self.spell.results.retain(|m| m.word != w);
            self.spell.sel = self.spell.sel.min(self.spell.results.len().saturating_sub(1));
        }
        if let Some(w) = add {
            if let Some(sp) = self.spell.speller.as_mut() { sp.add_user_word(&w); }
            self.spell.results.retain(|m| m.word != w);
            self.spell.sel = self.spell.sel.min(self.spell.results.len().saturating_sub(1));
        }
    }
}
