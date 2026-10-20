//! Aegisub's line tools in the editor: the line clipboard (copy / cut / paste / paste over),
//! Select Lines, the styles manager with style storage, multiple replace (Subtitle Edit) and the
//! translation and styling assistants.

use super::*;
use super::tools::{file_name, read_subtitle};
use asscore::lines::{self, field, MatchMode, SelectAction, SelectOpts};
use asscore::{catalog, replace};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum StyleSide { Script, Storage }

pub(super) struct EditTools {
    /// A paste asked for from a menu: Some(over) until the clipboard text arrives.
    pub pending_paste: Option<bool>,
    /// Lines waiting for the "Paste over" field choice.
    pub paste_over: Option<Vec<Event>>,
    pub sel_opts: SelectOpts,
    pub sel_action: SelectAction,
    pub sel_msg: String,
    pub catalogs: Vec<String>,
    pub storage: Vec<Style>,
    /// Name of the catalog held in `storage`.
    pub storage_loaded: Option<String>,
    pub storage_sel: BTreeSet<usize>,
    pub script_sel: BTreeSet<usize>,
    pub side: StyleSide,
    pub new_catalog: String,
    pub confirm_delete: bool,
    pub style_msg: String,
    pub import: Option<(PathBuf, Vec<(Style, bool)>)>,
    pub mr_groups: Vec<replace::Group>,
    pub mr_loaded: bool,
    pub mr_group: usize,
    pub mr_text_only: bool,
    pub mr_sel_only: bool,
    pub mr_skip_comments: bool,
    pub mr_preview: Vec<(usize, String, bool)>,
    pub mr_msg: String,
    pub tr_line: usize,
    pub tr_block: usize,
    pub tr_text: String,
    pub tr_seek: bool,
    pub tr_focus: bool,
    pub tr_msg: String,
    pub sa_filter: String,
    pub sa_pick: usize,
    pub sa_seek: bool,
    pub sa_focus: bool,
}

impl Default for EditTools {
    fn default() -> Self {
        EditTools {
            pending_paste: None, paste_over: None,
            sel_opts: SelectOpts::default(), sel_action: SelectAction::Set, sel_msg: String::new(),
            catalogs: vec![], storage: vec![], storage_loaded: None, storage_sel: BTreeSet::new(), script_sel: BTreeSet::new(), side: StyleSide::Script,
            new_catalog: String::new(), confirm_delete: false, style_msg: String::new(), import: None,
            mr_groups: vec![], mr_loaded: false, mr_group: 0, mr_text_only: true, mr_sel_only: false, mr_skip_comments: true, mr_preview: vec![], mr_msg: String::new(),
            tr_line: 0, tr_block: 0, tr_text: String::new(), tr_seek: true, tr_focus: false, tr_msg: String::new(),
            sa_filter: String::new(), sa_pick: 0, sa_seek: true, sa_focus: false,
        }
    }
}

fn catalog_dir() -> PathBuf { crate::settings::config_dir().join("catalog") }
fn rules_file() -> PathBuf { crate::settings::config_dir().join("multi_replace.tsv") }

/// Does the clipboard hold subtitle rows (so Ctrl+V pastes lines even in a text box, as in Aegisub)?
fn looks_like_rows(s: &str) -> bool {
    let mut any = false;
    for l in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if asscore::ass::parse_event_line(l).is_none() { return false; }
        any = true;
    }
    any
}

/// A name not yet used by `styles` ("Yeni stil", "Yeni stil 2", …).
fn unique_name(styles: &[Style], base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while styles.iter().any(|s| s.name == name) { name = format!("{} {}", base, n); n += 1; }
    name
}

/// Selectable list of style names; click selects, Ctrl+click toggles, Shift+click extends.
fn style_list(ui: &mut egui::Ui, id: &str, styles: &[Style], sel: &mut BTreeSet<usize>, height: f32) -> Option<usize> {
    let mut clicked = None;
    egui::Frame::group(ui.style()).inner_margin(2.0).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt(id).max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(ui, |ui| {
            for (i, s) in styles.iter().enumerate() {
                if ui.selectable_label(sel.contains(&i), &s.name).clicked() {
                    let m = ui.input(|i| i.modifiers);
                    if m.command {
                        if !sel.remove(&i) { sel.insert(i); }
                    } else if m.shift && !sel.is_empty() {
                        let a = *sel.iter().next().unwrap();
                        for k in a.min(i)..=a.max(i) { sel.insert(k); }
                    } else {
                        sel.clear();
                        sel.insert(i);
                    }
                    clicked = Some(i);
                }
            }
        });
    });
    clicked
}

/// The style editor grid; true when something changed.
fn style_editor(ui: &mut egui::Ui, s: &mut Style) -> bool {
    let before = s.clone();
    egui::Grid::new("styled").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label(t("Name"));
        if ui.text_edit_singleline(&mut s.name).changed() { s.name = s.name.replace(',', ";"); }
        ui.label(t("Font")); ui.text_edit_singleline(&mut s.fontname); ui.end_row();
        ui.label(t("Size")); ui.add(egui::DragValue::new(&mut s.fontsize).range(1.0..=999.0)); ui.label(t("Spacing")); ui.add(egui::DragValue::new(&mut s.spacing).speed(0.1)); ui.end_row();
        ui.label(t("Scale X/Y")); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.scale_x)); ui.add(egui::DragValue::new(&mut s.scale_y)); }); ui.label(t("Angle")); ui.add(egui::DragValue::new(&mut s.angle)); ui.end_row();
        ui.label(t("Outline")); ui.add(egui::DragValue::new(&mut s.outline).speed(0.1).range(0.0..=99.0)); ui.label(t("Shadow")); ui.add(egui::DragValue::new(&mut s.shadow).speed(0.1).range(0.0..=99.0)); ui.end_row();
        ui.label(t("Alignment")); ui.add(egui::DragValue::new(&mut s.alignment).range(1..=9)); ui.label(t("Border style"));
        egui::ComboBox::from_id_salt("bs").selected_text(if s.border_style == 3 { t("Opaque box") } else { t("Outline + shadow") }).show_ui(ui, |ui| { ui.selectable_value(&mut s.border_style, 1, t("Outline + shadow")); ui.selectable_value(&mut s.border_style, 3, t("Opaque box")); });
        ui.end_row();
        ui.label(t("Margins L/R/V")); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut s.margin_l)); ui.add(egui::DragValue::new(&mut s.margin_r)); ui.add(egui::DragValue::new(&mut s.margin_v)); }); ui.label(t("Encoding")); ui.add(egui::DragValue::new(&mut s.encoding)); ui.end_row();
        ui.label(t("Style")); ui.horizontal(|ui| { ui.checkbox(&mut s.bold, "B"); ui.checkbox(&mut s.italic, "I"); ui.checkbox(&mut s.underline, "U"); ui.checkbox(&mut s.strikeout, "S"); }); ui.end_row();
        for (lbl, col) in [("Primary", &mut s.primary), ("Secondary", &mut s.secondary), ("Outline", &mut s.outline_color), ("Shadow", &mut s.back)] {
            ui.label(t(lbl));
            ui.horizontal(|ui| {
                let mut rgb = [col.r, col.g, col.b];
                if ui.color_edit_button_srgb(&mut rgb).changed() { col.r = rgb[0]; col.g = rgb[1]; col.b = rgb[2]; }
                let mut a = 255 - col.a as i32;
                ui.label(t("opacity"));
                if ui.add(egui::DragValue::new(&mut a).range(0..=255)).changed() { col.a = (255 - a) as u8; }
            });
            ui.end_row();
        }
    });
    *s != before
}

/// Plain-text blocks of a line the translation assistant walks through (indices into the parts).
fn text_blocks(parts: &[tags::Part]) -> Vec<usize> {
    parts.iter().enumerate().filter(|(_, p)| matches!(p, tags::Part::Text(t) if !t.trim().is_empty())).map(|(i, _)| i).collect()
}

impl App {
    // ------------------------------------------------------------ line clipboard

    /// Ctrl+C / Ctrl+X / Ctrl+V on the grid (egui turns those keys into clipboard events).
    pub(super) fn clipboard_events(&mut self, ctx: &egui::Context, typing: bool) {
        let (evs, shift) = ctx.input(|i| (i.events.iter().filter(|e| matches!(e, egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_))).cloned().collect::<Vec<_>>(), i.modifiers.shift));
        if evs.is_empty() { return; }
        let pending = self.edit.pending_paste.take();
        for e in evs {
            match e {
                egui::Event::Paste(s) => {
                    // in a text box only subtitle rows are taken over; other text is pasted there
                    if typing && pending.is_none() && !looks_like_rows(&s) { continue; }
                    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                    if pending.unwrap_or(shift) { self.begin_paste_over(&s); } else { self.paste_lines(&s); }
                }
                egui::Event::Copy if !typing => self.copy_lines(),
                egui::Event::Cut if !typing => self.cut_lines(),
                _ => {}
            }
        }
    }

    pub(super) fn copy_lines(&mut self) {
        let idx = self.sel_vec();
        if idx.is_empty() { return; }
        self.ctx.copy_text(lines::to_clipboard(&self.doc, &idx));
        self.last_status = tf!("{} lines copied", idx.len());
    }

    pub(super) fn cut_lines(&mut self) {
        let n = self.selection.len();
        self.copy_lines();
        self.delete_selected();
        self.last_status = tf!("{} lines cut", n);
    }

    /// Paste rows (or plain text lines) above the active line, as Aegisub does.
    pub(super) fn paste_lines(&mut self, text: &str) {
        let new = lines::from_clipboard(text);
        if new.is_empty() { return; }
        self.push_undo();
        let at = self.active.min(self.doc.events.len());
        let n = new.len();
        for (k, e) in new.into_iter().enumerate() { self.doc.events.insert(at + k, e); }
        self.selection = (at..at + n).collect();
        self.active = at;
        self.anchor = at;
        self.scroll_to = Some(at);
        self.buf_key = (usize::MAX, 0, 0);
        self.run_qc();
        self.last_status = tf!("{} lines pasted", n);
    }

    pub(super) fn begin_paste_over(&mut self, text: &str) {
        let new = lines::from_clipboard(text);
        if !new.is_empty() { self.edit.paste_over = Some(new); }
    }

    /// Overwrite the chosen fields of the target lines with the pasted lines.
    pub(super) fn apply_paste_over(&mut self, fields: u16) {
        let Some(new) = self.edit.paste_over.take() else { return };
        let rows = lines::paste_over_targets(&self.doc, &self.selection, self.active);
        if rows.is_empty() || fields == 0 { return; }
        self.push_undo();
        let done = lines::paste_over_rows(&mut self.doc, &rows, &new, fields);
        self.settings.paste_over_fields = fields;
        self.settings.save();
        if let Some(&f) = done.first() { self.selection = done.iter().copied().collect(); self.active = f; self.anchor = f; self.scroll_to = Some(f); }
        self.buf_key = (usize::MAX, 0, 0);
        self.run_qc();
        self.last_status = tf!("Pasted over {} lines", done.len());
    }

    pub(super) fn paste_over_window(&mut self, ctx: &egui::Context) {
        let Some(n) = self.edit.paste_over.as_ref().map(|v| v.len()) else { return };
        let mut fields = self.settings.paste_over_fields;
        let (mut ok, mut cancel) = (false, false);
        let targets = lines::paste_over_targets(&self.doc, &self.selection, self.active).len().min(n);
        egui::Window::new(t("Paste over")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(tf!("{} lines from the clipboard go over {} lines. Fields to overwrite:", n, targets));
            egui::Grid::new("paste_fields").num_columns(2).show(ui, |ui| {
                for (k, (bit, label)) in field::ALL.iter().enumerate() {
                    let mut on = fields & bit != 0;
                    if ui.checkbox(&mut on, t(label)).changed() { if on { fields |= bit; } else { fields &= !bit; } }
                    if k % 2 == 1 { ui.end_row(); }
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button(t("All")).clicked() { fields = field::ALL.iter().fold(0, |a, (b, _)| a | b); }
                if ui.small_button(t("None")).clicked() { fields = 0; }
                if ui.small_button(t("Times")).clicked() { fields = field::START | field::END; }
                if ui.small_button(t("Text")).clicked() { fields = field::TEXT; }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(fields != 0, egui::Button::new(RichText::new(t("Paste over")).strong())).clicked() { ok = true; }
                if ui.button(t("Cancel")).clicked() { cancel = true; }
            });
        });
        self.settings.paste_over_fields = fields;
        if ok { self.apply_paste_over(fields); } else if cancel { self.edit.paste_over = None; }
    }

    // ------------------------------------------------------------ Select Lines

    pub(super) fn run_select_lines(&mut self) {
        match lines::select_lines(&self.doc, &self.edit.sel_opts) {
            Ok(m) => {
                let new = lines::combine(&self.selection, &m, self.edit.sel_action);
                self.edit.sel_msg = tf!("{} lines match · {} selected", m.len(), new.len());
                if let Some(&f) = new.iter().next() {
                    self.selection = new;
                    if !self.selection.contains(&self.active) { self.active = f; self.anchor = f; }
                    self.scroll_to = Some(self.active);
                } else {
                    // never leave the grid without an active line
                    self.select_only(self.active.min(self.doc.events.len().saturating_sub(1)));
                }
            }
            Err(e) => self.edit.sel_msg = e,
        }
    }

    pub(super) fn select_lines_ui(&mut self, ui: &mut egui::Ui) {
        let o = &mut self.edit.sel_opts;
        let r = ui.horizontal(|ui| {
            ui.label(t("Match"));
            ui.add(egui::TextEdit::singleline(&mut o.pattern).desired_width(260.0))
        }).inner;
        ui.horizontal(|ui| {
            ui.label(t("In"));
            let names = [t("Text (raw)"), t("Style"), t("Actor"), t("Effect"), t("Text (no tags)")];
            egui::ComboBox::from_id_salt("self").selected_text(names[o.field as usize]).show_ui(ui, |ui| {
                for (k, l) in names.iter().enumerate() { ui.selectable_value(&mut o.field, k as u8, *l); }
            });
            ui.radio_value(&mut o.mode, MatchMode::Contains, t("Contains"));
            ui.radio_value(&mut o.mode, MatchMode::Exact, t("Exactly"));
            ui.radio_value(&mut o.mode, MatchMode::Regex, t("Regex"));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut o.case_sensitive, t("Match case"));
            ui.checkbox(&mut o.invert, t("Does not match"));
            ui.separator();
            ui.checkbox(&mut o.dialogue, t("Dialogue"));
            ui.checkbox(&mut o.comments, t("Comments"));
        });
        ui.horizontal(|ui| {
            for (a, l) in [(SelectAction::Set, t("Set selection")), (SelectAction::Add, t("Add to selection")), (SelectAction::Subtract, t("Remove from selection")), (SelectAction::Intersect, t("Intersect"))] {
                ui.radio_value(&mut self.edit.sel_action, a, l);
            }
        });
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        ui.horizontal(|ui| {
            if ui.button(RichText::new(t("Select")).strong()).clicked() || enter { self.run_select_lines(); }
            if !self.edit.sel_msg.is_empty() { ui.weak(&self.edit.sel_msg); }
        });
    }

    // ------------------------------------------------------------ styles manager + storage

    pub(super) fn ensure_storage(&mut self) {
        if self.edit.storage_loaded.as_deref() == Some(self.settings.style_catalog.as_str()) { return; }
        let dir = catalog_dir();
        self.edit.catalogs = catalog::list(&dir);
        if !self.edit.catalogs.contains(&self.settings.style_catalog) { self.edit.catalogs.push(self.settings.style_catalog.clone()); }
        self.edit.storage = catalog::load(&dir, &self.settings.style_catalog);
        self.edit.storage_loaded = Some(self.settings.style_catalog.clone());
        self.edit.storage_sel.clear();
        if self.edit.side == StyleSide::Storage { self.edit.side = StyleSide::Script; }
    }

    fn save_storage(&mut self) {
        match catalog::save(&catalog_dir(), &self.settings.style_catalog, &self.edit.storage) {
            Ok(()) => { self.edit.catalogs = catalog::list(&catalog_dir()); }
            Err(e) => self.edit.style_msg = tf!("Could not save the storage: {}", e),
        }
    }

    /// Copy styles into the script (same name → replaced). Returns (added, replaced).
    pub(super) fn styles_into_script(&mut self, styles: Vec<Style>) -> (usize, usize) {
        if styles.is_empty() { return (0, 0); }
        self.push_undo();
        let (mut added, mut replaced) = (0, 0);
        for s in styles { if catalog::upsert(&mut self.doc.styles, s) { replaced += 1; } else { added += 1; } }
        self.edit.style_msg = tf!("{} styles added, {} replaced", added, replaced);
        (added, replaced)
    }

    pub(super) fn styles_into_storage(&mut self, styles: Vec<Style>) {
        self.ensure_storage();
        let n = styles.len();
        for s in styles { catalog::upsert(&mut self.edit.storage, s); }
        self.save_storage();
        self.edit.style_msg = tf!("{} styles copied to the storage", n);
    }

    pub(super) fn apply_style_to_selection(&mut self, name: &str) {
        self.push_undo();
        for i in self.sel_vec() { if let Some(e) = self.doc.events.get_mut(i) { e.style = name.to_string(); } }
        self.edit.style_msg = tf!("Style {} set on {} lines", name, self.selection.len());
        self.run_qc();
    }

    fn storage_column(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(t("Storage"));
            let mut pick = None;
            egui::ComboBox::from_id_salt("catalog").selected_text(&self.settings.style_catalog).show_ui(ui, |ui| {
                for c in &self.edit.catalogs { if ui.selectable_label(*c == self.settings.style_catalog, c).clicked() { pick = Some(c.clone()); } }
            });
            if let Some(c) = pick { self.settings.style_catalog = c; self.settings.save(); self.edit.confirm_delete = false; self.ensure_storage(); }
            if !self.edit.confirm_delete {
                if ui.small_button(t("Delete")).on_hover_text(t("Delete this storage")).clicked() { self.edit.confirm_delete = true; }
            } else {
                if ui.small_button(RichText::new(t("Really delete?")).color(Color32::from_rgb(255, 110, 100))).clicked() {
                    let _ = std::fs::remove_file(catalog_dir().join(format!("{}.sty", self.settings.style_catalog)));
                    let rest = catalog::list(&catalog_dir());
                    self.settings.style_catalog = rest.first().cloned().unwrap_or_else(|| "Default".into());
                    self.settings.save();
                    self.edit.confirm_delete = false;
                    self.edit.storage_loaded = None;
                    self.ensure_storage();
                }
                if ui.small_button(t("Cancel")).clicked() { self.edit.confirm_delete = false; }
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.edit.new_catalog).desired_width(130.0).hint_text(t("new storage name")));
            let name = self.edit.new_catalog.trim().to_string();
            if ui.add_enabled(catalog::valid_name(&name), egui::Button::new(t("Create"))).clicked() {
                self.settings.style_catalog = name;
                self.settings.save();
                self.edit.storage = vec![];
                self.edit.storage_loaded = Some(self.settings.style_catalog.clone());
                self.edit.storage_sel.clear();
                self.save_storage();
                self.edit.new_catalog.clear();
            }
        });
        if style_list(ui, "storage_list", &self.edit.storage, &mut self.edit.storage_sel, 160.0).is_some() { self.edit.side = StyleSide::Storage; }
        ui.horizontal_wrapped(|ui| {
            let sel: Vec<usize> = self.edit.storage_sel.iter().copied().filter(|&i| i < self.edit.storage.len()).collect();
            if ui.add_enabled(!sel.is_empty(), egui::Button::new(RichText::new(t("Copy to script ⏵")).strong())).clicked() {
                let st: Vec<Style> = sel.iter().map(|&i| self.edit.storage[i].clone()).collect();
                self.styles_into_script(st);
            }
            if ui.button(t("New")).clicked() {
                let mut s = Style::default();
                s.name = unique_name(&self.edit.storage, "Yeni stil");
                self.edit.storage.push(s);
                self.edit.storage_sel = [self.edit.storage.len() - 1].into_iter().collect();
                self.edit.side = StyleSide::Storage;
                self.save_storage();
            }
            if ui.add_enabled(!sel.is_empty(), egui::Button::new(t("Delete"))).clicked() {
                for &i in sel.iter().rev() { self.edit.storage.remove(i); }
                self.edit.storage_sel.clear();
                self.edit.side = StyleSide::Script;
                self.save_storage();
            }
        });
    }

    fn script_column(&mut self, ui: &mut egui::Ui) {
        ui.strong(t("Current script"));
        ui.add_space(26.0);
        if let Some(i) = style_list(ui, "script_list", &self.doc.styles, &mut self.edit.script_sel, 160.0) { self.edit.side = StyleSide::Script; self.style_sel = i; }
        let sel: Vec<usize> = self.edit.script_sel.iter().copied().filter(|&i| i < self.doc.styles.len()).collect();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!sel.is_empty(), egui::Button::new(RichText::new(t("⏴ Copy to storage")).strong())).clicked() {
                let st: Vec<Style> = sel.iter().map(|&i| self.doc.styles[i].clone()).collect();
                self.styles_into_storage(st);
            }
            if ui.button(t("New")).clicked() {
                self.push_undo();
                let mut s = Style::default();
                s.name = unique_name(&self.doc.styles, "Yeni stil");
                self.doc.styles.push(s);
                self.style_sel = self.doc.styles.len() - 1;
                self.edit.script_sel = [self.style_sel].into_iter().collect();
                self.edit.side = StyleSide::Script;
            }
            if ui.add_enabled(sel.len() == 1, egui::Button::new(t("Copy"))).clicked() {
                self.push_undo();
                let mut c = self.doc.styles[sel[0]].clone();
                c.name = unique_name(&self.doc.styles, &format!("{} kopya", c.name));
                self.doc.styles.insert(sel[0] + 1, c);
                self.style_sel = sel[0] + 1;
                self.edit.script_sel = [self.style_sel].into_iter().collect();
            }
            if ui.add_enabled(!sel.is_empty() && sel.len() < self.doc.styles.len(), egui::Button::new(t("Delete"))).clicked() {
                self.push_undo();
                for &i in sel.iter().rev() { self.doc.styles.remove(i); }
                self.edit.script_sel.clear();
                self.style_sel = 0;
            }
            if ui.add_enabled(sel.len() == 1, egui::Button::new("⏶")).on_hover_text(t("Move up")).clicked() {
                self.push_undo();
                self.style_sel = catalog::move_style(&mut self.doc.styles, sel[0], -1);
                self.edit.script_sel = [self.style_sel].into_iter().collect();
            }
            if ui.add_enabled(sel.len() == 1, egui::Button::new("⏷")).on_hover_text(t("Move down")).clicked() {
                self.push_undo();
                self.style_sel = catalog::move_style(&mut self.doc.styles, sel[0], 1);
                self.edit.script_sel = [self.style_sel].into_iter().collect();
            }
            if ui.button(t("Sort")).on_hover_text(t("Sort by name")).clicked() {
                self.push_undo();
                self.doc.styles.sort_by_key(|s| s.name.to_lowercase());
                self.edit.script_sel.clear();
            }
            if ui.button(t("Import from script…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter(t("Subtitles"), &["ass", "ssa"]).pick_file() {
                    match read_subtitle(&p) {
                        Some(sc) if !sc.styles.is_empty() => self.edit.import = Some((p, sc.styles.into_iter().map(|s| (s, true)).collect())),
                        _ => self.edit.style_msg = t("No styles in that file").into(),
                    }
                }
            }
            if ui.add_enabled(sel.len() == 1, egui::Button::new(t("Apply to selected lines"))).clicked() {
                let name = self.doc.styles[sel[0]].name.clone();
                self.apply_style_to_selection(&name);
            }
        });
    }

    pub(super) fn styles_ui(&mut self, ui: &mut egui::Ui) {
        self.ensure_storage();
        ui.columns(2, |cols| {
            let (a, b) = cols.split_at_mut(1);
            self.storage_column(&mut a[0]);
            self.script_column(&mut b[0]);
        });
        if !self.edit.style_msg.is_empty() { ui.weak(&self.edit.style_msg); }
        // import from another script
        let mut import_done = None;
        if let Some((p, list)) = self.edit.import.as_mut() {
            ui.separator();
            ui.label(tf!("Styles in {}:", file_name(p)));
            ui.horizontal_wrapped(|ui| { for (s, on) in list.iter_mut() { ui.checkbox(on, &s.name); } });
            ui.horizontal(|ui| {
                let n = list.iter().filter(|x| x.1).count();
                if ui.add_enabled(n > 0, egui::Button::new(tf!("Import ({})", n))).clicked() { import_done = Some(true); }
                if ui.button(t("Cancel")).clicked() { import_done = Some(false); }
            });
        }
        if let Some(go) = import_done {
            if let Some((_, list)) = self.edit.import.take() {
                if go { self.styles_into_script(list.into_iter().filter(|x| x.1).map(|x| x.0).collect()); }
            }
        }
        // editor for the style picked last (script or storage)
        ui.separator();
        match self.edit.side {
            StyleSide::Script => {
                if self.doc.styles.is_empty() { return; }
                let sel = self.style_sel.min(self.doc.styles.len() - 1);
                let mut s = self.doc.styles[sel].clone();
                let old_name = s.name.clone();
                ui.horizontal(|ui| { ui.strong(tf!("Script style: {}", old_name)); });
                crate::preview::style_sample(ui, &s, "Yuzu — Şimdi ığüşöç ÇĞİÖŞÜ", egui::Vec2::new(ui.available_width().min(520.0), 56.0));
                if style_editor(ui, &mut s) {
                    self.begin_edit(7_000_000 + sel as u64);
                    if s.name != old_name {
                        for e in self.doc.events.iter_mut().filter(|e| e.style == old_name) { e.style = s.name.clone(); }
                    }
                    self.doc.styles[sel] = s;
                }
            }
            StyleSide::Storage => {
                let Some(&sel) = self.edit.storage_sel.iter().next() else { return };
                if sel >= self.edit.storage.len() { return; }
                let mut s = self.edit.storage[sel].clone();
                ui.horizontal(|ui| { ui.strong(tf!("Storage style: {}", s.name)); });
                crate::preview::style_sample(ui, &s, "Yuzu — Şimdi ığüşöç ÇĞİÖŞÜ", egui::Vec2::new(ui.available_width().min(520.0), 56.0));
                if style_editor(ui, &mut s) {
                    self.edit.storage[sel] = s;
                    self.save_storage();
                }
            }
        }
    }

    // ------------------------------------------------------------ multiple replace

    pub(super) fn load_rules(&mut self) {
        if self.edit.mr_loaded { return; }
        self.edit.mr_groups = match std::fs::read(rules_file()) {
            Ok(b) => replace::parse(&String::from_utf8_lossy(&b)),
            Err(_) => replace::defaults(),
        };
        self.edit.mr_loaded = true;
    }

    fn save_rules(&mut self) {
        let _ = std::fs::create_dir_all(crate::settings::config_dir());
        if let Err(e) = std::fs::write(rules_file(), replace::write(&self.edit.mr_groups)) { self.edit.mr_msg = tf!("Could not save the rules: {}", e); }
    }

    pub(super) fn preview_rules(&mut self) {
        match replace::compile(&self.edit.mr_groups) {
            Ok(c) => {
                let rows = (self.edit.mr_sel_only && self.selection.len() > 1).then(|| self.sel_vec());
                self.edit.mr_preview = replace::preview(&self.doc, &c, rows.as_deref(), self.edit.mr_text_only, self.edit.mr_skip_comments).into_iter().map(|(r, s)| (r, s, true)).collect();
                self.edit.mr_msg = tf!("{} lines would change", self.edit.mr_preview.len());
            }
            Err(e) => { self.edit.mr_preview.clear(); self.edit.mr_msg = e; }
        }
    }

    pub(super) fn apply_rules(&mut self) {
        let chosen: Vec<(usize, String)> = self.edit.mr_preview.iter().filter(|x| x.2).map(|x| (x.0, x.1.clone())).collect();
        if chosen.is_empty() { return; }
        self.push_undo();
        for (r, s) in &chosen { if let Some(e) = self.doc.events.get_mut(*r) { e.text = s.clone(); } }
        self.edit.mr_preview.clear();
        self.edit.mr_msg = tf!("{} lines changed", chosen.len());
        self.run_qc();
    }

    pub(super) fn multi_replace_ui(&mut self, ui: &mut egui::Ui) {
        self.load_rules();
        let before = self.edit.mr_groups.clone();
        let mut del_group = None;
        let mut move_group: Option<(usize, isize)> = None;
        ui.horizontal(|ui| {
            ui.label(t("Groups"));
            for (k, g) in self.edit.mr_groups.iter_mut().enumerate() {
                ui.checkbox(&mut g.enabled, "").on_hover_text(t("Group on/off"));
                if ui.selectable_label(self.edit.mr_group == k, &g.name).clicked() { self.edit.mr_group = k; }
                ui.add_space(6.0);
            }
            if ui.small_button("+").on_hover_text(t("New group")).clicked() {
                self.edit.mr_groups.push(replace::Group { name: tf!("Group {}", self.edit.mr_groups.len() + 1), enabled: true, rules: vec![] });
                self.edit.mr_group = self.edit.mr_groups.len() - 1;
            }
        });
        if self.edit.mr_group >= self.edit.mr_groups.len() { self.edit.mr_group = self.edit.mr_groups.len().saturating_sub(1); }
        if let Some(g) = self.edit.mr_groups.get_mut(self.edit.mr_group) {
            let k = self.edit.mr_group;
            ui.horizontal(|ui| {
                ui.label(t("Name"));
                ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(200.0));
                if ui.small_button("⏴").on_hover_text(t("Move left")).clicked() { move_group = Some((k, -1)); }
                if ui.small_button("⏵").on_hover_text(t("Move right")).clicked() { move_group = Some((k, 1)); }
                if ui.small_button(t("Delete group")).clicked() { del_group = Some(k); }
            });
            let mut del_rule = None;
            let mut swap: Option<(usize, usize)> = None;
            let n = g.rules.len();
            egui::ScrollArea::vertical().max_height(220.0).id_salt("mr_rules").show(ui, |ui| {
                egui::Grid::new("mr_grid").num_columns(7).spacing([6.0, 3.0]).striped(true).show(ui, |ui| {
                    ui.label(""); ui.strong(t("Kind")); ui.strong(t("Find")); ui.strong(t("Replace with")); ui.strong(t("Note")); ui.label(""); ui.label(""); ui.end_row();
                    for (i, r) in g.rules.iter_mut().enumerate() {
                        ui.checkbox(&mut r.enabled, "");
                        egui::ComboBox::from_id_salt(("mr_kind", i)).width(110.0).selected_text(t(r.kind.label())).show_ui(ui, |ui| {
                            for kd in replace::Kind::ALL { ui.selectable_value(&mut r.kind, kd, t(kd.label())); }
                        });
                        // fixed sizes: text boxes in a grid otherwise keep the first frame's narrow width
                        ui.add_sized([190.0, 20.0], egui::TextEdit::singleline(&mut r.find).font(egui::TextStyle::Monospace));
                        ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut r.replace).font(egui::TextStyle::Monospace));
                        ui.add_sized([150.0, 20.0], egui::TextEdit::singleline(&mut r.note));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).clicked() { swap = Some((i, i - 1)); }
                            if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).clicked() { swap = Some((i, i + 1)); }
                        });
                        if ui.small_button("🗑").on_hover_text(t("Delete rule")).clicked() { del_rule = Some(i); }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = del_rule { g.rules.remove(i); }
            if let Some((a, b)) = swap { g.rules.swap(a, b); }
            if ui.button(t("+ Rule")).clicked() { g.rules.push(replace::Rule::new(replace::Kind::Plain, "", "", "")); }
        }
        if let Some(k) = del_group { self.edit.mr_groups.remove(k); self.edit.mr_group = k.saturating_sub(1); }
        if let Some((k, d)) = move_group {
            let j = (k as isize + d).clamp(0, self.edit.mr_groups.len() as isize - 1) as usize;
            self.edit.mr_groups.swap(k, j);
            self.edit.mr_group = j;
        }
        if self.edit.mr_groups != before { self.save_rules(); self.edit.mr_preview.clear(); }
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.edit.mr_text_only, t("Leave override tags alone"));
            ui.checkbox(&mut self.edit.mr_sel_only, t("Selected lines only"));
            ui.checkbox(&mut self.edit.mr_skip_comments, t("Skip comments"));
        });
        ui.horizontal(|ui| {
            if ui.button(RichText::new(t("Preview")).strong()).clicked() { self.preview_rules(); }
            let n = self.edit.mr_preview.iter().filter(|x| x.2).count();
            if ui.add_enabled(n > 0, egui::Button::new(tf!("Apply ({} lines)", n))).clicked() { self.apply_rules(); }
            if !self.edit.mr_msg.is_empty() { ui.weak(&self.edit.mr_msg); }
        });
        if self.edit.mr_preview.is_empty() { return; }
        let mut goto = None;
        let rows: Vec<(usize, String)> = self.edit.mr_preview.iter().map(|(r, _, _)| (*r, self.doc.events.get(*r).map(|e| e.text.clone()).unwrap_or_default())).collect();
        ui.push_id("mr_preview", |ui| {
            TableBuilder::new(ui).striped(true).max_scroll_height(240.0).auto_shrink([false, true])
                .column(Column::exact(24.0)).column(Column::exact(44.0))
                .column(Column::remainder().at_least(120.0).clip(true)).column(Column::remainder().at_least(120.0).clip(true))
                .header(20.0, |mut h| { for name in ["", "#", t("Before"), t("After")] { h.col(|ui| { ui.strong(name); }); } })
                .body(|body| body.rows(20.0, rows.len(), |mut row| {
                    let k = row.index();
                    row.col(|ui| { ui.checkbox(&mut self.edit.mr_preview[k].2, ""); });
                    row.col(|ui| { if ui.link(format!("{}", rows[k].0 + 1)).clicked() { goto = Some(rows[k].0); } });
                    row.col(|ui| { ui.add(egui::Label::new(rows[k].1.as_str()).truncate()).on_hover_text(rows[k].1.as_str()); });
                    let after = self.edit.mr_preview[k].1.clone();
                    row.col(|ui| { ui.add(egui::Label::new(RichText::new(after.as_str()).color(Color32::from_rgb(120, 200, 140))).truncate()).on_hover_text(after.as_str()); });
                }));
        });
        if let Some(r) = goto { self.select_only(r); self.scroll_to = Some(r); }
    }

    // ------------------------------------------------------------ translation assistant

    pub(super) fn start_translation(&mut self) {
        self.edit.tr_line = self.active;
        self.edit.tr_block = 0;
        self.edit.tr_text.clear();
        self.edit.tr_focus = true;
        self.edit.tr_msg.clear();
        if self.edit.tr_line < self.doc.events.len() && text_blocks(&tags::split_parts(&self.doc.events[self.edit.tr_line].text)).is_empty() { self.tr_step(true); }
    }

    /// Go to the next (or previous) block, crossing lines; lines without text are skipped.
    fn tr_step(&mut self, forward: bool) -> bool {
        let n = self.doc.events.len();
        let (mut line, mut block) = (self.edit.tr_line as i64, self.edit.tr_block as i64);
        loop {
            block += if forward { 1 } else { -1 };
            let count = self.doc.events.get(line as usize).map(|e| text_blocks(&tags::split_parts(&e.text)).len() as i64).unwrap_or(0);
            if block >= 0 && block < count { break; }
            line += if forward { 1 } else { -1 };
            if line < 0 || line >= n as i64 { self.edit.tr_msg = if forward { t("End of the file") } else { t("Start of the file") }.into(); return false; }
            let c = text_blocks(&tags::split_parts(&self.doc.events[line as usize].text)).len() as i64;
            if c == 0 { block = if forward { -1 } else { 0 }; continue; }
            block = if forward { 0 } else { c - 1 };
            break;
        }
        let changed_line = line as usize != self.edit.tr_line;
        self.edit.tr_line = line as usize;
        self.edit.tr_block = block as usize;
        self.edit.tr_text.clear();
        self.edit.tr_msg.clear();
        self.edit.tr_focus = true;
        if changed_line {
            self.select_only(self.edit.tr_line);
            self.scroll_to = Some(self.edit.tr_line);
            if self.edit.tr_seek && (self.media.video.is_some() || self.media.audio.is_some()) { let s = self.doc.events[self.edit.tr_line].start; self.media.seek(s); }
        }
        true
    }

    /// Put the typed translation in place of the current block and move on.
    pub(super) fn tr_commit(&mut self) {
        let line = self.edit.tr_line;
        let typed = self.edit.tr_text.trim_end_matches(['\n', '\r']).replace("\r\n", "\n").replace('\n', "\\N");
        if !typed.trim().is_empty() {
            if let Some(e) = self.doc.events.get(line) {
                let mut parts = tags::split_parts(&e.text);
                if let Some(&pi) = text_blocks(&parts).get(self.edit.tr_block) {
                    parts[pi] = tags::Part::Text(typed);
                    self.push_undo();
                    self.doc.events[line].text = tags::join_parts(&parts);
                    self.run_qc_line(line);
                }
            }
        }
        self.tr_step(true);
    }

    pub(super) fn translate_ui(&mut self, ui: &mut egui::Ui) {
        let n = self.doc.events.len();
        if n == 0 { return; }
        // the assistant follows lines picked in the grid
        if self.active != self.edit.tr_line && self.selection.len() == 1 {
            self.edit.tr_line = self.active.min(n - 1);
            self.edit.tr_block = 0;
            self.edit.tr_text.clear();
            self.edit.tr_focus = true;
        }
        let line = self.edit.tr_line.min(n - 1);
        let e = self.doc.events[line].clone();
        let parts = tags::split_parts(&e.text);
        let blocks = text_blocks(&parts);
        ui.horizontal(|ui| {
            ui.strong(tf!("Line {} of {}", line + 1, n));
            ui.weak(format!("{} – {} · {}{}", time::format_ass(e.start), time::format_ass(e.end), e.style, if e.actor.is_empty() { String::new() } else { format!(" · {}", e.actor) }));
            if blocks.len() > 1 { ui.weak(tf!("block {}/{}", self.edit.tr_block.min(blocks.len() - 1) + 1, blocks.len())); }
        });
        if let Some(r) = self.reference_text(line) {
            ui.horizontal_wrapped(|ui| { ui.weak(t("Reference:")); ui.label(RichText::new(r).italics().color(Color32::from_rgb(150, 170, 200))); });
        }
        ui.label(t("Original"));
        let cur = blocks.get(self.edit.tr_block).copied();
        let mut job = egui::text::LayoutJob::default();
        let font = egui::FontId::proportional(16.0);
        let normal = ui.visuals().text_color();
        for (i, p) in parts.iter().enumerate() {
            let (s, fmt) = match p {
                tags::Part::Block(b) => (format!("{{{}}}", b), egui::TextFormat { font_id: egui::FontId::monospace(12.0), color: Color32::GRAY, ..Default::default() }),
                tags::Part::Text(t) if Some(i) == cur => (t.clone(), egui::TextFormat { font_id: font.clone(), color: Color32::BLACK, background: Color32::from_rgb(240, 200, 90), ..Default::default() }),
                tags::Part::Text(t) => (t.clone(), egui::TextFormat { font_id: font.clone(), color: normal, ..Default::default() }),
            };
            job.append(&s, 0.0, fmt);
        }
        egui::Frame::group(ui.style()).show(ui, |ui| { ui.set_min_width(ui.available_width()); ui.label(job); });
        ui.label(t("Translation (Enter: accept and go on · Shift+Enter: line break)"));
        let id = egui::Id::new("translation_box");
        // Enter commits; Shift+Enter is left to the text box (new line → \N)
        let enter = ui.memory(|m| m.has_focus(id)) && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        ui.add(egui::TextEdit::multiline(&mut self.edit.tr_text).id(id).desired_rows(2).desired_width(f32::INFINITY).font(egui::TextStyle::Body));
        if self.edit.tr_focus { ui.memory_mut(|m| m.request_focus(id)); self.edit.tr_focus = false; }
        ui.horizontal(|ui| {
            if ui.button(RichText::new(t("Accept and next")).strong()).clicked() || enter { self.tr_commit(); }
            if ui.button(t("◀ Previous")).clicked() { self.tr_step(false); }
            if ui.button(t("Skip ▶")).clicked() { self.tr_step(true); }
            if ui.button(t("Insert original")).clicked() {
                if let Some(tags::Part::Text(t)) = cur.and_then(|c| parts.get(c)) { self.edit.tr_text = t.replace("\\N", "\n"); }
                self.edit.tr_focus = true;
            }
            if ui.button(t("▶ line")).on_hover_text(self.cmd_hint("play_line")).clicked() { self.play_part(PlayPart::Line); self.edit.tr_focus = true; }
            ui.checkbox(&mut self.edit.tr_seek, t("Seek video to the line"));
        });
        if !self.edit.tr_msg.is_empty() { ui.colored_label(Color32::from_rgb(235, 190, 80), &self.edit.tr_msg); }
    }

    // ------------------------------------------------------------ styling assistant

    fn sa_matches(&self) -> Vec<String> {
        let f = commands::fold(self.edit.sa_filter.trim());
        let mut v: Vec<(u8, String)> = self.doc.styles.iter().filter_map(|s| {
            let n = commands::fold(&s.name);
            if f.is_empty() { Some((1, s.name.clone())) } else if n.starts_with(&f) { Some((0, s.name.clone())) } else if n.contains(&f) { Some((1, s.name.clone())) } else { None }
        }).collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| x.1).collect()
    }

    /// Give the active line `style` and go to the next line.
    pub(super) fn sa_apply(&mut self, style: &str) {
        let i = self.active;
        if i >= self.doc.events.len() { return; }
        if self.doc.events[i].style != style { self.push_undo(); self.doc.events[i].style = style.to_string(); self.run_qc_line(i); }
        self.edit.sa_filter.clear();
        self.edit.sa_pick = 0;
        self.edit.sa_focus = true;
        if i + 1 < self.doc.events.len() {
            self.select_only(i + 1);
            self.scroll_to = Some(i + 1);
            if self.edit.sa_seek && (self.media.video.is_some() || self.media.audio.is_some()) { let s = self.doc.events[i + 1].start; self.media.seek(s); }
        }
    }

    pub(super) fn style_assist_ui(&mut self, ui: &mut egui::Ui) {
        let n = self.doc.events.len();
        if n == 0 { return; }
        let i = self.active.min(n - 1);
        let e = self.doc.events[i].clone();
        ui.horizontal(|ui| {
            ui.strong(tf!("Line {} of {}", i + 1, n));
            ui.weak(format!("{} – {}", time::format_ass(e.start), time::format_ass(e.end)));
            ui.label(tf!("Style: {}", e.style));
        });
        egui::Frame::group(ui.style()).show(ui, |ui| { ui.set_min_width(ui.available_width()); ui.label(RichText::new(tags::display_text(&e.text).replace('\n', " ⏎ ")).size(16.0)); });
        let matches = self.sa_matches();
        if self.edit.sa_pick >= matches.len() { self.edit.sa_pick = 0; }
        let id = egui::Id::new("style_assist_filter");
        let focused = ui.memory(|m| m.has_focus(id));
        let (enter, up, down) = if focused { ui.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::Enter), i.consume_key(egui::Modifiers::NONE, Key::ArrowUp), i.consume_key(egui::Modifiers::NONE, Key::ArrowDown))) } else { (false, false, false) };
        if up { self.edit.sa_pick = self.edit.sa_pick.saturating_sub(1); }
        if down && self.edit.sa_pick + 1 < matches.len() { self.edit.sa_pick += 1; }
        ui.horizontal(|ui| {
            ui.label(t("Style name"));
            if ui.add(egui::TextEdit::singleline(&mut self.edit.sa_filter).id(id).desired_width(220.0).hint_text(t("type a few letters, Enter applies"))).changed() { self.edit.sa_pick = 0; }
        });
        if self.edit.sa_focus { ui.memory_mut(|m| m.request_focus(id)); self.edit.sa_focus = false; }
        let mut pick = None;
        egui::ScrollArea::vertical().max_height(180.0).id_salt("sa_list").show(ui, |ui| {
            for (k, s) in matches.iter().enumerate() {
                if ui.selectable_label(k == self.edit.sa_pick, s).clicked() { pick = Some(s.clone()); }
            }
        });
        if enter { pick = matches.get(self.edit.sa_pick).cloned(); }
        ui.horizontal(|ui| {
            if ui.button(t("◀ Previous")).clicked() { self.step_line(-1); self.edit.sa_focus = true; }
            if ui.button(t("Skip ▶")).clicked() { self.step_line(1); self.edit.sa_focus = true; }
            if ui.button(t("▶ line")).on_hover_text(self.cmd_hint("play_line")).clicked() { self.play_part(PlayPart::Line); self.edit.sa_focus = true; }
            ui.checkbox(&mut self.edit.sa_seek, t("Seek video to the line"));
        });
        if let Some(s) = pick { self.sa_apply(&s); }
    }
}
