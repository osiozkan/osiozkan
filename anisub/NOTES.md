# anisub — geliştirme notları (her gece buradan devam edilir)

Hedef: Aegisub'ın tamamı + Subtitle Edit'in iyi özellikleri + anime altyazı iş akışı; Automation 4 Lua
betikleriyle uyumlu; kolay arayüz; yüksek performans. Stack: Rust workspace + gömülü LuaJIT (mlua, vendored; Aegisub ile aynı motor: bit/ffi/%s-boolean uyumu).

## Düzen
- `crates/core`  ASS/SRT modeli, zaman/renk, kodlama tespiti (UTF-8/16, cp1254), etiket ayrıştırma,
  karaoke hecelerine ayırma, toplu işlemler (`ops`), kalite kontrol (`qc`), Türkçe i/İ büyük-küçük harf.
- `crates/lua`   Automation 4 uyumluluğu: `aegisub.*`, `subs` nesnesi, `include` + gömülü
  `utils/unicode/karaskel/cleantags`. Betikler `lua/` altında, testler `tests/macros.rs`.
- `crates/cli`   `anisub convert|check|shift|fix|macros|run`.

## Durum (v0.1, ilk gece)
Tamam: ASS okuma/yazma (format sırası, Aegisub bölümleri korunur), SRT, Lua makro çalıştırma (headless),
karaskel alt kümesi, QC (CPS, satır uzunluğu, süre, çakışma), shift/resync/sort/merge/close-gaps.

## Bilinen eksikler / sıradaki işler (öncelik sırasıyla)
1. `aegisub.text_extents` fontdb+ttf-parser ile gerçek ölçüm yapıyor (VSFilter hücre yüksekliği modeli, win ascent/descent). Eksik: kerning yok, font fallback/karışık yazı, `--font-dir` CLI bayrağı, GDI ile birebir karşılaştırma testi.
2. Automation 4 tamamlama: `aegisub.dialog.display` gerçek GUI, `register_filter` config, `lpeg`/`re`/`lfs`,
   `unicode.lua` tam, `cleantags.lua` tam, `util.lua` eksikleri.
   Kullanıcının kendi Lua betikleri gelince `tests/fixtures`'a eklenip uyumluluk testi yapılmalı.
3. Etiket motoru: tam override-tag ayrıştırıcı/yazıcı (\t, \clip, \move vb. doğrulama), etiket temizleyici.
4. Biçimler: WebVTT, SSA v4, SUB/MicroDVD, TTML, EBU STL, ASS ↔ SRT stil dönüşümü; OCR (sup/idx) uzun vade.
5. GUI (egui/eframe önerilir): altyazı ızgarası, edit kutusu, stil yöneticisi, ara/değiştir, undo/redo, 
   ses dalga formu + spektrum, video önizleme (libass render + mpv/ffmpeg), zamanlama (frame, keyframe snap),
   karaoke zamanlama aracı, çeviri/ikili görünüm, Subtitle Edit tarzı toplu düzeltmeler, kısayol özelleştirme.
6. Undo/redo komut yığını (core), proje dosyası (Aegisub garbage uyumlu), otomatik kayıt.
7. Anime özel: tipografi/sing-sign stil şablonları, karaoke şablon (kara-templater) uyumu, font eksik kontrolü, 
   mkv içinden font/altyazı çıkarma/ekleme (mkvtoolnix), Türkçe karakter/yazım denetimi (Hunspell), CPS profilleri.
8. Performans: büyük dosyalarda (100k satır) benchmark; satır tablolarını Lua'ya tembel verme.

## Notlar
- Önceki oturumlardaki gereksinim/hata geçmişi bu depoda YOKTU (depo yalnızca profil README'siydi); kullanıcı
  kendi sorun listesini ve Lua betiklerini sağlayınca `docs/` ve `tests/fixtures/`'a işlenmeli.
- Gece çalışması: sabah 06:00'da biter. Her gece: testleri çalıştır → listeden ilerle → commit/push → NOTES güncelle.
