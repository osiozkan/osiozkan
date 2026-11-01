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
- 🟡 Auto4 filtreleri (dışa aktarma filtreleri) — motor hazır, GUI'de dışa aktarma menüsüne bağlanmadı
- ⬜ Karaoke modu (hece bölme + zamanlama)
- ⬜ Görsel dizgi araçları (\pos, \clip, \org, \frz sürükle)
- ⬜ VFR zaman kodları (v1/v2), Matroska timecodes
- ⬜ Gömülü yazı tipi/grafik (uuencode) ekleri, yazı tipi toplayıcı
- ⬜ Kısayol özelleştirme
- ⬜ kara-templater (Aegisub'ın) uyumlu şablon motoru — kullanıcının betikleri buna bağlıysa öncelik ver
- ⬜ Satır seç (regex/stil/aktör), çeviri yardımcısı, kanji zamanlayıcı

## Subtitle Edit'ten alınanlar
- ✅ Format dönüşümü (SRT/VTT/TXT/MicroDVD/LRC), yaygın hata düzeltme, İE notu silme, satır dengeleme, TR-harf büyüklüğü
- ✅ Okuma hızı (CPS), satır uzunluğu, süre, çakışma, boşluk denetimi; canlı renklendirme
- ✅ Süre ayarlama, boşluk kapatma, yinelenen birleştirme
- ⬜ Çeviri modu (referans altyazı yan yana)
- ⬜ Yazım denetimi (hunspell), istatistikler, karşılaştırma, toplu dönüştürme (klasör)
- ⬜ Görsel senkron (iki nokta + ses önizleme)

## Kalite / performans
- ✅ 50k satır: ayrıştırma 0.4 sn, QC 0.7 sn, Lua makro 1.1 sn
- ⬜ Snapshot yerine fark (diff) tabanlı geri al (çok büyük dosyalar)
- ⬜ Windows/macOS paketleme (PyInstaller), libass DLL ekleme
- ⬜ Gerçek Aegisub betikleriyle uyumluluk test seti (kullanıcı betikleri gelince `tests/lua_user/`)
