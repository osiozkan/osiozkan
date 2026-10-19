# SubForge Yol Haritası

Durum: ✅ var · 🟡 kısmen · ⬜ yok. Her gece bu liste yukarıdan aşağıya ilerletilir (bkz. CLAUDE.md).

## Çekirdek / format
- ✅ ASS/SSA okuma-yazma, SRT, VTT
- ✅ Kodlama algılama, mojibake onarımı, Türkçe büyük/küçük harf
- ✅ SSA v4 yazma, Extradata, gömülü font/grafik ekleri (çekirdek API; GUI arayüzü ⬜)
- ✅ MicroDVD, SubViewer, SBV, SAMI, LRC, TTML, TXT; ⬜ STL, PGS/VobSub (OCR)
- 🟡 Gömülü fontlar: çekirdekte çıkarma/gömme var, GUI'de yok
- ⬜ MKV içinden altyazı/font çıkarma ve geri gömme (mkvtoolnix çağrısı)

## Automation 4 (Lua)
- ✅ `subs` nesnesi, makrolar, diyaloglar, `karaskel`, `utils`, `unicode`, `re`, LuaJIT `ffi`/`bit`
- 🟡 `aegisub.text_extents` — sezgisel; **gerçek font ölçümü** (ttf-parser/fontdb) gerekli
- ✅ Dışa aktarma filtreleri GUI'den
- ⬜ `aegisub.dialog.open/save` GUI bağlantısı, `lfs` modülü, `clipper` (polygon), `lpeg` (LuaJIT C modülü)
- ✅ MoonScript (.moon) desteği (gömülü MoonScript+LuLPeg); ⬜ gerçek DependencyControl, ILL/Yutils/Aegisub-Motion testleri (bkz. crates/lua/COMPAT.md)
- ⬜ Kullanıcının kendi Lua betikleriyle uyumluluk test paketi (örnek betikler `tests/fixtures/`)
- ✅ Aegisub kurulum/profil klasörleri otomatik bulunur (`automation/autoload|include`)

## Zamanlama / ses / video
- ✅ Dalga biçimi, seçim, sürükle-bırak, anahtar kareler
- 🟡 Ses oynatma ffplay ile; video oynatma ffmpeg boru hattı (A/V senkron gevşek) → gerçek A/V saat, spektrum görünümü
- 🟡 Basit VAD ile konuşma bulma ✅; Zamanlayıcı modu (Aegisub tuş eşlemeleri) ⬜
- ⬜ Zaman kodu dosyaları (VFR), keyframe dosyası içe/dışa aktarma
- 🟡 Video üzerinde `\pos` sürükleme ✅; `\org`, `\clip`, `\frz`, ölçek tutamaçları, vektör çizim araçları ⬜
- ⬜ Gerçek zamanlı video oynatma

## Düzenleme
- ✅ Grid, düzenleyici, geri al/yinele, bul-değiştir, stil yöneticisi, QC penceresi
- ⬜ Satır içi etiket yardımcıları (italik/kalın/renk kısayolları), etiket temizleyici
- ✅ Yazım denetimi (Hunspell/spellbook, tr_TR indirilir); ⬜ satır içi altı çizili gösterim
- ⬜ Çeviri/çeviri-denetim modu (iki sütun), TM/terim sözlüğü, DeepL/LLM ile ön çeviri (isteğe bağlı)
- ✅ Stil içe aktarma/önizleme; ⬜ stil depolama (Style Storage), kitle stil değişimi
- 🟡 Karaoke düzenleyici penceresi ✅, kara-templater Lua ile çalışıyor ✅; ses üzerinde hece zamanlama ⬜
- ⬜ Satır sütunlarını özelleştirme, filtre/arama çubuğu, bölüm/işaret listesi

## Kalite / performans
- 🟡 Geri al: tam anlık görüntü (büyük FX betiklerinde yavaşlar) → fark tabanlı yapılmalı
- ⬜ 100k+ satırlı betik performans testleri/benchmark
- ⬜ GUI uçtan uca testleri (Xvfb ekran görüntüsü)
- ⬜ Paketleme: Windows (.msi/.zip), macOS (.dmg), Linux AppImage; GitHub Actions sürüm iş akışı

## Sonraki gece için öneriler
- GUI: gömülü font yöneticisi, VFR zaman kodu/anahtar kare dosyası içe-dışa aktarma, kısayol özelleştirme, İngilizce arayüz seçeneği
- Performans: fark tabanlı geri al (şu an tam kopya, büyük betiklerde derinlik sınırlı)
- Lua: `crates/lua/COMPAT.md` boşlukları (gerçek DependencyControl, native clipper)
- Ses: spektrum görünümü, gerçek A/V saat
- GUI için Xvfb ekran görüntüsü regresyon testleri
