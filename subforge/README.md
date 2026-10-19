# SubForge

Anime altyazı çalışma tezgâhı — **Aegisub'ın iş akışı + Subtitle Edit'in kolaylıkları + Aegisub Automation 4 (Lua) uyumluluğu**, sıfırdan Rust ile.

| | |
|---|---|
| `subforge-core` | ASS/SSA/SRT/WebVTT modeli, etiket ayrıştırıcı, zamanlama araçları, kalite kontrol (QC), Türkçe metin/kodlama araçları |
| `subforge-lua` | Aegisub Automation 4 uyumluluk katmanı (LuaJIT: `ffi`, `bit` dahil), `karaskel`, `utils`, `unicode`, `re` |
| `subforge-app` | Arayüzden bağımsız mantık: proje/geri-al, bul-değiştir, iş parçacığında Lua çalıştırıcı, ffmpeg medya |
| `subforge-cli` | `subforge` komut satırı (toplu işlem, CI, betik çalıştırma) |
| `subforge-gui` | `subforge-gui` masaüstü uygulaması (egui) |

## Derleme

```bash
cargo build --release          # tüm çalışma alanı
cargo test --workspace
./target/release/subforge-gui altyazi.ass
```

Gereksinimler: Rust 1.80+, C derleyici (LuaJIT için), `ffmpeg`/`ffprobe` (ses dalga biçimi, video, anahtar kareler; libass ile gerçek ASS önizleme). Linux'ta GUI için X11/Wayland + `libxkbcommon`.

## Neler var

- **ASS** okuma/yazma (kayıpsız gidiş-dönüş; Fonts/Graphics/Aegisub Project Garbage bölümleri korunur), SRT/VTT içe-dışa aktarma
- **Kodlama**: UTF-8/16, BOM, Windows-1254 (Türkçe) otomatik algılama; bozuk "Ã§ÄŸ" (mojibake) onarma
- **Türkçe**: İ/ı doğru büyük-küçük harf, İ≡i arama
- **Kalite kontrol**: çakışma, kısa/uzun süre, CPS, satır uzunluğu/sayısı, bilinmeyen stil, bozuk `{}`, çift boşluk, yinelenen/boş satır — otomatik düzeltme
- **Zamanlama**: kaydır (ms/kare), iki noktalı eşitleme, fps dönüştürme, kareye/anahtar kareye hizalama, zincirleme, lead-in/out, böl/birleştir
- **Ses dalga biçimi**: görüntüle, sürükleyerek zamanla (başlangıç/bitiş tutamaçları, seçim → satıra uygula), anahtar kare çizgileri
- **Video**: libass ile ASS'li önizleme (FX dahil), kare adımlama, anahtar kareye atlama
- **Automation 4**: `.lua` betiklerinizi olduğu gibi çalıştırır — `aegisub.register_macro/register_filter`, `subs` nesnesi, diyaloglar (GUI'de gerçek form), ilerleme/iptal, `karaskel`, `include`/`require`
- **CLI**: `info`, `convert`, `check --fix`, `shift`, `retime`, `fps`, `chain`, `fix-encoding`, `lua list`, `lua run`

Ayrıntı ve yapılacaklar: [ROADMAP.md](ROADMAP.md) · Değişiklikler: [CHANGELOG.md](CHANGELOG.md)

## Lua betikleri

Betikleri `~/.config/subforge/automation/` (veya Otomasyon ▸ Betik yöneticisi'nden eklediğiniz klasöre) koyun. Aynı betik komut satırında:

```bash
subforge lua list betik.lua
subforge lua run betik.lua giris.ass -m "Makro adı" -o cikis.ass --select 1-20 -a alan=deger
```
