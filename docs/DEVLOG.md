# Geliştirme günlüğü

Her gece çalışması (gece–06:00) buraya bir kayıt ekler. Yeni oturum önce bu dosyayı ve `docs/ROADMAP.md`'yi okumalı, kaldığı yerden devam etmeli. Sonuç o gecenin görevinin verdiği dala push edilir (`tools/ship.sh` geçerli dalı kullanır; 2026-10-29: `claude/vibrant-clarke-2e8jtn`, 2026-11-10: `claude/vibrant-clarke-6oujog`, 2026-11-11: `claude/vibrant-clarke-dsxf2y`); PR yalnızca kullanıcı isterse açılır.

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
