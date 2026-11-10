# Değişiklik günlüğü

## 0.3.0 — 2026-11-10 (ikinci gece)

**Lua uyumluluğu:** gerçek Aegisub `karaskel-auto4.lua` gömülü; **`kara-templater.lua` yerleşik** (gerçek betikle uçtan uca test); **MoonScript (`.moon`) betikleri** (gömülü moonscript 0.5.0 + LuLPeg); include klasörlerinden `require` (.lua/.moon); yeni API'ler: `aegisub.debug.out`, `aegisub.parse_karaoke_data`, `subs.script_resolution`, `unicode.charwidth/chars`, `utils.lua` genel takma adları, `cleantags.lua`; Aegisub izleme düzeyi (debug iletileri gizli); betik hata penceresi (hatalı satır + çevresi).
**Zamanlama:** VFR — timecode v1/v2 dosyaları (aç/kaydet/kapat) ve videodan **otomatik VFR algılama**; kare adımı, kareye yapışma, Lua kare dönüşümleri ve "Kareye oturt" bunları kullanır; CLI `--timecodes`.
**Tipografi:** `\move` video aracı (başlangıç/bitiş, birlikte taşıma, `\pos`a çevirme), döndürme aracında `\org` sürükleme.
**Karaoke:** Japonca otomatik hece (mora) ayırma — romaji ve kana.
**Çeviri:** çeviri belleği (bulanık öneri, Enter ile öğrenme, belgeden toplu ekleme, TSV içe/dışa aktarma).
**Arayüz:** Lua makrolarına kısayol atama; Düzen → Satırları seç (Aegisub Select Lines); ızgarada Katman/Efekt/Kenar boşluğu sütunları; elle yüklenen betikler kalıcı + Kaldır.
**Performans:** kalıcı yazı tipi dizini önbelleği.
**Hata düzeltmeleri:** medya açıkken kaydetme Project Garbage satırları ekleyip etkin satırı/ızgarayı kaydırıyordu; video araçlarında tutamaç isabet testi yanlış noktayı kullanıyordu; kısayol yakalama tek başına Ctrl tuşunu kaydediyordu.

## 0.2.0 — 2026-10-29 (ilk gece sürümü)

Sıfırdan yazıldı (Rust workspace).

**Çekirdek (`osisub-core`):** ASS/SSA bayt-kararlı okuma-yazma, SRT/VTT/LRC (gelişmiş)/MicroDVD/SBV/TTML, Windows-1254 çözme, etiket ayrıştırıcı, karaoke modeli, QC, "sık hataları düzelt" (Türkçe duyarlı: bozuk karakter onarımı, i/İ), bul/değiştir, zamanlama son-işleyicisi, yeniden örnekleme, yazı tipi çıkarımı, istatistik.
**Lua (`osisub-lua`):** LuaJIT tabanlı Aegisub Automation 4 uyumlu motor (`subs`, `aegisub.*`, `re`, `unicode`, `utf8`, `lfs`, `karaskel`, `utils`, filtreler, iletişim kutuları, gerçek yazı tipi ölçüleri).
**Medya (`osisub-media`):** ffmpeg ile ses dalga formu/spektrum, libass'lı video önizleme ve oynatma, anahtar kareler, konuşma tespiti, uygulama içi ses (cpal).
**Arayüz (`osisub-gui`):** ızgara, düzenleme paneli, dalga formu zamanlama, video araçları (konum/kırpma/döndürme/ölçek), karaoke ve tuşla zamanlama, stil düzenleyici, çeviri sekmesi, yazım denetimi (Hunspell/tr_TR), referans altyazı, Lua makroları, geri al/yinele, otomatik kayıt + kurtarma.
**CLI (`osisub`):** convert, check, fix, fixes, shift, post, resample, find, fonts, batch, macros, run, info.

Bilinen sınırlar: `docs/ROADMAP.md`. Doğrulanmayanlar: gerçek ses aygıtı, macOS, CI'nin gerçek çalıştırması.
