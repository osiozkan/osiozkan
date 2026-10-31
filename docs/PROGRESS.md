# YoruSub – ilerleme günlüğü

> Gece çalışmaları buradan devam eder. Her oturum: bu dosyayı oku → "Sıradaki" listesinden al → test et → commit/push → günlüğü güncelle.
> Mesai: gece boyunca, **her sabah 06:00'da biter**. Dal: `claude/vibrant-clarke-wibwnd`.
> Çalıştırma: `python -m venv .venv && .venv/bin/pip install -e .[dev] && QT_QPA_PLATFORM=offscreen pytest`

## Mimari
| Katman | Yer | Not |
|---|---|---|
| ASS modeli | `yorusub/core/ass.py` | Bilinmeyen bölümler (Fonts, Garbage, Extradata) kayıpsız korunur; cs hassasiyeti |
| Biçimler | `core/formats.py` | ASS/SRT/VTT (HTML etiketleri ↔ ASS) |
| Zamanlama | `core/timing.py` | kaydır, kareye oturt, çakışma, asgari süre, keyframe snap |
| QC | `core/qc.py` | CPS/CPL/satır/çakışma/boşluk/etiket/stil |
| Lua | `yorusub/lua/` | LuaJIT 2.1 (lupa). `subs` = `newproxy` userdata (LuaJIT'te `#` için şart), `aegisub.*`, `include`, `require 're'`, `util`, `unicode`, `karaskel` |
| CLI | `yorusub/cli.py` | convert, qc, shift, fix, lua, gui |
| GUI | `yorusub/gui/` | PySide6: tablo, editör, önizleme, stil yöneticisi, bul/değiştir, QC paneli, Lua diyalog köprüsü, geri al/yinele, otomatik kayıt |

## Bilinen kısıtlar / dürüst durum (v0.1.0)
- Kullanıcının kendi Lua betikleri **henüz bu depoda yok** → gerçek betiklerle uyumluluk testi yapılamadı. `scripts/` altına konursa test paketine eklenecek.
- `karaskel.lua` kendi yazımımız (alt küme): furigana, `line.kara` hece ölçüleri var; tam eşitlik doğrulanmadı.
- `aegisub.re` Python `re` ile taklit; dönüş yapısı (gruplar) belgeye göre yaklaşık.
- Video, dalga formu, ses, gerçek libass render **yok**. Önizleme basit Qt çizimi (etiket yok).
- "Geçmişte karşılaşılan altyazı sorunları" listesi bu depoya aktarılmadı; kullanıcıdan istenmeli (aşağıda).

## Sıradaki (öncelik sırasıyla)
1. Kullanıcı Lua betiklerini `scripts/` altında topla, her biri için yükleme + çalıştırma testi (gerçek uyumluluk).
2. Karaoke şablonlayıcı (`kara-templater` uyumu: `template line/syl/char`, `code`, `fx`, `retime`).
3. libass ile video üstü render (python-mpv / ffmpeg) + önizleme etiketleri (`\pos \an \fad \c \b \i \frz`).
4. Ses dalga formu + spektrum, ses tabanlı zamanlama, tek tuşla satır başlat/bitir (Aegisub tarzı).
5. Anahtar kare (keyframe) içe aktarma, "scene snap", otomatik zamanlama (VAD).
6. Subtitle Edit paritesi: otomatik çeviri (offline sözlük/LLM), OCR (bitmap altyazı), Netflix/BBC kalite kuralları, birleştir/böl akıllı, SSA→SRT stil kaybı uyarıları, toplu dönüştürme, İngilizce→Türkçe `HI` etiket temizleme.
7. Çok dosyalı sekmeler, paylaşılan stil kütüphanesi, font eksiği denetimi ve gömme (Fonts bölümü).
8. Performans: 50k satırlık dosyada tablo/undo (snapshot yerine diff tabanlı geri al), tembel QC.
9. Paketleme: PyInstaller ile Windows/macOS/Linux ikilileri; CI.

## Günlük
- **v0.1.0 (ilk gece)**: depo boştu; çekirdek, Lua çalışma zamanı, CLI, GUI iskeleti ve 13 test yazıldı (hepsi geçiyor).
