use super::*;

impl App {
    pub(super) fn user_dict_dir() -> Option<PathBuf> {
        osisub_core::paths::config_dir().map(|d| d.join("dict"))
    }

    pub(super) fn user_words_path() -> Option<PathBuf> {
        Self::user_dict_dir().map(|d| d.join("kullanici_sozcukleri.txt"))
    }

    pub(super) fn auto_load_dictionary(&mut self) {
        let (aff, dic) = (PathBuf::from(&self.settings.dict_aff), PathBuf::from(&self.settings.dict_dic));
        if aff.is_file() && dic.is_file() {
            self.load_dictionary(aff, dic);
        } else if let Some((a, d)) = osisub_spell::find_dictionary("tr_TR", &Self::user_dict_dir().into_iter().collect::<Vec<_>>()) {
            self.load_dictionary(a, d);
        }
    }

    pub(super) fn load_dictionary(&mut self, aff: PathBuf, dic: PathBuf) {
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

    pub(super) fn pump_spell(&mut self, ctx: &egui::Context) {
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

    pub(super) fn start_spellcheck(&mut self) {
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

    pub(super) fn spell_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn side(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn styles_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn start_tap(&mut self, ctx: &egui::Context) {
        if !self.media.is_open() { self.status = "Önce ses/video açın".into(); return; }
        if !matches!(self.doc.script.entries.get(self.doc.active), Some(Entry::Dialogue(_))) { return; }
        if self.media.is_playing() { self.media.toggle_play(&self.doc, ctx); }
        self.tap = Some(TapState { line: self.doc.active, taps: vec![] });
        self.media.toggle_play(&self.doc, ctx);
        self.status = "Tuşla zamanlama: her hecenin başlangıcında Boşluk'a basın".into();
    }

    pub(super) fn record_tap(&mut self) {
        let Some(t) = self.tap.as_mut() else { return };
        if !self.media.is_playing() { return; }
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(t.line) else { return };
        let ms = (self.media.playhead - d.start_time - self.tap_latency_ms).max(0);
        t.taps.push(ms);
        let n = osisub_core::karaoke::parse(&d.text).len();
        if t.taps.len() >= n { self.finish_tap(); }
    }

    pub(super) fn finish_tap(&mut self) {
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

    pub(super) fn karaoke_tab(&mut self, ui: &mut egui::Ui) {
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
            if ui.button("Karakterlere böl ve süreyi dağıt").on_hover_text("Japonca/Çince şarkı sözleri için").clicked() {
                let plain = osisub_core::tags::strip_tags(&d.text);
                let mut v: Vec<k::Syl> = plain.chars().filter(|c| !c.is_control()).map(|c| k::Syl { dur: 0, kind: "kf".into(), text: c.to_string() }).collect();
                k::distribute(&mut v, line_cs);
                new_syls = Some(v);
            }
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

    pub(super) fn quality_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn automation_tab(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn info_tab(&mut self, ui: &mut egui::Ui) {
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
}
