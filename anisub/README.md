# anisub

Aegisub-uyumlu, sıfırdan yazılan anime altyazı aracı. Çekirdek Rust ile yazılmıştır; mevcut Aegisub Lua
scriptleri (`aegisub.register_macro`, `karaskel`, `utils`) değişmeden çalışacak şekilde hedeflenmiştir.

## Durum (v0.1)

- ASS/SSA modeli: ayrıştırma, yazma, stil/olay/bilgi satırları, centisaniye yuvarlama
- SRT / WebVTT içe-dışa aktarma (temel biçim etiketleri ↔ ASS override)
- Lua 5.1 otomasyon motoru: `subs` nesnesi (`#subs`, `subs[i]`, `insert`, `append`, `delete`, `deleterange`),
  `aegisub.register_macro/filter`, `log`, `progress`, `frame_from_ms`, `ms_from_frame`, `text_extents` (yaklaşık),
  headless `dialog.display`, `include("utils.lua"|"karaskel.lua")`
- CLI: `info`, `convert`, `macros`, `run`

```
cargo run --release -- run examples/demo.lua "Split karaoke" examples/sample.ass out.ass
cargo test
```

Yol haritası için `ROADMAP.md` dosyasına bakın.
