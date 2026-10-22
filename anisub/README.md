# AniSub

Aegisub'un tam işlevselliğini, Subtitle Edit'in en iyi özelliklerini ve anime
altyazı iş akışının ihtiyaç duyduğu her şeyi tek çatı altında toplamayı hedefleyen,
sıfırdan Rust ile yazılan altyazı aracı. **Mevcut Aegisub Lua otomasyon
scriptleri (`aegisub.register_macro`, `karaskel`, `utils` …) değiştirilmeden çalışır.**

## Yapı

| Dizin | İçerik |
|---|---|
| `core/` | ASS/SRT modeli, zamanlama, QC, **LuaJIT tabanlı Aegisub uyumlu otomasyon motoru** |
| `core/lua/` | `prelude.lua` (aegisub.* API), `utils.lua`, `karaskel.lua`, `unicode.lua` |
| `cli/` | `anisub` komut satırı aracı |
| `examples/` | örnek altyazı + Lua makro |

## Kullanım

```
cargo build --release
anisub macros script.lua                  # makroları listele
anisub run alt.ass script.lua --macro 0 -o cikti.ass
anisub qc alt.ass                         # CPS, çakışma, süre, stil kontrolleri
anisub shift alt.ass 500                  # +500 ms
anisub convert alt.srt alt.ass
```

Durum ve yapılacaklar için bkz. [ROADMAP.md](ROADMAP.md).
