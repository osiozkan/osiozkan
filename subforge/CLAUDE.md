# SubForge — geliştirme rehberi (otomatik gece oturumları için)

Amaç: Aegisub'ı tamamen kopyalayan, Subtitle Edit'in iyi özelliklerini katan, anime altyazı iş akışı için her şeyi içeren, hızlı ve kolay arayüzlü, sahibinin Lua (Automation 4) betikleriyle uyumlu bir program. Sahibi Türkçe konuşuyor; arayüz Türkçe, kod/yorum İngilizce.

## Düzen
- `crates/core` — format/model/QC/timing (UI yok, test edilebilir)
- `crates/lua` — Automation 4 uyumluluğu (mlua + LuaJIT, `lua/*.lua` gömülü kütüphaneler, `tests/automation.rs`)
- `crates/app` — proje/undo, arama, Lua çalıştırıcı (thread), ffmpeg medya
- `crates/cli`, `crates/gui` — ince katmanlar
- Çalışma dalı: `claude/vibrant-clarke-z9g30i` (depo: osiozkan/osiozkan, proje `subforge/` klasöründe). PR açma; yalnızca bu dala push.

## Komutlar
- `cargo test --workspace` (hepsi yeşil olmalı), `cargo build -p subforge-gui`
- GUI ekran görüntüsü: `Xvfb :99 &`, `DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 target/debug/subforge-gui dosya.ass`, `ffmpeg -f x11grab -video_size 1440x900 -i :99 -frames:v 1 shot.png` (gerekirse `apt-get install libxkbcommon-x11-0 libegl1`)

## Gece rutini
1. `ROADMAP.md`'yi oku, en üstteki ⬜/🟡 maddelerden başla; Lua uyumluluğu ve zamanlama/ses öncelikli.
2. Her özellik için test ekle; `cargo test --workspace` ve GUI derlemesi yeşil olmadan commit etme.
3. `CHANGELOG.md` ve `ROADMAP.md`'yi güncelle (sürüm numarasını `Cargo.toml` workspace'te artır).
4. Küçük, anlamlı commit'ler; `git push -u origin claude/vibrant-clarke-z9g30i`.
5. İstanbul saatiyle 06:00'da dur. Kullanıcıyı yalnızca gerçekten bir şey gerekiyorsa (kırık build, karar gereken konu) bildirimle uyar; "her şey yolunda" bildirimi gönderme.

## Kurallar
- Aegisub davranışı referanstır (alan adları, API, ASS yazım biçimi). Emin olmadığında Aegisub'ın belgelenmiş davranışına uy.
- Kullanıcının Lua betikleri kutsaldır: uyumluluğu bozan değişiklik yapma; yeni Aegisub API'si eksikse ekle.
- egui'de emoji kullanma (varsayılan fontta yok); düz metin/ASCII ikonlar.

## Notlar
- Paralel ajan çalışması gerekirse dosya sahipliğini ayır (ör. `crates/lua` bir ajan, `crates/core/src/formats` başka); ajan worktree'lerini `.claude/worktrees/` altında tutar (gitignore'da), birleştirme sonrası `git worktree remove`.
- `SUBFORGE_OPEN=styles,qc,spell,...`, `SUBFORGE_RUN=betik.lua|0`, `SUBFORGE_AUTOPLAY=1` ve `SUBFORGE_CONFIG=dir` ortam değişkenleri GUI ekran görüntüsü/test için vardır.
- Türkçe sözlük testi: `SF_DICT_DIR=... cargo test -p subforge-app real_turkish -- --ignored`.
