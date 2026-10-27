# Kaku — Kullanım Kılavuzu

## Kurulum
* **Hazır sürüm:** GitHub Actions `kaku-v*` etiketlerinde Windows/Linux/macOS için `kaku` (komut satırı) ve `kaku-gui` (arayüz) üretir.
* **Kaynaktan:** `cd kaku && cargo build --release` → `target/release/kaku-gui`.
* Video/ses/önizleme için `ffmpeg` ve `ffprobe` PATH'te olmalı (veya `KAKU_FFMPEG`, `KAKU_FFPROBE` ortam değişkenleri).

## Arayüz
`kaku-gui dosya.ass --media bolum.mkv`

| İş | Nerede |
|----|--------|
| Aç/Kaydet | Ctrl+O / Ctrl+S (.ass, .srt, .vtt) |
| Satır ekle / çoğalt / sil / böl / birleştir | Subtitles menüsü |
| Zaman kaydırma, boşluk kapatma, CPS'e uzatma, TPP | Timing menüsü |
| Dalga formunda sarı kenarları sürükleyerek zamanlama | Alt panel (Ctrl+tekerlek: yakınlaştır, Alt: keyframe yapışmasını kapat) |
| Karaoke otomatik bölme / \k temizleme | Karaoke menüsü |
| Lua makroları | Automation menüsü (`./automation/*.lua` ve `~/.kaku/automation/*.lua` açılışta yüklenir) |
| Stil yöneticisi, QC, script özellikleri | Sağ panel sekmeleri |
| Font denetimi / toplama | Subtitles ▸ Check / collect fonts |

## Lua scriptleri (Aegisub uyumlu)
Mevcut Aegisub `.lua` dosyalarınızı değiştirmeden `automation/` klasörüne koyun. Ek kütüphaneleriniz
(`include/` içinde, örn. `ILL/ILL.lua`) için `automation/include` veya `~/.kaku/automation/include` kullanın.

Komut satırından deneme:
```
kaku macros   script.lua
kaku run      script.lua dosya.ass -m "Makro adı" -o cikti.ass --set alan=değer --button Apply
kaku template dosya.ass          # karaoke templater
```
Desteklenen: `aegisub.*` (dialog, progress, debug, text_extents, frame/ms dönüşümü…), `karaskel`, `util`, `re`, `unicode`, `utf8`, `lfs`, LuaJIT `ffi`.

## Komut satırı araçları
`info, convert, check, shift, fix, fps, resample, balance, case, hi, tpp, fonts, media, snap, frame, template, macros, run` — her biri için `kaku <komut> --help`.
