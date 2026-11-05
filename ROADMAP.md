# Kitsune — yol haritası ve gece günlüğü

Hedef: Aegisub'ı tam kopyalayan + Subtitle Edit'in iyi özelliklerini katan, Lua (Aegisub API) betiklerini
çalıştıran, kolay arayüzlü anime altyazı aracı. Her gece 00:00–06:00 arası bir sonraki sürüm hazırlanır.

## Mimari
- `kitsune/core` — ASS/SRT/VTT modeli, zaman, etiketler, kalite kontrol (GUI'den bağımsız, test edilebilir)
- `kitsune/lua` — Aegisub uyumlu Lua çalışma zamanı (LuaJIT/lupa): `subs` (userdata), `aegisub.*`, `karaskel`, `utils`, `re`, `unicode`
- `kitsune/gui` — PySide6 arayüz
- `automation/` — kullanıcı Lua makroları (otomatik yüklenir; ayrıca `~/.kitsune/automation`)
- Çalıştırma: `python -m kitsune gui|run|convert|lint`, testler: `pytest`

## v0.1 (1. gece) — tamam
- ASS/SSA oku-yaz (stil, bilgi, bilinmeyen bölümler korunur), SRT/VTT içe/dışa aktarma
- Aegisub Lua API çekirdeği: subs indeksleme/insert/append/delete, register_macro/filter, dialog.display, progress, karaskel/utils alt kümesi, rollback
- GUI: ızgara, düzenleme paneli, geri al/yinele, ekle/sil/çoğalt/birleştir, bul-değiştir, zaman kaydır, stil yöneticisi, QC paneli, Otomasyon menüsü
- Zamanlama: kareye yapıştır, boşluk kapat, çakışma düzelt, min süre/CPS, \N ile böl (`core/timing.py`, GUI menüsüne bağlanacak)
- QC: CPS, süre, satır uzunluğu/sayısı, çakışma, tanımsız stil, kapanmamış `{}`, boşluk

## Sıradaki işler (öncelik sırasıyla)
1. Video önizleme (libmpv/QtMultimedia), kare-bazlı zamanlama, anahtar kareler, FPS/timecode dosyası
2. Ses dalga formu + spektrum, görsel zamanlama (sürükle-bırak başlangıç/bitiş), ses önbelleği
3. ASS render önizleme (libass bağlama) — yazı tipi, `\pos`, `\move`, `\clip`, çizim
4. Aegisub API eksikleri: tam `karaskel` (furigana, `preproc_line_pos`), `util.lua` tamamı, `dialog` tüm sınıflar (color, coloralpha, slider), `aegisub.dialog.open/save`, `lfs`, `ffi`, `re` tam Boost sözdizimi, `unicode`
5. Aegisub komutları: Çevir/Zamanlama asistanı (timing post-processor, snap to keyframes), Çeviri Asistanı, Kareoke modu, Sürükle-pos görsel araçlar (VSFilter mod: `\pos`, `\org`, `\frz`, `\clip`)
6. Subtitle Edit'ten: çoklu format (50+), otomatik çeviri (DeepL/Google), bölüm bölme/birleştirme, sesli yazım (Whisper) → kaba zamanlama, yazım denetimi (TR/EN), sabit karakter sınırı düzeltici, "Satırları yeniden böl", OCR (Blu-ray sup/idx)
7. Anime'ye özel: stil şablonları (typesetting/sign, OP/ED karaoke), `\fad` yardımcıları, mocha tracking verisi içe aktarma, font toplayıcı (mux öncesi), Matroska'ya font/altyazı gömme (mkvmerge)
8. Performans: sanal tablo (100k satır), artımlı undo (komut tabanlı), arka plan ses/video çözme
9. Paketleme: PyInstaller (Win/Mac/Linux), tema (koyu/açık), Türkçe/İngilizce arayüz, eklenti sistemi

## Bilinen sorunlar / notlar
- Lua `string.upper` ASCII'dir (Aegisub ile aynı); Unicode için `unicode.*`
- `text_extents` GUI'de Qt ile ölçer, headless'ta tahmindir (karaskel çıktıları farklı olabilir)
- Geri alma tam-kopya tabanlıdır (büyük dosyada yavaşlar → madde 8)
