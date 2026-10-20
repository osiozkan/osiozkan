# Yuzu — geliştirme rehberi (gece oturumları)

Yuzu bu depodaki **ana üründür** (Aegisub iş akışı + Subtitle Edit araçları + Aegisub uyumlu Lua, Osi'nin çeviri araçları). Sahibi Türkçe konuşur; arayüz Türkçe (İngilizce seçilebilir), kod ve yorumlar İngilizce.
`../subforge/` aynı hedefle yazılmış kardeş denemedir; **donör** olarak kalır: Yuzu'da olmayan bir şeyi (ör. çekirdek biçimleri, MoonScript, yazım denetimi bu şekilde alındı) önce orada ara, sonra Yuzu'nun kalıbına uyarlayarak taşı. SubForge'u ayrıca geliştirme.

## Düzen
`asscore` (model/format/QC/zamanlama, UI yok) · `yuzu-lua` (Automation 4, LuaJIT) · `yuzu-media` (libmpv/ffmpeg) · `yuzu-fonts` · `yuzu-spell` · `yuzu-cli` · `yuzu-gui` (egui; `app.rs` + `tools.rs`/`editing.rs`/`visual.rs`/`spell.rs`…, çeviri `i18n.rs`).

## Kurallar
- Arayüz metni İngilizce yazılır, `t("…")`/`tf!` ile çevrilir; **her yeni metin `i18n.rs` TR tablosuna eklenmeli** (tamlık testi var). Komutlar `commands.rs` kaydından gelir.
- Her özellik test ister: `cargo test --workspace` yeşil olmadan commit etme (yaklaşık 2 dk). Derleyici uyarısı bırakma.
- egui'de emoji kullanma; ekran görüntüsü için Xvfb (`DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1`).
- Lua betik uyumluluğu kutsaldır (kullanıcının betikleri); `automation/README.md` ve `automation/tests` çalıştırılmalı.
- `ROADMAP.md` işaretli listeden ilerle, `CHANGELOG.md`'ye yaz, sürümü `Cargo.toml`'da artır.

## Gece rutini
Çalışma dalı `claude/vibrant-clarke-z9g30i` (depo osiozkan/osiozkan), PR açma. Küçük commit'ler, push. İstanbul saatiyle 06:00'da dur. Kullanıcıyı yalnızca gerçekten bir şey gerekiyorsa bildirimle uyar.
