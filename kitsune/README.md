# Kitsune

Anime altyazıcıları için, Aegisub'ın iş akışını temel alan ve Subtitle Edit'in güçlü yanlarını katan, **Rust** ile sıfırdan yazılan altyazı editörü. Hedef: mevcut **Aegisub Lua (Automation 4)** betiklerinin değişmeden çalışması, hızlı ve sade bir arayüz.

> Durum: erken geliştirme (v0.1). Yol haritası: [`docs/ROADMAP.md`](docs/ROADMAP.md) · Değişiklikler: [`CHANGELOG.md`](CHANGELOG.md)

## Bileşenler
| Crate | İş |
|---|---|
| `crates/core` | ASS/SSA/SRT/VTT okuma-yazma, etiket ayrıştırma, karaoke, zaman, kalite denetimleri |
| `crates/lua` | Aegisub uyumlu Lua motoru (LuaJIT): `subs` nesnesi, `aegisub.*`, `karaskel`, `util`, `unicode` |
| `crates/cli` | `kitsune` komut satırı aracı |
| `crates/gui` | egui tabanlı masaüstü arayüz |

## Derleme ve kullanım
```bash
cd kitsune
cargo test --workspace
cargo run -p kitsune-gui --release          # arayüz (Linux'ta libxkbcommon-x11 gerekir)
cargo run -p kitsune-cli -- --help
cargo run -p kitsune-cli -- lua run examples/ornek_makro.lua 0 girdi.ass cikti.ass 0,1
```

## Lua uyumluluğu (şu an)
`aegisub.register_macro/register_filter`, `aegisub.cancel`, `aegisub.debug.out`, `aegisub.progress.*`, `aegisub.dialog.display` (başsız: varsayılan değerler), `aegisub.text_extents` (**yaklaşık**), `aegisub.frame_from_ms/ms_from_frame`, `subs` (`#subs`, `subs[i]`, `insert/append/delete/deleterange`), `include("utils.lua"|"karaskel.lua")`, `require("aegisub.util")`, `unicode.*`. Eksikler yol haritasında.
