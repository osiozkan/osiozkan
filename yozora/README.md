# Yozora (夜空)

Anime altyazıcıları için sıfırdan yazılmış, **Aegisub uyumlu** ve Subtitle Edit'in güçlü yanlarını bünyesine alan
altyazı aracı. Çekirdek Rust ile yazılır; Lua betikleri (Automation 4) LuaJIT üzerinde çalışır.

> Durum: **v0.1 (çekirdek + CLI)**. Grafik arayüz sonraki sürümlerin hedefi — bkz. [ROADMAP.md](ROADMAP.md).
> Gece gece ilerleme kaydı: [NIGHTLY.md](NIGHTLY.md).

## Neler var (v0.1)

| Alan | Durum |
|---|---|
| ASS / SSA okuma-yazma (kayıpsız gidiş-dönüş, Fonts/Graphics/Garbage bölümleri korunur, bozuk dosyalara dayanıklı) | ✅ |
| SRT, WebVTT, MicroDVD, düz metin içe/dışa aktarma | ✅ |
| Override etiketi ayrıştırıcı, görünür metin, CPS | ✅ |
| Zamanlama: kaydırma, TPP (lead-in/out, anahtar kare yakalama, bağlama), birleştir/böl, sürekli yap | ✅ |
| Kalite kontrol (örtüşme, CPS, satır uzunluğu, süre, eksik stil, tipografi) | ✅ |
| Geri al / yinele (anlık görüntü) | ✅ |
| **Aegisub Automation 4 uyumu**: `aegisub.register_macro/filter`, `subs` nesnesi, `include("utils.lua"/"karaskel.lua"/"unicode.lua")`, `re`, `aegisub.dialog`, `text_extents` (gerçek yazı tipi ölçümü) | ✅ |
| Her betik ayrı Lua durumunda (global çakışma yok) | ✅ |

## Kullanım

```bash
cargo build --release          # ikili: target/release/yozora

yozora info bolum.ass
yozora convert bolum.srt bolum.ass
yozora qc bolum.ass --cps 17 --max-line 42
yozora shift in.ass out.ass --by -1.5s
yozora tpp in.ass out.ass --lead-in 120 --lead-out 500 --keyframes kf.txt --fps 23.976

# Aegisub betiklerin:
yozora lua list betik.lua
yozora lua run betik.lua -m "Makro adı" in.ass -o out.ass --select 1-20 -p shift=3 -p __button=Uygula
```

Başsız modda `aegisub.dialog.display` kontrolleri varsayılan değerlerini döndürür; `-p ad=değer` ile değiştirilir.

## Mimari

```
crates/
  yozora-core   ASS modeli, ayrıştırıcı/yazıcı, dönüşümler, zamanlama, QC, geçmiş
  yozora-lua    LuaJIT (mlua) üstünde Automation 4 uyumluluk katmanı
  yozora-cli    `yozora` komutu
```

## Geliştirme

```bash
cargo test --workspace
```
