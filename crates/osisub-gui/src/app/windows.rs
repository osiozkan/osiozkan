use crate::i18n::tr;
use crate::i18n::trf;
use super::*;

/// OP/ED yerleştirme penceresi: önceki bölüme göre zamanlanmış şarkı dosyasını yeni bölüme
/// sesle (ya da playhead'e) yerleştirir.
#[derive(Default)]
pub(crate) struct SongWin {
    song: Option<PathBuf>,
    old: Option<PathBuf>,
    by_audio: bool,
    rx: Option<std::sync::mpsc::Receiver<Result<(osisub_core::Script, i64, f64), String>>>,
    error: Option<String>,
}

impl SongWin {
    pub(crate) fn new() -> SongWin {
        SongWin { by_audio: true, ..Default::default() }
    }
}

/// Şarkı dosyasındaki satırların kapsadığı aralık (yorumlar hariç).
fn song_span(sc: &osisub_core::Script) -> Option<(i64, i64)> {
    let ds: Vec<&osisub_core::Dialogue> = sc.dialogues().filter(|d| !d.comment).collect();
    Some((ds.iter().map(|d| d.start_time).min()?, ds.iter().map(|d| d.end_time).max()?))
}

/// Sesten altyazı (whisper.cpp) penceresi durumu.
pub(crate) struct WhisperWin {
    pub(crate) path: Option<PathBuf>,
    models: Vec<PathBuf>,
    model: Option<PathBuf>,
    lang: String,
    translate: bool,
    max_len: u32,
    as_reference: bool,
    /// İlerleme (0..1) ve sonuç iletileri
    rx: Option<std::sync::mpsc::Receiver<Result<Option<osisub_core::Script>, String>>>,
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    started: Option<std::time::Instant>,
    error: Option<String>,
    ok: bool,
}

impl Default for WhisperWin {
    fn default() -> Self {
        let models = osisub_media::whisper::models();
        WhisperWin { path: None, model: models.first().cloned(), models, lang: "ja".into(), translate: false, max_len: 42, as_reference: false, rx: None, progress: Default::default(), started: None, error: None, ok: osisub_media::whisper::available() }
    }
}

/// Resim altyazısı (PGS) OCR penceresi durumu.
pub(crate) struct OcrWin {
    path: Option<PathBuf>,
    /// Videodaki PGS izleri (ffmpeg dizini, etiket)
    tracks: Vec<(u32, String)>,
    track: Option<u32>,
    langs: Vec<String>,
    lang: String,
    as_reference: bool,
    rx: Option<std::sync::mpsc::Receiver<Result<osisub_core::Script, String>>>,
    started: Option<std::time::Instant>,
    error: Option<String>,
    /// Tesseract kurulu mu (pencere açılırken bir kez denetlenir)
    tess_ok: bool,
    /// sonuç gelince yüklü yazım sözlüğüyle OCR düzeltmesi
    fix: bool,
}

impl OcrWin {
    /// Videodaki belirli bir PGS izi için hazır pencere.
    pub(crate) fn for_track(path: PathBuf, tracks: &[osisub_media::mux::SubTrack], index: u32) -> OcrWin {
        let mut w = OcrWin::default();
        w.tracks = tracks.iter().filter(|t| osisub_media::mux::is_bitmap_sub(&t.codec)).map(|t| (t.index, t.label())).collect();
        w.track = Some(index);
        if let Some(l) = tracks.iter().find(|t| t.index == index).map(|t| t.language.clone()) {
            // MKV dil kodu (eng, jpn…) Tesseract dil adıyla aynı; kuruluysa seç
            if w.langs.contains(&l) {
                w.lang = l;
            }
        }
        w.path = Some(path);
        w
    }
}

/// Düz metin içe aktarma penceresi (core::textimport).
#[derive(Default)]
pub(crate) struct TextImportWin {
    pub text: String,
    pub blocks: bool,
    pub speakers: bool,
}

/// Gömülü (hardsub) altyazıyı metne çevir penceresi (osisub_media::hardsub).
pub(crate) struct HardsubWin {
    path: Option<PathBuf>,
    opts: osisub_media::hardsub::HardsubOptions,
    langs: Vec<String>,
    as_reference: bool,
    rx: Option<std::sync::mpsc::Receiver<Result<osisub_core::Script, String>>>,
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    started: Option<std::time::Instant>,
    error: Option<String>,
    tess_ok: bool,
    fix: bool,
}

impl HardsubWin {
    pub(crate) fn new(video: Option<PathBuf>) -> Self {
        let langs = osisub_media::ocr::languages();
        let mut opts = osisub_media::hardsub::HardsubOptions::default();
        if !langs.iter().any(|l| l == "eng") { if let Some(l) = langs.first() { opts.lang = l.clone(); } }
        HardsubWin { path: video, opts, langs, as_reference: true, rx: None, progress: Default::default(), cancel: Default::default(), started: None, error: None, tess_ok: osisub_media::ocr::tesseract_available(), fix: false }
    }
}

impl Default for OcrWin {
    fn default() -> Self {
        let langs = osisub_media::ocr::languages();
        let lang = if langs.iter().any(|l| l == "eng") { "eng".into() } else { langs.first().cloned().unwrap_or_else(|| "eng".into()) };
        OcrWin { path: None, tracks: vec![], track: None, langs, lang, as_reference: false, rx: None, started: None, error: None, tess_ok: osisub_media::ocr::tesseract_available(), fix: false }
    }
}

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
        self.status = trf!("{} stil eklendi", n);
    }

    pub(super) fn import_window(&mut self, ctx: &egui::Context) {
        let Some(list) = self.import_styles.as_mut() else { return };
        let mut done: Option<bool> = None;
        egui::Window::new(tr("Stilleri içe aktar")).collapsible(false).show(ctx, |ui| {
            for (st, on) in list.iter_mut() { ui.checkbox(on, &st.name); }
            ui.horizontal(|ui| {
                if ui.button(tr("İçe aktar")).clicked() { done = Some(true); }
                if ui.button(tr("İptal")).clicked() { done = Some(false); }
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
                ui.label(RichText::new(trf!("{} — satır {}", file, line)).weak());
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
            egui::CollapsingHeader::new(tr("Ayrıntı (yığın izi)")).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| { ui.label(RichText::new(&msg).monospace().small()); });
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Kopyala")).clicked() { ui.ctx().copy_text(msg.clone()); }
                if ui.button(tr("Kapat")).clicked() { close = true; }
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
        egui::Window::new(tr("Satırları seç")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            let c = &mut self.select_crit;
            let r = ui.add(TextEdit::singleline(&mut c.pattern).hint_text(tr("desen (boş + Tam = alanı boş olanlar)")).desired_width(320.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { apply = true; }
            ui.horizontal(|ui| {
                ui.label(tr("Alan:"));
                for (f, l) in [(Field::Text, "Metin"), (Field::Style, "Stil"), (Field::Actor, "Konuşan"), (Field::Effect, "Efekt")] { ui.radio_value(&mut c.field, f, l); }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Eşleşme:"));
                for (m, l) in [(Mode::Contains, "İçerir"), (Mode::Exact, "Tam"), (Mode::Regex, "Düzenli ifade")] { ui.radio_value(&mut c.mode, m, l); }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut c.match_case, tr("Büyük/küçük harf duyarlı"));
                ui.checkbox(&mut c.invert, tr("Eşleşmeyenleri seç"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Satırlar:"));
                ui.checkbox(&mut c.dialogues, tr("Diyalog"));
                ui.checkbox(&mut c.comments, tr("Yorum"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Seçim:"));
                for (a, l) in [(Action::Set, "Yeni seçim"), (Action::Add, "Ekle"), (Action::Subtract, "Çıkar"), (Action::Intersect, "Kesişim")] { ui.radio_value(&mut c.action, a, l); }
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Seç")).clicked() { apply = true; }
                if ui.button(tr("Kapat")).clicked() { self.select_open = false; }
            });
        });
        if apply {
            match osisub_core::select::select(&self.doc.script, &self.select_crit, &self.doc.selection) {
                Ok(sel) => {
                    let n = sel.len();
                    let active = if sel.contains(&self.doc.active) { self.doc.active } else { sel.first().copied().unwrap_or(self.doc.active) };
                    self.doc.select(sel, active);
                    self.scroll_to = Some(active);
                    self.status = trf!("{} satır seçildi", n);
                }
                Err(e) => self.status = trf!("Desen hatası: {}", e),
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
        egui::Window::new(tr("Ekler")).open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            if list.is_empty() { ui.label(RichText::new(tr("Bu betikte gömülü dosya yok.")).weak()); }
            egui::Grid::new("attach_grid").num_columns(4).striped(true).show(ui, |ui| {
                for a in &list {
                    ui.label(RichText::new(&a.name).monospace());
                    ui.label(RichText::new(&a.section).weak());
                    ui.label(format!("{:.1} KB", a.size as f64 / 1024.0));
                    ui.horizontal(|ui| {
                        if ui.small_button(tr("Kaydet…")).clicked() { save = Some(a.name.clone()); }
                        if ui.small_button(tr("Sil")).clicked() { remove = Some(a.name.clone()); }
                    });
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("Dosya ekle…")).clicked() { add_files = true; }
                if ui.button(tr("Kullanılan yazı tiplerini göm")).on_hover_text(tr("Betikte kullanılan ve sistemde bulunan yazı tipi dosyalarını [Fonts] bölümüne ekler")).clicked() { embed_used = true; }
            });
            ui.label(RichText::new(tr("Gömülü yazı tipleri libass/VSFilter tarafından okunur; dosya boyutu ~%33 büyür.")).weak().small());
        });
        if let Some(n) = remove {
            let mut s = self.doc.script.clone();
            if attach::remove(&mut s, &n) { self.doc.apply_new_script("Eki sil", s); self.status = trf!("Ek silindi: {}", n); }
        }
        if let Some(n) = save {
            if let (Some(data), Some(p)) = (attach::extract(&self.doc.script, &n), rfd::FileDialog::new().set_file_name(n.as_str()).save_file()) {
                match std::fs::write(&p, data) { Ok(()) => self.status = trf!("Kaydedildi: {}", p.display()), Err(e) => self.status = trf!("Hata: {}", e) }
            }
        }
        if add_files {
            if let Some(files) = rfd::FileDialog::new().add_filter("Yazı tipi / grafik", &["ttf", "otf", "ttc", "otc", "png", "jpg", "bmp", "gif"]).pick_files() {
                let mut s = self.doc.script.clone();
                let mut n = 0;
                for f in files {
                    if let (Ok(data), Some(name)) = (std::fs::read(&f), f.file_name().map(|x| x.to_string_lossy().to_string())) { attach::attach(&mut s, &name, &data); n += 1; }
                }
                if n > 0 { self.doc.apply_new_script("Ek ekle", s); self.status = trf!("{} dosya gömüldü", n); }
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
                ui.label(RichText::new(trf!("+{} eklendi", a)).color(theme::ACCENT));
                ui.label(RichText::new(trf!("−{} silindi", r)).color(theme::BAD));
                ui.label(RichText::new(trf!("~{} değişti", m)).color(theme::WARN));
                if ui.small_button(tr("Yenile")).on_hover_text(tr("Açık belgedeki son düzenlemelerle yeniden karşılaştır")).clicked() { refresh = true; }
                if ui.small_button(tr("HTML rapor kaydet…")).on_hover_text(tr("Sözcük düzeyinde farklarla tek dosyalık rapor (editör değişikliklerini çevirmene göndermek için)")).clicked() {
                    let title = format!("{} → {}", name, self.doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "açık belge".into()));
                    if let Some(p) = rfd::FileDialog::new().set_file_name("farklar.html").add_filter("HTML", &["html"]).save_file() {
                        self.status = match std::fs::write(&p, osisub_core::compare::report_html(&title, &old, &self.doc.script, &changes)) { Ok(()) => format!("Rapor kaydedildi: {}", p.display()), Err(e) => format!("Hata: {e}") };
                    }
                }
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
                            if *timing { ui.label(RichText::new(trf!("    zaman: {} → {}", t0, t1)).monospace().small()); }
                            if *text {
                                ui.label(RichText::new(trf!("    eski: {}", x0)).color(Color32::GRAY).strikethrough());
                                ui.label(RichText::new(trf!("    yeni: {}", x1)));
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
        egui::Window::new(tr("Kısayolları düzenle")).open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Düğmeye tıklayıp tuş birleşimine basın (Esc = iptal, Backspace = kaldır). Lua makroları için: Otomasyon sekmesi.")).weak().small());
            if ui.button(tr("Aegisub hotkey.json içe aktar…")).on_hover_text(tr("Aegisub'daki kısayollarınızı (karşılığı olan komutlar ve Lua makroları) alır")).clicked() {
                let mut dlg = rfd::FileDialog::new().add_filter("hotkey.json", &["json"]);
                if let Some(d) = osisub_core::paths::aegisub_automation_dirs().first().and_then(|d| d.parent().map(|p| p.to_path_buf())) { dlg = dlg.set_directory(d); }
                if let Some(p) = dlg.pick_file() { self.import_aegisub_hotkeys(&p); }
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("hk_grid").num_columns(2).striped(true).show(ui, |ui| {
                    for (id, label) in super::COMMANDS {
                        ui.label(tr(label));
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
                self.status = trf!("Aegisub kısayolları: {n} atandı, {} atlandı", skipped.len());
                if !skipped.is_empty() { self.log.push_str(&format!("Aegisub kısayolları — atlananlar:\n  {}\n", skipped.join("\n  "))); }
            }
            Err(e) => self.status = e,
        }
    }

    pub(super) fn merge_window(&mut self, ctx: &egui::Context) {
        let Some((path, mut opts, mut offset)) = self.merge_win.take() else { return };
        let mut open = true;
        let mut go = false;
        egui::Window::new(tr("Dosya birleştir")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(path.display().to_string()).monospace().small());
            ui.horizontal(|ui| {
                ui.label(tr("Zaman kaydırma"));
                ui.add(egui::TextEdit::singleline(&mut offset).desired_width(110.0).font(egui::TextStyle::Monospace))
                    .on_hover_text(tr("Eklenen satırlara eklenir (ör. OP'yi 0:01:30.00'a yerleştirmek için). Negatif için başına '-'"));
            });
            ui.checkbox(&mut opts.sort, tr("Satırları zamana göre sırala"));
            ui.checkbox(&mut opts.skip_comments, tr("Yorum satırlarını alma"));
            ui.label(RichText::new(tr("Aynı adlı farklı stiller \"Ad (2)\" olarak eklenir; farklı çözünürlük bu belgeninkine örneklenir.")).small().weak());
            if ui.button(tr("Birleştir")).clicked() { go = true; }
        });
        if go {
            let (neg, t) = match offset.trim().strip_prefix('-') { Some(r) => (true, r), None => (false, offset.trim()) };
            let Some(ms) = osisub_core::time::parse_generic(t).or_else(|| t.parse::<i64>().ok()) else {
                self.status = trf!("Geçersiz zaman: {offset}");
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
        // Transform verisi değilse köşe pini (CC Power Pin / Corner Pin) dene
        let pin = if parsed.is_err() { osisub_core::motion::parse_corner_pin(&m.text).ok() } else { None };
        egui::Window::new(tr("Hareket izleme verisi")).open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Mocha: Export Tracking Data → \"After Effects Transform Data\" (konum/ölçek/dönüş) ya da \"After Effects CC Power Pin\" / \"Corner Pin\" (perspektif) → panoya kopyala, aşağıdaki kutuya Ctrl+V.")).small().weak());
            ui.horizontal(|ui| {
                if ui.button(tr("Dosyadan yükle…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Keyframe verisi", &["txt", "*"]).pick_file() {
                        if let Ok(t) = std::fs::read_to_string(&p) { m.text = t; }
                    }
                }
                if ui.button(tr("Temizle")).clicked() { m.text.clear(); }
            });
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut m.text).font(egui::TextStyle::Monospace).desired_rows(8).desired_width(f32::INFINITY).hint_text(tr("Adobe After Effects 6.0 Keyframe Data …")));
            });
            match (&parsed, &pin) {
                (Ok(d), _) => { ui.label(RichText::new(trf!("{} kare · kaynak {}×{} · {} fps", d.frames.len(), d.width, d.height, d.fps)).color(theme::ACCENT)); }
                (_, Some(d)) => { ui.label(RichText::new(trf!("Köşe pini: {} kare · kaynak {}×{} — satır kutusu her karede dörtgene perspektifle oturtulur (\\frx\\fry\\fax…), kırpma homografiyle taşınır", d.frames.len(), d.width, d.height)).color(theme::ACCENT)); }
                (Err(e), None) if !m.text.trim().is_empty() => { ui.label(RichText::new(e).color(theme::BAD)); }
                _ => {}
            }
            if pin.is_none() { ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut m.opts.position, tr("Konum"));
                ui.checkbox(&mut m.opts.scale, tr("Ölçek"));
                ui.add_enabled(m.opts.scale, egui::Checkbox::new(&mut m.opts.scale_border, "Kenar/gölge/bulanıklık da"));
                ui.checkbox(&mut m.opts.rotation, tr("Döndürme"));
                ui.checkbox(&mut m.opts.clip, tr("Kırpma (\\clip)"));
                ui.checkbox(&mut m.opts.linear, tr("Doğrusal (\\move + \\t, tek satır)")).on_hover_text(tr("Satırı kare kare bölmek yerine ilk ve son karenin değerleri arasında doğrusal geçiş; yavaş/düz hareketler için hafif çıktı"));
            }); }
            ui.horizontal(|ui| {
                ui.label(tr("İzin ilk karesi = video karesi"));
                ui.add(egui::DragValue::new(&mut m.opts.start_frame).range(0..=10_000_000));
                if ui.small_button(tr("seçimin ilk karesi")).clicked() {
                    let fr = self.motion_framerate(parsed.as_ref().ok().map(|d| d.fps));
                    if let Some(t) = self.doc.target_lines().iter().filter_map(|&i| match self.doc.script.entries.get(i) { Some(Entry::Dialogue(d)) => Some(d.start_time), _ => None }).min() {
                        m.opts.start_frame = fr.frame_at_start(t);
                    }
                }
            });
            let n = self.doc.target_lines().len();
            if ui.add_enabled((parsed.is_ok() || pin.is_some()) && n > 0, egui::Button::new(format!("{n} satıra uygula"))).clicked() { apply = true; }
        });
        if apply {
            if let Some(data) = &pin {
                let fr = self.motion_framerate(Some(data.fps));
                let targets: Vec<usize> = self.doc.target_lines().into_iter().filter(|&i| matches!(self.doc.script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
                let lines: Vec<osisub_core::ass::Dialogue> = targets.iter().filter_map(|&i| match &self.doc.script.entries[i] { Entry::Dialogue(d) => Some(d.clone()), _ => None }).collect();
                let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
                let boxes: Result<Vec<(f64, f64)>, String> = lines.iter().map(|d| osisub_lua::fonts::line_box(db, &self.doc.script, d)).collect();
                let mut s = self.doc.script.clone();
                // uzaklık kopyada hesaplanır; LayoutRes satırlar yerleştikten sonra yazılır (girdi indeksleri kaymasın)
                let dist = self.perspective_dist(&mut s.clone());
                match boxes.and_then(|b| osisub_core::motion::apply_corner_pin(&lines, &b, &s, data, &fr, m.opts.start_frame, dist)) {
                    Ok(new_lines) => {
                        let at = targets[0];
                        for &i in targets.iter().rev() { s.entries.remove(i); }
                        let n = new_lines.len();
                        s.entries.splice(at..at, new_lines.into_iter().map(Entry::Dialogue));
                        let before = s.entries.len();
                        self.perspective_dist(&mut s);
                        // LayoutRes eklendiyse olay satırları kayar
                        let at = at + (s.entries.len() - before);
                        self.doc.apply_new_script("Köşe pini izleme", s);
                        self.doc.select((at..at + n).collect(), at);
                        self.scroll_to = Some(at);
                        self.status = trf!("Köşe pini: {} satır → {} kare satırı (perspektif)", lines.len(), n);
                        return;
                    }
                    Err(e) => self.status = e,
                }
            } else if let Ok(data) = &parsed {
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
                        self.status = trf!("Hareket izleme: {} satır → {} kare satırı", lines.len(), n);
                        return; // pencere kapanır
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if open { self.motion = Some(m); }
    }

    pub(super) fn batch_window(&mut self, ctx: &egui::Context) {
        let Some(mut b) = self.batch.take() else { return };
        let mut open = true;
        // birden çok satır seçiliyse yalnızca seçim, yoksa tüm belge
        let sel = self.doc.selection.clone();
        let scope: Option<Vec<usize>> = (sel.len() > 1).then_some(sel);
        let mut action: Option<u8> = None;
        egui::Window::new(tr("Toplu satır işlemleri")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(match &scope { Some(s) => format!("Kapsam: seçili {} satır", s.len()), None => "Kapsam: tüm belge (birden çok satır seçerek daraltın)".into() }).small().weak());
            ui.label(RichText::new(tr("Yorum, tipografi, karaoke ve fx satırlarına dokunulmaz.")).small().weak());
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("Aynı metinli ardışık satırları birleştir; en çok boşluk"));
                ui.add(egui::DragValue::new(&mut b.same_gap).range(0..=5000).suffix(" ms"));
                if ui.button(tr("Uygula")).clicked() { action = Some(1); }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Kısa satırları birleştir; toplam ≤"));
                ui.add(egui::DragValue::new(&mut b.short_chars).range(4..=120).suffix(" kar"));
                ui.label(tr("boşluk ≤"));
                ui.add(egui::DragValue::new(&mut b.short_gap).range(0..=5000).suffix(" ms"));
                if ui.button(tr("Uygula")).clicked() { action = Some(2); }
            });
            ui.separator();
            ui.label(tr("Süreleri okuma hızına göre yeniden hesapla"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut b.cps).range(5.0..=40.0).speed(0.5).suffix(" kar/sn"));
                ui.label(tr("en az"));
                ui.add(egui::DragValue::new(&mut b.min_ms).range(100..=10000).suffix(" ms"));
                ui.label(tr("en çok"));
                ui.add(egui::DragValue::new(&mut b.max_ms).range(500..=30000).suffix(" ms"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("sonraki satıra boşluk"));
                ui.add(egui::DragValue::new(&mut b.gap_ms).range(0..=2000).suffix(" ms"));
                ui.checkbox(&mut b.extend_only, tr("yalnızca uzat"));
                if ui.button(tr("Uygula")).clicked() { action = Some(3); }
            });
        });
        if let Some(a) = action {
            let mut s = self.doc.script.clone();
            let only = scope.as_deref();
            let (n, what) = match a {
                1 => (osisub_core::batch::merge_same_text(&mut s, b.same_gap, only), "Aynı metinli satırları birleştir"),
                2 => (osisub_core::batch::merge_short_lines(&mut s, b.short_chars, b.short_gap, only), "Kısa satırları birleştir"),
                _ => (osisub_core::batch::recalc_durations(&mut s, b.cps, b.min_ms, b.max_ms, b.gap_ms, b.extend_only, only), "Süreleri yeniden hesapla"),
            };
            self.doc.apply_new_script(what, s);
            self.status = trf!("{what}: {n} satır");
        }
        if open { self.batch = Some(b); }
    }

    /// Stil yardımcısı (Aegisub Styling Assistant): etkin satırı gösterir; süzgeçle stil seçilir,
    /// Enter (ya da stile tıklama) stili uygular ve sonraki satıra geçer (medya açıksa satırı oynatır).
    pub(super) fn style_assist_window(&mut self, ctx: &egui::Context) {
        let Some(mut filter) = self.style_assist.take() else { return };
        let mut open = true;
        let mut chosen: Option<String> = None;
        let mut skip = 0i64;
        let at = self.doc.active;
        let (cur_style, text) = match self.doc.script.entries.get(at) { Some(Entry::Dialogue(d)) => (d.style.clone(), d.text.clone()), _ => (String::new(), String::new()) };
        let styles: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        let fl = filter.to_lowercase();
        let matches: Vec<&String> = styles.iter().filter(|s| fl.is_empty() || s.to_lowercase().contains(&fl)).collect();
        egui::Window::new(tr("Stil yardımcısı")).open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label(RichText::new(osisub_core::tags::visible_text(&text).replace('\n', " / ")).size(16.0));
            ui.label(RichText::new(trf!("Şu anki stil: {cur_style}")).weak());
            let r = ui.add(egui::TextEdit::singleline(&mut filter).hint_text(tr("stil süz… (Enter = ilk eşleşeni uygula, sonraki satır)")).desired_width(f32::INFINITY));
            // lost_focus() bellekten okunur: odak geri istenmeden ÖNCE denetlenmeli
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(s) = matches.first() { chosen = Some((*s).clone()); }
            }
            if !r.has_focus() && !ui.ctx().memory(|m| m.focused().is_some()) { r.request_focus(); }
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for s in &matches {
                    if ui.selectable_label(**s == cur_style, s.as_str()).clicked() { chosen = Some((*s).clone()); }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(tr("< Önceki satır")).clicked() { skip = -1; }
                if ui.button(tr("Atla >")).clicked() { skip = 1; }
                ui.label(RichText::new(tr("Ayrıca: PgUp/PgDn ile satırlar arasında gezin")).small().weak());
            });
            if ui.input(|i| i.key_pressed(egui::Key::PageDown)) { skip = 1; }
            if ui.input(|i| i.key_pressed(egui::Key::PageUp)) { skip = -1; }
        });
        if let Some(st) = chosen {
            if st != cur_style {
                self.doc.edit_keyed("Stil yardımcısı", None, |tx| tx.dialogue_mut(at, |d| d.style = st.clone()));
            }
            filter.clear();
            skip = 1;
        }
        if skip != 0 {
            let ds: Vec<usize> = self.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
            if let Some(p) = ds.iter().position(|&i| i == at) {
                let np = (p as i64 + skip).clamp(0, ds.len() as i64 - 1) as usize;
                let ni = ds[np];
                self.doc.select(vec![ni], ni);
                self.scroll_to = Some(ni);
                if self.media.is_open() { self.media.stop(); self.media.toggle_play(&self.doc, ctx); }
            }
        }
        if open { self.style_assist = Some(filter); }
    }

    pub(super) fn filebatch_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.filebatch.take() else { return };
        let mut open = true;
        if let Some(rx) = &w.rx {
            while let Ok(m) = rx.try_recv() {
                if m.is_empty() { w.rx = None; break; }
                w.log.push_str(&m);
                w.log.push('\n');
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let running = w.rx.is_some();
        let mut start = false;
        egui::Window::new(tr("Toplu dosya işlemleri")).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr("Dosya ekle…")).clicked() {
                    if let Some(v) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_files() { for p in v { if !w.files.contains(&p) { w.files.push(p); } } }
                }
                if ui.button(tr("Klasör ekle…")).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        if let Ok(rd) = std::fs::read_dir(&d) {
                            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && osisub_core::io::OPEN_EXTS.contains(&osisub_core::io::ext_of(p).as_str()) && osisub_core::io::ext_of(p) != "txt").collect();
                            v.sort();
                            for p in v { if !w.files.contains(&p) { w.files.push(p); } }
                        }
                    }
                }
                if ui.button(tr("Temizle")).clicked() { w.files.clear(); }
                ui.label(trf!("{} dosya", w.files.len()));
            });
            egui::ScrollArea::vertical().id_salt("fb_files").max_height(120.0).show(ui, |ui| {
                let mut rm = None;
                for (i, f) in w.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button(tr("x")).clicked() { rm = Some(i); }
                        ui.label(RichText::new(f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).monospace().small());
                    });
                }
                if let Some(i) = rm { w.files.remove(i); }
            });
            ui.separator();
            egui::Grid::new("fb_opts").num_columns(2).show(ui, |ui| {
                ui.label(tr("Çıktı biçimi"));
                egui::ComboBox::from_id_salt("fb_ext").selected_text(if w.job.out_ext.is_empty() { "aynı".to_string() } else { w.job.out_ext.clone() }).show_ui(ui, |ui| {
                    for e in ["", "ass", "srt", "vtt", "ttml", "itt", "stl", "sbv", "smi", "lrc", "txt"] { ui.selectable_value(&mut w.job.out_ext, e.to_string(), if e.is_empty() { "aynı" } else { e }); }
                });
                ui.end_row();
                ui.label(tr("Çıktı klasörü"));
                ui.horizontal(|ui| {
                    ui.label(w.job.out_dir.as_ref().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| "girdinin klasörü".into()));
                    if ui.small_button(tr("Seç…")).clicked() { w.job.out_dir = rfd::FileDialog::new().pick_folder(); }
                    if w.job.out_dir.is_some() && ui.small_button(tr("sıfırla")).clicked() { w.job.out_dir = None; }
                });
                ui.end_row();
                ui.label(tr("Ada son ek"));
                ui.add(egui::TextEdit::singleline(&mut w.job.suffix).desired_width(120.0).hint_text(tr("_tr")));
                ui.end_row();
                ui.label(tr("Zaman kaydır"));
                ui.add(egui::DragValue::new(&mut w.job.shift_ms).speed(10).suffix(" ms"));
                ui.end_row();
                ui.label("");
                ui.vertical(|ui| {
                    ui.checkbox(&mut w.fixes, tr("Sık hataları düzelt (varsayılan kurallar)"));
                    ui.checkbox(&mut w.job.remove_comments, tr("Yorum satırlarını sil"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut w.resample, tr("Yeniden örnekle"));
                        ui.add_enabled(w.resample, egui::DragValue::new(&mut w.res.0).range(16..=8192));
                        ui.label("×");
                        ui.add_enabled(w.resample, egui::DragValue::new(&mut w.res.1).range(16..=8192));
                    });
                    ui.checkbox(&mut w.job.overwrite, tr("Girdinin üzerine yazmaya izin ver"));
                });
                ui.end_row();
            });
            if let Some(f) = w.files.first() {
                ui.label(RichText::new(trf!("Örnek çıktı: {}", osisub_core::filebatch::out_path(f, &w.job).display())).small().weak());
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!running && !w.files.is_empty(), egui::Button::new(tr("Başlat"))).clicked() { start = true; }
                if ui.add_enabled(!running && !w.files.is_empty(), egui::Button::new(tr("Kalite raporu (HTML)…"))).on_hover_text(tr("Tüm dosyaları denetler (zamanlama, okuma hızı, etiket hataları, çevrilmemiş satırlar, betik başlığı) ve özet tablolu tek bir HTML rapor kaydeder; dosyalar değişmez")).clicked() {
                    if let Some(out) = rfd::FileDialog::new().add_filter("HTML", &["html"]).set_file_name("toplu-qc.html").save_file() {
                        let files = w.files.clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        let c2 = ui.ctx().clone();
                        std::thread::spawn(move || {
                            let mut res = vec![];
                            for f in &files {
                                match osisub_core::filebatch::qc_file(f) {
                                    Ok(r) => { let _ = tx.send(format!("{}: {} sorun", r.0, r.3.len() + r.2.len())); res.push(r); }
                                    Err(e) => { let _ = tx.send(format!("HATA {}: {e}", f.display())); }
                                }
                                c2.request_repaint();
                            }
                            let title = files.first().and_then(|f| f.parent()).and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            let msg = match std::fs::write(&out, osisub_core::qc::report_html_multi(&title, &res)) { Ok(()) => format!("Rapor kaydedildi: {}", out.display()), Err(e) => format!("Rapor yazılamadı: {e}") };
                            let _ = tx.send(msg);
                            let _ = tx.send(String::new());
                            c2.request_repaint();
                        });
                        w.rx = Some(rx);
                    }
                }
                if running { ui.spinner(); }
            });
            egui::ScrollArea::vertical().id_salt("fb_log").max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
                ui.label(RichText::new(&w.log).monospace().small());
            });
        });
        if start {
            let mut job = w.job.clone();
            job.fixes = w.fixes.then(osisub_core::fixes::Fixes::default);
            job.resample = w.resample.then_some(w.res);
            let files = w.files.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            w.rx = Some(rx);
            w.log.clear();
            std::thread::spawn(move || {
                let (mut ok, mut bad) = (0, 0);
                for f in files {
                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let m = match osisub_core::filebatch::run_file(&f, &job) {
                        Ok((out, note)) => { ok += 1; format!("TAMAM {name} → {}{}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), if note.is_empty() { String::new() } else { format!(" ({note})") }) }
                        Err(e) => { bad += 1; format!("HATA  {name}: {e}") }
                    };
                    let _ = tx.send(m);
                }
                let _ = tx.send(format!("Bitti: {ok} başarılı, {bad} hatalı"));
                let _ = tx.send(String::new());
            });
        }
        if open { self.filebatch = Some(w); }
    }

    /// Kanji zamanlayıcı (Aegisub Kanji Timer): kaynak stildeki romaji `\k` satırının zamanlamasını
    /// aynı zamandaki hedef stil satırına (kanji/kana) aktarır; gruplar otomatik eşlenir, elle düzeltilebilir.
    pub(super) fn kanji_window(&mut self, ctx: &egui::Context) {
        use osisub_core::{kanjitimer as kt, karaoke};
        let Some(mut w) = self.kanji.take() else { return };
        let styles: Vec<String> = self.doc.script.styles().map(|s| s.name.clone()).collect();
        if w.src_style.is_empty() || w.dst_style.is_empty() {
            // varsayılan: etkin satırın stili kaynak, ondan farklı ilk stil hedef
            let act = match self.doc.script.entries.get(self.doc.active) { Some(Entry::Dialogue(d)) => d.style.clone(), _ => String::new() };
            if w.src_style.is_empty() { w.src_style = if act.is_empty() { styles.first().cloned().unwrap_or_default() } else { act }; }
            if w.dst_style.is_empty() { w.dst_style = styles.iter().find(|s| **s != w.src_style).cloned().unwrap_or_default(); }
        }
        let ents = &self.doc.script.entries;
        let srcs: Vec<usize> = ents.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(d) if d.style == w.src_style && !karaoke::parse(&d.text).iter().all(|s| s.dur == 0))).map(|(i, _)| i).collect();
        if w.cur.map_or(true, |c| !srcs.contains(&c)) {
            w.cur = srcs.iter().copied().find(|&i| i >= self.doc.active).or(srcs.first().copied());
        }
        let pair = w.cur.and_then(|si| {
            let Entry::Dialogue(sd) = &ents[si] else { return None };
            ents.iter().enumerate().filter_map(|(i, e)| match e { Entry::Dialogue(d) if d.style == w.dst_style && i != si => Some((i, (d.start_time - sd.start_time).abs() + (d.end_time - sd.end_time).abs())), _ => None }).filter(|(_, diff)| *diff <= 200).min_by_key(|x| x.1).map(|(di, _)| (si, di))
        });
        let (src_syls, dst_text) = match pair {
            Some((si, di)) => match (&ents[si], &ents[di]) { (Entry::Dialogue(a), Entry::Dialogue(b)) => (karaoke::parse(&a.text), b.text.clone()), _ => (vec![], String::new()) },
            None => (vec![], String::new()),
        };
        // yeni çift: otomatik hizala
        if pair.is_some() && w.loaded != pair {
            let (c, d) = kt::auto_align(&src_syls, &dst_text);
            w.src_str = kt::src_string(&src_syls, &c);
            w.dst_str = d.join("|");
            w.loaded = pair;
        }
        let mut open = true;
        let mut act: i8 = 0; // 1 uygula+sonraki, 2 atla, -1 önceki
        egui::Window::new(tr("Kanji zamanlayıcı")).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Kaynak (romaji, \\k'li) satırın hece süreleri, aynı zamandaki hedef satırın (kanji/kana) gruplarına aktarılır. Gruplar | ile, bir gruptaki kaynak heceler + ile ayrılır.")).small().weak());
            ui.horizontal(|ui| {
                ui.label(tr("Kaynak stil"));
                egui::ComboBox::from_id_salt("kt_src").selected_text(&w.src_style).show_ui(ui, |ui| { for s in &styles { ui.selectable_value(&mut w.src_style, s.clone(), s); } });
                ui.label(tr("Hedef stil"));
                egui::ComboBox::from_id_salt("kt_dst").selected_text(&w.dst_style).show_ui(ui, |ui| { for s in &styles { ui.selectable_value(&mut w.dst_style, s.clone(), s); } });
            });
            let pos = w.cur.and_then(|c| srcs.iter().position(|&i| i == c)).map(|p| p + 1).unwrap_or(0);
            ui.label(trf!("Kaynak satır {pos} / {}", srcs.len()));
            if srcs.is_empty() { ui.label(RichText::new(tr("Kaynak stilde \\k'li satır yok")).color(theme::BAD)); return; }
            if pair.is_none() { ui.label(RichText::new(tr("Bu satırla aynı zamanda hedef stilde satır yok (±200 ms)")).color(theme::BAD)); }
            ui.label(RichText::new(src_syls.iter().map(|s| format!("{}({})", s.visible().trim(), s.dur)).collect::<Vec<_>>().join(" ")).monospace().small());
            egui::Grid::new("kt_g").num_columns(2).show(ui, |ui| {
                ui.label(tr("Kaynak gruplar"));
                ui.add(egui::TextEdit::singleline(&mut w.src_str).font(egui::TextStyle::Monospace).desired_width(420.0));
                ui.end_row();
                ui.label(tr("Hedef gruplar"));
                ui.add(egui::TextEdit::singleline(&mut w.dst_str).font(egui::TextStyle::Monospace).desired_width(420.0));
                ui.end_row();
            });
            let counts = kt::parse_src_counts(&w.src_str);
            let groups: Vec<String> = w.dst_str.split('|').map(String::from).collect();
            let builder = if w.furigana { kt::build_with_furigana } else { kt::build };
            let res = builder(&src_syls, &counts, &groups);
            match &res {
                Ok(t) => { ui.label(RichText::new(trf!("Sonuç: {t}")).monospace().color(theme::ACCENT)); }
                Err(e) => { ui.label(RichText::new(e).color(theme::BAD)); }
            }
            ui.horizontal(|ui| {
                if ui.button(tr("Otomatik")).clicked() { w.loaded = None; }
                ui.checkbox(&mut w.furigana, tr("Furigana okunuşu ekle")).on_hover_text(tr("Kanji gruplarına okunuşu romaji hecelerinden hiragana olarak ekler (君|きみ); sonra Araçlar → Furigana düzeni ile üste yerleştirin"));
                if ui.add_enabled(res.is_ok() && pair.is_some(), egui::Button::new(tr("Uygula ve sonraki"))).clicked() { act = 1; }
                if ui.button(tr("Atla")).clicked() { act = 2; }
                if ui.button(tr("Önceki")).clicked() { act = -1; }
            });
        });
        if act == 1 {
            if let Some((_, di)) = pair {
                let counts = kt::parse_src_counts(&w.src_str);
                let groups: Vec<String> = w.dst_str.split('|').map(String::from).collect();
                let builder = if w.furigana { kt::build_with_furigana } else { kt::build };
                if let Ok(t) = builder(&src_syls, &counts, &groups) {
                    // hedefin baştaki (karaoke dışı) etiketleri korunur
                    let lead: String = match osisub_core::overrides::parse(&dst_text).first() {
                        Some(osisub_core::overrides::Seg::Block(b)) => {
                            let keep: Vec<_> = b.iter().filter(|t| !matches!(t.name.as_str(), "k" | "kf" | "ko" | "K")).cloned().collect();
                            if keep.is_empty() { String::new() } else { format!("{{{}}}", osisub_core::overrides::block_to_string(&keep)) }
                        }
                        _ => String::new(),
                    };
                    let nt = if !lead.is_empty() && t.starts_with('{') { format!("{}{}", &lead[..lead.len() - 1], &t[1..]) } else { format!("{lead}{t}") };
                    self.doc.edit("Kanji zamanlayıcı", |tx| tx.dialogue_mut(di, |d| d.text = nt));
                }
            }
        }
        if act != 0 {
            if let Some(p) = w.cur.and_then(|c| srcs.iter().position(|&i| i == c)) {
                let np = if act < 0 { p.saturating_sub(1) } else { (p + 1).min(srcs.len() - 1) };
                w.cur = Some(srcs[np]);
                self.doc.select(vec![srcs[np]], srcs[np]);
                self.scroll_to = Some(srcs[np]);
            }
        }
        if open { self.kanji = Some(w); }
    }

    pub(crate) fn open_paste_over(&mut self, text: Option<String>) {
        let fields = self.paste_over.as_ref().map(|p| p.1).unwrap_or([false, true, true, false, false, false, false, false, false, false]);
        self.paste_over = Some((text.unwrap_or_else(|| self.last_copy.clone()), fields));
    }

    pub(super) fn paste_over_window(&mut self, ctx: &egui::Context) {
        let Some((mut text, mut f)) = self.paste_over.take() else { return };
        let mut open = true;
        let mut apply = false;
        egui::Window::new(tr("Üzerine yapıştır")).open(&mut open).collapsible(false).default_width(520.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Panodaki satırların işaretli alanları seçili satırlara sırayla yazılır (Aegisub: Paste Over). Kutuya Ctrl+V ile yapıştırabilirsiniz.")).small().weak());
            let names = ["Katman", "Başlangıç", "Bitiş", "Stil", "Konuşan", "Sol kenar", "Sağ kenar", "Dikey kenar", "Efekt", "Metin"];
            ui.horizontal_wrapped(|ui| {
                for (i, n) in names.iter().enumerate() { ui.checkbox(&mut f[i], *n); }
            });
            ui.horizontal(|ui| {
                if ui.small_button(tr("Hepsi")).clicked() { f = [true; 10]; }
                if ui.small_button(tr("Hiçbiri")).clicked() { f = [false; 10]; }
                if ui.small_button(tr("Zamanlar")).clicked() { f = [false, true, true, false, false, false, false, false, false, false]; }
                if ui.small_button(tr("Metin")).clicked() { f = [false; 10]; f[9] = true; }
            });
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_rows(6).desired_width(f32::INFINITY).hint_text(tr("Dialogue: 0,0:00:01.00,…")));
            });
            let src = text.lines().filter(|l| l.trim_start().starts_with("Dialogue:") || l.trim_start().starts_with("Comment:")).count();
            let n = self.doc.target_lines().len();
            ui.label(trf!("Panoda {src} satır · seçili {n} satır{}", if src != n && src > 0 { " (sayılar farklı: kısa olana kadar uygulanır)" } else { "" }));
            if ui.add_enabled(src > 0 && f.iter().any(|x| *x), egui::Button::new(tr("Uygula"))).clicked() { apply = true; }
        });
        if apply {
            let n = self.doc.paste_over(&text, &f);
            self.status = trf!("Üzerine yapıştırıldı: {n} satır değişti");
            open = false;
        }
        if open { self.paste_over = Some((text, f)); }
    }

    /// Etiket uygula (HYDRA) ve yeniden hesapla (Recalculator) penceresi; seçili satırlarda tek geri-al adımı.
    pub(super) fn tag_window(&mut self, ctx: &egui::Context) {
        use osisub_core::tagapply::{self, Mode, Op};
        let Some(mut w) = self.tagw.take() else { return };
        let mut open = true;
        let mut action: Option<u8> = None;
        let grad = w.mode >= 3;
        egui::Window::new(tr("Etiket uygula / yeniden hesapla")).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut w.mode, 0, tr("Başlangıç bloğu"));
                ui.radio_value(&mut w.mode, 1, tr("Tüm bloklar"));
                ui.radio_value(&mut w.mode, 2, tr("\\t dönüşümü"));
                ui.radio_value(&mut w.mode, 3, tr("Harf degradesi"));
                ui.radio_value(&mut w.mode, 4, tr("Satır degradesi"));
            });
            if w.mode == 2 {
                ui.horizontal(|ui| {
                    ui.label(tr("t1"));
                    ui.add(egui::DragValue::new(&mut w.t1).speed(10).suffix(" ms"));
                    ui.label(tr("t2"));
                    ui.add(egui::DragValue::new(&mut w.t2).speed(10).suffix(" ms"));
                    ui.label(tr("ivme"));
                    ui.add(egui::DragValue::new(&mut w.accel).range(0.01..=20.0).speed(0.05));
                    if ui.small_button(tr("tüm satır")).on_hover_text(tr("t1 = t2 = 0: satır boyunca")).clicked() { w.t1 = 0; w.t2 = 0; }
                });
            } else if grad {
                ui.horizontal(|ui| {
                    ui.label(tr("ivme"));
                    ui.add(egui::DragValue::new(&mut w.accel).range(0.01..=20.0).speed(0.05));
                    ui.label(RichText::new(if w.mode == 3 { "ilk harf = başlangıç, son harf = bitiş (boşluk ve \\N sayılmaz)" } else { "ilk seçili satır = başlangıç, son = bitiş (kare kare tipografi)" }).small().weak());
                });
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                egui::Grid::new("tagrows").num_columns(if grad { 3 } else { 2 }).striped(true).show(ui, |ui| {
                    for (n, on, a, b) in w.rows.iter_mut() {
                        ui.checkbox(on, format!("\\{n}"));
                        let color = n.ends_with('c');
                        for v in if grad { vec![a, b] } else { vec![a] } {
                            ui.horizontal(|ui| {
                                if color {
                                    if let Some(c) = osisub_core::color::Color::parse(v) {
                                        let mut rgb = [c.r, c.g, c.b];
                                        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                                            *v = osisub_core::color::Color { r: rgb[0], g: rgb[1], b: rgb[2], a: 0 }.to_tag_string();
                                            *on = true;
                                        }
                                    }
                                }
                                if ui.add(egui::TextEdit::singleline(v).font(egui::TextStyle::Monospace).desired_width(110.0)).changed() { *on = true; }
                            });
                        }
                        ui.end_row();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(tr("Ek etiketler"));
                ui.add(egui::TextEdit::singleline(&mut w.extra).font(egui::TextStyle::Monospace).desired_width(200.0).hint_text(tr("\\xbord2\\clip(…)")));
                if grad {
                    ui.label("→");
                    ui.add(egui::TextEdit::singleline(&mut w.extra_to).font(egui::TextStyle::Monospace).desired_width(160.0));
                }
            });
            let (a, _) = w.tags();
            ui.label(RichText::new(if a.is_empty() { "Etiket seçilmedi".to_string() } else { a }).monospace().small().color(theme::ACCENT));
            ui.horizontal(|ui| {
                if ui.button(trf!("{} satıra uygula", self.doc.target_lines().len())).clicked() { action = Some(1); }
                if ui.button(tr("Sıfırla")).clicked() { for r in w.rows.iter_mut() { r.1 = false; } w.extra.clear(); w.extra_to.clear(); }
            });
            ui.separator();
            ui.heading(tr("Yeniden hesapla (Recalculator)"));
            ui.horizontal(|ui| {
                ui.label(tr("etiket \\"));
                ui.add(egui::TextEdit::singleline(&mut w.rc_tag).desired_width(60.0));
                ui.radio_value(&mut w.rc_mul, true, "×");
                ui.radio_value(&mut w.rc_mul, false, "+");
                let xy = matches!(w.rc_tag.trim(), "pos" | "org" | "move");
                ui.label(if xy { "x" } else { "değer" });
                ui.add(egui::DragValue::new(&mut w.rc_x).speed(0.05));
                if xy {
                    ui.label(tr("y"));
                    ui.add(egui::DragValue::new(&mut w.rc_y).speed(0.05));
                }
                ui.checkbox(&mut w.rc_style, tr("yoksa stilden")).on_hover_text(tr("Satırda etiket yoksa stilin değerinden hesaplayıp başlangıca yaz"));
                if ui.button(tr("Uygula")).clicked() { action = Some(2); }
            });
            ui.label(RichText::new(tr("Başlangıç ve satır içi bloklar ile \\t içleri değişir; \\pos/\\move/\\org için x ve y ayrı.")).small().weak());
        });
        if let Some(a) = action {
            let targets: Vec<usize> = self.doc.target_lines().into_iter().filter(|&i| matches!(self.doc.script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
            let texts: Vec<(usize, String, String)> = targets.iter().filter_map(|&i| match &self.doc.script.entries[i] { Entry::Dialogue(d) => Some((i, d.text.clone(), d.style.clone())), _ => None }).collect();
            let new: Vec<String> = if a == 1 {
                let (from, to) = w.tags();
                match w.mode {
                    4 => tagapply::by_line(&texts.iter().map(|t| t.1.clone()).collect::<Vec<_>>(), &from, &to, w.accel),
                    m => {
                        let mode = match m { 0 => Mode::Start, 1 => Mode::AllBlocks, 2 => Mode::Transform { t1: w.t1, t2: w.t2, accel: w.accel }, _ => Mode::ByChar { to: to.clone(), accel: w.accel } };
                        texts.iter().map(|t| tagapply::apply(&t.1, &from, &mode)).collect()
                    }
                }
            } else {
                let name = w.rc_tag.trim().trim_start_matches('\\').to_string();
                let (ox, oy) = if w.rc_mul { (Op::Mul(w.rc_x), Op::Mul(w.rc_y)) } else { (Op::Add(w.rc_x), Op::Add(w.rc_y)) };
                texts.iter().map(|t| {
                    let st = self.doc.script.style_by_name(&t.2).cloned().unwrap_or_default();
                    let def = w.rc_style.then(|| match name.as_str() { "bord" => Some(st.outline), "shad" => Some(st.shadow), "fscx" => Some(st.scale_x), "fscy" => Some(st.scale_y), "fs" => Some(st.fontsize), "fsp" => Some(st.spacing), "frz" | "fr" => Some(st.angle), _ => None }).flatten();
                    tagapply::recalc(&t.1, &name, ox, oy, def).0
                }).collect()
            };
            let changes: Vec<(usize, String)> = texts.iter().zip(new).filter(|(t, n)| t.1 != *n).map(|(t, n)| (t.0, n)).collect();
            let n = changes.len();
            if n > 0 {
                self.doc.edit(if a == 1 { "Etiket uygula" } else { "Yeniden hesapla" }, |tx| for (i, t) in changes { tx.dialogue_mut(i, |d| d.text = t); });
            }
            self.status = trf!("{n} satır değişti");
        }
        if open { self.tagw = Some(w); }
    }

    pub(super) fn typeset_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.tsw.take() else { return };
        let mut open = true;
        let mut action: Option<u8> = None;
        egui::Window::new(tr("Degrade / parıltı")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(tr("Seçili satırlara uygulanır. Degrade için satırda dikdörtgen \\clip olmalı (Video aracı: Kırpma).")).small().weak());
            ui.heading(tr("Degrade"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut w.vertical, false, tr("Soldan sağa"));
                ui.radio_value(&mut w.vertical, true, tr("Yukarıdan aşağıya"));
                ui.label(tr("şerit"));
                ui.add(egui::DragValue::new(&mut w.strip).range(1.0..=200.0).suffix(" px"));
                ui.label(tr("ivme"));
                ui.add(egui::DragValue::new(&mut w.accel).range(0.1..=10.0).speed(0.05));
            });
            egui::Grid::new("grad").num_columns(2).show(ui, |ui| {
                ui.label(tr("Başlangıç"));
                ui.add(egui::TextEdit::singleline(&mut w.from).font(egui::TextStyle::Monospace).desired_width(320.0));
                ui.end_row();
                ui.label(tr("Bitiş"));
                ui.add(egui::TextEdit::singleline(&mut w.to).font(egui::TextStyle::Monospace).desired_width(320.0));
                ui.end_row();
            });
            ui.label(RichText::new(tr("Ortak etiketler enterpole edilir: \\c \\1c-4c, \\alpha \\1a-4a, \\bord \\shad \\blur \\fscx \\frz …")).small().weak());
            if ui.button(tr("Degrade uygula")).clicked() { action = Some(1); }
            ui.separator();
            ui.heading(tr("Parıltı (glow)"));
            ui.horizontal(|ui| {
                ui.label(tr("bulanıklık"));
                ui.add(egui::DragValue::new(&mut w.glow_blur).range(0.0..=50.0).speed(0.2));
                ui.label(tr("kenar rengi"));
                ui.add(egui::TextEdit::singleline(&mut w.glow_color).font(egui::TextStyle::Monospace).desired_width(100.0));
                ui.label(tr("saydamlık"));
                ui.add(egui::DragValue::new(&mut w.glow_alpha).range(0..=255).hexadecimal(2, false, true));
            });
            if ui.button(tr("Parıltı katmanı ekle")).on_hover_text(tr("Her satırın altına bulanık bir kopya ekler (asıl satır bir üst katmana geçer)")).clicked() { action = Some(2); }
        });
        if let Some(a) = action {
            let targets: Vec<usize> = self.doc.target_lines().into_iter().filter(|&i| matches!(self.doc.script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
            let mut s = self.doc.script.clone();
            let mut err = None;
            let mut made = 0;
            for &i in targets.iter().rev() {
                let Entry::Dialogue(d) = &s.entries[i] else { continue };
                let new: Result<Vec<osisub_core::ass::Dialogue>, String> = if a == 1 {
                    let o = osisub_core::typeset::GradientOptions { direction: if w.vertical { osisub_core::typeset::Direction::Vertical } else { osisub_core::typeset::Direction::Horizontal }, strip: w.strip, from: w.from.clone(), to: w.to.clone(), accel: w.accel };
                    osisub_core::typeset::gradient(d, None, &o)
                } else {
                    Ok(osisub_core::typeset::glow(d, w.glow_blur, Some(&w.glow_color), w.glow_alpha))
                };
                match new {
                    Ok(v) => { made += v.len(); s.entries.splice(i..=i, v.into_iter().map(Entry::Dialogue)); }
                    Err(e) => { err = Some(e); break; }
                }
            }
            match err {
                Some(e) => self.status = e,
                None if !targets.is_empty() => {
                    let first = targets[0];
                    self.doc.apply_new_script(if a == 1 { "Degrade" } else { "Parıltı" }, s);
                    self.doc.select((first..first + made).collect(), first);
                    self.status = trf!("{} satır → {} satır", targets.len(), made);
                }
                None => {}
            }
        }
        if open { self.tsw = Some(w); }
    }

    pub(super) fn tracks_window(&mut self, ctx: &egui::Context) {
        let Some((video, tracks)) = self.tracks_win.take() else { return };
        let mut open = true;
        // (iz, işlem): 0 = aç, 1 = kaydet, 2 = referans yap, 3 = OCR
        let mut action: Option<(usize, u8)> = None;
        let mut fonts = false;
        egui::Window::new(tr("Videodaki altyazı izleri")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new(video.display().to_string()).small().weak());
            if tracks.is_empty() { ui.label(tr("Altyazı izi yok.")); }
            for (k, t) in tracks.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(t.label());
                    if t.text_ext().is_some() {
                        if ui.button(tr("Aç")).clicked() { action = Some((k, 0)); }
                        if ui.button(tr("Referans yap")).on_hover_text(tr("İzi çıkarıp referans altyazı olarak yükler (açık belge değişmez)")).clicked() { action = Some((k, 2)); }
                        if ui.button(tr("Kaydet…")).clicked() { action = Some((k, 1)); }
                    } else if osisub_media::mux::is_bitmap_sub(&t.codec) {
                        if ui.button(tr("OCR…")).on_hover_text(tr("Resim altyazısını (Blu-ray PGS / DVD VobSub) Tesseract ile metne çevir")).clicked() { action = Some((k, 3)); }
                    } else {
                        ui.label(RichText::new(tr("resim tabanlı (desteklenmiyor)")).weak());
                    }
                });
            }
            ui.separator();
            if ui.button(tr("Ekli yazı tiplerini klasöre çıkar…")).clicked() { fonts = true; }
        });
        if fonts {
            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                match osisub_media::mux::extract_attachments(&video, &d) {
                    Ok(f) => self.status = trf!("{} ek çıkarıldı → {}", f.len(), d.display()),
                    Err(e) => self.status = e,
                }
            }
        }
        if let Some((k, act)) = action {
            let t = &tracks[k];
            if act == 3 {
                self.ocr = Some(OcrWin::for_track(video.clone(), &tracks, t.index));
                return;
            }
            let ext = t.text_ext().unwrap_or("ass");
            let stem = video.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let out = if act != 1 {
                Some(osisub_core::paths::data_dir().unwrap_or_else(std::env::temp_dir).join("extracted").join(format!("{stem}.{}.{ext}", t.index)))
            } else {
                rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(format!("{stem}.{}.{ext}", t.index)).save_file()
            };
            if let Some(out) = out {
                if let Some(d) = out.parent() { let _ = std::fs::create_dir_all(d); }
                match osisub_media::mux::extract_subtitle(&video, t, &out) {
                    Ok(()) => {
                        self.status = trf!("Çıkarıldı: {}", out.display());
                        if act == 0 { self.request(super::Guarded::OpenPath(out)); return; }
                        if act == 2 {
                            match osisub_core::io::load(&out) {
                                Ok(sc) => { let n = sc.dialogues().count(); self.set_reference(sc); self.status = trf!("Referans: {} ({n} satır)", t.label()); return; }
                                Err(e) => self.status = e,
                            }
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if open { self.tracks_win = Some((video, tracks)); }
    }

    fn palette_items(&self) -> Vec<(String, super::PalAct)> {
        use super::PalAct::*;
        let mut v: Vec<(String, super::PalAct)> = super::COMMANDS.iter().map(|(id, l)| (tr(l).to_string(), Cmd(id))).collect();
        for (w, l) in [
            ("find", "Bul / Değiştir"), ("goto", "Zamana git"), ("timing", "Kaydır / kareye oturt"), ("post", "Son-işleyici"), ("resample", "Çözünürlüğü yeniden örnekle"), ("dummy", "Sahte video aç"),
            ("fixes", "Sık hataları düzelt"), ("stats", "İstatistikler"), ("vad", "Sesten satır oluştur / konuşmaya yapıştır"), ("export", "Dışa aktar (filtreler)"), ("fonts", "Yazı tipi denetimi"),
            ("hotkeys", "Kısayolları düzenle"), ("batch", "Toplu satır işlemleri"), ("typeset", "Degrade / parıltı"), ("motion", "Hareket izleme verisi uygula"), ("tags", "Etiket uygula / yeniden hesapla"),
            ("todraw", "Metni çizime dönüştür"), ("tracks", "Videodaki altyazı izleri"), ("mux", "MKV'ye göm (softsub)"), ("burn", "Videoya yak (hardsub)"),
            ("merge", "Başka dosyayı birleştir"), ("mt", "Makine çevirisi"), ("resync", "Sesle yeniden zamanla (TV → BD)"), ("history", "Geçmiş (geri alma listesi)"), ("open", "Aç"), ("save", "Kaydet"), ("lua", "Lua betiği yükle"), ("reload", "Betikleri yeniden yükle"),
            ("ocr", "Resim altyazısını metne çevir (PGS / VobSub OCR)"), ("whisper", "Sesten altyazı (Whisper)"), ("collide", "Ekran çakışmasını denetle"), ("song", "OP/ED yerleştir (önceki bölümden)"), ("honor", "Saygı ekleri (-san/-kun)"),
            ("paste_over", "Üzerine yapıştır"), ("style_assist", "Stil yardımcısı"), ("kanji_timer", "Kanji zamanlayıcı"), ("furigana", "Furigana düzeni"), ("split_file", "Dosyayı böl"), ("hardsub", "Gömülü (hardsub) altyazıyı metne çevir"), ("text_import", "Düz metin içe aktar"), ("multi_replace", "Çoklu değiştir"), ("bookmarks", "Yer imleri"), ("file_batch", "Toplu dosya işlemleri"),
            ("quality", "Kalite sekmesi"), ("styles", "Stiller sekmesi"), ("karaoke", "Karaoke sekmesi"), ("tagstab", "Etiketler sekmesi"), ("translate", "Çeviri sekmesi"), ("spell", "Yazım sekmesi"),
        ] {
            v.push((tr(l).to_string(), Win(w)));
        }
        for m in self.macros.iter().filter(|m| m.info.kind == MacroKind::Macro && m.script != super::CONSOLE_NAME) {
            v.push((format!("Makro: {}", m.info.name), Macro(m.info.id)));
        }
        v
    }

    fn run_palette(&mut self, a: super::PalAct, ctx: &egui::Context) {
        use super::PalAct::*;
        match a {
            Cmd(id) => self.run_command(id, ctx),
            Macro(id) => self.run_macro(id),
            Win(w) => match w {
                "find" => { self.find_open = true; self.find_focus = true; }
                "goto" => { self.goto_open = true; self.goto_focus = true; }
                "timing" => self.timing_open = true,
                "post" => self.timing_open = true,
                "resample" => self.resample_open = true,
                "dummy" => self.open_dummy_window(),
                "fixes" => self.fixes_open = true,
                "stats" => self.stats_open = true,
                "vad" => self.vad_open = true,
                "export" => self.export_open = true,
                "fonts" => { self.fonts_open = true; self.start_font_scan(); }
                "hotkeys" => self.hotkeys_open = true,
                "batch" => self.batch = Some(super::BatchWin::default()),
                "typeset" => self.tsw = Some(super::TypesetWin::default()),
                "motion" => self.open_motion(),
                "tags" => self.tagw = Some(super::TagWin::default()),
                "paste_over" => self.open_paste_over(None),
                "style_assist" => self.style_assist = Some(String::new()),
                "kanji_timer" => self.kanji = Some(super::KanjiWin::default()),
                "file_batch" => self.filebatch = Some(super::FileBatchWin::default()),
                "todraw" => self.text_to_drawing(false),
                "furigana" => self.furigana_layout(),
                "bookmarks" => self.bookmarks_open = true,
                "multi_replace" => self.open_mrep(),
                "text_import" => self.text_import = Some(super::windows::TextImportWin { speakers: true, ..Default::default() }),
                "hardsub" => self.hardsub = Some(super::windows::HardsubWin::new(self.media.path.clone())),
                "split_file" => self.split_win = Some((String::new(), 2, false)),
                "tracks" => {
                    let v = self.media.path.clone().or_else(|| rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file());
                    if let Some(v) = v { match osisub_media::mux::subtitle_tracks(&v) { Ok(t) => self.tracks_win = Some((v, t)), Err(e) => self.status = e } }
                }
                "ocr" => self.ocr = Some(super::windows::OcrWin::default()),
                "whisper" => self.whisper = Some(super::windows::WhisperWin::default()),
                "honor" => self.honor_open = true,
                "song" => self.song_win = Some(super::windows::SongWin::new()),
                "collide" => { self.tab = super::Tab::Quality; self.start_collide_check(ctx); }
                "mux" => self.start_video_job(false),
                "burn" => self.start_video_job(true),
                "merge" => {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        self.merge_win = Some((p, osisub_core::merge::MergeOptions { sort: true, ..Default::default() }, "0:00:00.00".into()));
                    }
                }
                "open" => { if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() { self.request(super::Guarded::OpenPath(p)); } }
                "save" => self.save(false),
                "lua" => { if let Some(p) = rfd::FileDialog::new().add_filter("Lua / MoonScript", &["lua", "moon"]).pick_file() { self.load_lua(p); } }
                "reload" => self.reload_scripts(),
                "history" => self.history_open = true,
                "mt" => self.mt_open = true,
                "resync" => self.start_resync(),
                "quality" => self.tab = super::Tab::Quality,
                "styles" => self.tab = super::Tab::Styles,
                "karaoke" => self.tab = super::Tab::Karaoke,
                "tagstab" => self.tab = super::Tab::Tags,
                "translate" => self.tab = super::Tab::Translate,
                "spell" => self.tab = super::Tab::Spell,
                _ => {}
            },
        }
    }

    pub(super) fn palette_window(&mut self, ctx: &egui::Context) {
        let Some((mut q, mut sel)) = self.palette.take() else { return };
        let mut items: Vec<(usize, String, super::PalAct)> = self.palette_items().into_iter().filter_map(|(l, a)| super::palette_score(&l, &q).map(|s| (s, l, a))).collect();
        items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let (up, down, enter, esc) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if down { sel = (sel + 1).min(items.len().saturating_sub(1)); }
        if up { sel = sel.saturating_sub(1); }
        let mut chosen: Option<super::PalAct> = None;
        egui::Window::new(tr("Komut paleti")).title_bar(false).collapsible(false).resizable(false).fixed_size([480.0, 360.0]).anchor(egui::Align2::CENTER_TOP, [0.0, 60.0]).show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Komut, pencere ya da makro ara… (↑↓, Enter, Esc)")).desired_width(f32::INFINITY));
            r.request_focus();
            if r.changed() { sel = 0; }
            ui.separator();
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (k, (_, l, a)) in items.iter().enumerate().take(60) {
                    let resp = ui.selectable_label(k == sel, l);
                    if k == sel && (up || down) { resp.scroll_to_me(None); }
                    if resp.clicked() { chosen = Some(a.clone()); }
                }
                if items.is_empty() { ui.label(RichText::new(tr("Eşleşme yok")).weak()); }
            });
        });
        if enter { chosen = items.get(sel).map(|x| x.2.clone()); }
        if let Some(a) = chosen {
            self.run_palette(a, ctx);
            return;
        }
        if !esc { self.palette = Some((q, sel)); }
    }

    pub(super) fn history_window(&mut self, ctx: &egui::Context) {
        if !self.history_open { return; }
        let mut open = true;
        let mut jump: Option<usize> = None;
        let (undo, redo): (Vec<String>, Vec<String>) = {
            let (u, r) = self.doc.history();
            (u.iter().map(|x| x.to_string()).collect(), r.iter().map(|x| x.to_string()).collect())
        };
        egui::Window::new(tr("Geçmiş")).open(&mut open).default_width(300.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                if ui.selectable_label(undo.is_empty(), "• Açılış durumu").clicked() { jump = Some(0); }
                for (k, d) in undo.iter().enumerate() {
                    let cur = k + 1 == undo.len();
                    let r = ui.selectable_label(cur, format!("{}. {}", k + 1, d));
                    if cur { r.scroll_to_me(None); }
                    if r.clicked() { jump = Some(k + 1); }
                }
                for (k, d) in redo.iter().enumerate() {
                    if ui.selectable_label(false, RichText::new(format!("{}. {}", undo.len() + k + 1, d)).weak().italics()).on_hover_text(tr("Geri alındı — tıklayınca yinelenir")).clicked() { jump = Some(undo.len() + k + 1); }
                }
            });
        });
        if let Some(n) = jump {
            self.doc.goto_history(n);
            self.status = trf!("Geçmiş: {} adım", n);
        }
        if !open { self.history_open = false; }
    }

    fn mt_config(&self) -> Option<osisub_media::mt::MtConfig> {
        let st = &self.settings;
        Some(osisub_media::mt::MtConfig {
            provider: osisub_media::mt::Provider::from_id(&st.mt_provider)?,
            key: st.mt_key.clone(),
            url: st.mt_url.clone(),
            target: st.mt_target.clone(),
            source: st.mt_source.clone(),
            context: st.mt_context.clone(),
        })
    }

    /// Seçili satırları arka planda çevirir. Kaynak: referans altyazı yüklüyse zamanca en çok örtüşen
    /// referans satırı, değilse satırın kendi metni.
    pub(super) fn start_mt(&mut self) {
        let Some(cfg) = self.mt_config() else { return };
        let targets: Vec<usize> = self.doc.target_lines().into_iter().filter(|&i| matches!(self.doc.script.entries.get(i), Some(Entry::Dialogue(d)) if !d.comment)).collect();
        let mut idx = vec![];
        let mut texts = vec![];
        for i in targets {
            let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(i) else { continue };
            let src = if self.ref_lines.is_empty() {
                d.text.clone()
            } else {
                self.ref_lines.iter().filter(|r| r.1 > d.start_time && r.0 < d.end_time).max_by_key(|r| r.1.min(d.end_time) - r.0.max(d.start_time)).map(|r| r.2.replace('\n', "\\N")).unwrap_or_default()
            };
            if src.trim().is_empty() { continue; }
            idx.push(i);
            texts.push(src);
        }
        if texts.is_empty() { self.status = tr("Çevrilecek metin yok (satır seçin; referans kullanıyorsanız eşleşen referans satırı olmalı)").into(); return; }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut out = vec![];
            for chunk in texts.chunks(40) {
                match osisub_media::mt::translate(&cfg, chunk) {
                    Ok(v) => out.extend(v),
                    Err(e) => { let _ = tx.send((idx, Err(e))); return; }
                }
            }
            let _ = tx.send((idx, Ok(out)));
        });
        self.mt_rx = Some(rx);
        self.status = tr("Makine çevirisi çalışıyor…").into();
    }

    pub(super) fn poll_mt(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.mt_rx else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let Ok((idx, r)) = rx.try_recv() else { return };
        self.mt_rx = None;
        match r {
            Ok(out) => {
                let mut s = self.doc.script.clone();
                for (&i, t) in idx.iter().zip(out.iter()) {
                    if let Some(Entry::Dialogue(d)) = s.entries.get_mut(i) { d.text = t.clone(); }
                }
                self.doc.apply_new_script("Makine çevirisi", s);
                self.status = trf!("{} satır çevrildi (gözden geçirin!)", out.len());
            }
            Err(e) => self.status = trf!("Makine çevirisi: {e}"),
        }
    }

    pub(super) fn mt_window(&mut self, ctx: &egui::Context) {
        if !self.mt_open { return; }
        let mut open = true;
        let mut go = false;
        let mut changed = false;
        egui::Window::new(tr("Makine çevirisi")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            let st = &mut self.settings;
            egui::Grid::new("mt_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("Sağlayıcı"));
                egui::ComboBox::from_id_salt("mt_prov").selected_text(osisub_media::mt::Provider::ALL.iter().find(|p| p.0 == st.mt_provider).map(|p| p.1).unwrap_or("—")).show_ui(ui, |ui| {
                    for (id, l) in osisub_media::mt::Provider::ALL { changed |= ui.selectable_value(&mut st.mt_provider, id.to_string(), *l).changed(); }
                });
                ui.end_row();
                ui.label(tr("API anahtarı"));
                changed |= ui.add(egui::TextEdit::singleline(&mut st.mt_key).password(true).desired_width(260.0)).changed();
                ui.end_row();
                if st.mt_provider == "libre" {
                    ui.label(tr("Sunucu"));
                    changed |= ui.add(egui::TextEdit::singleline(&mut st.mt_url).desired_width(260.0)).changed();
                    ui.end_row();
                }
                ui.label(tr("Kaynak / hedef dil"));
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut st.mt_source).hint_text(tr("otomatik")).desired_width(70.0)).changed();
                    ui.label("→");
                    changed |= ui.add(egui::TextEdit::singleline(&mut st.mt_target).desired_width(70.0)).changed();
                });
                ui.end_row();
                if st.mt_provider == "claude" {
                    ui.label(tr("Bağlam / sözlük"));
                    changed |= ui.add(egui::TextEdit::multiline(&mut st.mt_context).desired_rows(3).desired_width(260.0).hint_text(tr("Dizi adı, karakterler, terimler (senpai = senpai)…"))).changed();
                    ui.end_row();
                }
            });
            ui.label(RichText::new(tr("Anahtar settings.json'da düz metin saklanır. Kaynak: referans altyazı yüklüyse eşleşen referans satırı, değilse satırın metni. Etiketler korunur. Sonucu mutlaka gözden geçirin.")).small().weak());
            let n = self.doc.target_lines().len();
            if ui.add_enabled(self.mt_rx.is_none(), egui::Button::new(format!("Seçili {n} satırı çevir"))).clicked() { go = true; }
        });
        if changed { self.settings.save(); }
        if go { self.start_mt(); }
        if !open { self.mt_open = false; }
    }

    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_open;
        egui::Window::new(tr("Dışa aktar — filtreler")).open(&mut open).collapsible(false).show(ctx, |ui| {
            let mut filters: Vec<(usize, String, String, i64)> = self.macros.iter().filter(|m| m.info.kind == MacroKind::Filter).map(|m| (m.info.id, m.info.name.clone(), m.info.description.clone(), m.info.priority)).collect();
            filters.sort_by_key(|f| -f.3);
            if filters.is_empty() { ui.label(RichText::new(tr("Yüklü filtre yok (Lua: aegisub.register_filter)")).weak()); }
            for (id, name, desc, prio) in &filters {
                let mut on = self.export_sel.contains(id);
                if ui.checkbox(&mut on, format!("{}  (öncelik {})", name, prio)).on_hover_text(desc).changed() {
                    if on { self.export_sel.push(*id); } else { self.export_sel.retain(|x| x != id); }
                }
            }
            ui.separator();
            ui.label(RichText::new(tr("Yerleşik")).strong());
            let b = &mut self.export_builtins;
            ui.checkbox(&mut b.strip_comments, tr("Yorumları ve kullanılmayan stilleri sil")).on_hover_text(tr("Şablon/not yorumları ve hiçbir satırın (\\r dahil) kullanmadığı stiller çıkar (Aegisub: Strip comments and unused styles)"));
            ui.checkbox(&mut b.clean_info, tr("Script Info'yu temizle")).on_hover_text(tr("Yalnızca oynatıcının ihtiyaç duyduğu anahtarlar (PlayRes, LayoutRes, WrapStyle, ScaledBorderAndShadow, YCbCr Matrix…) kalır; proje çöpü (video yolu vb.) silinir (Aegisub: Clean Script Info)"));
            ui.checkbox(&mut b.fix_styles, tr("Tanımsız stilleri Default yap")).on_hover_text(tr("Aegisub: Fix styles"));
            ui.checkbox(&mut b.strip_notes, tr("Metindeki {notları} sil"));
            ui.checkbox(&mut b.clean_tags, tr("Gereksiz etiketleri sil"));
            ui.label(RichText::new(tr("Seçili Lua filtreleri öncelik sırasıyla (büyükten küçüğe), ardından yerleşikler çalışır; belge değişmez, sonuç dosyaya yazılır.")).weak().small());
            if ui.add_enabled(self.running.is_none(), egui::Button::new(tr("Dışa aktar…"))).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).add_filter("Düz metin", &["txt"]).save_file() {
                    let mut queue: Vec<usize> = filters.iter().filter(|f| self.export_sel.contains(&f.0)).map(|f| f.0).collect();
                    if queue.is_empty() {
                        let mut sc = self.doc.script.clone();
                        osisub_core::exportfilters::apply(&mut sc, &self.export_builtins);
                        match osisub_core::io::save(&sc, &p) { Ok(()) => self.status = trf!("Dışa aktarıldı: {}", p.display()), Err(e) => self.status = trf!("Hata: {}", e) }
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
        egui::Window::new(tr("Zamanlama")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.shift_use_frames, tr("kare"));
                if self.shift_use_frames {
                    ui.label(tr("Kaydır (kare)"));
                    ui.add(egui::DragValue::new(&mut self.shift_frames));
                    let fps = self.media.fps().unwrap_or(self.fps);
                    self.shift_ms = (self.shift_frames as f64 * 1000.0 / fps).round() as i64;
                    ui.label(RichText::new(format!("= {} ms", self.shift_ms)).weak());
                } else {
                    ui.label(tr("Kaydır (ms)"));
                    ui.add(egui::DragValue::new(&mut self.shift_ms).speed(10));
                }
                ui.checkbox(&mut self.shift_all, tr("tüm satırlar"));
                ui.add_enabled(!self.shift_all, egui::Checkbox::new(&mut self.shift_after, "seçimden sonrakiler de")).on_hover_text(tr("Seçili ilk satırın başlangıcından sonra başlayan tüm satırlar (ör. sahne kesildiğinde)"));
                if ui.button(tr("Uygula")).clicked() {
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
                    self.status = trf!("{} satır kaydırıldı", n);
                }
            });
            ui.separator();
            ui.strong(tr("Son-işleyici"));
            egui::Grid::new("post_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("Lead-in (ms)")); ui.add(egui::DragValue::new(&mut self.post.lead_in_ms).speed(5)); ui.end_row();
                ui.label(tr("Lead-out (ms)")); ui.add(egui::DragValue::new(&mut self.post.lead_out_ms).speed(5)); ui.end_row();
                ui.label(tr("Bitişik satır eşiği (ms)")); ui.add(egui::DragValue::new(&mut self.post.adjacent_gap_ms).speed(5)); ui.end_row();
                ui.label(tr("Boşluk eğilimi (0–1)")); ui.add(egui::DragValue::new(&mut self.post.adjacent_bias).speed(0.05).range(0.0..=1.0)); ui.end_row();
                ui.label(tr("Anahtar kare: başl. önce/sonra")); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.post.kf_start_before).speed(5)); ui.add(egui::DragValue::new(&mut self.post.kf_start_after).speed(5)); }); ui.end_row();
                ui.label(tr("Anahtar kare: bitiş önce/sonra")); ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.post.kf_end_before).speed(5)); ui.add(egui::DragValue::new(&mut self.post.kf_end_after).speed(5)); }); ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.post_selection_only, tr("Yalnızca seçim"));
                ui.label(RichText::new(trf!("{} anahtar kare", self.media.keyframes.len())).weak());
                if ui.button(tr("Uygula")).clicked() {
                    let mut o = self.post.clone();
                    o.keyframes = match self.media.framerate() { Some(fr) => fr.keyframe_boundaries(&self.media.keyframes), None => self.media.keyframes.clone() };
                    o.only = if self.post_selection_only { Some(self.doc.selection.clone()) } else { None };
                    let mut s = self.doc.script.clone();
                    timing::post_process(&mut s, &o);
                    self.doc.apply_new_script("Zamanlama son-işleyici", s);
                }
            });
            ui.separator();
            ui.strong(tr("Noktadan senkron (iki nokta, doğrusal)"));
            ui.label(RichText::new(tr("Etkin satırı A/B noktası yapın, oynatma imlecini (playhead) hedef zaman olarak alın; uygulayınca tüm zamanlar kayar ve ölçeklenir.")).weak().small());
            for (name, slot_is_a) in [("A", true), ("B", false)] {
                ui.horizontal(|ui| {
                    ui.label(trf!("{} noktası:", name));
                    if ui.button(tr("Etkin satır")).clicked() {
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
                            if ui.button(tr("Playhead")).clicked() { t = self.media.playhead; }
                            ui.add(egui::DragValue::new(&mut t).speed(10).suffix(" ms"));
                            *target = t;
                            ui.label(RichText::new(time::format_ass(t)).monospace());
                        }
                        None => { ui.label(RichText::new(tr("seçilmedi")).weak()); }
                    }
                });
            }
            if ui.add_enabled(self.sync_a.is_some() && self.sync_b.is_some(), egui::Button::new(tr("Senkronu uygula"))).clicked() {
                if let (Some((ia, ta)), Some((ib, tb))) = (self.sync_a, self.sync_b) {
                    let src = |i: usize| match self.doc.script.entries.get(i) { Some(Entry::Dialogue(d)) => Some(d.start_time), _ => None };
                    if let (Some(sa), Some(sb)) = (src(ia), src(ib)) {
                        if sa == sb { self.status = tr("A ve B farklı zamanlarda olmalı").into(); } else {
                            let mut sc = self.doc.script.clone();
                            qc::linear_retime(&mut sc, (sa, ta), (sb, tb));
                            self.doc.apply_new_script("Noktadan senkron", sc);
                            self.sync_a = None; self.sync_b = None;
                            self.status = tr("Senkron uygulandı").into();
                        }
                    }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("FPS dönüştür"));
                ui.add(egui::DragValue::new(&mut self.fps_from).speed(0.001).range(1.0..=240.0));
                ui.label("→");
                ui.add(egui::DragValue::new(&mut self.fps_to).speed(0.001).range(1.0..=240.0));
                if ui.button(tr("Uygula")).on_hover_text(tr("Zamanları from/to oranıyla ölçekler (ör. 25 → 23.976)")).clicked() {
                    let mut sc = self.doc.script.clone();
                    qc::fps_convert(&mut sc, self.fps_from, self.fps_to);
                    self.doc.apply_new_script("FPS dönüştür", sc);
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("FPS"));
                ui.add(egui::DragValue::new(&mut self.fps).speed(0.001).range(1.0..=240.0));
                if ui.button(tr("Kareye oturt")).clicked() {
                    let mut s = self.doc.script.clone();
                    match &self.media.timecodes {
                        Some(tc) => qc::snap_to_framerate(&mut s, tc),
                        None => qc::snap_to_frames(&mut s, self.fps),
                    }
                    self.doc.apply_new_script("Kareye oturt", s);
                }
                if self.media.timecodes.is_some() {
                    ui.label(RichText::new(tr("(VFR timecode kullanılıyor)")).weak());
                }
            });
        });
        self.timing_open = open;
    }

    pub(super) fn find_window(&mut self, ctx: &egui::Context) {
        let mut open = self.find_open;
        egui::Window::new(tr("Bul / Değiştir")).open(&mut open).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("find_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("Bul"));
                let r = ui.add(TextEdit::singleline(&mut self.find.pattern).desired_width(280.0));
                if self.find_focus { r.request_focus(); self.find_focus = false; }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) { self.find_next(); }
                ui.end_row();
                ui.label(tr("Değiştir"));
                ui.add(TextEdit::singleline(&mut self.find.replacement).desired_width(280.0));
                ui.end_row();
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.find.regex, tr("Düzenli ifade"));
                ui.checkbox(&mut self.find.match_case, tr("Büyük/küçük harf"));
                ui.checkbox(&mut self.find.whole_word, tr("Tam sözcük"));
                ui.checkbox(&mut self.find.ignore_tags, tr("Etiketleri yok say"));
                ui.checkbox(&mut self.find.skip_comments, tr("Yorumları atla"));
                ui.checkbox(&mut self.find_selection_only, tr("Yalnızca seçim"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Alanlar:"));
                for (f, l) in [(Field::Text, "Metin"), (Field::Actor, "Konuşan"), (Field::Effect, "Efekt"), (Field::Style, "Stil")] {
                    let mut on = self.find.fields.contains(&f);
                    if ui.checkbox(&mut on, l).changed() {
                        if on { self.find.fields.push(f); } else { self.find.fields.retain(|x| *x != f); }
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Sonrakini bul")).clicked() { self.find_next(); }
                if ui.button(tr("Tümünü seç")).clicked() { self.find_select_all(); }
                if ui.button(tr("Tümünü değiştir")).clicked() { self.find_replace_all(); }
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
                self.status = trf!("{} eşleşme", hits.len());
            }
            Ok(_) => self.status = tr("Eşleşme yok").into(),
            Err(e) => self.status = trf!("Arama hatası: {}", e),
        }
    }

    pub(super) fn find_select_all(&mut self) {
        let o = self.find_opts();
        match search::find_all(&self.doc.script, &o) {
            Ok(hits) => {
                let mut v: Vec<usize> = hits.iter().map(|h| h.entry).collect();
                v.dedup();
                self.status = trf!("{} satır seçildi", v.len());
                let a = v.first().copied().unwrap_or(self.doc.active);
                self.doc.select(v, a);
                self.scroll_to = Some(a);
            }
            Err(e) => self.status = trf!("Arama hatası: {}", e),
        }
    }

    pub(super) fn find_replace_all(&mut self) {
        let o = self.find_opts();
        let mut s = self.doc.script.clone();
        match search::replace_all(&mut s, &o) {
            Ok(n) => {
                self.doc.apply_new_script("Bul/Değiştir", s);
                self.status = trf!("{} alan değiştirildi", n);
            }
            Err(e) => self.status = trf!("Arama hatası: {}", e),
        }
    }

    pub(super) fn resample_window(&mut self, ctx: &egui::Context) {
        let mut open = self.resample_open;
        egui::Window::new(tr("Çözünürlüğü yeniden örnekle")).open(&mut open).collapsible(false).show(ctx, |ui| {
            let (ox, oy) = self.doc.script.res();
            ui.label(trf!("Mevcut: {}×{}", ox, oy));
            ui.horizontal(|ui| {
                ui.label(tr("Yeni"));
                ui.add(egui::DragValue::new(&mut self.res_x).range(16..=16384));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.res_y).range(16..=16384));
                for (x, y, l) in [(1280, 720, "720p"), (1920, 1080, "1080p"), (384, 288, "384×288")] {
                    if ui.small_button(l).clicked() { self.res_x = x; self.res_y = y; }
                }
            });
            ui.label(RichText::new(tr("Stiller, kenar boşlukları, \\pos/\\move/\\org/\\clip/\\fs/\\bord/... ve çizimler ölçeklenir.")).weak().small());
            if ui.button(tr("Uygula")).clicked() {
                let mut s = self.doc.script.clone();
                timing::resample(&mut s, self.res_x, self.res_y);
                self.doc.apply_new_script("Çözünürlüğü yeniden örnekle", s);
            }
        });
        self.resample_open = open;
    }

    /// Yüklü yazım sözlüğüyle OCR düzeltmesi (sözlük yoksa 0).
    pub(super) fn ocr_fix_script(&self, sc: &mut osisub_core::Script) -> usize {
        let Some(sp) = self.spell.clone() else { return 0 };
        let mut total = 0;
        for e in sc.entries.iter_mut() {
            if let Entry::Dialogue(d) = e {
                let (t, n) = osisub_core::ocrfix::fix_text(&d.text, &|w| sp.check(w));
                if n > 0 { d.text = t; total += n; }
            }
        }
        total
    }

    pub(super) fn text_import_window(&mut self, ctx: &egui::Context) {
        use osisub_core::textimport as ti;
        let Some(w) = self.text_import.as_mut() else { return };
        let mut open = true;
        let mut act = 0; // 1 yeni satırlar, 2 mevcut zamanlamaya
        let items = ti::parse(&w.text, if w.blocks { ti::Split::Blocks } else { ti::Split::Lines }, w.speakers);
        egui::Window::new(tr("Düz metin içe aktar")).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Çeviri metnini yapıştırın ya da .txt dosyasından alın. '#' ya da '//' ile başlayan satırlar atlanır.")).weak());
            ui.horizontal(|ui| {
                if ui.button(tr("Dosyadan…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Metin", &["txt", "md"]).pick_file() {
                        if let Ok(b) = std::fs::read(&p) { w.text = osisub_core::ass::decode_bytes(&b); }
                    }
                }
                ui.radio_value(&mut w.blocks, false, tr("Her satır bir altyazı"));
                ui.radio_value(&mut w.blocks, true, tr("Boş satırla ayrılmış bloklar"));
                ui.checkbox(&mut w.speakers, tr("\"Ad: replik\" → Konuşan"));
            });
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                ui.add(TextEdit::multiline(&mut w.text).desired_rows(12).desired_width(f32::INFINITY).hint_text(tr("Hinata: Merhaba!\nKageyama: Geç kaldın.")));
            });
            ui.label(trf!("{} altyazı", items.len()));
            ui.horizontal(|ui| {
                if ui.add_enabled(!items.is_empty(), egui::Button::new(tr("Etkin satırdan itibaren zamanlamaya yerleştir"))).on_hover_text(tr("Zamanlanmış satırların metni sırayla bu metinle değiştirilir (yorum, tabela ve karaoke satırları atlanır; baştaki {\\an8} gibi etiketler korunur)")).clicked() { act = 2; }
                if ui.button(tr("Belgeden metni al")).on_hover_text(tr("Açık belgenin diyaloglarını (tabela/karaoke hariç) \"Ad: replik\" biçiminde buraya koyar: çevirmene gönderip geri yerleştirmek için")).clicked() { w.text = ti::export(&self.doc.script, w.speakers); }
                if ui.add_enabled(!items.is_empty(), egui::Button::new(tr("Yeni satırlar olarak ekle"))).on_hover_text(tr("Etkin satırdan sonra; süreler okuma hızına göre (playhead ya da etkin satırın bitişinden başlar)")).clicked() { act = 1; }
            });
        });
        if act == 2 {
            let mut s = self.doc.script.clone();
            let (n, left, touched) = ti::fill_existing(&mut s, &items, self.doc.active, w.speakers);
            self.doc.apply_new_script("Düz metni zamanlamaya yerleştir", s);
            if let Some(&a) = touched.first() { self.doc.active = a; }
            self.doc.selection = touched;
            self.status = if left > 0 { format!("{n} satıra yerleştirildi; {left} metin satırı arttı (zamanlanmış satır kalmadı)") } else { format!("{n} satıra yerleştirildi") };
            open = false;
        } else if act == 1 {
            let at = self.doc.active;
            let (start, style) = match self.doc.script.entries.get(at) {
                Some(Entry::Dialogue(d)) => (if self.media.path.is_some() { self.media.playhead } else { d.end_time }, d.style.clone()),
                _ => (self.media.playhead, "Default".to_string()),
            };
            let ds = ti::to_dialogues(&items, start, self.rules.max_cps.min(17.0), self.rules.min_duration_ms.max(1000), self.rules.max_duration_ms, 120, &style);
            let n = ds.len();
            let pos = if matches!(self.doc.script.entries.get(at), Some(Entry::Dialogue(_))) { at + 1 } else { self.doc.script.entries.len() };
            self.doc.edit("Düz metin içe aktar", |tx| tx.insert(pos, ds.into_iter().map(Entry::Dialogue).collect()));
            self.doc.selection = (pos..pos + n).collect();
            self.doc.active = pos;
            self.status = trf!("{n} satır eklendi");
            open = false;
        }
        if !open { self.text_import = None; }
    }

    pub(super) fn hardsub_window(&mut self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        let Some(mut w) = self.hardsub.take() else { return };
        if w.path.is_none() { w.path = self.media.path.clone(); }
        if let Some(rx) = &w.rx {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            if let Ok(r) = rx.try_recv() {
                w.rx = None;
                match r {
                    Ok(mut sc) => {
                        let n = sc.dialogues().count();
                        let fixed = if w.fix { self.ocr_fix_script(&mut sc) } else { 0 };
                        if w.as_reference {
                            self.set_reference(sc);
                            self.status = trf!("Gömülü altyazı: {n} satır referans olarak yüklendi{}", if fixed > 0 { format!(", {fixed} OCR hatası düzeltildi") } else { String::new() });
                        } else {
                            self.request(Guarded::OpenScript(sc, format!("Gömülü altyazı: {n} satır — denetleyip kaydedin")));
                        }
                        return;
                    }
                    Err(e) => w.error = Some(e),
                }
            }
        }
        let mut open = true;
        let busy = w.rx.is_some();
        egui::Window::new(tr("Gömülü altyazıyı metne çevir (hardsub OCR)")).open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Videoya yakılmış altyazıyı okur: kenarlı (outline) açık renkli metnin belirip kaybolduğu anlar satır olur, her satır Tesseract ile okunur. Sonucu mutlaka denetleyin.")).weak());
            if !w.tess_ok {
                ui.label(RichText::new(tr("Tesseract bulunamadı (OSISUB_TESSERACT ile yol verilebilir).")).color(theme::BAD));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(tr("Video seç…"))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file() { w.path = Some(p); }
                }
                match &w.path {
                    Some(p) => { ui.label(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).on_hover_text(p.display().to_string()); }
                    None => { ui.label(tr("(video yok)")); }
                }
            });
            egui::Grid::new("hs_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("Taranan bölge"));
                ui.horizontal(|ui| {
                    let mut t = (w.opts.top * 100.0).round() as i32;
                    let mut b = (w.opts.bottom * 100.0).round() as i32;
                    ui.label(tr("üst %")); if ui.add(egui::DragValue::new(&mut t).range(0..=95)).changed() { w.opts.top = t as f64 / 100.0; }
                    ui.label(tr("alt %")); if ui.add(egui::DragValue::new(&mut b).range(5..=100)).changed() { w.opts.bottom = b as f64 / 100.0; }
                    if ui.small_button(tr("Alt")).on_hover_text(tr("Diyalog: alt %30")).clicked() { w.opts.top = 0.70; w.opts.bottom = 1.0; }
                    if ui.small_button(tr("Üst")).on_hover_text(tr("Üstteki altyazılar/notlar: üst %30")).clicked() { w.opts.top = 0.0; w.opts.bottom = 0.30; }
                });
                ui.end_row();
                ui.label(tr("Örnekleme"));
                ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut w.opts.fps).range(2.0..=24.0).speed(0.5)); ui.label(tr("kare/sn (yüksek = daha hassas zaman, daha yavaş)")); });
                ui.end_row();
                ui.label(tr("Metin parlaklığı ≥"));
                ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut w.opts.detect.bright).range(100..=255)); ui.label(tr("(sarı/renkli metinde düşürün)")); });
                ui.end_row();
                ui.label(tr("Dil"));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("hs_lang").selected_text(w.opts.lang.clone()).show_ui(ui, |ui| {
                        for l in w.langs.clone() { ui.selectable_value(&mut w.opts.lang, l.clone(), l); }
                    });
                    ui.add(TextEdit::singleline(&mut w.opts.lang).desired_width(90.0)).on_hover_text(tr("Birden çok dil: eng+jpn; Türkçe için 'tur' dil verisi gerekir"));
                });
                ui.end_row();
            });
            ui.checkbox(&mut w.opts.also_top, tr("Üst bölgeyi de tara (tabela çevirileri / notlar; {\\an8} ile, süre iki katı)"));
            ui.checkbox(&mut w.opts.exact, tr("Kare-doğru zamanlar")).on_hover_text(tr("Satır sınırlarını videonun kendi karelerinde bulur (Aegisub kare anlambilimi); kapalıyken örnekleme aralığı kadar sapabilir, biraz daha hızlıdır"));
            ui.checkbox(&mut w.as_reference, tr("Referans olarak yükle (çeviri kaynağı; açık belge değişmez)"));
            ui.add_enabled(self.spell.is_some(), egui::Checkbox::new(&mut w.fix, "OCR hatalarını sözlükle düzelt (rn→m, l→I, 0→o…; sözlük OCR diliyle aynı olmalı)")).on_disabled_hover_text(tr("Yazım sekmesinden OCR dilinin sözlüğünü yükleyin"));
            if let Some(e) = &w.error { ui.label(RichText::new(e).color(theme::BAD)); }
            ui.horizontal(|ui| {
                if ui.add_enabled(w.path.is_some() && !busy && w.tess_ok, egui::Button::new(tr("Başlat"))).clicked() {
                    let (p, o) = (w.path.clone().unwrap(), w.opts.clone());
                    let (tx, rx) = std::sync::mpsc::channel();
                    let (pr, cancel, c2) = (w.progress.clone(), w.cancel.clone(), ctx.clone());
                    cancel.store(false, Ordering::Relaxed);
                    pr.store(0f32.to_bits(), Ordering::Relaxed);
                    std::thread::spawn(move || {
                        let c3 = c2.clone();
                        let r = osisub_media::hardsub::extract_all(&p, &o, &|f| { pr.store(f.to_bits(), Ordering::Relaxed); c3.request_repaint(); }, &cancel);
                        let _ = tx.send(r);
                        c2.request_repaint();
                    });
                    w.rx = Some(rx);
                    w.started = Some(std::time::Instant::now());
                    w.error = None;
                }
                if busy {
                    let f = f32::from_bits(w.progress.load(Ordering::Relaxed));
                    ui.add(egui::ProgressBar::new(f).desired_width(180.0).text(format!("%{:.0} · {} sn", f * 100.0, w.started.map(|s| s.elapsed().as_secs()).unwrap_or(0))));
                    if ui.button(tr("İptal")).clicked() { w.cancel.store(true, Ordering::Relaxed); }
                }
            });
        });
        if !open { w.cancel.store(true, Ordering::Relaxed); return; }
        self.hardsub = Some(w);
    }

    pub(super) fn ocr_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.ocr.take() else { return };
        // Arka plan işi bitti mi?
        if let Some(rx) = &w.rx {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            if let Ok(r) = rx.try_recv() {
                w.rx = None;
                match r {
                    Ok(mut sc) => {
                        if w.fix { self.ocr_fix_script(&mut sc); }
                        let n = sc.dialogues().count();
                        let name = w.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                        if w.as_reference {
                            self.set_reference(sc);
                            self.status = trf!("OCR: {n} satır referans olarak yüklendi ({name})");
                        } else {
                            self.request(Guarded::OpenScript(sc, format!("OCR: {n} satır ({name}) — kaydetmeyi unutmayın")));
                        }
                        return;
                    }
                    Err(e) => w.error = Some(e),
                }
            }
        }
        let mut open = true;
        let busy = w.rx.is_some();
        egui::Window::new(tr("Resim altyazısı OCR (PGS / VobSub)")).open(&mut open).collapsible(false).default_width(460.0).show(ctx, |ui| {
            if !w.tess_ok {
                ui.label(RichText::new(tr("Tesseract bulunamadı. Kurun (Windows: UB-Mannheim tesseract; Linux: tesseract-ocr paketi) ya da OSISUB_TESSERACT ile yolunu verin. Türkçe için 'tur' dil verisi gerekir.")).color(theme::BAD));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(tr("Dosya seç…"))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PGS / VobSub / video", &["sup", "idx", "sub", "mkv", "m2ts", "mks", "mp4", "vob"]).pick_file() {
                        w.tracks.clear();
                        w.track = None;
                        w.error = None;
                        if !matches!(osisub_core::io::ext_of(&p).as_str(), "sup" | "idx" | "sub") {
                            match osisub_media::mux::subtitle_tracks(&p) {
                                Ok(t) => {
                                    w.tracks = t.iter().filter(|t| osisub_media::mux::is_bitmap_sub(&t.codec)).map(|t| (t.index, t.label())).collect();
                                    w.track = w.tracks.first().map(|t| t.0);
                                    if w.tracks.is_empty() { w.error = Some("Bu videoda PGS/VobSub izi yok".into()); }
                                }
                                Err(e) => w.error = Some(e),
                            }
                        }
                        w.path = Some(p);
                    }
                }
                ui.label(w.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(dosya yok)".into()));
            });
            if !w.tracks.is_empty() {
                egui::ComboBox::from_label("İz").selected_text(w.tracks.iter().find(|t| Some(t.0) == w.track).map(|t| t.1.clone()).unwrap_or_default()).show_ui(ui, |ui| {
                    for (i, l) in w.tracks.clone() { ui.selectable_value(&mut w.track, Some(i), l); }
                });
            }
            ui.horizontal(|ui| {
                ui.label(tr("Dil"));
                egui::ComboBox::from_id_salt("ocr_lang").selected_text(w.lang.clone()).show_ui(ui, |ui| {
                    for l in w.langs.clone() { ui.selectable_value(&mut w.lang, l.clone(), l); }
                });
                ui.add(TextEdit::singleline(&mut w.lang).desired_width(90.0)).on_hover_text(tr("Birden çok dil: eng+jpn"));
            });
            ui.checkbox(&mut w.as_reference, tr("Referans olarak yükle (açık belgeyi değiştirmez; ızgarada Referans sütunu)"));
            ui.add_enabled(self.spell.is_some(), egui::Checkbox::new(&mut w.fix, "OCR hatalarını sözlükle düzelt (rn→m, l→I, 0→o…; sözlük OCR diliyle aynı olmalı)")).on_disabled_hover_text(tr("Yazım sekmesinden OCR dilinin sözlüğünü yükleyin"));
            if let Some(e) = &w.error { ui.label(RichText::new(e).color(theme::BAD)); }
            ui.horizontal(|ui| {
                let ready = w.path.is_some() && !busy;
                if ui.add_enabled(ready, egui::Button::new(tr("OCR başlat"))).clicked() {
                    let (p, t, l) = (w.path.clone().unwrap(), w.track, w.lang.clone());
                    let (tx, rx) = std::sync::mpsc::channel();
                    let c2 = ctx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(osisub_media::ocr::ocr_file(&p, t, &l));
                        c2.request_repaint();
                    });
                    w.rx = Some(rx);
                    w.started = Some(std::time::Instant::now());
                    w.error = None;
                }
                if busy {
                    ui.spinner();
                    ui.label(trf!("çalışıyor… {} sn", w.started.map(|s| s.elapsed().as_secs()).unwrap_or(0)));
                }
            });
            ui.label(RichText::new(tr("İpucu: sonuçta büyük I / küçük l karışıklıkları için Bul/Değiştir ve yazım denetimi kullanın.")).weak());
        });
        if open || busy {
            self.ocr = Some(w);
        }
    }

    pub(super) fn whisper_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.whisper.take() else { return };
        if let Some(rx) = &w.rx {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            if let Ok(r) = rx.try_recv() {
                w.rx = None;
                match r {
                    Ok(Some(sc)) => {
                        let n = sc.dialogues().count();
                        let name = w.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                        if w.as_reference {
                            self.set_reference(sc);
                            self.status = trf!("Whisper: {n} satır referans olarak yüklendi ({name})");
                        } else {
                            self.request(Guarded::OpenScript(sc, format!("Whisper: {n} satır ({name}) — kaydetmeyi unutmayın")));
                        }
                        return;
                    }
                    Ok(None) => {}
                    Err(e) => w.error = Some(e),
                }
            }
        }
        if w.path.is_none() {
            w.path = self.media.path.clone();
        }
        let mut open = true;
        let busy = w.rx.is_some();
        egui::Window::new(tr("Sesten altyazı (Whisper)")).open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Videonun sesini yerel olarak yazıya döker ve zamanlanmış satırlar üretir (Subtitle Edit \"Audio to text\" gibi). Ham Japonca döküm, İngilizce çeviri ya da boş zamanlama iskeleti için.")).weak());
            if !w.ok {
                ui.label(RichText::new(tr("whisper.cpp (whisper-cli) bulunamadı. Kurun ya da OSISUB_WHISPER ile yolunu verin; program klasörüne de koyabilirsiniz.")).color(theme::BAD));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(tr("Video/ses seç…"))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Video / ses", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts", "wav", "flac", "mka", "mp3", "aac", "opus", "ogg"]).pick_file() {
                        w.path = Some(p);
                    }
                }
                ui.label(w.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(dosya yok)".into()));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Model"));
                let cur = w.model.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "(yok)".into());
                egui::ComboBox::from_id_salt("whisper_model").selected_text(cur).show_ui(ui, |ui| {
                    for m in w.models.clone() {
                        let n = m.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        ui.selectable_value(&mut w.model, Some(m), n);
                    }
                });
                if ui.add_enabled(!busy, egui::Button::new(tr("Model dosyası…"))).on_hover_text(tr("whisper.cpp ggml modeli (ör. ggml-small.bin, ggml-large-v3-turbo.bin)")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("ggml model", &["bin"]).pick_file() {
                        if !w.models.contains(&p) { w.models.push(p.clone()); }
                        w.model = Some(p);
                    }
                }
            });
            if w.models.is_empty() {
                let dir = osisub_core::paths::config_dir().map(|d| d.join("whisper").display().to_string()).unwrap_or_default();
                ui.label(RichText::new(trf!("Model bulunamadı: ggml-*.bin dosyasını {dir} klasörüne koyun ya da seçin.")).color(theme::WARN));
            }
            ui.horizontal(|ui| {
                ui.label(tr("Dil"));
                egui::ComboBox::from_id_salt("whisper_lang").selected_text(w.lang.clone()).show_ui(ui, |ui| {
                    for (c, n) in [("auto", "otomatik"), ("ja", "Japonca"), ("en", "İngilizce"), ("tr", "Türkçe"), ("ko", "Korece"), ("zh", "Çince")] {
                        ui.selectable_value(&mut w.lang, c.to_string(), format!("{c} — {n}"));
                    }
                });
                ui.checkbox(&mut w.translate, tr("İngilizceye çevir"));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Satır başına en çok karakter"));
                ui.add(egui::DragValue::new(&mut w.max_len).range(0..=120)).on_hover_text(tr("0 = whisper varsayılanı (uzun bölümler); sözcük sınırında bölünür"));
            });
            ui.checkbox(&mut w.as_reference, tr("Referans olarak yükle (açık belgeyi değiştirmez)"));
            if let Some(e) = &w.error { ui.label(RichText::new(e).color(theme::BAD)); }
            ui.horizontal(|ui| {
                let ready = w.path.is_some() && w.model.is_some() && !busy;
                if ui.add_enabled(ready, egui::Button::new(tr("Başlat"))).clicked() {
                    let (p, o) = (w.path.clone().unwrap(), osisub_media::whisper::Options { model: w.model.clone().unwrap(), lang: w.lang.clone(), translate: w.translate, max_len: w.max_len, threads: 0 });
                    let (tx, rx) = std::sync::mpsc::channel();
                    let (c2, prog) = (ctx.clone(), w.progress.clone());
                    prog.store(0, std::sync::atomic::Ordering::Relaxed);
                    std::thread::spawn(move || {
                        let r = osisub_media::whisper::transcribe(&p, &o, |f| {
                            prog.store((f * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed);
                            c2.request_repaint();
                        });
                        let _ = tx.send(r.map(Some));
                        c2.request_repaint();
                    });
                    w.rx = Some(rx);
                    w.started = Some(std::time::Instant::now());
                    w.error = None;
                }
                if busy {
                    let f = w.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                    ui.add(egui::ProgressBar::new(f).desired_width(180.0).show_percentage());
                    ui.label(format!("{} sn", w.started.map(|s| s.elapsed().as_secs()).unwrap_or(0)));
                }
            });
            ui.label(RichText::new(tr("İpucu: sonucu Çeviri sekmesinde referans olarak kullanın; zamanlamayı Zamanlama → Sahne değişimine yapıştır ile iyileştirin.")).weak());
        });
        if open || busy {
            self.whisper = Some(w);
        }
    }

    pub(super) fn song_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.song_win.take() else { return };
        // arka plan sonucu
        if let Some(rx) = &w.rx {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            if let Ok(r) = rx.try_recv() {
                w.rx = None;
                match r {
                    Ok((song, off, score)) => {
                        self.place_song(&song, off, Some(score));
                        if score >= 0.6 { return; }
                    }
                    Err(e) => w.error = Some(e),
                }
            }
        }
        let busy = w.rx.is_some();
        let mut open = true;
        let mut go = false;
        egui::Window::new(tr("OP/ED yerleştir")).open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Önceki bir bölüme göre zamanlanmış şarkı altyazısını (OP/ED karaoke + çeviri) bu bölüme yerleştirir.")).weak());
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(tr("Şarkı dosyası…"))).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() { w.song = Some(p); w.error = None; }
                }
                ui.label(w.song.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(seçilmedi)".into()));
            });
            ui.radio_value(&mut w.by_audio, true, tr("Sesle bul: şarkının zamanlandığı ESKİ bölümü seç (açık video yeni bölüm olmalı)"));
            ui.radio_value(&mut w.by_audio, false, tr("Playhead'e yerleştir (şarkının ilk satırı playhead'de başlar)"));
            if w.by_audio {
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(tr("Eski bölüm…"))).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("Video / ses", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts", "mka", "flac", "wav", "aac", "opus", "mp3"]).pick_file() { w.old = Some(p); w.error = None; }
                    }
                    ui.label(w.old.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(seçilmedi)".into()));
                });
                if self.media.waveform().is_none() {
                    ui.label(RichText::new(tr("Önce yeni bölümün videosunu açın (Video → Video / ses aç…); dalga formu yüklenmeli.")).color(theme::WARN));
                }
            }
            if let Some(e) = &w.error { ui.label(RichText::new(e).color(theme::BAD)); }
            ui.horizontal(|ui| {
                let ready = w.song.is_some() && !busy && (!w.by_audio || (w.old.is_some() && self.media.waveform().is_some()));
                if ui.add_enabled(ready, egui::Button::new(tr("Yerleştir"))).clicked() { go = true; }
                if busy { ui.spinner(); ui.label(tr("ses eşleştiriliyor…")); }
            });
        });
        if go {
            let song = w.song.clone().unwrap();
            if w.by_audio {
                let (old, new, c2) = (w.old.clone().unwrap(), self.media.waveform().unwrap(), ctx.clone());
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let r = (|| {
                        let sc = osisub_core::io::load(&song)?;
                        let (a, b) = song_span(&sc).ok_or("Şarkı dosyasında satır yok")?;
                        let ow = osisub_media::load_waveform(&old)?;
                        let (off, score) = osisub_media::resync::locate(&ow, &new, a, b).ok_or("Eski bölümde şarkı aralığı sessiz ya da çok kısa")?;
                        Ok((sc, off, score))
                    })();
                    let _ = tx.send(r);
                    c2.request_repaint();
                });
                w.rx = Some(rx);
                w.error = None;
            } else {
                match osisub_core::io::load(&song) {
                    Ok(sc) => match song_span(&sc) {
                        Some((a, _)) => { let off = self.media.playhead - a; self.place_song(&sc, off, None); return; }
                        None => w.error = Some("Şarkı dosyasında satır yok".into()),
                    },
                    Err(e) => w.error = Some(e),
                }
            }
        }
        if open || w.rx.is_some() { self.song_win = Some(w); }
    }

    /// Şarkı dosyasını `off` ms kaydırarak birleştirir ve eklenen satırları seçer.
    fn place_song(&mut self, song: &osisub_core::Script, off: i64, score: Option<f64>) {
        let before: std::collections::HashSet<(i64, i64, String)> = self.doc.script.dialogues().map(|d| (d.start_time, d.end_time, d.text.clone())).collect();
        let mut s = self.doc.script.clone();
        let opts = osisub_core::merge::MergeOptions { offset_ms: off, sort: true, ..Default::default() };
        let r = osisub_core::merge::merge_into(&mut s, song, &opts);
        self.doc.apply_new_script("OP/ED yerleştir", s);
        let added: Vec<usize> = self.doc.script.entries.iter().enumerate().filter_map(|(i, e)| match e { Entry::Dialogue(d) if !before.contains(&(d.start_time, d.end_time, d.text.clone())) => Some(i), _ => None }).collect();
        if let Some(&first) = added.first() { self.doc.select(added.clone(), first); self.scroll_to = Some(first); }
        let sign = if off < 0 { "-" } else { "+" };
        let mut msg = format!("OP/ED yerleştirildi: {} satır, kayma {sign}{}", r.lines, time::format_ass(off.abs()));
        if let Some(sc) = score {
            msg.push_str(&format!(", ses eşleşmesi %{:.0}", sc * 100.0));
            if sc < 0.6 { msg.push_str(" — DÜŞÜK: yerleşimi kontrol edin (Ctrl+Z geri alır)"); }
        }
        self.status = msg;
    }

    pub(super) fn open_dummy_window(&mut self) {
        let mut d = osisub_core::dummy::DummyVideo::default();
        // Betik çözünürlüğü varsa onu öner
        let (w, h) = self.doc.script.res();
        if w >= 16 && h >= 16 {
            d.width = w as u32;
            d.height = h as u32;
        }
        let mins = d.duration_ms() as f64 / 60_000.0;
        self.dummy_win = Some((d, mins.round()));
    }

    pub(super) fn dummy_window(&mut self, ctx: &egui::Context) {
        let Some((mut d, mut mins)) = self.dummy_win.take() else { return };
        let mut open = true;
        let mut go = false;
        egui::Window::new(tr("Sahte video")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("dummy_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tr("Kare hızı"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut d.fps).speed(0.01).range(1.0..=240.0).max_decimals(3));
                    for f in [23.976, 24.0, 25.0, 29.97, 30.0] {
                        if ui.small_button(format!("{}", f)).clicked() { d.fps = f; }
                    }
                });
                ui.end_row();
                ui.label(tr("Süre (dakika)"));
                ui.add(egui::DragValue::new(&mut mins).speed(0.5).range(0.1..=600.0));
                ui.end_row();
                ui.label(tr("Çözünürlük"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut d.width).range(16..=8192));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut d.height).range(16..=8192));
                    for (w, h) in [(640, 480), (1280, 720), (1920, 1080)] {
                        if ui.small_button(format!("{}p", h)).clicked() { d.width = w; d.height = h; }
                    }
                });
                ui.end_row();
                ui.label(tr("Renk"));
                ui.color_edit_button_srgb(&mut d.rgb);
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut d.checkerboard, tr("Damalı desen"));
                ui.end_row();
            });
            d.frames = ((mins * 60.0 * d.fps).round() as u64).max(1);
            ui.label(RichText::new(trf!("{} kare. İlk açılışta üretilir (uzun sürelerde birkaç on saniye), sonra önbellekten anında açılır.", d.frames)).weak());
            if ui.button(tr("Aç")).clicked() { go = true; }
        });
        if go {
            self.media.open_dummy(&d, ctx);
            self.show_media = true;
        } else if open {
            self.dummy_win = Some((d, mins));
        }
    }

    pub(super) fn goto_window(&mut self, ctx: &egui::Context) {
        let mut open = self.goto_open;
        egui::Window::new(tr("Zamana git")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label(tr("Zaman (H:MM:SS.cc, MM:SS ya da saniye):"));
            let r = ui.add(TextEdit::singleline(&mut self.goto_buf).desired_width(160.0));
            if self.goto_focus { r.request_focus(); self.goto_focus = false; }
            if ui.button(tr("Git")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                let t = time::parse_ass(&self.goto_buf).or_else(|| self.goto_buf.trim().parse::<f64>().ok().map(|s| (s * 1000.0) as i64));
                match t {
                    Some(t) => {
                        // en yakın satır (başlangıcı t'ye en yakın/içeren)
                        let best = self.rows.iter().copied().min_by_key(|&i| match &self.doc.script.entries[i] { Entry::Dialogue(d) => if d.start_time <= t && t < d.end_time { 0 } else { (d.start_time - t).abs().min((d.end_time - t).abs()) }, _ => i64::MAX });
                        if let Some(b) = best { self.doc.select(vec![b], b); self.scroll_to = Some(b); }
                        self.media.playhead = t;
                        self.goto_open = false;
                    }
                    None => self.status = tr("Geçersiz zaman").into(),
                }
            }
        });
        if !open { self.goto_open = false; }
    }

    pub(super) fn vad_window(&mut self, ctx: &egui::Context) {
        let mut open = self.vad_open;
        egui::Window::new(tr("Sesten satır oluştur")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label(tr("Konuşma bölgelerini bulup boş altyazı satırları oluşturur (zamanlama iskeleti)."));
            egui::Grid::new("vad_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("Eşik payı (dB)")); ui.add(egui::DragValue::new(&mut self.vad.margin_db).speed(0.5).range(3.0..=40.0)); ui.end_row();
                ui.label(tr("Mutlak taban (dBFS)")); ui.add(egui::DragValue::new(&mut self.vad.floor_db).speed(0.5).range(-90.0..=-10.0)); ui.end_row();
                ui.label(tr("En kısa konuşma (ms)")); ui.add(egui::DragValue::new(&mut self.vad.min_speech_ms).speed(10)); ui.end_row();
                ui.label(tr("En kısa sessizlik (ms)")); ui.add(egui::DragValue::new(&mut self.vad.min_silence_ms).speed(10)); ui.end_row();
                ui.label(tr("Kenar payı (ms)")); ui.add(egui::DragValue::new(&mut self.vad.pad_ms).speed(5)); ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button(tr("Satırları konuşmaya yapıştır")).on_hover_text(tr("Seçili (yoksa tüm) satırların başlangıç/bitişini ±250 ms içindeki en yakın konuşma sınırına çeker")).clicked() {
                    if let Some(w) = self.media.waveform() {
                        let segs = osisub_media::detect_speech(&w, &self.vad);
                        let sel = self.doc.selection.clone();
                        let only = (sel.len() > 1).then_some(sel);
                        let mut s = self.doc.script.clone();
                        let n = osisub_core::timing::snap_to_speech(&mut s, &segs, 250, only.as_deref());
                        self.doc.apply_new_script("Konuşmaya yapıştır", s);
                        self.status = trf!("{n} satır konuşma sınırına yapıştırıldı");
                    } else {
                        self.status = tr("Önce video/ses açın (dalga formu gerekli)").into();
                    }
                }
            });
            ui.separator();
            ui.checkbox(&mut self.vad_skip_existing, tr("Zaten satır olan bölgeleri atla"));
            if ui.button(tr("Oluştur")).clicked() {
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
                    self.status = trf!("{} satır oluşturuldu", n);
                }
            }
        });
        self.vad_open = open;
    }

    /// Dosyayı böl (core::filesplit): kesme zamanları / eşit parça, isteğe bağlı zaman sıfırlama.
    pub(super) fn split_window(&mut self, ctx: &egui::Context) {
        let Some((cuts, n, rebase)) = self.split_win.as_mut() else { return };
        let mut open = true;
        let mut go = false;
        let playhead = self.media.playhead;
        let script = &self.doc.script;
        egui::Window::new(tr("Dosyayı böl")).open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label(tr("Kesme zamanları (virgülle; satırlar başlangıç zamanına göre dağıtılır):"));
            ui.add(TextEdit::singleline(cuts).hint_text("0:01:30.00, 0:22:10.00").desired_width(400.0));
            ui.horizontal(|ui| {
                if ui.button(tr("Playhead'i ekle")).clicked() {
                    if !cuts.trim().is_empty() { cuts.push_str(", "); }
                    cuts.push_str(&time::format_ass(playhead));
                }
                ui.separator();
                ui.add(egui::DragValue::new(n).range(2..=50));
                if ui.button(tr("parçaya eşit böl")).on_hover_text(tr("Satır sayısı eşit olacak kesme zamanlarını hesaplar")).clicked() {
                    *cuts = osisub_core::filesplit::equal_cuts(script, *n).iter().map(|&t| time::format_ass(t)).collect::<Vec<_>>().join(", ");
                }
            });
            ui.checkbox(rebase, "Her parçanın zamanını kendi başlangıcından sıfırla").on_hover_text(tr("Ör. bölümden OP'yi ayırıp başka bölüme yerleştirmek için"));
            let parsed: Vec<Option<i64>> = cuts.split(',').filter(|c| !c.trim().is_empty()).map(|c| osisub_core::time::parse_generic(c)).collect();
            if parsed.iter().any(|c| c.is_none()) {
                ui.label(RichText::new(tr("Geçersiz zaman var")).color(theme::BAD));
            } else if !parsed.is_empty() {
                let parts = osisub_core::filesplit::split_times(script, &parsed.iter().flatten().copied().collect::<Vec<_>>(), false);
                ui.label(trf!("{} parça: {}", parts.len(), parts.iter().map(|p| format!("{} satır", p.dialogues().count())).collect::<Vec<_>>().join(", ")));
                if ui.button(tr("Böl ve kaydet…")).clicked() { go = true; }
            }
        });
        if go {
            let (cuts, _, rebase) = self.split_win.clone().unwrap();
            let c: Vec<i64> = cuts.split(',').filter_map(|c| osisub_core::time::parse_generic(c)).collect();
            let def = self.doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "parca.ass".into());
            if let Some(base) = rfd::FileDialog::new().set_file_name(&def).add_filter("ASS", &["ass"]).save_file() {
                let parts = osisub_core::filesplit::split_times(&self.doc.script, &c, rebase);
                let paths = osisub_core::filesplit::part_paths(&base, parts.len());
                if let Some(p) = paths.iter().find(|p| p.exists()) {
                    self.status = trf!("Dosya zaten var, üzerine yazılmadı: {}", p.display());
                } else {
                    let mut err = None;
                    for (s, p) in parts.iter().zip(&paths) {
                        if let Err(e) = osisub_core::io::save(s, p) { err = Some(e); break; }
                    }
                    self.status = match err { Some(e) => format!("Kaydedilemedi: {e}"), None => format!("{} parça kaydedildi: {}", parts.len(), paths[0].display()) };
                    if self.status.contains("parça kaydedildi") { open = false; }
                }
            }
        }
        if !open { self.split_win = None; }
    }

    fn mrep_path() -> Option<PathBuf> {
        osisub_core::paths::data_dir().map(|d| d.join("multireplace.tsv"))
    }

    pub(super) fn open_mrep(&mut self) {
        let rules = Self::mrep_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| osisub_core::multireplace::from_tsv(&t)).filter(|r| !r.is_empty()).unwrap_or_else(osisub_core::multireplace::examples);
        self.mrep = Some((rules, vec![], false));
    }

    /// Çoklu değiştir penceresi.
    pub(super) fn mrep_window(&mut self, ctx: &egui::Context) {
        use osisub_core::multireplace as mr;
        let Some((mut rules, mut pv, mut only_sel)) = self.mrep.take() else { return };
        let was_sel = only_sel;
        let mut open = true;
        let mut apply = false;
        let mut changed = false;
        egui::Window::new(tr("Çoklu değiştir")).open(&mut open).collapsible(false).default_width(640.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Kurallar yukarıdan aşağı sırayla uygulanır; etiketler ({…}) ve yorum satırları korunur. Liste otomatik kaydedilir.")).weak());
            let mut del: Option<usize> = None;
            let mut up: Option<usize> = None;
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                egui::Grid::new("mrep_grid").num_columns(8).striped(true).show(ui, |ui| {
                    ui.label(""); ui.label(tr("Bul")); ui.label(tr("Değiştir")); ui.label(tr("Regex")); ui.label(tr("Aa")); ui.label(tr("Sözcük")); ui.label(tr("Eşleşme")); ui.label(""); ui.end_row();
                    for (k, r) in rules.iter_mut().enumerate() {
                        changed |= ui.checkbox(&mut r.on, "").changed();
                        changed |= ui.add_sized([180.0, 20.0], TextEdit::singleline(&mut r.find).font(egui::TextStyle::Monospace)).changed();
                        changed |= ui.add_sized([180.0, 20.0], TextEdit::singleline(&mut r.replace).font(egui::TextStyle::Monospace)).changed();
                        changed |= ui.checkbox(&mut r.regex, "").changed();
                        changed |= ui.checkbox(&mut r.match_case, "").on_hover_text(tr("Büyük/küçük harf duyarlı")).changed();
                        changed |= ui.checkbox(&mut r.whole_word, "").changed();
                        match pv.get(k) {
                            Some(Ok(n)) => { ui.label(if *n > 0 { RichText::new(trf!("{n} satır")).color(theme::ACCENT) } else { RichText::new("0").weak() }); }
                            Some(Err(e)) => { ui.label(RichText::new(tr("hata")).color(theme::BAD)).on_hover_text(e); }
                            None => { ui.label(""); }
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button(tr("Yukarı")).clicked() && k > 0 { up = Some(k); }
                            if ui.small_button(tr("Sil")).clicked() { del = Some(k); }
                        });
                        ui.end_row();
                    }
                });
            });
            if let Some(k) = del { rules.remove(k); changed = true; }
            if let Some(k) = up { rules.swap(k, k - 1); changed = true; }
            ui.horizontal(|ui| {
                if ui.button(tr("+ Kural")).clicked() { rules.push(mr::Rule::default()); changed = true; }
                if ui.button(tr("İçe aktar…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("TSV", &["tsv", "txt"]).pick_file() {
                        if let Ok(t) = std::fs::read_to_string(p) { rules.extend(mr::from_tsv(&t)); changed = true; }
                    }
                }
                if ui.button(tr("Dışa aktar…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_file_name("kurallar.tsv").save_file() { let _ = std::fs::write(p, mr::to_tsv(&rules)); }
                }
                ui.separator();
                ui.checkbox(&mut only_sel, tr("yalnızca seçili satırlar"));
                if ui.button(tr("Uygula")).clicked() { apply = true; }
            });
        });
        let only: Option<Vec<usize>> = if only_sel { Some(self.doc.target_lines()) } else { None };
        // belge, seçim ya da kapsam değişince canlı sayımlar yenilenir
        let key = (self.doc.rev, self.doc.selection.len(), self.doc.selection.first().copied(), only_sel);
        let stale = self.mrep_key != Some(key);
        self.mrep_key = Some(key);
        if changed || stale || was_sel != only_sel || pv.len() != rules.len() {
            pv = mr::preview(&self.doc.script, &rules, only.as_deref());
            if changed { if let Some(p) = Self::mrep_path() { if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); } let _ = std::fs::write(p, mr::to_tsv(&rules)); } }
        }
        if apply {
            let mut s = self.doc.script.clone();
            let r = mr::apply(&mut s, &rules, only.as_deref());
            let total: usize = r.iter().filter_map(|x| x.as_ref().ok()).sum();
            let errs = r.iter().filter(|x| x.is_err()).count();
            self.doc.apply_new_script("Çoklu değiştir", s);
            self.status = trf!("Çoklu değiştir: {total} değişiklik{}", if errs > 0 { format!(", {errs} kural hatalı (atlandı)") } else { String::new() });
            pv = mr::preview(&self.doc.script, &rules, only.as_deref());
        }
        if open { self.mrep = Some((rules, pv, only_sel)); }
    }

    /// Yayın öncesi kontrol listesi: mevcut denetimlerin özeti; satıra tıklayınca ilgili yere.
    pub(super) fn ready_window(&mut self, ctx: &egui::Context) {
        if !self.ready_open { return; }
        use osisub_core::qc::Kind;
        let mut open = true;
        let video = self.media.info.as_ref().filter(|i| i.has_video).map(|i| (i.width, i.height));
        let head = qc::script_issues(&self.doc.script, video);
        let count = |f: &dyn Fn(&qc::Issue) -> bool| self.issues.iter().filter(|i| f(i)).count();
        let rows: Vec<(&str, usize, &'static str, String)> = vec![
            ("Betik başlığı (PlayRes, YCbCr, ScaledBorder…)", head.len(), "", head.first().cloned().unwrap_or_default()),
            ("Zamanlama (çakışma, süre, titreme, sahne değişimi)", count(&|i| i.kind.category() == "zaman"), "zaman", String::new()),
            ("Metin (okuma hızı, uzunluk, boş, stil)", count(&|i| i.kind.category() == "metin" && i.kind != Kind::Untranslated), "metin", String::new()),
            ("Çevrilmemiş satır", count(&|i| i.kind == Kind::Untranslated), "metin", String::new()),
            ("Etiket hataları / ekran çakışması", count(&|i| i.kind.category() == "etiket"), "etiket", String::new()),
            ("Unutulmuş {not} blokları", count(&|i| i.kind == Kind::Note && i.message.starts_with("Metinde not")), "not", String::new()),
            ("Ad / çeviri tutarsızlığı", count(&|i| i.kind == Kind::Note && (i.message.starts_with("Ad tutarsız") || i.message.starts_with("Tutarsız çeviri"))), "not", String::new()),
            ("Diğer notlar (yazım önerileri, küçük uyarılar)", count(&|i| i.kind == Kind::Note && !i.message.starts_with("Metinde not") && !i.message.starts_with("Ad tutarsız") && !i.message.starts_with("Tutarsız çeviri")), "not", String::new()),
            ("Yazım hataları", if self.spell.is_some() { self.misses.len() } else { usize::MAX }, "", String::new()),
        ];
        let mut goto: Option<&'static str> = None;
        let mut spell = false;
        egui::Window::new(tr("Yayına hazır mı?")).open(&mut open).collapsible(false).default_width(470.0).show(ctx, |ui| {
            egui::Grid::new("ready_grid").num_columns(3).striped(true).show(ui, |ui| {
                for (label, n, cat, hint) in &rows {
                    let (txt, col) = match *n {
                        0 => ("TAMAM".to_string(), Color32::from_rgb(0x5c, 0xc0, 0x6a)),
                        usize::MAX => ("sözlük yok".to_string(), Color32::GRAY),
                        n => (format!("{n} sorun"), theme::BAD),
                    };
                    let r = ui.label(*label);
                    if !hint.is_empty() { r.on_hover_text(hint); }
                    ui.label(RichText::new(txt).strong().color(col));
                    if *n > 0 && *n != usize::MAX {
                        if label.starts_with("Yazım") { if ui.small_button(tr("Göster")).clicked() { spell = true; } }
                        else if ui.small_button(tr("Göster")).clicked() { goto = Some(cat); }
                    } else { ui.label(""); }
                    ui.end_row();
                }
            });
            let fonts_hint = "Yazı tiplerinin kurulu/gömülü olduğunu Araçlar → Yazı tipi denetimi ile; ekran çakışmasını Kalite sekmesindeki düğmeyle denetleyin.";
            ui.label(RichText::new(fonts_hint).small().weak());
            if self.media.path.is_none() { ui.label(RichText::new(tr("Video açık değil: sahne değişimi, titreme ve video süresi denetimleri yapılmadı.")).small().color(theme::WARN)); }
            if self.reference.is_none() { ui.label(RichText::new(tr("Referans altyazı yok: çevrilmemiş/tutarlılık denetimi yalnızca kısmen yapıldı.")).small().weak()); }
        });
        if let Some(c) = goto { self.qc_filter = c; self.tab = Tab::Quality; }
        if spell { self.tab = Tab::Spell; }
        self.ready_open = open;
    }

    /// Kayıt yedekleri (core::backup): karşılaştır / yeni belge olarak aç.
    pub(super) fn backups_window(&mut self, ctx: &egui::Context) {
        if !self.backups_open { return; }
        let mut open = true;
        let mut act: Option<(PathBuf, bool)> = None; // (yol, karşılaştır mı)
        let list = match (&self.doc.path, Self::backup_dir()) {
            (Some(p), Some(d)) => osisub_core::backup::list(&d, p),
            _ => vec![],
        };
        egui::Window::new(tr("Yedekler")).open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Her kayıttan önce dosyanın önceki hali saklanır (dosya başına son 20, UTC zamanı).")).weak());
            if list.is_empty() { ui.label(tr("Bu dosya için yedek yok (ilk kayıttan sonra oluşur).")); }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                egui::Grid::new("bk_grid").num_columns(3).striped(true).show(ui, |ui| {
                    for (p, st) in &list {
                        let pretty = if st.len() >= 15 { format!("{}-{}-{} {}:{}:{}", &st[0..4], &st[4..6], &st[6..8], &st[9..11], &st[11..13], &st[13..15]) } else { st.clone() };
                        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                        ui.label(trf!("{pretty}  ({} KB)", size.div_ceil(1024)));
                        if ui.small_button(tr("Karşılaştır")).on_hover_text(tr("Açık belgeyle farklar")).clicked() { act = Some((p.clone(), true)); }
                        if ui.small_button(tr("Aç")).on_hover_text(tr("Yeni (kaydedilmemiş) belge olarak açar; asıl dosyanın üzerine yazmak için Farklı kaydet")).clicked() { act = Some((p.clone(), false)); }
                        ui.end_row();
                    }
                });
            });
        });
        if let Some((p, cmp)) = act {
            match osisub_core::io::load(&p) {
                Ok(old) if cmp => {
                    let ch = osisub_core::compare::compare(&old, &self.doc.script);
                    self.status = trf!("{} fark", ch.len());
                    self.compare = Some((p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), old, ch));
                }
                Ok(old) => { self.request(Guarded::OpenScript(old, "Yedek açıldı — asıl dosyaya dönmek için Farklı kaydet".into())); open = false; }
                Err(e) => self.status = trf!("Hata: {e}"),
            }
        }
        self.backups_open = open;
    }

    /// Yer imleri listesi: tıkla = satıra git, not düzenle, kaldır.
    pub(super) fn bookmarks_window(&mut self, ctx: &egui::Context) {
        if !self.bookmarks_open { return; }
        let mut open = true;
        let mut jump = None;
        let mut change: Option<(usize, Option<String>)> = None;
        egui::Window::new(tr("Yer imleri")).open(&mut open).collapsible(false).min_width(480.0).show(ctx, |ui| {
            let list = osisub_core::bookmarks::list(&self.doc.script);
            if list.is_empty() {
                ui.label(tr("Yer imi yok. Satırda Ctrl+F2 ile koyun; F2 / Shift+F2 ile gezin. Yer imleri dosyada (Extradata) saklanır."));
                return;
            }
            let pos_of: std::collections::HashMap<usize, usize> = self.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).enumerate().map(|(n, (i, _))| (i, n + 1)).collect();
            if ui.small_button(tr("Listeyi kopyala")).on_hover_text(tr("Satır no, zaman, not ve metin — ekibe göndermek için")).clicked() {
                let txt: String = list.iter().filter_map(|(i, note)| match self.doc.script.entries.get(*i) {
                    Some(Entry::Dialogue(d)) => Some(format!("{} ({}) — {} — {}\n", pos_of.get(i).copied().unwrap_or(0), time::format_ass(d.start_time), if note.is_empty() { "-" } else { note.as_str() }, osisub_core::tags::visible_text(&d.text).replace('\n', " / "))),
                    _ => None,
                }).collect();
                ui.ctx().copy_text(txt);
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                egui::Grid::new("bm_grid").num_columns(4).striped(true).show(ui, |ui| {
                    for (i, note) in &list {
                        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(*i) else { continue };
                        if ui.selectable_label(self.doc.active == *i, format!("{} ({})", pos_of.get(i).copied().unwrap_or(0), time::format_ass(d.start_time))).clicked() { jump = Some(*i); }
                        // düzenleme tamponu karelerden bağımsız tutulur; odak kaybında / Enter'da kaydedilir
                        let editing = self.bookmark_edit.as_ref().is_some_and(|e| e.0 == *i);
                        let mut n = if editing { self.bookmark_edit.as_ref().unwrap().1.clone() } else { note.clone() };
                        let r = ui.add(TextEdit::singleline(&mut n).hint_text(tr("not…")).desired_width(150.0));
                        if r.changed() || r.has_focus() { self.bookmark_edit = Some((*i, n.clone())); }
                        if r.lost_focus() {
                            if n != *note { change = Some((*i, Some(n))); }
                            self.bookmark_edit = None;
                        }
                        ui.add(egui::Label::new(RichText::new(osisub_core::tags::visible_text(&d.text).replace('\n', " ")).weak()).truncate());
                        if ui.small_button(tr("Kaldır")).clicked() { change = Some((*i, None)); }
                        ui.end_row();
                    }
                });
            });
        });
        if let Some(e) = jump { self.doc.select(vec![e], e); self.scroll_to = Some(e); }
        if let Some((i, n)) = change {
            let mut s = self.doc.script.clone();
            osisub_core::bookmarks::set(&mut s, i, n.as_deref());
            self.doc.apply_new_script("Yer imi", s);
        }
        self.bookmarks_open = open;
    }

    /// Saygı ekleri (-san/-kun…): ad başına kullanım tablosu ve toplu kaldırma.
    pub(super) fn honorifics_window(&mut self, ctx: &egui::Context) {
        if !self.honor_open { return; }
        let mut open = true;
        let mut strip = false;
        egui::Window::new(tr("Saygı ekleri")).open(&mut open).collapsible(false).default_width(380.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("Çeviride adlara bitişik Japonca saygı ekleri (Hinata-kun, Yachi-san…). Ekip kararına (ekleri koru / kaldır) uyumu ve tutarlılığı denetlemek için.")).weak());
            let u = osisub_core::honorifics::usage(&self.doc.script);
            if u.is_empty() {
                ui.label(tr("Belgede saygı eki yok."));
            } else {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    egui::Grid::new("honor_grid").num_columns(2).striped(true).show(ui, |ui| {
                        for (name, v) in &u {
                            ui.label(RichText::new(name).strong());
                            let parts: Vec<String> = v.iter().map(|(s, n)| if s.is_empty() { format!("eksiz ×{n}") } else { format!("-{s} ×{n}") }).collect();
                            let mixed = v.len() > 1;
                            ui.label(if mixed { RichText::new(parts.join(", ")).color(theme::WARN) } else { RichText::new(parts.join(", ")) });
                            ui.end_row();
                        }
                    });
                });
                ui.label(RichText::new(tr("Sarı: aynı ad farklı biçimlerde geçiyor (bilinçli olabilir).")).small().weak());
                if ui.button(tr("Tüm ekleri kaldır")).on_hover_text(tr("Diyaloglardaki ada bitişik saygı eklerini siler (yorumlar hariç; tek geri-al adımı)")).clicked() { strip = true; }
            }
        });
        if strip {
            let mut s = self.doc.script.clone();
            let n = osisub_core::honorifics::strip_all(&mut s);
            self.doc.apply_new_script("Saygı eklerini kaldır", s);
            self.status = trf!("{n} satırda saygı eki kaldırıldı");
        }
        self.honor_open = open;
    }

    pub(super) fn stats_window(&mut self, ctx: &egui::Context) {
        let mut open = self.stats_open;
        egui::Window::new(tr("İstatistikler")).open(&mut open).collapsible(false).default_width(360.0).show(ctx, |ui| {
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
            ui.strong(tr("Okuma hızı dağılımı"));
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
            ui.strong(tr("Stiller"));
            for (k, v) in &st.styles { ui.label(format!("{}: {}", k, v)); }
            if !st.actors.is_empty() {
                ui.separator();
                ui.strong(tr("Konuşanlar"));
                for (k, v) in &st.actors { ui.label(format!("{}: {}", k, v)); }
            }
        });
        self.stats_open = open;
    }

    pub(super) fn fixes_window(&mut self, ctx: &egui::Context) {
        let mut open = self.fixes_open;
        egui::Window::new(tr("Sık hataları düzelt")).open(&mut open).collapsible(false).show(ctx, |ui| {
            let f = &mut self.fixes;
            ui.checkbox(&mut f.mojibake, tr("Bozuk Türkçe karakterleri onar (Ã¼ → ü, Ý → İ, þ → ş …)"));
            ui.checkbox(&mut f.double_spaces, tr("Çift boşlukları tek yap"));
            ui.checkbox(&mut f.edge_spaces, tr("Satır başı/sonu boşluklarını sil"));
            ui.checkbox(&mut f.space_before_punct, tr("Noktalamadan önceki boşluğu sil"));
            ui.checkbox(&mut f.space_after_punct, tr("Noktalamadan sonra boşluk ekle"));
            ui.checkbox(&mut f.ellipsis, tr("Üç noktayı … yap"));
            ui.checkbox(&mut f.dialogue_dash, tr("Diyalog çizgisinden sonra boşluk (-Merhaba → - Merhaba)"));
            ui.checkbox(&mut f.sentence_case, tr("Cümle başlarını büyük harf yap (Türkçe i/İ, ı/I)"));
            ui.checkbox(&mut f.remove_hi, tr("İşitme engelli notlarını sil ([ağlar], (müzik), AD: ön ekleri)"));
            ui.checkbox(&mut f.tr_spelling, tr("Türkçe yaygın yazım hataları (herşey → her şey, yada → ya da, şuan → şu an, gelecekmisin → gelecek misin…)"));
            ui.horizontal(|ui| {
                let mut on = f.balance_lines.is_some();
                ui.checkbox(&mut on, tr("Satırları dengele (satır başına karakter)")).on_hover_text(tr("Uzun metni en fazla iki dengeli satıra kırar (noktalamada, bağlaçla bitirmeden); sığan metni tek satır yapar. Tipografi/karaoke satırlarına dokunmaz."));
                let mut v = f.balance_lines.unwrap_or(self.rules.max_line_len.max(20));
                ui.add_enabled(on, egui::DragValue::new(&mut v).range(20..=120));
                f.balance_lines = on.then_some(v);
            });
            ui.horizontal(|ui| {
                let mut on = f.split_long.is_some();
                ui.checkbox(&mut on, tr("Uzun satırları böl (karakter)"));
                let mut v = f.split_long.unwrap_or(84);
                ui.add_enabled(on, egui::DragValue::new(&mut v).range(20..=300));
                f.split_long = on.then_some(v);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.min_dur_on, tr("En kısa süre (ms)"));
                let mut v = f.min_duration_ms.unwrap_or(833);
                ui.add_enabled(self.min_dur_on, egui::DragValue::new(&mut v).speed(10));
                f.min_duration_ms = self.min_dur_on.then_some(v);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.min_gap_on, tr("En küçük boşluk (ms)"));
                let mut v = f.min_gap_ms.unwrap_or(84);
                ui.add_enabled(self.min_gap_on, egui::DragValue::new(&mut v).speed(5));
                f.min_gap_ms = self.min_gap_on.then_some(v);
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.fixes_selection_only, tr("Yalnızca seçim"));
                if ui.button(tr("Uygula")).clicked() {
                    let mut o = self.fixes.clone();
                    o.only = if self.fixes_selection_only { Some(self.doc.selection.clone()) } else { None };
                    let mut s = self.doc.script.clone();
                    let rep = osisub_core::fixes::apply(&mut s, &o);
                    self.doc.apply_new_script("Sık hataları düzelt", s);
                    self.status = trf!("{} satır değişti, {} zamanlama düzeltildi, {} satır bölündü", rep.changed_lines, rep.timing_changes, rep.split_lines);
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
            let rep: Vec<_> = used.into_iter().map(|u| { let f = db.files_for(&u.family); let m = db.missing_chars(&u.family, u.bold, u.italic, u.chars.keys().copied()).unwrap_or_default(); (u, f, m) }).collect();
            let _ = tx.send(rep);
        });
    }

    pub(super) fn fonts_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.font_rx {
            if let Ok(r) = rx.try_recv() { self.font_report = Some(r); self.font_rx = None; }
            else { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
        }
        let mut open = self.fonts_open;
        let mut goto: Option<usize> = None;
        egui::Window::new(tr("Yazı tipi denetimi")).open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            if ui.button(tr("Yenile")).clicked() { self.start_font_scan(); }
            match &self.font_report {
                None => { ui.horizontal(|ui| { ui.spinner(); ui.label(tr("Sistem yazı tipleri taranıyor…")); }); }
                Some(rep) => {
                    let missing = rep.iter().filter(|(_, f, _)| f.is_empty()).count();
                    let glyphs = rep.iter().filter(|(_, _, g)| !g.is_empty()).count();
                    ui.label(if missing == 0 { RichText::new(tr("Tüm yazı tipleri kurulu")).color(Color32::from_rgb(0x7a, 0xd9, 0x8a)) } else { RichText::new(trf!("{} yazı tipi eksik", missing)).color(theme::BAD) });
                    if glyphs > 0 {
                        ui.label(RichText::new(trf!("{glyphs} yazı tipinde kullanılan karakterlerin glifi yok (ör. ğ ş İ ı): oynatıcı başka yazı tipine düşer ya da kutu gösterir. Karaktere tıklayınca ilk geçtiği satıra gidilir.")).color(theme::BAD));
                    }
                    egui::Grid::new("fonts_grid").striped(true).show(ui, |ui| {
                        ui.strong(tr("Aile")); ui.strong(tr("Stil")); ui.strong(tr("Satır")); ui.strong(tr("Durum")); ui.strong(tr("Eksik glif")); ui.end_row();
                        for (u, files, miss) in rep {
                            ui.label(&u.family);
                            ui.label(format!("{}{}", if u.bold { "kalın " } else { "" }, if u.italic { "italik" } else { "" }));
                            ui.label(u.lines.to_string());
                            if files.is_empty() { ui.label(RichText::new(tr("EKSİK")).color(theme::BAD)); } else { ui.label(RichText::new(tr("kurulu")).color(Color32::from_rgb(0x7a, 0xd9, 0x8a))).on_hover_text(files.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n")); }
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                for c in miss.iter().take(20) {
                                    if ui.add(egui::Button::new(RichText::new(c.to_string()).color(theme::BAD)).small()).on_hover_text(format!("U+{:04X}", *c as u32)).clicked() {
                                        if let Some(&e) = u.chars.get(c) { goto = Some(e); }
                                    }
                                }
                                if miss.len() > 20 { ui.label(format!("+{}", miss.len() - 20)); }
                            });
                            ui.end_row();
                        }
                    });
                    if ui.button(tr("Kurulu yazı tiplerini klasöre kopyala…")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let mut n = 0;
                            for (_, files, _) in rep { for f in files { if let Some(name) = f.file_name() { if std::fs::copy(f, dir.join(name)).is_ok() { n += 1; } } } }
                            self.status = trf!("{} yazı tipi dosyası kopyalandı", n);
                        }
                    }
                }
            }
        });
        self.fonts_open = open;
        if let Some(e) = goto {
            self.doc.select(vec![e], e);
            self.scroll_to = Some(e);
        }
    }
}
