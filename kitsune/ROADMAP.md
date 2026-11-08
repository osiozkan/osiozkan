# Kitsune — anime altyazı editörü (Aegisub + Subtitle Edit hedefi)

Kural: Her gece 22:00–06:00 arası çalışılır; her oturum sonunda commit + push, bu dosyanın
"Gece günlüğü" bölümü güncellenir. Sonraki oturum buradan devam eder.

## Mimari
- `core/`  Rust: model (Aegisub `subs` dizisiyle 1:1 düz liste), ASS/SSA/SRT/VTT, zaman, etiket ayrıştırma, QC/toplu araçlar.
- `lua/`   LuaJIT (Aegisub ile aynı) + Automation 4 API: `aegisub.*`, `subs` (index/len/append/insert/delete/deleterange), `util`, `karaskel`, `unicode`, `re`, `include`.
- `cli/`   `kitsune` komutu (convert/check/shift/fix/macros/run).
- (planlı) `gui/` egui/eframe arayüz; `media/` video/ses (ffmpeg), dalga formu, spektrum.

## Durum
- [x] ASS/SSA/SRT/VTT oku-yaz, UTF-16 BOM, SSA→ASS hizalama/renk dönüşümü
- [x] Lua makro yükleme, register_macro/filter, seçim/aktif satır, iptal+geri alma, headless dialog
- [x] util.lua, karaskel.lua (temel), unicode, re (regex crate)
- [x] QC: CPS, süre, satır uzunluğu/sayısı, çakışma, boşluk, eksik stil, süslü parantez
- [x] Araçlar: kaydır, çakışma düzelt, min süre/CPS uzat, aynı satırları birleştir, böl, dengele, HI temizle, lineer senkron, fps değiştir
## Yapılacak (öncelik sırasıyla)
1. Gerçek yazı tipi ölçümü (`aegisub.text_extents`): ttf-parser ile sistem/--fontdir fontları (şu an yaklaşık)
2. `aegisub.dialog.display` için GUI köprüsü; `lfs`, `ffi` uyumluluğu, `unicode` eksik fonksiyonlar, `re` tam Boost uyumu
3. Override etiket ayrıştırıcı (tam: \t, \clip, \move, \fad, renkler) + etiket düzenleyici API
4. Undo/redo (komut yığını), seçim modeli, arama/değiştir (regex), stil yöneticisi, satır bölme/birleştirme
5. GUI: ızgara, düzenleme kutusu, canlı ASS önizleme (libass bağları veya kendi rasterizer), video oynatıcı
6. Ses: dalga formu/spektrum, zamanlama (Aegisub tarzı), shot-change/keyframe, otomatik zamanlama
7. Karaoke: kara-templater (karaoke-template.lua uyumu), \k bölme/birleştirme, çeviri/ASR içe aktarım
8. Subtitle Edit özellikleri: OCR (bitmap altyazı), toplu dönüşüm, spell-check, çeviri (DeepL/Google), Netflix kalite kontrol, 300+ format
9. Performans: 100k satırda ölçüm, rope/arena, paralel QC
10. Kullanıcının kendi Lua betikleri: bir klasör verilirse hepsini yükle, uyumsuzlukları raporla (`kitsune lint-lua`)

## Gece günlüğü
- 2026-11-08: Sıfırdan çekirdek + Lua köprüsü + CLI kuruldu, testler geçiyor.
- 2026-11-08: `lint-lua` komutu eklendi. Kullanıcının Lua betikleri repoda yok; `kitsune lint-lua <klasör>` ile test edilmeli (betikleri `lua-scripts/` altına ekleyin).
