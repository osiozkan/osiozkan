# SubForge – Hızlı Başlangıç (Türkçe)

SubForge; Aegisub uyumlu bir ASS altyazı editörü + Subtitle Edit tarzı toplu araçlar + Lua (Automation 4) desteğidir.
Tek bir program dosyasıdır (`subforge`). Çift tıklayın ya da terminalde `subforge` yazın: tarayıcınızda editör açılır
(`http://127.0.0.1:8765`). Veriler yalnızca sizin bilgisayarınızda kalır.

## 1. Açma / kaydetme
* **Gözat…** diskinizdeki .ass/.ssa/.srt/.vtt dosyasını açar. Kodlama otomatik bulunur (eski Türkçe dosyalar için
  *Windows‑1254* dahil). Gerekirse üstteki **Aç: …** kutusundan kodlamayı elle seçin.
* **Kaydet** dosyanın üzerine yazar (önceki sürüm `dosya.ass.bak` olarak kalır). **Farklı kaydet…** yeni ad sorar.
  Kayıt kodlaması ve biçimi (ass/srt/vtt/txt) araç çubuğundan seçilir.
* Çökme/elektrik kesintisinde **Kurtar…** son otomatik kayıtları gösterir.

## 2. Zamanlama
* **Video…** ile video/ses seçin: dalga formu (waveform) ffmpeg ile çıkarılır; kare hızı ve keyframe’ler okunur.
* Dalga formunda satırın kenarını sürükleyin = zamanı değiştir (**Alt** ile keyframe’e yapışır). Sağ tık = bitiş, Shift+tık = başlangıç.
* `[` ve `]` oynatma noktasını başlangıç/bitiş yapar, `,` `.` bir kare geri/ileri, `P` seçili satırı çalar.
* **Araçlar…**: kaydırma (ms/kare), kare hızı dönüştürme, iki noktalı senkron, giriş/çıkış payı, zincirleme, min. boşluk, sıralama.

## 3. Metin / çeviri
* **Referans…** ile kaynak dil altyazısı yükleyin; seçili satırın üstünde zaman olarak örtüşen kaynak satır görünür.
* **Araçlar → Metin temizliği**: sık hatalar, [ses]/♪/KONUŞMACI: temizleme, **Türkçe i/İ kurallı** büyük/küçük harf, uzun satırı dengeleme, böl/birleştir.
* Metin kutusunun yazım denetimi dili üstteki **TR/EN…** kutusundan seçilir (tarayıcının yazım denetimini kullanır).
* **Kontroller** sekmesi: CPS, süre, çakışma, satır uzunluğu, eksik stil ve **fontta olmayan karakterler** (ş, ğ, İ, ı kutucuk çıkmasın diye).

## 4. Dizgi ve karaoke
* **✥ Dizgi**: önizlemede sürükleyin = `\pos`, Shift+sürükleyin = `\clip`. **libass ▶** kareyi birebir libass ile çizer.
* **Karaoke** sekmesi: seçili satırları hecelere böler (romaji/Türkçe/karakter/kelime), süreleri santisaniye olarak düzenlersiniz;
  **Örnek şablonları ekle** + **Karaoke şablonunu uygula** Aegisub’un kara‑templater’ını (programın içinde hazır) çalıştırır.
* **Stiller…** pencerede canlı önizleme vardır. **Araçlar → Çözünürlük** stilleri/etiketleri/çizimleri yeni çözünürlüğe ölçekler.

## 5. Kendi Lua betikleriniz
* **⚙ Ayarlar**: *Automation otomatik yükleme klasörleri* (ör. Aegisub’daki `automation/autoload`), *include klasörleri*
  (`automation/include`) ve tek tek betikleri ekleyin; her açılışta yüklenir.
* **Lua makroları** sekmesinde **Çalıştır**: makro seçili satırlarda çalışır; `aegisub.dialog.display` pencereleri
  tarayıcıda açılır; hata olursa belge olduğu gibi geri alınır (Aegisub gibi).
* Terminalden: `subforge run dosya.ass betik.lua "Makro adı" -p alan=değer`.

## 6. Komut satırı
`subforge --help` — `info, convert, shift, fps, resync, snap, qa, fix, replace, resample, fonts, clean, stats, macros, run, serve`.
Örnek: `subforge fonts bolum.ass --collect fontlar/` (kullanılan fontları klasöre toplar, eksikleri bildirir).

## Kısayollar
Ctrl+Z/Y geri/ileri · Ctrl+S kaydet · Ctrl+F bul/seç · F3 sonraki · Ctrl+G satıra git · Ctrl+A/C/X/V satırlar ·
Ctrl+D çoğalt · Del sil · Ctrl+Shift+C yorum satırı · Ctrl+Enter sonraki satır · Shift+Enter metinde `\N`.
