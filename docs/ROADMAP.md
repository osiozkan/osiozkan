# nightsub — yol haritası

Hedef: Aegisub'ın tüm yeteneklerini + Subtitle Edit'in iyi özelliklerini tek, hızlı ve kolay arayüzlü
bir anime altyazı aracında toplamak; mevcut Aegisub Lua (Automation 4) betikleri değişmeden çalışmalı.

Teknoloji: Rust (çekirdek + CLI + GUI), LuaJIT (mlua, vendored) — Aegisub'ın kullandığı Lua ile aynı sınıf.

## Durum

| Alan | Durum |
|---|---|
| ASS/SSA okuma-yazma (SSA v4 dönüşümü, ek bölümler korunur) | ✅ |
| SRT / WebVTT / MicroDVD / LRC içe-dışa aktarım, kodlama tespiti (Türkçe cp1254 dahil) | ✅ |
| Override tag ayrıştırıcı | ✅ (temel) |
| Zaman araçları: kaydır, iki noktalı senkron, fps dönüşümü, böl/birleştir, bul-değiştir | ✅ |
| Kalite kontrol (CPS, süre, satır uzunluğu, çakışma, bilinmeyen stil, ...) + otomatik düzeltme | ✅ |
| Türkçe-duyarlı büyük/küçük harf (İ/ı) | ✅ |
| Lua Automation 4: `aegisub.*`, `subs` nesnesi, macro/filter kaydı, dialog (headless), `re`, utils, karaskel, unicode | ✅ (çekirdek) |
| DependencyControl uyumluluk katmanı | 🟡 minimal shim |
| CLI `nightsub` | ✅ |
| Gerçek font ölçümü (`aegisub.text_extents`) | ⬜ şu an sezgisel |
| Kara Templater (`kara-templater.lua` eşdeğeri), `\t`/`\fad` yardımcıları | ⬜ |
| Aegisub standart otomasyon betikleri (clean tags, strip tags, vb.) | ⬜ |
| GUI: olay listesi, metin/zaman düzenleme, stil yöneticisi, makro menüsü | ⬜ |
| Video önizleme (libass), ses dalga formu/spektrum, zamanlama modu | ⬜ |
| Otomatik kaydetme/yedek, geri al/yinele | ⬜ |
| Yazım denetimi (Türkçe Hunspell), çeviri modu | ⬜ |
| Toplu dönüştürme, projeler, ayarlar | ⬜ |

## Her gece için kural
1. `docs/NIGHTLY.md` sonuna o gecenin kaydını ekle (ne yapıldı, ne kaldı, bilinen sorunlar).
2. `cargo test --workspace` yeşil olmadan push etme.
3. Sabah 06:00'ya (TR) kadar çalış; bitince özet bildirimi gönder.
