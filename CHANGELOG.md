# Değişiklik günlüğü

## 0.5.0 — geliştiriliyor (dördüncü gece, 2026-11-12)

**Tipografi:** köşe pini izleme (Mocha "CC Power Pin" / AE "Corner Pin" → kare kare perspektif; kırpma homografiyle taşınır; GUI Hareket izleme penceresi veriyi otomatik tanır, CLI `motion`); hareket izlemede doğrusal kip (`\move` + `\t`, tek satır); **çizim video aracı** (`\p` noktalarını videoda düzenle); **perspektif video aracı** — 4 köşeyi sürükle, metin canlı olarak dörtgene oturur (Aegisub'ın perspektif aracı gibi; LayoutRes otomatik); **Etiket uygula / yeniden hesapla** (HYDRA + Recalculator benzeri): etiketleri başlangıca, tüm bloklara, `\t` olarak, harf harf ya da satırlar boyunca degrade olarak uygula; sayısal değerleri (`\t` içleri ve `\pos/\move/\org` dahil) topla/çarp.
**Kalite:** ekip için HTML QC raporu (Kalite sekmesi, CLI `check --report`); eksik glif denetimi — yazı tipinde bulunmayan karakterler (ör. Türkçe ğ ş İ ı'sı olmayan tabela yazı tipleri) Yazı tipi denetimi penceresinde ve CLI `fonts`'ta.
**Performans:** satır ekleyen/silen işlemler (hareket izleme, birleştirme, bölme…) geri alma yığınında tüm belgenin iki kopyası yerine yalnızca değişen satırları tutar — büyük dosyalarda bellek ve geri alma geçmişi korunur (eskiden 8 adımdan eskisi atılıyordu); `OSISUB_FRAME_LOG=1` kare süresi günlüğü.
**Aegisub'dan geçiş:** Üzerine yapıştır (Paste Over, Ctrl+Shift+V) — panodaki satırların seçilen alanlarını seçili satırlara yazar; Stil yardımcısı (Styling Assistant); önceki/sonraki anahtar kareye git; videoda güvenli alan kılavuzu.
**Lua:** `BM.BadMutex` ve `PT.PreciseTimer` yerleşik karşılıkları (ASSFoundation / Aegisub-Motion bağımlılıkları; gerçekleri Windows/Linux'a özel DLL ister).
**Dağıtım:** gece sürüm paketleri (Linux/Windows/macOS; `[nightly]` commit'lerinde otomatik); Windows paketi ffmpeg/ffprobe/ffplay'i yanında getirir (program önce kendi klasörüne bakar).
**Dosyalar:** Toplu dosya işlemleri penceresi — birçok bölümü tek seferde dönüştür / kaydır / düzelt / yeniden örnekle.
**Video:** anamorfik kaynaklar (DVD) doğru en-boy oranında; Video → En-boy oranı geçersiz kılma (Aegisub Override AR).
**Yazım:** ek sözlük (ör. tr_TR + en_US; İngilizce şarkı sözleri/terimler yanlış alarm vermez); Japonca saygı ekleri ve sık anime sözcükleri artık hata sayılmıyor (`Hinata-kun`'da yalnızca ad denetlenir); tireli bileşikler parça parça denetlenir.
**Düzenleme:** Harf dönüştür (BÜYÜK / küçük / Başlık / Cümle; Türkçe i/İ ve ı/I doğru, etiketler ve `\N` korunur).
**Karaoke:** Kanji zamanlayıcı — romaji karaoke zamanlamasını kanji/kana satırına otomatik aktarır (kana okunuşlarından hizalama, kanji dizileri gruplanır; gruplar elle düzeltilebilir).
**Hata düzeltmeleri:** Lua `clipboard.get/set` arayüzde sistem panosunu kullanmıyordu (panodan veri alan Aegisub betikleri — ör. hareket verisi, HYDRA kopyala/yapıştır — boş okuyordu).
**Arayüz:** Etiketler sekmesi — etkin satırın etiketlerini blok blok gösterir; değerler sürüklenerek/renk seçiciyle değiştirilir, `\t` zamanları ve ivmesi ayrı alanlarda.

## 0.4.0 — 2026-11-11 (üçüncü gece)

**Arayüz:** komut paleti (Ctrl+Shift+P), geçmiş penceresi, düzenleme kutusu araç çubuğu (K/İ/A/Ü, renk), ayrı video penceresi, kalıcı pencere/sütun düzeni, başlangıç ekranı.
**Lua:** `aegisub.get_frame`.
**Aegisub'dan geçiş:** `l0.DependencyControl` uyumluluk katmanı (requireModules, registerMacro(s), ConfigHandler, Logger, sürüm denetimi; güncelleme indirmez) + yedek `json` modülü; kurulu Aegisub'ın `automation/include` ve `autoload` klasörleri otomatik; `/` ile iç içe makro menüleri; Aegisub `hotkey.json` içe aktarma; belgeye bağlı betikler ("Automation Scripts"); Aegisub ses kipi komutları ve varsayılan S/H/Q/W/E/D/Z/X tuşları.
**Tipografi:** perspektife oturt (4 noktalı kırpma → 3B dönüş etiketleri; libass ile doğrulandı); metin → çizim (`\p1`, gerçek yazı tipi konturları) ve vektör kırpma olarak kopyalama; hareket izleme verisi uygulama (Mocha/AE Transform Data → kare kare `\pos`/ölçek/dönüş/`\clip`; GUI + CLI `motion`); kırpma şeritleriyle degrade; parıltı (glow) katmanı.
**Kalite:** betik başlığı denetimi (PlayRes, ScaledBorderAndShadow, YCbCr Matrix); anime etiket denetimi (bozuk/bilinmeyen etiket, süreyle çelişen `\fad`/`\move`/`\t`, karaoke süresi, unutulmuş `{notlar}`), Kalite sekmesinde kategori süzgeci, "Etiketleri onar", "Notları sil", CLI `fixes --tags --notes`.
**Video ile:** MKV/MP4'teki altyazı izlerini aç/çıkar, ekli yazı tiplerini çıkar, MKV'ye göm (yazı tipleriyle), videoya yak (ilerlemeli); CLI `tracks/extract/mux/burn`.
**Çeviri:** makine çevirisi (DeepL, Google, LibreTranslate, Claude — kendi API anahtarınızla; etiketler korunur, referans altyazı kaynak alınır).
**Dosyalar:** dosya birleştirme (diyalog + tabela + OP/ED; stil çakışması, çözünürlük, zaman kaydırma, yazı tipleri; GUI + CLI `merge`); SAMI okuma/yazma, SubViewer 2 okuma, Matroska timestamp v4.
**Zamanlama:** sesle yeniden zamanlama (TV → BD: FFT çapraz-ilintiyle satır grupları; GUI + CLI `resync`); konuşma sınırlarına yapıştırma; oynatma hızı (0.25–2×, perde korunur) ve ses seviyesi; Subtitle Edit toplu işlemleri (aynı metinli/kısa satırları birleştir, okuma hızına göre süre).
**Hata düzeltmeleri:** ikinci bir dosya açıldığında ızgara/QC önbellekleri yenilenmiyordu (iki belgenin sürüm sayacı 0'dan başlıyordu → boş ya da eski ızgara); Lua `text_extents` harf aralığını `\fscx` ile ölçeklemiyordu (karaskel düzenleri `\fsp`+`\fscx`'li stillerde kayıyordu); çizim (`\p1`) satırlarının koordinatları okuma hızı/uzunluk sayılıyordu; satır sayısını değiştiren toplu işlemlerden sonra etkin satır/seçim yanlış satıra kayıyordu; "yalnızca uzat" süre hesabı çakışan satırı kısaltıyordu (yayın öncesi yakalandı); `sort_by_start` büyük dosyada karesel zamanlıydı.

## 0.3.0 — 2026-11-10 (ikinci gece)

**Lua uyumluluğu:** gerçek Aegisub `karaskel-auto4.lua` gömülü; **`kara-templater.lua` yerleşik** (gerçek betikle uçtan uca test); **MoonScript (`.moon`) betikleri** (gömülü moonscript 0.5.0 + LuLPeg); include klasörlerinden `require` (.lua/.moon); yeni API'ler: `aegisub.debug.out`, `aegisub.parse_karaoke_data`, `subs.script_resolution`, `unicode.charwidth/chars`, `utils.lua` genel takma adları, `cleantags.lua`; Aegisub izleme düzeyi (debug iletileri gizli); betik hata penceresi (hatalı satır + çevresi).
**Zamanlama:** VFR — timecode v1/v2 dosyaları (aç/kaydet/kapat) ve videodan **otomatik VFR algılama**; kare adımı, kareye yapışma, Lua kare dönüşümleri ve "Kareye oturt" bunları kullanır; CLI `--timecodes`.
**Tipografi:** `\move` video aracı (başlangıç/bitiş, birlikte taşıma, `\pos`a çevirme), döndürme aracında `\org` sürükleme.
**Karaoke:** Japonca otomatik hece (mora) ayırma — romaji ve kana.
**Çeviri:** çeviri belleği (bulanık öneri, Enter ile öğrenme, belgeden toplu ekleme, TSV içe/dışa aktarma).
**Arayüz:** Lua makrolarına kısayol atama; Düzen → Satırları seç (Aegisub Select Lines); ızgarada Katman/Efekt/Kenar boşluğu sütunları; elle yüklenen betikler kalıcı + Kaldır.
**Kare doğruluğu:** Aegisub START/END anlambilimi (F6/F7, yapışma, anahtar kare, kareye oturt, Lua kare dönüşümleri); satır seçince ilk görünen kareye gidilir; kare kipinde kaydırma satır başına gerçek kare kaydırması; "Video karesinde böl", "Çoğalt ve 1 kare kaydır", imleçte böl.
**Kalite (anime):** tipografi/karaoke/fx satırları yanlış alarm vermez; çakışma stil+katman başına zaman sırasıyla; sahne değişimi yakınlığı ve titreme denetimi + "Sahne değişimlerine yapıştır"; CLI `check --video`; sürüm karşılaştırma (GUI + CLI `compare`).
**Tipografi:** vektör `\clip`, 3B döndürme (`\frx/\fry`), video yakınlaştırma/kaydırma; ekler (yazı tipi gömme/çıkarma, önizlemede `fontsdir`).
**Karaoke:** dalga formunda hece sınırlarını sürükleyerek zamanlama.
**Arayüz:** yerleşik komutlara kısayol atama; ızgarada yazım göstergesi; MoonScript hata satırları .moon kaynağına eşlenir.
**Performans:** kalıcı yazı tipi dizini önbelleği; kare adımı için 6 karelik ön-yükleme.
**Hata düzeltmeleri:** medya açıkken kaydetme Project Garbage satırları ekleyip etkin satırı/ızgarayı kaydırıyordu; video araçlarında tutamaç isabet testi yanlış noktayı kullanıyordu; kısayol yakalama tek başına Ctrl tuşunu kaydediyordu; CFR'de kare numarası hesabı (floor) bazı karelerde bir önceki kareyi veriyordu; tam kare zamanı santisaniyeye yuvarlanınca satırlar bir kare geç başlıyordu; 23.976 mp4'te kare adımı bir sonraki kareyi gösteriyordu; "[..]" görünümlü gömülü yazı tipi satırları bölüm başlığı sanılıyordu; çakışma denetimi/düzeltmesi dosya sırasına bakıyordu; sık hataları düzelt ve son-işleyici tabelaları değiştiriyordu.

## 0.2.0 — 2026-10-29 (ilk gece sürümü)

Sıfırdan yazıldı (Rust workspace).

**Çekirdek (`osisub-core`):** ASS/SSA bayt-kararlı okuma-yazma, SRT/VTT/LRC (gelişmiş)/MicroDVD/SBV/TTML, Windows-1254 çözme, etiket ayrıştırıcı, karaoke modeli, QC, "sık hataları düzelt" (Türkçe duyarlı: bozuk karakter onarımı, i/İ), bul/değiştir, zamanlama son-işleyicisi, yeniden örnekleme, yazı tipi çıkarımı, istatistik.
**Lua (`osisub-lua`):** LuaJIT tabanlı Aegisub Automation 4 uyumlu motor (`subs`, `aegisub.*`, `re`, `unicode`, `utf8`, `lfs`, `karaskel`, `utils`, filtreler, iletişim kutuları, gerçek yazı tipi ölçüleri).
**Medya (`osisub-media`):** ffmpeg ile ses dalga formu/spektrum, libass'lı video önizleme ve oynatma, anahtar kareler, konuşma tespiti, uygulama içi ses (cpal).
**Arayüz (`osisub-gui`):** ızgara, düzenleme paneli, dalga formu zamanlama, video araçları (konum/kırpma/döndürme/ölçek), karaoke ve tuşla zamanlama, stil düzenleyici, çeviri sekmesi, yazım denetimi (Hunspell/tr_TR), referans altyazı, Lua makroları, geri al/yinele, otomatik kayıt + kurtarma.
**CLI (`osisub`):** convert, check, fix, fixes, shift, post, resample, find, fonts, batch, macros, run, info.

Bilinen sınırlar: `docs/ROADMAP.md`. Doğrulanmayanlar: gerçek ses aygıtı, macOS, CI'nin gerçek çalıştırması.
