# Osisub — proje notları (her gece oturumunun hafızası)

Hedef: Aegisub'ın tüm yeteneklerini + Subtitle Edit'in iyi özelliklerini taşıyan, anime altyazı/typesetting/karaoke
odaklı, Aegisub **Lua otomasyon betikleriyle uyumlu**, hızlı ve kolay arayüzlü bir araç.

> Not: Bu depo (`osiozkan/osiozkan`) GitHub profil deposudur; kök `README.md` profil sayfasıdır ve
> dokunulmaz. Proje dokümanları `docs/` altındadır.

## Mimari
```
osisub/
  core/        ass.py (Document/Style/Event, kayıpsız parse+yaz) · timeutil · tags · tools (zamanlama, metin, QC) · history (undo)
  formats/     SRT / WebVTT / TXT <-> Document, kodlama tespiti (UTF-8/BOM/cp1254…)
  automation/  runtime.py (lupa/LuaJIT) + lua/prelude.lua (aegisub.*, subs proxy, re, unicode, util)
               lua/include/{karaskel,utils,unicode}.lua  → include("karaskel.lua") gerçek Aegisub gibi çalışır
  media.py     ffmpeg/ffprobe: probe, kare yakalama (+libass ile ASS render), ses tepe değerleri, anahtar kareler
  gui/         PySide6: app.py (MainWindow), models, dialogs, widgets (Preview, Waveform)
  cli.py       osisub convert|macros|run|check|shift
tests/         pytest (core, lua, gui[offscreen])
```
Önemli tasarım kararları
- Dahili zaman = tamsayı ms; ASS yazarken centisaniyeye yuvarlanır.
- Lua: **LuaJIT 2.1** (Aegisub ile aynı). `subs` bir `newproxy` userdata'sı: `#subs`, `subs[i]`, `subs[-i]=` (araya ekle),
  `subs.insert/append/delete/deleterange/script_resolution`, `ipairs(subs)` desteklenir. Her betik kendi ortamında (env) çalışır.
- Satır tabloları Aegisub ile aynı alanlar: class/section/start_time/end_time/margin_t/margin_b/text/extra …
  Stil renkleri `color1..4`. Seçim indeksleri global satır numarasıdır (info+style+event, 1-tabanlı).
- Diyaloglar: `Engine.dialog_handler`; GUI'de Qt formu (`AutomationDialog`), başsız modda varsayılanlar ve ilk düğme.
- Önizleme: ffmpeg `ass` filtresi (libass) → gerçek ASS render. Video yoksa koyu zemin üzerine render.
- Gerçek Aegisub `kara-templater.lua`, `cleantags`, `strip-tags` vb. değiştirilmeden çalıştırıldı (uyumluluk testi; telifli
  dosyalar depoda tutulmaz, `tests/` yalnızca kendi betiklerimizi içerir).

## Çalıştırma
```
pip install -e .            # lupa, PySide6-Essentials, numpy
osisub-gui [dosya.ass]      # arayüz
osisub run betik.lua giris.ass -o cikis.ass -m 0 -s 1-5
```

## Durum (v0.1.0, ilk gece sonu: 28 test yeşil)
Tamam: ASS/SRT/VTT G/Ç, undo/redo, tablo+düzenleme paneli, stil yöneticisi, bul/değiştir, kaydır/eşitle/anahtar kare/FPS,
çakışma ve süre düzeltme, QC (CPS/CPL/çakışma…), Türkçe büyük/küçük harf, libass önizleme, dalga formu (sürükleyerek
zamanlama), otomasyon menüsü + Lua çalışma zamanı, CLI.

## Açık kalan notlar (gözden geçirme sonrası)
- Lua `re`: Boost'a tam uyum yok (kalan farklar: boş eşleşme davranışı, `\u \l \U \E` ikame kaçışları).
- Event `extra` / `margin_b` geri yazılmıyor; `unicode.to_upper("ß")` Python kuralı; `util.trim/uuid` yok.
- Dialog `color` sonucu biçimi (&H..& mı #RRGGBB mı) Aegisub ile doğrulanmalı.
- karaskel furigana düzeni basitleştirilmiş.
- Hunspell sözlüğü paketle gelmez (`hunspell-tr`); yükleme ~6 sn (arka plan iş parçacığı).

## Yol haritası (öncelik sırasıyla)
1. Lua uyumluluğu: `re` modülü kenar durumları, `lfs`/`lpeg` yerine saf Lua yedekleri, `aegisub.dialog` tüm kontroller,
   Aegisub 3.3 `aegisub.project_properties`, `karaskel` furigana düzeni, kullanıcının kendi betikleriyle regresyon testleri.
2. Karaoke: hece zamanlama editörü (\k düzenleme, ses üzerinde), karaoke şablon (templater) arayüzü, lead-in/out.
3. Typesetting: önizlemede \pos/\clip/\move sürükle-bırak, \org, vektör klip çizimi, tag blok yardımcıları, çizim (\p) önizleme.
4. Zamanlama: oynatma + kısayol akışı (Aegisub tuşları), sahne değişimi, timing post-processor, SE "visual sync".
5. Medya: gerçek oynatma (mpv/libmpv), ses oynatma, spektrogram, anahtar kare/timecodes dosyaları.
6. Subtitle Edit özellikleri: yazım denetimi (hunspell tr/en), çeviri modu (iki sütun), OCR'siz import (SUP hariç),
   toplu dönüştürme, birleştirme/bölme, kısaltma/akıllı bölme, ağ-bağımsız otomatik çeviri taslağı kancası.
7. Font: eksik font denetimi, font toplama/gömme (attachments), stil içe/dışa aktarma, çözünürlük yeniden örnekleme.
8. Paketleme: PyInstaller (AppImage/exe), kurulum betikleri, ayarlar penceresi, çoklu dil (tr/en), kısayol özelleştirme.
9. Performans: 50k+ satır, sanal tablo, kısmi yeniden çizim, Lua köprüsünde toplu dönüşüm.

## Gece sürüm kuralı
Her gece: `docs/CHANGELOG.md`'ye yeni sürüm bölümü ekle, `osisub/__init__.py` + `pyproject.toml` sürümünü yükselt,
testler yeşilken commit + push (`claude/vibrant-clarke-toha74`). Sabah 06:00'da bitir.
