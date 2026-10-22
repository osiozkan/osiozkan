# AniSub Yol Haritası ve Gece Günlüğü

Her gece oturumu bu dosyayı okuyup devam eder; biten maddeleri işaretler, yeni sürümü etiketler.

## Karar: Teknoloji
- Rust (core + cli), GUI: egui/eframe (planlı), Lua: **LuaJIT** (Aegisub ile aynı: `bit`, `ffi`, `jit` mevcut).
- Aegisub'da `subs` bir userdata olduğu için `#subs` çalışması gerekir → `SubsProxy` userdata (LuaJIT `__len` tablolarda çalışmaz).

## Tamamlanan (v0.1.0, gece 1)
- [x] ASS ayrıştırma/yazma (sıra-koruyan düz satır modeli, Aegisub `subs[i]` ile birebir)
- [x] SRT içe/dışa aktarma, zaman kodu yardımcıları
- [x] Aegisub Lua API: register_macro/filter, subs insert/append/delete/deleterange, log, dialog (headless varsayılanlar), text_extents (yaklaşık), frame/ms, include, cancel
- [x] utils.lua, unicode.lua, karaskel.lua (kendi yazımımız, API uyumlu)
- [x] QC (CPS, süre, çakışma, stil, parantez), zamanlama (shift, chain, frame snap)
- [x] CLI: macros, run, qc, shift, convert, info

## Sıradaki (öncelik sırasıyla)
1. [ ] Gerçek yazı tipi ölçüleri (fontdb + ttf-parser) → `text_extents` Aegisub/libass ile uyumlu
2. [x] `re` modülü tamam (Rust regex; lookaround yok). [ ] `lpeg` ihtiyacı değerlendirilecek, `clipboard`, `aegisub.dialog` GUI hook'u
3. [ ] kara-templater uyumlu şablon motoru (`code line/syl`, `template`, `fx`, `furi`) — kullanıcının mevcut Lua scriptleriyle test edilecek
4. [ ] Override tag ayrıştırıcı/yazıcı (tam: \t, \clip, \move, \fad, \pos …), tag-aware editörler
5. [ ] GUI (egui): ızgara, düzenleme kutusu, stil yöneticisi, undo/redo, çoklu seçim
6. [ ] Video (ffmpeg/libmpv) + libass önizleme, ses dalga formu/spektrum, keyframe, ses ile zamanlama
7. [ ] Subtitle Edit özellikleri: toplu çeviri (API), OCR içe aktarma (SUP/PGS), yazım denetimi, fark/karşılaştırma, birleştir/böl, FPS dönüştür, format çevirici (VTT, SBV, TTML …)
8. [ ] Anime özel: karaoke zamanlayıcı, typesetting araçları, \pos/\org/\frz görsel manipülasyon, çoklu stil toplu işlemler, sahne zamanlama, font toplama/gömme
9. [ ] Performans: 100k satırlık dosyalar, arka plan işleme, bellek profili
10. [ ] Paketleme: Windows/macOS/Linux sürümleri, otomatik güncelleme

## Notlar
- Kullanıcının kendi Lua scriptleri depoya eklenirse `examples/user-scripts/` altında regresyon testine alınacak.
- Önceki oturumlardan taşınan sorun listesi depoda bulunamadı; kullanıcı belirttiğinde buraya eklenecek.
