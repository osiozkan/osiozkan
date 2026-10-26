# osisub

Anime altyazı aracı: Aegisub uyumlu ASS çekirdeği + Aegisub **Lua Automation 4** uyumluluk katmanı + Subtitle Edit tarzı kalite kontrol/düzeltme araçları + tarayıcı arayüzü. Tek bağımlılık: `fengari` (saf JS Lua 5.3).

```
npm install
npm test
node src/cli/cli.js serve        # http://127.0.0.1:8787/web/index.html
node src/cli/cli.js run in.ass my_script.lua out.ass --macro "Adı" --select 1-20 --set n=5
```

## Lua uyumluluğu (Aegisub scriptleri)
`aegisub.register_macro/filter`, `subs` nesnesi (`#subs`, `subs[i]`, `subs.insert/delete/append/deleterange`, sınıflar `info`/`style`/`dialogue`),
`aegisub.log/progress/dialog.display/text_extents/frame_from_ms/ms_from_frame/keyframes/decode_path/clipboard/cancel`,
`include("utils.lua" | "karaskel.lua" | "unicode.lua" | "re.lua")`, `require("aegisub.util" | "aegisub.re" | ...)`, `util.*`, `karaskel.*` (collect_head, preproc_line, parse_kara_syllables),
`re.*` (JS RegExp köprüsü), LuaJIT/5.1 gölgeleri: `unpack`, `loadstring`, `setfenv/getfenv`, `bit.*`, `%d` ile ondalık sayı, `480` (`480.0` değil) sayı biçimi.

Diyaloglar etkileşimsizdir: varsayılan değerler ya da `--set ad=değer` / web arayüzünde JSON ile verilen değerler döner. Ayrıntı: `docs/ROADMAP.md`.

## Mimari
- `src/core` — zaman, ASS/SRT/VTT/TXT okuma-yazma, etiket yardımcıları, QC (`check`) ve düzeltmeler (`fixes`, `shift`, `stretch`, `changeFps`, `snapToKeyframes`). Tarayıcıda da çalışır (Node bağımlılığı yok).
- `src/lua` — `AutomationHost` (fengari), `runtime.lua` (aegisub/util/karaskel/re/unicode), `metrics.js` (yaklaşık metin ölçümü; gerçek font metriği enjekte edilebilir).
- `src/cli` — komut satırı + yerel sunucu (`/api/run`, `/api/macros`).
- `web` — editör: sanal kaydırmalı grid, video önizleme, dalga biçimi (sürükle-zamanla), QC, bul/değiştir, zamanlama, stiller, Lua paneli.
