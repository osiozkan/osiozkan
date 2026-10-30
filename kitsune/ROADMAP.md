# Yol haritası

Her gece bir sürüm; ilerleme `NIGHTLY.md`'de tutulur. Öncelik sırası:

## Sıradaki geceler
1. **Lua uyumluluğu (devam)**: `re` (regex) ve `lpeg` modülleri, `aegisub.text_extents` için gerçek yazı tipi metrikleri (ttf-parser), `aegisub.dialog.display` için arayüz bağı, `include` yolları, `karaskel` kara/furigana ayrıştırması, `aegisub.clipboard`, `lfs`.
2. **Arayüz (egui)**: ızgara (satır listesi), düzenleme kutusu, stil yöneticisi, makro menüsü, geri al/yinele, sekmeli çoklu belge.
3. **Video/ses**: ffmpeg ile video önizleme, ASS render (libass), dalga formu + spektrogram, kare-doğru zamanlama, keyframe snap.
4. **Aegisub'a özgü**: ASS çizim (drawing) editörü, vektör klip, `\move`/`\t` görsel araçları, kara-oke zamanlayıcı, zamanlama sonrası işlemci, çeviri asistanı, otomasyon 4 "Kopyala" makroları.
5. **Subtitle Edit'ten**: toplu dönüştürme (50+ biçim), OCR (görüntü tabanlı altyazı), otomatik çakışma/CPS düzeltme, yazım denetimi (Türkçe Hunspell), "bul ve değiştir" çoklu dosya, Netflix kalite kontrol kuralları, çevrimiçi çeviri.
6. **Performans**: 100k satırlık dosyalarda akıcılık, artımlı render, bellek profili.
7. **Türkçe/anime özel**: büyük/küçük harf `İ/ı` kuralları, TR yazım/noktalama QC, ad/unvan sözlükleri, durum bazlı stil şablonları.

## Bilinen eksikler (v0.1.0)
- ASS `[Aegisub Project Garbage]`/Fonts/Graphics bölümleri korunur ama düzenlenmez.
- SSA v4 hizalama dönüşümü yok.
- `text_extents` yaklaşıktır (karakter başına sabit genişlik).
- Lua: `re`/`lpeg`/`lfs`/`clipboard` modülleri yok; `dialog.display` başsız ortamda varsayılan değerleri döner.
