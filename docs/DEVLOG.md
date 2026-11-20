# Geliştirme günlüğü

Her gece çalışması (gece–06:00) buraya bir kayıt ekler. Yeni oturum önce bu dosyayı ve `docs/ROADMAP.md`'yi okumalı, kaldığı yerden devam etmeli. Sonuç o gecenin görevinin verdiği dala push edilir (`tools/ship.sh` geçerli dalı kullanır; 2026-10-29: `claude/vibrant-clarke-2e8jtn`, 2026-11-10: `claude/vibrant-clarke-6oujog`, 2026-11-11: `claude/vibrant-clarke-dsxf2y`, 2026-11-12: `claude/vibrant-clarke-xt32gt`); PR yalnızca kullanıcı isterse açılır.

## 2026-10-29 (v0.1.0 — ilk gece)
Depo boştu; sıfırdan başlandı.
- `osisub-core`, `osisub-lua`, `osisub-cli` yazıldı; tüm testler geçiyor (core 12, lua 3).
- Aegisub Lua uyumluluğu için LuaJIT (`mlua`, vendored) seçildi: ffi/bit yerleşik, Aegisub ile aynı 5.1 anlambilimi.
- Karar: GUI için egui/eframe önerildi, henüz başlanmadı.
- Not: Kullanıcının kendi Lua betikleri depoda yok; regresyon testi için betikler gerekli (kullanıcıdan istenmeli).
- Not: "Bugüne kadar altyazıda karşılaştığımız sorunlar" geçmişi bu oturumda erişilebilir değildi; depoda kayıt yok. Sorunlar tanımlandıkça `docs/ROADMAP.md`'ye eklenmeli.
- Sıradaki: gerçek yazı tipi ölçüsü (`text_extents`), override etiket ayrıştırıcı, GUI iskeleti.
- (aynı gece) Gerçek yazı tipi ölçümü, `osisub-doc` (Document: seçim, geri al/yinele, birleşen yazma adımları, böl/birleştir/çoğalt/sil, makro sonucu tek geri-al adımı), Lua arka plan çalışanı (iletişim kutuları arayüze iletilir), override etiket ayrıştırıcı, `osisub-gui` iskeleti.
- Ölçüm: 100k satırlık ASS ayrıştırma ~0.17 sn; karaskel ile 100k satır Lua işleme ~2.8 sn (release).
- (aynı gece) `osisub-media`: ffprobe/ffmpeg ile bilgi, dalga formu, libass'lı kare; GUI'de video + dalga formu paneli. Ekran görüntüleri: `docs/img/`. Başsız doğrulama için `libxkbcommon-x11-0` kurulmalı (`apt-get install`), sonra `LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a osisub-gui dosya --media video.mkv --screenshot cikti.png`.
- (aynı gece) Bul/Değiştir (`core::search`), zamanlama son-işleyicisi + çözünürlük yeniden örnekleme (`core::timing`), GUI'de pencereler, anahtar kareler, F5/F6/F7/Alt+↑↓ kısayolları, otomatik kayıt (~/.local/share/osisub/autosave), CI iş akışı.
- (aynı gece) Karaoke çekirdeği ve GUI sekmesi, `--line N`/`--open pencere` ekran görüntüsü bayrakları, Lua `utf8`/`clipboard`/auto4 takma adları ve geniş uyumluluk testi (compat.lua).
- (aynı gece) Yazı tipi denetimi (`core::fonts_used`), sağ tık menüsü + pano (Dialogue satırları ve düz metin), sürükle-bırak, kalıcı ayarlar (settings.json), son açılanlar, referans altyazı.
- (aynı gece) Sık hataları düzelt, konum aracı. HATA DÜZELTİLDİ: önizleme karesi kaba `-ss` ofseti yüzünden yanlış altyazı satırını gösteriyordu (`-copyts` ile çözüldü, `osisub-media` testi eklendi).
- (aynı gece) Yeni biçimler (`core::formats`), CLI: fixes/resample/post/find/fonts/batch. Bayt-kararlı gidiş-dönüş testi iki gerçek hata buldu ve düzeltildi: küçük harfli bölüm adları (`[script info]`) ve SSA ondalık renkler.
- (aynı gece) `osisub-spell` + GUI 'Yazım' sekmesi. LibreOffice tr_TR sözlüğü (36 MB .dic, `FLAG num`, bayrak 0) spellbook ile çalışır hale getirildi (bayrakları +1 kaydırma). Sözlük depoya KONMADI (lisans/boyut); `OSISUB_TR_DICT=<klasör> cargo test -p osisub-spell` gerçek sözlük testini çalıştırır. VAD, oynatma akışı, ayarlar da eklendi.
- (aynı gece) Uygulama içi ses motoru (`osisub-media::audio`, özellik `audio`; Linux'ta `libasound2-dev` gerekir), oynatma saati ses saatine bağlandı. DİKKAT: gerçek ses aygıtında henüz denenmedi — ilk gerçek makinede F5 ile doğrulayın; sorun olursa ffplay yedeği otomatik devreye girer. Ayrıca: video araçları (clip/frz/fsc), tuşla karaoke zamanlama, referans sütunu, Görünüm menüsü (açık tema, yakınlaştırma), Zamana git, .bak yedek.
- (aynı gece) `tools/ui-smoke.sh`: etkileşimli duman testi. Bulunan/giderilen hatalar: Zamana-git penceresi her karede odağı çalıyordu, Bul penceresinde otomatik odak yoktu, Esc pencereleri kapatmıyordu. Ayrıca: dışa aktarma filtreleri, istatistikler, platform yolları (`core::paths`), önizleme optimizasyonu (küçültülmüş ASS), worker'ı beklemeden bırakma.
- (aynı gece) Windows çapraz derleme doğrulandı: `apt-get install gcc-mingw-w64-x86-64; rustup target add x86_64-pc-windows-gnu; cargo build -p osisub-gui --target x86_64-pc-windows-gnu` (LuaJIT, cpal/WASAPI, eframe, rfd dahil derlenip BAĞLANDI; çalıştırılamadı — wine yok). `.cargo/config.toml` yalnızca bu hedef için bağlayıcıyı ayarlar. macOS doğrulanmadı.

### Gece sonu durumu (2026-10-29, v0.2.0)
- Tüm testler yeşil (`cargo test --workspace --exclude osisub-gui`), `tools/ui-e2e.sh` yeşil (düzenle/kaydet/geri al/yinele/sil/Lua makrosu/.bak), `tools/ui-smoke.sh` (+MEDIA_TEST) çökmesiz.
- Fuzz/dayanıklılık testleri bulup düzeltti: diyalog sonu boşluk kaybı, kapanmamış `{` ile ASS→SRT paniği, küçük harfli bölüm adı, SSA ondalık renk.
- Doğrulanmayanlar: gerçek ses aygıtı, macOS, Windows'ta çalıştırma, CI'nin gerçek çalıştırması.
- Depo: dal `claude/vibrant-clarke-2e8jtn`; PR açılmadı (istenmedi). Kullanım: `docs/KULLANIM.md`, proje notları: `CLAUDE.md`.

## 2026-11-10 (v0.3.0 — ikinci gece)
Dal: `claude/vibrant-clarke-6oujog` (main'den; `claude/vibrant-clarke-2e8jtn` ileri sarılarak devam edildi). Not: depoda önceki gecelerden kalan ~25 paralel deneme dalı var (OsiSub, Kitsune, Yoru, Yuzu, Kaku…); en olgunu osisub (2e8jtn) seçildi, diğerleri yalnızca parça kaynağı (ör. kara-templater) olarak kullanıldı.
- Lua: gerçek karaskel-auto4 + yerleşik kara-templater (`builtin:` yolu), MoonScript, include'dan require, eksik API'ler. Kara-templater çalıştırınca bulunan eksikler: `aegisub.debug` yoktu, `table.copy` yoktu, `parse_karaoke_data`, `subs.script_resolution`, `unicode.charwidth` yoktu.
- DİKKAT (ortam): otomatik mod güvenlik denetimi internetten yeni indirilen Aegisub betiklerini (strip-tags, select-overlaps.moon…) çalıştırmayı engelledi; bu betiklerle uyumluluk testi için kullanıcı onayı ya da betiklerin depoya kullanıcı tarafından eklenmesi gerekir.
- VFR (timecode + otomatik), `\move`/`\org` araçları, Japonca hece ayırma, çeviri belleği, makro kısayolları, Satırları seç, ızgara sütunları, kalıcı betik listesi, yazı tipi dizini önbelleği.
- HATA DÜZELTİLDİ: medya açıkken kaydetme `[Aegisub Project Garbage]` satırlarını olaylardan önce ekleyip dizinleri kaydırıyordu → etkin satır kayboluyor, ızgara yanlış satırları gösteriyordu (xdotool testiyle bulundu).
- HATA DÜZELTİLDİ: egui `drag_started` eşik sonrası gelir; video araçları tutamaç seçimini basma noktası yerine kaymış konumla yapıyordu.
- `tools/ship.sh` artık GUI birim testlerini de çalıştırır ve geçerli dala push eder.
- (aynı gece, devam) **Kare doğruluğu düzeltmeleri** (anime için kritik): (1) CFR `frame_at` `time_at`'in tersi değildi (floor) → 24/23.976 fps'de bazı karelerde bir önceki kare seçiliyordu; (2) tam kare zamanı santisaniyeye yuvarlanınca satır bir kare geç başlıyordu → Aegisub START/END (iki kare arası orta nokta) anlambilimi F6/F7, yapışma, anahtar kare, kareye oturt, son-işleyici ve Lua'ya uygulandı; (3) 23.976 mp4'te kare adımı bir sonraki kareyi gösteriyordu (yarım ms erken arama). Hepsi özellik/gidiş-dönüş testleriyle korunuyor.
- QC anime uyumu: tipografi/karaoke/fx satırlarına okuma hızı/uzunluk/süre uyarısı yok; çakışma stil+katman başına zaman sırasıyla (eskiden dosya sırasıyla ve yalnızca bir önceki satırla — araya giren tabela gerçek çakışmayı gizliyordu). Aynı mantık çakışma düzeltme, sık hataları düzelt (süre zinciri) ve son-işleyiciye uygulandı.
- QC kare denetimleri (sahne değişimine yakınlık, titreme) + "Sahne değişimlerine yapıştır"; CLI `check --video`.
- Video araçları: vektör `\clip`, 3B döndürme (`\frx/\fry`); kare adımı için 6 karelik ön-yükleme; satır seçince ilk görünen kareye gidiliyor.
- Ekler (yazı tipi gömme) + HATA: "[..]" görünümlü UUencode satırları bölüm başlığı sanılıyordu. Önizlemede gömülü yazı tipleri `fontsdir` ile.
- Windows çapraz derleme yeniden doğrulandı (osisub.exe + osisub-gui.exe bağlandı; çalıştırılamadı).


## 2026-11-11 (v0.4.0 — üçüncü gece)
Dal: `claude/vibrant-clarke-dsxf2y` (main'den; `claude/vibrant-clarke-6oujog` ileri sarılarak devam edildi). Ortam: `libasound2-dev xvfb xdotool libxkbcommon-x11-0` yeniden kuruldu.
- **DependencyControl uyumluluk katmanı** (`l0.DependencyControl` gömülü; gerçek DepCtrl'ün yerine geçer): requireModules/registerMacro(s)/ConfigHandler/Logger/sürüm denetimi; yedek `json` modülü. Kurulu Aegisub'ın `automation/include` + `autoload` klasörleri otomatik. Makro adlarındaki `/` iç içe menü.
- Aegisub `hotkey.json` içe aktarma; Aegisub ses kipi komutları + varsayılan S/H/Q/W/E/D/Z/X tuşları.
- **Etiket denetimi** (core::taglint): QC'de Etiket/Notlar süzgeci; Etiketleri onar / Notları sil; CLI `fixes --tags --notes`.
- Belgeye bağlı betikler (Aegisub "Automation Scripts").
- Ses: oynatma hızı (atempo, perde korunur) + ses seviyesi.
- Biçimler: SAMI, SubViewer 2, timestamp v4.
- **Dosya birleştirme** (core::merge, GUI + CLI `merge`).
- HATA DÜZELTİLDİ: satır sayısını değiştiren toplu işlemlerden sonra etkin satır/seçim yanlış satırı (ya da stil satırını) gösteriyordu → içerik eşlemesiyle korunuyor.
- Not: GUI testinden önce `cargo build -p osisub-gui` şart (`cargo test` ikiliyi güncellemez).
- (aynı gece, devam) **Tipografi:** hareket izleme (Mocha/AE verisi; konum/ölçek/dönüş/\clip), metin → çizim (gerçek konturlar), **perspektife oturt** (4 noktalı kırpma), degrade, parıltı. Hepsi libass render'ıyla piksel düzeyinde doğrulandı (IoU ≥ 0.97–0.999).
- libass bulguları (kaynaktan doğrulandı): perspektif kamera uzaklığı 20000/64 = **312.5 render pikseli** (betikte 312.5×PlayResY/LayoutResY; LayoutRes yoksa oynatıcı çözünürlüğüne bağlı!), `\fax` eğmesi `fax×fscx/fscy` ile ölçekli uzayda uygulanır, `\fsp` harf aralığı `\fscx` ile ölçeklenir.
- HATA DÜZELTİLDİ: Lua `text_extents` harf aralığını `\fscx` ile ölçeklemiyordu (Aegisub/libass ölçekler) → karaskel düzenleri kayıyordu.
- HATA DÜZELTİLDİ: ikinci dosyayı açınca ızgara/QC önbelleği yenilenmiyordu (her belgenin `rev`'i 0'dan başlıyordu).
- **Video ile:** MKV izleri aç/çıkar, ek yazı tipleri, MKV'ye göm, videoya yak. **Sesle yeniden zamanlama (TV→BD)**: FFT çapraz-ilinti.
- **Windows doğrulandı (wine):** `tools/win-check.sh` — CLI çıktısı Linux'la bayt bayt aynı, GUI açılıyor. CI gerçek Windows/macOS runner'larında yeşil.
- Kullanım kolaylığı: komut paleti (Ctrl+Shift+P), geçmiş penceresi, düzenleme kutusu araç çubuğu (K/İ/A/Ü/renk), pencere/sütun kalıcılığı, betik başlığı QC'si.

## 2026-11-12 (v0.5.0 — dördüncü gece)
Dal: `claude/vibrant-clarke-xt32gt` (main'den; `claude/vibrant-clarke-dsxf2y` ileri sarılarak devam edildi). Ortam: `apt-get update` sonrası `xvfb xdotool libxkbcommon-x11-0 ffmpeg libasound2-dev` kuruldu.
- **Tipografi:** köşe pini izleme (CC Power Pin/Corner Pin → kare kare perspektif, kırpma homografiyle; libass IoU ≥ 0.999), **perspektif video aracı** (4 köşe, canlı oturtma), **çizim (\p) video aracı**, hareket izlemede doğrusal kip, **Etiket uygula / yeniden hesapla** (HYDRA + Recalculator), **Etiketler sekmesi** (etiket denetçisi).
- **Aegisub'dan geçiş:** Üzerine yapıştır (Ctrl+Shift+V), Stil yardımcısı, **Kanji zamanlayıcı**, anahtar kareye atlama, güvenli alan kılavuzu, en-boy oranı geçersiz kılma (anamorfik DVD).
- **Kalite:** eksik glif denetimi (yazı tipinde ğ/ş/İ/ı yoksa); yazımda Japonca saygı ekleri/anime sözcükleri; Türkçe harf dönüştürme.
- **Lua:** `clipboard` artık gerçek sistem panosu (arboard); `BM.BadMutex`, `PT.PreciseTimer` yerleşik.
- **Performans:** geri alma yığını satır ekleyen işlemlerde yalnızca farkı saklar; `OSISUB_FRAME_LOG` ile 50k satırda medyan 2.1 ms/kare.
- **Dağıtım:** `[nightly]` commit'lerinde release.yml paketleri (ilk kez çalıştırıldı, Linux/macOS/Windows yeşil); Windows paketi ffmpeg getirir, program ffmpeg'i kendi klasöründe de arar.
- HATA DÜZELTİLDİ: köşe pini/perspektifte LayoutRes eklemek girdi indekslerini kaydırıp yanlış satırı siliyordu; Lua `clipboard.get` arayüzde boş dönüyordu.
- Tuzak: egui `lost_focus()` canlı okunur — aynı karede `request_focus()` sonrası false (Stil yardımcısında Enter çalışmıyordu).
- (aynı gece, devam) Toplu dosya işlemleri penceresi (core::filebatch), HTML QC raporu (GUI + CLI `check --report`), çok dilli yazım (ek sözlük, gerçek en_US ile testli), anamorfik video/en-boy oranı.
- Vektör kırpma/çizim araçlarında Ctrl+tık ile eğri; anamorfik videoda PlayRes QC'si DAR'ı kabul eder.
- Uçtan uca GUI testi (`tools/ui-e2e.sh`) yeni menü öğelerinden sonra yerelde yeniden çalıştırıldı: hepsi TAMAM.

### Gece sonu durumu (2026-11-12, v0.5.0)
- Tüm testler yeşil; CI her push'ta yeşil (Linux/macOS/Windows). Sürüm commit'i `[nightly]` ile → Release iş akışı paketleri üretir.
- Doğrulanmayanlar: gerçek ses aygıtı, Windows paketindeki gömülü ffmpeg'in gerçek Windows'ta çalışması, kullanıcının kendi Lua betikleri (depoda yok).

## 2026-11-13 (beşinci gece; kayıt 2026-11-15'te eklendi)
Dal: `claude/vibrant-clarke-u7myjw`. Gece sonu kaydı/sürüm commit'i yapılmamıştı; işler 0.6.0'a dahil edildi.
- PGS OCR (Tesseract), çevrilmemiş satır QC'si, satır dengeleme, satırları böl, eğme aracı, uygulama içi kılavuz (F1), EBU STL/iTT/TTML, OP/ED yerleştir, ekran genişliği QC'si, Türkçe yazım hataları.
- Performans: boştayken çizim yok (`ViewportCommand::Title` her karede gönderiliyordu).

## 2026-11-15 (v0.6.0 — altıncı gece)
Dal: `claude/vibrant-clarke-g5biih` (main'den; `-u7myjw` ileri sarıldı). 2026-11-14 gecesine ait dal/commit yok.
- **VobSub OCR:** `core::vobsub` (idx ayrıştırma, MPEG-PS demux, SPU RLE çözücü + test üreticisi); ffmpeg dvdsub çözücüsüyle çapraz doğrulandı; MKV izi için `-f mpeg` ile çıkarma + paleti ffprobe CodecPrivate dökümünden okuma. Gerçek Tesseract ile uçtan uca test (.idx ve MKV).
- **SubInspector.Inspector:** `core::coverage` (siyah/beyaz zemin farkından alfa), `media::render_coverage` (tek ffmpeg, `settb=1/1000`+`setpts` ile istenen anlar), Lua `osisub.render` köprüsü ve gömülü `SubInspector.Inspector` modülü; Host'a `render_bounds`.
- **Ekran çakışması QC** (`core::collide` + `media::screen_collisions`), **Whisper** (`media::whisper`, sahte ikiliyle testli), **bölümler** (`core::chapters`, MKV'ye gömme), **karaoke sese oturt** (`core::karaoke::snap_to_onsets`, `media::onset`).
- Tuzak: ffmpeg `setpts` süzgeci zaman tabanına yuvarlar — `color` kaynağı r=25 iken ms değerleri 40 ms'ye yuvarlanıyordu; önce `settb=1/1000`.
- libass `\p` çizimlerinde kenarlarda alfa≈2'lik pikseller bırakır (sınır testleri ±1 px toleranslı).
- Ortam: huggingface.co proxy tarafından engelli → Whisper gerçek modelle doğrulanamadı.
- Aynı gece (devam): karaoke gecikme kalibrasyonu (`karaoke::tap_offset_ms`), toplu kalite raporu (`qc::report_html_multi`, CLI `qc-report`), saygı ekleri (`core::honorifics`), komut paletine yeni pencereler.
- Bağımsız kod incelemesi (alt ajan) 9 bulgu verdi, hepsi düzeltildi: çakışma denetiminde libass yığması (diğer konumsuz satırlar saydam olarak çizime dahil — gerçek libass testiyle), `is_positioned` yalnız `\pos/\move`, `render_coverage` parça boyu kare boyutuna göre (~256 MB), benzersiz geçici dosyalar, VobSub idx zamanları, SubInspector kare anı, bölüm numarası, Whisper parantez süzgeci.

### Gece sonu durumu (2026-11-15, v0.6.0)
- Tüm testler yeşil, CI yeşil; 0.6.0 paketleri (Linux/Windows/macOS) Actions'ta.
- Doğrulanmayanlar: Whisper gerçek modelle (indirme engelli), gerçek ses aygıtı, gerçek ASSFoundation/Aegisub-Motion, kullanıcının kendi Lua betikleri (depoda yok).
- Sıradaki öneriler: Windows paketine whisper-cli/Tesseract ekleme (release.yml; URL'ler bu ortamda doğrulanamadı), macOS paketine ffmpeg, İngilizce arayüz, furigana düzeni.


## 2026-11-20 (yedinci gece, v0.7.0)
Dal: `claude/vibrant-clarke-er83om` (main'den; `-g5biih` ileri sarıldı). 2026-11-16…19 gecelerine ait dal yok. Ortam: `xvfb xdotool libxkbcommon-x11-0 libasound2-dev fonts-ipafont-gothic tesseract-ocr` kuruldu.
- **Furigana düzeni** (core::furigana, fonts::apply_furigana): `漢字|かんじ` heceleri → okunuşsuz ana satır + hece ortasına `\an2\pos` okunuş satırları (`<stil>-furigana`), hecesiyle vurgulanır; libass render'ıyla görsel doğrulandı (IPA Gothic). Kanji zamanlayıcıda romajiden otomatik okunuş (`romaji_to_hiragana`).
- **Aegisub satır işlemleri** (core::lineops): Recombine Lines, zamanları ardışık yap, değiş tokuş; ızgarada etiket göster/sadeleştir/gizle.
- **Yer imleri** (core::bookmarks; Extradata `osisub.bookmark`): Ctrl+F2 / F2 / Shift+F2, liste penceresi, not (xdotool ile doğrulandı).
- **QC profilleri** (Netflix yetişkin/çocuk, fansub, TV/EBU), **gereksiz etiketleri sil** (core::tagclean), **ad tutarlılığı** (core::names), **dosyayı böl** (core::filesplit).
- **Gömülü (hardsub) altyazı OCR** (core::hardsub + media::hardsub): kenarla çevrili parlak bileşenler, akış halinde satır sınırı, gri 2× görüntü → Tesseract. 1080p'de ~30× gerçek zaman (release, 4 çekirdek).
- Bağımsız kod incelemesi (alt ajan) 12 bulgu: düzeltilenler — yer imi notu düzenlenemiyordu (egui tamponu her karede sıfırlanıyordu), `re|n|a|i` → れない (hece sınırı kayboluyordu), boşluk/ē/m+b/va, Extradata olaylardan önce gelince yer imi yanlış satıra, CJK glifi olmayan yazı tipinde kanji genişliği yarım ölçülüyordu (artık 1 em), satır başı/sonu boşlukları, tek hatalı satır tüm furigana işlemini durduruyordu, eksik stilde ad, yeniden çalıştırmada eski çıktı (zaman yerine stille eşleşme), Recombine'da araya giren satır. Bilinçli bırakılan: furigana'da `\an`/`\pos` ilk-geçerli önceliği ve ikinci baştaki blok (nadir).
- Tuzak: egui'de her karede yeniden kurulan `TextEdit` tamponu yazılanı kaybeder — düzenleme tamponunu uygulama durumunda tut.
- Tuzak: hardsub'da maske "parlak + yakında koyu" piksel olarak kurulursa beyaz zeminde metnin çevresi de girer; bağlı bileşen + çevre oranı + harf boyutu + iç boşluk (kutu içinde kutu) kuralları gerekti.
- Google Drive'da kullanıcının `assmotion-devam-*.zip` (Lua betikleri olabilir, 1.4–2.5 MB) ve `OsiSub_tum_kod_3.x.txt` dökümleri bulundu; bağlama çekilemeyecek kadar büyük, açılmadı. Kullanıcının Lua betikleri hâlâ depoda yok.
- (devam) Düz metin içe/dışa aktarma, `require "osisub"` Lua API'si, video sonrası satır QC'si, yerleşik dışa aktarım süzgeçleri, hardsub üst bölge, macOS paketine ffmpeg (release.yml, `continue-on-error`), başlangıç ekranında Hızlı başla.
- İkinci bağımsız inceleme (alt ajan): düzeltilenler — **tagclean görünümü değiştiriyordu**: not/bilinmeyen etiketli blok ve çizim bloğu durumu güncellemiyordu, `\bord`↔`\xbord/\ybord` ve `\shad` aileleri ayrı izleniyordu, göreli `\fs+5` mutlak sanılıyordu, `\fsc` `fs` olarak ayrıştırılıyordu (hepsi teste eklendi); düz metin gidiş-dönüşü: `#`/`//` ile başlayan satırlar yorum sanılıp sonraki tüm satırları kaydırıyordu (kaçış), "Dinle: geliyorlar" ada dönüşüyordu (`: ` öneki), "Guard 2"/"Dr. Who" adları reddediliyordu, boş satır "-" oluyordu; hardsub: dönmüş videoda ffmpeg sessizce boş sonuç veriyordu (`-noautorotate` + stderr dosyaya + kare yoksa hata), fps ≤ 0, üst bölge seçiliyken çift tarama; Lua `time_format(math.huge)` taşması; ad denetiminde yalnızca cümle başında geçen biçimler (Bırak↔Burak, Selam↔Selim) artık uyarılmaz.
- Tuzak: ffmpeg varsayılan olarak dönme meta verisine göre kareyi döndürür; ffprobe'un `width/height`'ıyla kırpma yapan her yerde `-noautorotate` (ya da boyutları çevir).
- (devam, 0.7.0 sürüm commit'inden sonra) Kayıt yedekleri + Yedekler penceresi, çakışan satırları seç, OCR hatalarını düzelt (core::ocrfix; gerçek en_US Hunspell ile: rn→m, l'm→I'm, d0or→door, co1d→cold; belirsiz "yeu" bırakılır), OCR/hardsub pencerelerinde isteğe bağlı düzeltme, çeviri tutarlılığı (core::consistency), "Yayına hazır mı?" kontrol listesi, Lua konsolu (geçici betik olarak sarılıp mevcut worker'la yüklenir/çalışır; makro menülerinde gizli), karşılaştırma HTML raporu (sözcük düzeyinde LCS farkı), yer imi listesini kopyala, araç çubuğu, çoklu değiştir (core::multireplace), karaoke türünü toplu değiştir.
- Release: macOS ffmpeg için evermeet.cx 410 döndü (kaldırılmış); ffmpeg-static (GitHub, b6.0) + ffprobe-static (npm) ile değiştirildi, araç başına doğrulama, `continue-on-error` — bir sonraki `[nightly]` çalıştırmasında doğrulanacak.
- Tuzak: Rust `fancy_regex` ile `\s+([,.!?])` → `$1` gibi değiştirme kuralları `search::convert_repl` üzerinden çalışır; çoklu değiştir de aynı yolu kullanır.
- Üçüncü bağımsız inceleme (alt ajan): **eski bir Bul/Değiştir hatası** bulundu — fancy-regex'te ileri/geri bakış içeren desende `case_insensitive` bayrağı etkisiz kalıyordu: "tam sözcük" + harf duyarsız aramada "naruto" "Naruto"yu hiç bulmuyordu (satır içi `(?i)` ile düzeltildi); `\N` harf sayılıyordu (N→M kuralı `\N`'yi bozuyordu, satır sonundan sonraki sözcük tam sözcük sayılmıyordu); harf duyarsız kipte ı/i katlanıyordu (`(?-i:[ıI])`). Ayrıca: yedekler göreli yolla anahtarlanıyordu (iki klasördeki `tr.ass` yedekleri karışıyordu → canonicalize) ve yedeğin yedeği asıl listeye karışıyordu (katı ad süzgeci); OCR düzeltmesi "OOO"yu "000" yapıyordu; çoklu değiştir sayımları seçim/belge değişince yenilenmiyordu; TSV'de BOM; konsol başka makro çalışırken takılıyordu; Clean Script Info `Timer`'ı siliyordu.
- Tuzak: fancy-regex'te harf duyarsızlığı için `RegexBuilder::case_insensitive` yerine desene `(?i)` koy (bakış ifadeli desenlerde bayrak işlemiyor).
