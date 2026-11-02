# osisub

Anime altyazı aracı: Aegisub uyumlu ASS/Lua otomasyonu + Subtitle Edit'in iyi yanları, sıfırdan Rust ile.

**Durum: v0.1 (gece 1) — çekirdek + Lua otomasyon + CLI. GUI henüz yok** (bkz. [ROADMAP.md](ROADMAP.md)).

## Bileşenler
| Crate | İş |
|---|---|
| `osisub-core` | ASS/SSA ayrıştırıcı-yazıcı (bilinmeyen bölümleri korur), SRT/WebVTT, override-tag araçları, kalite kontrolleri (CPS, süre, satır uzunluğu, çakışma, stil), toplu işlemler (kaydır, sırala, çakışma düzelt, böl, birleştir, kareye oturt) |
| `osisub-lua` | Aegisub Automation 4 uyumlu çalışma ortamı (**LuaJIT / Lua 5.1**, Aegisub ile aynı): `subs[i]`, `#subs`, `subs.insert/append/delete/deleterange`, `aegisub.register_macro/register_filter`, `aegisub.log/dialog/progress/cancel/frame_from_ms/...`, `util`, `unicode`, `utf8`, `karaskel` (alt küme) |
| `osisub` (cli) | `info`, `convert`, `check`, `shift`, `fix`, `macros`, `run` |

## Kullanım
```
cargo build --release
osisub macros senin_scriptin.lua
osisub run senin_scriptin.lua "Makro Adı" bolum.ass -o cikti.ass --select 3,4,5
osisub run script.lua Makro in.ass --set amount=7      # aegisub.dialog.display varsayılanlarını geçersiz kıl
osisub check bolum.ass --max-cps 17
osisub convert bolum.ass bolum.srt
```
Makro hata verirse veya `aegisub.cancel()` çağırırsa dosya olduğu gibi geri yüklenir (Aegisub davranışı).

## Test
`cargo test` — 27 test (çekirdek birim testleri + gerçek Lua makroları ile entegrasyon).

## Bilinen uyumluluk boşlukları (dürüst liste)
- `aegisub.text_extents` **yaklaşık** ölçüm yapar (yazı tipi okumaz). Karaoke şablonlarında yerleşim hatası verebilir → gece 2'de gerçek font metrikleri.
- `ipairs(subs)` desteklenmez (Aegisub'da da güvenilir değildir); `for i=1,#subs` kullanın.
- `lpeg`, `re`, `karaskel.furigana`, `kara-templater (ILL/karaoke templater)` henüz yok.
- `aegisub.dialog.display` başsız çalışır (varsayılan değerleri döner); GUI gelince gerçek diyalog.
- `aegisub.project_properties`/video/ses işlevleri kısmi.
