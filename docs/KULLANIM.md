# osisub — hızlı başlangıç

## Kurulum ve çalıştırma

```
cargo build --release                     # target/release/osisub-gui ve osisub
target/release/osisub-gui film.ass --media film.mkv
```

Gereken harici araçlar: **ffmpeg/ffprobe** (video önizleme, ses dalga formu, anahtar kareler; PATH'te olmalı, ya da `OSISUB_FFMPEG`/`OSISUB_FFPROBE`). Linux'ta ses için ALSA, GUI için `libxkbcommon-x11`. Yazım denetimi için `tr_TR` sözlüğü: *Yazım* sekmesi → "tr_TR indir" (curl kullanır) ya da kendi `.dic` dosyanızı seçin.

## İş akışları

**Çeviri.** *Çeviri* sekmesi: referans büyük gösterilir, çeviri kutusunda Enter kaydedip sonraki satıra geçer, "Sonraki boş satır" sesten oluşturulan boş satırlara atlar. Altyazıyı aç → *Dosya → Referans altyazı aç…* ile İngilizce kaynağı yükle (ızgarada ve düzenleme panelinde eşleşen satır görünür). Metni yaz: yazım hataları kırmızı altçizgiyle görünür, karakter/okuma hızı sayacı canlıdır. `Ctrl+Enter` yeni satır, `Alt+↑/↓` satırlar arası gezinme. **Çeviri belleği:** referans satırına benzeyen eski çeviriler (%60 ve üzeri) çeviri kutusunun üstünde "TM %" ile önerilir; "Kullan" metni yerleştirir. Enter ile geçtiğiniz her satır belleğe öğretilir; geçmiş bölümleri *Araçlar → Çeviri belleği → Bu belgeyi belleğe ekle* ile toplu ekleyebilirsiniz. Bellek ekipçe TSV olarak paylaşılabilir.

**Zamanlama.** *Video → Video / ses aç…* Dalga formunda başlangıç/bitiş tutamaçlarını sürükle (kareye ve anahtar kareye yapışır), sağ tık = bitiş, Shift+tık = başlangıç; `F5` satırı video+sesle oynatır, `F6/F7` playhead'i başlangıç/bitiş yapar. Sıfırdan başlıyorsan *Zamanlama → Sesten satır oluştur…* konuşma bölgelerini boş satır olarak çıkarır. Toplu düzeltme: *Zamanlama → Son-işleyici* (lead-in/out, bitişik satırlar, anahtar kare). Satıra tıklayınca video, satırın göründüğü ilk kareye gider; `F6/F7`, yapışma ve anahtar kare işlemleri santisaniye yuvarlamasına dayanıklı kare sınırları kullanır (satır bir kare erken/geç başlamaz). Değişken kare hızlı (VFR) videolar otomatik algılanır; elle *Video → Timecode dosyası aç* da olur. Kare kare iş için *Zamanlama → Video karesinde böl* ve *Çoğalt ve 1 kare kaydır*; düzenleme kutusunda sağ tık → *İmleçte böl*.

**Kalite kontrol.** *Kalite* sekmesi (okuma hızı, süre, satır uzunluğu, çakışma, bilinmeyen stil…), *Araçlar → Sık hataları düzelt…* (bozuk Türkçe karakter onarımı, noktalama boşlukları, cümle başı büyük harf, işitme engelli notları…), *Yazı tipi denetimi…* (eksik fontlar), *İstatistikler…*. Video açıkken Kalite sekmesi sahne değişimine 1-3 kare yakın başlayan/biten satırları ve 1-2 karelik titremeleri de bulur; *Sahne değişimlerine yapıştır* tek tıkla düzeltir. Tabela/tipografi (`\pos`, `\move`, `\clip`…), karaoke ve fx satırları okuma hızı/uzunluk uyarısı almaz. *Dosya → Başka sürümle karşılaştır…* çevirmen/editör sürümleri arasındaki farkları listeler. *Düzen → Satırları seç…* stil/konuşan/efekt/metne göre seçer.

**Karaoke.** *Karaoke* sekmesi: hece çubuğunu sürükle, hece metnine `|` yazarak böl, "Süreye ölçekle/Harfe göre dağıt", ya da *Tuşla zamanla* (oynarken her hecenin başında Boşluk). Karaoke etiketi olmayan Japonca sözler için *Japonca hecele (romaji/kana)* mora kurallarıyla böler. Dalga formunda etkin karaoke satırının heceleri sarı çizgilerle görünür; çizgileri sürükleyerek zamanlayın. Efekt için Aegisub şablonları (`template syl`, `code once`, `!retime()!` …) *Otomasyon → Apply karaoke template* ile çalışır (gerçek kara-templater yerleşiktir).

**Tipografi.** *Video → Video aracı:* Konum (`\pos`), Hareket (`\move`: daire = başlangıç, kare = bitiş, boş alan = ikisini taşı, sağ tık = `\pos`a çevir), Kırpma (`\clip` dikdörtgen), Vektör kırpma (`\clip` çizimi: tıkla = nokta ekle, sürükle = taşı, sağ tık = sil), Döndürme (`\frz`), 3B döndürme (`\frx/\fry`), Ölçek; döndürmede beyaz nokta `\org`'u taşır. Videoda Ctrl+tekerlek yakınlaştırır, orta tuş kaydırır. Önizleme libass ile gerçek çıktıyı gösterir; *Araçlar → Ekler* ile yazı tiplerini ASS'e gömebilirsiniz. Stiller sekmesinde görsel stil düzenleyici, kataloğa kaydetme ve dosyadan içe aktarma.

**Lua betikleri.** `~/.config/osisub/automation/autoload/*.lua` içine koyun ya da *Otomasyon → Lua betiği yükle…* ile yükleyin; makrolar *Otomasyon* menüsünde. MoonScript (`.moon`) betikleri de çalışır; `require` `automation/include` klasörüne bakar. Hata olursa hatalı satırı gösteren bir pencere açılır; `aegisub.log` çıktısı *Otomasyon* sekmesindeki Günlük'te. Makrolara (Otomasyon sekmesi) ve yerleşik komutlara (*Yardım → Kısayolları düzenle…*) kısayol atanabilir. Ayrıntı: `LUA_UYUMLULUK.md`.

**Komut satırı.** `osisub --help`: dönüştürme, toplu dönüştürme, düzeltmeler, zamanlama, bul/değiştir, yeniden örnekleme, yazı tipi kontrolü, Lua makrosu çalıştırma.

## Dosyalar

- Ayarlar: `~/.config/osisub/settings.json` (Windows: `%APPDATA%\osisub`)
- Otomatik kayıt (60 sn): `~/.local/share/osisub/autosave/`
- Kayıtta ilk üzerine yazmada `dosya.ass.bak` yedeği alınır.
