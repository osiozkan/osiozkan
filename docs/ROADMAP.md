# Yol haritası

Öncelik sırası yukarıdan aşağıya. Tamamlananlar `[x]`.

## Sıradaki geceler için öncelikli öneriler
1. **Gerçek makinede doğrulama:** ses aygıtı (F5, tuşla zamanlama gecikmesi), Windows/macOS çalıştırma, GitHub Actions'ın ilk gerçek çalışması (CI + release iş akışları hiç çalıştırılmadı).
2. **Kullanıcının kendi Lua betikleri** ile regresyon testi (betikler depoya eklenip `crates/osisub-lua/tests/fixtures/`'a konmalı; çalışmayan API'ler `docs/LUA_UYUMLULUK.md`'e işlenmeli). `kara-templater.lua` uyumluluğu.
3. **Video araçları:** vektör `\clip`/çizim, `\t` animasyon düzenleme.
4. Çevrimiçi çeviri bağlayıcıları (anahtar gerektirir; kullanıcıya sorulmalı).
5. **Kısayol özelleştirme** (Aegisub kısayol profili).
6. **Performans:** yazı tipi dizini önbelleği; video için kalıcı ffmpeg/libmpv hattı; büyük belgede QC'yi artımlı yapma.
7. Ses/dalga: basılı tutarak oynat, hız/ses seviyesi, VFR/timecode dosyaları.

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
- [x] Timecode dosyaları v1/v2 (VFR, `core::vfr`): Video menüsünden aç/kaydet/kapat; kare adımı, kareye yapışma, `aegisub.frame_from_ms/ms_from_frame`, "Kareye oturt", CLI `--timecodes`
- [x] VFR: videodan otomatik algılama (ffprobe paket PTS; ms yuvarlaması toleranslı), videoda "kare N VFR" göstergesi; elle yüklenen timecode önceliklidir
- [ ] Matroska timestamp v4 dosyaları
- [x] Referans altyazı (zamana göre eşleşen satırlar editörde gösterilir)
- [x] Yazı tipi denetimi (kullanılan/eksik yazı tipleri, dosyaları klasöre topla)
- [x] Yazım denetimi: Hunspell uyumlu (spellbook), tr_TR indir/seç, öneriler, yoksay, kullanıcı sözlüğü; `FLAG num` bayrak-0 uyumluluğu (LibreOffice tr_TR ile doğrulandı, ~1 sn yükleme)
- [x] Çeviri sekmesi (büyük referans, Enter = kaydet+sonraki, sonraki boş satır), referans sütunu, uzun satır bölme
- [x] Terim sözlüğü denetimi (kaynak = hedef; ekli biçimler kabul; Çeviri sekmesinde satır uyarısı + toplu denetim)
- [x] Noktadan senkron, FPS dönüştürme, kare cinsinden kaydırma, anahtar kare dosyası, Aegisub proje bilgisi (Video File/Active Line), kodlama seçimi/tespiti
- [x] Çeviri belleği (TM): Çeviri sekmesinde bulanık öneriler (%60+, "Kullan"), Enter ile öğrenme, Araçlar → Çeviri belleği (belgeden ekle, TSV içe/dışa aktar); `<veri>/tm.tsv`
- [ ] Otomatik çeviri bağlayıcıları (DeepL/Google; API anahtarı kullanıcıdan)
- [x] Yazım: editörde canlı hatalı sözcük vurgusu (kırmızı altçizgi), etiket/\N renklendirme
- [ ] Yazım: çok dilli sözlük seçimi, ızgarada hata göstergesi
- [x] Karaoke hece modeli (core::karaoke) + GUI sekmesi: böl (|), birleştir, sürükle, ölçekle, harfe göre dağıt
- [x] Karaoke: oynatırken Boşluk ile tuşla zamanlama (gecikme ayarlı; çekirdek `apply_taps` testli)
- [x] Karaoke: otomatik Japonca hece (mora) ayırma — romaji (ki|t|to, shi|n|ji, kyo|u) ve kana (きょ|う, っ ayrı); şablonlayıcı: gerçek kara-templater yerleşik
- [ ] Karaoke: gecikmeyi ses saatiyle otomatik kalibre et; kanji için okuma (furigana) yardımı

## Lua (Automation 4)
- [x] subs nesnesi, register_macro/filter, dialog.display, re, unicode, lfs, karaskel, utils
- [x] Gerçek yazı tipi ölçüleri (`text_extents`: ttf-parser, GDI/VSFilter uyumlu hücre yüksekliği)
- [x] Yazı tipi dizini önbelleği (`<veri>/font-index.tsv`; yalnızca değişen dosyalar yeniden okunur, `OSISUB_NO_FONT_CACHE=1` kapatır)
- [x] `kara-templater.lua` yerleşik ve gerçek Aegisub `karaskel-auto4.lua` ile testli
- [x] MoonScript (`.moon`) betikleri, include klasörlerinden `require`
- [ ] Aegisub'ın diğer gelen betiklerinin (strip-tags, select-overlaps, auto-leadin…) otomatik testi — bu gece indirilen betikleri çalıştırmak otomatik mod güvenlik denetimince engellendi; betikler depoya kullanıcı onayıyla eklenmeli
- [ ] DependencyControl uyumluluğu (ASSFoundation, Aegisub-Motion gibi betik aileleri için)
- [x] Dışa aktarım filtreleri (register_filter): Dosya → Dışa aktar (öncelik sırası, belge değişmez); CLI: `osisub run girdi builtin:kara-templater.lua "Karaoke template" çıktı`
- [x] `aegisub.frame_from_ms/ms_from_frame/video_size/get_audio_selection/project_properties` GUI'den (video+etkin satır)
- [x] Betikleri yeniden yükle (motoru sıfırlar)
- [x] Betik hata penceresi: hata iletisi + hatalı kaynak satırı (±3), yığın izi, kopyala (Windows yollarındaki boşluklar dahil)
- [ ] MoonScript hatalarında .moon satırına eşleme (şu an derlenmiş Lua satırı)
- [ ] Kullanıcının Lua betikleriyle regresyon test paketi (kullanıcıdan betikler istenmeli)

## Arayüz (GUI)
- [x] Çerçeve: egui/eframe 0.37 (osisub-gui). Başsız ekran görüntüsü: `xvfb-run osisub-gui dosya --screenshot out.png`
- [x] İskelet: menü, ızgara (sanal kaydırma), düzenleme paneli, stil/kalite/otomasyon/bilgi sekmeleri, Lua diyalog köprüsü, geri al/yinele
- [x] Izgara sütunları: Katman, Efekt, Kenar boşlukları (Görünüm → Izgara sütunları; kalıcı); yorum satırlarında KSS gösterilmez
- [ ] Izgara: sütun sırası/genişliği kalıcılığı
- [ ] Düzenleme kutusu: etiket renklendirme, karakter/CPS sayacı, stil seçici
- [x] Video önizleme: ffmpeg + libass ile gerçek ASS render'ı (kare başına süreç; yavaş ama doğru)
- [x] Ses dalga formu: kaydır/yakınlaştır, satır bölgeleri, başlangıç/bitiş sürükle (kareye yapışmalı), sağ tık = bitiş, Shift+tık = başlangıç
- [ ] Akıcı video oynatma (libmpv ya da kalıcı ffmpeg boru hattı), kare önbelleği, ön-yükleme
- [x] Spektrogram görünümü (rustfft), anahtar kare işaretleri, video+ses satır oynatma (F5; ses harici ffplay)
- [x] Uygulama içi ses oynatma (cpal 0.18 + ffmpeg PCM; ses saati playhead/video/tuşla zamanlamayı sürer; aygıt yoksa ffplay'e düşer). HENÜZ GERÇEK AYGITTA TEST EDİLMEDİ (kum havuzunda ses aygıtı yok); mantık sahte çıkışla testli
- [ ] Basılı tutarak oynat, sürekli oynatma (satır sınırı olmadan), hız ayarı, ses seviyesi
- [ ] ffmpeg olmadan çalışma (symphonia ile ses çözme)
- [ ] Ses zamanlama: sürükle, snap, kısayol odaklı akış (Aegisub kısayollarıyla uyumlu)
- [ ] Stil yöneticisi, yazı tipi önizleme, renk seçici
- [x] Konum aracı: videoda \pos sürükle (varsayılan konum stil/hizalamadan türetilir)
- [x] Video araçları: \pos, \clip (dikdörtgen, köşe tutamaçları), \frz (Shift=15°), \fscx/\fscy
- [x] Video araçları: \move (başlangıç/bitiş tutamaçları, ikisini birlikte taşıma, sağ tık = \pos)
- [x] Video araçları: \org sürükleme (döndürme aracında beyaz nokta)
- [ ] Video araçları: vektör \clip/çizim, \fax/\fay, \t animasyon düzenleme
- [x] Makro menüsü, Lua diyalog penceresi (otomasyon sekmesi)
- [x] Bul/Değiştir (regex, Türkçe i/İ, alan/kapsam seçimi), çözünürlük yeniden örnekleme
- [x] Satırları seç (Aegisub Select Lines): metin/stil/konuşan/efekt; içerir/tam/regex; ters; diyalog/yorum; yeni/ekle/çıkar/kesişim (Düzen menüsü)
- [x] Lua makrolarına kısayol atama (Otomasyon sekmesi; settings.json `macro_hotkeys`)
- [x] Otomasyon: elle yüklenen betikler kalıcı (settings.json `scripts`), "Yüklü betik dosyaları" listesi + Kaldır; aynı betiği yeniden yüklemek motoru tazeler
- [ ] Yerleşik komutlar için kısayol profili (Aegisub uyumlu); belge başına betikler (Project Garbage "Automation Scripts")
- [x] Geri al/yinele, otomatik kayıt, kalıcı ayarlar, son açılanlar, sürükle-bırak, sağ tık menüsü, kopyala/kes/yapıştır
- [x] Çökme sonrası otomatik kayıttan kurtarma, çökme günlüğü, kaydedilmemiş değişiklik onayı
- [ ] Türkçe + İngilizce arayüz, açık/koyu tema

## Performans
- [ ] Büyük dosyalarda ayrıştırma ölçümleri (benchmark), bellek ayak izi
- [ ] Lua çağrıları için toplu (batch) API ile satır kopyası azaltma

## Dağıtım
- [x] GitHub Actions: Linux/Windows/macOS derleme + test (.github/workflows/ci.yml; henüz çalıştırılmadı)
- [ ] Sürüm etiketleri, değişiklik günlüğü
