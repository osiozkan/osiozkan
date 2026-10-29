# Geliştirme günlüğü

Her gece çalışması (gece–06:00) buraya bir kayıt ekler. Yeni oturum önce bu dosyayı ve `docs/ROADMAP.md`'yi okumalı, kaldığı yerden devam etmeli. Sonuç hep `claude/vibrant-clarke-2e8jtn` dalına push edilir; PR yalnızca kullanıcı isterse açılır.

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
