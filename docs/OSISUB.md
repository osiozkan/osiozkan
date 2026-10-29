# osisub — anime altyazı stüdyosu

Hedef: Aegisub'ın tüm işlevlerini, Subtitle Edit'in iyi özelliklerini ve anime altyazıcılığı için gereken her şeyi tek, hızlı ve kolay arayüzlü bir araçta toplamak; mevcut Aegisub Lua betiklerini (Automation 4) değişiklik olmadan çalıştırmak.

Not: Depo kökündeki `README.md` GitHub profil sayfasıdır; dokunulmaz. Proje belgeleri `docs/` altındadır.

## Mimari (Rust workspace)

| Crate | Görev |
|---|---|
| `osisub-core` | ASS/SSA/SRT/VTT okuma-yazma (korumalı, gidiş-dönüş güvenli), zaman/renk, etiket ayrıştırma, karaoke hece çıkarma, QC kuralları ve toplu düzeltmeler, Türkçe i/İ büyük-küçük harf, Windows-1254 çözme |
| `osisub-lua` | LuaJIT tabanlı Aegisub Automation 4 uyumlu motor: `aegisub.*`, `subs` nesnesi, `register_macro/filter`, `dialog.display`, `progress`, `re`, `unicode`, `lfs`, `clipboard`, `karaskel.lua`, `utils.lua` |
| `osisub-cli` | `osisub` komutu: convert, check, fix, shift, macros, run, info |

## Lua uyumluluğu

- Satır tabloları Aegisub ile aynı alanları taşır (`class`, `section="[Events]"`, `start_time`, `margin_t/b/v`, `extra`, `raw` ...).
- `subs[i]`, `#subs`, `subs.n`, `subs.append/insert/delete/deleterange`, `subs[0]=line` desteklenir.
- Makrodan dönen seçim/aktif satır ve `aegisub.cancel()` işlenir; `aegisub.set_undo_point` kaydedilir.
- `include("karaskel.lua")`, `include("utils.lua")`, `include("unicode.lua")`; `require "re" / "unicode" / "lfs" / "ffi" / "bit"`.
- İletişim kutuları `Host` arayüzü üzerinden gelir (CLI: varsayılanlar + `--param ad=değer`; GUI: gerçek diyalog).
- Betik dizini ve yapılandırılan include dizinleri önce aranır, sonra gömülü kütüphaneler.

Bilinen eksikler ve sıradaki işler: `docs/ROADMAP.md`. Gece gece ilerleme: `docs/DEVLOG.md`.

## Geliştirme

```
cargo test            # tüm testler
cargo build --release # hedef/release/osisub
target/release/osisub --help
```
