use crate::i18n::tr;
use crate::i18n::trf;
use super::*;

impl App {
    /// `tags`: 0 etiketleri göster, 1 her bloğu `{…}` olarak sadeleştir, 2 gizle (çizim satırı hep kısaltılır).
    pub(super) fn highlighted(text: &str, dim: bool, tags: u8) -> LayoutJob {
        let mut job = LayoutJob::default();
        let base = if dim { Color32::from_gray(120) } else { Color32::from_gray(225) };
        for p in osisub_core::tags::split(text) {
            match p {
                osisub_core::tags::Part::Block(b) => match tags {
                    0 => job.append(&format!("{{{}}}", b), 0.0, TextFormat { color: theme::TAG_COLOR, font_id: egui::FontId::monospace(11.0), ..Default::default() }),
                    1 => job.append("{…}", 0.0, TextFormat { color: theme::TAG_COLOR, font_id: egui::FontId::monospace(11.0), ..Default::default() }),
                    _ => {}
                },
                osisub_core::tags::Part::Text(t) => job.append(&t.replace("\\N", " ⏎ ").replace("\\n", " ⏎ "), 0.0, TextFormat { color: base, ..Default::default() }),
            }
        }
        job
    }

    pub(super) fn grid(&mut self, ui: &mut egui::Ui) {
        if self.bookmark_ids.0 != self.doc.rev { self.bookmark_ids = (self.doc.rev, osisub_core::bookmarks::ids(&self.doc.script)); }
        ui.horizontal(|ui| {
            ui.label(tr("Ara:"));
            ui.add(TextEdit::singleline(&mut self.filter).hint_text(tr("Süz (metin, stil, konuşan)…")).desired_width(260.0));
            if !self.filter.is_empty() && ui.small_button(tr("x")).clicked() { self.filter.clear(); }
            ui.label(RichText::new(trf!("{} / {} satır", self.rows.len(), self.doc.script.dialogues().count())).weak());
        });
        if self.doc.script.dialogues().next().is_none() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Henüz altyazı satırı yok")).size(18.0).weak());
                ui.label(RichText::new(tr("Bir altyazı dosyasını (.ass .srt .vtt …) buraya sürükleyin, Ctrl+O ile açın ya da Ctrl+Enter ile ilk satırı ekleyin.")).weak());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 470.0).max(0.0) / 2.0);
                    if ui.button(tr("Altyazı aç…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() { self.request(super::Guarded::OpenPath(p)); }
                    }
                    if ui.button(tr("Videodaki altyazıyı aç…")).clicked() {
                        if let Some(v) = rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file() {
                            match osisub_media::mux::subtitle_tracks(&v) { Ok(t) => self.tracks_win = Some((v, t)), Err(e) => self.status = e }
                        }
                    }
                    if ui.button(tr("İlk satırı ekle")).clicked() { let at = self.doc.insert_after_active(); self.scroll_to = Some(at); }
                    if ui.button(tr("Komut paleti")).on_hover_text(tr("Ctrl+Shift+P")).clicked() { self.palette = Some((String::new(), 0)); }
                });
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 470.0).max(0.0) / 2.0);
                    if ui.button(tr("Video / ses aç…")).on_hover_text(tr("Videosuz çalışmak için: Video → Sahte video aç…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Ortam", &["mkv", "mp4", "avi", "webm", "mov", "m2ts", "ts", "mka", "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"]).pick_file() {
                            self.media.open(p, ui.ctx());
                            self.show_media = true;
                        }
                    }
                    if ui.button(tr("Kullanım kılavuzu")).on_hover_text(tr("F1")).clicked() { self.guide = Some(String::new()); }
                    if ui.button(tr("Kısayollar")).clicked() { self.help_open = true; }
                });
                ui.add_space(12.0);
                ui.label(RichText::new(tr("Hızlı başla")).strong());
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((ui.available_width() - 640.0).max(0.0) / 2.0);
                    if ui.button(tr("Ham videodan çeviriye başla…")).on_hover_text(tr("Video açılır; videoda altyazı izi varsa onu referans (çeviri kaynağı) yapmanız için izler penceresi, yoksa yakılı (hardsub) altyazıyı okuma penceresi açılır")).clicked() {
                        if let Some(v) = rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file() {
                            self.media.open(v.clone(), ui.ctx());
                            self.show_media = true;
                            match osisub_media::mux::subtitle_tracks(&v) {
                                Ok(t) if !t.is_empty() => self.tracks_win = Some((v, t)),
                                _ => self.hardsub = Some(super::windows::HardsubWin::new(Some(v))),
                            }
                        }
                    }
                    if ui.button(tr("Çeviri metnini yerleştir…")).on_hover_text(tr("Düz metin çeviriyi zamanlanmış satırlara sırayla yerleştir")).clicked() { self.text_import = Some(super::windows::TextImportWin { speakers: true, ..Default::default() }); }
                    if ui.button(tr("Sesten altyazı (Whisper)…")).clicked() { self.whisper = Some(super::windows::WhisperWin::default()); }
                    if ui.button(tr("PGS / VobSub OCR…")).clicked() { self.ocr = Some(super::windows::OcrWin::default()); }
                });
                let recent: Vec<String> = self.settings.recent.iter().filter(|r| std::path::Path::new(r).is_file()).take(8).cloned().collect();
                if !recent.is_empty() {
                    ui.add_space(16.0);
                    ui.label(RichText::new(tr("Son açılanlar")).strong());
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
                        GridCol::Layer => titles.push(tr("K")),
                        GridCol::Start => titles.push(tr("Başlangıç")),
                        GridCol::End => titles.push(tr("Bitiş")),
                        GridCol::Dur => titles.push(tr("Süre")),
                        GridCol::Cps => titles.push(tr("KSS")),
                        GridCol::Len => titles.push(tr("Uz.")),
                        GridCol::Style => titles.push(tr("Stil")),
                        GridCol::Actor => titles.push(tr("Konuşan")),
                        GridCol::Effect => titles.push(tr("Efekt")),
                        GridCol::Margins => titles.extend(["Sol", "Sağ", "Dik"]),
                    }
                }
                if has_ref { titles.push(tr("Referans")); }
                titles.push(tr("Metin"));
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
                    let bookmarked = !d.extra.is_empty() && d.extra.iter().any(|id| self.bookmark_ids.1.contains(id));
                    row.col(|ui| {
                        let txt = if bookmarked { format!("» {}", pos + 1) } else { (pos + 1).to_string() };
                        let r = ui.label(col(txt, if misspelled { Some(theme::BAD) } else if bookmarked { Some(theme::ACCENT) } else { None }));
                        if misspelled { r.on_hover_text(tr("Bu satırda yazım hatası olabilir")); } else if bookmarked {
                            let note = osisub_core::bookmarks::get(&self.doc.script, ei).unwrap_or_default();
                            r.on_hover_text(if note.is_empty() { "Yer imi".to_string() } else { format!("Yer imi: {note}") });
                        }
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
                        let r = ui.add(egui::Label::new(Self::highlighted(&d.text, dim, self.settings.grid_tags)).truncate());
                        if tag_err {
                            // etiket hatası: metnin altında kırmızı çizgi
                            ui.painter().hline(r.rect.x_range(), r.rect.bottom() - 1.0, egui::Stroke::new(1.5, theme::BAD));
                        }
                    });
                    let resp = row.response();
                    if resp.clicked() { clicked = Some(pos); }
                    if resp.secondary_clicked() && sel_snapshot.binary_search(&ei).is_err() { ctx_row = Some(ei); }
                    resp.context_menu(|ui| {
                        if ui.button(tr("Önce satır ekle")).clicked() { action = Some(RowAction::InsertBefore); ui.close(); }
                        if ui.button(tr("Sonra satır ekle")).clicked() { action = Some(RowAction::InsertAfter); ui.close(); }
                        if ui.button(tr("Çoğalt")).clicked() { action = Some(RowAction::Duplicate); ui.close(); }
                        if ui.button(tr("Sil")).clicked() { action = Some(RowAction::Delete); ui.close(); }
                        if ui.button(tr("Birleştir")).clicked() { action = Some(RowAction::Merge); ui.close(); }
                        if ui.button(tr("Yorum yap / kaldır")).clicked() { action = Some(RowAction::ToggleComment); ui.close(); }
                        ui.separator();
                        if ui.button(tr("Kopyala")).clicked() { action = Some(RowAction::Copy); ui.close(); }
                        if ui.button(tr("Kes")).clicked() { action = Some(RowAction::Cut); ui.close(); }
                        if ui.button(tr("Yapıştır (pano)")).clicked() { action = Some(RowAction::Paste); ui.close(); }
                        ui.menu_button(tr("Stil"), |ui| {
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
                RowAction::Paste => self.status = tr("Yapıştırmak için Ctrl+V kullanın").into(),
                RowAction::SetStyle(n) => self.doc.set_style_selected(&n),
            }
            self.scroll_to = Some(self.doc.active);
        }
        if let Some(pos) = clicked {
            let ei = rows[pos];
            if mods.shift {
                // çapa, süzgeç/silme sonrası satır listesinin dışında kalmış olabilir
                let anchor = self.anchor.min(rows.len().saturating_sub(1));
                let (a, b) = (anchor.min(pos), anchor.max(pos));
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
            ui.label(RichText::new(tr("Düzenlemek için bir satır seçin")).weak());
            return;
        };
        let styles: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        let mut nd = d.clone();
        ui.horizontal(|ui| {
            ui.checkbox(&mut nd.comment, tr("Yorum"));
            ui.label(tr("Katman"));
            ui.add(egui::DragValue::new(&mut nd.layer));
            ui.label(tr("Başlangıç"));
            if let Some(t) = Self::time_field(ui, "start_f", &mut self.start_buf, d.start_time) { nd.start_time = t; }
            ui.label(tr("Bitiş"));
            if let Some(t) = Self::time_field(ui, "end_f", &mut self.end_buf, d.end_time) { nd.end_time = t; }
            ui.label(RichText::new(format!("{:.3} sn", d.duration() as f64 / 1000.0)).monospace());
            ui.label(tr("Stil"));
            egui::ComboBox::from_id_salt("style_cb").selected_text(nd.style.clone()).show_ui(ui, |ui| {
                for s in &styles { ui.selectable_value(&mut nd.style, s.clone(), s); }
            });
            ui.label(tr("Konuşan"));
            ui.add(TextEdit::singleline(&mut nd.actor).desired_width(90.0));
        });
        ui.horizontal(|ui| {
            ui.label(tr("Kenar boşluğu"));
            ui.add(egui::DragValue::new(&mut nd.margin_l));
            ui.add(egui::DragValue::new(&mut nd.margin_r));
            let mut v = nd.margin_t;
            if ui.add(egui::DragValue::new(&mut v)).changed() { nd.margin_t = v; nd.margin_b = v; }
            ui.label(tr("Efekt"));
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
                    if ui.small_button(t).on_hover_text(trf!("{tip} — seçimi sarar, seçim yoksa imleçte açar/kapatır")).clicked() {
                        nd.text = osisub_core::overrides::toggle_bool_tag(&nd.text, a, b, name, on);
                        tag_used = true;
                    }
                }
                ui.separator();
                for (name, label) in [("1c", "Renk"), ("2c", "İkincil"), ("3c", "Kenar"), ("4c", "Gölge")] {
                    let cur = osisub_core::overrides::tag_value_before(&nd.text, a, name).or_else(|| if name == "1c" { osisub_core::overrides::tag_value_before(&nd.text, a, "c") } else { None })
                        .and_then(|v| osisub_core::color::Color::parse(&v))
                        .or_else(|| osisub_core::color::Color::parse(match name { "1c" => &st.color1, "2c" => &st.color2, "3c" => &st.color3, _ => &st.color4 }))
                        .unwrap_or(osisub_core::color::Color::WHITE);
                    let mut rgb = [cur.r, cur.g, cur.b];
                    ui.label(RichText::new(tr(label)).small());
                    if crate::color_ui::button(ui, name, &mut rgb) {
                        let c = osisub_core::color::Color { r: rgb[0], g: rgb[1], b: rgb[2], a: 0 };
                        let t = osisub_core::overrides::set_tag_range(&nd.text, a, b, if name == "1c" && nd.text.contains("\\c&") { "c" } else { name }, &c.to_tag_string());
                        nd.text = t;
                        tag_used = true;
                    }
                }
                ui.separator();
                if ui.small_button(tr("\\N")).on_hover_text(tr("Satır sonu ekle")).clicked() {
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
            TextEdit::multiline(&mut nd.text).desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).layouter(&mut layouter).hint_text(tr("Altyazı metni — satır sonu için \\N")).show(ui)
        }).inner;
        // Enter ile girilen ham satır sonu dosyada Dialogue satırını böler: ASS satır sonuna (\N) çevir
        fix_newlines(ui.ctx(), out.response.id, &mut nd.text, true);
        if let Some((_, a, b)) = self.pending_text_sel.take().filter(|p| p.0 == at) {
            if let Some(mut st) = egui::text_edit::TextEditState::load(ui.ctx(), out.response.id) {
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(a), egui::text::CCursor::new(b))));
                st.store(ui.ctx(), out.response.id);
            } else {
                let mut st = egui::text_edit::TextEditState::default();
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(a), egui::text::CCursor::new(b))));
                st.store(ui.ctx(), out.response.id);
            }
            self.editor_cursor = Some((at, b));
            self.editor_sel = Some((at, a.min(b), a.max(b)));
            out.response.request_focus();
            ui.ctx().request_repaint();
        }
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
        let mut edit_cmd: Option<&'static str> = None;
        if out.response.secondary_clicked() {
            // Aegisub gibi: sağ tıklanan sözcük için yazım önerileri ve eş anlamlılar
            self.ctx_word = out.response.interact_pointer_pos().and_then(|p| {
                let ci = out.galley.cursor_from_pos(p - out.galley_pos).index;
                osisub_spell::thesaurus::word_at(&nd.text, ci.into()).map(|(a, b)| (at, a, b, nd.text.chars().skip(a).take(b - a).collect()))
            });
        }
        let mut word_repl: Option<String> = None;
        let mut add_word: Option<String> = None;
        let ctx_word = self.ctx_word.clone().filter(|w| w.0 == at);
        out.response.context_menu(|ui| {
            if let Some((_, _, _, w)) = &ctx_word {
                if let Some(sp) = self.spell.as_deref() {
                    if osisub_spell::should_flag(sp, w) && !self.spell_ignore.contains(&osisub_core::qc::tr_lower(w)) {
                        ui.label(RichText::new(trf!("Yazım: \"{}\"", w)).weak());
                        let sugs = sp.suggest(w, 8);
                        if sugs.is_empty() { ui.label(RichText::new(tr("öneri yok")).weak()); }
                        for s in sugs {
                            if ui.button(&s).clicked() { word_repl = Some(s); ui.close(); }
                        }
                        if ui.button(tr("Sözlüğe ekle")).clicked() { add_word = Some(w.clone()); ui.close(); }
                        ui.separator();
                    }
                }
                let meanings: Vec<_> = self.thesauri.get().map(|v| v.iter().flat_map(|t| t.lookup(w)).collect()).unwrap_or_default();
                if !meanings.is_empty() {
                    ui.menu_button(trf!("Eş anlamlılar: {}", w), |ui| {
                        egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                            for m in meanings.iter().take(30) {
                                ui.menu_button(format!("{} {}", m.words[0], m.pos), |ui| {
                                    for s in &m.words {
                                        if ui.button(s).clicked() { word_repl = Some(s.clone()); ui.close(); }
                                    }
                                });
                            }
                        });
                    });
                    ui.separator();
                }
            }
            if ui.button(tr("İmleçte böl (süre metne orantılı)")).clicked() { split_req = Some(false); ui.close(); }
            if ui.add_enabled(self.media.framerate().is_some(), egui::Button::new(tr("İmleçte böl (video karesinde)"))).clicked() { split_req = Some(true); ui.close(); }
            if ui.button(tr("İmleçte böl (zamanlar korunur)")).clicked() { edit_cmd = Some("split_preserve"); ui.close(); }
            ui.separator();
            if ui.button(tr("Metni temizle (etiketler kalır)")).on_hover_text(tr("Düz metni siler; etiketler, notlar ve çizimler kalır (Aegisub: Clear Text)")).clicked() { edit_cmd = Some("clear_text"); ui.close(); }
            if ui.button(tr("Orijinali ekle")).on_hover_text(tr("Satırın seçildiği andaki metnini imlece ekler (Aegisub: Insert Original)")).clicked() { edit_cmd = Some("insert_original"); ui.close(); }
            if ui.button(tr("Satırı geri al")).on_hover_text(tr("Satır metnini seçildiği andaki haline döndürür (Aegisub: Revert)")).clicked() { edit_cmd = Some("revert_line"); ui.close(); }
        });
        if let Some(w) = add_word {
            self.add_user_word(&w);
        }
        if let (Some(r), Some((_, a, b, w))) = (word_repl, ctx_word) {
            // sözcük hâlâ yerindeyse değiştir (menü açıkken metin değişmiş olabilir)
            let chars: Vec<char> = nd.text.chars().collect();
            if b <= chars.len() && chars[a..b].iter().collect::<String>() == w {
                let r = osisub_spell::thesaurus::match_case(&w, &r);
                nd.text = chars[..a].iter().collect::<String>() + &r + &chars[b..].iter().collect::<String>();
                tag_used = true;
            }
            self.ctx_word = None;
        }
        let cps = qc::cps(&nd);
        let vis = osisub_core::tags::visible_text(&nd.text);
        let longest = osisub_core::tags::max_line_len(&nd.text);
        ui.horizontal(|ui| {
            ui.label(RichText::new(trf!("{} karakter", osisub_core::tags::visible_len(&nd.text))).weak());
            ui.label(RichText::new(trf!("{} satır", vis.lines().count().max(1))).weak());
            ui.label(RichText::new(trf!("en uzun satır {}", longest)).color(if longest > self.rules.max_line_len { theme::BAD } else { Color32::GRAY }));
            ui.label(RichText::new(trf!("{} kar/sn", if cps.is_finite() { format!("{:.1}", cps) } else { "∞".into() })).color(if cps > self.rules.max_cps { theme::BAD } else { Color32::GRAY }));
        });
        if let Some(r) = &self.reference {
            let (a, b) = (d.start_time, d.end_time);
            let mut best: Vec<(i64, String)> = r.dialogues().filter(|x| !x.comment && x.end_time > a && x.start_time < b).map(|x| ((x.end_time.min(b) - x.start_time.max(a)), osisub_core::tags::visible_text(&x.text).replace('\n', " ⏎ "))).collect();
            best.sort_by_key(|x| -x.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Referans:")).color(theme::ACCENT));
                if best.is_empty() { ui.label(RichText::new(tr("(eşleşen satır yok)")).weak()); }
                for (_, t) in best.iter().take(3) { ui.label(RichText::new(t).italics()); }
            });
        }
        if let Some(at_frame) = split_req {
            if nd == d {
                self.split_at_cursor(at_frame);
                return;
            }
        }
        if let Some(c) = edit_cmd {
            if nd == d {
                let ctx = ui.ctx().clone();
                self.run_command(c, &ctx);
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

/// Metin kutusundaki ham satır sonlarını kaldırır (`keep` ise `\N` yapar) ve imleci buna göre kaydırır
/// (yoksa imleç `\` ile `N` arasında kalır ve sonraki harf araya girer).
pub(crate) fn fix_newlines(ctx: &egui::Context, id: egui::Id, text: &mut String, keep: bool) {
    if !text.contains(['\n', '\r']) {
        return;
    }
    let mut st = egui::text_edit::TextEditState::load(ctx, id);
    let cur: Option<usize> = st.as_ref().and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.into());
    let mut out = String::with_capacity(text.len() + 2);
    let (mut n, mut newpos) = (0usize, None);
    for (i, c) in text.chars().enumerate() {
        if Some(i) == cur { newpos = Some(n); }
        match c {
            '\r' => {}
            '\n' => if keep { out.push_str("\\N"); n += 2; },
            _ => { out.push(c); n += 1; }
        }
    }
    if newpos.is_none() && cur.is_some() { newpos = Some(n); }
    *text = out;
    if let (Some(s), Some(p)) = (st.as_mut(), newpos) {
        s.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(p))));
        s.clone().store(ctx, id);
    }
}
