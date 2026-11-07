# OsioSub — Yol Haritası / Roadmap

Hedef: Aegisub'ın tam işlevselliği + Subtitle Edit'in en iyi özellikleri + mevcut Aegisub Lua
(Automation 4) scriptleriyle birebir uyum, kolay arayüz, yüksek performans. Gece oturumları
bu dosyayı güncelleyerek devam eder (her gece 22:00 → 06:00).

## Mimari
- `core/`  – ASS/SRT/VTT modeli, zaman, renk, etiketler, QC, zamanlama, arama, geri alma, Türkçe metin.
- `lua/`   – Aegisub Automation 4 uyumlu Lua 5.1 çalışma zamanı (`aegisub.*`, `subs`, `include`, util/karaskel/re/unicode/utf8).
- `cli/`   – `osiosub` komut satırı (convert/check/shift/fix/macros/run).
- `media/` – ses çözme (symphonia + ffmpeg yedek), dalga formu, ffprobe/ffmpeg ile video önizleme (libass).
- `app/`   – egui masaüstü arayüzü (ızgara, editör, dalga formu, önizleme, stil yöneticisi, QC, yazım denetimi, Lua makroları).

## Durum
- [x] ASS okuma/yazma (SSA v4 dahil, bilinmeyen bölümler korunur)
- [x] SRT/VTT/TXT içe/dışa aktarma
- [x] Zaman araçları: kaydır, doğrusal, fps dönüşümü, boşluk kapat/zorla, böl/birleştir
- [x] QC: CPS, satır uzunluğu/sayısı, süre, çakışma, tipografi, tanımsız stil
- [x] Türkçe İ/ı büyük-küçük harf, tipografi düzeltmeleri
- [x] Bul/Değiştir (regex, etiket atlama)
- [x] Geri al/yinele
- [x] Lua: `subs` (okuma/yazma/insert/append/delete/deleterange), register_macro/filter, dialog, log, progress, cancel
- [x] Lua kütüphaneleri: util, karaskel (temel), utf8, unicode, re
- [x] CLI

- [x] GUI: ızgara, editör, geri al/yinele, bul/değiştir, stil yöneticisi, QC, klavye gezinmesi, ayar kalıcılığı
- [x] Dalga formu (sürükle: başlangıç/bitiş, Alt+tık, sağ tık), video önizleme (libass), Ctrl+tık → \pos
- [x] Türkçe yazım denetimi (gömülü Hunspell tr, MIT lisanslı sözlük: `dicts/`)
- [x] Toplu düzeltmeler (otomatik satır böl, HI sil, çakışma, CPS uzat, min süre), karaoke \k üretici
- [x] Gerçek Aegisub `karaskel`/`utils`/`cleantags` dosyaları gömülü (`lua/lib/include`), `aegisub.parse_karaoke_data`, `subs.script_resolution`, ipairs(subs)
- [x] CI iş akışı (Linux/Windows/macOS)

## Sıradaki işler (öncelik sırasıyla)
1. **Kullanıcının kendi Lua scriptleri ile uyumluluk testi**: scriptler `osiosub/automation/autoload/` altına konunca
   gece oturumu hepsini `osiosub run` ile deneyip hataları düzeltecek (şu an elimizde yok). Gerçek `kara-templater.lua`
   ile uçtan uca test (güvenilir kaynaktan, elle eklenince).
2. **Ses oynatma** (cpal) + oynatma imleci, spektrum görünümü, kare adımlı video gezinme, keyframe snap, sahne değişimi.
3. Video önizlemede sürekli oynatma (ffmpeg boru hattı) ve görsel typesetting araçları (\move, \clip, \org sürükle).
4. Kaydetme sırasında font gömme (`[Fonts]`), eksik font uyarısı (`fonts::has_font`).
5. Lua: `aegisub.dialog.open/save`, `ILL`/`Yutils` uyumluluk testleri, `lfs`, `clipper`, tam `karaskel` (furigana), `cleantags`, `auto4 filter config`.
6. Subtitle Edit özellikleri: Netflix/BBC kalite kontrol profilleri, OCR (Tesseract) ile görüntü tabanlı altyazı, otomatik çeviri (API), Whisper ile oto-transkripsiyon, spell-check (Hunspell tr_TR), bul-değiştir çoklu dosya, toplu dönüştürme.
7. Anime özel: Karaoke zamanlayıcı, typesetting araçları (\pos/\move/\clip görsel), stil kopyala, `Dialogue/Comment` toplu işlemleri, çeviri-düzenleme-QC iş akışı, sahne değişimi snap.
8. Paketleme: Windows/macOS/Linux sürümleri (GitHub Actions).

## Bilinen sınırlar
- `text_extents` şimdilik yaklaşık ölçüm yapıyor (font motoru yok).
- Eski oturumlardan "karşılaşılan altyazı sorunları" listesi bu depoda yok; sorunlar `docs/ISSUES.md` altında toplanacak
  (Türkçe İ/ı, çift boşluk, `...` → `…`, çakışmalar, CPS vb. QC'ye eklendi).
