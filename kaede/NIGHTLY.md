# Gece notları (Kaede)

Her gece 06:00'a kadar yeni sürüm hazırlanır. Bu dosya kaldığı yeri ve sıradaki işleri tutar.
**Kural:** her oturum başında bu dosyayı + `git log` oku, testleri çalıştır (`python3 -m pytest -q`,
GUI için `QT_QPA_PLATFORM=offscreen`), sonra aşağıdaki listeden devam et. Bitirince CHANGELOG'u ve bu dosyayı güncelle,
`claude/vibrant-clarke-h410jk` dalına commit+push et.

Ortam notu: sandbox'ta GUI testi için `libegl1 libgl1 libxkbcommon0 libfontconfig1 libdbus-1-3` kurulmalı;
`python3 -m pip install lupa PySide6 numpy pytest` (Pillow sistemde bozuk, kullanma — Qt ile PNG kaydet).

## Durum (Gece 1 — 2026-11-03)
Sıfırdan yazıldı: çekirdek (ASS/SRT/VTT/SBV, araçlar, QC, geri-al), LuaJIT otomasyon motoru (Aegisub API v4),
Qt GUI (tablo, düzenleme paneli, dalga formu, libass önizleme, video oynatıcı, diyaloglar), CLI, medya (ffmpeg).
Not: depoda önceki bir Kaede/altyazı geliştirmesi bulunmuyordu (sadece profil README'si); önceki sohbetlerdeki
sorun listesi bu oturuma aktarılmadığı için bilinen sorunlar aşağıya "Kullanıcı sorunları" olarak eklenmeli.

## Sıradaki işler (öncelik sırasıyla)
1. **Görsel yazı-dizgi araçları** (videoda `\pos`/`\an`/`\frz`/`\fscx` sürükle, `\clip`, `\org`, çizim) — Aegisub'ın en büyük eksik parçası
2. Ses **spektrogram** görünümü + ses oynatma/seçim oynatma, dalga formunda ekleme/bölme kısayolları (Aegisub ses kutusu iş akışı)
3. **Çeviri modu**: orijinal/çeviri iki sütun, referans altyazı yükle, otomatik aynı zaman eşleme
4. **Ayarlar penceresi** (betik klasörleri, QC varsayılanları, kısayol özelleştirme, autosave aralığı, dil)
5. Yazım denetimi (Türkçe hunspell/`spylls`), sık hatalar düzeltici (SE "Fix common errors": "..." → "…", tırnaklar, `-` diyalog çizgisi)
6. Lua: MoonScript (.moon) desteği, `re` için Boost'a daha yakın davranış, furigana ayrıştırma, `aegisub.get_audio_selection`
7. Karaoke araçları (şablon çalıştırıcı için Aegisub `kara-templater` ile uyum testi, yerleşik basit karaoke zamanlayıcı)
8. Paketleme: PyInstaller (Windows .exe / Linux AppImage), GitHub Actions
9. Toplu işlem (batch convert/QC), karşılaştırma (diff) görünümü, bookmark'lar, çoklu belge sekmeleri
10. libass ile yazı tipi eksikliği uyarısı (font fallback görsel gösterimi), `\fad`/`\t` canlı önizleme doğrulaması

## Kullanıcı sorunları (altyazıda karşılaşılanlar) — kontrol listesi
(Oturum sahibi eklesin; her biri için `core/tools.py` QC kuralı veya test eklenecek.)
- Türkçe kodlama bozulmaları (cp1254/UTF-8 karışıklığı, mojibake) → QC `turkish` kuralı + `decode_bytes` mevcut
- I/İ büyük-küçük harf hataları → `tr_upper/tr_lower`, Bul/Değiştir toleransı mevcut
- Kayıpsız ASS: bilinmeyen bölümler/Aegisub Project Garbage korunuyor → testlerle genişlet
