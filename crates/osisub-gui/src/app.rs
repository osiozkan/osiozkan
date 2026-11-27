use crate::i18n::tr;
use crate::i18n::trf;
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
    /// Makro başlarken belgenin `rev`'i: sonuç geldiğinde belge değişmişse (başka dosya açıldı,
    /// satır düzenlendi) sonuç uygulanmaz — yoksa eski kopya yeni belgenin üzerine yazılırdı
    run_rev: u64,
    /// Makine çevirisi başlarken belgenin `rev`'i
    pub(crate) mt_rev: u64,
    progress: f32,
    dialog: Option<(String, DialogState)>,
    log: String,
    status: String,
    tab: Tab,
    filter: String,
    rows: Vec<usize>,
    rows_key: (u64, String),
    /// Geçerli katlamalar ve (katlı satır varsa) ızgaradaki satırların gerçek numaraları
    folds: Vec<osisub_core::folds::Fold>,
    row_no: Vec<usize>,
    rules: Rules,
    issues: Vec<Issue>,
    /// Kalite sekmesi süzgeci ("" = tümü, yoksa Kind::category)
    qc_filter: &'static str,
    issue_map: HashMap<usize, Vec<Kind>>,
    issues_key: u64,
    qc_rx: Option<std::sync::mpsc::Receiver<(u64, Vec<Issue>)>>,
    qc_seen_rev: u64,
    qc_changed_at: std::time::Instant,
    /// Ekran genişliği denetimi (yazı tipiyle): (belge sürümü, sorunlar), arka plan işi, kalıcı yazı tipi dizini
    width_issues: (u64, Vec<Issue>),
    width_rx: Option<std::sync::mpsc::Receiver<(u64, Vec<Issue>)>>,
    width_db: std::sync::Arc<std::sync::Mutex<osisub_lua::fonts::FontDb>>,
    /// Ekran çakışması denetimi (libass ile çizerek, elle başlatılır): (belge sürümü, sorunlar), iş
    pub(crate) collide_issues: (u64, Vec<Issue>),
    pub(crate) collide_rx: Option<std::sync::mpsc::Receiver<(u64, Result<Vec<Issue>, String>)>>,
    start_buf: String,
    end_buf: String,
    style_sel: usize,
    scroll_to: Option<usize>,
    anchor: usize,
    timing_open: bool,
    shift_ms: i64,
    shift_use_frames: bool,
    /// Noktadan senkron: (satır, hedef ms)
    sync_pts: Vec<(usize, i64)>,
    fps_from: f64,
    fps_to: f64,
    shift_frames: i64,
    shift_all: bool,
    /// kaydırma: seçimden sonra başlayan tüm satırlar da
    shift_after: bool,
    shift_part: osisub_core::qc::ShiftPart,
    /// Stil adı düzenleme oturumu: (stil girdisi, son belge sürümü, eski adı kullanan satırlar, \\r içeren satırlar)
    style_rename: Option<(usize, u64, Vec<usize>, Vec<usize>)>,
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
    /// "Değiştir" (sonrakini) için konum: (satır, alan, bayt)
    find_pos: Option<(usize, osisub_core::search::Field, usize)>,
    /// Bilgi sekmesi: eklenecek yeni Script Info alanı (anahtar, değer)
    info_new: (String, String),
    /// Stiller sekmesi: oluşturulacak katalog adı
    catalog_new: String,
    /// Sık hataları düzelt önizlemesi: (girdi, uygula mı, önce, sonra)
    fixes_preview: Option<Vec<(usize, usize, bool, osisub_core::ass::Dialogue, osisub_core::ass::Dialogue)>>,
    /// Videoya hizala penceresi ve renk toleransı
    align_open: bool,
    align_tol: f64,
    /// Çözünürlük denetimi yapılan son ortam ve bekleyen soru (video genişliği, yüksekliği)
    res_checked: Option<PathBuf>,
    res_prompt: Option<(i32, i32)>,
    resample_open: bool,
    res_x: i32,
    res_ar: osisub_core::timing::ArMode,
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
    /// Çeviri sekmesi: (belge sürümü, çevrilmiş, toplam, referans satır sayısı)
    tr_progress: (u64, usize, usize, usize),
    /// Çeviri sekmesi: bir sonraki karede referans metnini kutuya ekle
    tr_insert_original: bool,
    /// Kullanım kılavuzu penceresi (arama metni)
    guide: Option<String>,
    about_open: bool,
    settings: Settings,
    reference: Option<osisub_core::Script>,
    ref_lines: Vec<(i64, i64, String)>,
    fonts_open: bool,
    fixes_open: bool,
    stats_open: bool,
    honor_open: bool,
    /// Yer imleri penceresi; (rev, yer imi Extradata kimlikleri) ızgara önbelleği
    bookmarks_open: bool,
    backups_open: bool,
    ready_open: bool,
    /// Çoklu değiştir: kurallar ve önizleme sonuçları (açıksa Some)
    mrep: Option<(Vec<osisub_core::multireplace::Rule>, Vec<Result<usize, String>>, bool)>,
    mrep_key: Option<(u64, usize, Option<usize>, bool)>,
    /// Lua konsolu: kod ve yüklendiğinde çalıştırılacak mı
    console_code: String,
    console_pending: bool,
    /// yer imi notu düzenleme tamponu: (girdi indeksi, metin)
    bookmark_edit: Option<(usize, String)>,
    /// Dosyayı böl penceresi: (kesme zamanları metni, eşit parça sayısı, zamanı sıfırla)
    split_win: Option<(String, usize, bool)>,
    bookmark_ids: (u64, std::collections::HashSet<u32>),
    export_open: bool,
    export_sel: Vec<usize>,
    export_job: Option<ExportJob>,
    /// yerleşik dışa aktarım süzgeçleri (Lua süzgeçlerinden sonra)
    export_builtins: osisub_core::exportfilters::Builtins,
    tap: Option<TapState>,
    tap_latency_ms: i64,
    backed_up: std::collections::HashSet<PathBuf>,
    theme_dirty: bool,
    goto_open: bool,
    goto_focus: bool,
    /// "Basılı tutarak oynat" tuşu basılı ve çalıyor
    hold_playing: bool,
    /// Pencereye en son gönderilen başlık
    last_title: Option<String>,
    /// Arka plan iş parçacıklarının arayüzü uyandırması için
    egui_ctx: egui::Context,
    /// Lua betik dosyası izleyicisi (kaydedilince otomatik yeniden yükleme)
    script_watch: crate::script_watch::ScriptWatch,
    /// Sahte video penceresi (açıksa düzenlenen ayarlar; süre dakika olarak ayrı tutulur)
    dummy_win: Option<(osisub_core::dummy::DummyVideo, f64)>,
    /// Kurulu yazı tipi aileleri (ilk gerektiğinde arka planda taranır)
    fonts_list: Option<std::sync::Arc<Vec<String>>>,
    fonts_rx: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    /// Resim altyazısı OCR penceresi ve arka plan işi
    ocr: Option<windows::OcrWin>,
    hardsub: Option<windows::HardsubWin>,
    text_import: Option<windows::TextImportWin>,
    whisper: Option<windows::WhisperWin>,
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
    /// Yazım: elle yazılan düzeltme (öneri tıklanınca dolar)
    spell_custom: String,
    spell_ignore: std::collections::HashSet<String>,
    /// Eş anlamlılar sözlükleri (arka planda yüklenir; tr_TR, en_US…)
    thesauri: std::sync::Arc<std::sync::OnceLock<Vec<osisub_spell::thesaurus::Thesaurus>>>,
    /// Metin kutusunda sağ tıklanan sözcük: (satır, baş, son karakter, sözcük)
    ctx_word: Option<(usize, usize, usize, String)>,
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
    /// OP/ED yerleştirme penceresi
    song_win: Option<windows::SongWin>,
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
    mt_rx: Option<std::sync::mpsc::Receiver<(Vec<usize>, Vec<String>, Option<String>)>>,
    mt_from_ref: bool,
    /// Kopyala-yapıştır çeviri oturumu (Subtitle Edit)
    cp: Option<windows::CpState>,
    resync_rx: Option<std::sync::mpsc::Receiver<Result<Vec<osisub_media::resync::Group>, String>>>,
    /// Izgara yazım göstergesi önbelleği: (belge sürümü, entry → hatalı sözcük var mı)
    spell_rows: std::cell::RefCell<(u64, std::collections::HashMap<usize, bool>)>,
    /// Düzenleme kutusundaki imleç (satır, karakter konumu) — "imleçte böl" için
    editor_cursor: Option<(usize, usize)>,
    /// Düzenleme kutusundaki seçim: (satır, başlangıç, bitiş) karakter dizinleri (araç çubuğu için)
    editor_sel: Option<(usize, usize, usize)>,
    /// Lua `aegisub.gui.set_selection/set_cursor` sonucu: (satır, bağlantı, imleç) karakter dizinleri; sonraki karede metin kutusuna uygulanır
    pending_text_sel: Option<(usize, usize, usize)>,
    /// Aegisub "initial line state": etkin satırın seçildiği andaki metni (geri al / orijinali ekle)
    line_initial: (usize, String, usize),
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
    ("ready_check", "Yayına hazır mı? (kontrol listesi)"),
    ("multi_replace", "Çoklu değiştir…"),
    ("select_overlaps", "Çakışan satırları seç"),
    ("silence_next", "Ses: sonraki sessizliğe atla"),
    ("silence_prev", "Ses: önceki sessizliğe atla"),
    ("word_to_next", "Son sözcüğü sonraki satırın başına taşı"),
    ("word_from_next", "Sonraki satırın ilk sözcüğünü bu satıra al"),
    ("word_to_prev", "İlk sözcüğü önceki satırın sonuna taşı"),
    ("word_from_prev", "Önceki satırın son sözcüğünü bu satıra al"),
    ("ocr_fix", "OCR hatalarını düzelt (yazım sözlüğüyle)"),
    ("bookmark_toggle", "Yer imi koy / kaldır"),
    ("bookmark_next", "Sonraki yer imi"),
    ("bookmark_prev", "Önceki yer imi"),
    ("bookmarks", "Yer imleri…"),
    ("recombine", "Satırları yeniden birleştir (örtüşen bölünmüş satırlar)"),
    ("swap_lines", "İki satırı değiş tokuş et"),
    ("continuous_end", "Zamanları ardışık yap (bitişi uzat)"),
    ("continuous_start", "Zamanları ardışık yap (başlangıcı çek)"),
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
    ("tool_shear", "Video aracı: eğme (\\fax/\\fay)"),
    ("hold_play", "Basılı tutarak oynat (playhead'den; bırakınca durur)"),
    ("guide", "Kullanım kılavuzu (F1)"),
    ("song_place", "OP/ED yerleştir (sesle ya da playhead'e)…"),
    ("split_breaks", "Satır sonlarından böl (süre metne orantılı)"),
    ("split_breaks_keep", "Satır sonlarından böl (aynı zaman)"),
    ("split_karaoke", "Karaoke hecelerine böl (her hece bir satır)"),
    ("tool_persp", "Video aracı: perspektif (4 köşe)"),
    ("tool_draw", "Video aracı: çizim (\\p)"),
    ("split_after", "Seçili satırları kareden sonra böl"),
    ("split_before", "Seçili satırları kareden önce böl"),
    ("snap_scene", "Sahneye yapıştır (çevreleyen anahtar kareler)"),
    ("shift_to_frame", "Geçerli kareye kaydır"),
    ("start_inc", "Başlangıcı 10 ms ileri"),
    ("start_dec", "Başlangıcı 10 ms geri"),
    ("length_inc", "Bitişi 10 ms ileri"),
    ("length_dec", "Bitişi 10 ms geri"),
    ("length_inc_shift", "Bitişi 10 ms ileri (karaoke: sonraki heceler de kayar)"),
    ("length_dec_shift", "Bitişi 10 ms geri (karaoke: sonraki heceler de kayar)"),
    ("kara_mode", "Ses: karaoke kipi aç/kapat (hece hece zamanlama)"),
    ("lead_in", "Giriş payı ekle (lead-in)"),
    ("lead_out", "Çıkış payı ekle (lead-out)"),
    ("boundary_next", "Sonraki satır sınırına git"),
    ("boundary_prev", "Önceki satır sınırına git"),
    ("frame_next_large", "10 kare ileri"),
    ("frame_prev_large", "10 kare geri"),
    ("select_visible", "Karede görünen satırları seç"),
    ("insert_before_video", "Önüne video zamanında satır ekle"),
    ("insert_after_video", "Arkasına video zamanında satır ekle"),
    ("move_up", "Satırları yukarı taşı"),
    ("move_down", "Satırları aşağı taşı"),
    ("split_preserve", "İmleçte böl (zamanlar korunur)"),
    ("clear_text", "Metni temizle (etiketler kalır)"),
    ("revert_line", "Satırı geri al (seçildiği andaki metin)"),
    ("insert_original", "Orijinal metni imlece ekle"),
    ("frame_save", "Kareyi PNG olarak kaydet (altyazılı)"),
    ("frame_save_raw", "Kareyi PNG olarak kaydet (altyazısız)"),
    ("frame_copy", "Kareyi panoya kopyala (altyazılı)"),
    ("frame_copy_raw", "Kareyi panoya kopyala (altyazısız)"),
    ("audio_clip", "Seçili satırların sesini kaydet (WAV)"),
    ("frame_save_subs", "Kareyi yalnız altyazılarla saydam PNG kaydet"),
    ("zoom_100", "Video yakınlaştırma %100"),
    ("zoom_200", "Video yakınlaştırma %200"),
    ("zoom_fit", "Videoyu panele sığdır"),
    ("find_next", "Sonrakini bul"),
    ("replace_next", "Sonrakini değiştir"),
    ("fold_create", "Seçimi katla"),
    ("fold_toggle", "Katlamayı aç/kapat"),
    ("fold_open", "Katlamayı aç"),
    ("fold_close", "Katlamayı kapat"),
    ("fold_clear", "Katlamayı kaldır"),
    ("fold_open_all", "Tüm katlamaları aç"),
    ("fold_close_all", "Tüm katlamaları kapat"),
    ("fold_clear_all", "Tüm katlamaları kaldır"),
    ("jump_start", "Videoyu satırın başına götür"),
    ("jump_end", "Videoyu satırın sonuna götür"),
    ("align_video", "Videoya hizala (tabela zamanlama)…"),
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
    // Çizim (\p1) metni denetlenmez: aynı uzunlukta boşlukla değiştirilir (bayt konumları korunur)
    let mut drawing = false;
    let visible: String = parts.iter().map(|p| match p {
        osisub_core::tags::Part::Text(t) if drawing => " ".repeat(t.len()),
        osisub_core::tags::Part::Text(t) => t.to_string(),
        osisub_core::tags::Part::Block(b) => {
            if let Some(d) = osisub_core::tags::drawing_mode(b) { drawing = d; }
            String::new()
        }
    }).collect();
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

/// Lua işçisinin olaylarını bir aktarıcı iş parçacığından geçirir; her olayda arayüzü uyandırır
/// (arayüz boştayken çizmez — işçi sonuçları beklemeden görünsün).
fn wake_on_events(worker: &mut Worker, ctx: &egui::Context) {
    let (tx, rx) = std::sync::mpsc::channel();
    let src = std::mem::replace(&mut worker.events, rx);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = src.recv() {
            if tx.send(ev).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: crate::Args) -> App {
        let crate::Args { file, scripts, screenshot: shot, tab, media, open_win, line, reference, shot_delay, compare } = args;
        theme::setup(&cc.egui_ctx);
        let mut worker = Worker::spawn(osisub_core::paths::automation_include_dirs(), vec![]);
        wake_on_events(&mut worker, &cc.egui_ctx);
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
        crate::i18n::set_lang(crate::i18n::from_code(&std::env::var("OSISUB_LANG").unwrap_or_else(|_| settings.ui_lang.clone())));
        if settings.hotkey_defaults < 1 {
            crate::hotkeys::apply_defaults(&mut settings.macro_hotkeys, COMMAND_SCRIPT);
            settings.hotkey_defaults = 1;
            settings.save();
        }
        if settings.hotkey_defaults < 2 {
            crate::hotkeys::apply_default_list(&mut settings.macro_hotkeys, COMMAND_SCRIPT, crate::hotkeys::DEFAULT_COMMAND_KEYS_V2);
            settings.hotkey_defaults = 2;
            settings.save();
        }
        if settings.hotkey_defaults < 3 {
            crate::hotkeys::apply_default_list(&mut settings.macro_hotkeys, COMMAND_SCRIPT, crate::hotkeys::DEFAULT_COMMAND_KEYS_V3);
            settings.hotkey_defaults = 3;
            settings.save();
        }
        if settings.hotkey_defaults < 4 {
            crate::hotkeys::apply_default_list(&mut settings.macro_hotkeys, COMMAND_SCRIPT, crate::hotkeys::DEFAULT_COMMAND_KEYS_V4);
            settings.hotkey_defaults = 4;
            settings.save();
        }
        if settings.hotkey_defaults < 5 {
            crate::hotkeys::apply_default_list(&mut settings.macro_hotkeys, COMMAND_SCRIPT, crate::hotkeys::DEFAULT_COMMAND_KEYS_V5);
            settings.hotkey_defaults = 5;
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
            run_rev: 0,
            mt_rev: 0,
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
            mt_from_ref: false,
            cp: None,
            editor_cursor: None,
            editor_sel: None,
            pending_text_sel: None,
            line_initial: (usize::MAX, String::new(), 0),
            style_pv: None,
            style_pv_rx: None,
            style_pv_text: "Aa Iı İi Ğğ Şş Çç Öö Üü 123".into(),
            spell_rows: std::cell::RefCell::new((u64::MAX, Default::default())),
            qc_media_key: (0, false),
            select_crit: Default::default(),
            // UTF-8 olmayan (Excel'de kaydedilmiş) bellek de okunur; okunamazsa boş bellek kaydedilip dosyayı silmez (tm_load_failed)
            tm: tm_path().and_then(|p| std::fs::read(p).ok()).map(|b| osisub_core::tm::Memory::parse(&osisub_core::ass::decode_bytes(&b))).unwrap_or_default(),
            tm_rev: 0,
            tm_dirty: false,
            tm_cache: (usize::MAX, u64::MAX, vec![]),
            rows: vec![],
            rows_key: (u64::MAX, String::new()),
            folds: vec![],
            row_no: vec![],
            rules: Rules::default(),
            issues: vec![],
            qc_filter: "",
            issue_map: HashMap::new(),
            issues_key: u64::MAX,
            qc_rx: None,
            qc_seen_rev: u64::MAX,
            qc_changed_at: std::time::Instant::now(),
            width_issues: (u64::MAX, vec![]),
            width_rx: None,
            width_db: Default::default(),
            collide_issues: (u64::MAX, vec![]),
            collide_rx: None,
            start_buf: String::new(),
            end_buf: String::new(),
            style_sel: 0,
            scroll_to: None,
            anchor: 0,
            timing_open: false,
            shift_ms: 500,
            shift_use_frames: false,
            sync_pts: vec![],
            fps_from: 25.0,
            fps_to: 23.976,
            shift_frames: 24,
            shift_all: false,
            shift_after: false,
            shift_part: Default::default(),
            style_rename: None,
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
            find_pos: None,
            info_new: Default::default(),
            catalog_new: String::new(),
            fixes_preview: None,
            align_open: false,
            align_tol: 20.0,
            res_checked: None,
            res_prompt: None,
            resample_open: false,
            res_x: 1920,
            res_ar: Default::default(),
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
            guide: None,
            tr_progress: (u64::MAX, 0, 0, 0),
            tr_insert_original: false,
            about_open: false,
            settings,
            reference: None,
            ref_lines: vec![],
            fonts_open: false,
            fixes_open: false,
            stats_open: false,
            honor_open: false,
            bookmarks_open: false,
            backups_open: false,
            ready_open: false,
            mrep: None,
            mrep_key: None,
            console_code: "-- subs, sel, active kullanılabilir; örnek (Türkçe uyumlu büyük harf):\nlocal osisub = require \"osisub\"\nfor _, i in ipairs(sel) do\n  local l = subs[i]\n  l.text = osisub.text.upper(l.text)\n  subs[i] = l\nend\naegisub.log(#sel .. \" satır\\n\")".into(),
            console_pending: false,
            bookmark_edit: None,
            split_win: None,
            bookmark_ids: (u64::MAX, Default::default()),
            export_open: false,
            export_sel: vec![],
            export_job: None,
            export_builtins: Default::default(),
            tap: None,
            tap_latency_ms: 200,
            backed_up: Default::default(),
            theme_dirty: true,
            goto_open: false,
            hold_playing: false,
            last_title: None,
            egui_ctx: cc.egui_ctx.clone(),
            script_watch: crate::script_watch::ScriptWatch::spawn(cc.egui_ctx.clone()),
            dummy_win: None,
            ocr: None,
            hardsub: None,
            text_import: None,
            whisper: None,
            song_win: None,
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
            spell_custom: String::new(),
            spell_ignore: Default::default(),
            thesauri: Default::default(),
            ctx_word: None,
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
        app.load_thesauri();
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
                app.set_reference(sc);
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
            Some("mt") => app.mt_open = true,
            Some("karamode") => { app.media.kara_mode = true; app.media.kara_sel = Some((app.doc.active, 2)); }
            Some("timing") => app.timing_open = true,
            Some("resample") => app.resample_open = true,
            Some("dummy") => app.open_dummy_window(),
            Some("song") => app.song_win = Some(windows::SongWin::new()),
            Some("refoverlay") => { app.settings.ref_overlay = true; app.sync_ref_overlay(); }
            Some("guide") => app.guide = Some(String::new()),
            Some("textimport") => app.text_import = Some(windows::TextImportWin { text: "Sakura: Birinci çeviri satırı.\nKenji: İkinci satır.\n# atlanır\nÜçüncü satır".into(), speakers: true, ..Default::default() }),
            Some("mrep") => app.open_mrep(),
            Some("ready") => app.ready_open = true,
            Some("backups") => app.backups_open = true,
            Some("export") => app.export_open = true,
            Some("hardsub") => app.hardsub = Some(windows::HardsubWin::new(app.media.path.clone())),
            Some("split") => app.split_win = Some(("0:00:09.00".into(), 2, false)),
            Some("bookmarks") => app.bookmarks_open = true,
            Some("tool_draw") => app.media.tool = crate::media_ui::VideoTool::Drawing,
            Some("tool_shear") => app.media.tool = crate::media_ui::VideoTool::Shear,
            Some("ocr") => app.ocr = Some(windows::OcrWin::default()),
            Some("whisper") => app.whisper = Some(windows::WhisperWin::default()),
            Some("fixes") => app.fixes_open = true,
            Some("stats") => app.stats_open = true,
            Some("honor") => app.honor_open = true,
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
        if open_win.as_deref() == Some("tracks") {
            if let Some(v) = app.media.path.clone() {
                if let Ok(t) = osisub_media::mux::subtitle_tracks(&v) { app.tracks_win = Some((v, t)); }
            }
        }
        app.update_script_watch();
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
                self.status = tr("Yeni belge").into();
            }
            Guarded::OpenScript(sc, desc) => {
                self.doc = Document::new(sc);
                self.misses.clear();
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
                        self.misses.clear(); // yazım sonuçları eski belgeye aitti
                            if let Some(f) = self.first_row() { self.doc.select(vec![f], f); self.scroll_to = Some(f); }
                            self.status = trf!("Açıldı ({}): {} — kaydedince UTF-8 olur", label, p.display());
                        }
                        Err(e) => self.status = trf!("Hata: {}", e),
                    }
                }
            }
        }
    }

    fn help_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.help_open;
        egui::Window::new(tr("Kısayollar")).open(&mut open).default_width(560.0).show(ctx, |ui| {
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
        egui::Window::new(tr("Hakkında")).open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.heading(trf!("osisub {}", env!("CARGO_PKG_VERSION")));
            ui.label(tr("Anime altyazı stüdyosu — Aegisub Lua uyumlu, Subtitle Edit'in iyi özellikleriyle."));
            ui.label(RichText::new(tr("Belgeler: docs/KULLANIM.md · docs/LUA_UYUMLULUK.md · docs/ROADMAP.md")).weak());
        });
        self.about_open = open;
    }

    fn load_glossary(&mut self, p: PathBuf) {
        match std::fs::read(&p) {
            Ok(b) => {
                self.glossary = osisub_core::glossary::parse(&osisub_core::ass::decode_bytes(&b));
                self.settings.glossary_path = p.to_string_lossy().to_string();
                self.status = trf!("Terim sözlüğü: {} terim", self.glossary.len());
            }
            Err(e) => self.status = trf!("Terim sözlüğü okunamadı: {}", e),
        }
    }

    fn guard_window(&mut self, ctx: &egui::Context) {
        let Some(g) = self.guard.clone() else { return };
        let mut choice: Option<u8> = None;
        egui::Window::new(tr("Kaydedilmemiş değişiklikler")).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(tr("Kaydedilmemiş değişiklikler var. Devam etmeden önce ne yapılsın?"));
            ui.horizontal(|ui| {
                if ui.button(tr("Kaydet ve devam")).clicked() { choice = Some(0); }
                if ui.button(tr("Kaydetmeden devam")).clicked() { choice = Some(1); }
                if ui.button(tr("İptal")).clicked() { choice = Some(2); }
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
                        self.misses.clear(); // yazım sonuçları eski belgeye aitti
                if let Some(f) = self.first_row() {
                    self.doc.select(vec![f], f);
                }
                self.status = trf!("Açıldı: {}", p.display());
                self.settings.push_recent(&p.to_string_lossy());
                self.save_settings();
                self.pending_auto_media = true;
            }
            Err(e) => self.status = trf!("Hata: {}", e),
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
            self.status = trf!("Ortam otomatik açıldı: {}", m.display());
            // Aegisub gibi: projeyle birlikte açılan videoda çözünürlük sorulmaz (yalnız PlayRes yoksa ayarlanır)
            if self.doc.script.info("PlayResX").is_some() || self.doc.script.info("PlayResY").is_some() { self.res_checked = Some(m.clone()); }
            self.media.open(m, ctx);
            self.show_media = true;
        }
    }

    /// Kaydederken Aegisub uyumlu proje bilgisini güncelle (yalnızca ortam açıkken).
    fn write_project_garbage(&mut self, script_path: &std::path::Path) {
        // Garbage satırları olay satırlarından önce eklenir: sonradan dizinleri (etkin satır, seçim) kaydırmak için.
        let before = self.doc.script.entries.len();
        let header_end = self.doc.script.entries.iter().position(|e| !matches!(e.section(), osisub_core::ass::SEC_INFO | osisub_core::ass::SEC_GARBAGE)).unwrap_or(before);
        // etkin satırın diyalog sırası, ekleme öncesi hesaplanır
        let active_pos = matches!(self.doc.script.entries.get(self.doc.active), Some(Entry::Dialogue(_)))
            .then(|| self.doc.script.entries[..self.doc.active].iter().filter(|e| matches!(e, Entry::Dialogue(_))).count());
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
            // Aegisub: etkin satırın diyaloglar arasındaki sırası (ızgara süzgecinden bağımsız)
            if let Some(pos) = active_pos {
                self.doc.script.set_garbage("Active Line", &(pos + 1).to_string());
            }
            if let Some(f) = self.media.framerate() {
                self.doc.script.set_garbage("Video Position", &f.frame_at(self.media.playhead).to_string());
            }
        }
        let added = self.doc.script.entries.len() - before;
        if added > 0 {
            // geri alma geçmişindeki konumlar da kaydırılır (yoksa Ctrl+Z yanlış satıra yazar)
            self.doc.note_external_insert(header_end, added);
            if let Some(t) = self.tap.as_mut() { if t.line >= header_end { t.line += added; } }
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
            // her kayıtta önceki hali zaman damgalı yedeğe (dosya başına son 20; Dosya → Yedekler)
            if let Some(dir) = Self::backup_dir() {
                if let Err(e) = osisub_core::backup::make(&dir, &p, 20, std::time::SystemTime::now()) { self.log.push_str(&format!("Yedek alınamadı: {e}\n")); }
            }
            self.write_project_garbage(&p);
            match self.doc.save_as(p.clone()) {
                Ok(()) => { self.status = trf!("Kaydedildi: {}", p.display()); self.settings.push_recent(&p.to_string_lossy()); self.clear_autosave(); self.save_tm(); }
                Err(e) => self.status = trf!("Hata: {}", e),
            }
        }
    }

    // ------------------------------------------------------------ Lua

    /// Sık işlemler araç çubuğu (Görünüm → Araç çubuğu).
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let b = |ui: &mut egui::Ui, t: &'static str, tip: &'static str| ui.add(egui::Button::new(tr(t)).small()).on_hover_text(tr(tip)).clicked();
            if b(ui, "Aç", "Ctrl+O") { self.request(Guarded::OpenDialog); }
            if b(ui, "Kaydet", "Ctrl+S") { self.save(false); }
            ui.separator();
            if b(ui, "Geri al", "Ctrl+Z") { if let Some(d) = self.doc.undo() { self.status = trf!("Geri alındı: {}", crate::i18n::tr_msg(&d)); } }
            if b(ui, "Yinele", "Ctrl+Y") { if let Some(d) = self.doc.redo() { self.status = trf!("Yinelendi: {}", crate::i18n::tr_msg(&d)); } }
            if b(ui, "Bul", "Ctrl+F") { self.find_open = true; self.find_focus = true; }
            ui.separator();
            if b(ui, "Satırı oynat", "F5") { self.run_command("play_line", ui.ctx()); }
            if b(ui, "Kaydır", "Zamanlama → Kaydır / kareye oturt") { self.timing_open = true; }
            if b(ui, "Yer imi", "Ctrl+F2") { self.run_command("bookmark_toggle", ui.ctx()); }
            ui.separator();
            if b(ui, "Kalite", "Kalite sekmesi") { self.tab = Tab::Quality; }
            if b(ui, "Yayına hazır mı?", "Kontrol listesi") { self.ready_open = true; }
            if b(ui, "Komut paleti", "Ctrl+Shift+P") { self.palette = Some((String::new(), 0)); }
        });
    }

    /// Lua konsolu: kodu geçici betik olarak sarar, yükler; yüklenince (pump_events) çalıştırılır.
    pub(super) fn run_console(&mut self) {
        let Some(dir) = osisub_core::paths::data_dir().map(|d| d.join("console")) else { self.status = tr("Veri klasörü yok").into(); return };
        if std::fs::create_dir_all(&dir).is_err() { self.status = tr("Konsol klasörü oluşturulamadı").into(); return; }
        let p = dir.join("konsol.lua");
        let src = format!("script_name = \"{CONSOLE_NAME}\"\naegisub.register_macro(\"Lua konsolu\", \"\", function(subs, sel, active)\n{}\nend)\n", self.console_code);
        if let Err(e) = std::fs::write(&p, src) { self.status = trf!("Konsol yazılamadı: {e}"); return; }
        self.console_pending = true;
        self.worker.send(Cmd::Load(p));
        self.status = tr("Lua konsolu çalışıyor… (hata satır numaraları 2 fazladır)").into();
    }

    fn run_macro(&mut self, id: usize) {
        if self.running.is_some() {
            return;
        }
        let Some(m) = self.macros.iter().find(|m| m.info.id == id) else { return };
        self.running = Some(m.info.name.clone());
        self.run_rev = self.doc.rev;
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
        // aegisub.gui: metin kutusundaki seçim (bağlantı, imleç) 0 tabanlı UTF-8 bayt; is_modified; active_row
        let at = self.doc.active;
        let (anchor, cursor) = match (self.editor_sel.filter(|s| s.0 == at), self.editor_cursor.filter(|c| c.0 == at)) {
            (Some((_, a, b)), Some((_, c))) => (if c == a { b } else { a }, c),
            (None, Some((_, c))) => (c, c),
            _ => (0, 0),
        };
        if let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at) {
            let byte = |ci: usize| d.text.char_indices().nth(ci).map_or(d.text.len(), |(b, _)| b);
            props.push(("text_sel_start".to_string(), byte(anchor).to_string()));
            props.push(("text_sel_end".to_string(), byte(cursor).to_string()));
        }
        props.push(("is_modified".to_string(), if self.doc.is_dirty() { "1" } else { "0" }.to_string()));
        // Aegisub Properties.active_row: olaylar içindeki 0 tabanlı satır (line->Row)
        props.push(("active_row".to_string(), self.doc.script.entries[..at.min(self.doc.script.entries.len())].iter().filter(|e| matches!(e, Entry::Dialogue(_))).count().to_string()));
        // Aegisub gibi: seçim boşsa etkin satır seçili sayılır.
        let mut selection = self.doc.selection.clone();
        if selection.is_empty() && self.doc.active < self.doc.script.entries.len() { selection.push(self.doc.active); }
        self.worker.send(Cmd::Run { id, script: self.doc.script.clone(), selection, active: self.doc.active, props });
    }

    /// Klasördeki .lua/.moon betiklerinin hepsini yükler (sıralı; hatalılar günlükte görünür).
    fn load_lua_dir(&mut self, d: &std::path::Path) {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        v.retain(|p| p.is_file() && p.extension().map_or(false, |e| e.eq_ignore_ascii_case("lua") || e.eq_ignore_ascii_case("moon")));
        v.sort();
        let n = v.len();
        for p in v {
            if !self.loaded_scripts.contains(&p) { self.load_lua(p); }
        }
        self.status = trf!("{} betik yükleniyor: {}", n, d.display());
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
        self.update_script_watch();
    }

    /// Motoru sıfırlayıp tüm betikleri yeniden yükler (betik geliştirirken değişiklikleri almak için).
    /// Elle yüklenmiş betiği kalıcı listeden çıkarır ve motoru onsuz yeniden yükler.
    pub(crate) fn remove_script(&mut self, p: &std::path::Path) {
        let ps = p.to_string_lossy().to_string();
        self.settings.scripts.retain(|s| *s != ps);
        self.settings.save();
        self.loaded_scripts.retain(|x| x != p);
        self.reload_scripts();
        self.status = trf!("Betik kaldırıldı: {}", p.display());
    }

    fn reload_scripts(&mut self) {
        self.dialog = None;
        self.running = None;
        self.tap = None;
        self.worker = Worker::spawn(osisub_core::paths::automation_include_dirs(), vec![]);
        wake_on_events(&mut self.worker, &self.egui_ctx);
        self.macros.clear();
        let list = self.loaded_scripts.clone();
        for p in list { self.worker.send(Cmd::Load(p)); }
        self.log.push_str("— betikler yeniden yüklendi —\n");
        self.update_script_watch();
    }

    /// İzlenen betikler: yüklü betik dosyaları + include ve autoload klasörleri (require edilen modüller).
    fn update_script_watch(&self) {
        let mut v: Vec<PathBuf> = self.loaded_scripts.iter().filter(|p| p.is_file()).cloned().collect();
        v.extend(osisub_core::paths::automation_include_dirs());
        v.extend(autoload_dir());
        self.script_watch.set(v);
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
        if self.script_watch.rx.try_recv().is_ok() {
            while self.script_watch.rx.try_recv().is_ok() {}
            // makro çalışırken ya da iletişim kutusu açıkken motoru sıfırlama
            if self.settings.lua_autoreload && self.running.is_none() && self.dialog.is_none() {
                self.reload_scripts();
                self.status = tr("Betik dosyası değişti: betikler yeniden yüklendi").into();
            }
        }
        while let Ok(ev) = self.worker.events.try_recv() {
            match ev {
                Event::Loaded(Ok((info, ms))) => {
                    let console = info.name == CONSOLE_NAME;
                    if console {
                        // konsol her çalıştırmada yeniden yüklenir: eski makro kaydı listeden çıkar
                        self.macros.retain(|m| m.script != CONSOLE_NAME);
                    }
                    let first = ms.first().map(|m| m.id);
                    for m in ms {
                        self.macros.push(MacroEntry { info: m, script: info.name.clone() });
                    }
                    if console {
                        if let (true, Some(id)) = (std::mem::take(&mut self.console_pending), first) {
                            if self.running.is_some() { self.status = tr("Lua konsolu: başka bir makro çalışıyor, bitince yeniden deneyin").into(); } else { self.run_macro(id); }
                        }
                        continue;
                    }
                    self.status = trf!("Betik yüklendi: {}", info.name);
                }
                Event::Loaded(Err(e)) => {
                    self.log.push_str(&format!("Betik yüklenemedi: {}\n", e));
                    self.status = tr("Betik yüklenemedi (günlüğe bakın)").into();
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
                                self.status = trf!("{}: iptal edildi; dışa aktarma durdu", name);
                            } else {
                                let mut job = self.export_job.take().unwrap();
                                job.script = done.script;
                                if job.queue.is_empty() {
                                    osisub_core::exportfilters::apply(&mut job.script, &self.export_builtins);
                                    match osisub_core::io::save(&job.script, &job.path) {
                                        Ok(()) => self.status = trf!("Dışa aktarıldı: {}", job.path.display()),
                                        Err(e) => self.status = trf!("Hata: {}", e),
                                    }
                                } else {
                                    let next = job.queue.remove(0);
                                    self.export_job = Some(job);
                                    self.run_export_step(next);
                                }
                            }
                        }
                        Ok(done) if self.doc.rev != self.run_rev => {
                            let _ = done;
                            self.log.push_str(&format!("{name}: belge makro çalışırken değişti; sonuç uygulanmadı\n"));
                            self.status = trf!("{}: belge makro çalışırken değişti, sonuç uygulanmadı — makroyu yeniden çalıştırın", name);
                        }
                        Ok(done) => {
                            let cancelled = done.cancelled;
                            let (tsel, stxt) = (done.text_selection, done.status_text.clone());
                            self.doc.apply_run_done(done, &name);
                            self.status = if cancelled { format!("{}: iptal edildi", name) } else { format!("{}: tamamlandı", name) };
                            if let (false, Some(t)) = (cancelled, stxt) {
                                self.status = t;
                            }
                            // Aegisub gibi: seçim değişiklikleri uygulandıktan sonra etkin satırın metninde
                            if let (false, Some((a, b)), Some(Entry::Dialogue(d))) = (cancelled, tsel, self.doc.script.entries.get(self.doc.active)) {
                                let ch = |bi: i64| { let bi = bi.clamp(0, d.text.len() as i64) as usize; d.text.char_indices().take_while(|(i, _)| *i < bi).count() };
                                self.pending_text_sel = Some((self.doc.active, ch(a), ch(b)));
                            }
                        }
                        Err((msg, log)) => {
                            self.export_job = None;
                            self.log.push_str(&log);
                            self.log.push_str(&format!("HATA ({}): {}\n", name, msg));
                            self.status = trf!("{}: hata (günlüğe bakın)", name);
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
        // satır ekleme/silme aynı indekse başka satır getirebilir: girdi sayısı değişince de yenile
        if self.line_initial.0 != self.doc.active || self.line_initial.2 != self.doc.script.entries.len() {
            if let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(self.doc.active) {
                self.line_initial = (self.doc.active, d.text.clone(), self.doc.script.entries.len());
            }
        }
        let key = (self.doc.rev, self.filter.clone());
        if self.rows_key != key {
            let f = self.filter.to_lowercase();
            // katlamalar (Aegisub folds): süzgeç boşken katlı blokların içi gizlenir
            self.folds = osisub_core::folds::list(&self.doc.script);
            let hidden = if f.is_empty() && self.folds.iter().any(|x| x.collapsed) { osisub_core::folds::hidden(&self.doc.script) } else { Default::default() };
            self.rows = self
                .doc
                .script
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| match e {
                    Entry::Dialogue(_) if hidden.contains(&i) => None,
                    Entry::Dialogue(d) if f.is_empty() || osisub_core::tags::strip_tags(&d.text).to_lowercase().contains(&f) || d.actor.to_lowercase().contains(&f) || d.style.to_lowercase().contains(&f) => Some(i),
                    _ => None,
                })
                .collect();
            // satır numarası: süzgeç yoksa gerçek sıra (katlı satırlar sayılır, Aegisub gibi)
            self.row_no = if hidden.is_empty() { vec![] } else {
                let mut n = 0;
                let mut ord = std::collections::HashMap::new();
                for (i, e) in self.doc.script.entries.iter().enumerate() { if matches!(e, Entry::Dialogue(_)) { n += 1; ord.insert(i, n); } }
                self.rows.iter().map(|i| ord[i]).collect()
            };
            self.rows_key = key;
        }
        if self.qc_seen_rev != self.doc.rev {
            self.qc_seen_rev = self.doc.rev;
            self.qc_changed_at = std::time::Instant::now();
        }
        // Ekran genişliği denetimi: belge durunca arka planda (küçük/orta belgelerde otomatik)
        if let Some(rx) = &self.width_rx {
            if let Ok((rev, w)) = rx.try_recv() {
                self.width_rx = None;
                if rev == self.doc.rev {
                    self.width_issues = (rev, w);
                    self.issues_key = u64::MAX; // birleştirmek için yeniden hesapla
                }
            }
        } else if self.width_issues.0 != self.doc.rev && self.doc.script.entries.len() < 5000 && self.qc_changed_at.elapsed() >= std::time::Duration::from_millis(600) {
            let (tx, rx) = std::sync::mpsc::channel();
            self.width_rx = Some(rx);
            let (script, rev, db, ctx2) = (self.doc.script.clone(), self.doc.rev, self.width_db.clone(), self.egui_ctx.clone());
            std::thread::spawn(move || {
                let v = match db.lock() {
                    Ok(mut db) => osisub_lua::fonts::width_issues(&mut db, &script).into_iter().map(|(entry, message)| Issue { entry, kind: Kind::LineTooLong, message }).collect(),
                    Err(_) => vec![],
                };
                let _ = tx.send((rev, v));
                ctx2.request_repaint();
            });
        }
        if let Some(rx) = &self.collide_rx {
            if let Ok((rev, r)) = rx.try_recv() {
                self.collide_rx = None;
                match r {
                    Ok(v) => {
                        self.status = if v.is_empty() { "Ekran çakışması yok".into() } else { format!("{} ekran çakışması bulundu (Kalite sekmesi)", v.len()) };
                        if rev == self.doc.rev {
                            self.collide_issues = (rev, v);
                            self.issues_key = u64::MAX;
                        }
                    }
                    Err(e) => self.status = trf!("Ekran çakışması denetlenemedi: {e}"),
                }
            }
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
            let vdur = self.media.info.as_ref().filter(|i| i.has_video).map(|i| i.duration_ms).unwrap_or(0);
            if self.doc.script.entries.len() < 5000 {
                let mut issues = qc::check(&self.doc.script, &self.rules);
                issues.extend(qc::custom_rules(&self.doc.script, &self.custom_rules()));
                issues.extend(qc::beyond_end(&self.doc.script, vdur));
                issues.extend(osisub_core::untranslated::check(&self.doc.script, self.reference.as_ref()));
                issues.extend(osisub_core::consistency::check(&self.doc.script, self.reference.as_ref()));
                if let Some((fr, kf)) = &frame_ctx {
                    issues.extend(qc::frame_issues(&self.doc.script, fr, kf, 3));
                }
                if self.width_issues.0 == self.doc.rev {
                    issues.extend(self.width_issues.1.iter().cloned());
                }
                if self.collide_issues.0 == self.doc.rev {
                    issues.extend(self.collide_issues.1.iter().cloned());
                }
                issues.sort_by_key(|x| x.entry);
                self.set_issues(issues, self.doc.rev);
            } else if self.qc_rx.is_none() && self.qc_changed_at.elapsed() >= std::time::Duration::from_millis(300) {
                // Büyük belge: düzenleme durunca arka planda hesapla.
                let (tx, rx) = std::sync::mpsc::channel();
                self.qc_rx = Some(rx);
                let (script, rules, rev, reference) = (self.doc.script.clone(), self.rules.clone(), self.doc.rev, self.reference.clone());
                let custom = self.custom_rules();
                std::thread::spawn(move || {
                    let mut issues = qc::check(&script, &rules);
                    issues.extend(qc::custom_rules(&script, &custom));
                    issues.extend(qc::beyond_end(&script, vdur));
                    issues.extend(osisub_core::untranslated::check(&script, reference.as_ref()));
                    issues.extend(osisub_core::consistency::check(&script, reference.as_ref()));
                    if let Some((fr, kf)) = &frame_ctx {
                        issues.extend(qc::frame_issues(&script, fr, kf, 3));
                    }
                    issues.sort_by_key(|x| x.entry);
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
        let Some((cl, pos)) = self.editor_cursor.filter(|(l, _)| *l == at) else { self.status = tr("Önce düzenleme kutusunda imleci konumlandırın").into(); return };
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
            self.status = trf!("Bölündü: {}", time::format_ass(t));
            self.editor_cursor = None;
        } else {
            self.status = tr("Bu konumda bölünemez (etiket içi, satır başı/sonu ya da süre dışı)").into();
        }
    }

    /// Aegisub "Resolution mismatch": video açılınca PlayRes video çözünürlüğünden farklıysa sorar
    /// (PlayRes hiç yoksa sessizce videoya ayarlar).
    fn res_mismatch_window(&mut self, ctx: &egui::Context) {
        if let (Some(p), Some(i)) = (self.media.path.clone(), self.media.info.clone()) {
            if self.res_checked.as_ref() != Some(&p) {
                self.res_checked = Some(p);
                if i.has_video && i.width > 0 && i.height > 0 && self.media.dummy.is_none() {
                    let (vw, vh) = (i.width as i32, i.height as i32);
                    let has = self.doc.script.info("PlayResX").is_some() || self.doc.script.info("PlayResY").is_some();
                    if !has {
                        let mut s = self.doc.script.clone();
                        s.set_info("PlayResX", &vw.to_string());
                        s.set_info("PlayResY", &vh.to_string());
                        self.doc.apply_new_script("Çözünürlüğü videoya ayarla", s);
                    } else if self.doc.script.res() != (vw, vh) && !self.settings.res_mismatch_ignore
                        // tam katlar (alt piksel hassasiyeti için 2× PlayRes) eşit sayılır
                        && !(self.doc.script.res().0 % vw == 0 && self.doc.script.res().1 % vh == 0) {
                        self.res_prompt = Some((vw, vh));
                    }
                }
            }
        }
        let Some((vw, vh)) = self.res_prompt else { return };
        let (sx, sy) = self.doc.script.res();
        let mut close = false;
        egui::Window::new(tr("Çözünürlük uyuşmuyor")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label(trf!("Betik çözünürlüğü {}×{}, video {}×{}.", sx, sy, vw, vh));
            ui.label(RichText::new(tr("Betik başka çözünürlükteki bir video için hazırlanmışsa yeniden örnekleyin; tabelalar ve kenarlıklar doğru ölçeklenir.")).weak().small());
            ui.horizontal(|ui| {
                if ui.button(tr("Yeniden örnekle…")).on_hover_text(tr("Stilleri, konumları, kenarlıkları ve çizimleri yeni çözünürlüğe ölçekler")).clicked() {
                    self.res_x = vw; self.res_y = vh; self.resample_open = true; close = true;
                }
                if ui.button(tr("Yalnız çözünürlüğü değiştir")).on_hover_text(tr("PlayRes'i video çözünürlüğü yapar; satırlar ölçeklenmez")).clicked() {
                    let mut s = self.doc.script.clone();
                    s.set_info("PlayResX", &vw.to_string());
                    s.set_info("PlayResY", &vh.to_string());
                    self.doc.apply_new_script("Çözünürlüğü videoya ayarla", s);
                    close = true;
                }
                if ui.button(tr("Yoksay")).clicked() { close = true; }
            });
            let mut never = self.settings.res_mismatch_ignore;
            if ui.checkbox(&mut never, tr("Bir daha sorma")).changed() { self.settings.res_mismatch_ignore = never; self.settings.save(); }
        });
        if close { self.res_prompt = None; }
    }

    /// Aegisub "Videoya hizala" penceresi; sonuç gelince etkin satırın zamanını ayarlar.
    fn align_window(&mut self, ctx: &egui::Context) {
        if let Some(r) = self.media.align_result.take() {
            match (r, self.media.framerate()) {
                (Ok((a, b)), Some(fr)) => {
                    let at = self.doc.active;
                    let (s, e) = (fr.time_at_start(a), fr.time_at_end(b));
                    if matches!(self.doc.script.entries.get(at), Some(Entry::Dialogue(_))) {
                        self.doc.edit("Videoya hizala", |tx| tx.dialogue_mut(at, |d| { d.start_time = s; d.end_time = e; }));
                        self.status = trf!("Hizalandı: kare {}–{} ({} – {})", a, b, time::format_ass(s), time::format_ass(e));
                    }
                }
                (Err(e), _) => self.status = trf!("Videoya hizala: {}", e),
                _ => {}
            }
        }
        if !self.align_open { return; }
        let mut open = true;
        egui::Window::new(tr("Videoya hizala")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(tr("Tabelanın göründüğü bir karede, tabelanın düz renkli bir noktasını seçin. Aynı renk ve şekil önceki/sonraki karelerde aranır; etkin satır tabelanın göründüğü karelere ayarlanır.")).weak().small());
            ui.horizontal(|ui| {
                if ui.add_enabled(self.media.info.as_ref().map_or(false, |i| i.has_video), egui::Button::new(tr("Videoda nokta seç"))).clicked() { self.media.align_pick = true; }
                if self.media.align_pick { ui.label(RichText::new(tr("videoya tıklayın (Esc: vazgeç)")).color(theme::ACCENT)); }
            });
            if let Some((x, y)) = self.media.align_point.as_mut() {
                ui.horizontal(|ui| {
                    ui.label("X"); ui.add(egui::DragValue::new(x));
                    ui.label("Y"); ui.add(egui::DragValue::new(y));
                });
            }
            ui.horizontal(|ui| {
                ui.label(tr("Renk toleransı"));
                ui.add(egui::DragValue::new(&mut self.align_tol).range(0.0..=255.0));
            });
            let ready = self.media.align_point.is_some() && !self.media.align_busy;
            if ui.add_enabled(ready, egui::Button::new(if self.media.align_busy { tr("Aranıyor…") } else { tr("Hizala") })).clicked() {
                if let Some((x, y)) = self.media.align_point { self.media.align_to_video(x, y, self.align_tol, ctx); }
            }
        });
        if !open { self.align_open = false; self.media.align_pick = false; }
    }

    /// Açık özel QC kuralları (ad, ifade, harf duyarlı).
    pub(crate) fn custom_rules(&self) -> Vec<(String, String, bool)> {
        self.settings.qc_custom.iter().filter(|r| r.0).map(|r| (r.1.clone(), r.2.clone(), r.3)).collect()
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
            "split_after" | "split_before" => {
                match self.media.cur_frame() {
                    Some((fr, f)) => { let n = self.doc.split_lines_at_frame(&fr, f, id == "split_after"); self.scroll_to = Some(self.doc.active); self.status = trf!("{} satır bölündü", n); }
                    None => self.status = tr("Önce video açın").into(),
                }
            }
            "snap_scene" => {
                match self.media.cur_frame() {
                    Some((fr, f)) if !self.media.keyframes.is_empty() => {
                        let mut kf: Vec<i64> = self.media.keyframes.iter().map(|&k| fr.frame_at(k)).collect();
                        kf.sort_unstable();
                        kf.dedup();
                        let count = self.media.frame_count().unwrap_or(kf.last().copied().unwrap_or(0) + 1);
                        self.doc.snap_to_scene(&fr, &kf, f, count);
                    }
                    _ => self.status = tr("Anahtar kareli video gerekli").into(),
                }
            }
            "shift_to_frame" => { if let Some((fr, f)) = self.media.cur_frame() { self.doc.shift_to_frame(&fr, f); } else { self.status = tr("Önce video açın").into(); } }
            // karaoke kipinde (Aegisub AudioTimingControllerKaraoke) seçili hece 1 cs değişir
            "start_inc" => { if !self.media.kara_modify(&mut self.doc, true, 1, false) { self.doc.nudge_active(false, 10); } }
            "start_dec" => { if !self.media.kara_modify(&mut self.doc, true, -1, false) { self.doc.nudge_active(false, -10); } }
            "length_inc" => { if !self.media.kara_modify(&mut self.doc, false, 1, false) { self.doc.nudge_active(true, 10); } }
            "length_dec" => { if !self.media.kara_modify(&mut self.doc, false, -1, false) { self.doc.nudge_active(true, -10); } }
            "length_inc_shift" => { if !self.media.kara_modify(&mut self.doc, false, 1, true) { self.doc.nudge_active(true, 10); } }
            "length_dec_shift" => { if !self.media.kara_modify(&mut self.doc, false, -1, true) { self.doc.nudge_active(true, -10); } }
            "kara_mode" => {
                self.media.kara_mode = !self.media.kara_mode;
                self.status = if self.media.kara_mode { tr("Karaoke kipi açık: tık = hece seç, sağ tık = heceyi oynat, Sonraki/Önceki heceden heceye") } else { tr("Karaoke kipi kapalı") }.into();
            }
            "lead_in" => { self.doc.add_lead(self.settings.lead_in_ms, 0); }
            "lead_out" => { self.doc.add_lead(0, self.settings.lead_out_ms); }
            "frame_next_large" => self.media.step_frames(10),
            "frame_prev_large" => self.media.step_frames(-10),
            "boundary_next" | "boundary_prev" => {
                // Aegisub video/frame/next|prev/boundary: etkin satırın ilk/son karesi, sonra komşu satır
                if let (Some((fr, f)), Some(Entry::Dialogue(d))) = (self.media.cur_frame(), self.doc.script.entries.get(self.doc.active).cloned()) {
                    let (s, e) = (fr.frame_at_start(d.start_time), fr.frame_at_end(d.end_time));
                    let next = id == "boundary_next";
                    let target = if next { [s, e].into_iter().find(|&t| t > f) } else { [e, s].into_iter().find(|&t| t < f) };
                    match target {
                        Some(t) => self.media.jump_frame(t),
                        None => {
                            let old = self.doc.active;
                            step_line(self, if next { 1 } else { -1 });
                            if self.doc.active != old {
                                if let Some(Entry::Dialogue(n)) = self.doc.script.entries.get(self.doc.active) {
                                    let t = if next { fr.frame_at_start(n.start_time) } else { fr.frame_at_end(n.end_time) };
                                    self.media.jump_frame(t);
                                }
                            }
                        }
                    }
                }
            }
            "select_visible" => {
                match self.media.cur_frame() {
                    Some((fr, f)) => { let n = self.doc.select_visible(&fr, f); self.scroll_to = Some(self.doc.active); self.status = trf!("{} satır seçildi (bu karede görünen)", n); }
                    None => self.status = tr("Önce video açın").into(),
                }
            }
            "insert_before_video" | "insert_after_video" => {
                match self.media.cur_frame() {
                    Some((fr, f)) => { if let Some(p) = self.doc.insert_at_frame(&fr, f, id == "insert_after_video", 2000) { self.scroll_to = Some(p); } }
                    None => self.status = tr("Önce video açın").into(),
                }
            }
            "move_up" | "move_down" => { if self.doc.move_selected(id == "move_up") { self.scroll_to = Some(self.doc.active); } }
            "split_frame" => { if let Some(fr) = &fr { if !self.doc.split_at_frame(fr, self.media.playhead) { self.status = tr("Bu kare satırın içinde değil").into(); } } }
            "dup_shift" => { if let Some(fr) = &fr { self.doc.duplicate_shift_frames(fr, 1); self.scroll_to = Some(self.doc.active); } }
            "play_line" => self.media.toggle_play(&self.doc, ctx),
            "play_head" => self.media.toggle_play_from_playhead(&self.doc, ctx),
            "hold_play" => self.status = tr("Basılı tutarak oynat: kısayol tuşunu basılı tutun (Yardım → Kısayolları düzenle)").into(),
            "set_start" => self.media.set_active_start(&mut self.doc),
            "set_end" => self.media.set_active_end(&mut self.doc),
            "frame_next" => self.media.step_frames(1),
            "frame_prev" => self.media.step_frames(-1),
            "kf_next" => { if !self.media.step_keyframe(1) { self.status = tr("Sonrasında anahtar kare yok").into(); } }
            "kf_prev" => { if !self.media.step_keyframe(-1) { self.status = tr("Öncesinde anahtar kare yok").into(); } }
            "safe_area" => self.media.safe_area = !self.media.safe_area,
            "style_assist" => self.style_assist = Some(String::new()),
            "kanji_timer" => self.kanji = Some(KanjiWin::default()),
            "file_batch" => self.filebatch = Some(FileBatchWin::default()),
            "case_upper" => self.change_case(osisub_core::casing::Case::Upper),
            "case_lower" => self.change_case(osisub_core::casing::Case::Lower),
            "case_title" => self.change_case(osisub_core::casing::Case::Title),
            "case_sentence" => self.change_case(osisub_core::casing::Case::Sentence),
            "play_sel" if self.media.kara_syls(&self.doc).is_some() => { if self.media.is_playing() { self.media.stop(); } else { self.media.play_syl(&self.doc, ctx); } }
            "play_sel" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::Selection),
            "play_before" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::Before),
            "play_after" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::After),
            "play_begin" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::Begin),
            "play_end" => self.media.play_part(&self.doc, ctx, crate::media_ui::PlayPart::End),
            "play_stop" => self.media.stop(),
            "commit_next" if self.media.kara_syls(&self.doc).is_some() => {
                // karaoke kipi: sonraki hece; son hecedeyse sonraki satır (Aegisub yeni satır açmaz)
                if !self.media.kara_step(&self.doc, ctx, 1) {
                    let old = self.doc.active;
                    step_line(self, 1);
                    if self.doc.active != old { self.media.kara_sel = Some((self.doc.active, 0)); }
                    self.media.play_syl(&self.doc, ctx);
                }
            }
            "line_prev" if self.media.kara_syls(&self.doc).is_some() => {
                if !self.media.kara_step(&self.doc, ctx, -1) {
                    let old = self.doc.active;
                    step_line(self, -1);
                    if self.doc.active != old { self.media.kara_sel = Some((self.doc.active, usize::MAX)); }
                    self.media.play_syl(&self.doc, ctx);
                }
            }
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
                if !self.doc.merge_selected_as(m) { self.status = tr("Birleştirmek için ardışık en az iki satır seçin").into(); }
            }
            "ocr_fix" => {
                match self.spell.clone() {
                    None => self.status = tr("Önce Yazım sekmesinden bir sözlük yükleyin (OCR dilindeki, ör. en_US)").into(),
                    Some(sp) => {
                        let mut targets = self.doc.target_lines();
                        if targets.len() <= 1 { targets = (0..self.doc.script.entries.len()).collect(); }
                        let mut s = self.doc.script.clone();
                        let mut total = 0;
                        for i in targets {
                            if let Some(Entry::Dialogue(d)) = s.entries.get_mut(i) {
                                if d.comment { continue; }
                                let (t, n) = osisub_core::ocrfix::fix_text(&d.text, &|w| sp.check(w));
                                if n > 0 { d.text = t; total += n; }
                            }
                        }
                        self.doc.apply_new_script("OCR hatalarını düzelt", s);
                        self.status = trf!("OCR: {total} sözcük düzeltildi (rn→m, l→I, 0→o…; Ctrl+Z geri alır)");
                    }
                }
            }
            "ready_check" => self.ready_open = true,
            "multi_replace" => self.open_mrep(),
            "silence_next" | "silence_prev" => {
                // Subtitle Edit Seek silence: ≥0.3 sn, tepe ≤ %10
                match self.media.waveform() {
                    None => self.status = tr("Önce ses/video açın (dalga formu yüklenmeli)").into(),
                    Some(w) => match w.seek_silence(self.media.playhead, id == "silence_next", 300, 0.10) {
                        Some(t) => self.media.playhead = t,
                        None => self.status = tr("Bu yönde sessiz bölüm yok").into(),
                    },
                }
            }
            "word_to_next" | "word_from_next" | "word_to_prev" | "word_from_prev" => {
                // Subtitle Edit: sözcüğü komşu satıra taşı (ızgara sırasıyla komşu; iki satır dengelenir)
                let at = self.doc.active;
                let p = self.rows.iter().position(|&r| r == at);
                let next = id.ends_with("next");
                let nb = p.and_then(|p| if next { self.rows.get(p + 1) } else { p.checked_sub(1).and_then(|q| self.rows.get(q)) }).copied();
                let (Some(nb), Some(Entry::Dialogue(cur))) = (nb, self.doc.script.entries.get(at).cloned()) else { self.status = tr("Komşu satır yok").into(); return; };
                let Some(Entry::Dialogue(other)) = self.doc.script.entries.get(nb).cloned() else { return };
                // (önceki/üst, sonraki/alt) çifti ve yön
                let (a, b, ia, ib, forward) = match id {
                    "word_to_next" => (&cur, &other, at, nb, true),
                    "word_from_next" => (&cur, &other, at, nb, false),
                    "word_to_prev" => (&other, &cur, nb, at, false),
                    _ => (&other, &cur, nb, at, true),
                };
                match osisub_core::lineops::move_word(&a.text, &b.text, forward, self.rules.max_line_len.max(10)) {
                    Some((ta, tb)) => self.doc.edit("Sözcük taşı", |tx| { tx.dialogue_mut(ia, |d| d.text = ta); tx.dialogue_mut(ib, |d| d.text = tb); }),
                    None => self.status = tr("Taşınacak sözcük yok").into(),
                }
            }
            "select_overlaps" => {
                let v = osisub_core::lineops::overlapping(&self.doc.script, true);
                if let Some(&a) = v.first() { self.scroll_to = Some(a); self.doc.select(v.clone(), a); }
                self.status = trf!("{} çakışan satır seçildi (aynı stil ve katman)", v.len());
            }
            "bookmark_toggle" => {
                let i = self.doc.active;
                let mut s = self.doc.script.clone();
                let on = osisub_core::bookmarks::get(&s, i).is_none();
                if osisub_core::bookmarks::set(&mut s, i, on.then_some("")) {
                    self.doc.apply_new_script(if on { "Yer imi koy" } else { "Yer imini kaldır" }, s);
                    self.status = if on { "Yer imi kondu (F2 sonraki, Shift+F2 önceki; not: Düzen → Yer imleri)".into() } else { "Yer imi kaldırıldı".into() };
                }
            }
            "bookmark_next" | "bookmark_prev" => {
                match osisub_core::bookmarks::next(&self.doc.script, self.doc.active, id == "bookmark_next") {
                    Some(e) => { self.doc.select(vec![e], e); self.scroll_to = Some(e); }
                    None => self.status = tr("Yer imi yok (Ctrl+F2 ile koyun)").into(),
                }
            }
            "bookmarks" => self.bookmarks_open = true,
            "recombine" => {
                let mut s = self.doc.script.clone();
                let r = osisub_core::lineops::recombine(&mut s, &self.doc.target_lines());
                if r.is_empty() { self.status = tr("Yeniden birleştirmek için en az iki satır seçin").into(); } else {
                    self.doc.apply_new_script("Satırları yeniden birleştir", s);
                    self.doc.active = r[0];
                    self.status = trf!("{} satır oluştu", r.len());
                    self.doc.selection = r;
                }
            }
            "swap_lines" => {
                let t = self.doc.target_lines();
                if t.len() != 2 { self.status = tr("Değiş tokuş için tam iki satır seçin").into(); } else {
                    let mut s = self.doc.script.clone();
                    s.entries.swap(t[0], t[1]);
                    self.doc.apply_new_script("Satırları değiş tokuş et", s);
                    self.doc.selection = t;
                }
            }
            "continuous_end" | "continuous_start" => {
                let mut s = self.doc.script.clone();
                let n = osisub_core::lineops::make_continuous(&mut s, &self.doc.target_lines(), id == "continuous_start");
                self.doc.apply_new_script("Zamanları ardışık yap", s);
                self.status = trf!("{n} boşluk/örtüşme kapatıldı");
            }
            "split_cursor" => self.split_at_cursor(false),
            "frame_save" | "frame_save_raw" | "frame_copy" | "frame_copy_raw" => self.media.export_frame(&self.doc, !id.ends_with("_raw"), id.starts_with("frame_copy"), ctx),
            "audio_clip" => self.media.save_audio_clip(&self.doc, ctx),
            "zoom_100" => self.media.zoom_preset = Some(1.0),
            "zoom_200" => self.media.zoom_preset = Some(2.0),
            "zoom_fit" => { self.media.video_zoom = 1.0; self.media.video_pan = egui::Vec2::ZERO; }
            "frame_save_subs" => self.media.export_frame_ex(&self.doc, true, true, false, ctx),
            "find_next" => { if self.find.pattern.is_empty() { self.find_open = true; self.find_focus = true; } else { self.find_next(); } }
            "replace_next" => { if self.find.pattern.is_empty() { self.find_open = true; self.find_focus = true; } else { self.find_replace_next(); } }
            "fold_create" => {
                let t = self.doc.target_lines();
                let mut sc = self.doc.script.clone();
                match (t.first(), t.last()) {
                    (Some(&a), Some(&b)) if a < b && osisub_core::folds::add(&mut sc, a, b) => { self.doc.apply_new_script("Katla", sc); self.doc.select(vec![a], a); }
                    _ => self.status = tr("Katlamak için en az iki satır seçin (başka bir katlamayı yarıda kesemez)").into(),
                }
            }
            "fold_toggle" | "fold_open" | "fold_close" | "fold_clear" | "fold_open_all" | "fold_close_all" | "fold_clear_all" => {
                let mut sc = self.doc.script.clone();
                let f = |c: bool| -> Option<bool> { match id { "fold_toggle" => Some(!c), "fold_open" | "fold_open_all" => Some(false), "fold_close" | "fold_close_all" => Some(true), _ => None } };
                let n = if id.ends_with("_all") { osisub_core::folds::update_all(&mut sc, f) } else {
                    let mut t = self.doc.target_lines();
                    if !t.contains(&self.doc.active) { t.push(self.doc.active); }
                    osisub_core::folds::update_at(&mut sc, &t, f)
                };
                if n == 0 { self.status = tr("Katlama yok").into(); } else { self.doc.apply_new_script("Katlama", sc); }
            }
            "align_video" => self.align_open = true,
            "jump_start" | "jump_end" => {
                if let (Some(fr), Some(Entry::Dialogue(d))) = (self.media.framerate(), self.doc.script.entries.get(self.doc.active)) {
                    let f = if id == "jump_start" { fr.frame_at_start(d.start_time) } else { fr.frame_at_end(d.end_time) };
                    self.media.jump_frame(f);
                }
            }
            "split_preserve" => {
                let at = self.doc.active;
                match self.editor_cursor.filter(|(l, _)| *l == at) {
                    Some((_, pos)) => { if self.doc.split_keep_times(at, pos) { self.editor_cursor = None; } else { self.status = tr("Bu konumda bölünemez (etiket içi, satır başı/sonu ya da süre dışı)").into(); } }
                    None => self.status = tr("Önce düzenleme kutusunda imleci konumlandırın").into(),
                }
            }
            "clear_text" | "revert_line" | "insert_original" => {
                let at = self.doc.active;
                if let Some(Entry::Dialogue(d)) = self.doc.script.entries.get(at) {
                    let orig = if self.line_initial.0 == at { self.line_initial.1.clone() } else { d.text.clone() };
                    let (desc, t) = match id {
                        "clear_text" => ("Metni temizle", osisub_core::tags::clear_text(&d.text)),
                        "revert_line" => ("Satırı geri al", orig),
                        _ => {
                            let chars: Vec<char> = d.text.chars().collect();
                            let (a, b) = self.editor_sel.filter(|s| s.0 == at).map(|s| (s.1, s.2)).unwrap_or((chars.len(), chars.len()));
                            let (a, b) = (a.min(chars.len()), b.min(chars.len()));
                            ("Orijinali ekle", chars[..a].iter().collect::<String>() + &orig + &chars[b..].iter().collect::<String>())
                        }
                    };
                    if t != d.text {
                        self.doc.edit(desc, |tx| tx.dialogue_mut(at, |x| x.text = t));
                    }
                }
            }
            "balance_lines" => self.rebreak_selected(true),
            "guide" => self.guide = Some(String::new()),
            "song_place" => self.song_win = Some(windows::SongWin::new()),
            "split_breaks" | "split_breaks_keep" | "split_karaoke" => {
                let n = self.doc.split_selected(id == "split_karaoke", id == "split_breaks_keep");
                self.status = if n == 0 { "Bölünecek satır yok (\\N ya da karaoke etiketi gerekli)".into() } else { format!("{n} satır bölündü") };
                self.scroll_to = Some(self.doc.active);
            }
            "unbreak_lines" => self.rebreak_selected(false),
            "split_cursor_frame" => self.split_at_cursor(true),
            "snap_scenes" => {
                if let Some(fr) = fr.filter(|_| !self.media.keyframes.is_empty()) {
                    let kf = self.media.keyframes.clone();
                    let mut s = self.doc.script.clone();
                    let n = qc::snap_to_keyframes(&mut s, &fr, &kf, 3);
                    self.doc.apply_new_script("Sahne değişimlerine yapıştır", s);
                    self.status = trf!("{} satır sahne değişimine yapıştırıldı", n);
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
            "tool_shear" => self.media.tool = T::Shear,
            _ => self.status = trf!("Bilinmeyen komut: {}", id),
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
                    self.status = tr("Kısayol atama iptal edildi").into();
                } else if key == Key::Backspace {
                    self.status = trf!("Kısayol kaldırıldı: {}", name);
                } else {
                    let sc = KeyboardShortcut::new(mods, key);
                    let keys = crate::hotkeys::format(&sc);
                    // aynı tuş başka makroda varsa ondan al
                    self.settings.macro_hotkeys.retain(|h| crate::hotkeys::parse(&h.keys) != Some(sc));
                    let label = if script == COMMAND_SCRIPT { COMMANDS.iter().find(|c| c.0 == name).map(|c| tr(c.1).to_string()).unwrap_or_else(|| name.clone()) } else { name.clone() };
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
                // Ctrl/Shift+ok, Home/End, sil tuşları metin kutusunda imleç/sözcük gezintisidir (Ctrl+←/→ vb.)
                let text_nav = !sc.modifiers.alt && crate::hotkeys::is_text_nav_key(sc.logical_key);
                if typing_now && ((plain && !fkey) || text_nav) {
                    continue;
                }
                if script == COMMAND_SCRIPT && name == "hold_play" {
                    // Tuş basılıyken çal, bırakınca dur (playhead durduğu yerde kalır)
                    let down = ctx.input_mut(|i| {
                        let _ = i.consume_shortcut(&sc);
                        i.key_down(sc.logical_key) && i.modifiers.matches_logically(sc.modifiers)
                    });
                    if down && !self.hold_playing {
                        self.hold_playing = true;
                        if !self.media.is_playing() { self.media.toggle_play_from_playhead(&self.doc, ctx); }
                    } else if !down && self.hold_playing {
                        self.hold_playing = false;
                        self.media.stop();
                    }
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
                        None => self.status = trf!("Kısayol {}: makro yüklü değil ({})", crate::hotkeys::format(&sc), name),
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
                    egui::Event::Paste(t) => { let n = self.doc.paste_lines(&t); if n > 0 { self.status = trf!("{} satır yapıştırıldı", n); self.scroll_to = Some(self.doc.active); } }
                    _ => {}
                }
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            match ext.as_str() {
                // .txt: anahtar kare / zaman kodu dosyası ("# keyframe format", "# timecode format") değilse düz metin altyazı
                "txt" if std::fs::read(&p).map_or(false, |b| String::from_utf8_lossy(&b[..b.len().min(64)]).trim_start().starts_with('#')) => {
                    self.status = tr("Anahtar kare/zaman kodu dosyası: Video menüsünden yükleyin").into();
                }
                e if osisub_core::io::OPEN_EXTS.contains(&e) => { self.request(Guarded::OpenPath(p)); }
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
            ui.menu_button(tr("Dosya"), |ui| {
                if ui.add(egui::Button::new(tr("Yeni")).shortcut_text("")).clicked() { ui.close(); self.request(Guarded::New); }
                if ui.add(egui::Button::new(tr("Aç…")).shortcut_text("Ctrl+O")).clicked() { ui.close(); self.request(Guarded::OpenDialog); }
                ui.menu_button(tr("Kodlamayla aç"), |ui| {
                    for (label, name) in osisub_core::io::ENCODINGS {
                        if ui.button(*name).clicked() { ui.close(); self.request(Guarded::OpenWithEncoding(label.to_string())); }
                    }
                });
                ui.menu_button(tr("Son açılanlar"), |ui| {
                    let recent = self.settings.recent.clone();
                    if recent.is_empty() { ui.label(RichText::new(tr("Yok")).weak()); }
                    for r in recent {
                        if ui.button(&r).clicked() { ui.close(); self.request(Guarded::OpenPath(PathBuf::from(&r))); }
                    }
                });
                if ui.button(tr("Toplu dosya işlemleri…")).on_hover_text(tr("Birden çok dosyayı dönüştür / düzelt / kaydır / yeniden örnekle (Subtitle Edit: Batch convert)")).clicked() { ui.close(); self.filebatch = Some(FileBatchWin::default()); }
                if ui.button(tr("OP/ED yerleştir…")).on_hover_text(tr("Önceki bölüme göre zamanlanmış şarkı dosyasını bu bölüme sesle eşleştirerek (ya da playhead'e) yerleştir")).clicked() { ui.close(); self.song_win = Some(windows::SongWin::new()); }
                if ui.button(tr("Başka dosyayı birleştir…")).on_hover_text(tr("Tabela / OP-ED / diyalog dosyalarını bu belgeye ekler: stil çakışmaları çözülür, farklı çözünürlük yeniden örneklenir, gömülü yazı tipleri taşınır")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        self.merge_win = Some((p, osisub_core::merge::MergeOptions { sort: true, ..Default::default() }, "0:00:00.00".into()));
                    }
                }
                if ui.button(tr("Düz metin içe aktar…")).on_hover_text(tr("Çevirmenin düz metnini zamanlanmış satırlara sırayla yerleştir ya da yeni satırlar oluştur; \"Ad: replik\" konuşana alınır (Subtitle Edit: Import plain text)")).clicked() { ui.close(); self.text_import = Some(windows::TextImportWin { speakers: true, ..Default::default() }); }
                if ui.button(tr("Dosyayı böl…")).on_hover_text(tr("Kesme zamanlarında (ör. OP sonu) ya da eşit satır sayısıyla ayrı dosyalara böl; başlık/stiller/ekler her parçaya kopyalanır (Subtitle Edit: Split)")).clicked() { ui.close(); self.split_win = Some((String::new(), 2, false)); }
                if ui.add_enabled(self.doc.path.is_some(), egui::Button::new(tr("Yedekler…"))).on_hover_text(tr("Bu dosyanın her kayıttan önceki halleri (son 20): karşılaştır ya da geri aç")).clicked() { ui.close(); self.backups_open = true; }
                if ui.button(tr("Başka sürümle karşılaştır…")).on_hover_text(tr("Açık belgeyi eski/başka bir sürümle karşılaştır (eklenen, silinen, değişen satırlar)")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        match osisub_core::io::load(&p) {
                            Ok(old) => {
                                let ch = osisub_core::compare::compare(&old, &self.doc.script);
                                self.status = trf!("{} fark", ch.len());
                                self.compare = Some((p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), old, ch));
                            }
                            Err(e) => self.status = trf!("Hata: {}", e),
                        }
                    }
                }
                if ui.button(tr("Referans altyazı aç…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Altyazı", osisub_core::io::OPEN_EXTS).pick_file() {
                        match osisub_core::io::load(&p) {
                            Ok(s) => { self.set_reference(s); self.status = trf!("Referans: {}", p.display()); }
                            Err(e) => self.status = trf!("Hata: {}", e),
                        }
                    }
                }
                if self.reference.is_some() && ui.button(tr("Referansı kapat")).clicked() { self.reference = None; self.ref_lines.clear(); self.issues_key = u64::MAX; self.sync_ref_overlay(); ui.close(); }
                ui.separator();
                if ui.add(egui::Button::new(tr("Kaydet")).shortcut_text("Ctrl+S")).clicked() { ui.close(); self.save(false); }
                if ui.add(egui::Button::new(tr("Farklı kaydet…")).shortcut_text("Ctrl+Shift+S")).clicked() { ui.close(); self.save(true); }
                if ui.button(tr("Dışa aktar (filtreler)…")).clicked() { ui.close(); self.export_open = true; }
                ui.menu_button(tr("Video ile"), |ui| {
                    if ui.button(tr("Resim altyazısını metne çevir (OCR)…")).on_hover_text(tr("Blu-ray PGS (.sup) ya da DVD VobSub (.idx/.sub) dosyası veya MKV izi → metin; Tesseract gerekir")).clicked() {
                        ui.close();
                        self.ocr = Some(windows::OcrWin::default());
                    }
                    if ui.button(tr("Gömülü (hardsub) altyazıyı metne çevir…")).on_hover_text(tr("Videoya yakılmış altyazıyı satır zamanlarıyla birlikte okur (Tesseract); çeviri kaynağı olarak referansa yüklenebilir")).clicked() {
                        ui.close();
                        self.hardsub = Some(windows::HardsubWin::new(self.media.path.clone()));
                    }
                    ui.menu_button(tr("Bölümler (chapters)"), |ui| {
                        let n = osisub_core::chapters::from_script(&self.doc.script).len();
                        ui.label(RichText::new(trf!("Belgede {n} bölüm (Efekt: chapter); MKV'ye gömerken eklenir")).weak());
                        if ui.button(tr("Video konumunda bölüm ekle")).on_hover_text(tr("Oynatma konumuna Efekt'i \"chapter\" olan bir yorum satırı ekler; metni bölüm adıdır")).clicked() {
                            ui.close();
                            let t = self.media.playhead.max(0);
                            let mut ch = osisub_core::chapters::from_script(&self.doc.script);
                            if !ch.iter().any(|c| c.start_ms == t) {
                                ch.push(osisub_core::chapters::Chapter { start_ms: t, title: format!("Bölüm {}", ch.len() + 1) });
                                ch.sort_by_key(|c| c.start_ms);
                                let mut s = self.doc.script.clone();
                                osisub_core::chapters::apply_to_script(&mut s, &ch);
                                self.doc.apply_new_script("Bölüm ekle", s);
                                self.status = trf!("Bölüm eklendi: {}", osisub_core::time::format_ass(t));
                            }
                        }
                        if ui.add_enabled(n > 0, egui::Button::new(tr("Dışa aktar (Matroska XML / OGM)…"))).clicked() {
                            ui.close();
                            if let Some(p) = rfd::FileDialog::new().add_filter("Matroska XML", &["xml"]).add_filter("OGM", &["txt"]).set_file_name("bolumler.xml").save_file() {
                                let ch = osisub_core::chapters::from_script(&self.doc.script);
                                let text = if osisub_core::io::ext_of(&p) == "txt" { osisub_core::chapters::to_ogm(&ch) } else { osisub_core::chapters::to_matroska_xml(&ch, "tur") };
                                self.status = match std::fs::write(&p, text) { Ok(()) => format!("{} bölüm yazıldı: {}", ch.len(), p.display()), Err(e) => e.to_string() };
                            }
                        }
                        if ui.button(tr("İçe aktar (XML / OGM / videodan)…")).on_hover_text(tr("Bölüm dosyası ya da MKV seçin; belgedeki bölüm satırlarının yerine geçer")).clicked() {
                            ui.close();
                            if let Some(p) = rfd::FileDialog::new().add_filter("Bölümler / video", &["xml", "txt", "mkv", "mp4"]).pick_file() {
                                let ch = match osisub_core::io::ext_of(&p).as_str() {
                                    "xml" | "txt" => std::fs::read(&p).map(|b| osisub_core::chapters::parse(&osisub_core::ass::decode_bytes(&b))).map_err(|e| e.to_string()),
                                    _ => osisub_media::mux::chapters(&p),
                                };
                                match ch {
                                    Ok(ch) if !ch.is_empty() => {
                                        let mut s = self.doc.script.clone();
                                        osisub_core::chapters::apply_to_script(&mut s, &ch);
                                        self.doc.apply_new_script("Bölümleri içe aktar", s);
                                        self.status = trf!("{} bölüm içe aktarıldı", ch.len());
                                    }
                                    Ok(_) => self.status = tr("Bölüm bulunamadı").into(),
                                    Err(e) => self.status = e,
                                }
                            }
                        }
                    });
                    if ui.button(tr("Sesten altyazı (Whisper)…")).on_hover_text(tr("Videonun sesini whisper.cpp ile yazıya döker: zamanlanmış satırlar (Japonca döküm ya da İngilizce çeviri)")).clicked() {
                        ui.close();
                        self.whisper = Some(windows::WhisperWin::default());
                    }
                    if ui.button(tr("Videodaki altyazı izleri…")).on_hover_text(tr("MKV/MP4 içindeki altyazıyı aç; ekli yazı tiplerini çıkar")).clicked() {
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
                    if ui.add_enabled(!busy, egui::Button::new(tr("MKV'ye göm (softsub)…"))).on_hover_text(tr("Açık videoya bu altyazıyı iz olarak ekler; kullanılan yazı tipleri de eklenir. Yeniden kodlama yok.")).clicked() { ui.close(); self.start_video_job(false); }
                    if ui.add_enabled(!busy, egui::Button::new(tr("Videoya yak (hardsub)…"))).on_hover_text(tr("x264 ile yeniden kodlar (CRF 18); ses kopyalanır")).clicked() { ui.close(); self.start_video_job(true); }
                });
                ui.separator();
                if ui.button(tr("Çıkış")).clicked() { ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close); }
            });
            ui.menu_button(tr("Düzen"), |ui| {
                if ui.add_enabled(self.doc.can_undo(), egui::Button::new(format!("{}{}", tr("Geri al"), self.doc.undo_desc().map(|d| format!(": {}", crate::i18n::tr_msg(&d))).unwrap_or_default())).shortcut_text("Ctrl+Z")).clicked() { self.doc.undo(); ui.close(); }
                if ui.add_enabled(self.doc.can_redo(), egui::Button::new(tr("Yinele")).shortcut_text("Ctrl+Y")).clicked() { self.doc.redo(); ui.close(); }
                ui.separator();
                if ui.button(tr("Geçmiş…")).on_hover_text(tr("Geri alma geçmişi: bir adıma tıklayarak o duruma dönün")).clicked() { ui.close(); self.history_open = true; }
                ui.menu_button(tr("Harf dönüştür"), |ui| {
                    use osisub_core::casing::Case;
                    for (c, l) in [(Case::Upper, "BÜYÜK HARF"), (Case::Lower, "küçük harf"), (Case::Title, "Başlık Biçimi"), (Case::Sentence, "Cümle biçimi.")] {
                        if ui.button(l).on_hover_text(tr("Seçili satırlar; etiketler ve \\N korunur, Türkçe i/İ ı/I")).clicked() { ui.close(); self.change_case(c); }
                    }
                });
                if ui.button(tr("Satırları dengele")).on_hover_text(tr("Seçili satırları satır başına en çok 'en uzun satır' sınırı kadar karakterle dengeli iki satıra kırar; sığanları tek satır yapar (Subtitle Edit: Auto balance)")).clicked() { ui.close(); self.rebreak_selected(true); }
                ui.menu_button(tr("Satırları böl"), |ui| {
                    if ui.button(tr("Satır sonlarından (süre metne orantılı)")).on_hover_text(tr("Her \\N ayrı satır olur; süre karakter sayısına göre paylaşılır (Aegisub/Subtitle Edit: Split)")).clicked() { ui.close(); self.run_command("split_breaks", ui.ctx()); }
                    if ui.button(tr("Satır sonlarından (aynı zaman)")).on_hover_text(tr("Her \\N aynı zamanlı ayrı satır olur (ör. ayrı konumlanacak iki konuşmacı)")).clicked() { ui.close(); self.run_command("split_breaks_keep", ui.ctx()); }
                    if ui.button(tr("Karaoke hecelerine")).on_hover_text(tr("Her \\k hecesi kendi süresinde ayrı satır olur (Aegisub: Split by karaoke)")).clicked() { ui.close(); self.run_command("split_karaoke", ui.ctx()); }
                });
                if ui.button(tr("Satır sonlarını kaldır")).on_hover_text(tr("Seçili satırlardaki \\N'leri boşluğa çevirir")).clicked() { ui.close(); self.rebreak_selected(false); }
                if ui.button(tr("Stil yardımcısı…")).on_hover_text(tr("Satır satır stil atama: süz, Enter = uygula + sonraki satır (Aegisub: Styling Assistant)")).clicked() { ui.close(); self.style_assist = Some(String::new()); }
                if ui.button(tr("Üzerine yapıştır…  Ctrl+Shift+V")).on_hover_text(tr("Panodaki satırların seçtiğiniz alanlarını (zaman, stil, metin…) seçili satırların üzerine yazar (Aegisub: Paste Over)")).clicked() { ui.close(); self.open_paste_over(None); }
                if ui.button(tr("Satırları seç…")).on_hover_text(tr("Stil, konuşan, efekt ya da metne göre seç (Aegisub: Select Lines)")).clicked() { self.select_open = true; ui.close(); }
                if ui.add(egui::Button::new(tr("Satır ekle")).shortcut_text("Ctrl+Enter")).clicked() { self.doc.insert_after_active(); self.scroll_to = Some(self.doc.active); ui.close(); }
                if ui.add(egui::Button::new(tr("Çoğalt")).shortcut_text("Ctrl+D")).clicked() { self.doc.duplicate_selected(); ui.close(); }
                if ui.add(egui::Button::new(tr("Sil")).shortcut_text("Del")).clicked() { self.doc.delete_selected(); ui.close(); }
                if ui.add(egui::Button::new(tr("Seçilenleri birleştir")).shortcut_text("Ctrl+M")).clicked() { self.doc.merge_selected(); ui.close(); }
                if ui.button(tr("Birleştir (ilkini koru)")).on_hover_text(tr("Zaman aralığı birleşir, metin ilk satırınki kalır")).clicked() { self.run_command("merge_keep", ui.ctx()); ui.close(); }
                if ui.button(tr("Karaoke olarak birleştir")).on_hover_text(tr("Her satır bir hece olur; \\k = satırın süresi")).clicked() { self.run_command("merge_kara", ui.ctx()); ui.close(); }
                if ui.button(tr("Satırları yeniden birleştir")).on_hover_text(tr("Aegisub Recombine Lines: örtüşen konuşmalar için bölünmüş \"A\" / \"A\\NB\" / \"B\" satırlarını her cümle tek satır olacak şekilde geri birleştirir")).clicked() { self.run_command("recombine", ui.ctx()); ui.close(); }
                if ui.button(tr("Çakışan satırları seç")).on_hover_text(tr("Aynı stil ve katmanda zamanı çakışan diyalog satırları (Aegisub: Select overlaps)")).clicked() { self.run_command("select_overlaps", ui.ctx()); ui.close(); }
                ui.menu_button(tr("Sözcük taşı"), |ui| {
                    for (id, l) in [("word_to_next", "Son sözcüğü sonraki satırın başına taşı"), ("word_from_next", "Sonraki satırın ilk sözcüğünü bu satıra al"), ("word_to_prev", "İlk sözcüğü önceki satırın sonuna taşı"), ("word_from_prev", "Önceki satırın son sözcüğünü bu satıra al")] {
                        if ui.button(tr(l)).clicked() { self.run_command(id, ui.ctx()); ui.close(); }
                    }
                });
                ui.menu_button(tr("Yer imleri"), |ui| {
                    if ui.add(egui::Button::new(tr("Yer imi koy / kaldır")).shortcut_text("Ctrl+F2")).clicked() { self.run_command("bookmark_toggle", ui.ctx()); ui.close(); }
                    if ui.add(egui::Button::new(tr("Sonraki yer imi")).shortcut_text("F2")).clicked() { self.run_command("bookmark_next", ui.ctx()); ui.close(); }
                    if ui.add(egui::Button::new(tr("Önceki yer imi")).shortcut_text("Shift+F2")).clicked() { self.run_command("bookmark_prev", ui.ctx()); ui.close(); }
                    if ui.button(tr("Yer imleri listesi…")).clicked() { self.bookmarks_open = true; ui.close(); }
                });
                if ui.button(tr("İki satırı değiş tokuş et")).clicked() { self.run_command("swap_lines", ui.ctx()); ui.close(); }
                ui.menu_button(tr("Katlama"), |ui| {
                    for (cmd, l) in [("fold_create", "Seçimi katla"), ("fold_toggle", "Aç/kapat"), ("fold_open", "Aç"), ("fold_close", "Kapat"), ("fold_clear", "Kaldır"), ("fold_open_all", "Tümünü aç"), ("fold_close_all", "Tümünü kapat"), ("fold_clear_all", "Tümünü kaldır")] {
                        if ui.button(tr(l)).clicked() { ui.close(); self.run_command(cmd, ui.ctx()); }
                    }
                }).response.on_hover_text(tr("Bir satır bloğunu (ör. tabelanın katmanları) tek satıra katlar; Aegisub katlamalarıyla aynı biçimde dosyaya kaydedilir"));
                if ui.button(tr("Karede görünen satırları seç")).on_hover_text(tr("Videonun geçerli karesinde görünen tüm satırlar (Aegisub: Select Visible)")).clicked() { self.run_command("select_visible", ui.ctx()); ui.close(); }
                if ui.button(tr("Satırları yukarı taşı")).clicked() { self.run_command("move_up", ui.ctx()); ui.close(); }
                if ui.button(tr("Satırları aşağı taşı")).clicked() { self.run_command("move_down", ui.ctx()); ui.close(); }
                if ui.button(tr("Arkasına video zamanında satır ekle")).clicked() { self.run_command("insert_after_video", ui.ctx()); ui.close(); }
            });
            ui.menu_button(tr("Zamanlama"), |ui| {
                if ui.button(tr("Kaydır / kareye oturt…")).clicked() { self.timing_open = true; ui.close(); }
                if ui.add_enabled(self.resync_rx.is_none(), egui::Button::new(tr("Sesle yeniden zamanla (TV → BD)…"))).on_hover_text(tr("Altyazı eski videoya göre zamanlanmışsa: açık (yeni) videonun sesiyle eşleyip her satır grubunu kaydırır (farklı kurgu, eklenen/çıkarılan sahneler, gecikme)")).clicked() { ui.close(); self.start_resync(); }
                ui.menu_button(tr("Zamanları ardışık yap"), |ui| {
                    if ui.button(tr("Bitişi sonraki satıra uzat")).on_hover_text(tr("Seçili satırlar arasındaki boşluk/örtüşme: her satırın bitişi sonrakinin başlangıcı olur")).clicked() { self.run_command("continuous_end", ui.ctx()); ui.close(); }
                    if ui.button(tr("Başlangıcı önceki satıra çek")).on_hover_text(tr("Her satırın başlangıcı öncekinin bitişi olur")).clicked() { self.run_command("continuous_start", ui.ctx()); ui.close(); }
                });
                if ui.button(tr("Toplu satır işlemleri…")).on_hover_text(tr("Aynı metinli/kısa satırları birleştir, süreleri okuma hızına göre hesapla (Subtitle Edit)")).clicked() { self.batch = Some(BatchWin::default()); ui.close(); }
                if ui.button(tr("Son-işleyici (lead-in/out, anahtar kare)…")).clicked() { self.timing_open = true; ui.close(); }
                if ui.add_enabled(self.media.waveform().is_some(), egui::Button::new(tr("Sesten satır oluştur…"))).clicked() { self.vad_open = true; ui.close(); }
                ui.separator();
                let fr = self.media.framerate();
                if ui.add_enabled(fr.is_some(), egui::Button::new(tr("Video karesinde böl"))).on_hover_text(tr("Etkin satırı videodaki karede ikiye böler (Aegisub: Split at video frame)")).clicked() {
                    ui.close();
                    if let Some(fr) = &fr {
                        if !self.doc.split_at_frame(fr, self.media.playhead) { self.status = tr("Bu kare satırın içinde değil").into(); }
                    }
                }
                if ui.add_enabled(fr.is_some(), egui::Button::new(tr("Çoğalt ve 1 kare kaydır"))).on_hover_text(tr("Kare kare tipografi: seçili satırların kopyası bir kare sonra")).clicked() {
                    ui.close();
                    if let Some(fr) = &fr { self.doc.duplicate_shift_frames(fr, 1); self.scroll_to = Some(self.doc.active); }
                }
                ui.menu_button(tr("Kareye göre (Aegisub)"), |ui| {
                    for (cmd, label, key, tip) in [
                        ("split_after", "Kareden sonra böl", "Ctrl+Shift+D", "Seçili satırlar bu karede biter, kopyaları sonraki karede başlar; kare satırın dışındaysa kopya yalnız bu kareyi kaplar (kare kare tipografi)"),
                        ("split_before", "Kareden önce böl", "", "Seçili satırlar önceki karede biter, kopyaları bu karede başlar"),
                        ("snap_scene", "Sahneye yapıştır", "Ctrl+5", "Seçili satırları geçerli kareyi çevreleyen anahtar karelere ayarlar (Aegisub: Snap to Scene)"),
                        ("shift_to_frame", "Geçerli kareye kaydır", "Ctrl+6", "Seçimi, etkin satır geçerli karede başlayacak kadar kaydırır"),
                        ("lead_in", "Giriş payı ekle", "C", "Son-işleyicideki lead-in süresini etkin satıra ekler"),
                        ("lead_out", "Çıkış payı ekle", "V", "Son-işleyicideki lead-out süresini etkin satıra ekler"),
                        ("align_video", "Videoya hizala…", "", "Tabelanın bir noktasını seçin: aynı renk ve şekil önceki/sonraki karelerde aranır, satır tabelanın göründüğü karelere ayarlanır (Aegisub: Align to Video)"),
                    ] {
                        let mut b = egui::Button::new(tr(label));
                        if !key.is_empty() { b = b.shortcut_text(key); }
                        if ui.add(b).on_hover_text(tr(tip)).clicked() { ui.close(); self.run_command(cmd, ui.ctx()); }
                    }
                });
                for only_sel in [false, true] {
                    ui.menu_button(tr(if only_sel { "Seçilenleri sırala" } else { "Sırala" }), |ui| {
                        for (k, l) in [(qc::SortKey::Start, "Başlangıç zamanına göre"), (qc::SortKey::End, "Bitiş zamanına göre"), (qc::SortKey::Style, "Stile göre"), (qc::SortKey::Actor, "Konuşana göre"), (qc::SortKey::Effect, "Efekte göre"), (qc::SortKey::Layer, "Katmana göre"), (qc::SortKey::Text, "Metne göre"), (qc::SortKey::TextRaw, "Metne göre (etiketler dahil)")] {
                            if ui.button(tr(l)).clicked() {
                                let mut s = self.doc.script.clone();
                                if only_sel { qc::sort_selected(&mut s, k, &self.doc.target_lines()); } else { qc::sort_by(&mut s, k); }
                                self.doc.apply_new_script("Sırala", s);
                                ui.close();
                            }
                        }
                    });
                }
            });
            ui.menu_button(tr("Görünüm"), |ui| {
                let mut light = self.settings.light_theme;
                if ui.checkbox(&mut light, tr("Açık tema")).changed() { self.settings.light_theme = light; self.theme_dirty = true; }
                ui.menu_button("Arayüz dili / Language", |ui| {
                    for (l, name) in [(crate::i18n::Lang::Tr, "Türkçe"), (crate::i18n::Lang::En, "English")] {
                        if ui.radio(crate::i18n::lang() == l, name).clicked() {
                            crate::i18n::set_lang(l);
                            self.settings.ui_lang = crate::i18n::code(l).into();
                            self.settings.save();
                            ui.close();
                        }
                    }
                });
                if ui.add(egui::Button::new(tr("Yakınlaştır")).shortcut_text("Ctrl +")).clicked() { self.settings.zoom = (self.settings.zoom + 0.1).min(2.5); self.theme_dirty = true; }
                if ui.add(egui::Button::new(tr("Uzaklaştır")).shortcut_text("Ctrl -")).clicked() { self.settings.zoom = (self.settings.zoom - 0.1).max(0.6); self.theme_dirty = true; }
                if ui.button(tr("Yakınlaştırmayı sıfırla")).clicked() { self.settings.zoom = 1.0; self.theme_dirty = true; }
                ui.separator();
                if ui.checkbox(&mut self.settings.toolbar, tr("Araç çubuğu")).on_hover_text(tr("Menünün altında sık işlemler (aç, kaydet, geri al, oynat, kalite…)")).changed() { self.settings.save(); }
                ui.menu_button(tr("Izgarada etiketler"), |ui| {
                    for (v, l, h) in [(0u8, "Göster", "Etiket bloklarını olduğu gibi göster"), (1, "Sadeleştir", "Her etiket bloğu {…} olarak görünür (Aegisub varsayılanı)"), (2, "Gizle", "Yalnızca görünür metin")] {
                        if ui.radio(self.settings.grid_tags == v, l).on_hover_text(h).clicked() { self.settings.grid_tags = v; self.settings.save(); ui.close(); }
                    }
                });
                ui.menu_button(tr("Izgara sütunları"), |ui| {
                    // Göster/gizle ve sırala (# ilk, Metin son sütun sabit)
                    let mut ch = false;
                    let cols = self.settings.grid_columns();
                    let n = cols.len();
                    egui::Grid::new("grid_cols_menu").num_columns(3).spacing([6.0, 2.0]).show(ui, |ui| {
                        for (i, c) in cols.into_iter().enumerate() {
                            let mut on = self.settings.grid_visible(c);
                            if ui.checkbox(&mut on, tr(c.label())).changed() { self.settings.set_grid_visible(c, on); ch = true; }
                            if ui.add_enabled(i > 0, egui::Button::new(tr("Yukarı")).small()).clicked() { self.settings.move_grid_column(c, -1); ch = true; }
                            if ui.add_enabled(i + 1 < n, egui::Button::new(tr("Aşağı")).small()).clicked() { self.settings.move_grid_column(c, 1); ch = true; }
                            ui.end_row();
                        }
                    });
                    if ui.button(tr("Varsayılana dön")).clicked() {
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
                if ui.add(egui::Button::new(tr("Zamana git…")).shortcut_text("Ctrl+G")).clicked() { self.goto_open = true; self.goto_focus = true; ui.close(); }
            });
            ui.menu_button(tr("Video"), |ui| {
                if ui.button(tr("Video / ses aç…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Ortam", &["mkv", "mp4", "avi", "webm", "mov", "mka", "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"]).pick_file() {
                        self.media.open(p, ui.ctx());
                        self.show_media = true;
                    }
                }
                if ui.button(tr("Sahte video aç…")).on_hover_text(tr("Videosuz zamanlama/tipografi için düz renkli video (Aegisub dummy video; projeye kaydedilir)")).clicked() {
                    ui.close();
                    self.open_dummy_window();
                }
                ui.checkbox(&mut self.show_media, tr("Medya panelini göster"));
                ui.menu_button(tr("Yakınlaştır"), |ui| {
                    if ui.button(tr("Panele sığdır")).clicked() { self.media.video_zoom = 1.0; self.media.video_pan = egui::Vec2::ZERO; ui.close(); }
                    for (z, l) in [(0.5, "%50"), (1.0, "%100 (gerçek boyut)"), (2.0, "%200")] {
                        if ui.button(tr(l)).clicked() { self.media.zoom_preset = Some(z); ui.close(); }
                    }
                });
                ui.menu_button(tr("Kareyi kaydet / kopyala"), |ui| {
                    let ok = self.media.info.as_ref().map_or(false, |i| i.has_video);
                    if ui.add_enabled(ok, egui::Button::new(tr("PNG olarak kaydet (altyazılı)…"))).clicked() { ui.close(); self.media.export_frame(&self.doc, true, false, ui.ctx()); }
                    if ui.add_enabled(ok, egui::Button::new(tr("PNG olarak kaydet (altyazısız)…"))).clicked() { ui.close(); self.media.export_frame(&self.doc, false, false, ui.ctx()); }
                    if ui.add_enabled(ok, egui::Button::new(tr("Panoya kopyala (altyazılı)"))).clicked() { ui.close(); self.media.export_frame(&self.doc, true, true, ui.ctx()); }
                    if ui.add_enabled(ok, egui::Button::new(tr("Panoya kopyala (altyazısız)"))).clicked() { ui.close(); self.media.export_frame(&self.doc, false, true, ui.ctx()); }
                    if ui.add_enabled(ok, egui::Button::new(tr("Yalnız altyazılar, saydam (PNG)…"))).on_hover_text(tr("Videosuz, saydam arka planlı kare: tabelayı başka programda incelemek/düzenlemek için")).clicked() { ui.close(); self.media.export_frame_ex(&self.doc, true, true, false, ui.ctx()); }
                });
                if ui.add_enabled(self.media.info.as_ref().map_or(false, |i| i.has_audio), egui::Button::new(tr("Seçili satırların sesini kaydet (WAV)…"))).on_hover_text(tr("Seçili satırların kapladığı aralığın sesi (Aegisub: Save audio clip) — çevirmene/kontrolcüye göndermek için")).clicked() { ui.close(); self.media.save_audio_clip(&self.doc, ui.ctx()); }
                ui.menu_button(tr("En-boy oranı"), |ui| {
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
                if ui.checkbox(&mut self.settings.ref_overlay, tr("Referans altyazıyı videoda göster")).on_hover_text(tr("Dosya → Referans altyazı aç ile yüklenen kaynak altyazı (ör. İngilizce) önizlemede üstte gri italik görünür; dosyaya yazılmaz")).changed() {
                    self.settings.save();
                    self.sync_ref_overlay();
                }
                ui.checkbox(&mut self.media.safe_area, tr("Güvenli alan kılavuzu")).on_hover_text(tr("Eylem güvenli (%90) ve başlık güvenli (%80) alan çerçeveleri; TV kırpmasına karşı tabela yerleşimi için"));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new(tr("< Önceki anahtar kare"))).clicked() { self.media.step_keyframe(-1); }
                    if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new(tr("Sonraki anahtar kare >"))).clicked() { self.media.step_keyframe(1); }
                });
                ui.checkbox(&mut self.media.detached, tr("Videoyu ayrı pencerede göster")).on_hover_text(tr("Çift monitör için (Aegisub: Detach video); pencereyi kapatınca panele döner"));
                if ui.add_enabled(self.media.video_zoom > 1.0, egui::Button::new(format!("Video yakınlaştırmasını sıfırla ({:.0}%)", self.media.video_zoom * 100.0))).on_hover_text(tr("Videoda Ctrl+tekerlek: yakınlaştır, orta tuş: kaydır, orta çift tık: sıfırla")).clicked() { self.media.video_zoom = 1.0; self.media.video_pan = egui::Vec2::ZERO; ui.close(); }
                if ui.button(tr("Anahtar kareleri yükle…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Anahtar kare", &["txt", "log", "stats", "keyframes"]).pick_file() {
                        match std::fs::read_to_string(&p) {
                            Ok(t) => {
                                self.media.keyframes = osisub_media::parse_keyframe_file(&t, self.media.fps().unwrap_or(23.976));
                                self.status = trf!("{} anahtar kare yüklendi", self.media.keyframes.len());
                            }
                            Err(e) => self.status = trf!("Hata: {}", e),
                        }
                    }
                }
                if ui.add_enabled(!self.media.keyframes.is_empty(), egui::Button::new(tr("Anahtar kareleri kaydet…"))).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Anahtar kare", &["txt"]).save_file() {
                        let t = osisub_media::write_keyframe_file(&self.media.keyframes, self.media.fps().unwrap_or(23.976));
                        match std::fs::write(&p, t) { Ok(()) => self.status = tr("Anahtar kareler kaydedildi").into(), Err(e) => self.status = trf!("Hata: {}", e) }
                    }
                }
                ui.separator();
                if ui.button(tr("Timecode dosyası aç (VFR)…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Timecode", &["txt", "tc"]).pick_file() {
                        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| osisub_core::vfr::Framerate::parse_timecodes(&t)) {
                            Ok(fr) => {
                                self.status = trf!("Timecode yüklendi: ortalama {:.3} fps", fr.fps());
                                self.media.timecodes = Some(std::sync::Arc::new(fr));
                                self.media.timecodes_auto = false;
                            }
                            Err(e) => self.status = trf!("Timecode hatası: {}", e),
                        }
                    }
                }
                if ui.add_enabled(self.media.framerate().is_some(), egui::Button::new(tr("Timecode kaydet (v2)…"))).clicked() {
                    ui.close();
                    if let (Some(fr), Some(p)) = (self.media.framerate(), rfd::FileDialog::new().add_filter("Timecode", &["txt"]).save_file()) {
                        // Kare sayısı: videonun süresi ya da son altyazı bitişi
                        let end = self.media.info.as_ref().map(|i| i.duration_ms).filter(|d| *d > 0).unwrap_or_else(|| self.doc.script.dialogues().map(|d| d.end_time).max().unwrap_or(0));
                        let frames = (fr.frame_at(end) + 1).max(1) as usize;
                        match std::fs::write(&p, fr.to_v2(frames)) { Ok(()) => self.status = trf!("{} kare için timecode kaydedildi", frames), Err(e) => self.status = trf!("Hata: {}", e) }
                    }
                }
                if ui.add_enabled(self.media.timecodes.is_some(), egui::Button::new(tr("Timecode'u kapat"))).clicked() {
                    ui.close();
                    self.media.timecodes = None;
                    self.media.timecodes_auto = false;
                    self.status = tr("Timecode kapatıldı (videonun sabit kare hızı kullanılıyor)").into();
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
                    ui.label(RichText::new(tr("Perde korunur; zamanlamada yavaş dinlemek için")).small().weak());
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Ses"));
                    let mut v = (self.media.play_opts.volume * 100.0) as f32;
                    if ui.add(egui::Slider::new(&mut v, 0.0..=200.0).suffix("%").integer()).drag_stopped() {
                        self.media.play_opts.volume = v as f64 / 100.0;
                        self.save_settings();
                    } else {
                        self.media.play_opts.volume = v as f64 / 100.0;
                    }
                });
                ui.checkbox(&mut self.media.spectrum, tr("Spektrum göster"));
                ui.checkbox(&mut self.media.snap, tr("Karelere yapış"));
                ui.checkbox(&mut self.media.snap_kf, tr("Anahtar karelere yapış"));
                ui.checkbox(&mut self.media.kara_mode, tr("Karaoke kipi (hece hece zamanlama)")).on_hover_text(tr("Dalga formunda tık = hece seç, sağ tık = heceyi oynat; Sonraki/Önceki (X/Z) heceden heceye, S heceyi oynatır; başlangıç/uzunluk ±10 ms komutları (kısayol atanabilir) seçili heceye uygulanır"));
                ui.menu_button(tr("Video aracı"), |ui| {
                    use crate::media_ui::VideoTool as T;
                    for (t, l) in [(T::None, "Yok"), (T::Pos, "Konum (\\pos sürükle)"), (T::Move, "Hareket (\\move başlangıç/bitiş; sağ tık = \\pos)"), (T::Clip, "Kırpma (\\clip dikdörtgen; sağ tık = kaldır)"), (T::VClip, "Vektör kırpma (\\clip çizim: tıkla = nokta, sürükle, sağ tık = sil)"), (T::Rotate, "Döndürme (\\frz; Shift = 15°)"), (T::RotateXY, "3B döndürme (\\frx/\\fry)"), (T::Scale, "Ölçek (\\fscx/\\fscy)"), (T::Shear, "Eğme (\\fax/\\fay)"), (T::Perspective, "Perspektif (4 köşeyi sürükle)"), (T::Drawing, "Çizim (\\p noktaları)")] {
                        if ui.radio_value(&mut self.media.tool, t, l).clicked() { ui.close(); }
                    }
                    ui.checkbox(&mut self.media.persp_keep_org, tr("Perspektif: \\org'u koru")).on_hover_text(tr("Satırda \\org varsa yerinde kalır, yalnız dönüş/ölçek oturtulur (izlenen tabelalarda; Aegisub: Keep \\org)"));
                });
                if ui.add_enabled(self.media.is_open(), egui::Button::new(tr("Satırı oynat / durdur")).shortcut_text("F5")).clicked() { self.media.toggle_play(&self.doc, ui.ctx()); ui.close(); }
                if ui.add_enabled(self.media.is_open(), egui::Button::new(tr("Playhead'den oynat / durdur")).shortcut_text("Shift+F5")).clicked() { self.media.toggle_play_from_playhead(&self.doc, ui.ctx()); ui.close(); }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.media.is_open(), egui::Button::new(tr("< Sessizlik"))).on_hover_text(tr("Ses: önceki sessizliğe atla")).clicked() { self.run_command("silence_prev", ui.ctx()); }
                    if ui.add_enabled(self.media.is_open(), egui::Button::new(tr("Sessizlik >"))).on_hover_text(tr("Ses: sonraki sessizliğe atla (en az 0.3 sn, tepe ≤ %10)")).clicked() { self.run_command("silence_next", ui.ctx()); }
                });
            });
            ui.menu_button(tr("Otomasyon"), |ui| {
                if self.macros.is_empty() { ui.label(RichText::new(tr("Yüklü makro yok")).weak()); }
                let list: Vec<(usize, String, String)> = self.macros.iter().filter(|m| m.info.kind == MacroKind::Macro && m.script != CONSOLE_NAME).map(|m| (m.info.id, m.info.name.clone(), m.info.description.clone())).collect();
                // Aegisub gibi: "Grup/Alt/Makro" adları iç içe alt menülere ayrılır.
                let mut clicked = None;
                macro_menu(ui, &list, "", self.running.is_none(), &mut clicked);
                if let Some(id) = clicked { ui.close(); self.run_macro(id); }
                ui.separator();
                if ui.button(tr("Lua betiği yükle…")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Lua / MoonScript", &["lua", "moon"]).pick_file() { self.load_lua(p); }
                }
                if ui.button(tr("Klasördeki betikleri yükle…")).on_hover_text(tr("Seçilen klasördeki tüm .lua/.moon betiklerini yükler (alt klasörler hariç; kalıcı listeye eklenir)")).clicked() {
                    ui.close();
                    if let Some(d) = rfd::FileDialog::new().pick_folder() { self.load_lua_dir(&d); }
                }
                if ui.button(tr("Betikleri yeniden yükle")).clicked() { ui.close(); self.reload_scripts(); }
                if ui.checkbox(&mut self.settings.lua_autoreload, tr("Betik dosyası değişince otomatik yeniden yükle")).on_hover_text(tr("Yüklü betikler ya da include/autoload klasörlerindeki .lua/.moon dosyaları kaydedilince motor yeniden yüklenir (betik geliştirirken)")).changed() { self.settings.save(); }
            });
            ui.menu_button(tr("Araçlar"), |ui| {
                if ui.add(egui::Button::new(tr("Bul / Değiştir…")).shortcut_text("Ctrl+F")).clicked() { self.find_open = true; self.find_focus = true; ui.close(); }
                if ui.button(tr("Terim sözlüğü yükle…")).on_hover_text(tr("Her satır: kaynak = hedef")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Metin/CSV", &["txt", "csv", "tsv"]).pick_file() { self.load_glossary(p); }
                }
                ui.menu_button(format!("Çeviri belleği ({} çift)", self.tm.len()), |ui| {
                    if ui.add_enabled(!self.ref_lines.is_empty(), egui::Button::new(tr("Bu belgeyi belleğe ekle"))).on_hover_text(tr("Referansla örtüşen her satır: referans metni → çeviri")).clicked() {
                        ui.close();
                        let n = self.tm_learn_document();
                        self.save_tm();
                        self.status = trf!("Çeviri belleğine {} çift eklendi/güncellendi (toplam {})", n, self.tm.len());
                    }
                    if ui.button(tr("İçe aktar (TSV)…")).clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("TSV", &["tsv", "txt"]).pick_file() {
                            match std::fs::read(&p).map(|b| osisub_core::ass::decode_bytes(&b)) {
                                Ok(t) => {
                                    let before = self.tm.len();
                                    self.tm.merge(osisub_core::tm::Memory::parse(&t));
                                    self.tm_rev += 1; self.tm_dirty = true; self.save_tm();
                                    self.status = trf!("Çeviri belleği: {} → {} çift", before, self.tm.len());
                                }
                                Err(e) => self.status = trf!("Hata: {}", e),
                            }
                        }
                    }
                    if ui.add_enabled(!self.tm.is_empty(), egui::Button::new(tr("Dışa aktar (TSV)…"))).clicked() {
                        ui.close();
                        if let Some(p) = rfd::FileDialog::new().add_filter("TSV", &["tsv"]).save_file() {
                            match std::fs::write(&p, self.tm.to_tsv()) { Ok(()) => self.status = tr("Çeviri belleği dışa aktarıldı").into(), Err(e) => self.status = trf!("Hata: {}", e) }
                        }
                    }
                });
                if ui.button(tr("Ekler (gömülü yazı tipleri)…")).on_hover_text(tr("[Fonts]/[Graphics]: dosyaları ASS'e göm, çıkar, sil")).clicked() { self.attach_open = true; ui.close(); }
                if ui.button(tr("İstatistikler…")).clicked() { self.stats_open = true; ui.close(); }
                if ui.button(tr("Saygı ekleri (-san/-kun)…")).clicked() { self.honor_open = true; ui.close(); }
                if ui.button(tr("Çoklu değiştir…")).on_hover_text(tr("Kaydedilen bul → değiştir kurallarını tek tıkla uygula (Subtitle Edit: Multiple replace)")).clicked() { ui.close(); self.open_mrep(); }
                if ui.button(tr("OCR hatalarını düzelt")).on_hover_text(tr("OCR/hardsub sonrası: sözlükte olmayan sözcüklerde rn↔m, l↔I, 0↔O, vv→w gibi karışıklıkları dener, sözlükte bulunan biçimi yazar. Yazım sekmesinde yüklü sözlüğü kullanır; birden çok satır seçiliyse yalnızca onlara")).clicked() { self.run_command("ocr_fix", ui.ctx()); ui.close(); }
                if ui.button(tr("Sık hataları düzelt…")).clicked() { self.fixes_open = true; ui.close(); }
                if ui.button(tr("Yazı tipi denetimi…")).clicked() { self.fonts_open = true; self.start_font_scan(); ui.close(); }
                if ui.button(tr("Perspektife oturt")).on_hover_text(tr("Etkin satıra 4 noktalı vektör kırpma çizin (sol üst, sağ üst, sağ alt, sol alt); satır o dörtgene \\frx\\fry\\frz\\fscx\\fscy\\fax\\org ile oturtulur")).clicked() { ui.close(); self.perspective_fit(); }
                ui.menu_button(tr("Metin → çizim"), |ui| {
                    if ui.button(tr("Seçili satırları çizime (\\p1) dönüştür")).on_hover_text(tr("Gerçek yazı tipi konturlarıyla; görünüm aynı kalır (libass ile doğrulandı)")).clicked() { ui.close(); self.text_to_drawing(false); }
                    if ui.button(tr("Etkin satırı vektör kırpma olarak kopyala")).on_hover_text(tr("Metnin şeklini \\clip(...) olarak panoya kopyalar; başka bir satıra yapıştırın")).clicked() { ui.close(); self.text_to_drawing(true); }
                });
                if ui.button(tr("Makine çevirisi…")).on_hover_text(tr("DeepL / Google / LibreTranslate / Claude ile seçili satırları çevir (kendi API anahtarınızla)")).clicked() { ui.close(); self.mt_open = true; }
                if ui.button(tr("Kanji zamanlayıcı…")).on_hover_text(tr("Romaji karaoke zamanlamasını kanji/kana satırına aktar (Aegisub: Kanji Timer)")).clicked() { ui.close(); self.kanji = Some(KanjiWin::default()); }
                if ui.button(tr("Furigana düzeni")).on_hover_text(tr("Karaoke hecelerindeki 漢字|かんじ okunuşlarını ana satırın üstüne küçük, hecesiyle birlikte vurgulanan satırlar olarak yerleştirir (karaskel furigana; şablon gerekmez). Kaynak satır yorum olarak kalır; yeniden çalıştırınca çıktı yenilenir")).clicked() { ui.close(); self.furigana_layout(); }
                if ui.button(tr("Etiket uygula / yeniden hesapla…")).on_hover_text(tr("HYDRA + Recalculator benzeri: etiketleri başlangıca, tüm bloklara, \\t olarak ya da harf/satır degradesi olarak uygula; değerleri topla/çarp")).clicked() { ui.close(); self.tagw = Some(TagWin::default()); }
                if ui.button(tr("Degrade / parıltı…")).on_hover_text(tr("Kırpma şeritleriyle renk/saydamlık degradesi, parıltı (glow) katmanı")).clicked() { ui.close(); self.tsw = Some(TypesetWin::default()); }
                if ui.button(tr("Hareket izleme verisi uygula…")).on_hover_text(tr("Mocha / After Effects keyframe verisini seçili satırlara kare kare uygular (Aegisub-Motion)")).clicked() {
                    ui.close();
                    self.open_motion();
                }
                if ui.button(tr("Çözünürlüğü yeniden örnekle…")).clicked() {
                    let (x, y) = self.doc.script.res();
                    self.res_x = x; self.res_y = y; self.resample_open = true; ui.close();
                }
                ui.separator();
                if ui.button(tr("Yayına hazır mı?…")).on_hover_text(tr("Yayın öncesi kontrol listesi: başlık, zamanlama, metin, etiket, çeviri, yazım, notlar — tek bakışta")).clicked() { self.ready_open = true; ui.close(); }
                if ui.button(tr("Kalite kontrolü göster")).clicked() { self.tab = Tab::Quality; ui.close(); }
                if ui.button(tr("Metni temizle (etiket + boşluk)")).clicked() {
                    let mut s = self.doc.script.clone();
                    for d in s.dialogues_mut() { d.text = qc::clean_text(&d.text); }
                    self.doc.apply_new_script("Metni temizle", s);
                    ui.close();
                }
            });
            ui.menu_button(tr("Yardım"), |ui| {
                if ui.add(egui::Button::new(tr("Kullanım kılavuzu")).shortcut_text("F1")).clicked() { self.guide = Some(String::new()); ui.close(); }
                if ui.button(tr("Kısayollar")).clicked() { self.help_open = true; ui.close(); }
                if ui.add(egui::Button::new(tr("Komut paleti…")).shortcut_text("Ctrl+Shift+P")).clicked() { ui.close(); self.palette = Some((String::new(), 0)); }
                if ui.button(tr("Kısayolları düzenle…")).clicked() { self.hotkeys_open = true; ui.close(); }
                if ui.button(tr("Hakkında")).clicked() { self.about_open = true; ui.close(); }
            });
        });
    }

    /// Arayüz boştayken çizilmez: arka plan işi sürerken kısa aralıkla yokla, kirli belgede
    /// otomatik kayıt zamanı gelince uyan.
    fn schedule_wakeups(&self, ctx: &egui::Context) {
        let busy = self.qc_rx.is_some() || self.width_rx.is_some() || self.collide_rx.is_some() || self.spell_load_rx.is_some() || self.miss_rx.is_some() || self.font_rx.is_some() || self.running.is_some();
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.doc.is_dirty() {
            let left = 30u64.saturating_sub(self.last_autosave.elapsed().as_secs());
            ctx.request_repaint_after(std::time::Duration::from_secs(left.max(1)));
        }
    }

    pub(super) fn backup_dir() -> Option<PathBuf> {
        osisub_core::paths::data_dir().map(|d| d.join("autoback"))
    }

    fn autosave_dir() -> Option<PathBuf> {
        osisub_core::paths::data_dir().map(|d| d.join("autosave"))
    }

    fn autosave_name(&self) -> String {
        // farklı klasörlerdeki aynı adlı dosyalar (ep01/sub.ass, ep02/sub.ass) çakışmasın: tam yol özeti
        match &self.doc.path {
            Some(p) => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                p.hash(&mut h);
                let stem = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "adsiz".into());
                format!("{stem}-{:08x}", h.finish() as u32)
            }
            None => "adsiz".into(),
        }
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
        egui::Window::new(tr("Otomatik kayıt bulundu")).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(tr("Önceki oturum düzgün kapanmamış olabilir. Kurtarılabilir otomatik kayıtlar:"));
            for (i, r) in self.recovery.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(trf!("{} — {} dk önce", r.original.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| r.file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), r.age_min));
                    if ui.button(tr("Kurtar")).clicked() { act = Some((i, true)); }
                    if ui.button(tr("Sil")).clicked() { act = Some((i, false)); }
                });
            }
            if ui.button(tr("Hepsini yoksay")).clicked() { self.recovery.clear(); }
        });
        if let (Some((_, true)), true) = (act, self.doc.is_dirty()) {
            // açık belgede kaydedilmemiş değişiklik var: kurtarma onu sessizce silmesin
            self.status = tr("Önce açık belgedeki değişiklikleri kaydedin (kurtarma onları silerdi)").into();
        } else if let Some((i, recover)) = act {
            let r = self.recovery.remove(i);
            if recover {
                match osisub_core::io::load(&r.file) {
                    Ok(s) => {
                        let mut d = Document::new(s);
                        d.path = r.original.clone();
                        d.mark_dirty();
                        self.doc = d;
                        self.misses.clear(); // yazım sonuçları eski belgeye aitti
                        if let Some(f) = self.first_row() { self.doc.select(vec![f], f); self.scroll_to = Some(f); }
                        self.status = tr("Otomatik kayıttan kurtarıldı — kaydetmeyi unutmayın").into();
                    }
                    Err(e) => self.status = trf!("Kurtarılamadı: {}", e),
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
            // OSISUB_FRAME_LOG=2: yeniden çizimi isteyen kod yerlerini de yaz (boşta sürekli çizimi bulmak için)
            let causes = if std::env::var("OSISUB_FRAME_LOG").as_deref() == Ok("2") { format!(" {:?}", ui.ctx().repaint_causes()) } else { String::new() };
            eprintln!("kare {:.2} ms{}", t.elapsed().as_secs_f64() * 1000.0, causes);
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
        // Başlık yalnızca değişince gönderilir: her viewport komutu yeni bir kare ister (her karede
        // göndermek boştayken bile ~100 kare/sn sürekli çizime yol açıyordu).
        let title = self.title();
        if self.last_title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = Some(title);
        }

        if ctx.input(|i| i.viewport().close_requested()) && self.doc.is_dirty() && !self.quit_confirm && self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_confirm = true;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        if self.settings.toolbar {
            egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        }
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(j) = &self.job {
                    let p = j.progress.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                    ui.add(egui::ProgressBar::new(p).desired_width(160.0).text(format!("{} %{:.0}", j.label, p * 100.0)));
                    if ui.small_button(tr("İptal")).clicked() { j.cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let (w, h) = self.doc.script.res();
                    ui.label(RichText::new(trf!("{}×{}  |  seçili {}  |  {} sorun", w, h, self.doc.selection.len(), self.issues.len())).weak());
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
        self.honorifics_window(&ctx);
        self.bookmarks_window(&ctx);
        self.backups_window(&ctx);
        self.ready_window(&ctx);
        self.mrep_window(&ctx);
        self.split_window(&ctx);
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
        crate::guide_ui::show(&ctx, &mut self.guide);
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) { self.guide = if self.guide.is_some() { None } else { Some(String::new()) }; }
        self.ocr_window(&ctx);
        self.hardsub_window(&ctx);
        self.text_import_window(&ctx);
        self.whisper_window(&ctx);
        self.song_window(&ctx);
        self.import_window(&ctx);
        self.resample_window(&ctx);
        self.res_mismatch_window(&ctx);
        self.align_window(&ctx);
        self.autosave();
        self.schedule_wakeups(&ctx);
        if let Some((title, st)) = self.dialog.as_mut() {
            if let Some(res) = dialog_ui::show(&ctx, title, st) {
                self.worker.reply_dialog(res);
                self.dialog = None;
            }
        }
        if self.quit_confirm {
            egui::Window::new(tr("Kaydedilmemiş değişiklikler")).collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                ui.label(tr("Kaydedilmemiş değişiklikler var."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Kaydet ve çık")).clicked() { self.save(false); if !self.doc.is_dirty() { self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); } }
                    if ui.button(tr("Kaydetmeden çık")).clicked() { self.doc = Document::new(osisub_core::Script::new_default()); self.quit_confirm = false; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    if ui.button(tr("İptal")).clicked() { self.quit_confirm = false; }
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
    /// kanji gruplarına romajiden furigana okunuşu ekle (`君|きみ`)
    pub furigana: bool,
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
        // Belgedeki "chapter" satırları MKV bölümü olarak gömülür
        let ch = osisub_core::chapters::from_script(&self.doc.script);
        let chapters = (!ch.is_empty()).then(|| osisub_core::chapters::to_ffmetadata(&ch, duration.max(ch.last().map_or(0, |c| c.start_ms) + 1000)));
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
                let o = osisub_media::mux::MuxOptions { language: "tur".into(), title: "Türkçe".into(), fonts: font_files, default_track: true, keep_existing_subs: true, chapters };
                osisub_media::mux::mux_mkv(&video, &ass, &out, &o)
            };
            let _ = tx.send(r.map(|_| out.display().to_string()));
        });
        self.job = Some(VideoJob { label: if burn { "Videoya yakılıyor".into() } else { "MKV'ye gömülüyor".into() }, progress, cancel, rx });
    }

    fn start_resync(&mut self) {
        let Some(new) = self.media.path.clone() else { self.status = tr("Önce yeni videoyu açın (Video → Video / ses aç)").into(); return };
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
        self.status = tr("Sesle yeniden zamanlanıyor…").into();
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
                self.status = trf!("Yeniden zamanlandı: {} grup, {} farklı kayma{}", groups.len(), offs.len(), if weak > 0 { format!(", {weak} grup GÜVENSİZ (günlüğe bakın)") } else { String::new() });
                for g in groups.iter().filter(|g| g.fallback) {
                    self.log.push_str(&format!("Yeniden zamanlama güvensiz: {} – {} (ilinti {:.2}), önceki kayma {:+} ms kullanıldı\n", time::format_ass(g.start_ms), time::format_ass(g.end_ms), g.score, g.offset_ms));
                }
            }
            Err(e) => self.status = trf!("Yeniden zamanlama başarısız: {e}"),
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
        self.issues_key = u64::MAX; // çevrilmemiş satır denetimi referansı kullanır
        self.sync_ref_overlay();
    }

    /// Video önizlemesindeki referans katmanını ayara ve yüklü referansa göre günceller.
    pub(crate) fn sync_ref_overlay(&mut self) {
        let want = if self.settings.ref_overlay {
            self.reference.as_ref().map(|r| std::sync::Arc::new(r.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, osisub_core::tags::strip_tags(&d.text))).collect::<Vec<_>>()))
        } else {
            None
        };
        let changed = want.as_ref().map(|v| v.len()) != self.media.ref_overlay.as_ref().map(|v| v.len()) || want.is_some() != self.media.ref_overlay.is_some();
        self.media.ref_overlay = want;
        if changed || self.media.ref_overlay.is_some() {
            self.media.invalidate_frames();
        }
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
        self.status = trf!("{desc}: {n} satır");
    }

    /// Seçili satırlara harf dönüşümü (Türkçe i/İ duyarlı; etiketler korunur), tek geri-al adımı.
    pub(crate) fn change_case(&mut self, case: osisub_core::casing::Case) {
        let changes: Vec<(usize, String)> = self.doc.target_lines().into_iter().filter_map(|i| match self.doc.script.entries.get(i) {
            Some(Entry::Dialogue(d)) => { let t = osisub_core::casing::apply(&d.text, case, true); (t != d.text).then_some((i, t)) }
            _ => None,
        }).collect();
        let n = changes.len();
        if n > 0 { self.doc.edit("Harf dönüştür", |tx| for (i, t) in changes { tx.dialogue_mut(i, |d| d.text = t); }); }
        self.status = trf!("Harf dönüştürüldü: {n} satır");
    }

    /// Perspektif video aracı için etkin satırın kutusunu (yazı tipiyle ölçülür, metin değişince
    /// yeniden) ve kamera uzaklığını hazırlar; araç LayoutRes'siz betiğe yazdıysa LayoutRes'i ekler.
    fn prepare_perspective_tool(&mut self) {
        if self.media.tool == crate::media_ui::VideoTool::Drawing {
            self.media.persp_dist = self.perspective_dist_ro();
            return;
        }
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
        self.media.persp_dist = self.perspective_dist_ro();
    }

    /// `perspective_dist`'in betiği değiştirmeyen (ve kopyalamayan) hâli: her karede çağrılabilir.
    pub(crate) fn perspective_dist_ro(&self) -> f64 {
        let (_, play_y) = self.doc.script.res();
        let layout_y = self.doc.script.info("LayoutResY").and_then(|v| v.trim().parse::<f64>().ok())
            .or_else(|| self.media.info.as_ref().filter(|i| i.has_video).map(|i| i.height as f64))
            .unwrap_or(play_y as f64);
        osisub_core::perspective::dist_for(play_y as f64, layout_y)
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
            self.status = tr("Önce satıra 4 noktalı vektör kırpma çizin (Video aracı: Vektör kırpma): sol üst → sağ üst → sağ alt → sol alt").into();
            return;
        };
        let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
        let (w, h) = match osisub_lua::fonts::line_box(db, &self.doc.script, &d) { Ok(b) => b, Err(e) => { self.status = e; return; } };
        let mut s = self.doc.script.clone();
        let dist = self.perspective_dist(&mut s);
        let Some((p, org, err)) = perspective::fit(&quad, w, h, dist) else { self.status = tr("Perspektif çözümü bulunamadı").into(); return };
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
        self.status = trf!("Perspektif: köşe hatası {:.2} px, frx {:.1}° fry {:.1}°", err, p.frx, p.fry);
    }

    /// Seçili satırlara furigana düzeni (osisub_lua::fonts::apply_furigana); eklenen satırlar seçilir.
    fn furigana_layout(&mut self) {
        let db = self.font_db.get_or_insert_with(osisub_lua::fonts::FontDb::default);
        let targets = self.doc.target_lines();
        let mut s = self.doc.script.clone();
        match osisub_lua::fonts::apply_furigana(db, &mut s, &targets, osisub_core::furigana::SCALE) {
            Ok((0, _, _)) => self.status = tr("Seçili satırlarda furigana yok (hece biçimi: {\\k20}漢字|かんじ)").into(),
            Ok((n, added, skipped)) => {
                self.doc.apply_new_script("Furigana düzeni", s);
                if let Some(&a) = added.first() { self.doc.active = a; }
                self.doc.selection = added;
                self.status = trf!("Furigana: {n} satır düzenlendi (kaynaklar yorum olarak korundu)");
                if let Some(e) = skipped.first() { self.status.push_str(&format!("; {} satır atlandı: {e}", skipped.len())); }
            }
            Err(e) => self.status = e,
        }
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
                        self.status = tr("Vektör kırpma panoya kopyalandı (döndürme/eğme uygulanmaz)").into();
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
            self.status = trf!("{n} satır çizime dönüştürüldü");
        }
    }
}

/// Lua konsolunun geçici betik adı (makro menüsünde gizlenir).
pub(crate) const CONSOLE_NAME: &str = "osisub Lua konsolu";

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
