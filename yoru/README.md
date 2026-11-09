# 夜 Yoru — Anime Altyazı Stüdyosu

Aegisub'ın iş akışını (ASS + Lua otomasyon) Subtitle Edit'in dönüştürme/denetim araçlarıyla birleştiren, tarayıcı tabanlı (Vite + TypeScript) sıfırdan yazılmış altyazı editörü. Kendi Aegisub Lua betiklerini **değiştirmeden** çalıştırmayı hedefler.

```bash
npm install        # fengari'yi LuaJIT uyumlu (5 → "5", "5.0" değil) yamalar
npm run dev        # geliştirme sunucusu
npm test           # birim testleri (vitest)
npm run build && node e2e/smoke.mjs   # tarayıcı duman testi (Playwright)
```

## Mimari
| Katman | Dosya | Açıklama |
|---|---|---|
| Çekirdek | `src/core/ass.ts` | ASS ayrıştır/yaz (Fonts/Graphics/Garbage bölümleri korunur), stil/olay modeli |
| | `src/core/formats.ts` | SRT / WebVTT / düz metin içe-dışa aktarım, etiket dönüşümü |
| | `src/core/tags.ts` | override etiket ayrıştırma, karaoke, görünür-metin işlemleri |
| | `src/core/ops.ts` | kaydır, böl, birleştir, doğrusal senkron, bul-değiştir, kareye oturt, zincirle |
| | `src/core/qc.ts` | CPS, süre, satır uzunluğu, çakışma, tanımsız stil… + otomatik düzeltme |
| Lua | `src/lua/prelude.lua` | `aegisub.*` API, `subs` nesnesi, makro/filtre kaydı, diyalog (coroutine ile), `re`, `unicode`, `bit` |
| | `src/lua/util.lua`, `karaskel.lua` | `include("utils.lua")`, `include("karaskel.lua")` uyumlu |
| | `src/lua/runtime.ts` | fengari (Lua 5.3) köprüsü; JS↔Lua yalnızca JSON dizgeleriyle |
| Arayüz | `src/ui/*` | sanal kaydırmalı ızgara, düzenleme paneli, video + DOM önizleme, dalga formu, stil yöneticisi, QC, otomasyon |

## Aegisub Lua uyumluluğu (şimdiki durum)
Var: `register_macro/filter`, `subs[i]`/atama/`insert/append/delete/deleterange/n`, `aegisub.dialog.display/open/save`, `progress`, `debug.out`/`log`, `cancel`, `text_extents`, `frame_from_ms`/`ms_from_frame`, `video_size`, `keyframes`, `project_properties`, `decode_path`, `clipboard`, `re.*`, `unicode.*`, `util.*`, `karaskel.collect_head/preproc_line*`, `include`/`require`, Lua 5.1 uyumluluk katmanı (`unpack`, `setfenv`, `loadstring`, `math.pow`, `bit`).
Eksik/yaklaşık: bkz. `ROADMAP.md`.
