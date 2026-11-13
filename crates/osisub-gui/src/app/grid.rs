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
        if self.doc.script.dialogues().next().is_none() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Henüz altyazı satırı yok").size(18.0).weak());
                ui.label(RichText::new("Bir altyazı dosyasını (.ass .srt .vtt …) buraya sürükleyin, Ctrl+O ile açın ya da Ctrl+Enter ile ilk satırı ekleyin.").weak());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 470.0).max(0.0) / 2.0);
                    if ui.button("Altyazı aç…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() { self.request(super::Guarded::OpenPath(p)); }
                    }
                    if ui.button("Videodaki altyazıyı aç…").clicked() {
                        if let Some(v) = rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file() {
                            match osisub_media::mux::subtitle_tracks(&v) { Ok(t) => self.tracks_win = Some((v, t)), Err(e) => self.status = e }
                        }
                    }
                    if ui.button("İlk satırı ekle").clicked() { let at = self.doc.insert_after_active(); self.scroll_to = Some(at); }
                    if ui.button("Komut paleti").on_hover_text("Ctrl+Shift+P").clicked() { self.palette = Some((String::new(), 0)); }
                });
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 470.0).max(0.0) / 2.0);
                    if ui.button("Video / ses aç…").on_hover_text("Videosuz çalışmak için: Video → Sahte video aç…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Ortam", &["mkv", "mp4", "avi", "webm", "mov", "m2ts", "ts", "mka", "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"]).pick_file() {
                            self.media.open(p, ui.ctx());
                            self.show_media = true;
                        }
                    }
                    if ui.button("Kullanım kılavuzu").on_hover_text("F1").clicked() { self.guide = Some(String::new()); }
                    if ui.button("Kısayollar").clicked() { self.help_open = true; }
                });
                let recent: Vec<String> = self.settings.recent.iter().filter(|r| std::path::Path::new(r).is_file()).take(8).cloned().collect();
                if !recent.is_empty() {
                    ui.add_space(16.0);
                    ui.label(RichText::new("Son açılanlar").strong());
                    for r in recent {
                        let name = std::path::Path::new(&r).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| r.clone());
                        if ui.link(name).on_hover_text(&r).clicked() { self.request(super::Guarded::OpenPath(std::path::PathBuf::from(r))); }
                    }
                }
            });
            return;
        }
        let mods = ui.input(|i| i.modifiers);
        let has_ref = !self.ref_lines.is_empty();
        use crate::settings::GridCol;
        let cols: Vec<GridCol> = self.settings.grid_columns().into_iter().filter(|c| self.settings.grid_visible(*c)).collect();
        let col_w = |c: GridCol| -> f32 {
            match c {
                GridCol::Layer => 28.0,
                GridCol::Start | GridCol::End => 86.0,
                GridCol::Dur => 52.0,
                GridCol::Cps | GridCol::Len => 44.0,
                GridCol::Style => 90.0,
                GridCol::Actor | GridCol::Effect => 80.0,
                GridCol::Margins => 3.0 * 34.0,
            }
        };
        let fixed_w: f32 = 44.0 + cols.iter().map(|c| col_w(*c)).sum::<f32>();
        let avail_w = ui.available_width();
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(Align::Center))
            .column(Column::exact(44.0));
        for c in &cols {
            table = match c {
                GridCol::Style | GridCol::Actor | GridCol::Effect => table.column(Column::initial(col_w(*c)).clip(true)),
                GridCol::Margins => table.column(Column::exact(34.0)).column(Column::exact(34.0)).column(Column::exact(34.0)),
                _ => table.column(Column::exact(col_w(*c))),
            };
        }
        if has_ref {
            // Kalan genişliği Referans ve Metin eşit paylaşsın.
            let w = ((avail_w - fixed_w - 16.0) / 2.0).max(120.0);
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
                let mut titles: Vec<&str> = vec!["#"];
                for c in &cols {
                    match c {
                        GridCol::Layer => titles.push("K"),
                        GridCol::Start => titles.push("Başlangıç"),
                        GridCol::End => titles.push("Bitiş"),
                        GridCol::Dur => titles.push("Süre"),
                        GridCol::Cps => titles.push("KSS"),
                        GridCol::Len => titles.push("Uz."),
                        GridCol::Style => titles.push("Stil"),
                        GridCol::Actor => titles.push("Konuşan"),
                        GridCol::Effect => titles.push("Efekt"),
                        GridCol::Margins => titles.extend(["Sol", "Sağ", "Dik"]),
                    }
                }
                if has_ref { titles.push("Referans"); }
                titles.push("Metin");
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
                    // Yazım göstergesi (sözlük yüklüyse): satır numarası kırmızı
                    let misspelled = match (&self.spell, d.comment) {
                        (Some(sp), false) => {
                            let mut c = self.spell_rows.borrow_mut();
                            if c.0 != self.doc.rev { *c = (self.doc.rev, Default::default()); }
                            *c.1.entry(ei).or_insert_with(|| osisub_spell::words_of(&d.text).iter().any(|w| osisub_spell::should_flag(sp, w)))
                        }
                        _ => false,
                    };
                    row.col(|ui| {
                        let r = ui.label(col((pos + 1).to_string(), misspelled.then_some(theme::BAD)));
                        if misspelled { r.on_hover_text("Bu satırda yazım hatası olabilir"); }
                    });
                    for c in &cols {
                        match c {
                            GridCol::Layer => { row.col(|ui| { ui.label(col(d.layer.to_string(), if d.layer != 0 { Some(theme::ACCENT) } else { None })); }); }
                            GridCol::Start => { row.col(|ui| { ui.label(col(time::format_ass(d.start_time), if overlap { Some(theme::BAD) } else { None })); }); }
                            GridCol::End => { row.col(|ui| { ui.label(col(time::format_ass(d.end_time), None)); }); }
                            GridCol::Dur => { row.col(|ui| { ui.label(col(format!("{:.2}", d.duration() as f64 / 1000.0), flags.and_then(|f| f.iter().find(|k| matches!(k, Kind::TooShort | Kind::TooLong | Kind::NegativeDuration)).map(|_| theme::WARN)))); }); }
                            GridCol::Cps => { row.col(|ui| { ui.label(col(if d.comment { String::new() } else if cps.is_finite() { format!("{:.0}", cps) } else { "∞".into() }, flags.and_then(|f| f.contains(&Kind::ReadingSpeed).then_some(theme::BAD)))); }); }
                            GridCol::Len => {
                                let n = if d.comment || qc::is_typeset(d) { 0 } else { osisub_core::tags::max_line_len(&d.text) };
                                row.col(|ui| { ui.label(col(if n == 0 { String::new() } else { n.to_string() }, (n > self.rules.max_line_len).then_some(theme::BAD))); });
                            }
                            GridCol::Style => { row.col(|ui| { ui.label(RichText::new(&d.style).color(if flags.map_or(false, |f| f.contains(&Kind::UnknownStyle)) { theme::BAD } else { Color32::from_gray(180) })); }); }
                            GridCol::Actor => { row.col(|ui| { ui.label(RichText::new(&d.actor).weak()); }); }
                            GridCol::Effect => { row.col(|ui| { ui.label(RichText::new(&d.effect).weak()); }); }
                            GridCol::Margins => {
                                for m in [d.margin_l, d.margin_r, d.margin_t] {
                                    row.col(|ui| { ui.label(col(if m == 0 { String::new() } else { m.to_string() }, None)); });
                                }
                            }
                        }
                    }
                    if has_ref {
                        let refs = &self.ref_lines;
                        let end = refs.partition_point(|r| r.0 < d.end_time);
                        let best = refs[..end].iter().rev().take(8).filter(|r| r.1 > d.start_time).max_by_key(|r| r.1.min(d.end_time) - r.0.max(d.start_time));
                        row.col(|ui| { if let Some(r) = best { ui.add(egui::Label::new(RichText::new(&r.2).italics().color(theme::ACCENT)).truncate()); } });
                    }
                    let tag_err = flags.map_or(false, |f| f.contains(&Kind::TagError));
                    row.col(|ui| {
                        let r = ui.add(egui::Label::new(Self::highlighted(&d.text, dim)).truncate());
                        if tag_err {
                            // etiket hatası: metnin altında kırmızı çizgi
                            ui.painter().hline(r.rect.x_range(), r.rect.bottom() - 1.0, egui::Stroke::new(1.5, theme::BAD));
                        }
                    });
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
        // Aegisub düzenleme kutusu düğmeleri: seçimi sarar ya da imleçte durumu çevirir
        let mut tag_used = false;
        {
            let st = self.doc.script.style_by_name(&nd.style).cloned().unwrap_or_default();
            let (a, b) = self.editor_sel.filter(|s| s.0 == at).map(|s| (s.1, s.2)).unwrap_or_else(|| { let n = nd.text.chars().count(); (n, n) });
            ui.horizontal(|ui| {
                for (name, label, tip, on) in [("b", "K", "Kalın (\\b)", st.bold), ("i", "İ", "İtalik (\\i)", st.italic), ("u", "A", "Altı çizili (\\u)", st.underline), ("s", "Ü", "Üstü çizili (\\s)", st.strikeout)] {
                    let mut t = RichText::new(label).strong();
                    if name == "i" { t = t.italics(); }
                    if name == "u" { t = t.underline(); }
                    if name == "s" { t = t.strikethrough(); }
                    if ui.small_button(t).on_hover_text(format!("{tip} — seçimi sarar, seçim yoksa imleçte açar/kapatır")).clicked() {
                        nd.text = osisub_core::overrides::toggle_bool_tag(&nd.text, a, b, name, on);
                        tag_used = true;
                    }
                }
                ui.separator();
                for (name, label) in [("1c", "Renk"), ("3c", "Kenar"), ("4c", "Gölge")] {
                    let cur = osisub_core::overrides::tag_value_before(&nd.text, a, name).or_else(|| if name == "1c" { osisub_core::overrides::tag_value_before(&nd.text, a, "c") } else { None })
                        .and_then(|v| osisub_core::color::Color::parse(&v))
                        .or_else(|| osisub_core::color::Color::parse(match name { "1c" => &st.color1, "3c" => &st.color3, _ => &st.color4 }))
                        .unwrap_or(osisub_core::color::Color::WHITE);
                    let mut rgb = [cur.r, cur.g, cur.b];
                    ui.label(RichText::new(label).small());
                    if crate::color_ui::button(ui, name, &mut rgb) {
                        let c = osisub_core::color::Color { r: rgb[0], g: rgb[1], b: rgb[2], a: 0 };
                        let t = osisub_core::overrides::set_tag_range(&nd.text, a, b, if name == "1c" && nd.text.contains("\\c&") { "c" } else { name }, &c.to_tag_string());
                        nd.text = t;
                        tag_used = true;
                    }
                }
                ui.separator();
                if ui.small_button("\\N").on_hover_text("Satır sonu ekle").clicked() {
                    let chars: Vec<char> = nd.text.chars().collect();
                    let p = a.min(chars.len());
                    nd.text = chars[..p].iter().collect::<String>() + "\\N" + &chars[p..].iter().collect::<String>();
                    tag_used = true;
                }
            });
        }
        // Çok uzun metin (ör. \p1 çizimi) kutuyu büyütüp ızgarayı itmesin: kaydırılabilir alan
        let max_h = (ui.ctx().content_rect().height() * 0.3).max(90.0);
        let out = egui::ScrollArea::vertical().id_salt("editor_text").max_height(max_h).show(ui, |ui| {
            TextEdit::multiline(&mut nd.text).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).layouter(&mut layouter).hint_text("Altyazı metni — satır sonu için \\N").show(ui)
        }).inner;
        if tag_used {
            // eklenen etiketlerden sonra seçim aynı metni göstersin (ardışık düğmeler için)
            if let Some((_, a, b)) = self.editor_sel.filter(|s| s.0 == at) {
                self.editor_sel = osisub_core::overrides::remap_selection(&d.text, &nd.text, a, b).map(|(x, y)| (at, x, y));
            }
        } else if let Some(c) = out.cursor_range {
            self.editor_cursor = Some((at, c.primary.index.into()));
            let (p, q): (usize, usize) = (c.primary.index.into(), c.secondary.index.into());
            self.editor_sel = Some((at, p.min(q), p.max(q)));
        }
        let mut split_req: Option<bool> = None; // Some(false) = orantılı süre, Some(true) = video karesi
        out.response.context_menu(|ui| {
            if ui.button("İmleçte böl (süre metne orantılı)").clicked() { split_req = Some(false); ui.close(); }
            if ui.add_enabled(self.media.framerate().is_some(), egui::Button::new("İmleçte böl (video karesinde)")).clicked() { split_req = Some(true); ui.close(); }
        });
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
        if let Some(at_frame) = split_req {
            if nd == d {
                self.split_at_cursor(at_frame);
                return;
            }
        }
        if nd != d {
            let key = if tag_used { String::new() } else if nd.text != d.text { format!("text:{}", at) } else if nd.start_time != d.start_time || nd.end_time != d.end_time { format!("time:{}", at) } else { String::new() };
            let desc = if tag_used { "Etiket ekle" } else if nd.text != d.text { "Metin düzenle" } else { "Satırı düzenle" };
            self.doc.edit_keyed(desc, if key.is_empty() { None } else { Some(&key) }, |tx| tx.replace(at, Entry::Dialogue(nd)));
        }
    }
}
