# Yozora yol haritası

Hedef: Aegisub'un tüm iş akışını (ASS, stil yönetimi, ses dalga formu, video, karaoke, Automation 4),
Subtitle Edit'in iyi yanlarını (çok biçim, toplu araçlar, QC, çeviri/OCR yardımcıları) ve anime fansub'ına özgü
araçları tek, hızlı ve sade bir arayüzde birleştirmek.

## Tamamlandı
- [x] ASS/SSA model + ayrıştırıcı + yazıcı
- [x] SRT / VTT / MicroDVD / TXT
- [x] Zamanlama araçları, TPP, QC, undo/redo
- [x] Automation 4: `subs`, `aegisub.*`, `utils`, `karaskel`, `unicode`, `re`, başsız diyaloglar
- [x] CLI

## Sıradaki (öncelik sırasıyla)
1. **Automation 4 tamlığı**: `lpeg`, `lfs`, `json` modülleri; `karaskel` furigana; `aegisub.dialog` gerçek GUI geri çağrısı;
   Aegisub'un `kara-templater.lua` / `cleantags` / `clipboard` makrolarıyla uyum testleri; `?script` yolları;
   Aegisub'un Lua kütüphanesinden geriye kalan `aegisub.*` işlevleri (`audio_*`, `get_cursor`...).
2. **Masaüstü arayüzü** (Tauri veya egui): satır tablosu, düzenleme kutusu, stil yöneticisi, Aegisub benzeri kısayollar,
   sekmeli birden çok dosya, koyu/açık tema, Türkçe/İngilizce arayüz.
3. **Video**: libmpv ile oynatma, ASS (libass) önizleme, kare-doğru arama, anahtar kare üretimi, timecode dosyaları (VFR).
4. **Ses**: dalga formu + spektrum, ses seçimi ile zamanlama, otomatik bölme (VAD), Whisper tabanlı taslak yazı.
5. **Karaoke**: görsel \k zamanlayıcı, hece bölme, şablon (kara-templater) uyumlu çalıştırma.
6. **Subtitle Edit'ten**: toplu dönüştürme, çok sayıda ek biçim (STL, SBV, LRC, TTML, PGS/OCR), yinelenen/kısa satır düzeltmeleri,
   sözlük/yazım denetimi (Hunspell, Türkçe), çeviri API'leri, birleştir/böl, FPS dönüştürme, karşılaştırma (diff).
7. **Anime'ye özgü**: stil/şablon kütüphanesi, tipografi (sign), \pos/\move/\clip görsel aracı, `mocha` hareket verisi içe aktarma,
   yazı tipi eksikleri denetimi + gömme, chapter (XML/OGM) üretimi, muxing (mkvmerge) sihirbazı.
8. **Performans**: 100k satırlık dosyalarda anlık kaydırma, sanal liste, artımlı ayrıştırma, çok iş parçacıklı QC.
