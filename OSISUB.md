# OsiSub — anime altyazı editörü

Aegisub'ın tam karşılığı + Subtitle Edit'in iyi özellikleri, Aegisub Lua (otomasyon v4) uyumlu.
Çalıştırma: `pip install lupa` → `python -m osisub [dosya.ass]` (tarayıcıda yerel arayüz, `127.0.0.1:8765`).
Test: `python -m pytest tests`

## Mimari
- `osisub/ass.py` ASS modeli, zaman/renk, Türkçe kodlama (cp1254) algılama
- `osisub/formats.py` SRT/VTT içe-dışa aktarma, biçim tanıma
- `osisub/qc.py` CPS, satır uzunluğu, süre, çakışma, tanımsız stil, etiket, tipografi
- `osisub/tools.py` kaydır, ölçekle, sırala, çakışma/boşluk düzelt, keyframe snap, bul-değiştir (regex), böl/birleştir, yeniden ölçekle
- `osisub/lua/` LuaJIT (lupa) üzerinde Aegisub API: `aegisub.*`, `subs` nesnesi (insert/append/delete/deleterange/n),
  `include("karaskel.lua"|"utils.lua")`, `require "aegisub.re|unicode|util|clipboard"`, register_macro/filter, dialog, progress, debug.out
- `osisub/server.py` + `osisub/web/index.html` arayüz: sanal kaydırmalı grid, editör, video + önizleme, undo/redo, QC, stil, araçlar, otomasyon, bilgi
- `automation/` kullanıcı Lua scriptleri (ayrıca `~/.osisub/automation/`)

## Durum — v0.1.0 (gece 1)
Yapıldı: yukarıdaki tüm çekirdek, 7 test yeşil, UI tarayıcıda hatasız açılıyor.

## Sıradaki işler (öncelik sırası)
1. Lua uyumluluk: kullanıcının gerçek scriptleriyle test (repoya `automation/` altına koyulursa), eksik API'ler (`aegisub.dialog.display` UI köprüsü, `lfs`, `karaskel` furigana, `text_extents` gerçek font ölçümü, `aegisub.keyframes`)
2. ASS render (JASSUB/libass-wasm) ile video üstünde gerçek önizleme; timeline + ses dalga formu + spektrum; keyframe yükleme/snap UI
3. Karaoke zamanlama aracı (\k, ses ile), tag editörü/toolbar, stil önizleme, ayrı stil yöneticisi (kopyala/sil/yeniden adlandır, kütüphane)
4. Subtitle Edit özellikleri: çeviri modu (iki panel), yazım denetimi (Türkçe hunspell), OCR yok, otomatik süre düzeltme, FPS dönüşümü, SSA/SUB/STL/TTML dışa aktarma
5. Masaüstü paketleme (pywebview/Tauri), kısayol özelleştirme, bellek/performans (100k satır), otomatik yedek
