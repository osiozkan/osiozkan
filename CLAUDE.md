# osisub — Claude için proje notları

Bu depo kökündeki `README.md` kullanıcının **GitHub profil sayfasıdır; dokunma.** Proje: **osisub**, Rust ile yazılmış anime altyazı stüdyosu (Aegisub + Subtitle Edit'in iyi yönleri, Aegisub Lua betiklerini çalıştırır). Kullanıcı Türk; arayüz ve belgeler **Türkçe**. Çalışma dalı: `claude/vibrant-clarke-2e8jtn` (başka dala push etme, PR yalnızca istenirse).

## Gece rutini (zamanlanmış çalışma)
1. Önce `docs/DEVLOG.md` ve `docs/ROADMAP.md` oku; kaldığın yerden devam et. `CHANGELOG.md`'de sürüm notu tut.
2. Küçük adımlarla ilerle; her adımda `tools/ship.sh "mesaj"` (derle + test + commit + push; hata varsa durur). **Derleme kırıkken asla commit etme.**
3. Kullanıcı görünür değişikliklerini ekran görüntüsüyle doğrula (aşağıya bak), `docs/ROADMAP.md` ve `docs/DEVLOG.md`'yi güncelle.
4. Bitişte (sabah 06:00'dan önce) özet için `PushNotification` gönder (`<routine_summary>` içinde); sorun yoksa sessiz kalma — gece sonu özeti kullanıcının beklediği sonuçtur.

## Mimari
Rust workspace (`crates/`): `osisub-core` (biçimler, QC, düzeltmeler, karaoke, arama, zamanlama…), `osisub-lua` (LuaJIT, Aegisub Automation 4 API), `osisub-doc` (belge/undo/pano/Lua worker), `osisub-media` (ffmpeg: dalga/video/ses saati), `osisub-spell` (Hunspell/spellbook), `osisub-cli`, `osisub-gui` (egui/eframe 0.37; `app.rs` + `app/{grid,tabs,windows}.rs`, `media_ui.rs`, `dialog_ui.rs`, `style_ui.rs`, `settings.rs`). Ayrıntı: `docs/OSISUB.md`.

## Komutlar
```
cargo test --workspace --exclude osisub-gui      # tüm testler
cargo build -p osisub-gui                        # GUI (release ~2.5 dk)
tools/ship.sh "commit mesajı"                    # derle+test+commit+push
tools/ui-e2e.sh target/debug/osisub-gui examples/demo.ass   # uçtan uca GUI testi (xvfb+xdotool)
MEDIA_TEST=1 tools/ui-smoke.sh target/debug/osisub-gui examples/demo.ass out.png --media video.mkv
```
Başsız GUI görüntüsü: `LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a -s "-screen 0 1400x900x24" target/debug/osisub-gui dosya.ass [--media v.mkv] [--ref r.srt] [--tab styles|translate|karaoke|spell|quality|automation|info] [--line N] [--open find|timing|resample|fonts|fixes|stats|vad|pos|clip|rotate|scale|play|spectrum|spell] --screenshot out.png` (sonra `Read` ile görüntüyü incele). Ortam kurulumu: `apt-get install xvfb xdotool libxkbcommon-x11-0 ffmpeg libasound2-dev`.
Test medyası üretmek: `ffmpeg -f lavfi -i testsrc2=size=1920x1080:rate=24:duration=16 -f lavfi -i "aevalsrc=...:s=44100:d=16" -shortest demo.mkv`.
Gerçek tr_TR sözlük testi: `OSISUB_TR_DICT=<klasör> cargo test --release -p osisub-spell` (sözlük depoya konmaz; LibreOffice/dictionaries'den indirilir).

## Tuzaklar / öğrenilenler
- egui 0.37 API'si eğitim verisinden farklı: `Panel::top("id").show(ui, …)`, `App::ui(&mut self, ui, frame)`, `Id::unique`, `ctx.global_style_mut`, `ui.fonts_mut`, `ui.close()`. Emin değilsen `~/.cargo/registry/src/*/egui-0.37.0` kaynağına bak.
- Emoji/simge karakterleri (⚠ 🔍 ▶) varsayılan fontta yok → kutu çıkar; düz metin kullan.
- Seçilebilir etiketler Ctrl+C'yi tüketir: `selectable_labels=false` (theme.rs).
- Xvfb'de klavye odağı için `xdotool search --onlyvisible` + `windowfocus` gerekir; tıklamadan önce `mousemove` + kısa bekleme.
- `-copyts` olmadan ffmpeg `-ss` altyazı zamanını kaydırır (önizleme yanlış satırı gösterir).
- Menü sırası değişirse `tools/ui-e2e.sh` içindeki tıklama koordinatları (Otomasyon menüsü) kayar.
- `pkill -f osisub-gui` kendi kabuğunu da öldürür; PID ile öldür.
- Yeni Lua API'si eklerken `crates/osisub-lua/tests/fixtures/compat.lua`'ya test ekle.
- Doğrulanmamış: gerçek ses aygıtı (cpal), macOS, CI'nin gerçek çalıştırması, Windows'ta çalıştırma (yalnızca derleme+bağlama doğrulandı).

## Stil
Yorum ve belgeler Türkçe; kod yapısı çevresindekiyle uyumlu; gereksiz soyutlama yok. Testi olmayan çekirdek mantık ekleme.
