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

## Gece 1'de tamamlananlar (ek)
Görsel araçlar (konum/Z döndür/ölçek/orijin/kırp), spektrogram, çeviri modu (referans sütunu), ayarlar penceresi,
"sık hataları düzelt", ASS round-trip testleri, performans düzeltmeleri (20k satırlık Lua geçişi 0.75 sn).

## Sıradaki işler (öncelik sırasıyla)
1. Görsel araçlar: `\move`, vektör `\clip` (çizim), `\frx/\fry`, sürükle-tutamaçlar (şu an yalnız tek tıkla-sürükle); `\t` canlı doğrulama
2. Ses oynatma / seçim oynatma, dalga formunda ekleme/bölme kısayolları (Aegisub ses kutusu iş akışı: Enter=uygula, G=ses seçimini satıra ata…)
3. Çeviri modu: iki belge arasında satır eşleme düzenleme, "orijinali kopyala" kısayolu
4. Kısayol özelleştirme, çoklu dil (TR/EN) arayüz, yazım denetimi (Türkçe hunspell/`spylls`)
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
