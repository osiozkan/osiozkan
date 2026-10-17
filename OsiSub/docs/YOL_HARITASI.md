# Yol haritası

## Aşama 1 — Çekirdek + Automation + temel arayüz (0.1.0) ✅
Bayt korumalı model, Automation 4 çalışma zamanı, ızgara, düzenleme kutusu, stil yöneticisi,
bul/değiştir, CPL denetimi, harici araç çalıştırıcı, testler.

## Aşama 2 — Video ve ses (0.2.0) ✅
Kare dizini, libmpv video + canlı altyazı, ses bölmesi (dalga formu/spektrogram, zamanlama,
yaslama, çalma), Lua video API'leri (frame_from_ms, keyframes, video_size, get_audio_selection,
get_frame), kareye oturtma, proje dosyası video/ses alanları.
0.3.0'da tamamlananlar: kare şeridi, zamanlama son işlemcisi, Ctrl+sürükle bağlı sınırlar,
çeviri modu (referans sütunu + TR→EN aktarma + inceleme raporu), SRT/VTT.

## Aşama 3 — Kendi araçlarının uygulama içine alınması
- ✅ 0.7.0: hareket takibi bölmesi (izleme + satıra uygulama + .assmotiontrack + Mocha/AE verisi)
- ✅ 0.8.0: örtme (ortanca modelli warp-fark → \iclip), görünürlük + düşük görünürlükte satır üretmeme,
  toplu izleme (satır başına alan, sahne kesmesi koruması)
- ✅ 0.9.0: ECC (xtrack), renk anahtarı (track), elle iz düzeltme, libass ile görünürlük denetimi
- ✅ 1.1.0: hairtrack (renk modeliyle maske, tohum aracı, iz gerektirmeden \\iclip)
- Sırada: derinlik tabanlı örtme (Depth Anything ONNX, onnxruntime), maske yayılımı (optik akış),
  ekran dışına taşmanın libass ölçümüne katılması
- ✅ 1.2.0: bsync (konuşmaya göre parçalı senkron)
- ✅ 1.3.0: aligncompare (kılavuzdaki tüm bayraklar, otomatik kayma, HTML/CSV/işaretli kopya; uygulama içi bölme
  + `python -m osisub aligncompare`), referansa göre parçalı senkron (bsnap'in rolü), referans kayması
- hairtrack: maske/\iclip üretimi, assmotion ön ucu
- bsync: dalga formuna göre senkron
- ✅ 1.3.0: TTML/DFXP okuma-yazma (Dosya > Dışa aktar). Osi'nin "DFXP Basic" dönüştürücüsü yüklenirse onun
  kuralları (zaman biçimi, stil/bölge adları, dil) karşılaştırılıp birebir uyarlanacak
- TR-EN birleştirme: ✅ 0.3.0'da yerleşik (Çeviri menüsü). Osi'nin v1.0 Lua betiği yüklenirse
  davranış farkları karşılaştırılıp eksik kural varsa eklenecek
- Osi menü betikleri (osi_*.lua): her biri OsiSub'da çalıştırılıp çıktısı Aegisub'ınkiyle bayt bayt karşılaştırılacak
- **Gereken:** assmotion-1.7.0, hairtrack.py, bsync.py, bsnap.py, aligncompare.py, asslint, termcheck,
  DFXP dönüştürücü, osi_*.lua dosyaları (kaynaklar bu oturumlarda yok; 1.3.0'daki aligncompare ve bsnap rolü
  kılavuzdan yazıldı — kaynak yüklenirse çıktılar karşılaştırılıp fark varsa düzeltilecek)

## Aşama 4 — Görsel tipografi (0.4.0) ✅
Taşı (\pos/\move, çoklu satır), döndür Z/XY + \org, ölçek, dikdörtgen/vektör clip (+iclip), damlalık.
Sırada: vektör clip'te Bezier tutamaçları, \t ile canlı animasyon önizlemesi (zaman çubuğu),
satırın metin kutusunu (\fs, \fsp, kenarlık) tuvalde çerçeve olarak gösterme,
assmotion izleme sonuçlarını tuvalde kare kare gösterip düzeltme.

## Aşama 5 — Subtitle Edit'ten alınacaklar
- ✅ 0.5.0: yaygın hataları düzelt (19 kural), Türkçe yazım denetimi (Hunspell/spylls), sahne değişimi algılama
- ✅ 0.6.0: terim sözlüğü denetimi, konuşma algılama (VAD) + konuşmaya yaslama + konuşmadan satır,
  Whisper ile yazıya dökme (referans ya da satır)
- ✅ 1.3.0: dengeli kırma, büyük/küçük harf, işitme engelli metni, süreler, boşluk köprüleme, birleştir/böl,
  kare hızı/hız/noktalı senkron, sıralama, istatistik, çoklu değiştir, TTML/DFXP/SBV/MicroDVD/WebVTT/metin,
  toplu dönüştürme, videoya yakma, MKV'ye gömme, gömülü altyazı çıkarma
- Sırada: makine çevirisi ile taslak (terim sözlüğü ve Osi kurallarıyla; anahtar Ayarlar'da), yazım denetimi
  penceresi (tek tek gez/değiştir/sözlüğe ekle), PGS/VobSub OCR, Whisper ile mevcut satırları sözcük düzeyinde
  hizalama, dalga formunda konuşma bölümlerinden otomatik zamanlama önerisi

## Aşama 7 — Aegisub eksikleri ve kullanıcı deneyimi (1.3.0) ✅
Satır seç, yazı tipi toplayıcı (eksik Türkçe karakter denetimi), çözünürlüğü yeniden örnekle, imleçten böl,
zamanları sürekli yap, 1 kare kaydırarak çoğalt, üstüne yapıştır, anahtar kare dosyaları; komut paleti,
Ayarlar + kısayol düzenleyici, çalışma düzenleri, ızgara sütunları; QC ve ızgarada büyük dosya hızı.
Sırada (Aegisub'dan): **ses bölmesinde karaoke kipi** (hece bölme/birleştirme, \k sürükleme — OP/ED karaokesi için),
stil yardımcısı (Styling Assistant), video yakınlaştırma/kaydırma, sahte video, zaman kodu (VFR v1/v2) dosyaları,
MoonScript + DependencyControl (TypesettingTools betikleri).

## Aşama 6 — Dağıtım (1.0.0) ✅
PyInstaller .exe, Inno Setup kurulum, otomatik yedek/kurtarma, çökme raporu, günlük.
Sırada: gerçek Windows'ta BUILD_EXE.bat denemesi (bana çıktısını gönder), imzasız .exe için SmartScreen notu,
güncelleme denetimi (isteğe bağlı), derinlik tabanlı örtme, hairtrack.

## Uyumluluk açıkları (izleniyor)
- MoonScript betikleri: lpeg gerekiyor (saf Lua LuLPeg ile denenecek)
- DependencyControl: ağ indirme + FFI; ayrı ele alınacak
- `text_extents`: Windows'ta GDI ile birkaç piksel sapma olabilir; gerçek Windows'ta ölçülecek
- Ses çalma 22050 Hz mono (config.json > audio_rate ile yükseltilebilir; bellek artar)
- Qt Multimedia ses aygıtı sorgusu bazı Linux ortamlarında çöküyor; bu yüzden ilk çalmada yapılıyor
- Büyük dosyada makro başlatma ~16 ms/1000 satır (20k satırda ~0.3 sn); gerekirse tembel satır kurulumu
- LuaJIT `io.open` Türkçe karakterli yolları açamaz; include/require Python üzerinden çözüldü,
  betiklerin kendi `io.open` çağrıları için sarmalayıcı düşünülecek
