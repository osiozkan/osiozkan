# Yol haritası

Her gece bir sonraki sürüm bu listeden ilerler. `[x]` tamamlandı.

## Çekirdek
- [x] ASS/SSA/SRT/VTT, etiket ayrıştırıcı, karaoke bölme
- [x] Alt biçimler: MicroDVD, SBV, LRC, TTML, SAMI, CSV, JSON (ajan modülü)
- [x] Subtitle Edit "yaygın hataları düzelt" kümesi, Türkçe büyük/küçük harf
- [ ] Proje dosyası (zaman kodları, anahtar kareler, video yolu, betik ayarları)

## Aegisub eşdeğerliği
- [x] Satır tablosu, düzenleyici, stil yöneticisi, geri al/yinele
- [x] Dalga formu zamanlama, anahtar kare yakalama, video + libass önizleme
- [x] libmpv oynatıcı: kare doğru atlama, sesle eşzamanlı oynatma, canlı altyazı (ffmpeg yedeği)
- [x] Spektrum görünümü, Aegisub zamanlama tuşları ve faresi
- [x] Karaoke modu: hece sınırlarının ses görünümünde sürüklenmesi, heceyi çalma
- [x] Görsel dizgi araçları (konum/`\move`, merkez, döndürme Z/XY, ölçek, dikdörtgen ve vektör clip)
- [x] Font toplayıcı + gömme (ajan modülü)
- [x] Kısayol özelleştirme, komut paleti, Ayarlar penceresi, Türkçe arayüz
- [x] Çeviri asistanı, stil asistanı, satır seç, satır panosu ve üstüne yapıştır

## Osi'nin araçları
- [x] Referans altyazı: sütun, kayma bulma, metin aktarımı
- [x] Kaynakla karşılaştırma (aligncompare): bayraklar, HTML/CSV rapor, işaretli kopya
- [x] Parçalı senkron: referansa (bsnap) ve sesteki konuşmaya (bsync, modelsiz VAD)
- [x] Terim denetimi (termcheck), Osi CPL kuralı
- [ ] Konuşma algılamayla satır başlangıç/bitişlerini yapıştırma önerileri
- [x] Stil kataloğu / depolama, kaydedilmiş arama-değiştirme (çoklu değiştir)
- [x] Yazım denetimi (Hunspell/spellbook; tr_TR indirilir, bayrak yaması; `yuzu-spell`)

## Lua
- [x] macro/filter, karaskel, utils, unicode, re, dialog
- [x] Karaoke şablon motoru (kara-templater) — ajan modülü
- [x] Aegisub 3.x API eksikleri (set_undo_point, file_name, aegisub.* modül adları), extradata (`line.extra`), `subs[0]`/`subs[-i]`
- [x] DependencyControl uyumluluk katmanı (modül listesi, yapılandırma, alt menü), `json`; uyumluluk testleri
- [x] Hata izi, otomatik yeniden yükleme, makro doğrulama/is_active, otomasyon yöneticisi
- [ ] Gerçek ILL / ASSFoundation / a-mo kurulumuyla deneme (bu ortamdan indirilemedi)
- [ ] MoonScript (`.moon`) betikleri, `aegisub.get_frame`, dışa aktarma süzgeci ayar pencereleri
- [ ] Betik sandbox ayarları

## Kalite
- [ ] Çok-gece performans profili (100k satır)
- [ ] Paketleme: Windows MSI/zip, AppImage, macOS dmg
