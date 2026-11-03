# Kaede 楓 — anime altyazı editörü

Aegisub'ın iş akışını (ASS, stil yönetimi, video + ses + dalga formu, Lua otomasyonu) ve Subtitle Edit'in
pratik araçlarını (QC, iki noktalı senkron, satır dengeleme, biçim dönüştürme…) tek, hızlı ve sade bir arayüzde
birleştirir. Türkçe için baştan tasarlandı (I/İ ı/I, cp1254 kodlama algılama, Türkçe QC kuralları).

## Kurulum / çalıştırma

```bash
cd kaede
python3 -m pip install -e ".[dev]"     # lupa (LuaJIT), PySide6, numpy
# ffmpeg ve libass sistemde kurulu olmalı (video/ses/önizleme için)
python3 -m kaede.gui.app [dosya.ass]    # GUI   (veya: kaede)
python3 -m kaede.cli --help             # komut satırı (convert, qc, shift, run)
python3 -m pytest -q                    # testler (GUI testleri için QT_QPA_PLATFORM=offscreen)
```

## Özellikler (v0.1)

| Alan | Durum |
|---|---|
| ASS/SSA ayrıştırma + yazma (Aegisub ek bölümleri korunur), SRT/VTT/SBV/TXT içe-dışa aktarma | ✔ |
| Kodlama algılama: UTF-8/16, BOM'suz, Windows-1254 (Türkçe) | ✔ |
| Tablo (sanal, 20k+ satır akıcı), düzenleme paneli, canlı CPS/CPL, etiket düğmeleri, geri-al/yinele | ✔ |
| **libass ile gerçek ASS önizleme** (Aegisub ile aynı işleyici) — videoyla veya videosuz | ✔ |
| Video oynatma (QtMultimedia), kare adımlama, satır oynat, VFR zaman kodu, keyframe içe/dışa aktarma | ✔ |
| Ses dalga formu (ffmpeg), sürükleyerek zamanlama, Ctrl+sürükle ile yeni satır, keyframe çizgileri, konuşma tespiti | ✔ |
| **Aegisub uyumlu Lua otomasyonu (LuaJIT 2.1, `ffi`/`bit` dahil)** — makro, filtre, diyalog, `karaskel`, `util`, `utf8`, `re`, `unicode`, `lfs`, `clipboard` | ✔ |
| Stil yöneticisi + kalıcı depo, betik özellikleri, yeniden ölçekleme (resample) | ✔ |
| Zamanlama: kaydır (ms/sn/kare), kareye hizala, keyframe'e yapıştır, çakışma/zincirleme/min-süre/CPS düzeltmeleri, iki noktalı senkron, FPS dönüştürme | ✔ |
| QC: CPS, CPL, satır sayısı, süre, çakışma, tanımsız stil, bozuk kodlama (mojibake), I/İ şüpheleri… | ✔ |
| Türkçe harf büyüklüğü, Bul/Değiştir (i/İ ı/I toleranslı), satır dengeleme, İE metni silme, yinelenenleri silme, istatistik | ✔ |
| Yazı tipi denetimi + toplama (zip) | ✔ |
| Otomatik yedek (2 dk), sürükle-bırak, son açılanlar, koyu/açık tema | ✔ |

Yol haritası ve gece notları: [`NIGHTLY.md`](NIGHTLY.md). Lua uyumluluk ayrıntıları: [`docs/LUA_COMPAT.md`](docs/LUA_COMPAT.md).

## Lua betiklerin
Betikler şu klasörlerden otomatik yüklenir: `~/.kaede/automation/autoload`, `kaede/scripts`, ve varsa Aegisub'ın
kendi klasörleri (`~/.aegisub/automation/autoload`, `%APPDATA%\Aegisub\automation\autoload`). `include`/`require`
için Aegisub'ın `automation/include` klasörleri ve `KAEDE_INCLUDE` ortam değişkeni de aranır; yani mevcut betik
kütüphanelerin değişiklik gerektirmeden çalışmalıdır. Otomasyon → Yönetici ile hata durumlarını görebilirsin.
