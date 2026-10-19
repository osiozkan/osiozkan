# Changelog

## 0.1.0 — ilk gece
- Çekirdek: ASS/SRT/VTT modeli, etiket ayrıştırıcı, zamanlama, QC, Türkçe araçlar (büyük/küçük harf, mojibake, kodlama)
- Metin araçları: yaygın hataları düzelt, işitme engelli metni sil, satır dengeleme, etiket temizleme/birleştirme, karaoke araçları
- Çözünürlük yeniden örnekleme (\pos/\move/\clip/\fs/\bord/\t dahil), stil içe aktarma, kullanılmayan stil silme
- Automation 4 uyumluluk katmanı (LuaJIT: ffi/bit): `subs` nesnesi, makrolar, filtreler, diyaloglar, `parse_karaoke_data`, gerçek Aegisub `karaskel`/`utils`/`cleantags` (BSD), `re`, `unicode`, `lfs`; gerçek `kara-templater.lua` testlerden geçiyor
- Yazı tipi ölçümü (fontdb + ttf-parser) ile `aegisub.text_extents`
- CLI: info, convert, check --fix, shift, retime, fps, chain, text, fix-encoding, lua list/run
- GUI (egui): grid (200k satır akıcı), etiket renklendirmeli editör, geri al/yinele, bul-değiştir (Türkçe İ/ı duyarlı), satır seç, stil yöneticisi (libass önizlemeli), QC penceresi (arka plan iş parçacığı), ses dalga biçimi (sürükle-zamanla, anahtar kareler, konuşma algılama), libass ile ASS'li video önizleme ve gerçek zamanlı oynatma, \pos sürükleme, referans altyazı sütunu (çeviri), karaoke düzenleyici, yazı tipi toplayıcı, Lua diyalogları/iptal/ilerleme, dışa aktarma filtreleri, otomatik kayıt, Aegisub otomasyon klasörlerini otomatik bulma
- Yazım denetimi (Türkçe sözlük indirme + bayrak yaması), Lua konsolu, MoonScript desteği, DependencyControl vekili, görsel dizgi araçları (pos/org/clip/döndür/ölçek), stil deposu, kurtarma
