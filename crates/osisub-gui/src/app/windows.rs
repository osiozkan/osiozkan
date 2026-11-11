use super::*;

impl App {
    pub(super) fn catalog_path() -> Option<PathBuf> {
        osisub_core::paths::config_dir().map(|d| d.join("styles.ass"))
    }

    pub(super) fn catalog_styles() -> Vec<Style> {
        Self::catalog_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| osisub_core::Script::parse(&t).styles().cloned().collect()).unwrap_or_default()
    }

    pub(super) fn save_catalog(styles: &[Style]) {
        if let Some(p) = Self::catalog_path() {
            if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
            let mut s = osisub_core::Script::new_default();
            s.entries.retain(|e| !matches!(e, Entry::Style(_)));
            s.entries.extend(styles.iter().cloned().map(Entry::Style));
            let _ = std::fs::write(p, s.to_ass_string());
        }
    }

    pub(super) fn add_styles(&mut self, list: Vec<Style>) {
        if list.is_empty() { return; }
        let at = self.doc.script.entries.iter().rposition(|e| matches!(e, Entry::Style(_))).map(|i| i + 1).unwrap_or(self.doc.script.entries.len());
        let mut items = vec![];
        let mut taken: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        for mut st in list {
            if taken.contains(&st.name) {
                let base = st.name.clone();
                st.name = (2..).map(|n| format!("{} ({})", base, n)).find(|n| !taken.contains(n)).unwrap();
            }
            taken.push(st.name.clone());
            items.push(Entry::Style(st));
        }
        let n = items.len();
        self.doc.edit("Stil ekle", |tx| tx.insert(at, items));
        self.status = format!("{} stil eklendi", n);
    }

    pub(super) fn import_window(&mut self, ctx: &egui::Context) {
        let Some(list) = self.import_styles.as_mut() else { return };
        let mut done: Option<bool> = None;
        egui::Window::new("Stilleri içe aktar").collapsible(false).show(ctx, |ui| {
            for (st, on) in list.iter_mut() { ui.checkbox(on, &st.name); }
            ui.horizontal(|ui| {
                if ui.button("İçe aktar").clicked() { done = Some(true); }
                if ui.button("İptal").clicked() { done = Some(false); }
            });
        });
        if let Some(go) = done {
            let chosen: Vec<Style> = self.import_styles.take().unwrap_or_default().into_iter().filter(|(_, on)| *on && go).map(|(s, _)| s).collect();
            self.add_styles(chosen);
        }
    }

    /// Lua hata penceresi: ileti + hatanın olduğu kaynak satırı (±3 satır).
    pub(super) fn lua_error_window(&mut self, ctx: &egui::Context) {
        let Some((name, msg)) = self.lua_error.clone() else { return };
        let mut open = true;
        let mut close = false;
        egui::Window::new(format!("Betik hatası — {}", name)).open(&mut open).collapsible(false).default_width(640.0).show(ctx, |ui| {
            let first = msg.lines().next().unwrap_or("").to_string();
            ui.label(RichText::new(&first).color(theme::WARN).strong());
            if let Some((file, line)) = osisub_lua::error_location(&msg) {
                ui.label(RichText::new(format!("{} — satır {}", file, line)).weak());
                if let Ok(src) = std::fs::read_to_string(&file) {
                    let lines: Vec<&str> = src.lines().collect();
                    let a = line.saturating_sub(4);
                    let b = (line + 3).min(lines.len());
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        for (n, l) in lines.iter().enumerate().take(b).skip(a) {
                            let t = RichText::new(format!("{:>5}  {}", n + 1, l)).monospace();
                            ui.label(if n + 1 == line { t.color(theme::WARN).strong() } else { t });
                        }
                    });
                }
            }
            egui::CollapsingHeader::new("Ayrıntı (yığın izi)").show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| { ui.label(RichText::new(&msg).monospace().small()); });
            });
            ui.horizontal(|ui| {
                if ui.button("Kopyala").clicked() { ui.ctx().copy_text(msg.clone()); }
                if ui.button("Kapat").clicked() { close = true; }
            });
        });
        if !open || close { self.lua_error = None; }
    }

    /// Aegisub "Satırları seç": alan + kip + eylem.
    pub(super) fn select_window(&mut self, ctx: &egui::Context) {
        use osisub_core::search::Field;
        use osisub_core::select::{Action, Mode};
        if !self.select_open { return; }
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Satırları seç").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            let c = &mut self.select_crit;
            let r = ui.add(TextEdit::singleline(&mut c.pattern).hint_text("desen (boş + Tam = alanı boş olanlar)").desired_width(320.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { apply = true; }
            ui.horizontal(|ui| {
                ui.label("Alan:");
                for (f, l) in [(Field::Text, "Metin"), (Field::Style, "Stil"), (Field::Actor, "Konuşan"), (Field::Effect, "Efekt")] { ui.radio_value(&mut c.field, f, l); }
            });
            ui.horizontal(|ui| {
                ui.label("Eşleşme:");
                for (m, l) in [(Mode::Contains, "İçerir"), (Mode::Exact, "Tam"), (Mode::Regex, "Düzenli ifade")] { ui.radio_value(&mut c.mode, m, l); }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut c.match_case, "Büyük/küçük harf duyarlı");
                ui.checkbox(&mut c.invert, "Eşleşmeyenleri seç");
            });
            ui.horizontal(|ui| {
                ui.label("Satırlar:");
                ui.checkbox(&mut c.dialogues, "Diyalog");
                ui.checkbox(&mut c.comments, "Yorum");
            });
            ui.horizontal(|ui| {
                ui.label("Seçim:");
                for (a, l) in [(Action::Set, "Yeni seçim"), (Action::Add, "Ekle"), (Action::Subtract, "Çıkar"), (Action::Intersect, "Kesişim")] { ui.radio_value(&mut c.action, a, l); }
            });
            ui.horizontal(|ui| {
                if ui.button("Seç").clicked() { apply = true; }
                if ui.button("Kapat").clicked() { self.select_open = false; }
            });
        });
        if apply {
            match osisub_core::select::select(&self.doc.script, &self.select_crit, &self.doc.selection) {
                Ok(sel) => {
                    let n = sel.len();
                    let active = if sel.contains(&self.doc.active) { self.doc.active } else { sel.first().copied().unwrap_or(self.doc.active) };
                    self.doc.select(sel, active);
                    self.scroll_to = Some(active);
                    self.status = format!("{} satır seçildi", n);
                }
                Err(e) => self.status = format!("Desen hatası: {}", e),
            }
        }
        if !open { self.select_open = false; }
    }

    /// Ekler: ASS'e gömülü yazı tipleri/grafikler (Aegisub Attachments).
    pub(super) fn attach_window(&mut self, ctx: &egui::Context) {
        use osisub_core::attach;
        if !self.attach_open { return; }
        let mut open = true;
        let list = attach::list(&self.doc.script);
        let mut remove: Option<String> = None;
        let mut save: Option<String> = None;
        let mut add_files = false;
        let mut embed_used = false;
        egui::Window::new("Ekler").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            if list.is_empty() { ui.label(RichText::new("Bu betikte gömülü dosya yok.").weak()); }
            egui::Grid::new("attach_grid").num_columns(4).striped(true).show(ui, |ui| {
                for a in &list {
                    ui.label(RichText::new(&a.name).monospace());
                    ui.label(RichText::new(&a.section).weak());
                    ui.label(format!("{:.1} KB", a.size as f64 / 1024.0));
                    ui.horizontal(|ui| {
                        if ui.small_button("Kaydet…").clicked() { save = Some(a.name.clone()); }
                        if ui.small_button("Sil").clicked() { remove = Some(a.name.clone()); }
                    });
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Dosya ekle…").clicked() { add_files = true; }
                if ui.button("Kullanılan yazı tiplerini göm").on_hover_text("Betikte kullanılan ve sistemde bulunan yazı tipi dosyalarını [Fonts] bölümüne ekler").clicked() { embed_used = true; }
            });
            ui.label(RichText::new("Gömülü yazı tipleri libass/VSFilter tarafından okunur; dosya boyutu ~%33 büyür.").weak().small());
        });
        if let Some(n) = remove {
            let mut s = self.doc.script.clone();
            if attach::remove(&mut s, &n) { self.doc.apply_new_script("Eki sil", s); self.status = format!("Ek silindi: {}", n); }
        }
        if let Some(n) = save {
            if let (Some(data), Some(p)) = (attach::extract(&self.doc.script, &n), rfd::FileDialog::new().set_file_name(n.as_str()).save_file()) {
                match std::fs::write(&p, data) { Ok(()) => self.status = format!("Kaydedildi: {}", p.display()), Err(e) => self.status = format!("Hata: {}", e) }
            }
        }
        if add_files {
            if let Some(files) = rfd::FileDialog::new().add_filter("Yazı tipi / grafik", &["ttf", "otf", "ttc", "otc", "png", "jpg", "bmp", "gif"]).pick_files() {
                let mut s = self.doc.script.clone();
                let mut n = 0;
                for f in files {
                    if let (Ok(data), Some(name)) = (std::fs::read(&f), f.file_name().map(|x| x.to_string_lossy().to_string())) { attach::attach(&mut s, &name, &data); n += 1; }
                }
                if n > 0 { self.doc.apply_new_script("Ek ekle", s); self.status = format!("{} dosya gömüldü", n); }
            }
        }
        if embed_used {
            let mut db = osisub_lua::fonts::FontDb::default();
            let mut s = self.doc.script.clone();
            let (mut n, mut missing) = (0, vec![]);
            let mut seen = std::collections::HashSet::new();
            for u in osisub_core::fonts_used::used_fonts(&self.doc.script) {
                let files = db.files_for(&u.family);
                if files.is_empty() { missing.push(u.family.clone()); }
                for f in files {
                    if !seen.insert(f.clone()) { continue; }
                    if let (Ok(data), Some(name)) = (std::fs::read(&f), f.file_name().map(|x| x.to_string_lossy().to_string())) { attach::attach(&mut s, &name, &data); n += 1; }
                }
            }
            if n > 0 { self.doc.apply_new_script("Yazı tiplerini göm", s); }
            self.status = if missing.is_empty() { format!("{} yazı tipi dosyası gömüldü", n) } else { format!("{} dosya gömüldü; bulunamayan: {}", n, missing.join(", ")) };
        }
        if !open { self.attach_open = false; }
    }

    /// Sürüm karşılaştırma sonuçları; tıklayınca açık belgedeki satıra gider.
    pub(super) fn compare_window(&mut self, ctx: &egui::Context) {
        use osisub_core::compare::Change;
        let Some((name, old, changes)) = self.compare.take() else { return };
        let mut open = true;
        let mut jump: Option<usize> = None;
        let mut refresh = false;
        let text_of = |s: &osisub_core::Script, i: usize| -> (String, String) {
            match s.entries.get(i) {
                Some(Entry::Dialogue(d)) => (format!("{}–{}", time::format_ass(d.start_time), time::format_ass(d.end_time)), d.text.clone()),
                _ => (String::new(), String::new()),
            }
        };
        egui::Window::new(format!("Karşılaştırma — {} → açık belge", name)).open(&mut open).default_width(640.0).default_height(420.0).show(ctx, |ui| {
            let (mut a, mut r, mut m) = (0, 0, 0);
            for c in &changes { match c { Change::Added { .. } => a += 1, Change::Removed { .. } => r += 1, Change::Modified { .. } => m += 1 } }
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("+{} eklendi", a)).color(theme::ACCENT));
                ui.label(RichText::new(format!("−{} silindi", r)).color(theme::BAD));
                ui.label(RichText::new(format!("~{} değişti", m)).color(theme::WARN));
                if ui.small_button("Yenile").on_hover_text("Açık belgedeki son düzenlemelerle yeniden karşılaştır").clicked() { refresh = true; }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for c in &changes {
                    match c {
                        Change::Added { new } => {
                            let (t, x) = text_of(&self.doc.script, *new);
                            if ui.selectable_label(false, RichText::new(format!("+ {}  {}", t, x)).color(theme::ACCENT)).clicked() { jump = Some(*new); }
                        }
                        Change::Removed { old: o } => {
                            let (t, x) = text_of(&old, *o);
                            ui.label(RichText::new(format!("− {}  {}", t, x)).color(theme::BAD).strikethrough());
                        }
                        Change::Modified { old: o, new, text, timing, style, actor } => {
                            let (t0, x0) = text_of(&old, *o);
                            let (t1, x1) = text_of(&self.doc.script, *new);
                            let mut what = vec![];
                            if *text { what.push("metin"); }
                            if *timing { what.push("zaman"); }
                            if *style { what.push("stil"); }
                            if *actor { what.push("konuşan"); }
                            let r = ui.selectable_label(false, RichText::new(format!("~ {}  [{}]", t1, what.join(", "))).color(theme::WARN));
                            if *timing { ui.label(RichText::new(format!("    zaman: {} → {}", t0, t1)).monospace().small()); }
                            if *text {
                                ui.label(RichText::new(format!("    eski: {}", x0)).color(Color32::GRAY).strikethrough());
                                ui.label(RichText::new(format!("    yeni: {}", x1)));
                            }
                            if r.clicked() { jump = Some(*new); }
                        }
                    }
                }
            });
        });
        if let Some(e) = jump { self.doc.select(vec![e], e); self.scroll_to = Some(e); }
        if open {
            let ch = if refresh { osisub_core::compare::compare(&old, &self.doc.script) } else { changes };
            self.compare = Some((name, old, ch));
        }
    }

    /// Yerleşik komutlara kısayol atama (Lua makroları Otomasyon sekmesinde).
    pub(super) fn hotkeys_window(&mut self, ctx: &egui::Context) {
        if !self.hotkeys_open { return; }
        let mut open = true;
        egui::Window::new("Kısayolları düzenle").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label(RichText::new("Düğmeye tıklayıp tuş birleşimine basın (Esc = iptal, Backspace = kaldır). Lua makroları için: Otomasyon sekmesi.").weak().small());
            if ui.button("Aegisub hotkey.json içe aktar…").on_hover_text("Aegisub'daki kısayollarınızı (karşılığı olan komutlar ve Lua makroları) alır").clicked() {
                let mut dlg = rfd::FileDialog::new().add_filter("hotkey.json", &["json"]);
                if let Some(d) = osisub_core::paths::aegisub_automation_dirs().first().and_then(|d| d.parent().map(|p| p.to_path_buf())) { dlg = dlg.set_directory(d); }
                if let Some(p) = dlg.pick_file() { self.import_aegisub_hotkeys(&p); }
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("hk_grid").num_columns(2).striped(true).show(ui, |ui| {
                    for (id, label) in super::COMMANDS {
                        ui.label(*label);
                        let cur = self.settings.macro_hotkeys.iter().find(|h| h.script == super::COMMAND_SCRIPT && h.name == *id).map(|h| h.keys.clone());
                        let capturing = self.hotkey_capture.as_ref().map_or(false, |(s, n)| s == super::COMMAND_SCRIPT && n == id);
                        let text = if capturing { "tuşa basın…".to_string() } else { cur.unwrap_or_else(|| "—".into()) };
                        if ui.add(egui::Button::new(RichText::new(text).monospace()).selected(capturing)).clicked() {
                            self.hotkey_capture = Some((super::COMMAND_SCRIPT.to_string(), id.to_string()));
                        }
                        ui.end_row();
                    }
                });
            });
        });
        if !open { self.hotkeys_open = false; }
    }

    pub(super) fn import_aegisub_hotkeys(&mut self, path: &std::path::Path) {
        let text = match std::fs::read_to_string(path) { Ok(t) => t, Err(e) => { self.status = format!("{}: {e}", path.display()); return; } };
        match crate::hotkeys::import_aegisub(&text) {
            Ok((binds, skipped)) => {
                let n = binds.len();
                for (target, keys) in binds {
                    let (script, name) = match target.strip_prefix("macro:") {
                        Some(m) => (String::new(), m.to_string()),
                        None => (super::COMMAND_SCRIPT.to_string(), target),
                    };
                    self.settings.macro_hotkeys.retain(|h| h.keys != keys && !(h.script == script && h.name == name));
                    self.settings.macro_hotkeys.push(crate::settings::MacroHotkey { keys, script, name });
                }
                self.settings.save();
                self.status = format!("Aegisub kısayolları: {n} atandı, {} atlandı", skipped.len());
                if !skipped.is_empty() { self.log.push_str(&format!("Aegisub kısayolları — atlananlar:\n  {}\n", skipped.join("\n  "))); }
            }
            Err(e) => self.status = e,
        }
    }

    pub(super) fn merge_window(&mut self, ctx: &egui::Context) {
        let Some((path, mut opts, mut offset)) = self.merge_win.take() else { return };
        let mut open = true;
        let mut go = false;
        egui::Window::new("Dosya birleştir").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(path.display().to_string()).monospace().small());
            ui.horizontal(|ui| {
                ui.label("Zaman kaydırma");
                ui.add(egui::TextEdit::singleline(&mut offset).desired_width(110.0).font(egui::TextStyle::Monospace))
                    .on_hover_text("Eklenen satırlara eklenir (ör. OP'yi 0:01:30.00'a yerleştirmek için). Negatif için başına '-'");
            });
            ui.checkbox(&mut opts.sort, "Satırları zamana göre sırala");
            ui.checkbox(&mut opts.skip_comments, "Yorum satırlarını alma");
            ui.label(RichText::new("Aynı adlı farklı stiller \"Ad (2)\" olarak eklenir; farklı çözünürlük bu belgeninkine örneklenir.").small().weak());
            if ui.button("Birleştir").clicked() { go = true; }
        });
        if go {
            let (neg, t) = match offset.trim().strip_prefix('-') { Some(r) => (true, r), None => (false, offset.trim()) };
            let Some(ms) = osisub_core::time::parse_generic(t).or_else(|| t.parse::<i64>().ok()) else {
                self.status = format!("Geçersiz zaman: {offset}");
                self.merge_win = Some((path, opts, offset));
                return;
            };
            opts.offset_ms = if neg { -ms } else { ms };
            match osisub_core::io::load(&path) {
                Ok(other) => {
                    let mut s = self.doc.script.clone();
                    let r = osisub_core::merge::merge_into(&mut s, &other, &opts);
                    self.doc.apply_new_script("Dosya birleştir", s);
                    self.scroll_to = Some(self.doc.active);
                    let mut msg = format!("Birleştirildi: {} satır, {} yeni stil, {} yazı tipi", r.lines, r.styles_added, r.fonts_added);
                    if let Some((w, h)) = r.resampled_from { msg.push_str(&format!(", {w}×{h}'den örneklendi")); }
                    if !r.styles_renamed.is_empty() { msg.push_str(&format!(", yeniden adlandırılan stil: {}", r.styles_renamed.iter().map(|(a, b)| format!("{a}→{b}")).collect::<Vec<_>>().join(", "))); }
                    self.status = msg;
                }
                Err(e) => self.status = e,
            }
            return;
        }
        if open { self.merge_win = Some((path, opts, offset)); }
    }

    /// Seçimin ilk karesini izin başlangıcı yaparak hareket izleme penceresini açar.
    pub(crate) fn open_motion(&mut self) {
        let fr = self.motion_framerate(None);
        let first = self.doc.target_lines().iter().filter_map(|&i| match self.doc.script.entries.get(i) { Some(Entry::Dialogue(d)) => Some(d.start_time), _ => None }).min();
        let start_frame = first.map(|t| fr.frame_at_start(t)).unwrap_or(0);
        let text = self.motion.take().map(|m| m.text).unwrap_or_default();
        self.motion = Some(super::MotionWin { text, opts: osisub_core::motion::MotionOptions { start_frame, ..Default::default() } });
    }

    /// Video açıksa onun kare zamanları (VFR dahil), yoksa ayarlardaki kare hızı (başlangıç karesiyle tutarlı olsun diye iz verisinin fps'i kullanılmaz).
    fn motion_framerate(&self, _data_fps: Option<f64>) -> osisub_core::vfr::Framerate {
        self.media.framerate().map(|f| (*f).clone()).unwrap_or_else(|| osisub_core::vfr::Framerate::cfr(self.fps))
    }

    pub(super) fn motion_window(&mut self, ctx: &egui::Context) {
        let Some(mut m) = self.motion.take() else { return };
        let mut open = true;
        let mut apply = false;
        let parsed = osisub_core::motion::parse_ae(&m.text);
        egui::Window::new("Hareket izleme verisi").open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
            ui.label(RichText::new("Mocha: Export Tracking Data → \"After Effects Transform Data\" → panoya kopyala, aşağıdaki kutuya Ctrl+V.").small().weak());
            ui.horizontal(|ui| {
                if ui.button("Dosyadan yükle…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Keyframe verisi", &["txt", "*"]).pick_file() {
                        if let Ok(t) = std::fs::read_to_string(&p) { m.text = t; }
                    }
                }
                if ui.button("Temizle").clicked() { m.text.clear(); }
            });
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut m.text).font(egui::TextStyle::Monospace).desired_rows(8).desired_width(f32::INFINITY).hint_text("Adobe After Effects 6.0 Keyframe Data …"));
            });
            match &parsed {
                Ok(d) => { ui.label(RichText::new(format!("{} kare · kaynak {}×{} · {} fps", d.frames.len(), d.width, d.height, d.fps)).color(theme::ACCENT)); }
                Err(e) if !m.text.trim().is_empty() => { ui.label(RichText::new(e).color(theme::BAD)); }
                _ => {}
            }
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut m.opts.position, "Konum");
                ui.checkbox(&mut m.opts.scale, "Ölçek");
                ui.add_enabled(m.opts.scale, egui::Checkbox::new(&mut m.opts.scale_border, "Kenar/gölge/bulanıklık da"));
                ui.checkbox(&mut m.opts.rotation, "Döndürme");
            });
            ui.horizontal(|ui| {
                ui.label("İzin ilk karesi = video karesi");
                ui.add(egui::DragValue::new(&mut m.opts.start_frame).range(0..=10_000_000));
                if ui.small_button("seçimin ilk karesi").clicked() {
                    let fr = self.motion_framerate(parsed.as_ref().ok().map(|d| d.fps));
                    if let Some(t) = self.doc.target_lines().iter().filter_map(|&i| match self.doc.script.entries.get(i) { Some(Entry::Dialogue(d)) => Some(d.start_time), _ => None }).min() {
                        m.opts.start_frame = fr.frame_at_start(t);
                    }
                }
            });
            let n = self.doc.target_lines().len();
            if ui.add_enabled(parsed.is_ok() && n > 0, egui::Button::new(format!("{n} satıra uygula"))).clicked() { apply = true; }
        });
        if apply {
            if let Ok(data) = &parsed {
                let fr = self.motion_framerate(Some(data.fps));
                let targets: Vec<usize> = self.doc.target_lines().into_iter().filter(|&i| matches!(self.doc.script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
                let lines: Vec<osisub_core::ass::Dialogue> = targets.iter().filter_map(|&i| match &self.doc.script.entries[i] { Entry::Dialogue(d) => Some(d.clone()), _ => None }).collect();
                match osisub_core::motion::apply(&self.doc.script, &lines, data, &fr, &m.opts) {
                    Ok(new_lines) => {
                        let at = targets[0];
                        let mut s = self.doc.script.clone();
                        for &i in targets.iter().rev() { s.entries.remove(i); }
                        let n = new_lines.len();
                        s.entries.splice(at..at, new_lines.into_iter().map(Entry::Dialogue));
                        self.doc.apply_new_script("Hareket izleme", s);
                        self.doc.select((at..at + n).collect(), at);
                        self.scroll_to = Some(at);
                        self.status = format!("Hareket izleme: {} satır → {} kare satırı", lines.len(), n);
                        return; // pencere kapanır
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if open { self.motion = Some(m); }
    }

    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_open;
        egui::Window::new("Dışa aktar — filtreler").open(&mut open).collapsible(false).show(ctx, |ui| {
            let mut filters: Vec<(usize, String, String, i64)> = self.macros.iter().filter(|m| m.info.kind == MacroKind::Filter).map(|m| (m.info.id, m.info.name.clone(), m.info.description.clone(), m.info.priority)).collect();
            filters.sort_by_key(|f| -f.3);
            if filters.is_empty() { ui.label(RichText::new("Yüklü filtre yok (Lua: aegisub.register_filter)").weak()); }
            for (id, name, desc, prio) in &filters {
                let mut on = self.export_sel.contains(id);
                if ui.checkbox(&mut on, format!("{}  (öncelik {})", name, prio)).on_hover_text(desc).changed() {
                    if on { self.export_sel.push(*id); } else { self.export_sel.retain(|x| x != id); }
                }
            }
            ui.label(RichText::new("Seçili filtreler öncelik sırasıyla (büyükten küçüğe) çalışır; belge değişmez, sonuç dosyaya yazılır.").weak().small());
            if ui.add_enabled(self.running.is_none(), egui::Button::new("Dışa aktar…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).add_filter("Düz metin", &["txt"]).save_file() {
                    let mut queue: Vec<usize> = filters.iter().filter(|f| self.export_sel.contains(&f.0)).map(|f| f.0).collect();
                    if queue.is_empty() {
                        match osisub_core::io::save(&self.doc.script, &p) { Ok(()) => self.status = format!("Dışa aktarıldı: {}", p.display()), Err(e) => self.status = format!("Hata: {}", e) }
                    } else {
                        let first = queue.remove(0);
                        self.export_job = Some(ExportJob { queue, script: self.doc.script.clone(), path: p });
                        self.run_export_step(first);
                    }
                }
            }
        });
        self.export_open = open;
    }

    pub(super) fn run_export_step(&mut self, id: usize) {
        let Some(script) = self.export_job.as_ref().map(|j| j.script.clone()) else { return };
        let name = self.macros.iter().find(|m| m.info.id == id).map(|m| m.info.name.clone()).unwrap_or_default();
        self.running = Some(name);
        self.sync_media_state();
        self.worker.send(Cmd::Run { id, script, selection: vec![], active: 0, props: vec![] });
    }

    pub(super) fn timing_window(&mut self, ctx: &egui::Context) {
        let mut open = self.timing_open;
        egui::Window::new("Zamanlama").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.shift_use_frames, "kare");
                if self.shift_use_frames {
                    ui.label("Kaydır (kare)");
                    ui.add(egui::DragValue::new(&mut self.shift_frames));
                    let fps = self.media.fps().unwrap_or(self.fps);
                    self.shift_ms = (self.shift_frames as f64 * 1000.0 / fps).round() as i64;
                    ui.label(RichText::new(format!("= {} ms", self.shift_ms)).weak());
                } else {
                    ui.label("Kaydır (ms)");
                    ui.add(egui::DragValue::new(&mut self.shift_ms).speed(10));
                }
                ui.checkbox(&mut self.shift_all, "tüm satırlar");
                ui.add_enabled(!self.shift_all, egui::Checkbox::new(&mut self.shift_after, "seçimden sonrakiler de")).on_hover_text("Seçili ilk satırın başlangıcından sonra başlayan tüm satırlar (ör. sahne kesildiğinde)");
                if ui.button("Uygula").clicked() {
                    let only: Option<Vec<usize>> = if self.shift_all {
                        None
                    } else {
                        let sel = self.doc.target_lines();
                        if self.shift_after {
                            let from = sel.iter().filter_map(|&i| if let Entry::Dialogue(d) = &self.doc.script.entries[i] { Some(d.start_time) } else { None }).min().unwrap_or(i64::MAX);
                            let mut v: Vec<usize> = self.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(d) if d.start_time >= from)).map(|(i, _)| i).collect();
                            v.extend(sel);
                            v.sort_unstable();
                            v.dedup();
                            Some(v)
                        } else {
                            Some(sel)
                        }
                    };
                    let n = only.as_ref().map_or(self.doc.script.dialogues().count(), |v| v.len());
                    let mut s = self.doc.script.clone();
                    // Kare kipi + video kare hızı: satır başına gerçek kare kaydırması (VFR doğru)
                    match self.media.framerate().filter(|_| self.shift_use_frames) {
                        Some(fr) => { qc::shift_frames(&mut s, &fr, self.shift_frames, only.as_deref()); self.doc.apply_new_script("Kare kaydır", s); }
                        None => { qc::shift(&mut s, self.shift_ms, only.as_deref()); self.doc.apply_new_script("Kaydır", s); }
                    }
                    self.status = format!("{} satır kaydırıldı", n);
                }
            });
            ui.separator();
            ui.strong("Son-işleyici");
            egui::Grid::new("post_grid").num_columns(2).show(ui, |ui| {
                ui.label("Lead-in (ms)"); ui.add(egui::DragValue::new(&mut self.post.lead_in_ms).speed(5)); ui.end_row();
                ui.label("Lead-out (ms)"); ui.add(egui::DragValue::new(&mut self.post.lead_out_ms).speed(5)); ui.end_row();
                ui.label("Bitişik satır eşiği (ms)"); ui.add(egui::DragValue::new(&mut self.post.adjacent_gap_ms).speed(5)); ui.end_row();
                ui.label("Boşluk eğilimi (0–1)"); ui.add(egui::DragValue::new(&mut self.post.adjacent_bias).speed(0.05).range(0.0..=1.0)); ui.end_row();
                ui.label("Anahtar kare: başl. önce/sonra"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.post.kf_start_before).speed(5)); ui.add(egui::DragValue::new(&mut self.post.kf_start_after).speed(5)); }); ui.end_row();
                ui.label("Anahtar kare: bitiş önce/sonra"); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.post.kf_end_before).speed(5)); ui.add(egui::DragValue::new(&mut self.post.kf_end_after).speed(5)); }); ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.post_selection_only, "Yalnızca seçim");
                ui.label(RichText::new(format!("{} anahtar kare", self.media.keyframes.len())).weak());
                if ui.button("Uygula").clicked() {
                    let mut o = self.post.clone();
                    o.keyframes = match self.media.framerate() { Some(fr) => fr.keyframe_boundaries(&self.media.keyframes), None => self.media.keyframes.clone() };
                    o.only = if self.post_selection_only { Some(self.doc.selection.clone()) } else { None };
                    let mut s = self.doc.script.clone();
                    timing::post_process(&mut s, &o);
                    self.doc.apply_new_script("Zamanlama son-işleyici", s);
                }
            });
            ui.separator();
            ui.strong("Noktadan senkron (iki nokta, doğrusal)");
            ui.label(RichText::new("Etkin satırı A/B noktası yapın, oynatma imlecini (playhead) hedef zaman olarak alın; uygulayınca tüm zamanlar kayar ve ölçeklenir.").weak().small());
            for (name, slot_is_a) in [("A", true), ("B", false)] {
                ui.horizontal(|ui| {
                    ui.label(format!("{} noktası:", name));
                    if ui.button("Etkin satır").clicked() {
                        if let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(self.doc.active) {
                            let v = Some((self.doc.active, d.start_time));
                            if slot_is_a { self.sync_a = v.map(|(i, t)| (i, t)); } else { self.sync_b = v.map(|(i, t)| (i, t)); }
                        }
                    }
                    let slot = if slot_is_a { &mut self.sync_a } else { &mut self.sync_b };
                    match slot {
                        Some((i, target)) => {
                            let src = match self.doc.script.entries.get(*i) { Some(Entry::Dialogue(d)) => d.start_time, _ => 0 };
                            ui.label(RichText::new(format!("{} →", time::format_ass(src))).monospace());
                            let mut t = *target;
                            if ui.button("Playhead").clicked() { t = self.media.playhead; }
                            ui.add(egui::DragValue::new(&mut t).speed(10).suffix(" ms"));
                            *target = t;
                            ui.label(RichText::new(time::format_ass(t)).monospace());
                        }
                        None => { ui.label(RichText::new("seçilmedi").weak()); }
                    }
                });
            }
            if ui.add_enabled(self.sync_a.is_some() && self.sync_b.is_some(), egui::Button::new("Senkronu uygula")).clicked() {
                if let (Some((ia, ta)), Some((ib, tb))) = (self.sync_a, self.sync_b) {
                    let src = |i: usize| match self.doc.script.entries.get(i) { Some(Entry::Dialogue(d)) => Some(d.start_time), _ => None };
                    if let (Some(sa), Some(sb)) = (src(ia), src(ib)) {
                        if sa == sb { self.status = "A ve B farklı zamanlarda olmalı".into(); } else {
                            let mut sc = self.doc.script.clone();
                            qc::linear_retime(&mut sc, (sa, ta), (sb, tb));
                            self.doc.apply_new_script("Noktadan senkron", sc);
                            self.sync_a = None; self.sync_b = None;
                            self.status = "Senkron uygulandı".into();
                        }
                    }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("FPS dönüştür");
                ui.add(egui::DragValue::new(&mut self.fps_from).speed(0.001).range(1.0..=240.0));
                ui.label("→");
                ui.add(egui::DragValue::new(&mut self.fps_to).speed(0.001).range(1.0..=240.0));
                if ui.button("Uygula").on_hover_text("Zamanları from/to oranıyla ölçekler (ör. 25 → 23.976)").clicked() {
                    let mut sc = self.doc.script.clone();
                    qc::fps_convert(&mut sc, self.fps_from, self.fps_to);
                    self.doc.apply_new_script("FPS dönüştür", sc);
                }
            });
            ui.horizontal(|ui| {
                ui.label("FPS");
                ui.add(egui::DragValue::new(&mut self.fps).speed(0.001).range(1.0..=240.0));
                if ui.button("Kareye oturt").clicked() {
                    let mut s = self.doc.script.clone();
                    match &self.media.timecodes {
                        Some(tc) => qc::snap_to_framerate(&mut s, tc),
                        None => qc::snap_to_frames(&mut s, self.fps),
                    }
                    self.doc.apply_new_script("Kareye oturt", s);
                }
                if self.media.timecodes.is_some() {
                    ui.label(RichText::new("(VFR timecode kullanılıyor)").weak());
                }
            });
        });
        self.timing_open = open;
    }

    pub(super) fn find_window(&mut self, ctx: &egui::Context) {
        let mut open = self.find_open;
        egui::Window::new("Bul / Değiştir").open(&mut open).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("find_grid").num_columns(2).show(ui, |ui| {
                ui.label("Bul");
                let r = ui.add(TextEdit::singleline(&mut self.find.pattern).desired_width(280.0));
                if self.find_focus { r.request_focus(); self.find_focus = false; }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { self.find_next(); }
                ui.end_row();
                ui.label("Değiştir");
                ui.add(TextEdit::singleline(&mut self.find.replacement).desired_width(280.0));
                ui.end_row();
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.find.regex, "Düzenli ifade");
                ui.checkbox(&mut self.find.match_case, "Büyük/küçük harf");
                ui.checkbox(&mut self.find.whole_word, "Tam sözcük");
                ui.checkbox(&mut self.find.ignore_tags, "Etiketleri yok say");
                ui.checkbox(&mut self.find.skip_comments, "Yorumları atla");
                ui.checkbox(&mut self.find_selection_only, "Yalnızca seçim");
            });
            ui.horizontal(|ui| {
                ui.label("Alanlar:");
                for (f, l) in [(Field::Text, "Metin"), (Field::Actor, "Konuşan"), (Field::Effect, "Efekt"), (Field::Style, "Stil")] {
                    let mut on = self.find.fields.contains(&f);
                    if ui.checkbox(&mut on, l).changed() {
                        if on { self.find.fields.push(f); } else { self.find.fields.retain(|x| *x != f); }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Sonrakini bul").clicked() { self.find_next(); }
                if ui.button("Tümünü seç").clicked() { self.find_select_all(); }
                if ui.button("Tümünü değiştir").clicked() { self.find_replace_all(); }
            });
        });
        self.find_open = open;
    }

    pub(super) fn find_opts(&self) -> search::Options {
        let mut o = self.find.clone();
        if self.find_selection_only { o.only = Some(self.doc.selection.clone()); }
        o
    }

    pub(super) fn find_next(&mut self) {
        let o = self.find_opts();
        match search::find_all(&self.doc.script, &o) {
            Ok(hits) if !hits.is_empty() => {
                let cur = self.doc.active;
                let h = hits.iter().find(|h| h.entry > cur).unwrap_or(&hits[0]).entry;
                self.doc.select(vec![h], h);
                self.scroll_to = Some(h);
                self.status = format!("{} eşleşme", hits.len());
            }
            Ok(_) => self.status = "Eşleşme yok".into(),
            Err(e) => self.status = format!("Arama hatası: {}", e),
        }
    }

    pub(super) fn find_select_all(&mut self) {
        let o = self.find_opts();
        match search::find_all(&self.doc.script, &o) {
            Ok(hits) => {
                let mut v: Vec<usize> = hits.iter().map(|h| h.entry).collect();
                v.dedup();
                self.status = format!("{} satır seçildi", v.len());
                let a = v.first().copied().unwrap_or(self.doc.active);
                self.doc.select(v, a);
                self.scroll_to = Some(a);
            }
            Err(e) => self.status = format!("Arama hatası: {}", e),
        }
    }

    pub(super) fn find_replace_all(&mut self) {
        let o = self.find_opts();
        let mut s = self.doc.script.clone();
        match search::replace_all(&mut s, &o) {
            Ok(n) => {
                self.doc.apply_new_script("Bul/Değiştir", s);
                self.status = format!("{} alan değiştirildi", n);
            }
            Err(e) => self.status = format!("Arama hatası: {}", e),
        }
    }

    pub(super) fn resample_window(&mut self, ctx: &egui::Context) {
        let mut open = self.resample_open;
        egui::Window::new("Çözünürlüğü yeniden örnekle").open(&mut open).collapsible(false).show(ctx, |ui| {
            let (ox, oy) = self.doc.script.res();
            ui.label(format!("Mevcut: {}×{}", ox, oy));
            ui.horizontal(|ui| {
                ui.label("Yeni");
                ui.add(egui::DragValue::new(&mut self.res_x).range(16..=16384));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.res_y).range(16..=16384));
                for (x, y, l) in [(1280, 720, "720p"), (1920, 1080, "1080p"), (384, 288, "384×288")] {
                    if ui.small_button(l).clicked() { self.res_x = x; self.res_y = y; }
                }
            });
            ui.label(RichText::new("Stiller, kenar boşlukları, \\pos/\\move/\\org/\\clip/\\fs/\\bord/... ve çizimler ölçeklenir.").weak().small());
            if ui.button("Uygula").clicked() {
                let mut s = self.doc.script.clone();
                timing::resample(&mut s, self.res_x, self.res_y);
                self.doc.apply_new_script("Çözünürlüğü yeniden örnekle", s);
            }
        });
        self.resample_open = open;
    }

    pub(super) fn goto_window(&mut self, ctx: &egui::Context) {
        let mut open = self.goto_open;
        egui::Window::new("Zamana git").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label("Zaman (H:MM:SS.cc, MM:SS ya da saniye):");
            let r = ui.add(TextEdit::singleline(&mut self.goto_buf).desired_width(160.0));
            if self.goto_focus { r.request_focus(); self.goto_focus = false; }
            if ui.button("Git").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                let t = time::parse_ass(&self.goto_buf).or_else(|| self.goto_buf.trim().parse::<f64>().ok().map(|s| (s * 1000.0) as i64));
                match t {
                    Some(t) => {
                        // en yakın satır (başlangıcı t'ye en yakın/içeren)
                        let best = self.rows.iter().copied().min_by_key(|&i| match &self.doc.script.entries[i] { Entry::Dialogue(d) => if d.start_time <= t && t < d.end_time { 0 } else { (d.start_time - t).abs().min((d.end_time - t).abs()) }, _ => i64::MAX });
                        if let Some(b) = best { self.doc.select(vec![b], b); self.scroll_to = Some(b); }
                        self.media.playhead = t;
                        self.goto_open = false;
                    }
                    None => self.status = "Geçersiz zaman".into(),
                }
            }
        });
        if !open { self.goto_open = false; }
    }

    pub(super) fn vad_window(&mut self, ctx: &egui::Context) {
        let mut open = self.vad_open;
        egui::Window::new("Sesten satır oluştur").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label("Konuşma bölgelerini bulup boş altyazı satırları oluşturur (zamanlama iskeleti).");
            egui::Grid::new("vad_grid").num_columns(2).show(ui, |ui| {
                ui.label("Eşik payı (dB)"); ui.add(egui::DragValue::new(&mut self.vad.margin_db).speed(0.5).range(3.0..=40.0)); ui.end_row();
                ui.label("Mutlak taban (dBFS)"); ui.add(egui::DragValue::new(&mut self.vad.floor_db).speed(0.5).range(-90.0..=-10.0)); ui.end_row();
                ui.label("En kısa konuşma (ms)"); ui.add(egui::DragValue::new(&mut self.vad.min_speech_ms).speed(10)); ui.end_row();
                ui.label("En kısa sessizlik (ms)"); ui.add(egui::DragValue::new(&mut self.vad.min_silence_ms).speed(10)); ui.end_row();
                ui.label("Kenar payı (ms)"); ui.add(egui::DragValue::new(&mut self.vad.pad_ms).speed(5)); ui.end_row();
            });
            ui.checkbox(&mut self.vad_skip_existing, "Zaten satır olan bölgeleri atla");
            if ui.button("Oluştur").clicked() {
                if let Some(w) = self.media.waveform() {
                    let segs = osisub_media::detect_speech(&w, &self.vad);
                    let existing: Vec<(i64, i64)> = self.doc.script.dialogues().map(|d| (d.start_time, d.end_time)).collect();
                    let style = self.doc.script.styles().next().map(|s| s.name.clone()).unwrap_or_else(|| "Default".into());
                    let mut s = self.doc.script.clone();
                    let mut n = 0;
                    for (a, b) in segs {
                        let covered = existing.iter().any(|&(x, y)| (y.min(b) - x.max(a)) * 2 > (b - a));
                        if self.vad_skip_existing && covered { continue; }
                        s.entries.push(Entry::Dialogue(osisub_core::Dialogue { start_time: a, end_time: b, style: style.clone(), ..Default::default() }));
                        n += 1;
                    }
                    qc::sort_by_start(&mut s);
                    self.doc.apply_new_script("Sesten satır oluştur", s);
                    self.status = format!("{} satır oluşturuldu", n);
                }
            }
        });
        self.vad_open = open;
    }

    pub(super) fn stats_window(&mut self, ctx: &egui::Context) {
        let mut open = self.stats_open;
        egui::Window::new("İstatistikler").open(&mut open).collapsible(false).default_width(360.0).show(ctx, |ui| {
            let st = osisub_core::stats::compute(&self.doc.script);
            egui::Grid::new("stats_grid").num_columns(2).striped(true).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, k: &str, v: String| { ui.label(k); ui.label(RichText::new(v).monospace()); ui.end_row(); };
                row(ui, "Satır (yorum hariç)", st.lines.to_string());
                row(ui, "Yorum satırı", st.comments.to_string());
                row(ui, "Sözcük", st.words.to_string());
                row(ui, "Karakter (boşluksuz)", st.chars.to_string());
                row(ui, "Toplam konuşma süresi", time::format_ass(st.total_duration_ms));
                row(ui, "Kapsanan aralık", time::format_ass(st.span_ms));
                row(ui, "Ortalama süre", format!("{:.2} sn", st.avg_duration_ms / 1000.0));
                row(ui, "Ortalama okuma hızı", format!("{:.1} kar/sn", st.avg_cps));
                row(ui, "En yüksek okuma hızı", format!("{:.1} kar/sn", st.max_cps));
            });
            ui.separator();
            ui.strong("Okuma hızı dağılımı");
            let labels = ["0–5", "6–10", "11–15", "16–20", "21–25", "26+"];
            let maxv = *st.cps_histogram.iter().max().unwrap_or(&1).max(&1) as f32;
            for (i, l) in labels.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{:>6}", l)).monospace());
                    let w = 200.0 * st.cps_histogram[i] as f32 / maxv;
                    let (r, _) = ui.allocate_exact_size(egui::vec2(w.max(1.0), 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, if i >= 4 { theme::BAD } else if i == 3 { theme::WARN } else { theme::ACCENT });
                    ui.label(st.cps_histogram[i].to_string());
                });
            }
            ui.separator();
            ui.strong("Stiller");
            for (k, v) in &st.styles { ui.label(format!("{}: {}", k, v)); }
            if !st.actors.is_empty() {
                ui.separator();
                ui.strong("Konuşanlar");
                for (k, v) in &st.actors { ui.label(format!("{}: {}", k, v)); }
            }
        });
        self.stats_open = open;
    }

    pub(super) fn fixes_window(&mut self, ctx: &egui::Context) {
        let mut open = self.fixes_open;
        egui::Window::new("Sık hataları düzelt").open(&mut open).collapsible(false).show(ctx, |ui| {
            let f = &mut self.fixes;
            ui.checkbox(&mut f.mojibake, "Bozuk Türkçe karakterleri onar (Ã¼ → ü, Ý → İ, þ → ş …)");
            ui.checkbox(&mut f.double_spaces, "Çift boşlukları tek yap");
            ui.checkbox(&mut f.edge_spaces, "Satır başı/sonu boşluklarını sil");
            ui.checkbox(&mut f.space_before_punct, "Noktalamadan önceki boşluğu sil");
            ui.checkbox(&mut f.space_after_punct, "Noktalamadan sonra boşluk ekle");
            ui.checkbox(&mut f.ellipsis, "Üç noktayı … yap");
            ui.checkbox(&mut f.dialogue_dash, "Diyalog çizgisinden sonra boşluk (-Merhaba → - Merhaba)");
            ui.checkbox(&mut f.sentence_case, "Cümle başlarını büyük harf yap (Türkçe i/İ, ı/I)");
            ui.checkbox(&mut f.remove_hi, "İşitme engelli notlarını sil ([ağlar], (müzik), AD: ön ekleri)");
            ui.horizontal(|ui| {
                let mut on = f.split_long.is_some();
                ui.checkbox(&mut on, "Uzun satırları böl (karakter)");
                let mut v = f.split_long.unwrap_or(84);
                ui.add_enabled(on, egui::DragValue::new(&mut v).range(20..=300));
                f.split_long = on.then_some(v);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.min_dur_on, "En kısa süre (ms)");
                let mut v = f.min_duration_ms.unwrap_or(833);
                ui.add_enabled(self.min_dur_on, egui::DragValue::new(&mut v).speed(10));
                f.min_duration_ms = self.min_dur_on.then_some(v);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.min_gap_on, "En küçük boşluk (ms)");
                let mut v = f.min_gap_ms.unwrap_or(84);
                ui.add_enabled(self.min_gap_on, egui::DragValue::new(&mut v).speed(5));
                f.min_gap_ms = self.min_gap_on.then_some(v);
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.fixes_selection_only, "Yalnızca seçim");
                if ui.button("Uygula").clicked() {
                    let mut o = self.fixes.clone();
                    o.only = if self.fixes_selection_only { Some(self.doc.selection.clone()) } else { None };
                    let mut s = self.doc.script.clone();
                    let rep = osisub_core::fixes::apply(&mut s, &o);
                    self.doc.apply_new_script("Sık hataları düzelt", s);
                    self.status = format!("{} satır değişti, {} zamanlama düzeltildi, {} satır bölündü", rep.changed_lines, rep.timing_changes, rep.split_lines);
                }
            });
        });
        self.fixes_open = open;
    }

    pub(super) fn start_font_scan(&mut self) {
        let used = osisub_core::fonts_used::used_fonts(&self.doc.script);
        let (tx, rx) = std::sync::mpsc::channel();
        self.font_rx = Some(rx);
        self.font_report = None;
        std::thread::spawn(move || {
            let mut db = osisub_lua::fonts::FontDb::default();
            let rep: Vec<_> = used.into_iter().map(|u| { let f = db.files_for(&u.family); (u, f) }).collect();
            let _ = tx.send(rep);
        });
    }

    pub(super) fn fonts_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.font_rx {
            if let Ok(r) = rx.try_recv() { self.font_report = Some(r); self.font_rx = None; }
            else { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
        }
        let mut open = self.fonts_open;
        egui::Window::new("Yazı tipi denetimi").open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            if ui.button("Yenile").clicked() { self.start_font_scan(); }
            match &self.font_report {
                None => { ui.horizontal(|ui| { ui.spinner(); ui.label("Sistem yazı tipleri taranıyor…"); }); }
                Some(rep) => {
                    let missing = rep.iter().filter(|(_, f)| f.is_empty()).count();
                    ui.label(if missing == 0 { RichText::new("Tüm yazı tipleri kurulu").color(Color32::from_rgb(0x7a, 0xd9, 0x8a)) } else { RichText::new(format!("{} yazı tipi eksik", missing)).color(theme::BAD) });
                    egui::Grid::new("fonts_grid").striped(true).show(ui, |ui| {
                        ui.strong("Aile"); ui.strong("Stil"); ui.strong("Satır"); ui.strong("Durum"); ui.end_row();
                        for (u, files) in rep {
                            ui.label(&u.family);
                            ui.label(format!("{}{}", if u.bold { "kalın " } else { "" }, if u.italic { "italik" } else { "" }));
                            ui.label(u.lines.to_string());
                            if files.is_empty() { ui.label(RichText::new("EKSİK").color(theme::BAD)); } else { ui.label(RichText::new("kurulu").color(Color32::from_rgb(0x7a, 0xd9, 0x8a))).on_hover_text(files.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n")); }
                            ui.end_row();
                        }
                    });
                    if ui.button("Kurulu yazı tiplerini klasöre kopyala…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let mut n = 0;
                            for (_, files) in rep { for f in files { if let Some(name) = f.file_name() { if std::fs::copy(f, dir.join(name)).is_ok() { n += 1; } } } }
                            self.status = format!("{} yazı tipi dosyası kopyalandı", n);
                        }
                    }
                }
            }
        });
        self.fonts_open = open;
    }
}
