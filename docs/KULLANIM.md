# nightsub — Kullanım Kılavuzu

## Kurulum
- Sürümler: GitHub Actions → `release` iş akışı → artifact (`nightsub-gui`, `nightsub`).
- Video/ses için sisteminizde **ffmpeg** (ffprobe, ffplay) PATH'te olmalı.
- Kaynaktan derleme: `cargo build --release -p nsub-gui -p nsub-cli` (Rust gerekir; Linux'ta `libxkbcommon-x11`, `libwayland`, `libgl` geliştirme paketleri).

## Arayüz
- **Dosya → Aç** (ASS, SSA, SRT, VTT, SUB, LRC); sürükle-bırak da çalışır (.lua/.moon sürüklerseniz makro olarak yüklenir).
- Satırı seçince video o satırın başına gider; altta dalga formunda **yeşil çizgi = başlangıç**, **kırmızı = bitiş**; sürükleyerek zamanlayın (mor çizgiler anahtar kareler, yapışır).
- **Boşluk**: satırı çal/durdur · **Ctrl+Z / Ctrl+Shift+Z**: geri al/yinele · **Ctrl+F**: bul/değiştir · **Ctrl+D**: çoğalt · **Del**: sil · **↑/↓**: satır değiştir.
- Metin kutusunda **Shift+Enter** → `\N` (satır sonu).
- **Görünüm → Language / Dil**: Türkçe veya İngilizce.
- **Dosya → Kaynak altyazı aç**: çeviri yaparken orijinal altyazıyı (örn. İngilizce) satırların yanında gösterir.

## Lua / MoonScript betikleri
- Mevcut Aegisub klasörünüz otomatik taranır: Windows `%APPDATA%\Aegisub\automation\autoload\*.lua|*.moon` ve `include\`.
- Ek klasör: `~/.nightsub/automation/autoload` (Windows: `%USERPROFILE%\.nightsub\automation\autoload`).
- **Otomasyon** menüsünden makroları çalıştırın; diyaloglar gerçek pencere olarak açılır, ilerleme çubuğu ve İptal vardır.
- Komut satırı: `nightsub run dosya.ass --script betik.lua --macro "Makro Adı" -o cikti.ass --answer ad=değer`.

## Komut satırı (`nightsub`)
```
nightsub info dosya.ass
nightsub convert dosya.srt dosya.ass
nightsub check dosya.ass --cps 17 --line-len 42
nightsub fix dosya.ass -o temiz.ass --overlaps --min-dur 700 --cps 17 --spaces --hi --case sentence --balance 42
nightsub resample dosya.ass 1920 1080 -o buyuk.ass
nightsub fonts dosya.ass --copy yazitipleri/
nightsub select dosya.ass "merhaba" --field plain
```
