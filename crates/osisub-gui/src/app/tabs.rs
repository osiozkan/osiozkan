use super::*;

impl App {
    /// Çevirmen görünümü: referans büyük gösterilir, Enter kaydedip sonraki satıra geçer.
    pub(super) fn translate_tab(&mut self, ui: &mut egui::Ui) {
        let at = self.doc.active;
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() else {
            ui.label(RichText::new("Bir satır seçin").weak());
            return;
        };
        let pos = self.rows.iter().position(|&r| r == at).map(|p| p + 1).unwrap_or(0);
        if !self.ref_lines.is_empty() && ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::I))) {
            self.tr_insert_original = true;
        }
        // İlerleme: metni dolu (ve referanstan farklı) diyalog satırları — sürüm başına önbellekli
        if self.tr_progress.0 != self.doc.rev || self.tr_progress.3 != self.ref_lines.len() {
            let (mut done, mut total) = (0usize, 0usize);
            for dd in self.doc.script.dialogues().filter(|x| !x.comment && !qc::is_typeset(x) && !qc::is_karaoke(x)) {
                total += 1;
                let v = osisub_core::tags::visible_text(&dd.text).replace('\n', " ⏎ ");
                if v.trim().is_empty() { continue; }
                let end = self.ref_lines.partition_point(|r| r.0 < dd.end_time);
                let same = self.ref_lines[..end].iter().rev().take(6).any(|r| r.1 > dd.start_time && r.2.trim() == v.trim());
                if !same { done += 1; }
            }
            self.tr_progress = (self.doc.rev, done, total, self.ref_lines.len());
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Satır {} / {}", pos, self.rows.len())).strong());
            ui.label(RichText::new(format!("{} → {}", time::format_ass(d.start_time), time::format_ass(d.end_time))).monospace().weak());
            let (_, done, total, _) = self.tr_progress;
            if total > 0 {
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_width(120.0).text(format!("çevrildi {done}/{total}"))).on_hover_text("Metni dolu ve referanstan farklı diyalog satırları (tabela/karaoke hariç)");
            }
        });
        // Kaynak metin: referanstan eşleşenler
        let (a, b) = (d.start_time, d.end_time);
        let refs = &self.ref_lines;
        let mut hits: Vec<&(i64, i64, String)> = refs.iter().filter(|r| r.1 > a && r.0 < b).collect();
        hits.sort_by_key(|r| -(r.1.min(b) - r.0.max(a)));
        // Bağlam: referansta bir önceki ve bir sonraki satır (soluk)
        let in_hits = |r: &(i64, i64, String)| hits.iter().any(|h| std::ptr::eq(*h, r));
        let prev_ref = refs.iter().rev().find(|r| r.0 < a && !in_hits(r));
        let next_ref = refs.iter().find(|r| r.0 >= a && !in_hits(r));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_height(64.0);
            if let Some(p) = prev_ref {
                ui.label(RichText::new(format!("↑ {}", p.2)).small().color(Color32::from_gray(130)));
            }
            if refs.is_empty() {
                ui.label(RichText::new("Referans altyazı yüklü değil (Dosya → Referans altyazı aç…)").weak());
            } else if hits.is_empty() {
                ui.label(RichText::new("(bu aralıkta referans satırı yok)").weak());
            }
            for h in hits.iter().take(3) {
                ui.label(RichText::new(&h.2).size(17.0).color(theme::ACCENT));
            }
            if let Some(n) = next_ref {
                ui.label(RichText::new(format!("↓ {}", n.2)).small().color(Color32::from_gray(130)));
            }
        });
        // Eksik terimler (kaynak referansta geçiyor, çeviride karşılığı yok)
        if !self.glossary.is_empty() && !hits.is_empty() {
            let src: String = hits.iter().map(|h| h.2.as_str()).collect::<Vec<_>>().join(" ");
            let missing = osisub_core::glossary::missing_in_line(&src, &d.text, &self.glossary);
            if !missing.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Terim:").color(theme::WARN));
                    for t in missing { ui.label(RichText::new(format!("{} → {}", t.source, t.target)).color(theme::WARN).strong()); }
                });
            }
        }
        // Çeviri belleği önerileri (satır ya da bellek değişince yeniden hesaplanır)
        let mut use_tm: Option<String> = None;
        if !self.tm.is_empty() && !hits.is_empty() {
            if self.tm_cache.0 != at || self.tm_cache.1 != self.tm_rev {
                let src = self.ref_source(a, b).unwrap_or_default();
                self.tm_cache = (at, self.tm_rev, self.tm.suggest(&src, 3, 60));
            }
            for s in &self.tm_cache.2 {
                ui.horizontal(|ui| {
                    let col = if s.score == 100 { theme::ACCENT } else { Color32::GRAY };
                    ui.label(RichText::new(format!("TM {}%", s.score)).monospace().small().color(col));
                    if ui.small_button("Kullan").on_hover_text(format!("Kaynak: {}", s.source)).clicked() { use_tm = Some(s.target.clone()); }
                    ui.label(RichText::new(&s.target).italics());
                });
            }
        }
        // Çeviri kutusu
        let mut nd = d.clone();
        if use_tm.is_none() && self.tr_insert_original {
            self.tr_insert_original = false;
            let src: Vec<String> = { let mut h = hits.clone(); h.sort_by_key(|r| r.0); h.iter().map(|r| r.2.replace(" ⏎ ", "\\N")).collect() };
            if !src.is_empty() { use_tm = Some(src.join("\\N")); }
        }
        if let Some(t) = use_tm.take() {
            let head = osisub_core::overrides::parse(&nd.text).into_iter().take_while(|s| matches!(s, osisub_core::overrides::Seg::Block(_))).collect::<Vec<_>>();
            nd.text = format!("{}{}", osisub_core::overrides::to_string(&head), t);
        }
        let sp = self.spell.clone();
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = editor_job(buf.as_str(), sp.as_deref());
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let id = egui::Id::unique("translate_box");
        let r = ui.add(TextEdit::multiline(&mut nd.text).id(id).desired_rows(4).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).layouter(&mut layouter).hint_text("Çeviri — Enter: kaydet ve sonraki satır"));
        if self.tr_last_active != at {
            self.tr_last_active = at;
            r.request_focus();
        }
        // Enter = sonraki satır (metin kutusu Enter'ı tüketmeden önce yakala)
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
        if enter {
            nd.text = nd.text.trim_end_matches('\n').to_string();
        }
        let cps = qc::cps(&nd);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} karakter", osisub_core::tags::visible_len(&nd.text))).weak());
            ui.label(RichText::new(format!("{} kar/sn", if cps.is_finite() { format!("{:.1}", cps) } else { "∞".into() })).color(if cps > self.rules.max_cps { theme::BAD } else { Color32::GRAY }));
        });
        if nd.text != d.text {
            let key = format!("text:{}", at);
            self.doc.edit_keyed("Metin düzenle", Some(&key), |tx| tx.replace(at, Entry::Dialogue(nd.clone())));
        }
        let step = |app: &mut App, delta: isize| {
            let p = app.rows.iter().position(|&r| r == at).unwrap_or(0) as isize;
            let np = (p + delta).clamp(0, app.rows.len() as isize - 1) as usize;
            if let Some(&row) = app.rows.get(np) {
                app.doc.select(vec![row], row);
                app.scroll_to = Some(row);
            }
        };
        ui.horizontal(|ui| {
            if ui.button("◀ Önceki").clicked() { step(self, -1); }
            if ui.button("Sonraki ▶").clicked() { step(self, 1); }
            if ui.add_enabled(self.mt_rx.is_none(), egui::Button::new("Makine çevirisi")).on_hover_text("Bu satırı (referans varsa referans metnini) seçili sağlayıcıyla çevirir; ayarlar: Araçlar → Makine çevirisi").clicked() {
                let a = self.doc.active;
                self.doc.select(vec![a], a);
                self.start_mt();
            }
            if ui.add_enabled(!self.ref_lines.is_empty(), egui::Button::new("Orijinali ekle")).on_hover_text("Referans metnini çeviri kutusuna kopyalar (baştaki etiketler korunur; Ctrl+I)").clicked() {
                self.tr_insert_original = true;
            }
            if ui.button("Sonraki boş satır").on_hover_text("Metni boş olan sonraki satıra atla (ör. sesten oluşturulanlar)").clicked() {
                let p = self.rows.iter().position(|&r| r == at).unwrap_or(0);
                let next = self.rows.iter().enumerate().skip(p + 1).find(|(_, &r)| matches!(&self.doc.script.entries[r], Entry::Dialogue(x) if osisub_core::tags::visible_text(&x.text).trim().is_empty())).map(|(_, &r)| r);
                match next {
                    Some(row) => { self.doc.select(vec![row], row); self.scroll_to = Some(row); }
                    None => self.status = "Boş satır kalmadı".into(),
                }
            }
        });
        if enter {
            // Çeviriyi belleğe öğret (kaynak = referanstaki eşleşen satır)
            if let Some(src) = self.ref_source(a, b) {
                let tgt = osisub_core::tags::visible_text(&nd.text).replace('\n', "\\N");
                if self.tm.add(&src, &tgt) { self.tm_rev += 1; self.tm_dirty = true; }
            }
            step(self, 1);
        }
        if !self.glossary.is_empty() && !self.ref_lines.is_empty() {
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Tüm terimleri denetle").clicked() {
                    self.gloss_findings = osisub_core::glossary::check(&self.doc.script, &self.ref_lines, &self.glossary);
                    self.status = format!("{} eksik terim", self.gloss_findings.len());
                }
                ui.label(RichText::new(format!("{} terim, {} bulgu", self.glossary.len(), self.gloss_findings.len())).weak());
            });
            let mut jump = None;
            egui::ScrollArea::vertical().id_salt("gloss").max_height(160.0).show(ui, |ui| {
                for f in &self.gloss_findings {
                    if ui.selectable_label(false, format!("{} → {}  (satır {})", f.term.source, f.term.target, self.rows.iter().position(|&r| r == f.entry).map(|p| p + 1).unwrap_or(0))).clicked() { jump = Some(f.entry); }
                }
            });
            if let Some(e) = jump { self.doc.select(vec![e], e); self.scroll_to = Some(e); }
        }
    }

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
        let second = (PathBuf::from(&self.settings.dict2_aff), PathBuf::from(&self.settings.dict2_dic));
        std::thread::spawn(move || {
            let r = osisub_spell::Speller::load(&aff, &dic).map(|mut s| {
                if second.0.is_file() && second.1.is_file() {
                    let _ = s.add_dictionary(&second.0, &second.1);
                }
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
        // ikinci sözlük: İngilizce şarkı sözleri/terimler için (sözcük herhangi bir sözlükte varsa doğru)
        ui.horizontal_wrapped(|ui| {
            let has2 = !self.settings.dict2_dic.is_empty();
            ui.label(RichText::new(if has2 { format!("Ek sözlük: {}", PathBuf::from(&self.settings.dict2_dic).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()) } else { "Ek sözlük yok".into() }).weak());
            let (aff, dic) = (PathBuf::from(&self.settings.dict_aff), PathBuf::from(&self.settings.dict_dic));
            if ui.button("Ek sözlük seç…").on_hover_text("İkinci dil (ör. en_US): sözcük iki sözlükten birinde varsa hata sayılmaz").clicked() {
                if let Some(d2) = rfd::FileDialog::new().add_filter("Hunspell sözlüğü", &["dic"]).pick_file() {
                    if d2.with_extension("aff").is_file() {
                        self.settings.dict2_aff = d2.with_extension("aff").to_string_lossy().to_string();
                        self.settings.dict2_dic = d2.to_string_lossy().to_string();
                        if aff.is_file() { self.load_dictionary(aff.clone(), dic.clone()); }
                    } else { self.spell_status = "Aynı klasörde .aff dosyası yok".into(); }
                }
            }
            if ui.button("en_US indir").on_hover_text("LibreOffice İngilizce sözlüğünü ek sözlük olarak indirir (curl gerekir)").clicked() {
                if let Some(dir) = Self::user_dict_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    let ok = ["en_US.aff", "en_US.dic"].iter().all(|f| std::process::Command::new("curl").args(["-sSfL", "-o"]).arg(dir.join(f)).arg(format!("https://raw.githubusercontent.com/LibreOffice/dictionaries/master/en/{}", f)).status().map(|s| s.success()).unwrap_or(false));
                    if ok {
                        self.settings.dict2_aff = dir.join("en_US.aff").to_string_lossy().to_string();
                        self.settings.dict2_dic = dir.join("en_US.dic").to_string_lossy().to_string();
                        if aff.is_file() { self.load_dictionary(aff.clone(), dic.clone()); }
                    } else { self.spell_status = "İndirme başarısız (curl/ağ)".into(); }
                }
            }
            if has2 && ui.button("Ek sözlüğü kaldır").clicked() {
                self.settings.dict2_aff.clear();
                self.settings.dict2_dic.clear();
                if aff.is_file() { self.load_dictionary(aff, dic); }
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
            for (t, l) in [(Tab::Translate, "Çeviri"), (Tab::Styles, "Stiller"), (Tab::Karaoke, "Karaoke"), (Tab::Tags, "Etiketler"), (Tab::Spell, "Yazım"), (Tab::Quality, "Kalite"), (Tab::Automation, "Otomasyon"), (Tab::Info, "Bilgi")] {
                ui.selectable_value(&mut self.tab, t, l);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
            Tab::Translate => self.translate_tab(ui),
            Tab::Styles => self.styles_tab(ui),
            Tab::Karaoke => self.karaoke_tab(ui),
            Tab::Tags => self.tags_tab(ui),
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
            if ui.button("Kullanılmayanları sil").on_hover_text("Hiçbir satırın (stil ya da \\r ile) kullanmadığı stiller; Default korunur").clicked() {
                let mut sc = self.doc.script.clone();
                let removed = osisub_core::merge::remove_unused_styles(&mut sc);
                if !removed.is_empty() { self.doc.apply_new_script("Kullanılmayan stilleri sil", sc); self.style_sel = 0; }
                self.status = if removed.is_empty() { "Kullanılmayan stil yok".into() } else { format!("Silindi: {}", removed.join(", ")) };
            }
            if ui.button("Sil").clicked() && idxs.len() > 1 {
                if let Some(&i) = idxs.get(self.style_sel) { self.doc.edit("Stil sil", |tx| tx.remove(i, 1)); }
            }
        });
        ui.separator();
        if let Some(&i) = idxs.get(self.style_sel) {
            if let Entry::Style(s) = self.doc.script.entries[i].clone() {
                let mut ns = s.clone();
                self.style_preview_ui(ui, &ns);
                let fonts = self.font_families();
                if fonts.is_none() { ui.ctx().request_repaint_after(std::time::Duration::from_millis(300)); }
                if style_ui::edit(ui, &mut ns, fonts.as_deref().map(|v| v.as_slice())) && ns != s {
                    // Ad değişirse diyalogları da güncelle.
                    let rename = (ns.name != s.name).then(|| (s.name.clone(), ns.name.clone()));
                    let key = format!("style:{}", i);
                    self.doc.edit_keyed("Stil düzenle", Some(&key), |tx| {
                        tx.replace(i, Entry::Style(ns));
                        if let Some((old, new)) = rename {
                            let affected: Vec<usize> = tx.script().entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(d) if d.style == old)).map(|(k, _)| k).collect();
                            for k in affected { tx.dialogue_mut(k, |d| d.style = new.clone()); }
                            // \r<eski> başvuruları da
                            let map = [(old.clone(), new.clone())];
                            let refs: Vec<usize> = tx.script().entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(d) if d.text.contains("\\r"))).map(|(k, _)| k).collect();
                            for k in refs { tx.dialogue_mut(k, |d| d.text = osisub_core::merge::rename_r_refs(&d.text, &map)); }
                        }
                    });
                }
            }
        }
    }

    /// libass ile çizilen stil önizlemesi (arka planda; stil/metin değişince yenilenir).
    pub(super) fn style_preview_ui(&mut self, ui: &mut egui::Ui, st: &osisub_core::ass::Style) {
        use std::hash::{Hash, Hasher};
        let (w, h) = (((ui.available_width() - 24.0).min(420.0) as u32).max(160), 90u32);
        // Büyük yazı tipleri sığsın: PlayRes'i büyüterek tekdüze küçült (kenar/gölge oranı korunur)
        let k = (st.fontsize as f64 / (h as f64 * 0.45)).max(1.0);
        let (rx, ry) = ((w as f64 * k).round() as u32, (h as f64 * k).round() as u32);
        let line = osisub_core::ass::style_line(st);
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        (line.as_str(), self.style_pv_text.as_str(), w).hash(&mut hs);
        let key = hs.finish();
        if let Some(rx) = &self.style_pv_rx {
            if let Ok((k, res)) = rx.try_recv() {
                self.style_pv_rx = None;
                match res {
                    Ok(f) => {
                        let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba);
                        self.style_pv = Some((k, ui.ctx().load_texture("style_pv", img, egui::TextureOptions::LINEAR)));
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if self.style_pv.as_ref().map(|x| x.0) != Some(key) && self.style_pv_rx.is_none() && osisub_media::ffmpeg_available() {
            let text = self.style_pv_text.replace(['{', '}'], "");
            let ass = format!("[Script Info]\nScriptType: v4.00+\nPlayResX: {rx}\nPlayResY: {ry}\nWrapStyle: 2\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n{line}\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:10.00,{},,0,0,0,,{{\\an5\\pos({},{})}}{}\n", st.name, rx / 2, ry / 2, text);
            let tmp = std::env::temp_dir().join(format!("osisub_stylepv_{}", std::process::id()));
            let (tx, rx) = std::sync::mpsc::channel();
            self.style_pv_rx = Some(rx);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = tx.send((key, osisub_media::render_ass_preview(&ass, w, h, "0x3a3a46", &tmp)));
                ctx.request_repaint();
            });
        }
        egui::CollapsingHeader::new("Önizleme").id_salt("style_pv").default_open(true).show(ui, |ui| {
            if let Some((_, t)) = &self.style_pv {
                ui.image((t.id(), t.size_vec2()));
            } else {
                ui.label(RichText::new(if osisub_media::ffmpeg_available() { "çiziliyor…" } else { "önizleme için ffmpeg gerekli" }).weak());
            }
            ui.add(TextEdit::singleline(&mut self.style_pv_text).desired_width(f32::INFINITY).hint_text("önizleme metni"));
        });
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

    /// Etkin satırın etiket denetçisi: her blok ve etiket ayrı satırda düzenlenebilir; `\t`
    /// dönüşümlerinin zamanları/ivmesi ve iç etiketleri ayrı alanlarda.
    pub(super) fn tags_tab(&mut self, ui: &mut egui::Ui) {
        use osisub_core::overrides::{self as ov, Seg, Tag};
        let at = self.doc.active;
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() else {
            ui.label(RichText::new("Bir satır seçin").weak());
            return;
        };
        let mut segs = ov::parse(&d.text);
        let mut changed = false;
        let mut remove: Option<(usize, usize)> = None;
        let mut add_block_at_start = false;
        let blocks = segs.iter().filter(|g| matches!(g, Seg::Block(_))).count();
        if blocks == 0 {
            ui.label(RichText::new("Satırda etiket bloğu yok.").weak());
        }
        let mut before = String::new();
        let n_segs = segs.len();
        for si in 0..n_segs {
            let ctx_text = before.clone();
            let Seg::Block(tags) = &mut segs[si] else {
                if let Seg::Text(t) = &segs[si] { before.push_str(t); }
                continue;
            };
            let title = if ctx_text.is_empty() { "Başlangıç bloğu".to_string() } else {
                let tail: String = ctx_text.chars().rev().take(14).collect::<Vec<_>>().into_iter().rev().collect();
                format!("Blok: …{tail}| sonrası")
            };
            egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(("tagblk", at, si)).default_open(true).show(ui, |ui| {
                egui::Grid::new(("tagg", at, si)).num_columns(3).striped(true).show(ui, |ui| {
                    for (ti, t) in tags.iter_mut().enumerate() {
                        if t.name.is_empty() {
                            ui.label(RichText::new("not").weak());
                            if ui.add(egui::TextEdit::singleline(&mut t.arg).desired_width(220.0)).changed() { changed = true; }
                        } else if t.name == "t" {
                            ui.label(RichText::new("\\t").monospace().color(theme::ACCENT));
                            let a = t.args();
                            let nums: Vec<f64> = a[..a.len().saturating_sub(1)].iter().filter_map(|x| x.trim().parse().ok()).collect();
                            let (mut t1, mut t2, mut acc) = match nums.as_slice() {
                                [x, y] => (*x as i64, *y as i64, 1.0),
                                [x, y, z] => (*x as i64, *y as i64, *z),
                                [z] => (0, 0, *z),
                                _ => (0, 0, 1.0),
                            };
                            let mut inner = a.last().cloned().unwrap_or_default();
                            let mut c = false;
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    c |= ui.add(egui::DragValue::new(&mut t1).speed(10).prefix("t1 ").suffix(" ms")).changed();
                                    c |= ui.add(egui::DragValue::new(&mut t2).speed(10).prefix("t2 ").suffix(" ms")).changed();
                                    c |= ui.add(egui::DragValue::new(&mut acc).speed(0.05).range(0.01..=20.0).prefix("ivme ")).changed();
                                });
                                c |= ui.add(egui::TextEdit::singleline(&mut inner).font(egui::TextStyle::Monospace).desired_width(220.0)).changed();
                            });
                            if c {
                                let acc_s = if acc == 1.0 { String::new() } else { format!("{},", (acc * 1000.0).round() / 1000.0) };
                                t.arg = if t1 == 0 && t2 == 0 { format!("({acc_s}{inner})") } else { format!("({t1},{t2},{acc_s}{inner})") };
                                changed = true;
                            }
                        } else {
                            ui.label(RichText::new(format!("\\{}", t.name)).monospace().color(theme::ACCENT));
                            let mut v = t.arg.clone();
                            let r = if let Some(x) = t.number().filter(|_| !matches!(t.name.as_str(), "an" | "b" | "i" | "u" | "s" | "q" | "p" | "k" | "kf" | "ko" | "K")) {
                                let mut x = x;
                                let speed = if matches!(t.name.as_str(), "fax" | "fay") { 0.01 } else { 0.5 };
                                let r = ui.add(egui::DragValue::new(&mut x).speed(speed).max_decimals(3));
                                if r.changed() { v = format!("{}", (x * 1000.0).round() / 1000.0); }
                                r
                            } else if let Some(c) = matches!(t.name.as_str(), "c" | "1c" | "2c" | "3c" | "4c").then(|| osisub_core::color::Color::parse(&t.arg)).flatten() {
                                ui.horizontal(|ui| {
                                    let mut rgb = [c.r, c.g, c.b];
                                    let picked = crate::color_ui::button(ui, ("tagcol", t.name.as_str()), &mut rgb);
                                    let mut rt = ui.add(egui::TextEdit::singleline(&mut v).font(egui::TextStyle::Monospace).desired_width(170.0));
                                    if picked { v = osisub_core::color::Color { r: rgb[0], g: rgb[1], b: rgb[2], a: 0 }.to_tag_string(); rt.mark_changed(); }
                                    rt
                                }).inner
                            } else {
                                ui.add(egui::TextEdit::singleline(&mut v).font(egui::TextStyle::Monospace).desired_width(220.0))
                            };
                            if r.changed() && v != t.arg { t.arg = v; changed = true; }
                        }
                        if ui.small_button("x").on_hover_text("Etiketi sil").clicked() { remove = Some((si, ti)); }
                        ui.end_row();
                    }
                });
                let key = egui::Id::unique(("tagadd", at, si));
                let mut add: String = ui.data_mut(|m| m.get_temp(key).unwrap_or_default());
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut add).font(egui::TextStyle::Monospace).desired_width(160.0).hint_text("\\bord2\\blur1"));
                    if (ui.small_button("Ekle").clicked() || r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) && !add.trim().is_empty() {
                        let body = add.trim().trim_start_matches('{').trim_end_matches('}').to_string();
                        for nt in ov::parse_block(&body) { tags.push(nt); }
                        add.clear();
                        changed = true;
                    }
                });
                ui.data_mut(|m| m.insert_temp(key, add));
            });
        }
        if !matches!(segs.first(), Some(Seg::Block(_))) && ui.button("Başlangıç bloğu ekle").clicked() {
            add_block_at_start = true;
        }
        if let Some((si, ti)) = remove {
            if let Seg::Block(t) = &mut segs[si] { t.remove(ti); }
            segs.retain(|g| !matches!(g, Seg::Block(t) if t.is_empty()));
            changed = true;
        }
        if add_block_at_start {
            segs.insert(0, Seg::Block(vec![Tag::new("bord", "2")]));
            changed = true;
        }
        if changed {
            let nt = ov::to_string(&segs);
            if nt != d.text {
                let key = format!("tagtab:{at}");
                self.doc.edit_keyed("Etiket düzenle", Some(&key), |tx| tx.dialogue_mut(at, |dd| dd.text = nt));
            }
        }
        ui.separator();
        ui.label(RichText::new("Sayıları sürükleyerek değiştirin (Aegisub'ın görsel araçlarına ek olarak). Aynı satırdaki ardışık değişiklikler tek geri-al adımıdır.").small().weak());
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
            if ui.button("Japonca hecele (romaji/kana) ve süreyi eşit dağıt").on_hover_text("Mora kuralları: ki|mi |no, ki|t|to, shi|n|ji; きょ|う; küçük kana ve ー önceki heceye yapışır").clicked() {
                let plain = osisub_core::tags::strip_tags(&d.text);
                new_syls = Some(k::equal_syllables(&k::auto_syllables(&plain), line_cs, "kf"));
            }
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
        // Betik başlığı denetimleri (PlayRes, ScaledBorderAndShadow, YCbCr Matrix…)
        let video = self.media.info.as_ref().filter(|i| i.has_video).map(|i| (i.width, i.height));
        let dar = self.media.render_info().filter(|i| i.has_video && i.dar > 0.0).map(|i| i.dar);
        let head = qc::script_issues_dar(&self.doc.script, video, dar);
        if !head.is_empty() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                for m in &head {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(m).color(theme::WARN));
                        if m.starts_with("ScaledBorderAndShadow") && ui.small_button("Düzelt").clicked() {
                            let mut s = self.doc.script.clone();
                            s.set_info("ScaledBorderAndShadow", "yes");
                            self.doc.apply_new_script("ScaledBorderAndShadow: yes", s);
                        }
                        if m.starts_with("YCbCr Matrix") && ui.small_button("Düzelt").clicked() {
                            let mut s = self.doc.script.clone();
                            s.set_info("YCbCr Matrix", if video.map_or(false, |v| v.1 >= 720) { "TV.709" } else { "TV.601" });
                            self.doc.apply_new_script("YCbCr Matrix", s);
                        }
                    });
                }
            });
        }
        if ui.button("Raporu kaydet (HTML)…").on_hover_text("Başlık ve satır sorunlarını ekip QC'si için tek dosyalık HTML rapor olarak kaydeder").clicked() {
            let title = self.doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "adsız".into());
            let def = format!("{}-qc.html", title.trim_end_matches(".ass"));
            if let Some(p) = rfd::FileDialog::new().set_file_name(&def).add_filter("HTML", &["html"]).save_file() {
                let html = qc::report_html(&self.doc.script, &title, &head, &self.issues);
                self.status = match std::fs::write(&p, html) { Ok(()) => format!("QC raporu kaydedildi: {}", p.display()), Err(e) => format!("Rapor yazılamadı: {e}") };
            }
        }
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
            if ui.button("Etiketleri onar").on_hover_text("Boş {} bloklarını siler, bitişik etiket bloklarını birleştirir, satırdan uzun \\fad'ı kısaltır (yorum ve fx satırlarına dokunmaz)").clicked() {
                apply(self, "Etiketleri onar", &|s| {
                    for e in s.entries.iter_mut() {
                        if let Entry::Dialogue(d) = e {
                            if d.comment || d.effect.eq_ignore_ascii_case("fx") { continue; }
                            if let Some(t) = osisub_core::taglint::repair(d) { d.text = t; }
                        }
                    }
                });
            }
            if self.issues.iter().any(|i| i.kind == Kind::Note && i.message.starts_with("Metinde not")) && ui.button("Notları sil").on_hover_text("Diyalog metnindeki {çevirmen notu} bloklarını siler (yayın öncesi)").clicked() {
                apply(self, "Notları sil", &|s| {
                    for e in s.entries.iter_mut() {
                        if let Entry::Dialogue(d) = e {
                            if d.comment { continue; }
                            let t = osisub_core::taglint::strip_notes(&d.text);
                            if t != d.text { d.text = t; }
                        }
                    }
                });
            }
            if let Some(fr) = self.media.framerate().filter(|_| !self.media.keyframes.is_empty()) {
                if ui.button("Sahne değişimlerine yapıştır (≤3 kare)").on_hover_text("Anahtar kareye 1-3 kare yakın başlangıç/bitişleri anahtar kareye çeker").clicked() {
                    let kf = self.media.keyframes.clone();
                    let mut s = self.doc.script.clone();
                    let n = qc::snap_to_keyframes(&mut s, &fr, &kf, 3);
                    self.doc.apply_new_script("Sahne değişimlerine yapıştır", s);
                    self.status = format!("{} satır sahne değişimine yapıştırıldı", n);
                }
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{} sorun", self.issues.len())).strong());
            for (cat, label) in [("", "Tümü"), ("zaman", "Zamanlama"), ("metin", "Metin"), ("etiket", "Etiket"), ("not", "Notlar")] {
                let n = if cat.is_empty() { self.issues.len() } else { self.issues.iter().filter(|i| i.kind.category() == cat).count() };
                if (n > 0 || cat.is_empty()) && ui.selectable_label(self.qc_filter == cat, format!("{label} {n}")).clicked() { self.qc_filter = cat; }
            }
        });
        let filter = self.qc_filter;
        let issues: Vec<Issue> = self.issues.iter().filter(|i| filter.is_empty() || i.kind.category() == filter).cloned().collect();
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
            if let Some(p) = rfd::FileDialog::new().add_filter("Lua / MoonScript", &["lua", "moon"]).pick_file() { self.load_lua(p); }
        }
        if ui.button("Betikleri yeniden yükle").clicked() { self.reload_scripts(); }
        let aegi = osisub_core::paths::aegisub_automation_dirs();
        if !aegi.is_empty() {
            let dirs = aegi.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ");
            if ui.checkbox(&mut self.settings.aegisub_autoload, "Aegisub'ın autoload betiklerini de yükle")
                .on_hover_text(format!("Bulunan Aegisub klasörü: {dirs}\ninclude/ klasörü (DependencyControl modülleri dahil) her zaman aranır. Değişiklik bir sonraki açılışta geçerli olur."))
                .changed() { self.settings.save(); }
        }
        egui::CollapsingHeader::new(format!("Yüklü betik dosyaları ({})", self.loaded_scripts.len())).id_salt("script_files").show(ui, |ui| {
            let mut remove: Option<PathBuf> = None;
            for p in self.loaded_scripts.clone() {
                let ps = p.to_string_lossy().to_string();
                ui.horizontal(|ui| {
                    if self.settings.scripts.contains(&ps) {
                        if ui.small_button("Kaldır").on_hover_text("Listeden çıkar (dosya silinmez)").clicked() { remove = Some(p.clone()); }
                    } else if ps.starts_with(osisub_lua::BUILTIN_PREFIX) {
                        ui.label(RichText::new("yerleşik").small().weak());
                    } else {
                        ui.label(RichText::new("otomatik").small().weak()).on_hover_text("Otomatik yükleme klasöründen ya da komut satırından");
                    }
                    if !ps.starts_with(osisub_lua::BUILTIN_PREFIX) {
                        let mut bound = self.doc_scripts.contains(&p);
                        if ui.checkbox(&mut bound, "belgeye bağlı").on_hover_text("Aegisub gibi: kaydederken betik bu altyazı dosyasına yazılır, dosya açılınca otomatik yüklenir").changed() {
                            if bound { self.doc_scripts.push(p.clone()); } else { self.doc_scripts.retain(|x| x != &p); }
                            self.doc.mark_dirty();
                        }
                    }
                    ui.label(RichText::new(&ps).monospace().small());
                });
            }
            if let Some(p) = remove { self.remove_script(&p); }
        });
        if let Some(r) = self.running.clone() { ui.horizontal(|ui| { ui.spinner(); ui.label(format!("Çalışıyor: {}", r)); if ui.small_button("Durdur").on_hover_text("Betik motorunu sıfırlar (takılan betikler için)").clicked() { self.reload_scripts(); } if self.progress > 0.0 { ui.add(egui::ProgressBar::new(self.progress).desired_width(100.0)); } }); }
        let list: Vec<(usize, String, String, String, MacroKind)> = self.macros.iter().map(|m| (m.info.id, m.script.clone(), m.info.name.clone(), m.info.description.clone(), m.info.kind.clone())).collect();
        for (id, script, name, desc, kind) in list {
            ui.horizontal(|ui| {
                let enabled = self.running.is_none() && kind == MacroKind::Macro;
                if ui.add_enabled(enabled, egui::Button::new("Çalıştır")).clicked() { self.run_macro(id); }
                if kind == MacroKind::Macro {
                    let cur = self.settings.macro_hotkeys.iter().find(|h| h.script == script && h.name == name).map(|h| h.keys.clone());
                    let capturing = self.hotkey_capture.as_ref().map_or(false, |(s, n)| *s == script && *n == name);
                    let label = if capturing { "tuşa basın…".to_string() } else { cur.clone().unwrap_or_else(|| "kısayol yok".into()) };
                    if ui.add(egui::Button::new(RichText::new(label).monospace().small()).selected(capturing)).on_hover_text("Tıklayın, sonra tuş birleşimine basın (Esc = iptal, Backspace = kaldır)").clicked() {
                        self.hotkey_capture = Some((script.clone(), name.clone()));
                    }
                }
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
