# Yol haritası

Öncelik sırası yukarıdan aşağıya. Tamamlananlar `[x]`.

## Çekirdek
- [x] ASS/SSA oku-yaz, bilinmeyen bölümleri koru, CRLF algıla, UTF-8/16/Windows-1254
- [x] SRT/VTT oku-yaz, temel HTML etiketi ↔ ASS dönüşümü
- [x] QC: çakışma, süre, okuma hızı, satır uzunluğu, bilinmeyen stil, boşluk
- [x] Toplu araçlar: kaydır, doğrusal senkron, fps dönüşümü, kareye oturt, sırala, yinelenen/boş sil, çakışma düzelt, Türkçe harf
- [x] Override etiketi ayrıştırıcı/yazıcı (gidiş-dönüş güvenli), başlangıç bloğu get/set, temizleyici
- [x] Sık hataları düzelt (core::fixes): bozuk Türkçe karakter onarımı, boşluk/noktalama, diyalog çizgisi, cümle başı büyük harf (i/İ), işitme engelli notları, süre/boşluk
- [ ] Override etiketlerinin tipli anlamlandırılması (\t içi, \clip vektör, animasyon süreleri)
- [ ] ASS Extradata/Fonts/Graphics yapılandırılmış erişim
- [x] Biçimler: LRC (gelişmiş sözcük zamanlı → \kf), MicroDVD, SBV, TTML/DFXP okuma+yazma; TXT/CSV yazma; CLI toplu dönüştürme
- [ ] Biçimler: SAMI, SubViewer, STL, iTT, Netflix TTML tam, PGS/VobSub OCR
- [x] Zamanlama: videodan anahtar kareler, kareye/anahtar kareye yapışma, son-işleyici (lead-in/out, bitişik satır, anahtar kare)
- [ ] Anahtar kare/timecode dosyaları (Aegisub .txt, v1/v2), değişken fps
- [x] Referans altyazı (zamana göre eşleşen satırlar editörde gösterilir)
- [x] Yazı tipi denetimi (kullanılan/eksik yazı tipleri, dosyaları klasöre topla)
- [x] Yazım denetimi: Hunspell uyumlu (spellbook), tr_TR indir/seç, öneriler, yoksay, kullanıcı sözlüğü; `FLAG num` bayrak-0 uyumluluğu (LibreOffice tr_TR ile doğrulandı, ~1 sn yükleme)
- [ ] Çeviri akışı: iki dilli paralel ızgara, TM/sözlük, otomatik çeviri bağlayıcıları
- [x] Yazım: editörde canlı hatalı sözcük vurgusu (kırmızı altçizgi), etiket/\N renklendirme
- [ ] Yazım: çok dilli sözlük seçimi, ızgarada hata göstergesi
- [x] Karaoke hece modeli (core::karaoke) + GUI sekmesi: böl (|), birleştir, sürükle, ölçekle, harfe göre dağıt
- [ ] Karaoke: ses/dalga üzerinde canlı k-timing (tuşa basarak), otomatik hece ayırma (Japonca romaji/kana), şablonlayıcı (kara-templater uyumlu)

## Lua (Automation 4)
- [x] subs nesnesi, register_macro/filter, dialog.display, re, unicode, lfs, karaskel, utils
- [x] Gerçek yazı tipi ölçüleri (`text_extents`: ttf-parser, GDI/VSFilter uyumlu hücre yüksekliği)
- [ ] Yazı tipi dizini önbelleği (tarama her açılışta tüm dosyaları okuyor)
- [ ] `kara-templater.lua` ve Aegisub'ın gelen otomasyon betiklerinin birebir testi
- [ ] Dışa aktarım filtreleri (register_filter) için boru hattı
- [x] `aegisub.frame_from_ms/ms_from_frame/video_size/get_audio_selection/project_properties` GUI'den (video+etkin satır)
- [x] Betikleri yeniden yükle (motoru sıfırlar)
- [ ] Betik hata ayıklama: satır numaralı iz, günlük penceresi
- [ ] Kullanıcının Lua betikleriyle regresyon test paketi (kullanıcıdan betikler istenmeli)

## Arayüz (GUI)
- [x] Çerçeve: egui/eframe 0.37 (osisub-gui). Başsız ekran görüntüsü: `xvfb-run osisub-gui dosya --screenshot out.png`
- [x] İskelet: menü, ızgara (sanal kaydırma), düzenleme paneli, stil/kalite/otomasyon/bilgi sekmeleri, Lua diyalog köprüsü, geri al/yinele
- [ ] Satır ızgarası (sanal kaydırma, 100k satır), çoklu seçim, sütun özelleştirme
- [ ] Düzenleme kutusu: etiket renklendirme, karakter/CPS sayacı, stil seçici
- [x] Video önizleme: ffmpeg + libass ile gerçek ASS render'ı (kare başına süreç; yavaş ama doğru)
- [x] Ses dalga formu: kaydır/yakınlaştır, satır bölgeleri, başlangıç/bitiş sürükle (kareye yapışmalı), sağ tık = bitiş, Shift+tık = başlangıç
- [ ] Akıcı video oynatma (libmpv ya da kalıcı ffmpeg boru hattı), kare önbelleği, ön-yükleme
- [x] Spektrogram görünümü (rustfft), anahtar kare işaretleri, video+ses satır oynatma (F5; ses harici ffplay)
- [ ] Ses oynatma uygulama içinden (cpal/rodio), basılı tutarak oynat, ses/video senkronu
- [ ] ffmpeg olmadan çalışma (symphonia ile ses çözme)
- [ ] Ses zamanlama: sürükle, snap, kısayol odaklı akış (Aegisub kısayollarıyla uyumlu)
- [ ] Stil yöneticisi, yazı tipi önizleme, renk seçici
- [x] Konum aracı: videoda \pos sürükle (varsayılan konum stil/hizalamadan türetilir)
- [x] Video araçları: \pos, \clip (dikdörtgen, köşe tutamaçları), \frz (Shift=15°), \fscx/\fscy
- [ ] Video araçları: \move (iki nokta), \org sürükleme, vektör \clip/çizim, \fax/\fay, \t animasyon düzenleme
- [x] Makro menüsü, Lua diyalog penceresi (otomasyon sekmesi)
- [x] Bul/Değiştir (regex, Türkçe i/İ, alan/kapsam seçimi), çözünürlük yeniden örnekleme
- [ ] Otomasyon yöneticisi (yükle/kaldır/yeniden yükle, kalıcı liste), kısayol atama
- [x] Geri al/yinele, otomatik kayıt, kalıcı ayarlar, son açılanlar, sürükle-bırak, sağ tık menüsü, kopyala/kes/yapıştır
- [ ] Çökme sonrası otomatik kayıttan kurtarma sorusu
- [ ] Türkçe + İngilizce arayüz, açık/koyu tema

## Performans
- [ ] Büyük dosyalarda ayrıştırma ölçümleri (benchmark), bellek ayak izi
- [ ] Lua çağrıları için toplu (batch) API ile satır kopyası azaltma

## Dağıtım
- [x] GitHub Actions: Linux/Windows/macOS derleme + test (.github/workflows/ci.yml; henüz çalıştırılmadı)
- [ ] Sürüm etiketleri, değişiklik günlüğü
