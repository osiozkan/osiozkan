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
- [x] QC kare denetimleri (video açıkken): sahne değişimine ≤3 kare yakın başlangıç/bitiş (sahne taşması), aynı stilde 1-2 karelik boşluk (titreme); CLI `check --video`
- [x] QC anime uyumu: tipografi (\pos/\move/\clip/\org/çizim), karaoke (\k) ve fx satırları okuma hızı/uzunluk/süre/çakışma uyarısı almaz; çakışma artık stil+katman başına ZAMAN sırasıyla (dosya sırası değil) denetlenir
- [x] Subtitle Edit toplu işlemleri (core::batch; Zamanlama → Toplu satır işlemleri…, CLI `fix --merge-same/--merge-short/--recalc`): aynı metinli ardışık satırları birleştir, kısa satırları birleştir, süreleri okuma hızına göre yeniden hesapla (yalnızca uzat, sonraki satıra boşluk)
- [x] Toplu araçlar: kaydır, doğrusal senkron, fps dönüşümü, kareye oturt, sırala, yinelenen/boş sil, çakışma düzelt, Türkçe harf
- [x] Override etiketi ayrıştırıcı/yazıcı (gidiş-dönüş güvenli), başlangıç bloğu get/set, temizleyici
- [x] Sık hataları düzelt (core::fixes): bozuk Türkçe karakter onarımı, boşluk/noktalama, diyalog çizgisi, cümle başı büyük harf (i/İ), işitme engelli notları, süre/boşluk
- [x] Etiket onarımı ve "Notları sil" (Kalite sekmesi, CLI `fixes --tags --notes`)
- [x] Etiket denetimi (core::taglint, QC'de "Etiket"/"Notlar" süzgeci, ızgarada kırmızı altçizgi): eşleşmeyen { }, bilinmeyen/yazım hatalı etiket (\blurr, \fsc100), geçersiz renk/saydamlık, \pos/\move/\fad/\fade/\clip argüman sayısı, satır süresini aşan \fad, satır bittikten sonra başlayan \t/\move, ters zamanlar, \t ile canlandırılamayan etiketler, iç içe \t, birden fazla \pos, bilinmeyen \r stili, karaoke hece toplamı ≠ satır süresi, metinde unutulmuş {not}
- [ ] Override etiketlerinin tipli anlamlandırılması (\t animasyon düzenleme arayüzü)
- [x] Ekler: [Fonts]/[Graphics] gömme/çıkarma/silme (core::attach, ASS UUencode), "Kullanılan yazı tiplerini göm" (Araçlar → Ekler); ayrıştırıcı "[..]" gibi görünen veri satırlarını bölüm sanmıyor
- [ ] ASS Extradata yapılandırılmış erişim
- [x] Biçimler: LRC (gelişmiş sözcük zamanlı → \kf), MicroDVD, SBV, TTML/DFXP okuma+yazma; TXT/CSV yazma; CLI toplu dönüştürme
- [x] Biçimler: SAMI (.smi okuma/yazma, çok dilli dosyada ilk sınıf), SubViewer 2 (.sub okuma; MicroDVD ile otomatik ayrım)
- [ ] Biçimler: EBU STL, iTT, Netflix TTML tam, PGS/VobSub OCR
- [x] Zamanlama: videodan anahtar kareler, kareye/anahtar kareye yapışma, son-işleyici (lead-in/out, bitişik satır, anahtar kare)
- [x] Timecode dosyaları v1/v2 (VFR, `core::vfr`): Video menüsünden aç/kaydet/kapat; kare adımı, kareye yapışma, `aegisub.frame_from_ms/ms_from_frame`, "Kareye oturt", CLI `--timecodes`
- [x] Kare doğruluğu: Aegisub START/END anlambilimi (iki kare arasının ortası) — F6/F7, kareye yapışma, anahtar kare yapışması, "Kareye oturt", son-işleyici ve Lua `frame_from_ms/ms_from_frame` santisaniye yuvarlamasına dayanıklı (eskiden 23.976'da bazı satırlar bir kare geç başlıyordu); önizleme playhead'deki kareyi gösterir
- [x] VFR: videodan otomatik algılama (ffprobe paket PTS; ms yuvarlaması toleranslı), videoda "kare N VFR" göstergesi; elle yüklenen timecode önceliklidir
- [x] Matroska timestamp v4 dosyaları (sırasız satırlar sıralanır)
- [x] Referans altyazı (zamana göre eşleşen satırlar editörde gösterilir)
- [x] Dosya birleştirme (core::merge; Dosya → Başka dosyayı birleştir…, CLI `merge`): stil çakışmasında "Ad (2)" + `\r` başvuruları, farklı çözünürlükte otomatik yeniden örnekleme, zaman kaydırma (OP/ED), gömülü yazı tipleri; toplu işlemlerden sonra etkin satır/seçim içeriğe göre korunur
- [x] Yazı tipi denetimi (kullanılan/eksik yazı tipleri, dosyaları klasöre topla)
- [x] Yazım denetimi: Hunspell uyumlu (spellbook), tr_TR indir/seç, öneriler, yoksay, kullanıcı sözlüğü; `FLAG num` bayrak-0 uyumluluğu (LibreOffice tr_TR ile doğrulandı, ~1 sn yükleme)
- [x] Çeviri sekmesi (büyük referans, Enter = kaydet+sonraki, sonraki boş satır), referans sütunu, uzun satır bölme
- [x] Terim sözlüğü denetimi (kaynak = hedef; ekli biçimler kabul; Çeviri sekmesinde satır uyarısı + toplu denetim)
- [x] Noktadan senkron, FPS dönüştürme, kare cinsinden kaydırma, anahtar kare dosyası, Aegisub proje bilgisi (Video File/Active Line), kodlama seçimi/tespiti
- [x] Çeviri belleği (TM): Çeviri sekmesinde bulanık öneriler (%60+, "Kullan"), Enter ile öğrenme, Araçlar → Çeviri belleği (belgeden ekle, TSV içe/dışa aktar); `<veri>/tm.tsv`
- [ ] Otomatik çeviri bağlayıcıları (DeepL/Google; API anahtarı kullanıcıdan)
- [x] Yazım: editörde canlı hatalı sözcük vurgusu (kırmızı altçizgi), etiket/\N renklendirme
- [x] Yazım: ızgarada hata göstergesi (satır numarası kırmızı; sürüm başına önbellekli)
- [ ] Yazım: çok dilli sözlük seçimi
- [x] Karaoke hece modeli (core::karaoke) + GUI sekmesi: böl (|), birleştir, sürükle, ölçekle, harfe göre dağıt
- [x] Karaoke kipi dalga formunda: hece sınırları ve metinleri etkin satırda gösterilir, sınırlar sürüklenerek ayarlanır (toplam süre korunur)
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
- [x] DependencyControl uyumluluk katmanı (requireModules, registerMacro(s), ConfigHandler, Logger, sürüm denetimi; güncelleme yapmaz) + yedek `json` modülü; Aegisub'ın automation/include ve autoload klasörleri otomatik; `/` ile iç içe makro menüleri
- [ ] Gerçek ASSFoundation / Aegisub-Motion ile doğrulama (ffi DLL yerine yerel karşılıklar: BadMutex, PreciseTimer)
- [x] Dışa aktarım filtreleri (register_filter): Dosya → Dışa aktar (öncelik sırası, belge değişmez); CLI: `osisub run girdi builtin:kara-templater.lua "Karaoke template" çıktı`
- [x] `aegisub.frame_from_ms/ms_from_frame/video_size/get_audio_selection/project_properties` GUI'den (video+etkin satır)
- [x] Betikleri yeniden yükle (motoru sıfırlar)
- [x] Betik hata penceresi: hata iletisi + hatalı kaynak satırı (±3), yığın izi, kopyala (Windows yollarındaki boşluklar dahil)
- [x] MoonScript hatalarında .moon satırına eşleme (yükleme ve çalıştırma hataları; hata penceresi .moon kaynağını gösterir)
- [ ] Kullanıcının Lua betikleriyle regresyon test paketi (kullanıcıdan betikler istenmeli)

## Arayüz (GUI)
- [x] Çerçeve: egui/eframe 0.37 (osisub-gui). Başsız ekran görüntüsü: `xvfb-run osisub-gui dosya --screenshot out.png`
- [x] İskelet: menü, ızgara (sanal kaydırma), düzenleme paneli, stil/kalite/otomasyon/bilgi sekmeleri, Lua diyalog köprüsü, geri al/yinele
- [x] Izgara sütunları: Katman, Efekt, Kenar boşlukları (Görünüm → Izgara sütunları; kalıcı); yorum satırlarında KSS gösterilmez
- [ ] Izgara: sütun sırası/genişliği kalıcılığı
- [ ] Düzenleme kutusu: etiket renklendirme, karakter/CPS sayacı, stil seçici
- [x] Video önizleme: ffmpeg + libass ile gerçek ASS render'ı (kare başına süreç; yavaş ama doğru)
- [x] Ses dalga formu: kaydır/yakınlaştır, satır bölgeleri, başlangıç/bitiş sürükle (kareye yapışmalı), sağ tık = bitiş, Shift+tık = başlangıç
- [x] Oynatma sırasında akış (kalıcı ffmpeg boru hattı) — önceki geceden; kare önbelleği + 6 karelik ön-yükleme (ileri kare adımı anında)
- [x] Geri kare adımı için ön-yükleme (istenen karenin 3 öncesi + 5 sonrası tek ffmpeg çağrısında)
- [ ] libmpv ile donanım hızlandırmalı oynatma
- [x] Spektrogram görünümü (rustfft), anahtar kare işaretleri, video+ses satır oynatma (F5; ses harici ffplay)
- [x] Uygulama içi ses oynatma (cpal 0.18 + ffmpeg PCM; ses saati playhead/video/tuşla zamanlamayı sürer; aygıt yoksa ffplay'e düşer). HENÜZ GERÇEK AYGITTA TEST EDİLMEDİ (kum havuzunda ses aygıtı yok); mantık sahte çıkışla testli
- [x] Playhead'den sürekli oynatma (Shift+F5, Video menüsü, kısayol komutu)
- [x] Oynatma hızı (0.25–2x, perde korunur; ffmpeg atempo) ve ses seviyesi (Video menüsü, kalıcı); ses saati hızla ölçekli
- [ ] Basılı tutarak oynat
- [ ] ffmpeg olmadan çalışma (symphonia ile ses çözme)
- [x] Ses zamanlama kısayolları (Aegisub varsayılanları S/H/Q/W/E/D/Z/X; Aegisub hotkey.json eşlemesi; yeniden atanabilir)
- [x] Stil önizlemesi (libass, Türkçe örnek metin, düzenlenebilir)
- [ ] Stil yöneticisi: renk seçici iyileştirmeleri
- [x] Konum aracı: videoda \pos sürükle (varsayılan konum stil/hizalamadan türetilir)
- [x] Video araçları: \pos, \clip (dikdörtgen, köşe tutamaçları), \frz (Shift=15°), \fscx/\fscy
- [x] Video araçları: \move (başlangıç/bitiş tutamaçları, ikisini birlikte taşıma, sağ tık = \pos)
- [x] Video araçları: \org sürükleme (döndürme aracında beyaz nokta)
- [x] Video araçları: vektör \clip/\iclip (core::drawing: m/l/b korunur, ölçek parametresi; tıkla = nokta ekle, sürükle, sağ tık = sil)
- [x] Video önizlemede yakınlaştırma (1600%e kadar, imleç etrafında) ve kaydırma; araçlar yakınlaştırılmış görüntüde doğru çalışır
- [x] Video araçları: 3B döndürme \frx/\fry (yatay/dikey sürükle, Shift = 15°)
- [x] Hareket izleme verisi uygulama (core::motion; Araçlar → Hareket izleme verisi uygula…, CLI `motion`): Mocha/AE Transform Data → kare kare \pos/\fscx/\fscy/\frz (+\bord/\shad/\blur), kaynak çözünürlük ölçeklemesi, \t/\fad kaydırma, \move → \pos; VFR uyumlu
- [x] Hareket izleme: \clip/\iclip izleme (dikdörtgen; dönüşte vektöre çevrilir; vektör kırpma ölçek parametresiyle)
- [ ] Hareket izleme: doğrusal kip (\move+\t), köşe pini (CC Power Pin)
- [x] Tipografi yardımcıları (core::typeset; Araçlar → Degrade / parıltı…): kırpma şeritleriyle degrade (renk/saydamlık/sayısal etiketler, yatay/dikey, ivme), parıltı katmanı
- [ ] Perspektif aracı (4 köşe → \frx\fry\fax\org), metin → çizim (\p1) dönüştürme
- [ ] Video araçları: \p1 çizim düzenleme, \fax/\fay, \t animasyon düzenleme, vektör kırpmada eğri (b) ekleme
- [x] Makro menüsü, Lua diyalog penceresi (otomasyon sekmesi)
- [x] Bul/Değiştir (regex, Türkçe i/İ, alan/kapsam seçimi), çözünürlük yeniden örnekleme
- [x] Sürüm karşılaştırma (Dosya → Başka sürümle karşılaştır; CLI `compare`): zaman örtüşmesiyle eşleme, eklenen/silinen/metin-zaman-stil değişimi, tıklayınca satıra git
- [x] Zamanlama: "Video karesinde böl", "Çoğalt ve 1 kare kaydır" (kare kare tipografi)
- [x] İmleçte böl (düzenleme kutusu sağ tık; süre metne orantılı ya da video karesinde; kısayol atanabilir)
- [x] Satırları seç (Aegisub Select Lines): metin/stil/konuşan/efekt; içerir/tam/regex; ters; diyalog/yorum; yeni/ekle/çıkar/kesişim (Düzen menüsü)
- [x] Lua makrolarına kısayol atama (Otomasyon sekmesi; settings.json `macro_hotkeys`)
- [x] Otomasyon: elle yüklenen betikler kalıcı (settings.json `scripts`), "Yüklü betik dosyaları" listesi + Kaldır; aynı betiği yeniden yüklemek motoru tazeler
- [x] Yerleşik komutlara kısayol atama (Yardım → Kısayolları düzenle; 19 komut)
- [x] Aegisub kısayol profilini içe aktarma (hotkey.json; 3.0 nesne ve 3.2 "Ctrl-K" biçimleri, Lua makro kısayolları dahil)
- [x] Belgeye bağlı betikler (Aegisub "Automation Scripts" alanı: `~` göreli, `$` otomasyon tabanı, `/` mutlak; açılışta yüklenir, Otomasyon sekmesinde "belgeye bağlı")
- [x] Geri al/yinele, otomatik kayıt, kalıcı ayarlar, son açılanlar, sürükle-bırak, sağ tık menüsü, kopyala/kes/yapıştır
- [x] Çökme sonrası otomatik kayıttan kurtarma, çökme günlüğü, kaydedilmemiş değişiklik onayı
- [ ] Türkçe + İngilizce arayüz, açık/koyu tema

## Performans
- [ ] Büyük dosyalarda ayrıştırma ölçümleri (benchmark), bellek ayak izi
- [ ] Lua çağrıları için toplu (batch) API ile satır kopyası azaltma

## Dağıtım
- [x] GitHub Actions: Linux/Windows/macOS derleme + test (.github/workflows/ci.yml; henüz çalıştırılmadı)
- [ ] Sürüm etiketleri, değişiklik günlüğü
