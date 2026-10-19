# SubForge Yol Haritası

Durum: ✅ var · 🟡 kısmen · ⬜ yok. Her gece bu liste yukarıdan aşağıya ilerletilir (bkz. CLAUDE.md).

## Çekirdek / format
- ✅ ASS/SSA okuma-yazma, SRT, VTT
- ✅ Kodlama algılama, mojibake onarımı, Türkçe büyük/küçük harf
- ⬜ SSA v4 (eski) tam yazma, `[Aegisub Extradata]` yapılandırılmış erişim
- ⬜ Diğer formatlar: SUB/MicroDVD, SBV, TTML, STL, PGS/VobSub (OCR) içe aktarma
- ⬜ Gömülü fontlar (uuencode) çıkarma/gömme
- ⬜ MKV içinden altyazı/font çıkarma ve geri gömme (mkvtoolnix çağrısı)

## Automation 4 (Lua)
- ✅ `subs` nesnesi, makrolar, diyaloglar, `karaskel`, `utils`, `unicode`, `re`, LuaJIT `ffi`/`bit`
- 🟡 `aegisub.text_extents` — sezgisel; **gerçek font ölçümü** (ttf-parser/fontdb) gerekli
- ⬜ Dışa aktarma filtreleri GUI'den (config diyaloğu + sıralama)
- ⬜ `aegisub.dialog.open/save` GUI bağlantısı, `lfs` modülü, `clipper` (polygon), `lpeg` (LuaJIT C modülü)
- ⬜ MoonScript (.moon) derleyici desteği, Yutils/ILL/Aegisub-Motion uyumluluk testleri
- ⬜ Kullanıcının kendi Lua betikleriyle uyumluluk test paketi (örnek betikler `tests/fixtures/`)
- ⬜ Aegisub'da çalışan `automation/include/` kütüphaneleri için bulma yolu ayarı

## Zamanlama / ses / video
- ✅ Dalga biçimi, seçim, sürükle-bırak, anahtar kareler
- ⬜ Spektrum görünümü, ses oynatma (şu an ffplay), kare-hassas oynatma
- ⬜ Zamanlayıcı modu (Aegisub tuş eşlemeleri), otomatik zamanlama (VAD), sessizlik bölme
- ⬜ Zaman kodu dosyaları (VFR), keyframe dosyası içe/dışa aktarma
- ⬜ Video üzerinde sürükleyerek konumlandırma (`\pos`, `\org`, `\clip`), vektör çizim araçları
- ⬜ Gerçek zamanlı video oynatma

## Düzenleme
- ✅ Grid, düzenleyici, geri al/yinele, bul-değiştir, stil yöneticisi, QC penceresi
- ⬜ Satır içi etiket yardımcıları (italik/kalın/renk kısayolları), etiket temizleyici
- ⬜ Yazım denetimi (Türkçe sözlük: hunspell), noktalama/tırnak düzeltme
- ⬜ Çeviri/çeviri-denetim modu (iki sütun), TM/terim sözlüğü, DeepL/LLM ile ön çeviri (isteğe bağlı)
- ⬜ Kitle stil değişimi, stil içe/dışa aktarma, stil önizleme
- ⬜ Karaoke araçları: heceleme, `\k` zamanlama, şablon (kara-templater) çalıştırıcı
- ⬜ Satır sütunlarını özelleştirme, filtre/arama çubuğu, bölüm/işaret listesi

## Kalite / performans
- 🟡 Geri al: tam anlık görüntü (büyük FX betiklerinde yavaşlar) → fark tabanlı yapılmalı
- ⬜ 100k+ satırlı betik performans testleri/benchmark
- ⬜ GUI uçtan uca testleri (Xvfb ekran görüntüsü)
- ⬜ Paketleme: Windows (.msi/.zip), macOS (.dmg), Linux AppImage; GitHub Actions sürüm iş akışı
