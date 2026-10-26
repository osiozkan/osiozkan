# Yol haritası ve gece günlüğü

Hedef: Aegisub'ın tüm yetenekleri + Subtitle Edit'in iyi özellikleri + kullanıcının mevcut Lua scriptleriyle tam uyum, kolay arayüz, yüksek performans.
Her gece bu dosya güncellenir; bir sonraki sürüm buradaki "Sıradaki" listesinden devam eder.

## v0.1.0 (ilk gece) — yapıldı
- ASS/SSA/SRT/VTT/TXT okuma-yazma; bilinmeyen bölümler (Fonts, Graphics, Aegisub garbage/extradata) korunur
- QC: CPS, CPL, satır sayısı, süre, çakışma, eksik stil, süslü parantez, boşluklar; düzeltmeler: çakışma, boşluk, min süre, satır dengeleme, sıralama
- Kaydırma, doğrusal senkron, fps dönüşümü, keyframe yakalama
- Lua Automation 4 ana makinesi (makro + filtre), karaskel/util/re/unicode alt kümesi, LuaJIT uyumluluk gölgeleri
- CLI + yerel sunucu + web editörü (video, dalga biçimi, grid, panel)
- Testler: `npm test`

## Bilinen eksikler / riskler
- `aegisub.text_extents` yaklaşık; gerçek font ölçümü yok (tarayıcıda canvas ile enjekte edilebilir)
- Lua diyalogları etkileşimsiz (varsayılan/override değerleri); gerçek diyalog UI'ı yok
- Dalga biçimi: tüm sesi bellekte çözer (büyük dosyalarda ağır) → ffmpeg/WASM akışlı çözüm gerek
- Video üstünde yalnız basit metin; `\pos`, `\move`, `\clip`, çizimler, karaoke efektleri çizilmiyor → libass-WASM (JASSUB) entegrasyonu
- Kullanıcının kendi Lua scriptleri depoda yok; verildiğinde `samples/` altına alınıp regresyon testi yazılacak

## Sıradaki (öncelik sırasıyla)
1. Kullanıcının Lua scriptlerini yükleyip uyumluluk sınaması (ihtiyaç duyulan API boşluklarını kapat: `aegisub.dialog` düzeni, `karaskel` tam, `lfs`, `io`/`os` erişimi)
2. Etiket ayrıştırıcı/yazıcı (override tag AST) + etiket temizleme, `\t` desteği, renk/alfa yardımcıları
3. JASSUB (libass-WASM) ile gerçek ASS önizleme; font yönetimi
4. Karaoke aracı (\k, otomatik hece bölme, şablon "kara-templater" benzeri Lua uyumu)
5. Ses: akışlı dalga biçimi/spektrum, otomatik zamanlama önerisi, keyframe (sahne değişimi) çıkarma, magnetic snap
6. Tam diyalog UI'ı (Aegisub dialog JSON → form), makro kısayolları, makro menüsü
7. Yazım denetimi (Türkçe hunspell), çeviri modu (iki dil yan yana), toplu işlem, proje dosyası
8. Performans: 50k satır, worker içinde Lua çalıştırma, artımlı QC
