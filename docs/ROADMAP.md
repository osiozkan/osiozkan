# Yol Haritası

Durum: ✅ var · 🟡 kısmi · ⬜ yok

## Aegisub eşdeğerliği
- ✅ ASS/SSA oku-yaz (stil, olay, info, fonts/graphics korunur), kodlama tespiti (UTF-8/16, cp1254)
- ✅ Satır ızgarası (50k satırda akıcı), düzenleme kutusu, etiket vurgulama, çoklu seçim alan düzenleme
- ✅ Ses dalga formu + spektrogram, sürükle-zamanla, anahtar kare/satır kenarı yapışması
- ✅ Video + libass overlay (video yokken yalın önizleme)
- ✅ Stil yöneticisi (canlı libass önizleme), betik özellikleri, çözünürlük yeniden örnekleme
- ✅ Bul/Değiştir (regex), zaman kaydır, son işlemci (lead-in/out, kare yapışma, boşluk), iki nokta senkron, FPS dönüştür
- ✅ Auto4 Lua: makro, doğrulama, iletişim kutuları, ilerleme, günlük, `karaskel`, `util`, `re`, `unicode`
- ✅ Auto4 filtreleri: Dosya > Filtreli dışa aktar (yapılandırma iletişim kutusu dahil)
- ✅ Karaoke zamanlayıcı (karakter/sözcük/romaji bölme, eşitle, dokunarak zamanla) — 🟡 dalga formunda hece sürükleme yok
- ✅ Görsel dizgi: \pos, \clip/\iclip (dikdörtgen), \frz — ⬜ vektör klip, \org, \move, ölçek/ \fscx-y tutamaçları, çizim (\p) aracı
- ✅ VFR zaman kodları (v1/v2 dosyası + videodan otomatik algılama)
- ✅ Gömülü yazı tipi/grafik ekleri, yazı tipi toplayıcı/eksik tespiti
- ✅ Kısayol özelleştirme (çakışma denetimli, kalıcı)
- ✅ Aegisub'ın kendi kara-templater/karaskel/cleantags betikleri çalışıyor (birebir)
- ✅ Satır seç (regex/stil/aktör/yorum)
- ⬜ Çeviri yardımcısı (Aegisub), kanji zamanlayıcı

## Subtitle Edit'ten alınanlar
- ✅ Format dönüşümü (SRT/VTT/TXT/MicroDVD/LRC), yaygın hata düzeltme, İE notu silme, satır dengeleme, TR-harf büyüklüğü
- ✅ Okuma hızı (CPS), satır uzunluğu, süre, çakışma, boşluk denetimi; canlı renklendirme
- ✅ Süre ayarlama, boşluk kapatma, yinelenen birleştirme
- ✅ Çeviri/referans paneli (zamana veya sıraya göre eşleme, satıra kopyala)
- ✅ İstatistikler
- ⬜ Yazım denetimi (hunspell/Türkçe sözlük), iki dosya karşılaştırma, toplu dönüştürme (klasör)
- ⬜ Görsel senkron (iki nokta + ses önizleme)

## Kalite / performans
- ✅ 50k satır: ayrıştırma 0.4 sn, QC 0.7 sn, Lua makro 1.1 sn
- ⬜ Snapshot yerine fark (diff) tabanlı geri al (çok büyük dosyalar)
- ⬜ Windows/macOS paketleme (PyInstaller), libass DLL ekleme
- ⬜ Gerçek Aegisub betikleriyle uyumluluk test seti (kullanıcı betikleri gelince `tests/lua_user/`)

## Sonraki turlar için öneri (öncelik sırasıyla)
1. **Kullanıcının kendi Lua betikleri** `scripts/` altına konulunca: hepsini yükle/çalıştır, hataları `docs/SORUNLAR.md`'ye yaz, API eksiklerini kapat.
2. Aegisub ile birebir `aegisub.text_extents` (Windows'ta GDI) farkları — şablon çıktılarında ±px sapma var mı karşılaştır.
3. Dalga formunda karaoke hece sürükleme, vektör klip/çizim aracı, \move/\org tutamaçları.
4. Yazım denetimi (Türkçe), iki dosya karşılaştırma, toplu dönüştürme.
5. Windows paketleme (PyInstaller + libass DLL + ffmpeg), otomatik güncelleme/sürüm bildirimi.
6. Büyük dosyalarda (100k+ satır) snapshot yerine fark tabanlı geri al.
