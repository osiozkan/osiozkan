# osisub yol haritası ve gece günlüğü

Hedef: Aegisub'ın tam karşılığı + Subtitle Edit'in güçlü özellikleri + tüm kullanıcı Lua scriptlerinin çalışması + çok iyi performans + kolay arayüz.
Zamanlama: her gece, sabah 06:00'ya kadar çalışılır; her çalışma bu dosyadaki ilk tamamlanmamış maddeden devam eder, bitirince günlüğe yazar ve push eder.

## Mimari kararlar
- Rust çekirdek (hız, bellek güvenliği), tek ikili; Lua = LuaJIT (Aegisub ile birebir 5.1 semantiği).
- GUI: egui/eframe (tek ikili, çapraz platform) — gece 3'te başlar.
- Satır modeli Aegisub'ı yansıtır: düz `entries` listesi, bilinmeyen bölümler korunur.

## Yapılacaklar (öncelik sırasıyla)
### Gece 2 — Lua uyumu derinleştirme
- [ ] Gerçek font metrikleri (`ttf-parser`/`fontdb`) ile `aegisub.text_extents`; sistem fontları + `Fonts` bölümü
- [ ] `karaskel`: furigana, `parse_syllables` tam uyum (`\-`, inline fx, `kdur`), `preproc_line` eksik alanlar
- [ ] `lpeg`/`re` (en azından yaygın kullanım) — veya vendored lpeg
- [ ] Yaygın üçüncü taraf scriptler: Kara Templater (kara-templater.lua), ILL, zeref, a-mo utilities için uyumluluk testleri
- [ ] `ipairs(subs)` / `__ipairs`, `subs.n`, hata mesajı biçimleri
- [ ] `aegisub.dialog.display` için JSON ile girdi/çıktı (başsız otomasyon)
### Gece 3 — GUI (egui)
- [ ] Altyazı ızgarası (sanal kaydırma, 100k satıra dayanıklı), düzenleme kutusu, stil yöneticisi
- [ ] Geri al/yinele (komut tabanlı), seçim, bul/değiştir (regex), toplu zamanlama
- [ ] Automation menüsü: makro listesi, gerçek diyalog çizimi (`dialog.display` kontrolleri)
### Gece 4+ — Video/ses
- [ ] Video önizleme (libmpv/ffmpeg bağlama), ASS render (libass), sesli dalga biçimi/spektrum, snap/shift-to-frame, keyframe
- [ ] Zamanlama modu (Aegisub tuş eşlemeleri), tag-kopyalama, typesetting araçları
### Subtitle Edit'ten alınacaklar
- [ ] Çoklu format (SUB, STL, TTML, SBV, ... ) giriş/çıkış
- [ ] Yazım denetimi, OCR (görsel altyazı), çeviri yardımcıları, otomatik kırma/satır dengeleme, CPS/CPL düzeltmeleri
- [ ] Toplu dönüştürme, karşılaştırma, birleştirme/bölme, FPS dönüştürme, ses çeviri (Whisper) entegrasyonu
### Kalite
- [ ] Performans testleri (büyük dosya ayrıştırma < 100 ms/100k satır), fuzz, CI

## Günlük
### Gece 1 (2026-11-02)
- Depo sıfırdı. `osisub/` workspace kuruldu.
- core: ASS/SSA parse+write, SRT/VTT, tag araçları, kontroller, işlemler (16 test).
- lua: LuaJIT otomasyon katmanı, subs nesnesi, macro/filter kaydı, geri alma, util/unicode/utf8/karaskel alt kümesi (11 test).
- cli: info/convert/check/shift/fix/macros/run.
- Not: kullanıcının kendi Lua scriptleri depoda yok; gerçek uyumluluk testi için `tests/lua/` altına eklenmeli.
