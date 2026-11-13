# Yol haritası

Öncelik sırası yukarıdan aşağıya. Tamamlananlar `[x]`.

## Sıradaki geceler için öncelikli öneriler
1. **Kullanıcının kendi Lua betikleri** ile regresyon testi (en kritik; betikler depoya eklenmeli → `crates/osisub-lua/tests/fixtures/`). `SubInspector.Inspector` karşılığı (ASSFoundation `getLineBounds`); gerçek ASSFoundation / Aegisub-Motion ile uçtan uca deneme.
2. **Gerçek makinede doğrulama:** ses aygıtı (F5, tuşla zamanlama gecikmesi), Windows paketi (gömülü ffmpeg ile video/ses), macOS'ta GUI. Gece paketleri artık her gece Actions'ta (Release).
3. Video araçları: döndürülmüş / `\an≠7` çizim düzenleme, `\fax/\fay` aracı.
4. libmpv ile donanım hızlandırmalı oynatma.
5. Karaoke: gecikme kalibrasyonu, furigana düzeni; basılı tutarak oynat; ızgara sütun sırası.

## Çekirdek
- [x] ASS/SSA oku-yaz, bilinmeyen bölümleri koru, CRLF algıla, UTF-8/16/Windows-1254
- [x] SRT/VTT oku-yaz, temel HTML etiketi ↔ ASS dönüşümü
- [x] QC: çakışma, süre, okuma hızı, satır uzunluğu, bilinmeyen stil, boşluk
- [x] QC raporu (HTML, tek dosya; Kalite sekmesi → Raporu kaydet, CLI `check --report`): başlık + kategoriye göre satır sorunları, açık/koyu tema
- [x] QC betik başlığı: PlayRes yok/en-boy oranı videoyla uyuşmuyor, ScaledBorderAndShadow, YCbCr Matrix (HD video + TV.601), yinelenen stil adı; tek tıkla düzeltme (Kalite sekmesi), CLI check
- [x] QC kare denetimleri (video açıkken): sahne değişimine ≤3 kare yakın başlangıç/bitiş (sahne taşması), aynı stilde 1-2 karelik boşluk (titreme); CLI `check --video`
- [x] QC anime uyumu: tipografi (\pos/\move/\clip/\org/çizim), karaoke (\k) ve fx satırları okuma hızı/uzunluk/süre/çakışma uyarısı almaz; çakışma artık stil+katman başına ZAMAN sırasıyla (dosya sırası değil) denetlenir
- [x] Subtitle Edit toplu işlemleri (core::batch; Zamanlama → Toplu satır işlemleri…, CLI `fix --merge-same/--merge-short/--recalc`): aynı metinli ardışık satırları birleştir, kısa satırları birleştir, süreleri okuma hızına göre yeniden hesapla (yalnızca uzat, sonraki satıra boşluk)
- [x] Toplu araçlar: kaydır, doğrusal senkron, fps dönüşümü, kareye oturt, sırala, yinelenen/boş sil, çakışma düzelt, Türkçe harf
- [x] Override etiketi ayrıştırıcı/yazıcı (gidiş-dönüş güvenli), başlangıç bloğu get/set, temizleyici
- [x] Harf dönüştür (core::casing; Düzen menüsü, kısayol komutları): BÜYÜK/küçük/Başlık/Cümle — Türkçe i/İ ı/I, etiketler/\N/çizimler korunur
- [x] Sık hataları düzelt (core::fixes): bozuk Türkçe karakter onarımı, boşluk/noktalama, diyalog çizgisi, cümle başı büyük harf (i/İ), işitme engelli notları, süre/boşluk
- [x] Etiket onarımı ve "Notları sil" (Kalite sekmesi, CLI `fixes --tags --notes`)
- [x] Etiket denetimi (core::taglint, QC'de "Etiket"/"Notlar" süzgeci, ızgarada kırmızı altçizgi): eşleşmeyen { }, bilinmeyen/yazım hatalı etiket (\blurr, \fsc100), geçersiz renk/saydamlık, \pos/\move/\fad/\fade/\clip argüman sayısı, satır süresini aşan \fad, satır bittikten sonra başlayan \t/\move, ters zamanlar, \t ile canlandırılamayan etiketler, iç içe \t, birden fazla \pos, bilinmeyen \r stili, karaoke hece toplamı ≠ satır süresi, metinde unutulmuş {not}
- [x] Etiket uygula (HYDRA benzeri; core::tagapply, Araçlar → Etiket uygula / yeniden hesapla): başlangıç bloğu, tüm bloklar, `\t(t1,t2,ivme)`, harf harf degrade (boşluk/\N atlanır, ara etiketler korunur), satırlar boyunca degrade; renk seçici; eşanlamlılar (\c=\1c, \fr=\frz)
- [x] Yeniden hesapla (Recalculator): sayısal etiketleri topla/çarp — satır içi bloklar ve `\t` içleri dahil, \pos/\move/\org için x/y ayrı, etiket yoksa stil değerinden
- [x] Etiketler sekmesi (etiket denetçisi): etkin satırın her bloğu/etiketi ayrı satırda; sayılar sürüklenir, renkler seçiciyle, `\t` için t1/t2/ivme + iç etiketler; etiket sil/ekle; ardışık değişiklikler tek geri-al adımı
- [x] Ekler: [Fonts]/[Graphics] gömme/çıkarma/silme (core::attach, ASS UUencode), "Kullanılan yazı tiplerini göm" (Araçlar → Ekler); ayrıştırıcı "[..]" gibi görünen veri satırlarını bölüm sanmıyor
- [x] ASS Extradata yapılandırılmış erişim (core::extradata; Lua `line.extra` okunur/yazılır, `{=id}` ön eki ayrıştırılır, dosya birleştirmede kimlikler yeniden eşlenir, panoya kimlik yazılmaz)
- [x] Biçimler: LRC (gelişmiş sözcük zamanlı → \kf), MicroDVD, SBV, TTML/DFXP okuma+yazma; TXT/CSV yazma; CLI toplu dönüştürme
- [x] Biçimler: SAMI (.smi okuma/yazma, çok dilli dosyada ilk sınıf), SubViewer 2 (.sub okuma; MicroDVD ile otomatik ayrım)
- [ ] Biçimler: EBU STL, iTT, Netflix TTML tam, PGS/VobSub OCR
- [x] Zamanlama: videodan anahtar kareler, kareye/anahtar kareye yapışma, son-işleyici (lead-in/out, bitişik satır, anahtar kare)
- [x] Timecode dosyaları v1/v2 (VFR, `core::vfr`): Video menüsünden aç/kaydet/kapat; kare adımı, kareye yapışma, `aegisub.frame_from_ms/ms_from_frame`, "Kareye oturt", CLI `--timecodes`
- [x] Kare doğruluğu: Aegisub START/END anlambilimi (iki kare arasının ortası) — F6/F7, kareye yapışma, anahtar kare yapışması, "Kareye oturt", son-işleyici ve Lua `frame_from_ms/ms_from_frame` santisaniye yuvarlamasına dayanıklı (eskiden 23.976'da bazı satırlar bir kare geç başlıyordu); önizleme playhead'deki kareyi gösterir
- [x] VFR: videodan otomatik algılama (ffprobe paket PTS; ms yuvarlaması toleranslı), videoda "kare N VFR" göstergesi; elle yüklenen timecode önceliklidir
- [x] Matroska timestamp v4 dosyaları (sırasız satırlar sıralanır)
- [x] Video ile (osisub-media::mux; Dosya → Video ile, CLI `tracks/extract/mux/burn`): gömülü altyazı izlerini listele/aç/kaydet (ASS kayıpsız), ekli yazı tiplerini çıkar, MKV'ye göm (dil/başlık/varsayılan, mevcut izler korunur, kullanılan yazı tipleri eklenir), videoya yak (x264, ilerleme + iptal, gömülü yazı tipleri fontsdir ile)
- [ ] Resim tabanlı altyazı (PGS/VobSub) OCR
- [x] Referans altyazı (zamana göre eşleşen satırlar editörde gösterilir)
- [x] Toplu dosya işlemleri (core::filebatch; Dosya menüsü; Subtitle Edit Batch convert): çok dosya/klasör → biçim dönüştür, kaydır, sık hatalar, yorumları sil, yeniden örnekle; çıktı klasörü/son ek, üzerine yazma koruması, arka planda günlüklü
- [x] Dosya birleştirme (core::merge; Dosya → Başka dosyayı birleştir…, CLI `merge`): stil çakışmasında "Ad (2)" + `\r` başvuruları, farklı çözünürlükte otomatik yeniden örnekleme, zaman kaydırma (OP/ED), gömülü yazı tipleri; toplu işlemlerden sonra etkin satır/seçim içeriğe göre korunur
- [x] Yazı tipi denetimi (kullanılan/eksik yazı tipleri, dosyaları klasöre topla)
- [x] Eksik glif denetimi: yazı tipinde bulunmayan kullanılan karakterler (Türkçe ğ ş İ ı, Japonca…) yazı tipi + stil başına; tıklayınca ilk satıra git; CLI `fonts` raporlar ve hata koduyla çıkar
- [x] Yazım denetimi: Hunspell uyumlu (spellbook), tr_TR indir/seç, öneriler, yoksay, kullanıcı sözlüğü; `FLAG num` bayrak-0 uyumluluğu (LibreOffice tr_TR ile doğrulandı, ~1 sn yükleme)
- [x] Çeviri sekmesi (büyük referans, Enter = kaydet+sonraki, sonraki boş satır), referans sütunu, uzun satır bölme
- [x] Terim sözlüğü denetimi (kaynak = hedef; ekli biçimler kabul; Çeviri sekmesinde satır uyarısı + toplu denetim)
- [x] Noktadan senkron, FPS dönüştürme, kare cinsinden kaydırma, anahtar kare dosyası, Aegisub proje bilgisi (Video File/Active Line), kodlama seçimi/tespiti
- [x] Çeviri belleği (TM): Çeviri sekmesinde bulanık öneriler (%60+, "Kullan"), Enter ile öğrenme, Araçlar → Çeviri belleği (belgeden ekle, TSV içe/dışa aktar); `<veri>/tm.tsv`
- [x] Makine çevirisi bağlayıcıları (osisub-media::mt; Araçlar → Makine çevirisi): DeepL, Google v2, LibreTranslate, Claude (claude-opus-5-5, yapılandırılmış JSON çıktı, effort low, sunucu tarafı fallback); etiketler korunur, referans altyazı kaynak olarak kullanılır; istek/yanıt çevrimdışı testli. CANLI DENENMEDİ (anahtar yok)
- [x] Çeviri sekmesinde "Makine çevirisi" düğmesi (etkin satır)
- [x] Yazım: editörde canlı hatalı sözcük vurgusu (kırmızı altçizgi), etiket/\N renklendirme
- [x] Yazım: ızgarada hata göstergesi (satır numarası kırmızı; sürüm başına önbellekli)
- [x] Yazım: anime uyumu — Japonca saygı ekleri (`Hinata-kun` → gövde denetlenir), sık Japonca sözcükler (senpai, itadakimasu, onii-chan…) hata sayılmaz; tireli bileşiklerde her parça denetlenir
- [x] Yazım: çok dilli — ek sözlük (Yazım sekmesi: seç / en_US indir / kaldır; ayarlarda kalıcı); sözcük herhangi bir sözlükte varsa doğru, öneriler birleşik (gerçek en_US ile testli: `OSISUB_EN_DICT`)
- [x] Karaoke hece modeli (core::karaoke) + GUI sekmesi: böl (|), birleştir, sürükle, ölçekle, harfe göre dağıt
- [x] Karaoke kipi dalga formunda: hece sınırları ve metinleri etkin satırda gösterilir, sınırlar sürüklenerek ayarlanır (toplam süre korunur)
- [x] Karaoke: oynatırken Boşluk ile tuşla zamanlama (gecikme ayarlı; çekirdek `apply_taps` testli)
- [x] Karaoke: otomatik Japonca hece (mora) ayırma — romaji (ki|t|to, shi|n|ji, kyo|u) ve kana (きょ|う, っ ayrı); şablonlayıcı: gerçek kara-templater yerleşik
- [x] Kanji zamanlayıcı (Aegisub Kanji Timer; core::kanjitimer, Araçlar menüsü): romaji `\k` satırının süreleri aynı zamandaki kanji/kana satırına; kana okunuştan (Hepburn/Kunrei, edatlar を/は/へ, っ, ー, küçük kana), kanji dizileri sonraki kanaya kadar otomatik gruplanır; gruplar `ki+mi|no` / `君|の` olarak düzenlenebilir; Uygula ve sonraki
- [ ] Karaoke: gecikmeyi ses saatiyle otomatik kalibre et; furigana düzeni

## Lua (Automation 4)
- [x] subs nesnesi, register_macro/filter, dialog.display, re, unicode, lfs, karaskel, utils
- [x] Gerçek yazı tipi ölçüleri (`text_extents`: ttf-parser, GDI/VSFilter uyumlu hücre yüksekliği)
- [x] Yazı tipi dizini önbelleği (`<veri>/font-index.tsv`; yalnızca değişen dosyalar yeniden okunur, `OSISUB_NO_FONT_CACHE=1` kapatır)
- [x] `kara-templater.lua` yerleşik ve gerçek Aegisub `karaskel-auto4.lua` ile testli
- [x] MoonScript (`.moon`) betikleri, include klasörlerinden `require`
- [ ] Aegisub'ın diğer gelen betiklerinin (strip-tags, select-overlaps, auto-leadin…) otomatik testi — bu gece indirilen betikleri çalıştırmak otomatik mod güvenlik denetimince engellendi; betikler depoya kullanıcı onayıyla eklenmeli
- [x] DependencyControl uyumluluk katmanı (requireModules, registerMacro(s), ConfigHandler, Logger, sürüm denetimi; güncelleme yapmaz) + yedek `json` modülü; Aegisub'ın automation/include ve autoload klasörleri otomatik; `/` ile iç içe makro menüleri
- [x] ffi DLL karşılıkları: `BM.BadMutex`, `PT.PreciseTimer` (yerleşik, DepCtrl sürümlü)
- [ ] `SubInspector.Inspector` karşılığı (libass satır sınırları); gerçek ASSFoundation / Aegisub-Motion ile uçtan uca doğrulama
- [x] Dışa aktarım filtreleri (register_filter): Dosya → Dışa aktar (öncelik sırası, belge değişmez); CLI: `osisub run girdi builtin:kara-templater.lua "Karaoke template" çıktı`
- [x] `aegisub.frame_from_ms/ms_from_frame/video_size/get_audio_selection/project_properties` GUI'den (video+etkin satır)
- [x] `aegisub.get_frame` (arch1t3cht API: width/height/getPixel/getPixelFormatted; altyazılı kare)
- [x] Betikleri yeniden yükle (motoru sıfırlar)
- [x] Betik hata penceresi: hata iletisi + hatalı kaynak satırı (±3), yığın izi, kopyala (Windows yollarındaki boşluklar dahil)
- [x] MoonScript hatalarında .moon satırına eşleme (yükleme ve çalıştırma hataları; hata penceresi .moon kaynağını gösterir)
- [ ] Kullanıcının Lua betikleriyle regresyon test paketi (kullanıcıdan betikler istenmeli)

## Arayüz (GUI)
- [x] Çerçeve: egui/eframe 0.37 (osisub-gui). Başsız ekran görüntüsü: `xvfb-run osisub-gui dosya --screenshot out.png`
- [x] İskelet: menü, ızgara (sanal kaydırma), düzenleme paneli, stil/kalite/otomasyon/bilgi sekmeleri, Lua diyalog köprüsü, geri al/yinele
- [x] Izgara sütunları: Katman, Efekt, Kenar boşlukları (Görünüm → Izgara sütunları; kalıcı); yorum satırlarında KSS gösterilmez
- [x] Pencere boyutu/konumu ve ızgara sütun genişlikleri kalıcı (`<veri>/ui-state.ron`; test kipinde kapalı)
- [ ] Izgara: sütun sırası
- [x] Düzenleme kutusu araç çubuğu (Aegisub gibi): Kalın/İtalik/Altı/Üstü çizili (seçimi sarar, imleçte çevirir, stil varsayılanını bilir), birincil/kenar/gölge rengi (seçimde önceki renk geri yüklenir), \N; ardışık düğmeler aynı seçime uygulanır; etiket renklendirme, KSS sayacı, stil seçici
- [x] Video önizleme: ffmpeg + libass ile gerçek ASS render'ı (kare başına süreç; yavaş ama doğru)
- [x] Ses dalga formu: kaydır/yakınlaştır, satır bölgeleri, başlangıç/bitiş sürükle (kareye yapışmalı), sağ tık = bitiş, Shift+tık = başlangıç
- [x] Oynatma sırasında akış (kalıcı ffmpeg boru hattı) — önceki geceden; kare önbelleği + 6 karelik ön-yükleme (ileri kare adımı anında)
- [x] Geri kare adımı için ön-yükleme (istenen karenin 3 öncesi + 5 sonrası tek ffmpeg çağrısında)
- [x] Anamorfik videoda PlayRes en-boy QC'si görüntü oranını da kabul eder
- [x] Anamorfik video (SAR ≠ 1, ör. DVD 720×480 → 16:9) doğru en-boy oranıyla gösterilir; Video → En-boy oranı (Otomatik/4:3/16:9/1.85/2.35; Aegisub Override AR)
- [x] Videoyu ayrı pencerede göster (Video menüsü; çift monitör, Aegisub "Detach video"); video araçları ayrı pencerede de çalışır
- [ ] libmpv ile donanım hızlandırmalı oynatma
- [x] Spektrogram görünümü (rustfft), anahtar kare işaretleri, video+ses satır oynatma (F5; ses harici ffplay)
- [x] Uygulama içi ses oynatma (cpal 0.18 + ffmpeg PCM; ses saati playhead/video/tuşla zamanlamayı sürer; aygıt yoksa ffplay'e düşer). HENÜZ GERÇEK AYGITTA TEST EDİLMEDİ (kum havuzunda ses aygıtı yok); mantık sahte çıkışla testli
- [x] Playhead'den sürekli oynatma (Shift+F5, Video menüsü, kısayol komutu)
- [x] Oynatma hızı (0.25–2x, perde korunur; ffmpeg atempo) ve ses seviyesi (Video menüsü, kalıcı); ses saati hızla ölçekli
- [ ] Basılı tutarak oynat
- [ ] ffmpeg olmadan çalışma (symphonia ile ses çözme)
- [x] Sesle yeniden zamanlama (TV → BD; osisub-media::resync, Zamanlama menüsü, CLI `resync`): satır grupları eski seste → yeni seste FFT çapraz-ilinti; eklenen/çıkarılan sahne ve gecikme; güvensiz eşleşmede önceki kayma (günlükte raporlanır)
- [x] Konuşmaya yapıştır (Zamanlama → Sesten satır oluştur penceresi): başlangıç/bitiş ±250 ms içindeki konuşma sınırına
- [x] Ses zamanlama kısayolları (Aegisub varsayılanları S/H/Q/W/E/D/Z/X; Aegisub hotkey.json eşlemesi; yeniden atanabilir)
- [x] Stil önizlemesi (libass, Türkçe örnek metin, düzenlenebilir)
- [x] Stil yöneticisi: yeniden adlandırma `\r` başvurularını da günceller; "Kullanılmayanları sil"
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
- [x] Hareket izleme: köşe pini (Mocha "CC Power Pin" / AE "Corner Pin" verisi; core::motion::apply_corner_pin): satır kutusu her karede dörtgene perspektifle oturtulur, \clip homografiyle taşınır; GUI aynı pencere (otomatik algılama) + CLI `motion`; libass render'ıyla IoU ≥ 0.999
- [x] Hareket izleme: doğrusal kip (\move + \t, satır bölünmez; GUI onay kutusu, CLI `--linear`)
- [x] Tipografi yardımcıları (core::typeset; Araçlar → Degrade / parıltı…): kırpma şeritleriyle degrade (renk/saydamlık/sayısal etiketler, yatay/dikey, ivme), parıltı katmanı
- [x] Metin → çizim (\p1) dönüştürme (gerçek yazı tipi konturları; hizalama, çok satır, \fscx/\fscy/\fsp, döndürmede \org korunur; libass çıktısıyla piksel karşılaştırması IoU ≥ 0.97, sınır kutuları birebir) + vektör kırpma olarak kopyalama; CLI `to-drawing`, `shape`
- [x] Perspektife oturt (Araçlar menüsü / kısayol komutu / CLI `perspective`): 4 noktalı vektör kırpma → \frx\fry\frz\fscx\fscy\fax\org\pos; libass dönüşüm zinciriyle Levenberg–Marquardt; video açıksa `LayoutResX/Y` yazılır (perspektif oynatıcı çözünürlüğünden bağımsız); libass render'ıyla IoU ≥ 0.999
- [x] Perspektif video aracı (Video aracı: Perspektif; kısayol komutu `tool_persp`, Aegisub `video/tool/perspective`): 4 köşe tutamacı + içeriden taşıma, her sürüklemede canlı oturtma; dörtgen satırın perspektif etiketlerinden geri hesaplanır (yoksa 4 noktalı kırpma ya da konum çevresinde kutu); LayoutRes otomatik eklenir; sağ tık = perspektifi kaldır (xdotool ile doğrulandı)
- [x] Video aracı: \p çizim noktalarını düzenleme (\an7, döndürmesiz; ölçek ve \p düzeyi hesaba katılır; ekle/taşı/sil)
- [x] Vektör kırpma ve çizim araçlarında Ctrl+tık: düz parçayı kübik eğriye (b) çevir (core::drawing::line_to_bezier)
- [ ] Video araçları: döndürülmüş/\an≠7 çizimler, \fax/\fay
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
- [x] Stil yardımcısı (Aegisub Styling Assistant; Düzen menüsü): satır metni büyük, stil süzgeci, Enter = uygula + sonraki satır (medya açıksa oynatır), PgUp/PgDn
- [x] Önceki/sonraki anahtar kareye git (Video menüsü, kısayol komutları, Aegisub `video/frame/next/keyframe`); güvenli alan kılavuzu (%90/%80)
- [x] Üzerine yapıştır (Aegisub Paste Over, Ctrl+Shift+V / Düzen menüsü): panodaki satırların seçilen alanları (zaman, stil, konuşan, kenar, efekt, metin) seçili satırlara; sistem panosundan okunur
- [x] Lua `clipboard.get/set` GUI'de gerçek sistem panosuna bağlı (arboard; eskiden yalnızca betik içi bellekti)
- [x] Geri al/yinele, otomatik kayıt, kalıcı ayarlar, son açılanlar, sürükle-bırak, sağ tık menüsü, kopyala/kes/yapıştır
- [x] Çökme sonrası otomatik kayıttan kurtarma, çökme günlüğü, kaydedilmemiş değişiklik onayı
- [x] Geçmiş penceresi (Düzen → Geçmiş…): geri alma listesi, tıklayınca o duruma git (geri al/yinele)
- [x] Komut paleti (Ctrl+Shift+P; komutlar, pencereler, sekmeler, Lua makroları; Türkçe duyarsız bulanık arama)
- [ ] Türkçe + İngilizce arayüz, açık/koyu tema

## Performans
- [x] Büyük dosya ölçümleri: çekirdek `cargo test --release -p osisub-core --test perf -- --ignored --nocapture` (100k satır: QC 0.31 sn, yazma 0.07 sn, okuma 0.12 sn); arayüz `OSISUB_FRAME_LOG=1` ile kare süresi — 50k satırlık tipografi dosyasında (release) medyan 2.1 ms, yazarken en kötü 11 ms, kaydırma/gezinme ≤ 7 ms (2026-11-12)
- [x] Geri alma belleği: satır sayısını değiştiren işlemler artık yalnızca değişen aralığı saklar (ortak önek/sonek); toplu ekleme/silme splice ile O(n); >5000 girdilik adımlar en fazla 8 tane tutulur
- [ ] Bellek ayak izi ölçümü
- [ ] Lua çağrıları için toplu (batch) API ile satır kopyası azaltma

## Dağıtım
- [x] GitHub Actions: Linux/Windows/macOS derleme + test + ui-e2e (.github/workflows/ci.yml; her push'ta yeşil)
- [ ] Sürüm etiketleri, değişiklik günlüğü
