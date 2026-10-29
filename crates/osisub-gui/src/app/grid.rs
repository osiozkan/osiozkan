use super::*;

impl App {
    pub(super) fn highlighted(text: &str, dim: bool) -> LayoutJob {
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

    pub(super) fn grid(&mut self, ui: &mut egui::Ui) {
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
        let rows = std::mem::take(&mut self.rows); // kopyalamadan ödünç al; fonksiyon sonunda geri konur
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
        self.rows = rows;
    }

    pub(super) fn time_field(ui: &mut egui::Ui, id: &str, buf: &mut String, current: i64) -> Option<i64> {
        let eid = egui::Id::unique(id);
        let focused = ui.memory(|m| m.has_focus(eid));
        if !focused { *buf = time::format_ass(current); }
        let r = ui.add(TextEdit::singleline(buf).id(eid).desired_width(92.0).font(egui::TextStyle::Monospace));
        if r.changed() { time::parse_ass(buf) } else { None }
    }

    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
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
}
