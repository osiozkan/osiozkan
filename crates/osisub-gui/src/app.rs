use crate::dialog_ui::{self, DialogState};
use crate::media_ui::Media;
use crate::settings::Settings;
use crate::{style_ui, theme};
use eframe::egui::{self, text::LayoutJob, Align, Color32, Key, KeyboardShortcut, Modifiers, RichText, TextEdit, TextFormat};
use egui_extras::{Column, TableBuilder};
use osisub_core::ass::{Entry, Style};
use osisub_core::qc::{self, Issue, Kind, Rules};
use osisub_core::search::{self, Field};
use osisub_core::timing::{self, PostOptions};
use osisub_core::time;
use osisub_doc::worker::{Cmd, Event, Worker};
use osisub_doc::Document;
use osisub_lua::{MacroInfo, MacroKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Translate,
    Styles,
    Karaoke,
    Tags,
    Spell,
    Quality,
    Automation,
    Info,
}

struct MacroEntry {
    info: MacroInfo,
    script: String,
}

pub struct App {
    doc: Document,
    worker: Worker,
    macros: Vec<MacroEntry>,
    running: Option<String>,
    progress: f32,
    dialog: Option<(String, DialogState)>,
    log: String,
    status: String,
    tab: Tab,
    filter: String,
    rows: Vec<usize>,
    rows_key: (u64, String),
    rules: Rules,
    issues: Vec<Issue>,
    /// Kalite sekmesi süzgeci ("" = tümü, yoksa Kind::category)
    qc_filter: &'static str,
    issue_map: HashMap<usize, Vec<Kind>>,
    issues_key: u64,
    qc_rx: Option<std::sync::mpsc::Receiver<(u64, Vec<Issue>)>>,
    qc_seen_rev: u64,
    qc_changed_at: std::time::Instant,
    start_buf: String,
    end_buf: String,
    style_sel: usize,
    scroll_to: Option<usize>,
    anchor: usize,
    timing_open: bool,
    shift_ms: i64,
    shift_use_frames: bool,
    sync_a: Option<(usize, i64)>,
    sync_b: Option<(usize, i64)>,
    fps_from: f64,
    fps_to: f64,
    shift_frames: i64,
    shift_all: bool,
    /// kaydırma: seçimden sonra başlayan tüm satırlar da
    shift_after: bool,
    fps: f64,
    quit_confirm: bool,
    shot: Option<PathBuf>,
    shot_frame: u32,
    shot_requested: bool,
    shot_delay: Option<f32>,
    started: std::time::Instant,
    shot_done: Arc<AtomicBool>,
    media: Media,
    show_media: bool,
    find_open: bool,
    find: search::Options,
    find_selection_only: bool,
    resample_open: bool,
    res_x: i32,
    res_y: i32,
    post: PostOptions,
    post_selection_only: bool,
    last_autosave: std::time::Instant,
    recovery: Vec<Recovery>,
    guard: Option<Guarded>,
    pending_auto_media: bool,
    glossary: Vec<osisub_core::glossary::Term>,
    gloss_findings: Vec<osisub_core::glossary::Finding>,
    help_open: bool,
    about_open: bool,
    settings: Settings,
    reference: Option<osisub_core::Script>,
    ref_lines: Vec<(i64, i64, String)>,
    fonts_open: bool,
    fixes_open: bool,
    stats_open: bool,
    export_open: bool,
    export_sel: Vec<usize>,
    export_job: Option<ExportJob>,
    tap: Option<TapState>,
    tap_latency_ms: i64,
    backed_up: std::collections::HashSet<PathBuf>,
    theme_dirty: bool,
    goto_open: bool,
    goto_focus: bool,
    /// Sahte video penceresi (açıksa düzenlenen ayarlar; süre dakika olarak ayrı tutulur)
    dummy_win: Option<(osisub_core::dummy::DummyVideo, f64)>,
    /// Kurulu yazı tipi aileleri (ilk gerektiğinde arka planda taranır)
    fonts_list: Option<std::sync::Arc<Vec<String>>>,
    fonts_rx: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    /// Resim altyazısı OCR penceresi ve arka plan işi
    ocr: Option<windows::OcrWin>,
    tr_last_active: usize,
    find_focus: bool,
    goto_buf: String,
    import_styles: Option<Vec<(Style, bool)>>,
    autospell: bool,
    spell: Option<std::sync::Arc<osisub_spell::Speller>>,
    spell_load_rx: Option<std::sync::mpsc::Receiver<Result<osisub_spell::Speller, String>>>,
    spell_status: String,
    misses: Vec<osisub_spell::Miss>,
    miss_rx: Option<std::sync::mpsc::Receiver<Vec<osisub_spell::Miss>>>,
    miss_sel: Option<usize>,
    suggestions: Vec<String>,
    spell_ignore: std::collections::HashSet<String>,
    vad_open: bool,
    vad: osisub_media::VadOptions,
    vad_skip_existing: bool,
    autoplay: bool,
    play_wait: u32,
    loaded_scripts: Vec<PathBuf>,
    /// Belgeye bağlı betikler (Aegisub "Automation Scripts" proje alanı)
    doc_scripts: Vec<PathBuf>,
    fixes: osisub_core::fixes::Fixes,
    fixes_selection_only: bool,
    min_dur_on: bool,
    min_gap_on: bool,
    font_report: Option<Vec<(osisub_core::fonts_used::FontUse, Vec<PathBuf>, Vec<char>)>>,
    font_rx: Option<std::sync::mpsc::Receiver<Vec<(osisub_core::fonts_used::FontUse, Vec<PathBuf>, Vec<char>)>>>,
    /// Kısayol atanmak üzere tuş bekleyen makro (betik adı, makro adı)
    hotkey_capture: Option<(String, String)>,
    /// Son Lua hatası (makro adı, ileti) — hata penceresinde gösterilir
    lua_error: Option<(String, String)>,
    /// Çeviri belleği (<veri>/tm.tsv), değişiklik sayacı, kaydedilmemiş mi
    tm: osisub_core::tm::Memory,
    tm_rev: u64,
    tm_dirty: bool,
    /// (etkin satır, tm_rev) için hesaplanmış öneriler
    tm_cache: (usize, u64, Vec<osisub_core::tm::Suggestion>),
    /// "Satırları seç" penceresi
    select_open: bool,
    select_crit: osisub_core::select::Criteria,
    /// Kare denetimleri için medya durumu (anahtar kare sayısı, kare hızı var mı); değişince QC yenilenir
    qc_media_key: (usize, bool),
    /// Ekler (gömülü yazı tipleri/grafikler) penceresi
    attach_open: bool,
    /// Karşılaştırma: (eski sürüm adı, eski betik, farklar)
    compare: Option<(String, osisub_core::Script, Vec<osisub_core::compare::Change>)>,
    /// Kısayol düzenleyici penceresi (yerleşik komutlar)
    hotkeys_open: bool,
    /// Betik birleştirme penceresi: (eklenecek dosya, seçenekler, kaydırma metni)
    merge_win: Option<(PathBuf, osisub_core::merge::MergeOptions, String)>,
    /// Hareket izleme penceresi
    motion: Option<MotionWin>,
    /// Toplu satır işlemleri penceresi
    batch: Option<BatchWin>,
    /// Tipografi yardımcıları penceresi (degrade, parıltı)
    tsw: Option<TypesetWin>,
    tagw: Option<TagWin>,
    /// Üzerine yapıştır penceresi: (pano metni, alanlar)
    paste_over: Option<(String, [bool; 10])>,
    kanji: Option<KanjiWin>,
    filebatch: Option<FileBatchWin>,
    /// Stil yardımcısı penceresi (süzgeç metni)
    style_assist: Option<String>,
    last_copy: String,
    /// Metin → çizim için yazı tipi dizini (ilk kullanımda taranır)
    font_db: Option<osisub_lua::fonts::FontDb>,
    /// Bir sonraki karede panoya kopyalanacak metin (bağlamı olmayan işlevlerden)
    ctx_copy: Option<String>,
    /// Arka plan video işi (MKV'ye gömme / yakma): (ad, ilerleme 0..1000, iptal, sonuç kanalı)
    job: Option<VideoJob>,
    /// Videodaki altyazı izleri penceresi: (video, izler)
    tracks_win: Option<(PathBuf, Vec<osisub_media::mux::SubTrack>)>,
    /// Komut paleti: (arama, seçili sıra)
    palette: Option<(String, usize)>,
    history_open: bool,
    /// Sesle yeniden zamanlama sonucu (arka plan)
    mt_open: bool,
    /// Makine çevirisi sonucu: (entry dizinleri, sonuç)
    mt_rx: Option<std::sync::mpsc::Receiver<(Vec<usize>, Result<Vec<String>, String>)>>,
    resync_rx: Option<std::sync::mpsc::Receiver<Result<Vec<osisub_media::resync::Group>, String>>>,
    /// Izgara yazım göstergesi önbelleği: (belge sürümü, entry → hatalı sözcük var mı)
    spell_rows: std::cell::RefCell<(u64, std::collections::HashMap<usize, bool>)>,
    /// Düzenleme kutusundaki imleç (satır, karakter konumu) — "imleçte böl" için
    editor_cursor: Option<(usize, usize)>,
    /// Düzenleme kutusundaki seçim: (satır, başlangıç, bitiş) karakter dizinleri (araç çubuğu için)
    editor_sel: Option<(usize, usize, usize)>,
    /// Stil önizlemesi: (anahtar, doku), bekleyen istek, örnek metin
    style_pv: Option<(u64, egui::TextureHandle)>,
    style_pv_rx: Option<std::sync::mpsc::Receiver<(u64, Result<osisub_media::Frame, String>)>>,
    style_pv_text: String,
}

/// Kaydedilmemiş değişiklik varken onay gerektiren işlemler.
#[derive(Clone)]
enum Guarded {
    New,
    OpenDialog,
    OpenPath(PathBuf),
    OpenWithEncoding(String),
    /// Bellekteki betiği yeni (kaydedilmemiş) belge olarak aç (ör. OCR sonucu)
    OpenScript(osisub_core::Script, String),
}

/// Bulunan otomatik kayıt dosyası.
struct Recovery {
    file: PathBuf,
    original: Option<PathBuf>,
    age_min: u64,
}

struct ExportJob {
    queue: Vec<usize>,
    script: osisub_core::Script,
    path: PathBuf,
}

struct TapState {
    line: usize,
    /// Her hecenin başlangıcı (ms, satır başına göre).
    taps: Vec<i64>,
}

enum RowAction {
    InsertBefore,
    InsertAfter,
    Duplicate,
    Delete,
    ToggleComment,
    Merge,
    Copy,
    Cut,
    Paste,
    SetStyle(String),
}

/// Kısayol atanabilen yerleşik komutlar (kimlik, ad). Ayarlarda betik adı `@komut` ile saklanır.
pub(crate) const COMMANDS: &[(&str, &str)] = &[
    ("split_frame", "Video karesinde böl"),
    ("dup_shift", "Çoğalt ve 1 kare kaydır"),
    ("play_line", "Satırı oynat / durdur"),
    ("play_head", "Playhead'den oynat / durdur"),
    ("play_sel", "Ses: seçimi oynat (dolgusuz)"),
    ("play_before", "Ses: başlangıçtan önceki 500 ms"),
    ("play_after", "Ses: bitişten sonraki 500 ms"),
    ("play_begin", "Ses: ilk 500 ms"),
    ("play_end", "Ses: son 500 ms"),
    ("play_stop", "Ses: durdur"),
    ("commit_next", "Sonraki satır (sonuncudaysa yeni satır ekle)"),
    ("set_start", "Başlangıcı video karesine al"),
    ("set_end", "Bitişi video karesine al"),
    ("frame_next", "Sonraki kare"),
    ("frame_prev", "Önceki kare"),
    ("kf_next", "Sonraki anahtar kare"),
    ("kf_prev", "Önceki anahtar kare"),
    ("safe_area", "Güvenli alan kılavuzu aç/kapat"),
    ("style_assist", "Stil yardımcısı…"),
    ("kanji_timer", "Kanji zamanlayıcı…"),
    ("file_batch", "Toplu dosya işlemleri…"),
    ("case_upper", "Harf: BÜYÜK HARF"),
    ("case_lower", "Harf: küçük harf"),
    ("case_title", "Harf: Başlık Biçimi"),
    ("case_sentence", "Harf: Cümle biçimi"),
    ("balance_lines", "Satırları dengele (iki satıra kır / birleştir)"),
    ("unbreak_lines", "Satır sonlarını kaldır"),
    ("line_next", "Sonraki satır"),
    ("line_prev", "Önceki satır"),
    ("select_lines", "Satırları seç…"),
    ("paste_over", "Üzerine yapıştır…"),
    ("merge_keep", "Birleştir (ilkini koru)"),
    ("merge_kara", "Karaoke olarak birleştir"),
    ("perspective", "Perspektife oturt (4 noktalı vektör kırpmadan)"),
    ("split_cursor", "İmleçte böl (süre metne orantılı)"),
    ("split_cursor_frame", "İmleçte böl (video karesinde)"),
    ("snap_scenes", "Sahne değişimlerine yapıştır (≤3 kare)"),
    ("tool_none", "Video aracı: yok"),
    ("tool_pos", "Video aracı: konum"),
    ("tool_move", "Video aracı: hareket"),
    ("tool_clip", "Video aracı: kırpma"),
    ("tool_vclip", "Video aracı: vektör kırpma"),
    ("tool_rotate", "Video aracı: döndürme"),
    ("tool_rotatexy", "Video aracı: 3B döndürme"),
    ("tool_scale", "Video aracı: ölçek"),
    ("tool_persp", "Video aracı: perspektif (4 köşe)"),
    ("tool_draw", "Video aracı: çizim (\\p)"),
];
pub(crate) const COMMAND_SCRIPT: &str = "@komut";

fn tm_path() -> Option<PathBuf> {
    osisub_core::paths::data_dir().map(|d| d.join("tm.tsv"))
}

fn autoload_dir() -> Option<PathBuf> {
    osisub_core::paths::config_dir().map(|d| d.join("automation/autoload"))
}

/// Düzenleme kutusu renklendirmesi: etiket blokları soluk, `\\N` vurgulu, yazım hataları kırmızı altçizgili.
fn editor_job(text: &str, sp: Option<&osisub_spell::Speller>) -> LayoutJob {
    let parts = osisub_core::tags::split(text);
    let visible: String = parts.iter().filter_map(|p| if let osisub_core::tags::Part::Text(t) = p { Some(*t) } else { None }).collect();
    let bad: Vec<(usize, usize)> = match sp {
        Some(sp) => osisub_spell::word_spans(&visible).into_iter().filter(|&(a, b)| osisub_spell::should_flag(sp, &visible[a..b])).collect(),
        None => vec![],
    };
    let font = egui::FontId::monospace(14.0);
    let normal = TextFormat { color: Color32::from_gray(225), font_id: font.clone(), ..Default::default() };
    let tag = TextFormat { color: theme::TAG_COLOR, font_id: font.clone(), ..Default::default() };
    let nl = TextFormat { color: theme::ACCENT, font_id: font.clone(), ..Default::default() };
    let wrong = TextFormat { underline: egui::Stroke::new(1.5, theme::BAD), ..normal.clone() };
    let mut job = LayoutJob::default();
    let mut off = 0usize;
    for p in parts {
        match p {
            osisub_core::tags::Part::Block(b) => job.append(&format!("{{{}}}", b), 0.0, tag.clone()),
            osisub_core::tags::Part::Text(t) => {
                // Hata sınırlarında ve \N konumlarında böl.
                let mut cuts: Vec<usize> = vec![0, t.len()];
                for &(a, b) in &bad {
                    for x in [a, b] { if x > off && x < off + t.len() { cuts.push(x - off); } }
                }
                let mut i = 0;
                while let Some(j) = t[i..].find("\\N") { cuts.push(i + j); cuts.push(i + j + 2); i += j + 2; }
                cuts.sort_unstable();
                cuts.dedup();
                for w in cuts.windows(2) {
                    let seg = &t[w[0]..w[1]];
                    if seg.is_empty() { continue; }
                    let fmt = if seg == "\\N" { &nl } else if bad.iter().any(|&(a, b)| off + w[0] >= a && off + w[1] <= b) { &wrong } else { &normal };
                    job.append(seg, 0.0, fmt.clone());
                }
                off += t.len();
            }
        }
    }
    job
}

mod grid;
mod tabs;
mod windows;

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: crate::Args) -> App {
        let crate::Args { file, scripts, screenshot: shot, tab, media, open_win, line, reference, shot_delay, compare } = args;
        theme::setup(&cc.egui_ctx);
        let worker = Worker::spawn(osisub_core::paths::automation_include_dirs(), vec![]);
        let mut status = String::from("Hazır");
        let doc = match file {
            Some(p) => match Document::open(&p) {
                Ok(d) => d,
                Err(e) => {
                    status = e;
                    Document::new(osisub_core::Script::new_default())
                }
            },
            None => Document::new(osisub_core::Script::new_default()),
        };
        let mut all = scripts;
        let mut user_auto: Vec<PathBuf> = vec![];
        if let Some(d) = autoload_dir() {
            if let Ok(rd) = std::fs::read_dir(d) {
                user_auto = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "lua" || x == "moon")).collect();
                user_auto.sort();
            }
        }
        // Yerleşik Aegisub betikleri (kara-templater); kullanıcı aynı adda dosya koyduysa onunki kullanılır.
        for (name, _) in osisub_lua::BUILTIN_SCRIPTS {
            if !user_auto.iter().any(|p| p.file_name().map_or(false, |f| f == *name)) {
                all.push(PathBuf::from(format!("{}{}", osisub_lua::BUILTIN_PREFIX, name)));
            }
        }
        all.extend(user_auto.iter().cloned());
        let mut settings = Settings::load();
        if settings.hotkey_defaults < 1 {
            crate::hotkeys::apply_defaults(&mut settings.macro_hotkeys, COMMAND_SCRIPT);
            settings.hotkey_defaults = 1;
            settings.save();
        }
        // Kurulu Aegisub'ın autoload betikleri (osisub'da aynı adda betik yoksa)
        if settings.aegisub_autoload {
            for d in osisub_core::paths::aegisub_automation_dirs() {
                let Ok(rd) = std::fs::read_dir(d.join("autoload")) else { continue };
                let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "lua" || x == "moon")).collect();
                v.sort();
                for p in v {
                    let name = p.file_name().map(|f| f.to_os_string());
                    let builtin = osisub_lua::BUILTIN_SCRIPTS.iter().any(|(b, _)| name.as_deref() == Some(std::ffi::OsStr::new(b)));
                    if !builtin && !all.iter().any(|q| q.file_name().map(|f| f.to_os_string()) == name) { all.push(p); }
                }
            }
        }
        for s in &settings.scripts {
            let p = PathBuf::from(s);
            if p.is_file() && !all.contains(&p) { all.push(p); }
        }
        let mut loaded_scripts = vec![];
        for s in all {
            loaded_scripts.push(s.clone());
            worker.send(Cmd::Load(s));
        }
        let mut app = App {
            doc,
            worker,
            macros: vec![],
            running: None,
            progress: 0.0,
            dialog: None,
            log: String::new(),
            status,
            tab: match tab.as_deref() { Some("quality") => Tab::Quality, Some("translate") => Tab::Translate, Some("karaoke") => Tab::Karaoke, Some("tags") => Tab::Tags, Some("spell") => Tab::Spell, Some("automation") => Tab::Automation, Some("info") => Tab::Info, _ => Tab::Styles },
            filter: String::new(),
            hotkey_capture: None,
            lua_error: None,
            select_open: false,
            attach_open: false,
            compare: None,
            hotkeys_open: false,
            merge_win: None,
            motion: None,
            batch: None,
            tsw: None,
            tagw: None,
            paste_over: None,
            style_assist: None,
            kanji: None,
            filebatch: None,
            last_copy: String::new(),
            font_db: None,
            ctx_copy: None,
            job: None,
            tracks_win: None,
            palette: None,
            history_open: false,
            resync_rx: None,
            mt_open: false,
            mt_rx: None,
            editor_cursor: None,
            editor_sel: None,
            style_pv: None,
            style_pv_rx: None,
            style_pv_text: "Aa Iı İi Ğğ Şş Çç Öö Üü 123".into(),
            spell_rows: std::cell::RefCell::new((u64::MAX, Default::default())),
            qc_media_key: (0, false),
            select_crit: Default::default(),
            tm: tm_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| osisub_core::tm::Memory::parse(&t)).unwrap_or_default(),
            tm_rev: 0,
            tm_dirty: false,
            tm_cache: (usize::MAX, u64::MAX, vec![]),
            rows: vec![],
            rows_key: (u64::MAX, String::new()),
            rules: Rules::default(),
            issues: vec![],
            qc_filter: "",
            issue_map: HashMap::new(),
            issues_key: u64::MAX,
            qc_rx: None,
            qc_seen_rev: u64::MAX,
            qc_changed_at: std::time::Instant::now(),
            start_buf: String::new(),
            end_buf: String::new(),
            style_sel: 0,
            scroll_to: None,
            anchor: 0,
            timing_open: false,
            shift_ms: 500,
            shift_use_frames: false,
            sync_a: None,
            sync_b: None,
            fps_from: 25.0,
            fps_to: 23.976,
            shift_frames: 24,
            shift_all: false,
            shift_after: false,
            fps: 23.976,
            quit_confirm: false,
            shot,
            shot_frame: 0,
            shot_requested: false,
            shot_delay,
            started: std::time::Instant::now(),
            shot_done: Arc::new(AtomicBool::new(false)),
            media: Media::new(),
            show_media: false,
            find_open: false,
            find: search::Options::default(),
            find_selection_only: false,
            resample_open: false,
            res_x: 1920,
            res_y: 1080,
            post: PostOptions::default(),
            post_selection_only: true,
            last_autosave: std::time::Instant::now(),
            recovery: vec![],
            guard: None,
            pending_auto_media: false,
            glossary: vec![],
            gloss_findings: vec![],
            help_open: false,
            about_open: false,
            settings,
            reference: None,
            ref_lines: vec![],
            fonts_open: false,
            fixes_open: false,
            stats_open: false,
            export_open: false,
            export_sel: vec![],
            export_job: None,
            tap: None,
            tap_latency_ms: 200,
            backed_up: Default::default(),
            theme_dirty: true,
            goto_open: false,
            dummy_win: None,
            ocr: None,
            fonts_list: None,
            fonts_rx: None,
            goto_focus: false,
            tr_last_active: usize::MAX,
            find_focus: false,
            goto_buf: String::new(),
            import_styles: None,
            autospell: false,
            spell: None,
            spell_load_rx: None,
            spell_status: String::new(),
            misses: vec![],
            miss_rx: None,
            miss_sel: None,
            suggestions: vec![],
            spell_ignore: Default::default(),
            vad_open: false,
            vad: osisub_media::VadOptions::default(),
            vad_skip_existing: true,
            autoplay: false,
            play_wait: 0,
            loaded_scripts,
            doc_scripts: vec![],
            fixes: osisub_core::fixes::Fixes::default(),
            fixes_selection_only: false,
            min_dur_on: false,
            min_gap_on: false,
            font_report: None,
            font_rx: None,
        };
        app.apply_settings();
        app.auto_load_dictionary();
        if !app.settings.glossary_path.is_empty() { app.load_glossary(PathBuf::from(app.settings.glossary_path.clone())); }
        if app.doc.path.is_none() { app.scan_recovery(); }
        if let Some(c) = compare {
            if let Ok(old) = osisub_core::io::load(&c) {
                let ch = osisub_core::compare::compare(&old, &app.doc.script);
                app.compare = Some((c.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), old, ch));
            }
        }
        if let Some(r) = reference {
            if let Ok(sc) = osisub_core::io::load(&r) {
                app.ref_lines = { let mut v: Vec<(i64, i64, String)> = sc.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, osisub_core::tags::visible_text(&d.text).replace('\n', " ⏎ "))).collect(); v.sort_by_key(|x| x.0); v };
                app.reference = Some(sc);
            }
        }
        if let Some(first) = app.first_row() {
            app.doc.select(vec![first], first);
        }
        app.pending_auto_media = app.doc.path.is_some() && media.is_none();
        if let Some(n) = line {
            if let Some(&r) = app.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect::<Vec<_>>().get(n.saturating_sub(1)) {
                app.doc.select(vec![r], r);
                app.scroll_to = Some(r);
            }
        }
        match open_win.as_deref() {
            Some("find") => app.find_open = true,
            Some("timing") => app.timing_open = true,
            Some("resample") => app.resample_open = true,
            Some("dummy") => app.open_dummy_window(),
            Some("ocr") => app.ocr = Some(windows::OcrWin::default()),
            Some("fixes") => app.fixes_open = true,
            Some("stats") => app.stats_open = true,
            Some("vad") => app.vad_open = true,
            Some("spell") => { app.tab = Tab::Spell; app.autospell = true; }
            Some("select") => app.select_open = true,
            Some("attach") => app.attach_open = true,
            Some("hotkeys") => app.hotkeys_open = true,
            Some("motion") => app.open_motion(),
            Some("batch") => app.batch = Some(BatchWin::default()),
            Some("typeset") => app.tsw = Some(TypesetWin::default()),
            Some("tags") => app.tagw = Some(TagWin::default()),
            Some("styleassist") => app.style_assist = Some(String::new()),
            Some("kanji") => app.kanji = Some(KanjiWin::default()),
            Some("filebatch") => app.filebatch = Some(FileBatchWin::default()),
            Some(w) if w.starts_with("filebatch:") => {
                let mut fb = FileBatchWin::default();
                if let Ok(rd) = std::fs::read_dir(&w[10..]) { fb.files = rd.flatten().map(|e| e.path()).filter(|p| osisub_core::io::ext_of(p) == "ass").collect(); fb.files.sort(); }
                app.filebatch = Some(fb);
            }
            Some("safe") => app.media.safe_area = true,
            Some("detach") => app.media.detached = true,
            Some("history") => app.history_open = true,
            Some(w) if w.starts_with("palette") => app.palette = Some((w.strip_prefix("palette:").unwrap_or("").to_string(), 0)),
            Some(w) if w.starts_with("tracks:") => { let v = PathBuf::from(&w[7..]); if let Ok(t) = osisub_media::mux::subtitle_tracks(&v) { app.tracks_win = Some((v, t)); } }
            Some(w) if w.starts_with("motion:") => {
                app.open_motion();
                if let (Some(m), Ok(t)) = (app.motion.as_mut(), std::fs::read_to_string(&w[7..])) { m.text = t; }
            }
            Some(w) if w.starts_with("merge:") => app.merge_win = Some((PathBuf::from(&w[6..]), osisub_core::merge::MergeOptions { sort: true, ..Default::default() }, "0:00:00.00".into())),
            Some("pos") => app.media.tool = crate::media_ui::VideoTool::Pos,
            Some("move") => app.media.tool = crate::media_ui::VideoTool::Move,
            Some("clip") => app.media.tool = crate::media_ui::VideoTool::Clip,
            Some("vclip") => app.media.tool = crate::media_ui::VideoTool::VClip,
            Some("rotate") => app.media.tool = crate::media_ui::VideoTool::Rotate,
            Some("rotatexy") => app.media.tool = crate::media_ui::VideoTool::RotateXY,
            Some("persp") => app.media.tool = crate::media_ui::VideoTool::Perspective,
            Some("draw") => app.media.tool = crate::media_ui::VideoTool::Drawing,
            Some("scale") => app.media.tool = crate::media_ui::VideoTool::Scale,
            Some("spectrum") => app.media.spectrum = true,
            Some("play") => app.autoplay = true,
            Some("fonts") => { app.fonts_open = true; app.start_font_scan(); }
            _ => {}
        }
        if let Some(m) = media {
            app.media.open(m, &cc.egui_ctx);
            app.show_media = true;
        }
        app
    }

    fn apply_settings(&mut self) {
        self.rules = self.settings.rules();
        self.media.snap = self.settings.snap_frames;
        self.media.snap_kf = self.settings.snap_keyframes;
        self.media.play_opts = osisub_media::audio::PlayOpts { speed: self.settings.play_speed, volume: self.settings.volume };
        self.post.lead_in_ms = self.settings.lead_in_ms;
        self.post.lead_out_ms = self.settings.lead_out_ms;
        self.post.adjacent_gap_ms = self.settings.adjacent_gap_ms;
        self.fps = self.settings.fps;
        self.theme_dirty = true;
    }

    fn save_settings(&mut self) {
        let s = &mut self.settings;
        s.max_cps = self.rules.max_cps;
        s.max_line_len = self.rules.max_line_len;
        s.max_lines = self.rules.max_lines;
        s.min_duration_ms = self.rules.min_duration_ms;
        s.max_duration_ms = self.rules.max_duration_ms;
        s.min_gap_ms = self.rules.min_gap_ms;
        s.snap_frames = self.media.snap;
        s.snap_keyframes = self.media.snap_kf;
        s.play_speed = self.media.play_opts.speed;
        s.volume = self.media.play_opts.volume;
        s.lead_in_ms = self.post.lead_in_ms;
        s.lead_out_ms = self.post.lead_out_ms;
        s.adjacent_gap_ms = self.post.adjacent_gap_ms;
        s.fps = self.fps;
        s.save();
    }

    // ------------------------------------------------------------ yazım denetimi

    fn first_row(&self) -> Option<usize> {
        self.doc.script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_)))
    }

    fn title(&self) -> String {
        let name = self.doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Adsız".into());
        format!("{}{} — osisub", name, if self.doc.is_dirty() { " •" } else { "" })
    }

    // ------------------------------------------------------------ dosya

    /// Değişiklik varsa onay iste, yoksa işlemi hemen yap.
    fn request(&mut self, g: Guarded) {
        if self.doc.is_dirty() { self.guard = Some(g); } else { self.run_guarded(g); }
    }

    fn run_guarded(&mut self, g: Guarded) {
        match g {
            Guarded::New => {
                self.doc = Document::new(osisub_core::Script::new_default());
                self.tap = None;
                self.status = "Yeni belge".into();
            }
            Guarded::OpenScript(sc, desc) => {
                self.doc = Document::new(sc);
                self.tap = None;
                self.status = desc;
            }
            Guarded::OpenDialog => self.open_dialog(),
            Guarded::OpenPath(p) => self.open_path(p),
            Guarded::OpenWithEncoding(label) => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                    match osisub_core::io::load_with_encoding(&p, &label) {
                        Ok(sc) => {
                            let mut d = Document::new(sc);
                            d.path = Some(p.clone());
                            // Kaydederken UTF-8 yazılır; içerik değiştiği için kaydedilmemiş say.
                            d.mark_dirty();
                            self.doc = d;
                            if let Some(f) = self.first_row() { self.doc.select(vec![f], f); self.scroll_to = Some(f); }
                            self.status = format!("Açıldı ({}): {} — kaydedince UTF-8 olur", label, p.display());
                        }
                        Err(e) => self.status = format!("Hata: {}", e),
                    }
                }
            }
        }
    }

    fn help_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.help_open;
        egui::Window::new("Kısayollar").open(&mut open).default_width(560.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                for line in include_str!("../../../docs/HOTKEYS.md").lines().skip(2) {
                    let cols: Vec<&str> = line.trim_matches('|').split('|').map(|c| c.trim()).collect();
                    if cols.len() >= 2 && !cols[0].starts_with("---") {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(cols[0]).monospace().color(theme::ACCENT));
                            ui.label(cols[1..].join(" | "));
                        });
                    }
                }
            });
        });
        self.help_open = open;
        let mut open = self.about_open;
        egui::Window::new("Hakkında").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.heading(format!("osisub {}", env!("CARGO_PKG_VERSION")));
            ui.label("Anime altyazı stüdyosu — Aegisub Lua uyumlu, Subtitle Edit'in iyi özellikleriyle.");
            ui.label(RichText::new("Belgeler: docs/KULLANIM.md · docs/LUA_UYUMLULUK.md · docs/ROADMAP.md").weak());
        });
        self.about_open = open;
    }

    fn load_glossary(&mut self, p: PathBuf) {
        match std::fs::read(&p) {
            Ok(b) => {
                self.glossary = osisub_core::glossary::parse(&osisub_core::ass::decode_bytes(&b));
                self.settings.glossary_path = p.to_string_lossy().to_string();
                self.status = format!("Terim sözlüğü: {} terim", self.glossary.len());
            }
            Err(e) => self.status = format!("Terim sözlüğü okunamadı: {}", e),
        }
    }

    fn guard_window(&mut self, ctx: &egui::Context) {
        let Some(g) = self.guard.clone() else { return };
        let mut choice: Option<u8> = None;
        egui::Window::new("Kaydedilmemiş değişiklikler").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("Kaydedilmemiş değişiklikler var. Devam etmeden önce ne yapılsın?");
            ui.horizontal(|ui| {
                if ui.button("Kaydet ve devam").clicked() { choice = Some(0); }
                if ui.button("Kaydetmeden devam").clicked() { choice = Some(1); }
                if ui.button("İptal").clicked() { choice = Some(2); }
            });
        });
        match choice {
            Some(0) => { self.save(false); if !self.doc.is_dirty() { self.guard = None; self.run_guarded(g); } }
            Some(1) => { self.guard = None; self.run_guarded(g); }
            Some(2) => self.guard = None,
            _ => {}
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
            self.open_path(p);
        }
    }

    fn open_path(&mut self, p: PathBuf) {
        match Document::open(&p) {
            Ok(d) => {
                self.doc = d;
                if let Some(f) = self.first_row() {
                    self.doc.select(vec![f], f);
                }
                self.status = format!("Açıldı: {}", p.display());
                self.settings.push_recent(&p.to_string_lossy());
                self.save_settings();
                self.pending_auto_media = true;
            }
            Err(e) => self.status = format!("Hata: {}", e),
        }
    }

    /// Aegisub proje bilgisini (`Video File`, `Audio File`) çözüp varsa ortamı otomatik açar.
    fn auto_media_from_script(&mut self, ctx: &egui::Context) {
        let base = self.doc.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let resolve = |v: &str| -> Option<PathBuf> {
            if v.is_empty() || v.starts_with('?') { return None; }
            let p = PathBuf::from(v);
            if p.is_absolute() { return p.is_file().then_some(p); }
            let joined = base.as_ref()?.join(&p);
            joined.is_file().then_some(joined)
        };
        // Aegisub'ın "Active Line" bilgisini uygula (diyalog sırası, 1 tabanlı).
        if let Some(n) = self.doc.script.garbage("Active Line").and_then(|v| v.parse::<usize>().ok()) {
            let idx = self.doc.script.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).nth(n.saturating_sub(1));
            if let Some(i) = idx { self.doc.select(vec![i], i); self.scroll_to = Some(i); }
        }
        // Belgeye bağlı Lua betikleri (Aegisub "Automation Scripts")
        let base_aut = osisub_core::paths::aegisub_automation_dirs().into_iter().next();
        self.doc_scripts = self.doc.script.garbage("Automation Scripts")
            .map(|v| osisub_core::paths::decode_automation_scripts(v, base.as_deref(), base_aut.as_deref()))
            .unwrap_or_default();
        for p in self.doc_scripts.clone() {
            if !p.is_file() { self.log.push_str(&format!("Belgeye bağlı betik bulunamadı: {}\n", p.display())); continue; }
            if !self.loaded_scripts.contains(&p) { self.loaded_scripts.push(p.clone()); self.worker.send(Cmd::Load(p)); }
        }
        let video = self.doc.script.garbage("Video File").and_then(resolve);
        let audio = self.doc.script.garbage("Audio File").and_then(resolve);
        if video.is_none() {
            if let Some(d) = self.doc.script.garbage("Video File").and_then(osisub_core::dummy::DummyVideo::parse) {
                self.media.open_dummy(&d, ctx);
                self.show_media = true;
                return;
            }
        }
        if let Some(m) = video.or(audio) {
            self.status = format!("Ortam otomatik açıldı: {}", m.display());
            self.media.open(m, ctx);
            self.show_media = true;
        }
    }

    /// Kaydederken Aegisub uyumlu proje bilgisini güncelle (yalnızca ortam açıkken).
    fn write_project_garbage(&mut self, script_path: &std::path::Path) {
        // Garbage satırları olay satırlarından önce eklenir: sonradan dizinleri (etkin satır, seçim) kaydırmak için.
        let before = self.doc.script.entries.len();
        let header_end = self.doc.script.entries.iter().position(|e| !matches!(e.section(), osisub_core::ass::SEC_INFO | osisub_core::ass::SEC_GARBAGE)).unwrap_or(before);
        if !self.doc_scripts.is_empty() || self.doc.script.garbage("Automation Scripts").is_some() {
            let v = osisub_core::paths::encode_automation_scripts(&self.doc_scripts, script_path.parent());
            self.doc.script.set_garbage("Automation Scripts", &v);
        }
        if let Some(m) = self.media.path.clone() {
            let rel = |p: &std::path::Path| -> String {
                script_path.parent().and_then(|d| p.strip_prefix(d).ok()).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string_lossy().to_string())
            };
            let has_video = self.media.info.as_ref().map_or(false, |i| i.has_video);
            if let Some(d) = self.media.dummy.clone() {
                // Sahte video: Aegisub uyumlu `?dummy:` dizesi yazılır (önbellek yolu değil)
                self.doc.script.set_garbage("Video File", &d);
            } else {
                if has_video { self.doc.script.set_garbage("Video File", &rel(&m)); }
                self.doc.script.set_garbage("Audio File", &rel(&m));
            }
            if let Some(pos) = self.rows.iter().position(|&r| r == self.doc.active) {
                self.doc.script.set_garbage("Active Line", &(pos + 1).to_string());
            }
            if let Some(f) = self.media.framerate() {
                self.doc.script.set_garbage("Video Position", &f.frame_at(self.media.playhead).to_string());
            }
        }
        let added = self.doc.script.entries.len() - before;
        if added > 0 {
            let shift = |i: &mut usize| if *i >= header_end { *i += added };
            shift(&mut self.doc.active);
            self.doc.selection.iter_mut().for_each(shift);
            self.doc.rev += 1; // ızgara önbelleği yenilensin
        }
    }

    fn save(&mut self, as_new: bool) {
        let target = if as_new || self.doc.path.is_none() {
            rfd::FileDialog::new().add_filter("ASS", &["ass"]).add_filter("SRT", &["srt"]).add_filter("WebVTT", &["vtt"]).add_filter("LRC", &["lrc"]).add_filter("MicroDVD", &["sub"]).add_filter("SBV", &["sbv"]).add_filter("SAMI", &["smi"]).add_filter("TTML", &["ttml"]).add_filter("iTunes Timed Text", &["itt"]).add_filter("EBU STL", &["stl"]).add_filter("Düz metin", &["txt"]).add_filter("CSV", &["csv"]).save_file()
        } else {
            self.doc.path.clone()
        };
        if let Some(p) = target {
            // Güvenlik: mevcut dosyanın oturumdaki ilk kaydında .bak yedeği al.
            if p.is_file() && self.backed_up.insert(p.clone()) {
                let mut bak = p.clone().into_os_string();
                bak.push(".bak");
                let _ = std::fs::copy(&p, PathBuf::from(bak));
            }
            self.write_project_garbage(&p);
            match self.doc.save_as(p.clone()) {
                Ok(()) => { self.status = format!("Kaydedildi: {}", p.display()); self.settings.push_recent(&p.to_string_lossy()); self.clear_autosave(); self.save_tm(); }
                Err(e) => self.status = format!("Hata: {}", e),
            }
        }
    }

    // ------------------------------------------------------------ Lua

    fn run_macro(&mut self, id: usize) {
        if self.running.is_some() {
            return;
        }
        let Some(m) = self.macros.iter().find(|m| m.info.id == id) else { return };
        self.running = Some(m.info.name.clone());
        let mut props = vec![];
        if let Some(p) = &self.doc.path {
            props.push(("script_file".to_string(), p.to_string_lossy().to_string()));
            if let Some(d) = p.parent() {
                props.push(("script_dir".to_string(), d.to_string_lossy().to_string()));
            }
        }
        if let Some(p) = &self.media.path {
            props.push(("video_file".to_string(), p.to_string_lossy().to_string()));
            props.push(("audio_file".to_string(), p.to_string_lossy().to_string()));
        }
        self.sync_media_state();
        if let Some(f) = self.media.framerate() { props.push(("video_position".to_string(), f.frame_at(self.media.playhead).to_string())); }
        // Aegisub gibi: seçim boşsa etkin satır seçili sayılır.
        let mut selection = self.doc.selection.clone();
        if selection.is_empty() && self.doc.active < self.doc.script.entries.len() { selection.push(self.doc.active); }
        self.worker.send(Cmd::Run { id, script: self.doc.script.clone(), selection, active: self.doc.active, props });
    }

    fn load_lua(&mut self, p: PathBuf) {
        // Kalıcı liste: otomatik yükleme klasöründekiler zaten her açılışta gelir
        let in_autoload = autoload_dir().map_or(false, |d| p.parent() == Some(d.as_path()));
        let ps = p.to_string_lossy().to_string();
        if !in_autoload && !self.settings.scripts.contains(&ps) {
            self.settings.scripts.push(ps);
            self.settings.save();
        }
        if self.loaded_scripts.contains(&p) {
            // aynı betiği iki kez yükleme: motoru yeniden yükle (betik geliştirirken değişiklikleri alır)
            self.reload_scripts();
            return;
        }
        self.loaded_scripts.push(p.clone());
        self.worker.send(Cmd::Load(p));
    }

    /// Motoru sıfırlayıp tüm betikleri yeniden yükler (betik geliştirirken değişiklikleri almak için).
    /// Elle yüklenmiş betiği kalıcı listeden çıkarır ve motoru onsuz yeniden yükler.
    pub(crate) fn remove_script(&mut self, p: &std::path::Path) {
        let ps = p.to_string_lossy().to_string();
        self.settings.scripts.retain(|s| *s != ps);
        self.settings.save();
        self.loaded_scripts.retain(|x| x != p);
        self.reload_scripts();
        self.status = format!("Betik kaldırıldı: {}", p.display());
    }

    fn reload_scripts(&mut self) {
        self.dialog = None;
        self.running = None;
        self.tap = None;
        self.worker = Worker::spawn(osisub_core::paths::automation_include_dirs(), vec![]);
        self.macros.clear();
        let list = self.loaded_scripts.clone();
        for p in list { self.worker.send(Cmd::Load(p)); }
        self.log.push_str("— betikler yeniden yüklendi —\n");
    }

    fn sync_media_state(&mut self) {
        if let Ok(mut m) = self.worker.media.lock() {
            m.fps = self.media.fps();
            m.timecodes = self.media.timecodes.clone();
            m.video_size = self.media.info.as_ref().filter(|i| i.has_video).map(|i| (i.width as i32, i.height as i32));
            m.video = match (&self.media.path, &self.media.info) { (Some(p), Some(i)) if i.has_video => Some((p.clone(), i.clone())), _ => None };
            m.keyframes = self.media.framerate().map(|f| self.media.keyframes.iter().map(|&ms| f.frame_at(ms)).collect()).unwrap_or_default();
            m.audio_selection = match self.doc.script.entries.get(self.doc.active) {
                Some(Entry::Dialogue(d)) => Some((d.start_time, d.end_time)),
                _ => None,
            };
        }
    }

    fn pump_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.worker.events.try_recv() {
            match ev {
                Event::Loaded(Ok((info, ms))) => {
                    for m in ms {
                        self.macros.push(MacroEntry { info: m, script: info.name.clone() });
                    }
                    self.status = format!("Betik yüklendi: {}", info.name);
                }
                Event::Loaded(Err(e)) => {
                    self.log.push_str(&format!("Betik yüklenemedi: {}\n", e));
                    self.status = "Betik yüklenemedi (günlüğe bakın)".into();
                    self.tab = Tab::Automation;
                }
                Event::Dialog(spec) => {
                    let title = self.running.clone().unwrap_or_else(|| "Betik".into());
                    self.dialog = Some((title, DialogState::new(spec)));
                }
                Event::Progress { fraction, .. } => self.progress = fraction as f32,
                Event::Log(m) => self.log.push_str(&m),
                Event::Valid { .. } => {}
                Event::Done(r) => {
                    let name = self.running.take().unwrap_or_default();
                    self.progress = 0.0;
                    match r {
                        Ok(done) if self.export_job.is_some() => {
                            if done.cancelled {
                                self.export_job = None;
                                self.status = format!("{}: iptal edildi; dışa aktarma durdu", name);
                            } else {
                                let mut job = self.export_job.take().unwrap();
                                job.script = done.script;
                                if job.queue.is_empty() {
                                    match osisub_core::io::save(&job.script, &job.path) {
                                        Ok(()) => self.status = format!("Dışa aktarıldı: {}", job.path.display()),
                                        Err(e) => self.status = format!("Hata: {}", e),
                                    }
                                } else {
                                    let next = job.queue.remove(0);
                                    self.export_job = Some(job);
                                    self.run_export_step(next);
                                }
                            }
                        }
                        Ok(done) => {
                            let cancelled = done.cancelled;
                            self.doc.apply_run_done(done, &name);
                            self.status = if cancelled { format!("{}: iptal edildi", name) } else { format!("{}: tamamlandı", name) };
                        }
                        Err((msg, log)) => {
                            self.export_job = None;
                            self.log.push_str(&log);
                            self.log.push_str(&format!("HATA ({}): {}\n", name, msg));
                            self.status = format!("{}: hata (günlüğe bakın)", name);
                            self.tab = Tab::Automation;
                            self.lua_error = Some((name.clone(), msg));
                        }
                    }
                }
            }
        }
        if self.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    // ------------------------------------------------------------ önbellekler

    fn refresh_caches(&mut self) {
        let key = (self.doc.rev, self.filter.clone());
        if self.rows_key != key {
            let f = self.filter.to_lowercase();
            self.rows = self
                .doc
                .script
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| match e {
                    Entry::Dialogue(d) if f.is_empty() || osisub_core::tags::strip_tags(&d.text).to_lowercase().contains(&f) || d.actor.to_lowercase().contains(&f) || d.style.to_lowercase().contains(&f) => Some(i),
                    _ => None,
                })
                .collect();
            self.rows_key = key;
        }
        if self.qc_seen_rev != self.doc.rev {
            self.qc_seen_rev = self.doc.rev;
            self.qc_changed_at = std::time::Instant::now();
        }
        if let Some(rx) = &self.qc_rx {
            if let Ok((rev, issues)) = rx.try_recv() {
                self.qc_rx = None;
                if rev == self.doc.rev {
                    self.set_issues(issues, rev);
                }
            }
        }
        let mkey = (self.media.keyframes.len(), self.media.framerate().is_some());
        if mkey != self.qc_media_key {
            self.qc_media_key = mkey;
            self.issues_key = u64::MAX;
        }
        if self.issues_key != self.doc.rev {
            // Video açıksa kareye dayalı denetimler (sahne değişimi yakınlığı, titreme) da eklenir.
            let frame_ctx = self.media.framerate().map(|fr| (fr, self.media.keyframes.clone()));
            if self.doc.script.entries.len() < 5000 {
                let mut issues = qc::check(&self.doc.script, &self.rules);
                if let Some((fr, kf)) = &frame_ctx {
                    issues.extend(qc::frame_issues(&self.doc.script, fr, kf, 3));
                    issues.sort_by_key(|x| x.entry);
                }
                self.set_issues(issues, self.doc.rev);
            } else if self.qc_rx.is_none() && self.qc_changed_at.elapsed() >= std::time::Duration::from_millis(300) {
                // Büyük belge: düzenleme durunca arka planda hesapla.
                let (tx, rx) = std::sync::mpsc::channel();
                self.qc_rx = Some(rx);
                let (script, rules, rev) = (self.doc.script.clone(), self.rules.clone(), self.doc.rev);
                std::thread::spawn(move || {
                    let mut issues = qc::check(&script, &rules);
                    if let Some((fr, kf)) = &frame_ctx {
                        issues.extend(qc::frame_issues(&script, fr, kf, 3));
                        issues.sort_by_key(|x| x.entry);
                    }
                    let _ = tx.send((rev, issues));
                });
            }
        }
    }

    fn set_issues(&mut self, issues: Vec<Issue>, rev: u64) {
        self.issue_map.clear();
        for i in &issues {
            self.issue_map.entry(i.entry).or_default().push(i.kind.clone());
        }
        self.issues = issues;
        self.issues_key = rev;
    }

    /// Etkin satırı düzenleme kutusundaki imleçten ikiye böler. Süre: görünür metin oranında
    /// (Aegisub "Split at cursor (estimate times)") ya da videodaki karede.
    pub(crate) fn split_at_cursor(&mut self, at_frame: bool) {
        let at = self.doc.active;
        let Some((cl, pos)) = self.editor_cursor.filter(|(l, _)| *l == at) else { self.status = "Önce düzenleme kutusunda imleci konumlandırın".into(); return };
        let _ = cl;
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() else { return };
        let t = if at_frame {
            match self.media.framerate() { Some(fr) => fr.time_at_start(fr.frame_at(self.media.playhead)), None => return }
        } else {
            let before: String = d.text.chars().take(pos).collect();
            let (n0, n) = (osisub_core::tags::visible_len(&before), osisub_core::tags::visible_len(&d.text).max(1));
            let ms = d.start_time + (d.duration() as f64 * n0 as f64 / n as f64).round() as i64;
            // kare hızı biliniyorsa kare sınırına
            self.media.framerate().map_or(ms, |fr| fr.snap(ms))
        };
        if self.doc.split(at, pos, t) {
            self.status = format!("Bölündü: {}", time::format_ass(t));
            self.editor_cursor = None;
        } else {
            self.status = "Bu konumda bölünemez (etiket içi, satır başı/sonu ya da süre dışı)".into();
        }
    }

    /// Yerleşik komutu çalıştırır (kısayoldan).
    pub(crate) fn run_command(&mut self, id: &str, ctx: &egui::Context) {
        use crate::media_ui::VideoTool as T;
        let fr = self.media.framerate();
        let step_line = |app: &mut App, d: isize| {
            if app.rows.is_empty() { return; }
            let p = app.rows.iter().position(|&r| r == app.doc.active).unwrap_or(0) as isize;
            let np = (p + d).clamp(0, app.rows.len() as isize - 1) as usize;
            let r = app.rows[np];
            app.doc.select(vec![r], r);
            app.anchor = np;
            app.scroll_to = Some(r);
        };
        match id {
            "split_frame" => { if let Some(fr) = &fr { if !self.doc.split_at_frame(fr, self.media.playhead) { self.status = "Bu kare satırın içinde değil".into(); } } }
            "dup_shift" => { if let Some(fr) = &fr { self.doc.duplicate_shift_frames(fr, 1); self.scroll_to = Some(self.doc.active); } }
            "play_line" => self.media.toggle_play(&self.doc, ctx),
            "play_head" => self.media.toggle_play_from_playhead(&self.doc, ctx),
            "set_start" => self.media.set_active_start(&mut self.doc),
            "set_end" => self.media.set_active_end(&mut self.doc),
            "frame_next" => self.media.step_frames(1),
            "frame_prev" => self.media.step_frames(-1),
            "kf_next" => { if !self.media.step_keyframe(1) { self.status = "Sonrasında anahtar kare yok".into(); } }
            "kf_prev" => { if !self.media.step_keyframe(-1) { self.status = "Öncesinde anahtar kare yok".into(); } }
            "safe_area" => self.media.safe_area = !self.media.safe_area,
            "style_assist" => self.style_assist = Some(String::new()),
            "kanji_timer" => self.kanji = Some(KanjiWin::default()),
            "file_batch" => self.filebatch = Some(FileBatchWin::default()),
            "case_upper" => self.change_case(osisub_core::casing::Case::Upper),
            "case_lower" => self.change_case(osisub_core::casing::Case::Lower),
            "case_title" => self.change_case(osisub_core::casing::Case::Title),
            "case_sentence" => self.change_case(osisub_core::casing::Case::Sentence),
            "play_sel" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::Selection),
            "play_before" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::Before),
            "play_after" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::After),
            "play_begin" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::Begin),
            "play_end" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::End),
            "play_stop" => self.media.stop(),
            "commit_next" => {
                // Aegisub time/next: son satırdaysa bitişinden başlayan yeni satır
                if self.rows.last() == Some(&self.doc.active) || self.rows.is_empty() {
                    let at = self.doc.insert_after_active();
                    self.scroll_to = Some(at);
                } else {
                    step_line(self, 1);
                }
            }
            "line_next" => step_line(self, 1),
            "line_prev" => step_line(self, -1),
            "select_lines" => self.select_open = true,
            "paste_over" => self.open_paste_over(None),
            "perspective" => self.perspective_fit(),
            "merge_keep" | "merge_kara" => {
                let m = if id == "merge_keep" { osisub_doc::MergeMode::KeepFirst } else { osisub_doc::MergeMode::Karaoke };
                if !self.doc.merge_selected_as(m) { self.status = "Birleştirmek için ardışık en az iki satır seçin".into(); }
            }
            "split_cursor" => self.split_at_cursor(false),
            "balance_lines" => self.rebreak_selected(true),
            "unbreak_lines" => self.rebreak_selected(false),
            "split_cursor_frame" => self.split_at_cursor(true),
            "snap_scenes" => {
                if let Some(fr) = fr.filter(|_| !self.media.keyframes.is_empty()) {
                    let kf = self.media.keyframes.clone();
                    let mut s = self.doc.script.clone();
                    let n = qc::snap_to_keyframes(&mut s, &fr, &kf, 3);
                    self.doc.apply_new_script("Sahne değişimlerine yapıştır", s);
                    self.status = format!("{} satır sahne değişimine yapıştırıldı", n);
                }
            }
            "tool_none" => self.media.tool = T::None,
            "tool_pos" => self.media.tool = T::Pos,
            "tool_move" => self.media.tool = T::Move,
            "tool_clip" => self.media.tool = T::Clip,
            "tool_vclip" => self.media.tool = T::VClip,
            "tool_rotate" => self.media.tool = T::Rotate,
            "tool_rotatexy" => self.media.tool = T::RotateXY,
            "tool_persp" => self.media.tool = T::Perspective,
            "tool_draw" => self.media.tool = T::Drawing,
            "tool_scale" => self.media.tool = T::Scale,
            _ => self.status = format!("Bilinmeyen komut: {}", id),
        }
    }

    /// Çeviri belleğini diske yazar (değiştiyse).
    pub(crate) fn save_tm(&mut self) {
        if !self.tm_dirty { return; }
        if let Some(p) = tm_path() {
            if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
            if std::fs::write(&p, self.tm.to_tsv()).is_ok() { self.tm_dirty = false; }
        }
    }

    /// Referansla zamanı örtüşen satırların (kaynak, çeviri) çiftlerini belleğe ekler.
    pub(crate) fn tm_learn_document(&mut self) -> usize {
        let mut n = 0;
        let ds: Vec<(i64, i64, String)> = self.doc.script.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, d.text.clone())).collect();
        for (a, b, text) in ds {
            if let Some(src) = self.ref_source(a, b) {
                if self.tm.add(&src, &osisub_core::tags::visible_text(&text).replace('\n', "\\N")) { n += 1; }
            }
        }
        if n > 0 { self.tm_rev += 1; self.tm_dirty = true; }
        n
    }

    /// Zaman aralığıyla en çok örtüşen referans satırının metni.
    pub(crate) fn ref_source(&self, a: i64, b: i64) -> Option<String> {
        self.ref_lines.iter().filter(|r| r.1 > a && r.0 < b).max_by_key(|r| r.1.min(b) - r.0.max(a)).map(|r| r.2.replace(" ⏎ ", " "))
    }

    fn recheck(&mut self) {
        self.issues_key = u64::MAX;
    }

    // ------------------------------------------------------------ kısayollar

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input();
        let k = |m: Modifiers, key: Key| KeyboardShortcut::new(m, key);
        // Esc: küçük araç pencerelerini kapat (metin kutusu Esc'yi tüketmeden önce yakala).
        // Makro kısayolu atama: sıradaki tuş birleşimini yakala (Esc = iptal, Backspace = kaldır)
        if let Some((script, name)) = self.hotkey_capture.clone() {
            let pressed = ctx.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } if !crate::hotkeys::is_modifier(*key) => Some((*key, *modifiers)),
                _ => None,
            }));
            if let Some((key, mods)) = pressed {
                self.hotkey_capture = None;
                self.settings.macro_hotkeys.retain(|h| !(h.script == script && h.name == name));
                if key == Key::Escape {
                    self.status = "Kısayol atama iptal edildi".into();
                } else if key == Key::Backspace {
                    self.status = format!("Kısayol kaldırıldı: {}", name);
                } else {
                    let sc = KeyboardShortcut::new(mods, key);
                    let keys = crate::hotkeys::format(&sc);
                    // aynı tuş başka makroda varsa ondan al
                    self.settings.macro_hotkeys.retain(|h| crate::hotkeys::parse(&h.keys) != Some(sc));
                    let label = if script == COMMAND_SCRIPT { COMMANDS.iter().find(|c| c.0 == name).map(|c| c.1.to_string()).unwrap_or_else(|| name.clone()) } else { name.clone() };
                    self.settings.macro_hotkeys.push(crate::settings::MacroHotkey { keys: keys.clone(), script, name: name.clone() });
                    let name = label;
                    self.status = if crate::hotkeys::conflicts_builtin(&sc) { format!("{} → {} (UYARI: yerleşik kısayolu ezer)", keys, name) } else { format!("{} → {}", keys, name) };
                }
                self.settings.save();
                ctx.input_mut(|i| i.events.clear());
            }
            return;
        }
        // Lua makro kısayolları (yerleşiklerden önce: kullanıcı bilerek ezebilir)
        if self.running.is_none() {
            let binds: Vec<(KeyboardShortcut, String, String)> = self.settings.macro_hotkeys.iter().filter_map(|h| Some((crate::hotkeys::parse(&h.keys)?, h.script.clone(), h.name.clone()))).collect();
            let typing_now = ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input();
            for (sc, script, name) in binds {
                // Metin yazarken değiştiricisiz harf kısayolları yazmayı bozmasın (F tuşları hariç)
                let plain = !(sc.modifiers.command || sc.modifiers.ctrl || sc.modifiers.alt);
                let fkey = sc.logical_key.name().starts_with('F') && sc.logical_key.name().len() > 1;
                if typing_now && plain && !fkey {
                    continue;
                }
                if script == COMMAND_SCRIPT {
                    if ctx.input_mut(|i| i.consume_shortcut(&sc)) { self.run_command(&name, ctx); }
                    continue;
                }
                if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                    let id = self.macros.iter().find(|m| m.info.kind == MacroKind::Macro && m.info.name == name && (m.script == script || script.is_empty())).map(|m| m.info.id);
                    match id {
                        Some(id) => self.run_macro(id),
                        None => self.status = format!("Kısayol {}: makro yüklü değil ({})", crate::hotkeys::format(&sc), name),
                    }
                }
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.find_open = false;
            self.goto_open = false;
            self.timing_open = false;
            self.resample_open = false;
            self.fixes_open = false;
            self.stats_open = false;
            self.vad_open = false;
            self.export_open = false;
            self.fonts_open = false;
        }
        if !typing {
            let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
            for ev in events {
                match ev {
                    egui::Event::Copy => { let t = self.doc.copy_lines(); if !t.is_empty() { self.last_copy = t.clone(); ctx.copy_text(t); } }
                    egui::Event::Cut => { let t = self.doc.cut_lines(); if !t.is_empty() { self.last_copy = t.clone(); ctx.copy_text(t); } }
                    egui::Event::Paste(t) => { let n = self.doc.paste_lines(&t); if n > 0 { self.status = format!("{} satır yapıştırıldı", n); self.scroll_to = Some(self.doc.active); } }
                    _ => {}
                }
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            match ext.as_str() {
                e if e != "txt" && osisub_core::io::OPEN_EXTS.contains(&e) => { self.request(Guarded::OpenPath(p)); }
                "lua" | "moon" => self.load_lua(p),
                _ => { self.media.open(p, ctx); self.show_media = true; }
            }
        }
        // Ctrl+Shift+V egui'de Paste olayı üretmez: sistem panosundan okunur
        if !typing && ctx.input_mut(|i| i.consume_shortcut(&k(Modifiers::COMMAND | Modifiers::SHIFT, Key::V))) {
            let t = osisub_doc::sysclip::get().filter(|t| t.contains("Dialogue:") || t.contains("Comment:"));
            self.open_paste_over(t);
        }
        let (open, save, save_as, undo, redo, redo2, dup, ins, merge) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&k(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::Y)),
                i.consume_shortcut(&k(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::D)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::Enter)),
                i.consume_shortcut(&k(Modifiers::COMMAND, Key::M)),
            )
        });
        if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F5)) { self.media.toggle_play_from_playhead(&self.doc, ctx); }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) { self.media.toggle_play(&self.doc, ctx); }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F6)) { self.media.set_active_start(&mut self.doc); }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F7)) { self.media.set_active_end(&mut self.doc); }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::H))) { self.find_open = true; self.find_focus = true; }
        let (alt_up, alt_down) = ctx.input_mut(|i| (i.consume_key(Modifiers::ALT, Key::ArrowUp), i.consume_key(Modifiers::ALT, Key::ArrowDown)));
        if (alt_up || alt_down) && !self.rows.is_empty() {
            let pos = self.rows.iter().position(|&r| r == self.doc.active).unwrap_or(0);
            let np = if alt_up { pos.saturating_sub(1) } else { (pos + 1).min(self.rows.len() - 1) };
            let r = self.rows[np];
            self.doc.select(vec![r], r);
            self.anchor = np;
            self.scroll_to = Some(r);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::G))) { self.goto_open = true; self.goto_focus = true; }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::P))) { self.palette = Some((String::new(), 0)); }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus))) { self.settings.zoom = (self.settings.zoom + 0.1).min(2.5); self.theme_dirty = true; }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus))) { self.settings.zoom = (self.settings.zoom - 0.1).max(0.6); self.theme_dirty = true; }
        if self.tap.is_some() && !typing && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
            self.record_tap();
        }
        if !typing {
            let (prev, next) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Comma), i.consume_key(Modifiers::NONE, Key::Period)));
            if prev || next { self.media.step_frames(if next { 1 } else { -1 }); }
        }
        if open { self.request(Guarded::OpenDialog); }
        if save_as { self.save(true); } else if save { self.save(false); }
        if undo || redo || redo2 {
            if undo { self.doc.undo(); } else { self.doc.redo(); }
            self.scroll_to = Some(self.doc.active);
        }
        if dup { self.doc.duplicate_selected(); }
        if ins { self.doc.insert_after_active(); self.scroll_to = Some(self.doc.active); }
        if merge { self.doc.merge_selected(); }
        if !typing {
            let (up, down, pgup, pgdn, del, all) = ctx.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::PageUp),
                    i.consume_key(Modifiers::NONE, Key::PageDown),
                    i.consume_key(Modifiers::NONE, Key::Delete),
                    i.consume_shortcut(&k(Modifiers::COMMAND, Key::A)),
                )
            });
            let step = if pgup || pgdn { 10 } else { 1 };
            if (up || pgup || down || pgdn) && !self.rows.is_empty() {
                let pos = self.rows.iter().position(|&r| r == self.doc.active).unwrap_or(0);
                let np = if up || pgup { pos.saturating_sub(step) } else { (pos + step).min(self.rows.len() - 1) };
                let r = self.rows[np];
                self.doc.select(vec![r], r);
                self.anchor = np;
                self.scroll_to = Some(r);
            }
            if del { self.doc.delete_selected(); }
            if all { let v = self.rows.clone(); self.doc.select(v, self.doc.active); }
        }
    }

    // ------------------------------------------------------------ çizim

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Dosya", |ui| {
                if ui.add(egui::Button::new("Yeni").shortcut_text("")).clicked() { ui.close(); self.request(Guarded::New); }
                if ui.add(egui::Button::new("Aç…").shortcut_text("Ctrl+O")).clicked() { ui.close(); self.request(Guarded::OpenDialog); }
                ui.menu_button("Kodlamayla aç", |ui| {
                    for (label, name) in osisub_core::io::ENCODINGS {
                        if ui.button(*name).clicked() { ui.close(); self.request(Guarded::OpenWithEncoding(label.to_string())); }
                    }
                });
                ui.menu_button("Son açılanlar", |ui| {
                    let recent = self.settings.recent.clone();
                    if recent.is_empty() { ui.label(RichText::new("Yok").weak()); }
                    for r in recent {
                        if ui.button(&r).clicked() { ui.close(); self.request(Guarded::OpenPath(PathBuf::from(&r))); }
                    }
                });
                if ui.button("Toplu dosya işlemleri…").on_hover_text("Birden çok dosyayı dönüştür / düzelt / kaydır / yeniden örnekle (Subtitle Edit: Batch convert)").clicked() { ui.close(); self.filebatch = Some(FileBatchWin::default()); }
                if ui.button("Başka dosyayı birleştir…").on_hover_text("Tabela / OP-ED / diyalog dosyalarını bu belgeye ekler: stil çakışmaları çözülür, farklı çözünürlük yeniden örneklenir, gömülü yazı tipleri taşınır").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        self.merge_win = Some((p, osisub_core::merge::MergeOptions { sort: true, ..Default::default() }, "0:00:00.00".into()));
                    }
                }
                if ui.button("Başka sürümle karşılaştır…").on_hover_text("Açık belgeyi eski/başka bir sürümle karşılaştır (eklenen, silinen, değişen satırlar)").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        match osisub_core::io::load(&p) {
                            Ok(old) => {
                                let ch = osisub_core::compare::compare(&old, &self.doc.script);
                                self.status = format!("{} fark", ch.len());
                                self.compare = Some((p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), old, ch));
                            }
                            Err(e) => self.status = format!("Hata: {}", e),
                        }
                    }
                }
                if ui.button("Referans altyazı aç…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        match osisub_core::io::load(&p) {
                            Ok(s) => { self.ref_lines = { let mut v: Vec<(i64, i64, String)> = s.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, osisub_core::tags::visible_text(&d.text).replace('\n', " ⏎ "))).collect(); v.sort_by_key(|x| x.0); v }; self.reference = Some(s); self.status = format!("Referans: {}", p.display()); }
                            Err(e) => self.status = format!("Hata: {}", e),
                        }
                    }
                }
                if self.reference.is_some() && ui.button("Referansı kapat").clicked() { self.reference = None; self.ref_lines.clear(); ui.close(); }
                ui.separator();
                if ui.add(egui::Button::new("Kaydet").shortcut_text("Ctrl+S")).clicked() { ui.close(); self.save(false); }
                if ui.add(egui::Button::new("Farklı kaydet…").shortcut_text("Ctrl+Shift+S")).clicked() { ui.close(); self.save(true); }
                if ui.button("Dışa aktar (filtreler)…").clicked() { ui.close(); self.export_open = true; }
                ui.menu_button("Video ile", |ui| {
                    if ui.button("Resim altyazısını metne çevir (PGS OCR)…").on_hover_text("Blu-ray PGS (.sup ya da MKV izi) → metin; Tesseract gerekir").clicked() {
                        ui.close();
                        self.ocr = Some(windows::OcrWin::default());
                    }
                    if ui.button("Videodaki altyazı izleri…").on_hover_text("MKV/MP4 içindeki altyazıyı aç; ekli yazı tiplerini çıkar").clicked() {
                        ui.close();
                        let v = self.media.path.clone().or_else(|| rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file());
                        if let Some(v) = v {
                            match osisub_media::mux::subtitle_tracks(&v) {
                                Ok(t) => self.tracks_win = Some((v, t)),
                                Err(e) => self.status = e,
                            }
                        }
                    }
                    let busy = self.job.is_some();
                    if ui.add_enabled(!busy, egui::Button::new("MKV'ye göm (softsub)…")).on_hover_text("Açık videoya bu altyazıyı iz olarak ekler; kullanılan yazı tipleri de eklenir. Yeniden kodlama yok.").clicked() { ui.close(); self.start_video_job(false); }
                    if ui.add_enabled(!busy, egui::Button::new("Videoya yak (hardsub)…")).on_hover_text("x264 ile yeniden kodlar (CRF 18); ses kopyalanır").clicked() { ui.close(); self.start_video_job(true); }
                });
                ui.separator();
                if ui.button("Çıkış").clicked() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close); }
            });
            ui.menu_button("Düzen", |ui| {
                if ui.add_enabled(self.doc.can_undo(), egui::Button::new(format!("Geri al{}", self.doc.undo_desc().map(|d| format!(": {}", d)).unwrap_or_default())).shortcut_text("Ctrl+Z")).clicked() { self.doc.undo(); ui.close(); }
                if ui.add_enabled(self.doc.can_redo(), egui::Button::new("Yinele").shortcut_text("Ctrl+Y")).clicked() { self.doc.redo(); ui.close(); }
                ui.separator();
                if ui.button("Geçmiş…").on_hover_text("Geri alma geçmişi: bir adıma tıklayarak o duruma dönün").clicked() { ui.close(); self.history_open = true; }
                ui.menu_button("Harf dönüştür", |ui| {
                    use osisub_core::casing::Case;
                    for (c, l) in [(Case::Upper, "BÜYÜK HARF"), (Case::Lower, "küçük harf"), (Case::Title, "Başlık Biçimi"), (Case::Sentence, "Cümle biçimi.")] {
                        if ui.button(l).on_hover_text("Seçili satırlar; etiketler ve \\N korunur, Türkçe i/İ ı/I").clicked() { ui.close(); self.change_case(c); }
                    }
                });
                if ui.button("Satırları dengele").on_hover_text("Seçili satırları satır başına en çok 'en uzun satır' sınırı kadar karakterle dengeli iki satıra kırar; sığanları tek satır yapar (Subtitle Edit: Auto balance)").clicked() { ui.close(); self.rebreak_selected(true); }
                if ui.button("Satır sonlarını kaldır").on_hover_text("Seçili satırlardaki \\N'leri boşluğa çevirir").clicked() { ui.close(); self.rebreak_selected(false); }
                if ui.button("Stil yardımcısı…").on_hover_text("Satır satır stil atama: süz, Enter = uygula + sonraki satır (Aegisub: Styling Assistant)").clicked() { ui.close(); self.style_assist = Some(String::new()); }
                if ui.button("Üzerine yapıştır…  Ctrl+Shift+V").on_hover_text("Panodaki satırların seçtiğiniz alanlarını (zaman, stil, metin…) seçili satırların üzerine yazar (Aegisub: Paste Over)").clicked() { ui.close(); self.open_paste_over(None); }
                if ui.button("Satırları seç…").on_hover_text("Stil, konuşan, efekt ya da metne göre seç (Aegisub: Select Lines)").clicked() { self.select_open = true; ui.close(); }
                if ui.add(egui::Button::new("Satır ekle").shortcut_text("Ctrl+Enter")).clicked() { self.doc.insert_after_active(); self.scroll_to = Some(self.doc.active); ui.close(); }
                if ui.add(egui::Button::new("Çoğalt").shortcut_text("Ctrl+D")).clicked() { self.doc.duplicate_selected(); ui.close(); }
                if ui.add(egui::Button::new("Sil").shortcut_text("Del")).clicked() { self.doc.delete_selected(); ui.close(); }
                if ui.add(egui::Button::new("Seçilenleri birleştir").shortcut_text("Ctrl+M")).clicked() { self.doc.merge_selected(); ui.close(); }
                if ui.button("Birleştir (ilkini koru)").on_hover_text("Zaman aralığı birleşir, metin ilk satırınki kalır").clicked() { self.run_command("merge_keep", ui.ctx()); ui.close(); }
                if ui.button("Karaoke olarak birleştir").on_hover_text("Her satır bir hece olur; \\k = satırın süresi").clicked() { self.run_command("merge_kara", ui.ctx()); ui.close(); }
                if ui.button("Satırları dengele").clicked() { self.doc.balance_selected(self.rules.max_line_len); ui.close(); }
            });
            ui.menu_button("Zamanlama", |ui| {
                if ui.button("Kaydır / kareye oturt…").clicked() { self.timing_open = true; ui.close(); }
                if ui.add_enabled(self.resync_rx.is_none(), egui::Button::new("Sesle yeniden zamanla (TV → BD)…")).on_hover_text("Altyazı eski videoya göre zamanlanmışsa: açık (yeni) videonun sesiyle eşleyip her satır grubunu kaydırır (farklı kurgu, eklenen/çıkarılan sahneler, gecikme)").clicked() { ui.close(); self.start_resync(); }
                if ui.button("Toplu satır işlemleri…").on_hover_text("Aynı metinli/kısa satırları birleştir, süreleri okuma hızına göre hesapla (Subtitle Edit)").clicked() { self.batch = Some(BatchWin::default()); ui.close(); }
                if ui.button("Son-işleyici (lead-in/out, anahtar kare)…").clicked() { self.timing_open = true; ui.close(); }
                if ui.add_enabled(self.media.waveform().is_some(), egui::Button::new("Sesten satır oluştur…")).clicked() { self.vad_open = true; ui.close(); }
                ui.separator();
                let fr = self.media.framerate();
                if ui.add_enabled(fr.is_some(), egui::Button::new("Video karesinde böl")).on_hover_text("Etkin satırı videodaki karede ikiye böler (Aegisub: Split at video frame)").clicked() {
                    ui.close();
                    if let Some(fr) = &fr {
                        if !self.doc.split_at_frame(fr, self.media.playhead) { self.status = "Bu kare satırın içinde değil".into(); }
                    }
                }
                if ui.add_enabled(fr.is_some(), egui::Button::new("Çoğalt ve 1 kare kaydır")).on_hover_text("Kare kare tipografi: seçili satırların kopyası bir kare sonra").clicked() {
                    ui.close();
                    if let Some(fr) = &fr { self.doc.duplicate_shift_frames(fr, 1); self.scroll_to = Some(self.doc.active); }
                }
                ui.menu_button("Sırala", |ui| {
                    for (k, l) in [(qc::SortKey::Start, "Başlangıç zamanına göre"), (qc::SortKey::End, "Bitiş zamanına göre"), (qc::SortKey::Style, "Stile göre"), (qc::SortKey::Actor, "Konuşana göre"), (qc::SortKey::Effect, "Efekte göre"), (qc::SortKey::Layer, "Katmana göre"), (qc::SortKey::Text, "Metne göre")] {
                        if ui.button(l).clicked() {
                            let mut s = self.doc.script.clone();
                            qc::sort_by(&mut s, k);
                            self.doc.apply_new_script("Sırala", s);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Görünüm", |ui| {
                let mut light = self.settings.light_theme;
                if ui.checkbox(&mut light, "Açık tema").changed() { self.settings.light_theme = light; self.theme_dirty = true; }
                if ui.add(egui::Button::new("Yakınlaştır").shortcut_text("Ctrl +")).clicked() { self.settings.zoom = (self.settings.zoom + 0.1).min(2.5); self.theme_dirty = true; }
                if ui.add(egui::Button::new("Uzaklaştır").shortcut_text("Ctrl -")).clicked() { self.settings.zoom = (self.settings.zoom - 0.1).max(0.6); self.theme_dirty = true; }
                if ui.button("Yakınlaştırmayı sıfırla").clicked() { self.settings.zoom = 1.0; self.theme_dirty = true; }
                ui.separator();
                ui.menu_button("Izgara sütunları", |ui| {
                    // Göster/gizle ve sırala (# ilk, Metin son sütun sabit)
                    let mut ch = false;
                    let cols = self.settings.grid_columns();
                    let n = cols.len();
                    egui::Grid::new("grid_cols_menu").num_columns(3).spacing([6.0, 2.0]).show(ui, |ui| {
                        for (i, c) in cols.into_iter().enumerate() {
                            let mut on = self.settings.grid_visible(c);
                            if ui.checkbox(&mut on, c.label()).changed() { self.settings.set_grid_visible(c, on); ch = true; }
                            if ui.add_enabled(i > 0, egui::Button::new("Yukarı").small()).clicked() { self.settings.move_grid_column(c, -1); ch = true; }
                            if ui.add_enabled(i + 1 < n, egui::Button::new("Aşağı").small()).clicked() { self.settings.move_grid_column(c, 1); ch = true; }
                            ui.end_row();
                        }
                    });
                    if ui.button("Varsayılana dön").clicked() {
                        let d = Settings::default();
                        self.settings.grid_order = d.grid_order;
                        self.settings.grid_hidden = d.grid_hidden;
                        self.settings.grid_layer = d.grid_layer;
                        self.settings.grid_effect = d.grid_effect;
                        self.settings.grid_margins = d.grid_margins;
                        ch = true;
                    }
                    if ch { self.settings.save(); }
                });
                ui.separator();
                if ui.add(egui::Button::new("Zamana git…").shortcut_text("Ctrl+G")).clicked() { self.goto_open = true; self.goto_focus = true; ui.close(); }
            });
            ui.menu_button("Video", |ui| {
                if ui.button("Video / ses aç…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Ortam", &["mkv", "mp4", "avi", "webm", "mov", "mka", "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"]).pick_file() {
                        self.media.open(p, ui.ctx());
                        self.show_media = true;
                    }
                }
                if ui.button("Sahte video aç…").on_hover_text("Videosuz zamanlama/tipografi için düz renkli video (Aegisub dummy video; projeye kaydedilir)").clicked() {
                    ui.close();
                    self.open_dummy_window();
                }
                ui.checkbox(&mut self.show_media, "Medya panelini göster");
                ui.menu_button("En-boy oranı", |ui| {
                    let cur = self.media.ar_override;
                    let auto = self.media.info.as_ref().map(|i| i.display_aspect()).unwrap_or(16.0 / 9.0);
                    let anam = self.media.info.as_ref().map_or(false, |i| i.dar > 0.0);
                    let mut set = |ui: &mut egui::Ui, v: Option<f64>, l: &str| {
                        if ui.radio(cur.map(|c| (c * 1000.0).round()) == v.map(|c| (c * 1000.0).round()), l).clicked() { self.media.ar_override = v; self.media.invalidate_frames(); ui.close(); }
                    };
                    set(ui, None, &format!("Otomatik ({:.3}:1{})", auto, if anam { ", anamorfik" } else { "" }));
                    set(ui, Some(4.0 / 3.0), "4:3");
                    set(ui, Some(16.0 / 9.0), "16:9");
                    set(ui, Some(1.85), "1.85:1");
                    set(ui, Some(2.35), "2.35:1");
                });
                ui.checkbox(&mut self.media.safe_area, "Güvenli alan kılavuzu").on_hover_text("Eylem güvenli (%90) ve başlık güvenli (%80) alan çerçeveleri; TV kırpmasına karşı tabela yerleşimi için");
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new("< Önceki anahtar kare")).clicked() { self.media.step_keyframe(-1); }
                    if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new("Sonraki anahtar kare >")).clicked() { self.media.step_keyframe(1); }
                });
                ui.checkbox(&mut self.media.detached, "Videoyu ayrı pencerede göster").on_hover_text("Çift monitör için (Aegisub: Detach video); pencereyi kapatınca panele döner");
                if ui.add_enabled(self.media.video_zoom > 1.0, egui::Button::new(format!("Video yakınlaştırmasını sıfırla ({:.0}%)", self.media.video_zoom * 100.0))).on_hover_text("Videoda Ctrl+tekerlek: yakınlaştır, orta tuş: kaydır, orta çift tık: sıfırla").clicked() { self.media.video_zoom = 1.0; self.media.video_pan = egui::Vec2::ZERO; ui.close(); }
                if ui.button("Anahtar kareleri yükle…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Anahtar kare", &["txt", "log", "stats", "keyframes"]).pick_file() {
                        match std::fs::read_to_string(&p) {
                            Ok(t) => {
                                self.media.keyframes = osisub_media::parse_keyframe_file(&t, self.media.fps().unwrap_or(23.976));
                                self.status = format!("{} anahtar kare yüklendi", self.media.keyframes.len());
                            }
                            Err(e) => self.status = format!("Hata: {}", e),
                        }
                    }
                }
                if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new("Anahtar kareleri kaydet…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Anahtar kare", &["txt"]).save_file() {
                        let t = osisub_media::write_keyframe_file(&self.media.keyframes, self.media.fps().unwrap_or(23.976));
                        match std::fs::write(&p, t) { Ok(()) => self.status = "Anahtar kareler kaydedildi".into(), Err(e) => self.status = format!("Hata: {}", e) }
                    }
                }
                ui.separator();
                if ui.button("Timecode dosyası aç (VFR)…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Timecode", &["txt", "tc"]).pick_file() {
                        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| osisub_core::vfr::Framerate::parse_timecodes(&t)) {
                            Ok(fr) => {
                                self.status = format!("Timecode yüklendi: ortalama {:.3} fps", fr.fps());
                                self.media.timecodes = Some(std::sync::Arc::new(fr));
                                self.media.timecodes_auto = false;
                            }
                            Err(e) => self.status = format!("Timecode hatası: {}", e),
                        }
                    }
                }
                if ui.add_enabled(self.media.framerate().is_some(), egui::Button::new("Timecode kaydet (v2)…")).clicked() {
                    ui.close();
                    if let (Some(fr), Some(p)) = (self.media.framerate(), rfd::FileDialog::new().add_filter("Timecode", &["txt"]).save_file()) {
                        // Kare sayısı: videonun süresi ya da son altyazı bitişi
                        let end = self.media.info.as_ref().map(|i| i.duration_ms).filter(|d| *d > 0).unwrap_or_else(|| self.doc.script.dialogues().map(|d| d.end_time).max().unwrap_or(0));
                        let frames = (fr.frame_at(end) + 1).max(1) as usize;
                        match std::fs::write(&p, fr.to_v2(frames)) { Ok(()) => self.status = format!("{} kare için timecode kaydedildi", frames), Err(e) => self.status = format!("Hata: {}", e) }
                    }
                }
                if ui.add_enabled(self.media.timecodes.is_some(), egui::Button::new("Timecode'u kapat")).clicked() {
                    ui.close();
                    self.media.timecodes = None;
                    self.media.timecodes_auto = false;
                    self.status = "Timecode kapatıldı (videonun sabit kare hızı kullanılıyor)".into();
                }
                ui.separator();
                ui.menu_button(format!("Oynatma hızı ({}x)", self.media.play_opts.speed), |ui| {
                    for sp in [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
                        if ui.radio(self.media.play_opts.speed == sp, format!("{sp}x")).clicked() {
                            self.media.play_opts.speed = sp;
                            self.save_settings();
                            ui.close();
                        }
                    }
                    ui.label(RichText::new("Perde korunur; zamanlamada yavaş dinlemek için").small().weak());
                });
                ui.horizontal(|ui| {
                    ui.label("Ses");
                    let mut v = (self.media.play_opts.volume * 100.0) as f32;
                    if ui.add(egui::Slider::new(&mut v, 0.0..=200.0).suffix("%").integer()).drag_stopped() {
                        self.media.play_opts.volume = v as f64 / 100.0;
                        self.save_settings();
                    } else {
                        self.media.play_opts.volume = v as f64 / 100.0;
                    }
                });
                ui.checkbox(&mut self.media.spectrum, "Spektrum göster");
                ui.checkbox(&mut self.media.snap, "Karelere yapış");
                ui.checkbox(&mut self.media.snap_kf, "Anahtar karelere yapış");
                ui.menu_button("Video aracı", |ui| {
                    use crate::media_ui::VideoTool as T;
                    for (t, l) in [(T::None, "Yok"), (T::Pos, "Konum (\\pos sürükle)"), (T::Move, "Hareket (\\move başlangıç/bitiş; sağ tık = \\pos)"), (T::Clip, "Kırpma (\\clip dikdörtgen; sağ tık = kaldır)"), (T::VClip, "Vektör kırpma (\\clip çizim: tıkla = nokta, sürükle, sağ tık = sil)"), (T::Rotate, "Döndürme (\\frz; Shift = 15°)"), (T::RotateXY, "3B döndürme (\\frx/\\fry)"), (T::Scale, "Ölçek (\\fscx/\\fscy)"), (T::Perspective, "Perspektif (4 köşeyi sürükle)"), (T::Drawing, "Çizim (\\p noktaları; \\an7)")] {
                        if ui.radio_value(&mut self.media.tool, t, l).clicked() { ui.close(); }
                    }
                });
                if ui.add_enabled(self.media.is_open(), egui::Button::new("Satırı oynat / durdur").shortcut_text("F5")).clicked() { self.media.toggle_play(&self.doc, ui.ctx()); ui.close(); }
                if ui.add_enabled(self.media.is_open(), egui::Button::new("Playhead'den oynat / durdur").shortcut_text("Shift+F5")).clicked() { self.media.toggle_play_from_playhead(&self.doc, ui.ctx()); ui.close(); }
            });
            ui.menu_button("Otomasyon", |ui| {
                if self.macros.is_empty() { ui.label(RichText::new("Yüklü makro yok").weak()); }
                let list: Vec<(usize, String, String)> = self.macros.iter().filter(|m| m.info.kind == MacroKind::Macro).map(|m| (m.info.id, m.info.name.clone(), m.info.description.clone())).collect();
                // Aegisub gibi: "Grup/Alt/Makro" adları iç içe alt menülere ayrılır.
                let mut clicked = None;
                macro_menu(ui, &list, "", self.running.is_none(), &mut clicked);
                if let Some(id) = clicked { ui.close(); self.run_macro(id); }
                ui.separator();
                if ui.button("Lua betiği yükle…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Lua / MoonScript", &["lua", "moon"]).pick_file() { self.load_lua(p); }
                }
                if ui.button("Betikleri yeniden yükle").clicked() { ui.close(); self.reload_scripts(); }
            });
            ui.menu_button("Araçlar", |ui| {
                if ui.add(egui::Button::new("Bul / Değiştir…").shortcut_text("Ctrl+F")).clicked() { self.find_open = true; self.find_focus = true; ui.close(); }
                if ui.button("Terim sözlüğü yükle…").on_hover_text("Her satır: kaynak = hedef").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Metin/CSV", &["txt", "csv", "tsv"]).pick_file() { self.load_glossary(p); }
                }
                ui.menu_button(format!("Çeviri belleği ({} çift)", self.tm.len()), |ui| {
                    if ui.add_enabled(!self.ref_lines.is_empty(), egui::Button::new("Bu belgeyi belleğe ekle")).on_hover_text("Referansla örtüşen her satır: referans metni → çeviri").clicked() {
                        ui.close();
                        let n = self.tm_learn_document();
                        self.save_tm();
                        self.status = format!("Çeviri belleğine {} çift eklendi/güncellendi (toplam {})", n, self.tm.len());
                    }
                    if ui.button("İçe aktar (TSV)…").clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("TSV", &["tsv", "txt"]).pick_file() {
                            match std::fs::read_to_string(&p) {
                                Ok(t) => {
                                    let before = self.tm.len();
                                    self.tm.merge(osisub_core::tm::Memory::parse(&t));
                                    self.tm_rev += 1; self.tm_dirty = true; self.save_tm();
                                    self.status = format!("Çeviri belleği: {} → {} çift", before, self.tm.len());
                                }
                                Err(e) => self.status = format!("Hata: {}", e),
                            }
                        }
                    }
                    if ui.add_enabled(!self.tm.is_empty(), egui::Button::new("Dışa aktar (TSV)…")).clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("TSV", &["tsv"]).save_file() {
                            match std::fs::write(&p, self.tm.to_tsv()) { Ok(()) => self.status = "Çeviri belleği dışa aktarıldı".into(), Err(e) => self.status = format!("Hata: {}", e) }
                        }
                    }
                });
                if ui.button("Ekler (gömülü yazı tipleri)…").on_hover_text("[Fonts]/[Graphics]: dosyaları ASS'e göm, çıkar, sil").clicked() { self.attach_open = true; ui.close(); }
                if ui.button("İstatistikler…").clicked() { self.stats_open = true; ui.close(); }
                if ui.button("Sık hataları düzelt…").clicked() { self.fixes_open = true; ui.close(); }
                if ui.button("Yazı tipi denetimi…").clicked() { self.fonts_open = true; self.start_font_scan(); ui.close(); }
                if ui.button("Perspektife oturt").on_hover_text("Etkin satıra 4 noktalı vektör kırpma çizin (sol üst, sağ üst, sağ alt, sol alt); satır o dörtgene \\frx\\fry\\frz\\fscx\\fscy\\fax\\org ile oturtulur").clicked() { ui.close(); self.perspective_fit(); }
                ui.menu_button("Metin → çizim", |ui| {
                    if ui.button("Seçili satırları çizime (\\p1) dönüştür").on_hover_text("Gerçek yazı tipi konturlarıyla; görünüm aynı kalır (libass ile doğrulandı)").clicked() { ui.close(); self.text_to_drawing(false); }
                    if ui.button("Etkin satırı vektör kırpma olarak kopyala").on_hover_text("Metnin şeklini \\clip(...) olarak panoya kopyalar; başka bir satıra yapıştırın").clicked() { ui.close(); self.text_to_drawing(true); }
                });
                if ui.button("Makine çevirisi…").on_hover_text("DeepL / Google / LibreTranslate / Claude ile seçili satırları çevir (kendi API anahtarınızla)").clicked() { ui.close(); self.mt_open = true; }
                if ui.button("Kanji zamanlayıcı…").on_hover_text("Romaji karaoke zamanlamasını kanji/kana satırına aktar (Aegisub: Kanji Timer)").clicked() { ui.close(); self.kanji = Some(KanjiWin::default()); }
                if ui.button("Etiket uygula / yeniden hesapla…").on_hover_text("HYDRA + Recalculator benzeri: etiketleri başlangıca, tüm bloklara, \\t olarak ya da harf/satır degradesi olarak uygula; değerleri topla/çarp").clicked() { ui.close(); self.tagw = Some(TagWin::default()); }
                if ui.button("Degrade / parıltı…").on_hover_text("Kırpma şeritleriyle renk/saydamlık degradesi, parıltı (glow) katmanı").clicked() { ui.close(); self.tsw = Some(TypesetWin::default()); }
                if ui.button("Hareket izleme verisi uygula…").on_hover_text("Mocha / After Effects keyframe verisini seçili satırlara kare kare uygular (Aegisub-Motion)").clicked() {
                    ui.close();
                    self.open_motion();
                }
                if ui.button("Çözünürlüğü yeniden örnekle…").clicked() {
                    let (x, y) = self.doc.script.res();
                    self.res_x = x; self.res_y = y; self.resample_open = true; ui.close();
                }
                ui.separator();
                if ui.button("Kalite kontrolü göster").clicked() { self.tab = Tab::Quality; ui.close(); }
                if ui.button("Metni temizle (etiket + boşluk)").clicked() {
                    let mut s = self.doc.script.clone();
                    for d in s.dialogues_mut() { d.text = qc::clean_text(&d.text); }
                    self.doc.apply_new_script("Metni temizle", s);
                    ui.close();
                }
            });
            ui.menu_button("Yardım", |ui| {
                if ui.button("Kısayollar").clicked() { self.help_open = true; ui.close(); }
                if ui.add(egui::Button::new("Komut paleti…").shortcut_text("Ctrl+Shift+P")).clicked() { ui.close(); self.palette = Some((String::new(), 0)); }
                if ui.button("Kısayolları düzenle…").clicked() { self.hotkeys_open = true; ui.close(); }
                if ui.button("Hakkında").clicked() { self.about_open = true; ui.close(); }
            });
        });
    }

    fn autosave_dir() -> Option<PathBuf> {
        osisub_core::paths::data_dir().map(|d| d.join("autosave"))
    }

    fn autosave_name(&self) -> String {
        self.doc.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "adsiz".into())
    }

    fn autosave(&mut self) {
        if !self.doc.is_dirty() || self.last_autosave.elapsed().as_secs() < 30 { return; }
        self.last_autosave = std::time::Instant::now();
        let Some(dir) = Self::autosave_dir() else { return };
        if std::fs::create_dir_all(&dir).is_err() { return; }
        let name = self.autosave_name();
        let _ = std::fs::write(dir.join(format!("{}.autosave.ass", name)), self.doc.script.to_ass_string());
        // Özgün yol (kurtarma için)
        let side = dir.join(format!("{}.autosave.path", name));
        match &self.doc.path {
            Some(p) => { let _ = std::fs::write(side, p.to_string_lossy().as_bytes()); }
            None => { let _ = std::fs::remove_file(side); }
        }
    }

    /// Kaydedildiğinde ya da temiz çıkışta bu belgenin otomatik kaydını sil.
    fn clear_autosave(&self) {
        if let Some(dir) = Self::autosave_dir() {
            let name = self.autosave_name();
            let _ = std::fs::remove_file(dir.join(format!("{}.autosave.ass", name)));
            let _ = std::fs::remove_file(dir.join(format!("{}.autosave.path", name)));
        }
    }

    /// Açılışta kalmış otomatik kayıtları tara (önceki oturum çökmüş olabilir).
    fn scan_recovery(&mut self) {
        let Some(dir) = Self::autosave_dir() else { return };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if !p.to_string_lossy().ends_with(".autosave.ass") { continue; }
            let age_min = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|d| d.as_secs() / 60).unwrap_or(0);
            let side = PathBuf::from(p.to_string_lossy().replace(".autosave.ass", ".autosave.path"));
            let original = std::fs::read_to_string(side).ok().map(|t| PathBuf::from(t.trim()));
            self.recovery.push(Recovery { file: p, original, age_min });
        }
    }

    fn recovery_window(&mut self, ctx: &egui::Context) {
        if self.recovery.is_empty() { return; }
        let mut act: Option<(usize, bool)> = None; // (indeks, kurtar mı)
        egui::Window::new("Otomatik kayıt bulundu").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("Önceki oturum düzgün kapanmamış olabilir. Kurtarılabilir otomatik kayıtlar:");
            for (i, r) in self.recovery.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} — {} dk önce", r.original.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| r.file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), r.age_min));
                    if ui.button("Kurtar").clicked() { act = Some((i, true)); }
                    if ui.button("Sil").clicked() { act = Some((i, false)); }
                });
            }
            if ui.button("Hepsini yoksay").clicked() { self.recovery.clear(); }
        });
        if let Some((i, recover)) = act {
            let r = self.recovery.remove(i);
            if recover {
                match osisub_core::io::load(&r.file) {
                    Ok(s) => {
                        let mut d = Document::new(s);
                        d.path = r.original.clone();
                        d.mark_dirty();
                        self.doc = d;
                        if let Some(f) = self.first_row() { self.doc.select(vec![f], f); self.scroll_to = Some(f); }
                        self.status = "Otomatik kayıttan kurtarıldı — kaydetmeyi unutmayın".into();
                    }
                    Err(e) => self.status = format!("Kurtarılamadı: {}", e),
                }
            } else {
                let _ = std::fs::remove_file(&r.file);
                let _ = std::fs::remove_file(PathBuf::from(r.file.to_string_lossy().replace(".autosave.ass", ".autosave.path")));
            }
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.shot_frame += 1;
        ctx.request_repaint();
        let delay_ok = self.shot_delay.map_or(true, |d| self.started.elapsed().as_secs_f32() >= d);
        if self.shot_frame >= 12 && delay_ok && !self.shot_requested && (!self.media.is_open() || self.media.ready() || self.shot_frame > 900) && self.font_rx.is_none() && !self.autoplay && self.play_wait == 0 && !self.autospell && self.miss_rx.is_none() && self.spell_load_rx.is_none() {
            self.shot_requested = true;
            let done = self.shot_done.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                let mut buf = Vec::with_capacity((w * h * 4) as usize);
                for p in &img.pixels { buf.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]); }
                if let Some(im) = image::RgbaImage::from_raw(w, h, buf) { let _ = im.save(&path); }
                done.store(true, Ordering::SeqCst);
            })));
        }
        if self.shot_done.load(Ordering::SeqCst) { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
    }
}

impl eframe::App for App {
    /// Ekran görüntüsü/test kipinde egui belleği (sütun genişlikleri, pencere konumları) kalıcı değil
    fn persist_egui_memory(&self) -> bool {
        self.shot.is_none()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
        self.save_tm();
        if !self.doc.is_dirty() { self.clear_autosave(); }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // OSISUB_FRAME_LOG=1: her karenin arayüz süresini stderr'e yazar (performans ölçümü)
        static FRAME_LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *FRAME_LOG.get_or_init(|| std::env::var_os("OSISUB_FRAME_LOG").is_some()) {
            let t = std::time::Instant::now();
            self.ui_inner(ui, frame);
            eprintln!("kare {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
        } else {
            self.ui_inner(ui, frame);
        }
    }
}

impl App {
    fn ui_inner(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.theme_dirty {
            self.theme_dirty = false;
            theme::apply(&ctx, self.settings.light_theme);
            ctx.set_zoom_factor(self.settings.zoom);
        }
        if std::env::var_os("OSISUB_DEBUG_INPUT").is_some() {
            ctx.input(|i| for e in &i.events { if !matches!(e, egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_)) { eprintln!("girdi: {:?}", e); } });
        }
        if self.pending_auto_media {
            self.pending_auto_media = false;
            self.auto_media_from_script(&ctx);
        }
        self.pump_events(&ctx);
        self.pump_spell(&ctx);
        self.refresh_caches();
        if self.issues_key != self.doc.rev { ctx.request_repaint_after(std::time::Duration::from_millis(150)); }
        self.shortcuts(&ctx);
        if let Some(t) = self.ctx_copy.take() { ctx.copy_text(t); }
        self.poll_job(&ctx);
        self.poll_resync(&ctx);
        self.poll_mt(&ctx);
        self.refresh_caches(); // kısayollar belgeyi değiştirmiş olabilir
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

        if ctx.input(|i| i.viewport().close_requested()) && self.doc.is_dirty() && !self.quit_confirm && self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_confirm = true;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(j) = &self.job {
                    let p = j.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                    ui.add(egui::ProgressBar::new(p).desired_width(160.0).text(format!("{} %{:.0}", j.label, p * 100.0)));
                    if ui.small_button("İptal").clicked() { j.cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let (w, h) = self.doc.script.res();
                    ui.label(RichText::new(format!("{}×{}  |  seçili {}  |  {} sorun", w, h, self.doc.selection.len(), self.issues.len())).weak());
                });
            });
        });
        if self.show_media || self.media.is_open() {
            self.prepare_perspective_tool();
            egui::Panel::top("media").resizable(true).default_size(280.0).show(ui, |ui| self.media.ui(ui, &mut self.doc));
            if self.media.detached && self.media.info.as_ref().map_or(false, |i| i.has_video) {
                let ctx = ui.ctx().clone();
                ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("osisub_video"), egui::ViewportBuilder::default().with_title("osisub — video").with_inner_size([960.0, 560.0]), |ui, _| {
                    egui::CentralPanel::default_margins().show(ui, |ui| {
                        let (w, h) = (ui.available_width(), ui.available_height());
                        self.media.video_ui(ui, &mut self.doc, w, h);
                    });
                    if ui.input(|i| i.viewport().close_requested()) { self.media.detached = false; }
                });
            }
        }
        if self.tap.is_some() && !self.media.is_playing() { self.finish_tap(); }
        egui::Panel::bottom("editor").resizable(true).default_size(190.0).show(ui, |ui| self.editor(ui));
        egui::Panel::right("side").resizable(true).default_size(380.0).show(ui, |ui| self.side(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| self.grid(ui));

        self.timing_window(&ctx);
        self.find_window(&ctx);
        self.fonts_window(&ctx);
        self.fixes_window(&ctx);
        self.stats_window(&ctx);
        self.recovery_window(&ctx);
        self.guard_window(&ctx);
        self.help_windows(&ctx);
        self.export_window(&ctx);
        self.lua_error_window(&ctx);
        self.select_window(&ctx);
        self.attach_window(&ctx);
        self.compare_window(&ctx);
        self.hotkeys_window(&ctx);
        self.merge_window(&ctx);
        self.motion_window(&ctx);
        self.tag_window(&ctx);
        self.paste_over_window(&ctx);
        self.style_assist_window(&ctx);
        self.kanji_window(&ctx);
        self.filebatch_window(&ctx);
        self.batch_window(&ctx);
        self.typeset_window(&ctx);
        self.tracks_window(&ctx);
        self.palette_window(&ctx);
        self.history_window(&ctx);
        self.mt_window(&ctx);
        self.vad_window(&ctx);
        self.goto_window(&ctx);
        self.dummy_window(&ctx);
        self.ocr_window(&ctx);
        self.import_window(&ctx);
        self.resample_window(&ctx);
        self.autosave();
        if let Some((title, st)) = self.dialog.as_mut() {
            if let Some(res) = dialog_ui::show(&ctx, title, st) {
                self.worker.reply_dialog(res);
                self.dialog = None;
            }
        }
        if self.quit_confirm {
            egui::Window::new("Kaydedilmemiş değişiklikler").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.label("Kaydedilmemiş değişiklikler var.");
                ui.horizontal(|ui| {
                    if ui.button("Kaydet ve çık").clicked() { self.save(false); if !self.doc.is_dirty() { self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); } }
                    if ui.button("Kaydetmeden çık").clicked() { self.doc = Document::new(osisub_core::Script::new_default()); self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    if ui.button("İptal").clicked() { self.quit_confirm = false; }
                });
            });
        }
        if self.autoplay && self.media.ready() {
            self.autoplay = false;
            self.play_wait = 40;
            self.media.toggle_play(&self.doc, &ctx);
        }
        if self.autospell && self.spell.is_some() && self.miss_rx.is_none() {
            self.autospell = false;
            self.start_spellcheck();
        }
        if self.play_wait > 0 { self.play_wait -= 1; ctx.request_repaint(); }
        self.handle_screenshot(&ctx);
    }
}

/// Toplu satır işlemleri penceresinin ayarları.
pub(crate) struct BatchWin {
    pub same_gap: i64,
    pub short_chars: usize,
    pub short_gap: i64,
    pub cps: f64,
    pub min_ms: i64,
    pub max_ms: i64,
    pub gap_ms: i64,
    pub extend_only: bool,
}

impl Default for BatchWin {
    fn default() -> Self {
        BatchWin { same_gap: 250, short_chars: 40, short_gap: 250, cps: 15.0, min_ms: 1000, max_ms: 7000, gap_ms: 84, extend_only: true }
    }
}

/// Degrade / parıltı penceresinin ayarları.
/// Toplu dosya işlemleri penceresi.
pub(crate) struct FileBatchWin {
    pub files: Vec<PathBuf>,
    pub job: osisub_core::filebatch::Job,
    pub fixes: bool,
    pub resample: bool,
    pub res: (i32, i32),
    pub log: String,
    pub rx: Option<std::sync::mpsc::Receiver<String>>,
}

impl Default for FileBatchWin {
    fn default() -> Self {
        FileBatchWin { files: vec![], job: osisub_core::filebatch::Job { out_ext: "srt".into(), ..Default::default() }, fixes: false, resample: false, res: (1920, 1080), log: String::new(), rx: None }
    }
}

/// Kanji zamanlayıcı penceresi.
#[derive(Default)]
pub(crate) struct KanjiWin {
    pub src_style: String,
    pub dst_style: String,
    /// sıradaki kaynak satırın girdi indeksi
    pub cur: Option<usize>,
    /// düzenlenen gruplar (kaynak `ki+mi|no`, hedef `君|の`) ve hangi satır çifti için
    pub src_str: String,
    pub dst_str: String,
    pub loaded: Option<(usize, usize)>,
}

/// Etiket uygulama penceresi (HYDRA + Recalculator).
pub(crate) struct TagWin {
    /// (ad, açık mı, başlangıç değeri, bitiş değeri)
    pub rows: Vec<(&'static str, bool, String, String)>,
    pub extra: String,
    pub extra_to: String,
    /// 0 başlangıç, 1 tüm bloklar, 2 \t, 3 harf degradesi, 4 satır degradesi
    pub mode: u8,
    pub t1: i64,
    pub t2: i64,
    pub accel: f64,
    pub rc_tag: String,
    pub rc_mul: bool,
    pub rc_x: f64,
    pub rc_y: f64,
    pub rc_style: bool,
}

impl Default for TagWin {
    fn default() -> Self {
        let rows = vec![
            ("1c", "&HFFFFFF&", "&H00A5FF&"), ("3c", "&H000000&", "&H000000&"), ("4c", "&H000000&", "&H000000&"),
            ("alpha", "&H00&", "&HFF&"), ("1a", "&H00&", "&HFF&"), ("3a", "&H00&", "&HFF&"),
            ("bord", "2", "4"), ("shad", "0", "2"), ("blur", "1", "5"), ("be", "1", "3"),
            ("fs", "60", "80"), ("fscx", "100", "120"), ("fscy", "100", "120"), ("fsp", "0", "5"),
            ("frz", "0", "10"), ("frx", "0", "10"), ("fry", "0", "10"), ("fax", "0", "0.1"),
            ("an", "5", "5"), ("b", "1", "1"), ("i", "1", "1"), ("q", "2", "2"),
        ];
        TagWin {
            rows: rows.into_iter().map(|(n, a, b)| (n, false, a.to_string(), b.to_string())).collect(),
            extra: String::new(),
            extra_to: String::new(),
            mode: 0,
            t1: 0,
            t2: 500,
            accel: 1.0,
            rc_tag: "fscx".into(),
            rc_mul: true,
            rc_x: 1.0,
            rc_y: 1.0,
            rc_style: true,
        }
    }
}

impl TagWin {
    /// (başlangıç, bitiş) etiket dizileri
    pub fn tags(&self) -> (String, String) {
        let mut a = String::new();
        let mut b = String::new();
        for (n, on, x, y) in &self.rows {
            if *on {
                a.push_str(&format!("\\{n}{x}"));
                b.push_str(&format!("\\{n}{y}"));
            }
        }
        a.push_str(self.extra.trim());
        b.push_str(if self.extra_to.trim().is_empty() { self.extra.trim() } else { self.extra_to.trim() });
        (a, b)
    }
}

pub(crate) struct TypesetWin {
    pub vertical: bool,
    pub strip: f64,
    pub from: String,
    pub to: String,
    pub accel: f64,
    pub glow_blur: f64,
    pub glow_color: String,
    pub glow_alpha: u8,
}

impl Default for TypesetWin {
    fn default() -> Self {
        TypesetWin { vertical: false, strip: 4.0, from: "\\c&HFFFFFF&".into(), to: "\\c&H00A5FF&".into(), accel: 1.0, glow_blur: 5.0, glow_color: "&HFFFFFF&".into(), glow_alpha: 0x80 }
    }
}

/// Arka planda çalışan video işi.
pub(crate) struct VideoJob {
    pub label: String,
    pub progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub rx: std::sync::mpsc::Receiver<Result<String, String>>,
}

/// Komut paletindeki bir öğenin eylemi.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PalAct {
    Cmd(&'static str),
    Win(&'static str),
    Macro(usize),
}

/// Türkçe duyarsız arama anahtarı: küçük harf + ı/İ/ş/ğ/ü/ö/ç → ASCII.
pub(crate) fn fold(s: &str) -> String {
    osisub_core::qc::tr_lower(s).chars().map(|c| match c { 'ı' => 'i', 'ş' => 's', 'ğ' => 'g', 'ü' => 'u', 'ö' => 'o', 'ç' => 'c', 'â' => 'a', 'î' => 'i', 'û' => 'u', c => c }).collect()
}

/// Sorgunun tüm sözcükleri etikette geçiyorsa puan (küçük = iyi), yoksa None.
pub(crate) fn palette_score(label: &str, query: &str) -> Option<usize> {
    let l = fold(label);
    let mut score = 0;
    for w in fold(query).split_whitespace() {
        let p = l.find(w)?;
        // sözcük başında eşleşme daha iyi
        score += if p == 0 || l[..p].ends_with(' ') { p / 4 } else { p + 20 };
    }
    Some(score)
}

#[cfg(test)]
mod palette_tests {
    use super::*;
    #[test]
    fn turkish_insensitive_fuzzy() {
        assert!(palette_score("Sahne değişimlerine yapıştır", "degisim").is_some());
        assert!(palette_score("İstatistikler", "istat").is_some());
        assert!(palette_score("Yazı tipi denetimi", "YAZI denet").is_some());
        assert!(palette_score("Bul / Değiştir", "kaydet").is_none());
        // sözcük başı eşleşmesi daha iyi puan alır
        assert!(palette_score("Kaydet", "kay").unwrap() < palette_score("Satırı kaydır", "kay").unwrap() + 1);
        assert!(palette_score("Videoya yak", "yak").unwrap() < palette_score("Yakınlaştır menüsü bayrak", "rak").unwrap());
    }
}

/// Hareket izleme penceresinin durumu.
pub(crate) struct MotionWin {
    pub text: String,
    pub opts: osisub_core::motion::MotionOptions,
}

impl App {
    /// MKV'ye gömme (`burn` = false) ya da videoya yakma işini arka planda başlatır.
    fn start_video_job(&mut self, burn: bool) {
        let Some(video) = self.media.path.clone().or_else(|| rfd::FileDialog::new().add_filter("Video", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts"]).pick_file()) else { return };
        let stem = video.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "cikti".into());
        let (ext, filt) = if burn { ("mp4", "MP4") } else { ("mkv", "Matroska") };
        let Some(out) = rfd::FileDialog::new().add_filter(filt, &[ext]).set_file_name(format!("{stem}.osisub.{ext}")).save_file() else { return };
        // Belgenin şu anki hali (kaydedilmemiş değişiklikler dahil) geçici ASS'e
        let tmp = std::env::temp_dir().join(format!("osisub-job-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let ass = tmp.join("altyazi.ass");
        if let Err(e) = std::fs::write(&ass, self.doc.script.to_ass_string()) { self.status = e.to_string(); return; }
        // Gömülü yazı tipleri klasöre (yakma için fontsdir), kurulu yazı tipleri dosya listesine (gömme için)
        let fonts_dir = tmp.join("fonts");
        let _ = std::fs::remove_dir_all(&fonts_dir);
        let _ = std::fs::create_dir_all(&fonts_dir);
        for a in osisub_core::attach::list(&self.doc.script).into_iter().filter(|a| a.section == osisub_core::attach::SEC_FONTS) {
            if let Some(d) = osisub_core::attach::extract(&self.doc.script, &a.name) { let _ = std::fs::write(fonts_dir.join(a.name.replace(['/', '\\'], "_")), d); }
        }
        let mut font_files = vec![];
        if !burn {
            let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
            for u in osisub_core::fonts_used::used_fonts(&self.doc.script) {
                for f in db.files_for(&u.family) { if !font_files.contains(&f) { font_files.push(f); } }
            }
        }
        let duration = self.media.info.as_ref().map(|i| i.duration_ms).unwrap_or(0);
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let (p2, c2) = (progress.clone(), cancel.clone());
        std::thread::spawn(move || {
            let r = if burn {
                let dur = if duration > 0 { duration } else { osisub_media::probe(&video).map(|i| i.duration_ms).unwrap_or(0) };
                let o = osisub_media::mux::BurnOptions { fonts_dir: Some(fonts_dir), ..Default::default() };
                osisub_media::mux::burn(&video, &ass, &out, &o, dur, |p| p2.store((p * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed), || c2.load(std::sync::atomic::Ordering::Relaxed))
            } else {
                let o = osisub_media::mux::MuxOptions { language: "tur".into(), title: "Türkçe".into(), fonts: font_files, default_track: true, keep_existing_subs: true };
                osisub_media::mux::mux_mkv(&video, &ass, &out, &o)
            };
            let _ = tx.send(r.map(|_| out.display().to_string()));
        });
        self.job = Some(VideoJob { label: if burn { "Videoya yakılıyor".into() } else { "MKV'ye gömülüyor".into() }, progress, cancel, rx });
    }

    fn start_resync(&mut self) {
        let Some(new) = self.media.path.clone() else { self.status = "Önce yeni videoyu açın (Video → Video / ses aç)".into(); return };
        let Some(old) = rfd::FileDialog::new().set_title("Altyazının zamanlandığı ESKİ video/ses").add_filter("Video / ses", &["mkv", "mp4", "webm", "avi", "mov", "m2ts", "ts", "wav", "flac", "mka", "aac", "opus", "mp3"]).pick_file() else { return };
        let lines: Vec<(i64, i64)> = self.doc.script.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time)).collect();
        let new_wave = self.media.waveform();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = (|| {
                let o = osisub_media::load_waveform(&old)?;
                let n = match new_wave { Some(w) => (*w).clone(), None => osisub_media::load_waveform(&new)? };
                Ok(osisub_media::resync::compute_offsets(&o, &n, &lines, &Default::default()))
            })();
            let _ = tx.send(r);
        });
        self.resync_rx = Some(rx);
        self.status = "Sesle yeniden zamanlanıyor…".into();
    }

    fn poll_resync(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.resync_rx else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let Ok(r) = rx.try_recv() else { return };
        self.resync_rx = None;
        match r {
            Ok(groups) => {
                let mut s = self.doc.script.clone();
                for d in s.dialogues_mut() {
                    let off = osisub_media::resync::offset_for(&groups, d.start_time);
                    d.start_time = (d.start_time + off).max(0);
                    d.end_time = (d.end_time + off).max(0);
                }
                self.doc.apply_new_script("Sesle yeniden zamanla", s);
                let weak = groups.iter().filter(|g| g.fallback).count();
                let mut offs: Vec<i64> = groups.iter().map(|g| g.offset_ms).collect();
                offs.dedup();
                self.status = format!("Yeniden zamanlandı: {} grup, {} farklı kayma{}", groups.len(), offs.len(), if weak > 0 { format!(", {weak} grup GÜVENSİZ (günlüğe bakın)") } else { String::new() });
                for g in groups.iter().filter(|g| g.fallback) {
                    self.log.push_str(&format!("Yeniden zamanlama güvensiz: {} – {} (ilinti {:.2}), önceki kayma {:+} ms kullanıldı\n", time::format_ass(g.start_ms), time::format_ass(g.end_ms), g.score, g.offset_ms));
                }
            }
            Err(e) => self.status = format!("Yeniden zamanlama başarısız: {e}"),
        }
    }

    /// Video işinin durumunu yoklar (her kare).
    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(j) = &self.job else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        if let Ok(r) = j.rx.try_recv() {
            self.status = match r { Ok(p) => format!("Hazır: {p}"), Err(e) => format!("Video işi başarısız: {e}") };
            self.job = None;
        }
    }

    /// Kurulu yazı tipi aileleri; ilk çağrıda taramayı başlatır, hazır değilse None.
    pub(crate) fn font_families(&mut self) -> Option<std::sync::Arc<Vec<String>>> {
        if self.fonts_list.is_none() {
            match &self.fonts_rx {
                None => {
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || { let _ = tx.send(osisub_lua::fonts::FontDb::default().families()); });
                    self.fonts_rx = Some(rx);
                }
                Some(rx) => {
                    if let Ok(v) = rx.try_recv() {
                        self.fonts_list = Some(std::sync::Arc::new(v));
                        self.fonts_rx = None;
                    }
                }
            }
        }
        self.fonts_list.clone()
    }

    /// Referans altyazıyı ayarla (ızgaradaki Referans sütunu, çeviri sekmesi).
    pub(crate) fn set_reference(&mut self, sc: osisub_core::Script) {
        let mut v: Vec<(i64, i64, String)> = sc.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, osisub_core::tags::visible_text(&d.text).replace('\n', " ⏎ "))).collect();
        v.sort_by_key(|x| x.0);
        self.ref_lines = v;
        self.reference = Some(sc);
    }

    /// Seçili satırları dengele (`balance`) ya da satır sonlarını kaldır; tipografi/karaoke satırları atlanır.
    pub(crate) fn rebreak_selected(&mut self, balance: bool) {
        let max = self.rules.max_line_len.max(10);
        let changes: Vec<(usize, String)> = self.doc.target_lines().into_iter().filter_map(|i| match self.doc.script.entries.get(i) {
            Some(Entry::Dialogue(d)) if !qc::is_typeset(d) && !qc::is_karaoke(d) => {
                let t = if balance { osisub_core::linebreak::balance(&d.text, max) } else { osisub_core::linebreak::unbreak(&d.text) };
                (t != d.text).then_some((i, t))
            }
            _ => None,
        }).collect();
        let n = changes.len();
        let desc = if balance { "Satırları dengele" } else { "Satır sonlarını kaldır" };
        if n > 0 { self.doc.edit(desc, |tx| for (i, t) in changes { tx.dialogue_mut(i, |d| d.text = t); }); }
        self.status = format!("{desc}: {n} satır");
    }

    /// Seçili satırlara harf dönüşümü (Türkçe i/İ duyarlı; etiketler korunur), tek geri-al adımı.
    pub(crate) fn change_case(&mut self, case: osisub_core::casing::Case) {
        let changes: Vec<(usize, String)> = self.doc.target_lines().into_iter().filter_map(|i| match self.doc.script.entries.get(i) {
            Some(Entry::Dialogue(d)) => { let t = osisub_core::casing::apply(&d.text, case, true); (t != d.text).then_some((i, t)) }
            _ => None,
        }).collect();
        let n = changes.len();
        if n > 0 { self.doc.edit("Harf dönüştür", |tx| for (i, t) in changes { tx.dialogue_mut(i, |d| d.text = t); }); }
        self.status = format!("Harf dönüştürüldü: {n} satır");
    }

    /// Perspektif video aracı için etkin satırın kutusunu (yazı tipiyle ölçülür, metin değişince
    /// yeniden) ve kamera uzaklığını hazırlar; araç LayoutRes'siz betiğe yazdıysa LayoutRes'i ekler.
    fn prepare_perspective_tool(&mut self) {
        if self.media.tool != crate::media_ui::VideoTool::Perspective {
            return;
        }
        let at = self.doc.active;
        if self.media.persp_need_layout {
            self.media.persp_need_layout = false;
            if self.doc.script.info("LayoutResY").is_none() && self.media.info.as_ref().map_or(false, |i| i.has_video) {
                let mut s = self.doc.script.clone();
                let before = s.entries.len();
                self.perspective_dist(&mut s);
                let shift = s.entries.len() - before;
                let sel: Vec<usize> = self.doc.selection.iter().map(|&i| i + shift).collect();
                self.doc.apply_new_script("LayoutRes (perspektif)", s);
                self.doc.select(sel, at + shift);
                return;
            }
        }
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at) else { self.media.persp_box = None; return };
        if !matches!(&self.media.persp_box, Some((l, t, _)) if *l == at && *t == d.text) {
            let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
            self.media.persp_box = osisub_lua::fonts::line_box(db, &self.doc.script, d).ok().map(|b| (at, d.text.clone(), b));
        }
        self.media.persp_dist = self.perspective_dist(&mut self.doc.script.clone());
    }

    /// Perspektif için betik koordinatında kamera uzaklığı. LayoutRes: betikte varsa o, yoksa
    /// video çözünürlüğü (betiğe yazılır), o da yoksa PlayRes.
    pub(crate) fn perspective_dist(&self, s: &mut osisub_core::Script) -> f64 {
        let (_, play_y) = s.res();
        let layout_y = match s.info("LayoutResY").and_then(|v| v.trim().parse::<f64>().ok()) {
            Some(v) => v,
            None => match self.media.info.as_ref().filter(|i| i.has_video) {
                Some(i) => {
                    s.set_info("LayoutResX", &i.width.to_string());
                    s.set_info("LayoutResY", &i.height.to_string());
                    i.height as f64
                }
                None => play_y as f64,
            },
        };
        osisub_core::perspective::dist_for(play_y as f64, layout_y)
    }

    /// Etkin satırın 4 noktalı vektör kırpmasını hedef dörtgen alıp satırı perspektifle oturtur.
    fn perspective_fit(&mut self) {
        use osisub_core::{overrides, perspective};
        let at = self.doc.active;
        let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at).cloned() else { return };
        let quad = overrides::get_initial(&d.text, "clip")
            .map(|t| t.args())
            .and_then(|a| a.last().cloned())
            .and_then(|draw| perspective::quad_from_clip(&draw));
        let Some(quad) = quad else {
            self.status = "Önce satıra 4 noktalı vektör kırpma çizin (Video aracı: Vektör kırpma): sol üst → sağ üst → sağ alt → sol alt".into();
            return;
        };
        let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
        let (w, h) = match osisub_lua::fonts::line_box(db, &self.doc.script, &d) { Ok(b) => b, Err(e) => { self.status = e; return; } };
        let mut s = self.doc.script.clone();
        let dist = self.perspective_dist(&mut s);
        let Some((p, org, err)) = perspective::fit(&quad, w, h, dist) else { self.status = "Perspektif çözümü bulunamadı".into(); return };
        let mut text = d.text.clone();
        for n in ["clip", "iclip", "pos", "move", "org", "frx", "fry", "frz", "fr", "fscx", "fscy", "fax", "fay", "an", "a"] {
            text = overrides::remove_initial(&text, n);
        }
        for t in overrides::parse_block(&perspective::tags(&p, org)) {
            text = overrides::set_initial(&text, &t.name, &t.arg);
        }
        let idx = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(x) if *x == d)).unwrap_or(at);
        if let Some(Entry::Dialogue(x)) = s.entries.get_mut(idx) { x.text = text; }
        self.doc.apply_new_script("Perspektife oturt", s);
        self.status = format!("Perspektif: köşe hatası {:.2} px, frx {:.1}° fry {:.1}°", err, p.frx, p.fry);
    }

    /// Seçili satırları `\p1` çizimine çevirir; `as_clip` ise etkin satırın şeklini mutlak `\clip(...)` olarak panoya koyar.
    fn text_to_drawing(&mut self, as_clip: bool) {
        let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
        let targets: Vec<usize> = if as_clip { vec![self.doc.active] } else { self.doc.target_lines() };
        let mut s = self.doc.script.clone();
        let mut n = 0;
        for &i in &targets {
            let Some(Entry::Dialogue(d)) = s.entries.get(i) else { continue };
            match osisub_lua::fonts::line_to_drawing(db, &s, d) {
                Ok(t) => {
                    if as_clip {
                        // \an7\pos(x,y) + çizim → mutlak koordinatlı vektör kırpma
                        let pos = osisub_core::overrides::get_initial(&t, "pos").map(|p| p.args()).unwrap_or_default();
                        let (x, y) = (pos.first().and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0), pos.get(1).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0));
                        let draw = t.rsplit('}').next().unwrap_or("");
                        let mut cmds = osisub_core::drawing::parse(draw);
                        for c in cmds.iter_mut() { for p in c.pts.iter_mut() { p.0 += x; p.1 += y; } }
                        let clip = format!("\\clip({})", osisub_core::drawing::to_string(&cmds));
                        self.ctx_copy = Some(clip);
                        self.status = "Vektör kırpma panoya kopyalandı (döndürme/eğme uygulanmaz)".into();
                        return;
                    }
                    if let Some(Entry::Dialogue(d)) = s.entries.get_mut(i) { d.text = t; }
                    n += 1;
                }
                Err(e) => { self.status = e; return; }
            }
        }
        if n > 0 {
            self.doc.apply_new_script("Metin → çizim", s);
            self.status = format!("{n} satır çizime dönüştürüldü");
        }
    }
}

/// Makro adlarını "/" ile iç içe menülere böler (Aegisub automation menüsü). `prefix` = geçerli düzey.
fn macro_menu(ui: &mut egui::Ui, list: &[(usize, String, String)], prefix: &str, enabled: bool, clicked: &mut Option<usize>) {
    let mut groups: Vec<String> = vec![];
    for (id, name, desc) in list {
        let Some(rest) = name.strip_prefix(prefix) else { continue };
        match rest.split_once('/') {
            Some((g, tail)) if !g.is_empty() && !tail.is_empty() => {
                if !groups.iter().any(|x| x == g) { groups.push(g.to_string()); }
            }
            _ => {
                if ui.add_enabled(enabled, egui::Button::new(rest)).on_hover_text(desc).clicked() { *clicked = Some(*id); }
            }
        }
    }
    for g in groups {
        let p = format!("{prefix}{g}/");
        ui.menu_button(g, |ui| macro_menu(ui, list, &p, enabled, clicked));
    }
}
