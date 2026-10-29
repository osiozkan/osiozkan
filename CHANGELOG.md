# Değişiklik günlüğü

## 0.2.0 — 2026-10-29 (ilk gece sürümü)

Sıfırdan yazıldı (Rust workspace).

**Çekirdek (`osisub-core`):** ASS/SSA bayt-kararlı okuma-yazma, SRT/VTT/LRC (gelişmiş)/MicroDVD/SBV/TTML, Windows-1254 çözme, etiket ayrıştırıcı, karaoke modeli, QC, "sık hataları düzelt" (Türkçe duyarlı: bozuk karakter onarımı, i/İ), bul/değiştir, zamanlama son-işleyicisi, yeniden örnekleme, yazı tipi çıkarımı, istatistik.
**Lua (`osisub-lua`):** LuaJIT tabanlı Aegisub Automation 4 uyumlu motor (`subs`, `aegisub.*`, `re`, `unicode`, `utf8`, `lfs`, `karaskel`, `utils`, filtreler, iletişim kutuları, gerçek yazı tipi ölçüleri).
**Medya (`osisub-media`):** ffmpeg ile ses dalga formu/spektrum, libass'lı video önizleme ve oynatma, anahtar kareler, konuşma tespiti, uygulama içi ses (cpal).
**Arayüz (`osisub-gui`):** ızgara, düzenleme paneli, dalga formu zamanlama, video araçları (konum/kırpma/döndürme/ölçek), karaoke ve tuşla zamanlama, stil düzenleyici, çeviri sekmesi, yazım denetimi (Hunspell/tr_TR), referans altyazı, Lua makroları, geri al/yinele, otomatik kayıt + kurtarma.
**CLI (`osisub`):** convert, check, fix, fixes, shift, post, resample, find, fonts, batch, macros, run, info.

Bilinen sınırlar: `docs/ROADMAP.md`. Doğrulanmayanlar: gerçek ses aygıtı, macOS, CI'nin gerçek çalıştırması.
