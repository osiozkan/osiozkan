# Yol haritası

Her gece bir sonraki sürüm bu listeden ilerletilir. `[x]` bitti, `[~]` kısmen, `[ ]` yapılacak.

## Biçimler
- [x] ASS okuma/yazma (bilinmeyen bölümler, yorumlar, Format sırası korunur)
- [~] SSA v4 (stil hizası/renk dönüşümü var; test az)
- [x] SRT, WebVTT okuma; SRT yazma
- [ ] VTT yazma, SUB/MicroDVD, TTML, SBV, LRC
- [ ] UTF-8 dışı kodlama otomatik tespiti (UTF-16 BOM var)
- [ ] Aegisub Project Garbage'daki video/ses/zaman kodu/keyframe bilgisini kullanma
- [ ] Gömülü fontlar (uuencode) çözme/yazma, extradata

## Aegisub eşdeğerleri
- [~] Satır tablosu: ekle, sil, çoğalt, birleştir, sırala, geri al/yinele
- [ ] Altyazı ızgarası: sütun seçimi, renkli stil/yorum, çoklu seçim kısayolları, sürükle-bırak
- [ ] Video önizleme (ffmpeg/libmpv), ASS çizimi (libass), kare-kesin zamanlama
- [ ] Ses dalga formu / spektrum, zamanlama sürükleme, snap-to-keyframe
- [ ] Stil yöneticisi (renk seçici, depolama, içe aktarma), canlı önizleme
- [ ] Zaman kaydırma (kare/ms), zamanlama sonrası işlemci (lead-in/out, keyframe snap)
- [ ] Bul/Değiştir (regex, alan seçimi), Spell check (Hunspell), Tercüme asistanı
- [ ] Çizim/vektör (\p) görsel editörü, \pos/\move/\clip görsel araçları
- [ ] Karaoke aracı (heceleme, ses ile eşleme, otomatik süre)
- [ ] Çözünürlük dönüştürücü, renk matrisi, Fontları topla

## Subtitle Edit'ten alınacaklar
- [x] Kalite denetimi (CPS, süre, satır uzunluğu, çakışma, boş metin, tanımsız stil)
- [ ] Çeviri: yan yana iki dil, DeepL/Google/yerel LLM ile otomatik çeviri
- [ ] OCR (bitmap altyazılar), ses→metin (Whisper)
- [ ] Çakışma düzeltme, satır bölme/birleştirme önerileri, otomatik satır kırma
- [ ] Toplu dönüştürme, FPS değiştirme, senkron noktası (iki nokta ile ger/esnet)
- [ ] Sözlük/ortak ifade düzeltmeleri (Türkçe için "i/İ" kuralları dahil)

## Lua (Automation 4) uyumu
- [x] `subs` userdata, makro/filtre kaydı, iptal, günlük, karaskel, utils, unicode
- [ ] Gerçek `text_extents` (font metrikleri: fontdb + rustybuzz; şu an yaklaşık)
- [ ] `aegisub.dialog.display` için gerçek arayüz diyalogu (GUI) ve yapılandırma
- [ ] `re` (regex), `lfs`, `clipboard`, `lpeg` modülleri; `aegisub.util` tam eşdeğerlik
- [ ] Video/ses bağlamı: `aegisub.video_size`, `keyframes`, `get_audio_selection`, proje özellikleri
- [ ] Betik dizini taraması (autoload), hata ekranı satır numaralı, `script_*` meta
- [ ] Popüler betikleri (Kara Effector, Karaoke Templater `kara-templater`, Dependency Control) uyumluluk testi
- [ ] Lua 5.1/LuaJIT `ffi` güvenlik/sandbox ayarı

## Performans ve kalite
- [ ] 100k satırlık dosyalarda sanal kaydırma, artımlı geri alma (fark tabanlı)
- [ ] Fuzz testleri (ASS ayrıştırıcı), gerçek dünya örnek dosyaları ile regresyon seti
- [ ] CI: Linux/Windows/macOS sürüm paketleri
