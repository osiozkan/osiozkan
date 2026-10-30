# Kitsune

Aegisub'in tüm yeteneklerini, Subtitle Edit'in en iyi özelliklerini ve anime altyazıcılığı için gereken her şeyi tek çatıda toplamayı hedefleyen, sıfırdan Rust ile yazılan altyazı aracı. **Mevcut Aegisub Lua (Automation 4) betikleriyle uyumludur.**

## Durum (v0.1.0 — gece 1)
Çalışan çekirdek ve komut satırı; grafik arayüz henüz yok (bkz. [ROADMAP.md](ROADMAP.md)).

| Katman | İçerik |
|---|---|
| `core` | ASS/SSA + SRT okuma/yazma (bilinmeyen bölümler korunur), zamanlama araçları, etiket ayrıştırma, kalite kontrol |
| `lua` | Gömülü Lua 5.1; `aegisub.*` API'si, `subs` nesnesi (indeks/append/insert/delete/deleterange), `util`, `unicode`, `karaskel`, geri alma (iptal/hata) |
| `cli` | `check`, `convert`, `shift`, `fix`, `lua-list`, `lua-run` |

## Kullanım
```
cargo build --release
target/release/kitsune check examples/ornek.ass
target/release/kitsune lua-run examples/ornek.lua examples/ornek.ass cikti.ass --select 1-1
cargo test
```
