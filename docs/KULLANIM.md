# osisub — hızlı başlangıç

## Kurulum ve çalıştırma

```
cargo build --release                     # target/release/osisub-gui ve osisub
target/release/osisub-gui film.ass --media film.mkv
```

Gereken harici araçlar: **ffmpeg/ffprobe** (video önizleme, ses dalga formu, anahtar kareler; PATH'te olmalı, ya da `OSISUB_FFMPEG`/`OSISUB_FFPROBE`). Linux'ta ses için ALSA, GUI için `libxkbcommon-x11`. Yazım denetimi için `tr_TR` sözlüğü: *Yazım* sekmesi → "tr_TR indir" (curl kullanır) ya da kendi `.dic` dosyanızı seçin.

## İş akışları

**Çeviri.** Altyazıyı aç → *Dosya → Referans altyazı aç…* ile İngilizce kaynağı yükle (ızgarada ve düzenleme panelinde eşleşen satır görünür). Metni yaz: yazım hataları kırmızı altçizgiyle görünür, karakter/okuma hızı sayacı canlıdır. `Ctrl+Enter` yeni satır, `Alt+↑/↓` satırlar arası gezinme.

**Zamanlama.** *Video → Video / ses aç…* Dalga formunda başlangıç/bitiş tutamaçlarını sürükle (kareye ve anahtar kareye yapışır), sağ tık = bitiş, Shift+tık = başlangıç; `F5` satırı video+sesle oynatır, `F6/F7` playhead'i başlangıç/bitiş yapar. Sıfırdan başlıyorsan *Zamanlama → Sesten satır oluştur…* konuşma bölgelerini boş satır olarak çıkarır. Toplu düzeltme: *Zamanlama → Son-işleyici* (lead-in/out, bitişik satırlar, anahtar kare).

**Kalite kontrol.** *Kalite* sekmesi (okuma hızı, süre, satır uzunluğu, çakışma, bilinmeyen stil…), *Araçlar → Sık hataları düzelt…* (bozuk Türkçe karakter onarımı, noktalama boşlukları, cümle başı büyük harf, işitme engelli notları…), *Yazı tipi denetimi…* (eksik fontlar), *İstatistikler…*

**Karaoke.** *Karaoke* sekmesi: hece çubuğunu sürükle, hece metnine `|` yazarak böl, "Süreye ölçekle/Harfe göre dağıt", ya da *Tuşla zamanla* (oynarken her hecenin başında Boşluk).

**Tipografi.** *Video → Video aracı:* Konum (`\pos`), Kırpma (`\clip`), Döndürme (`\frz`), Ölçek. Önizleme libass ile gerçek çıktıyı gösterir. Stiller sekmesinde görsel stil düzenleyici, kataloğa kaydetme ve dosyadan içe aktarma.

**Lua betikleri.** `~/.config/osisub/automation/autoload/*.lua` içine koyun ya da *Otomasyon → Lua betiği yükle…* ile yükleyin; makrolar *Otomasyon* menüsünde. Hata ve `aegisub.log` çıktısı *Otomasyon* sekmesindeki Günlük'te. Ayrıntı: `LUA_UYUMLULUK.md`.

**Komut satırı.** `osisub --help`: dönüştürme, toplu dönüştürme, düzeltmeler, zamanlama, bul/değiştir, yeniden örnekleme, yazı tipi kontrolü, Lua makrosu çalıştırma.

## Dosyalar

- Ayarlar: `~/.config/osisub/settings.json` (Windows: `%APPDATA%\osisub`)
- Otomatik kayıt (60 sn): `~/.local/share/osisub/autosave/`
- Kayıtta ilk üzerine yazmada `dosya.ass.bak` yedeği alınır.
