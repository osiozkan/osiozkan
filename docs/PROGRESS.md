# SubForge — İlerleme Günlüğü (her gece buradan devam edilir)

> Bu dosya, geceleri çalışan otomasyonun bir önceki turdan kaldığı yeri bilmesi içindir.
> Her tur sonunda **güncelle**: yapılanlar, bilinen sorunlar, sıradaki işler.

## Proje özeti
Aegisub'ın tüm çekirdek işlevlerini + Subtitle Edit'in iyi özelliklerini birleştiren, Aegisub **Auto4 Lua** betikleriyle
uyumlu, anime altyazı stüdyosu. Python 3.11 + PySide6 (Qt) + LuaJIT (lupa) + libass (ctypes) + ffmpeg.

- `subforge/core` — ASS modeli/ayrıştırıcı/yazıcı, SRT/VTT/MicroDVD/LRC, zaman araçları, QC, Türkçe harf işleme
- `subforge/automation` — Auto4 uyumlu Lua motoru (`aegisub.*`, `karaskel`, `util`, `utils`, `re`, `unicode`, `lfs`)
- `subforge/gui` — Qt arayüzü (grid, düzenleme paneli, dalga formu/spektrogram, video+libass, stiller, bul/değiştir, …)
- `subforge/media.py` — ffprobe/ffmpeg: bilgi, anahtar kareler, ses önbelleği (16 kHz mono memmap)
- `subforge/render.py` — libass ile birebir ASS render (yoksa önizleme kapalı)
- `subforge/cli.py` — `python -m subforge {gui,convert,check,shift,fix,lua,info}`
- `scripts/` — kullanıcının Lua betikleri buraya konur (otomatik taranır). **Kullanıcının kendi betikleri henüz depoda yok.**

## Test / çalıştırma
```
pip install PySide6 lupa numpy pytest
python -m pytest tests -q          # GUI testleri QT_QPA_PLATFORM=offscreen ile çalışır
python -m subforge                 # arayüz
python -m subforge lua betik.lua dosya.ass -m "Makro" --set anahtar=değer
```

## Sürüm geçmişi
Bkz. `CHANGELOG.md`.

## Kararlar
- Lua dizeleri **bayt-şeffaf** (latin-1 köprüsü): Aegisub/Lua 5.1 gibi `text:sub()` UTF-8 ortasından kesebilir. Python↔Lua
  sınırında her zaman `enc()/dec()` kullan (`automation/engine.py`).
- `subs` nesnesi LuaJIT'te `#subs` çalışsın diye `newproxy` userdata.
- Düz (flat) satır listesi: info + stil + olay, Aegisub ile aynı indeksleme. GUI satırları (event) ↔ flat dönüşümü `run_macro`'da.
- Geri al: anlık görüntü (snapshot) tabanlı; metin düzenleme 1.2 sn içinde birleştirilir.
- Dokunulmayan: kök `README.md` kullanıcının GitHub profil README'sidir; proje belgeleri `docs/` altındadır.

## Bilinen eksikler / sıradaki işler (öncelik sırasıyla)
Bkz. `docs/ROADMAP.md`.

## Tur günlüğü
### Tur 1 (ilk gece) — v0.2.0
- Sıfırdan: çekirdek + Lua motoru + Qt arayüz + CLI + 68 test (hepsi geçiyor)
- Aegisub'ın gerçek karaskel/utils/kara-templater/cleantags betikleri dahil ve doğrulandı
- Bilinmeyenler: kullanıcının kendi Lua betikleri depoda yok → uyumluluk onlarla sınanmadı. Önceki sohbet geçmişi (karşılaşılan sorunlar) erişilemedi → `docs/SORUNLAR.md` genel Türkçe/anime sorunlarından derlendi.
- Sonraki tur: `docs/ROADMAP.md` > "Sonraki turlar için öneri".

### Her tur başında yapılacaklar
1. `git pull`, `python -m pytest tests -q` (hepsi geçmeli)
2. `scripts/` ve `docs/SORUNLAR.md` içinde yeni madde var mı bak → önce onları çöz
3. ROADMAP'ten en üstteki maddeyi al, test yaz, uygula, CHANGELOG + bu dosyayı güncelle, commit + push
