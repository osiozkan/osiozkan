# Gece kayıtları

## 2026-10-20 (gece 1)
- Depo boştu (yalnızca profil README'si); "şimdiye kadarki geliştirme" bulunamadı, proje sıfırdan başlatıldı.
- `nsub-core`: ASS modeli/ayrıştırıcı/yazıcı, tag ayrıştırıcı, SRT/VTT/MicroDVD/LRC, kodlama tespiti, QC + düzeltmeler, zaman araçları, Türkçe harf işlemleri.
- `nsub-lua`: LuaJIT üzerinde Aegisub Automation 4 uyumluluğu (subs nesnesi, register_macro/filter, dialog, re, utils, karaskel, unicode, DependencyControl shim).
- `nsub-cli`: `nightsub` komutu.
- (devam) Aegisub'ın kendi include dosyaları gömüldü; `subs` semantiği Aegisub kaynak koduna göre birebir yapıldı; gerçek `kara-templater.lua` çalışıyor.
- LPeg + MoonScript, `.moon` betikleri, DependencyControl çevrimdışı katmanı, JSON config.
- GUI: satır tablosu, düzenleyici, stil yöneticisi, QC paneli, bul/değiştir, makro menüsü + gerçek Lua diyalogları (worker thread), video önizleme (libass), dalga formu, anahtar kareler, satır çalma, kaynak altyazı (çeviri modu), TR/EN, tema, ayarlar.
- Araçlar: yaygın hata düzeltme, seç/sırala, post-processor, yeniden örnekleme, yazı tipi toplayıcı. CLI karşılıkları.
- Performans: 100k satır ayrıştırma 0.16 s; QC 0.36 s; geri alma bellek bütçeli; büyük dosyada QC arka plan iş parçacığında.

### Bilinen sorunlar / sonraki gece için fikirler
- Görsel typesetting araçları (`\pos` sürükleme), karaoke zamanlama düzenleyicisi, ses spektrumu.
- Gerçek zamanlı video oynatma (ffmpeg boru hattı veya libmpv), ses çalma şu an ffplay ile.
- Extradata, yazım denetimi (Hunspell tr_TR), kısayol özelleştirme, çoklu belge sekmeleri.
- Gerçek DependencyControl 0.7 yüklemesi (ModuleProvider/Host kullanıyor) — şimdilik çevrimdışı shim.
- Windows'ta `release.yml` henüz denenmedi (LuaJIT MSVC derlemesi, rfd).

## 2026-10-21 — sürüm 0.2.0
- Karaoke araçları (eşit aralıkla `\k` bölme, etiket silme; çekirdek `kara::auto_split/strip_kara`, CLI `nightsub karaoke`, GUI Araçlar menüsü).
- Zamanlama kısayolları: `[` başlangıcı, `]` bitişi video zamanına ayarla; `,` `.` kare adımı.
- İstatistik penceresi / `nightsub stats`; toplu dönüştürme `nightsub batch`.
