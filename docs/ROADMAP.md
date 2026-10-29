# Yol haritası

Öncelik sırası yukarıdan aşağıya. Tamamlananlar `[x]`.

## Çekirdek
- [x] ASS/SSA oku-yaz, bilinmeyen bölümleri koru, CRLF algıla, UTF-8/16/Windows-1254
- [x] SRT/VTT oku-yaz, temel HTML etiketi ↔ ASS dönüşümü
- [x] QC: çakışma, süre, okuma hızı, satır uzunluğu, bilinmeyen stil, boşluk
- [x] Toplu araçlar: kaydır, doğrusal senkron, fps dönüşümü, kareye oturt, sırala, yinelenen/boş sil, çakışma düzelt, Türkçe harf
- [ ] Override etiketi tam ayrıştırıcı + yazıcı (\t, \clip, \move, \fad ... tipli)
- [ ] ASS Extradata/Fonts/Graphics yapılandırılmış erişim
- [ ] Biçimler: SUB (MicroDVD), SBV, TTML, STL, ITT, PGS/VobSub OCR (SE'den)
- [ ] Zamanlama: anahtar kare (keyframe) dosyaları, timecodes v1/v2, değişken fps
- [ ] Sahne kesimine yapışma (snap) + lead-in/out, ortak "zamanlama sihirbazı"
- [ ] Çeviri akışı: iki dilli paralel görünüm, TM/sözlük, otomatik çeviri bağlayıcıları, yazım denetimi (Hunspell, tr_TR)
- [ ] Karaoke: hece ayırma/birleştirme, otomatik k-timing, şablonlayıcı (kara-templater uyumlu)

## Lua (Automation 4)
- [x] subs nesnesi, register_macro/filter, dialog.display, re, unicode, lfs, karaskel, utils
- [ ] Gerçek yazı tipi ölçüleri (`text_extents` için ttf-parser/ab_glyph + sistem yazı tipi taraması)
- [ ] `kara-templater.lua` ve Aegisub'ın gelen otomasyon betiklerinin birebir testi
- [ ] Dışa aktarım filtreleri (register_filter) için boru hattı
- [ ] `aegisub.get_audio_selection`, `project_properties` GUI'den
- [ ] Betik hata ayıklama: satır numaralı iz, günlük penceresi
- [ ] Kullanıcının Lua betikleriyle regresyon test paketi (kullanıcıdan betikler istenmeli)

## Arayüz (GUI)
- [ ] Çerçeve seçimi ve iskelet (egui/eframe önerilir: tek ikili, GPU hızlı, çapraz platform)
- [ ] Satır ızgarası (sanal kaydırma, 100k satır), çoklu seçim, sütun özelleştirme
- [ ] Düzenleme kutusu: etiket renklendirme, karakter/CPS sayacı, stil seçici
- [ ] Video önizleme (libmpv/ffmpeg), ASS renderer (libass), ses dalga formu/spektrum
- [ ] Ses zamanlama: sürükle, snap, kısayol odaklı akış (Aegisub kısayollarıyla uyumlu)
- [ ] Stil yöneticisi, yazı tipi önizleme, renk seçici
- [ ] Çizim/konum görsel araçları (\pos, \move, \clip, \org, \frz)
- [ ] Makro menüsü, otomasyon yöneticisi, Lua diyalog penceresi
- [ ] Geri al/yinele (satır bazlı fark), otomatik kayıt/kurtarma
- [ ] Türkçe + İngilizce arayüz, açık/koyu tema

## Performans
- [ ] Büyük dosyalarda ayrıştırma ölçümleri (benchmark), bellek ayak izi
- [ ] Lua çağrıları için toplu (batch) API ile satır kopyası azaltma

## Dağıtım
- [ ] GitHub Actions: Linux/Windows/macOS derleme + test
- [ ] Sürüm etiketleri, değişiklik günlüğü
