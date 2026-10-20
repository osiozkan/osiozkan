//! Interface language. UI strings are written in English in the code and looked up here;
//! Turkish is the default. `t("Open…")` translates a fixed string, `tf!("Saved {}", path)`
//! a template with `{}` placeholders. Messages produced by the core library (quality check)
//! are translated by pattern in [`message`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang { Tr, En }

impl Lang {
    pub fn code(self) -> &'static str { match self { Lang::Tr => "tr", Lang::En => "en" } }
    pub fn from_code(s: &str) -> Lang { if s.trim().eq_ignore_ascii_case("en") { Lang::En } else { Lang::Tr } }
    pub fn name(self) -> &'static str { match self { Lang::Tr => "Türkçe", Lang::En => "English" } }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) { LANG.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed); }
pub fn lang() -> Lang { if LANG.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::Tr } }

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| TR.iter().copied().collect())
}

/// Translate a UI string (returned unchanged in English or when no translation exists).
pub fn t(s: &'static str) -> &'static str {
    if lang() == Lang::En { return s; }
    table().get(s).copied().unwrap_or(s)
}

/// Is there a Turkish entry for `s`? (used by the completeness test)
#[cfg_attr(not(test), allow(dead_code))]
pub fn has(s: &str) -> bool { table().contains_key(s) }

/// Replace successive `{}` in `tpl` with `args`.
pub fn fill(tpl: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut it = args.iter();
    let mut rest = tpl;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        match it.next() { Some(a) => out.push_str(&a.to_string()), None => out.push_str("{}") }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// `tf!("Saved {}", path)`: translated template filled with Display arguments.
macro_rules! tf {
    ($tpl:literal $(, $arg:expr)* $(,)?) => {
        $crate::i18n::fill($crate::i18n::t($tpl), &[$(&$arg as &dyn std::fmt::Display),*])
    };
}
pub(crate) use tf;

/// Translate a message produced by the core library (quality check, fixes).
pub fn message(m: &str) -> String {
    if lang() == Lang::En { return m.to_string(); }
    static PATS: OnceLock<Vec<(regex::Regex, &'static str)>> = OnceLock::new();
    let pats = PATS.get_or_init(|| MESSAGES.iter().map(|(p, r)| (regex::Regex::new(&format!("^{}$", p)).expect("message pattern"), *r)).collect());
    for (re, rep) in pats {
        if let Some(c) = re.captures(m) {
            let mut out = String::new();
            c.expand(rep, &mut out);
            return out;
        }
    }
    m.to_string()
}

/// Core-library messages: regex (anchored) → Turkish template ($1, $2… are the captures).
const MESSAGES: &[(&str, &str)] = &[
    (r"End time is not after start time", "Bitiş, başlangıçtan sonra değil"),
    (r"Unbalanced \{ \} in text", "Metinde { } eşleşmiyor"),
    (r"Style '(.*)' does not exist", "'$1' stili yok"),
    (r"Line has no visible text", "Satırda görünen metin yok"),
    (r"Duration (\d+) ms below minimum", "Süre $1 ms, en azın altında"),
    (r"Duration (\d+) ms above maximum", "Süre $1 ms, en çoğun üstünde"),
    (r"Duration (\d+) ms below (\d+) ms", "Süre $1 ms, $2 ms'nin altında"),
    (r"Duration (\d+) ms above (\d+) ms", "Süre $1 ms, $2 ms'nin üstünde"),
    (r"([\d.]+) characters/second exceeds ([\d.]+)", "Saniyede $1 karakter, sınır $2"),
    (r"(\d+) lines \(max (\d+)\)", "$1 satır (en çok $2)"),
    (r"Line of (\d+) chars exceeds (\d+)", "$1 karakterlik satır, sınır $2"),
    (r"A line part of (\d+) chars exceeds (\d+) \(([\d+]+)\)", "Bölünmüş satırın $1 karakterlik parçası sınırı ($2) aşıyor ($3)"),
    (r"Double space", "Çift boşluk"),
    (r"Leading/trailing whitespace", "Baştaki/sondaki boşluk"),
    (r"Three dots; consider '…'", "Üç nokta; '…' kullanılabilir"),
    (r"Overlaps previous line by (-?\d+) ms", "Önceki satırla $1 ms çakışıyor"),
    (r"Gap (\d+) ms below minimum", "Ara $1 ms, en azın altında"),
    (r"Use the single ellipsis character", "Tek karakterlik üç nokta (…) kullanın"),
    (r"Two-speaker cue: both lines need a dash", "İki konuşmacılı satır: iki satırda da tire gerekir"),
    (r"Dialogue dash should be followed by a space", "Diyalog tiresinden sonra boşluk olmalı"),
    (r"Top line longer than bottom line", "Üst satır alttakinden uzun"),
    (r"Gap (\d+) ms below (\d+) frames \((\d+) ms\)", "Ara $1 ms, $2 karenin ($3 ms) altında"),
    (r"Gap (\d+) ms: close to (\d+) frames or chain lines", "Ara $1 ms: $2 kareye açın ya da satırları birleştirin"),
];

/// English → Turkish.
const TR: &[(&str, &str)] = &[
    // ---------------------------------------------------------------- menus
    ("File", "Dosya"), ("Edit", "Düzen"), ("Timing", "Zamanlama"), ("Tools", "Araçlar"), ("Automation", "Otomasyon"),
    ("Video", "Video"), ("View", "Görünüm"), ("Help", "Yardım"),
    ("Open recent", "Son açılanlar"), ("Export as…", "Farklı biçimde dışa aktar…"),
    ("Save encoding: {}", "Kaydetme kodlaması: {}"), ("Write byte-order mark", "BOM yaz (bayt sırası işareti)"),
    ("Frame rate", "Kare hızı"), ("No scripts loaded", "Yüklü betik yok"),
    // ---------------------------------------------------------------- commands (labels)
    ("New", "Yeni"), ("Open…", "Aç…"), ("Save", "Kaydet"), ("Save as…", "Farklı kaydet…"),
    ("Export SRT…", "SRT olarak dışa aktar…"), ("Export WebVTT…", "WebVTT olarak dışa aktar…"),
    ("Load reference / original subtitle…", "Referans / özgün altyazı yükle…"), ("Close reference", "Referansı kapat"),
    ("Script properties…", "Betik özellikleri…"), ("Quit", "Çık"),
    ("Undo", "Geri al"), ("Redo", "Yinele"), ("Insert line after", "Sonrasına satır ekle"), ("Insert line before", "Öncesine satır ekle"),
    ("Duplicate", "Çoğalt"), ("Join selected", "Seçilenleri birleştir"), ("Delete", "Sil"), ("Find & replace…", "Bul ve değiştir…"),
    ("Select all lines", "Tüm satırları seç"), ("Previous line", "Önceki satır"), ("Next line", "Sonraki satır"),
    ("Next line (new at the end)", "Sonraki satır (sondaysa yeni)"),
    ("Command palette…", "Komut paleti…"), ("Preferences…", "Ayarlar…"),
    ("Shift times…", "Zamanları kaydır…"), ("Snap to frames", "Karelere yapıştır"), ("Timing post-processor…", "Zamanlama son işlemcisi…"),
    ("Sort by start", "Başlangıca göre sırala"), ("Sort by end", "Bitişe göre sırala"), ("Sort by style", "Stile göre sırala"),
    ("Snap selected to keyframes (±300 ms)", "Seçilenleri anahtar karelere yapıştır (±300 ms)"),
    ("Styles manager…", "Stil yöneticisi…"), ("Quality check…", "Denetim (QC)…"), ("Resample resolution…", "Çözünürlüğü yeniden örnekle…"),
    ("Fix common errors…", "Yaygın hataları düzelt…"), ("Netflix-style quality check", "Netflix kurallarıyla denetim"),
    ("Fonts collector / embed…", "Yazı tipi toplayıcı / gömme…"), ("Karaoke editor…", "Karaoke düzenleyici…"),
    ("Clean tags (selected)", "Etiketleri toparla (seçili)"), ("Strip all tags (selected)", "Tüm etiketleri kaldır (seçili)"),
    ("Strip karaoke (selected)", "Karaoke etiketlerini kaldır (seçili)"), ("Remove duplicate lines", "Yinelenen satırları kaldır"),
    ("Trim whitespace (all)", "Boşlukları kırp (tümü)"),
    ("Reload scripts", "Betikleri yeniden yükle"), ("Add script folder…", "Betik klasörü ekle…"), ("Console", "Konsol"),
    ("Open video…", "Video aç…"), ("Open audio…", "Ses aç…"), ("Close media", "Videoyu/sesi kapat"),
    ("Set line start from video", "Satır başlangıcı = videodaki kare"), ("Set line end from video", "Satır bitişi = videodaki kare"),
    ("Play line", "Satırı oynat"), ("Play the end of the line", "Satırın sonunu oynat"), ("Play / pause", "Oynat / duraklat"),
    ("Previous frame", "Önceki kare"), ("Next frame", "Sonraki kare"), ("Back 1 second", "1 saniye geri"), ("Forward 1 second", "1 saniye ileri"),
    ("Go to line start", "Satır başına git"), ("Go to line end", "Satır sonuna git"),
    ("Play 500 ms before the start", "Başlangıçtan önceki 500 ms'yi oynat"), ("Play 500 ms after the end", "Bitişten sonraki 500 ms'yi oynat"),
    ("Play the first 500 ms", "İlk 500 ms'yi oynat"), ("Play the last 500 ms", "Son 500 ms'yi oynat"),
    ("Play from the playhead to the line end", "İmleçten satır sonuna oynat"), ("Stop", "Durdur"),
    ("Line start = playhead", "Başlangıç = oynatma imleci"), ("Line end = playhead", "Bitiş = oynatma imleci"),
    ("Scroll timing view left", "Zamanlama görünümünü sola kaydır"), ("Scroll timing view right", "Zamanlama görünümünü sağa kaydır"),
    ("Waveform / spectrum", "Dalga formu / spektrum"), ("Snap to keyframes", "Anahtar karelere yapıştır"), ("Follow active line", "Etkin satırı izle"),
    ("Preview panel", "Önizleme paneli"), ("Dark theme", "Koyu tema"), ("About Yuzu", "Yuzu hakkında"),
    ("Keyboard shortcuts…", "Klavye kısayolları…"),
    // command groups
    ("Line", "Satır"), ("Timing keys", "Zamanlama tuşları"),
    // ---------------------------------------------------------------- grid / editor
    ("#", "#"), ("L", "K"), ("Start", "Başlangıç"), ("End", "Bitiş"), ("Dur", "Süre"), ("CPS", "CPS"), ("Style", "Stil"),
    ("Actor", "Aktör"), ("Text", "Metin"), ("Effect", "Efekt"), ("Comment", "Yorum"), ("Layer", "Katman"), ("Margins", "Kenar boşlukları"),
    ("Duration", "Süre"), ("({} lines selected)", "({} satır seçili)"), ("Bold", "Kalın"), ("Italic", "Eğik"), ("Underline", "Altı çizili"),
    ("Strikeout", "Üstü çizili"), ("Insert line break", "Satır sonu ekle"), ("Top-center alignment", "Üst ortaya hizala"),
    ("Split", "Böl"), ("Split line at cursor, dividing time proportionally", "Satırı imleçten böl; süre orantılı bölünür"),
    ("Reference:", "Referans:"), ("Show lines without tags", "Satırları etiketsiz göster"),
    ("{} chars · {} CPS · {} ms", "{} karakter · {} CPS · {} ms"), ("Karaoke: {} syllables, {}s", "Karaoke: {} hece, {} sn"),
    ("Preview", "Önizleme"), ("Untitled", "Adsız"),
    // ---------------------------------------------------------------- status bar
    ("Ready", "Hazır"), ("Running: {}", "Çalışıyor: {}"), ("Cancel", "İptal"), ("OK", "Tamam"),
    ("{} lines · {}×{} · {} fps · {} · QC {}", "{} satır · {}×{} · {} fps · {} · QC {}"),
    ("Opened {}", "Açıldı: {}"), ("Open failed: {}", "Açılamadı: {}"), ("Saved {}", "Kaydedildi: {}"), ("Save failed: {}", "Kaydedilemedi: {}"),
    ("Exported {}", "Dışa aktarıldı: {}"), ("Removed {} duplicate line(s)", "{} yinelenen satır kaldırıldı"),
    ("{} automation script(s) loaded", "{} otomasyon betiği yüklendi"), (", {} failed (see Console)", ", {} tanesi yüklenemedi (Konsol'a bakın)"),
    ("Macro cancelled", "Makro iptal edildi"), ("Macro finished", "Makro bitti"), ("Macro failed (see Console)", "Makro başarısız (Konsol'a bakın)"),
    ("Video: {}×{} @ {} fps, {}s", "Video: {}×{} @ {} fps, {} sn"),
    ("Shortcuts saved", "Kısayollar kaydedildi"), ("Settings saved", "Ayarlar kaydedildi"),
    // ---------------------------------------------------------------- quit dialog
    ("Unsaved changes", "Kaydedilmemiş değişiklikler"), ("Save changes before closing?", "Kapatmadan önce değişiklikler kaydedilsin mi?"),
    ("Discard", "Kaydetme"),
    // ---------------------------------------------------------------- video / timing view
    ("Tool:", "Araç:"), ("No tool", "Araç yok"), ("Pos", "Konum"), ("Org", "Merkez"), ("Rot", "Döndür"), ("Clip", "Kırp"),
    ("Click/drag to set \\pos", "Tıkla/sürükle: \\pos"), ("Set \\org", "\\org koy"), ("Drag around the origin to set \\frz", "Merkez çevresinde sürükle: \\frz"),
    ("Drag a rectangle to set \\clip", "Dikdörtgen sürükle: \\clip"), ("inverse", "ters"),
    ("This frame is a keyframe", "Bu kare bir anahtar kare"), ("Rendering frame…", "Kare çiziliyor…"), ("Loading video…", "Video yükleniyor…"),
    ("{}  ·  frame {}", "{}  ·  kare {}"),
    ("Start=⏱ F1", "Başlangıç=⏱ F1"), ("End=⏱ F2", "Bitiş=⏱ F2"),
    ("Set line start to the video frame / playhead", "Satır başlangıcını videodaki kareye / oynatma imlecine koy"),
    ("Set line end to the video frame / playhead", "Satır bitişini videodaki kareye / oynatma imlecine koy"),
    ("▶ line", "▶ satır"),
    ("Waveform", "Dalga formu"), ("Spectrum", "Spektrum"),
    ("Mel spectrogram: speech onsets and syllables stand out", "Mel spektrogram: konuşma başlangıçları ve heceler belirgin"),
    ("computed after the audio is decoded", "ses çözüldükten sonra hesaplanır"),
    ("Zoom", "Yakınlaştırma"), ("Volume", "Ses düzeyi"), ("decoding audio…", "ses çözülüyor…"),
    ("ffmpeg/ffprobe not found in PATH", "ffmpeg/ffprobe PATH'te bulunamadı"),
    // ---------------------------------------------------------------- windows
    ("Find & replace", "Bul ve değiştir"), ("Find", "Bul"), ("Replace", "Değiştir"), ("Regex", "Düzenli ifade"), ("Match case", "Harf duyarlı"),
    ("Whole word", "Bütün sözcük"), ("Skip comments", "Yorumları atla"), ("In", "Nerede"),
    ("Text (raw)", "Metin (ham)"), ("Text (no tags)", "Metin (etiketsiz)"),
    ("Find next", "Sonrakini bul"), ("Select all matches", "Tüm eşleşmeleri seç"), ("Replace all", "Tümünü değiştir"),
    ("{} match(es)", "{} eşleşme"), ("No matches", "Eşleşme yok"), ("{} replacement(s)", "{} değiştirme"),
    ("With several lines selected, Replace all applies to the selection only.", "Birden çok satır seçiliyken Tümünü değiştir yalnız seçime uygulanır."),
    ("Shift times", "Zamanları kaydır"), ("Amount", "Miktar"), ("frames", "kare"), ("Start & end", "Başlangıç ve bitiş"),
    ("Start only", "Yalnız başlangıç"), ("End only", "Yalnız bitiş"), ("Selected lines", "Seçili satırlar"), ("All lines", "Tüm satırlar"),
    ("From selection onward", "Seçimden sonrası"), ("shifts the selected line and everything after it", "seçili satırı ve sonrasındaki her şeyi kaydırır"),
    ("Apply", "Uygula"),
    ("Styles manager", "Stil yöneticisi"), ("Copy", "Kopyala"), ("Name", "Ad"), ("Font", "Yazı tipi"), ("Size", "Boyut"), ("Spacing", "Aralık"),
    ("Scale X/Y", "Ölçek X/Y"), ("Angle", "Açı"), ("Outline", "Kenarlık"), ("Shadow", "Gölge"), ("Alignment", "Hizalama"),
    ("Border style", "Kenarlık türü"), ("Opaque box", "Opak kutu"), ("Outline + shadow", "Kenarlık + gölge"), ("Margins L/R/V", "Kenar boşluğu S/S/D"),
    ("Encoding", "Kodlama"), ("Primary", "Birincil"), ("Secondary", "İkincil"), ("opacity", "opaklık"), (" copy", " kopya"),
    ("Quality check", "Denetim (QC)"), ("Max CPS", "En çok CPS"), ("Max line length", "En uzun satır"), ("Min dur (ms)", "En kısa süre (ms)"),
    ("Max dur (ms)", "En uzun süre (ms)"), ("Run", "Çalıştır"), ("No issues found.", "Sorun bulunmadı."),
    ("ERR", "HATA"), ("WARN", "UYARI"), ("info", "bilgi"),
    ("Script properties", "Betik özellikleri"), ("Title", "Başlık"), ("Original Script", "Özgün betik"), ("Original Translation", "Özgün çeviri"),
    ("Original Editing", "Özgün düzenleme"), ("Original Timing", "Özgün zamanlama"), ("Synch Point", "Eşleme noktası"),
    ("Script Updated By", "Betiği güncelleyen"), ("Update Details", "Güncelleme ayrıntıları"),
    ("Automation console", "Otomasyon konsolu"), ("Clear", "Temizle"),
    ("Timing post-processor", "Zamanlama son işlemcisi"), ("Lead-in (ms)", "Önden pay (ms)"), ("Lead-out (ms)", "Arkadan pay (ms)"),
    ("Join gaps up to (ms)", "Şu kadar boşluğu kapat (ms)"), ("Minimum gap (ms)", "En az ara (ms)"), ("Apply to selected lines", "Seçili satırlara uygula"),
    ("Resample resolution", "Çözünürlüğü yeniden örnekle"), ("Current: {}×{}", "Şu an: {}×{}"), ("Resample", "Yeniden örnekle"),
    ("Fix common errors", "Yaygın hataları düzelt"), ("Language", "Dil"), ("Other", "Diğer"),
    ("Fix overlaps", "Çakışmaları düzelt"), ("Fix too-short durations", "Çok kısa süreleri düzelt"), ("Fix too-long durations", "Çok uzun süreleri düzelt"),
    ("Double spaces", "Çift boşluklar"), ("Missing spaces", "Eksik boşluklar"), ("Unneeded spaces", "Gereksiz boşluklar"),
    ("ALL CAPS to sentence case", "TÜMÜ BÜYÜK harften cümle düzenine"), ("I / l / İ / ı confusion", "I / l / İ / ı karışıklığı"),
    ("Dialogue dashes", "Diyalog tireleri"), ("Ellipsis", "Üç nokta"), ("Remove [sound] / ♪ / NAME:", "[ses] / ♪ / AD: kaldır"),
    ("Auto-balance line breaks", "Satır kırmalarını dengele"), ("Merge duplicate lines", "Yinelenen satırları birleştir"),
    ("Remove empty lines", "Boş satırları kaldır"), ("Curly quotes", "Kıvrık tırnak"), ("Apply to selection", "Seçime uygula"),
    ("Apply to all lines", "Tüm satırlara uygula"), ("{}: {} change(s)", "{}: {} değişiklik"),
    ("Undo (Ctrl+Z) reverts the whole run.", "Geri al (Ctrl+Z) tüm çalıştırmayı geri alır."),
    ("Fonts", "Yazı tipleri"), ("B/I", "K/E"), ("Lines", "Satır"), ("File ", "Dosya "), ("substitute: {}", "yerine: {}"), ("MISSING", "BULUNAMADI"),
    ("Copy fonts to folder…", "Yazı tiplerini klasöre kopyala…"), ("Save as .zip…", ".zip olarak kaydet…"),
    ("Embed used fonts into script", "Kullanılan yazı tiplerini betiğe göm"), ("{} embedded font(s):", "{} gömülü yazı tipi:"),
    ("remove", "kaldır"), ("extract…", "çıkar…"), ("embedded {}", "gömüldü: {}"),
    ("Karaoke editor", "Karaoke düzenleyici"), ("Line {} · {} – {}  ({}s)", "Satır {} · {} – {}  ({} sn)"),
    ("Syllable markup ( | splits )", "Hece işaretleme ( | böler )"), ("Auto split:", "Otomatik böl:"),
    ("char", "harf"), ("word", "sözcük"), ("romaji", "romaji"), ("kana", "kana"), ("Effect:", "Efekt:"),
    ("Even", "Eşit"), ("Fit to line", "Satıra sığdır"), ("Scale durations to the line's length", "Süreleri satırın uzunluğuna ölçekle"),
    ("Split in half", "İkiye böl"), ("Join with next", "Sonrakiyle birleştir"), ("Total {} cs / line {} cs", "Toplam {} cs / satır {} cs"),
    ("Tap latency", "Tıklama gecikmesi"), ("● Tap timing", "● Tıklayarak zamanla"),
    ("Plays the line; press Space at the end of each syllable", "Satırı oynatır; her hecenin sonunda Space'e basın"),
    ("■ Cancel tapping", "■ Tıklamayı iptal et"), ("Tapping… {}/{} — press Space", "Tıklanıyor… {}/{} — Space'e basın"),
    ("Tip: drag a syllable's duration to move the boundary with its neighbour (hold Shift to change only that syllable).",
     "İpucu: hecenin süresini sürüklemek komşusuyla sınırını taşır (yalnız o heceyi değiştirmek için Shift'e basılı tutun)."),
    ("Anime subtitle editor — Aegisub-compatible Lua automation, Subtitle Edit style tooling.",
     "Anime altyazı düzenleyici — Aegisub uyumlu Lua otomasyonu, Subtitle Edit tarzı araçlar."),
    ("Version {}", "Sürüm {}"), ("Video / audio: {}", "Video / ses: {}"), ("ffmpeg / ffprobe: {}", "ffmpeg / ffprobe: {}"),
    ("found", "bulundu"), ("not found (needed for waveform, keyframes and export)", "bulunamadı (dalga formu, anahtar kareler ve dışa aktarma için gerekli)"),
    // ---------------------------------------------------------------- Osi's tools
    ("Translation", "Çeviri"), ("CPL", "CPL"),
    ("Reference panel…", "Referans paneli…"), ("Reference column in the grid", "Izgarada Referans sütunu"),
    ("Compare with the source (aligncompare)…", "Kaynakla karşılaştır (aligncompare)…"), ("Sync to the reference…", "Referansa göre senkron…"),
    ("Sync to speech…", "Konuşmaya göre senkron…"), ("Term check…", "Terim denetimi…"), ("Spell check…", "Yazım denetimi…"), ("Create lines from speech", "Konuşmadan satır oluştur"), ("No new speech found", "Yeni konuşma bulunamadı"), ("{} speech lines added", "{} konuşma satırı eklendi"),
    ("Reference", "Referans"), ("Reference ({} s)", "Referans ({} sn)"), ("Reference: {}", "Referans: {}"), ("No reference loaded", "Referans yüklü değil"),
    ("Load…", "Yükle…"), ("Offset", "Kayma"), ("How much later the document is than the reference", "Belge referanstan ne kadar geç"),
    ("Find offset", "Kaymayı bul"), ("Offset {} s (confidence {}%)", "Kayma {} sn (güven %{})"),
    ("No clear offset found (confidence {}%) — not applied", "Belirgin bir kayma bulunamadı (güven %{}) — uygulanmadı"),
    ("Transfer the reference text into the document", "Referans metnini belgeye aktar"),
    ("Each reference line goes to the document line it overlaps most; unmatched document lines stay, unmatched reference lines are added.",
     "Her referans satırı en çok çakıştığı belge satırına yazılır; karşılıksız belge satırları kalır, ev sahipsiz referans satırları eklenir."),
    ("Min overlap", "En az çakışma"), ("Keep leading tags", "Baştaki etiketleri koru"), ("Keep the original as {EN: …}", "Özgün metni {EN: …} olarak sakla"),
    ("Use the reference's times", "Referansın zamanlarını kullan"), ("Add unmatched reference lines", "Ev sahipsiz referans satırlarını ekle"),
    ("{} lines filled · {} added · {} without reference · {} notes", "{} satır dolduruldu · {} eklendi · {} karşılıksız · {} not"),
    ("{} lines filled from the reference", "Referanstan {} satır dolduruldu"),
    ("Compare with the source", "Kaynakla karşılaştır"), ("Source", "Kaynak"), ("reference", "referans"), ("Choose…", "Seç…"),
    ("Use the reference", "Referansı kullan"), ("candidate = this document", "aday = bu belge"),
    ("Find the shift (±15 s)", "Kaymayı bul (±15 sn)"), ("Shift", "Kayma"), ("Length ratio", "Uzunluk oranı"), ("Similarity", "Benzerlik"),
    ("Start difference", "Başlangıç farkı"), ("Include signs / typesetting", "Tabela / typeset dahil"), ("Skip styles (source)", "Atlanacak stiller (kaynak)"),
    ("(candidate)", "(aday)"), ("Compare", "Karşılaştır"), ("Choose a source file or load a reference first", "Önce bir kaynak dosya seçin ya da referans yükleyin"),
    ("All ({})", "Tümü ({})"), ("Flagged ({})", "Bayraklı ({})"), ("Kind", "Tür"), ("Candidate", "Aday"), ("Flags", "Bayraklar"),
    ("Select the candidate lines", "Aday satırları seç"), ("Select flagged lines ({})", "Bayraklı satırları seç ({})"),
    ("Add missing lines as comments ({})", "Eksik satırları yorum olarak ekle ({})"), ("Apply the shift to the reference", "Kaymayı referansa uygula"),
    ("HTML report…", "HTML rapor…"), ("CSV…", "CSV…"), ("Marked copy…", "İşaretli kopya…"), ("Nothing to add", "Eklenecek bir şey yok"),
    ("{} missing lines added as comments", "{} eksik satır yorum olarak eklendi"),
    ("The marked copy cannot overwrite the document", "İşaretli kopya belgenin üstüne yazılamaz"),
    ("Piecewise sync", "Parçalı senkron"), ("Time source", "Zaman kaynağı"), ("Speech in the audio", "Sesteki konuşma"), ("Reference subtitle", "Referans altyazı"),
    ("The offset only changes at scene breaks; signs, screen text and OP/ED move with their scene but are not used for alignment.",
     "Kayma yalnız sahne aralarında değişir; tabelalar, ekran yazıları ve OP/ED sahneleriyle kayar ama hizalamada kullanılmaz."),
    ("Scene break ≥", "Sahne arası ≥"), ("Search ±", "Arama ±"), ("Selected lines only", "Yalnız seçili satırlar"), ("OP/ED styles", "OP/ED stilleri"),
    ("Plan", "Planla"), ("Load a correctly timed reference first", "Önce doğru zamanlı bir referans yükleyin"),
    ("Open the video or audio first (the speech is read from it)", "Önce videoyu ya da sesi açın (konuşma ondan okunur)"),
    ("Wait until the audio is decoded", "Ses çözülene kadar bekleyin"), ("Detecting speech…", "Konuşma algılanıyor…"),
    ("The document changed since planning — plan again", "Planlamadan sonra belge değişti — yeniden planlayın"),
    ("Nothing to change", "Değişecek bir şey yok"), ("{} lines moved", "{} satır kaydırıldı"), ("{} scenes · {} lines will move", "{} sahne · {} satır kayacak"),
    ("Scene", "Sahne"), ("Margin", "Fark payı"),
    ("How much better the chosen offset fits than the best one at least 0.3 s away, as a share of the scene's anchor speech. Under 5% is doubtful.", "Seçilen kaymanın, en az 0,3 sn uzaktaki en iyi rakibinden ne kadar iyi oturduğu (sahnenin dayanak konuşmasına oranla). %5'in altı şüphelidir."), ("Note", "Not"), ("Apply ({} lines)", "Uygula ({} satır)"),
    ("Term check", "Terim denetimi"), ("Spell check", "Yazım denetimi"), ("Dictionary downloaded.", "Sözlük indirildi."), ("Download failed: {}", "İndirilemedi: {}"), ("Language", "Dil"), ("No dictionary installed for this language (LibreOffice dictionaries, downloaded from the internet; tr_TR ≈ 38 MB).", "Bu dil için sözlük kurulu değil (LibreOffice sözlükleri, internetten indirilir; tr_TR ≈ 38 MB)."), ("Download dictionary", "Sözlüğü indir"), ("Downloading…", "İndiriliyor…"), ("{} misspelled words", "{} yanlış yazılmış sözcük"), ("in {} lines", "{} satırda"), ("Fix all", "Tümünü düzelt"), ("Ignore", "Yoksay"), ("Add to dictionary", "Sözlüğe ekle"), ("{} terms", "{} terim"), ("No glossary (terimler.tsv next to the script is loaded automatically)", "Sözlük yok (betiğin yanındaki terimler.tsv kendiliğinden yüklenir)"),
    ("Reload", "Yeniden yükle"), ("Check", "Denetle"), ("{} findings", "{} bulgu"), ("Fix forbidden terms ({})", "Yasak terimleri düzelt ({})"),
    ("Fix", "Düzelt"), ("{} lines fixed", "{} satır düzeltildi"),
    // ---------------------------------------------------------------- palette / preferences
    ("Type a command…  (:123 goes to line 123)", "Komut yazın…  (:123 ile 123. satıra gidilir)"), ("Go to line {}", "{}. satıra git"),
    ("No matching command", "Eşleşen komut yok"), ("Automation macro", "Otomasyon makrosu"),
    ("Preferences", "Ayarlar"), ("General", "Genel"), ("Video & audio", "Video ve ses"), ("Shortcuts", "Kısayollar"),
    ("Interface language", "Arayüz dili"), ("Theme", "Tema"), ("Dark", "Koyu"), ("Light", "Açık"), ("Interface size", "Arayüz boyutu"),
    ("Autosave every (s, 0 = off)", "Otomatik yedek aralığı (sn, 0 = kapalı)"), ("Default frame rate", "Varsayılan kare hızı"),
    ("Max characters per second", "Saniyede en çok karakter"), ("Max characters per line", "Satırda en çok karakter"),
    ("Snap timing edits to keyframes", "Zamanlama düzenlemeleri anahtar karelere yapışsın"), ("Video follows the active line", "Video etkin satırı izlesin"),
    ("Show the spectrum by default", "Varsayılan olarak spektrumu göster"), ("Karaoke tap latency (ms)", "Karaoke tıklama gecikmesi (ms)"),
    ("Script folders", "Betik klasörleri"), ("Add…", "Ekle…"), ("Remove", "Kaldır"),
    ("Search", "Ara"), ("Command", "Komut"), ("Shortcut", "Kısayol"), ("Second", "İkinci"), ("Reset", "Sıfırla"), ("Reset all", "Tümünü sıfırla"),
    ("press keys…", "tuşlara basın…"), ("Esc cancels, Backspace clears", "Esc iptal eder, Backspace siler"),
    ("Conflict: {} is also used by {}", "Çakışma: {} aynı zamanda şunda kullanılıyor: {}"),
    ("Resolve the conflicts before saving.", "Kaydetmeden önce çakışmaları giderin."),
    ("Save", "Kaydet"), ("Close", "Kapat"),
    ("Subtitles", "Altyazılar"), ("Video / audio", "Video / ses"), ("Audio / video", "Ses / video"), ("ms", "ms"), ("Yuzu 柚子", "Yuzu 柚子"),
    ("Single keys and arrows only work while no text box is being edited.", "Tek tuşlar ve oklar yalnız bir metin kutusu düzenlenmiyorken çalışır."),
    // ---------------------------------------------------------------- line clipboard, select lines
    ("Copy lines", "Satırları kopyala"), ("Cut lines", "Satırları kes"), ("Paste lines", "Satırları yapıştır"), ("Paste over…", "Üstüne yapıştır…"),
    ("Paste over", "Üstüne yapıştır"), ("{} lines copied", "{} satır kopyalandı"), ("{} lines cut", "{} satır kesildi"), ("{} lines pasted", "{} satır yapıştırıldı"),
    ("Pasted over {} lines", "{} satırın üstüne yapıştırıldı"),
    ("{} lines from the clipboard go over {} lines. Fields to overwrite:", "Panodaki {} satır {} satırın üstüne yazılacak. Değişecek alanlar:"),
    ("Start time", "Başlangıç zamanı"), ("End time", "Bitiş zamanı"), ("Left margin", "Sol kenar"), ("Right margin", "Sağ kenar"), ("Vertical margin", "Dikey kenar"),
    ("All", "Tümü"), ("None", "Hiçbiri"), ("Times", "Zamanlar"),
    ("Select lines…", "Satır seç…"), ("Select lines", "Satır seç"), ("Match", "Eşleşme"), ("Contains", "İçerir"), ("Exactly", "Tam olarak"),
    ("Does not match", "Eşleşmeyenler"), ("Dialogue", "Diyalog"), ("Comments", "Yorumlar"), ("Set selection", "Seçimi bununla değiştir"),
    ("Add to selection", "Seçime ekle"), ("Remove from selection", "Seçimden çıkar"), ("Intersect", "Seçimle kesiştir"), ("Select", "Seç"),
    ("{} lines match · {} selected", "{} satır eşleşti · {} satır seçili"),
    // ---------------------------------------------------------------- styles manager
    ("Storage", "Depo"), ("Current script", "Bu betik"), ("Delete this storage", "Bu depoyu sil"), ("Really delete?", "Gerçekten silinsin mi?"),
    ("new storage name", "yeni depo adı"), ("Create", "Oluştur"), ("Copy to script ⏵", "Betiğe kopyala ⏵"), ("⏴ Copy to storage", "⏴ Depoya kopyala"),
    ("Move up", "Yukarı taşı"), ("Move down", "Aşağı taşı"), ("Sort", "Sırala"), ("Sort by name", "Ada göre sırala"),
    ("Import from script…", "Betikten al…"), ("Apply to selected lines", "Seçili satırlara uygula"), ("No styles in that file", "O dosyada stil yok"),
    ("Styles in {}:", "{} içindeki stiller:"), ("Import ({})", "Al ({})"), ("Script style: {}", "Betik stili: {}"), ("Storage style: {}", "Depo stili: {}"),
    ("Could not save the storage: {}", "Depo kaydedilemedi: {}"), ("{} styles added, {} replaced", "{} stil eklendi, {} stil değiştirildi"),
    ("{} styles copied to the storage", "{} stil depoya kopyalandı"), ("Style {} set on {} lines", "{} stili {} satıra verildi"),
    // ---------------------------------------------------------------- multiple replace
    ("Multiple replace…", "Çoklu değiştir…"), ("Multiple replace", "Çoklu değiştir"), ("Groups", "Gruplar"), ("Group on/off", "Grubu aç/kapat"),
    ("New group", "Yeni grup"), ("Group {}", "Grup {}"), ("Move left", "Sola taşı"), ("Move right", "Sağa taşı"), ("Delete group", "Grubu sil"),
    ("Replace with", "Yerine"), ("Delete rule", "Kuralı sil"), ("+ Rule", "+ Kural"), ("Normal", "Normal"), ("Case-insensitive", "Harf duyarsız"),
    ("Leave override tags alone", "Etiketlere dokunma"), ("Before", "Önce"), ("After", "Sonra"), ("{} lines would change", "{} satır değişecek"),
    ("{} lines changed", "{} satır değişti"), ("Could not save the rules: {}", "Kurallar kaydedilemedi: {}"),
    // ---------------------------------------------------------------- assistants
    ("Translation assistant…", "Çeviri asistanı…"), ("Translation assistant", "Çeviri asistanı"), ("Line {} of {}", "Satır {} / {}"),
    ("block {}/{}", "parça {}/{}"), ("Original", "Özgün"), ("Translation (Enter: accept and go on · Shift+Enter: line break)", "Çeviri (Enter: kabul et ve ilerle · Shift+Enter: satır sonu)"),
    ("Accept and next", "Kabul et ve ilerle"), ("◀ Previous", "◀ Önceki"), ("Skip ▶", "Atla ▶"), ("Insert original", "Özgünü yerleştir"),
    ("Seek video to the line", "Videoyu satıra götür"), ("End of the file", "Dosyanın sonu"), ("Start of the file", "Dosyanın başı"),
    ("Styling assistant…", "Stil asistanı…"), ("Styling assistant", "Stil asistanı"), ("Style: {}", "Stil: {}"), ("Style name", "Stil adı"),
    ("type a few letters, Enter applies", "birkaç harf yazın, Enter uygular"),
    ("Could not read the amount", "Miktar okunamadı"), ("{} lines shifted", "{} satır kaydırıldı"),
    ("Karaoke", "Karaoke"), ("Show the active line's syllables: drag a boundary to retime, click a syllable to hear it", "Etkin satırın hecelerini göster: sınırı sürükleyerek zamanlayın, heceye tıklayarak dinleyin"),
    // ---------------------------------------------------------------- automation
    ("Automation manager…", "Otomasyon yöneticisi…"), ("Automation manager", "Otomasyon yöneticisi"), ("Reload all", "Tümünü yeniden yükle"),
    ("Reload when changed", "Değişince yeniden yükle"), ("Reload scripts when they change on disk", "Betikler diskte değişince yeniden yükle"),
    ("Script", "Betik"), ("Version", "Sürüm"), ("Author", "Yazar"), ("Macros", "Makrolar"), ("validates", "doğrular"), ("checkable", "işaretlenebilir"),
    ("{} scripts · {} macros · {} failed", "{} betik · {} makro · {} hatalı"), ("Pick a script to see its macros or its error.", "Makrolarını ya da hatasını görmek için bir betik seçin."),
    ("Not available for the current selection", "Bu seçim için kullanılamaz"), ("{} is not available for the current selection", "{} bu seçim için kullanılamaz"),
    ("Scripts changed ({}), reloading", "Betikler değişti ({}), yeniden yükleniyor"),
    // ---------------------------------------------------------------- visual tools
    ("–", "–"), ("Rot Z", "Döndür Z"), ("Rot XY", "Döndür XY"), ("Scale", "Ölçek"), ("VClip", "VClip"), ("Remove clip", "Clip'i kaldır"),
    ("Click or drag to set \\pos; with \\move drag either end", "Tıklayın ya da sürükleyin: \\pos; \\move varsa iki ucu da sürüklenir"),
    ("Set \\org (the rotation origin)", "\\org (dönme merkezi)"),
    ("Drag around the origin to rotate (\\frz); Shift snaps to 15°", "Merkez çevresinde sürükleyerek döndürün (\\frz); Shift 15°'lik adımlar"),
    ("Drag to tilt (\\frx, \\fry)", "Sürükleyerek eğin (\\frx, \\fry)"),
    ("Drag right/up to scale (\\fscx, \\fscy); Shift keeps the ratio", "Sağa/yukarı sürükleyerek ölçekleyin (\\fscx, \\fscy); Shift oranı korur"),
    ("Drag a rectangle (\\clip); drag its corners or inside to adjust", "Dikdörtgen çizin (\\clip); köşelerinden ya da içinden sürükleyerek ayarlayın"),
    ("Vector clip: click adds a point, drag moves one, right-click deletes", "Vektör clip: tıklama nokta ekler, sürükleme taşır, sağ tık siler"),
    ("Switch the line between \\pos and \\move", "Satırı \\pos ile \\move arasında çevir"), ("pos → move", "pos → move"), ("move → pos", "move → pos"),
    ("A whole number is milliseconds (frames in frame mode); 1.5 or 0:00:01.50 is a time. Negative shifts earlier.", "Tam sayı milisaniyedir (kare kipinde kare); 1.5 ya da 0:00:01.50 bir süredir. Eksi değer öne alır."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_messages() {
        set_lang(Lang::Tr);
        assert_eq!(t("File"), "Dosya");
        assert_eq!(tf!("Saved {}", "a.ass"), "Kaydedildi: a.ass");
        assert_eq!(tf!("{} lines · {}×{} · {} fps · {} · QC {}", 3, 1920, 1080, "23.976", "UTF-8", 0), "3 satır · 1920×1080 · 23.976 fps · UTF-8 · QC 0");
        assert_eq!(message("Duration 20000 ms above maximum"), "Süre 20000 ms, en çoğun üstünde");
        assert_eq!(message("Style 'Sign' does not exist"), "'Sign' stili yok");
        assert_eq!(message("21.5 characters/second exceeds 20"), "Saniyede 21.5 karakter, sınır 20");
        assert_eq!(message("something new"), "something new");
        set_lang(Lang::En);
        assert_eq!(t("File"), "File");
        assert_eq!(message("Double space"), "Double space");
        set_lang(Lang::Tr);
    }

    /// Every string the GUI passes to t() / tf!() must have a Turkish entry.
    #[test]
    fn every_ui_string_is_translated() {
        let sources = [include_str!("app.rs"), include_str!("commands.rs"), include_str!("palette.rs"), include_str!("prefs.rs"), include_str!("tools.rs"), include_str!("editing.rs"), include_str!("visual.rs"), include_str!("automation.rs")];
        let re = regex::Regex::new(r#"\b(?:t\(|tf!\()\s*"((?:[^"\\]|\\.)*)""#).unwrap();
        let mut missing = Vec::new();
        for src in sources {
            for c in re.captures_iter(src) {
                let s = c[1].replace("\\\\", "\\").replace("\\\"", "\"").replace("\\n", "\n");
                if !has(&s) { missing.push(s); }
            }
        }
        // labels translated through t() at run time
        for (_, l) in asscore::lines::field::ALL { if !has(l) { missing.push(l.to_string()); } }
        for k in asscore::replace::Kind::ALL { if !has(k.label()) { missing.push(k.label().to_string()); } }
        for c in crate::commands::COMMANDS {
            if !has(c.label) { missing.push(c.label.to_string()); }
            if !has(c.group) { missing.push(c.group.to_string()); }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "untranslated UI strings: {:#?}", missing);
    }
}
