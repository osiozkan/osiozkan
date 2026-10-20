# Yuzu 柚子

Sıfırdan yazılmış anime altyazı aracı: **Aegisub**'un iş akışı + **Subtitle Edit**'in araçları, Aegisub uyumlu **Lua otomasyonu** ile.

* ASS/SSA tam desteği (stil, katman, karaoke, etiketler), SRT / WebVTT içe-dışa aktarma
* **Video oynatıcı (libmpv):** kare doğruluğunda atlama, sesle eşzamanlı oynatma, düzenlenen altyazı videoda **canlı** (libass); libmpv yoksa ffmpeg ile duraksama karesi
* Zamanlama görünümü: **dalga formu veya spektrum** (mel ölçekli; konuşma başlangıçları ve heceler belirgin), anahtar kareler, yakınlaşınca tek tek kareler; Aegisub fare ve klavye düzeni
* Aegisub Lua otomasyonu: `aegisub.register_macro/filter`, `karaskel`, `utils`, `unicode`, `re`, `aegisub.util`, `aegisub.dialog`, DependencyControl uyumluluk katmanı
* QC (CPS/CPL/çakışma/stil/etiket), zamanlama post-işlemcisi, bul-değiştir (regex), yeniden ölçekleme
* Aegisub araçları: satır kopyala/kes/yapıştır ve **üstüne yapıştır**, **satır seç**, **stil deposu** (katalog), **çoklu değiştir** (kayıtlı kurallar), **çeviri ve stil asistanı**, görsel dizgi araçları (konum/`\move`, merkez, döndürme Z ve XY, ölçek, dikdörtgen ve **vektör clip**), zamanlama görünümünde **karaoke hece sınırları**
* **Osi'nin çeviri araçları:** referans altyazı sütunu ve kayma bulma, **kaynakla karşılaştırma** (EKSİK / FAZLA / İNGİLİZCE? / ZAMAN … bayrakları, HTML-CSV rapor), **sahne sahne senkron** (referansa ya da sesteki konuşmaya göre), **terim denetimi** (`terimler.tsv`, Türkçe ekleri koruyarak düzeltir), Osi CPL kuralı (42; `\N` parçaları ayrı)
* **Türkçe arayüz** (İngilizce seçilebilir), **komut paleti** (Ctrl+Shift+P; Türkçe karakter yazmadan da bulur), **Ayarlar** (Ctrl+,) ve **kısayol düzenleyici**
* Rust çekirdeği: hızlı, tek ikili dosya, Windows/Linux/macOS

## Derleme

```
cd yuzu
cargo build --release -p yuzu-gui -p yuzu-cli
```

`yuzu-gui` arayüzdür, `yuzu` komut satırı aracıdır (`yuzu run betik.lua Makro in.ass out.ass` ile Lua betiklerinizi başsız çalıştırabilirsiniz).

### Video ve ses için gerekenler

* **libmpv** (önerilir — oynatma, kare doğru atlama, canlı altyazı):
  * Windows: `libmpv-2.dll` dosyasını `yuzu-gui.exe` ile aynı klasöre koyun ([mpv-winbuild](https://github.com/shinchiro/mpv-winbuild-cmake/releases) `mpv-dev-x86_64-….7z` içinden).
  * Linux: dağıtımın paketi (`libmpv2` / `mpv-libs`). macOS: `brew install mpv`.
  * Başka bir yer için: `YUZU_LIBMPV=/yol/libmpv.so`. Hangisinin kullanıldığı *Yardım → Yuzu hakkında*'da yazar.
* **ffmpeg / ffprobe** (dalga formu, spektrum, anahtar kareler; libmpv yoksa önizleme ve `ffplay` ile ses): PATH'te veya çalıştırılabilir dosyanın yanında.

## Çeviri araçları (Çeviri menüsü)

* **Referans paneli** — İngilizce altyazıyı yanda açın (`yuzu-gui ceviri.ass --reference en.srt` ile de olur).
  *Kaymayı bul* iki dosya arasındaki sabit kaymayı bulur; ızgaradaki Referans sütunu ve düzenleyicinin üstündeki
  satır bu kaymayla eşleşir. *Referans metnini belgeye aktar* boş bir çeviri dosyasını İngilizce metinle doldurur.
* **Kaynakla karşılaştır** — satır eksik mi, fazla mı, çevrilmemiş mi, zamanı kaymış mı? Bayraklı satırları seçin,
  eksikleri yorum olarak ekleyin, HTML/CSV rapor ya da işaretli kopya alın.
* **Referansa / konuşmaya eşitle** — farklı kaynaktan (TV/BD/web) gelen çevirinin zamanlarını sahne sahne düzeltir.
  Önce plan gösterilir; *Uygula* tek adımda geri alınabilir.
* **Terim denetimi** — betiğin yanına `terimler.tsv` koyun:

  ```
  kaynak	türkçe	yasaklar	not
  Stand	Stand	Stant|Ruh
  Hokage	Hokage	Ateş Gölgesi
  ```

Aynı araçlar komut satırında: `yuzu compare kaynak.srt ceviri.ass -o rapor.html`,
`yuzu sync-ref en.srt ceviri.ass cikti.ass`, `yuzu glossary terimler.tsv ceviri.ass --ref en.srt --fix duzeltilmis.ass`.

## Lua betikleri

Betikler şu klasörlerden otomatik yüklenir: `~/.config/yuzu/automation/autoload`, `~/.aegisub/automation/autoload`, `%APPDATA%\Aegisub\automation\autoload` ve Yuzu'nun kendi `automation/autoload` klasörü. *Otomasyon → Betik klasörü ekle…* ile yenisini ekleyebilirsiniz.

* Bir betik ya da `include` içindeki bir kütüphane diskte değişince betikler **kendiliğinden yeniden yüklenir**.
* Hatalar **yığın iziyle** gelir; *Otomasyon → Otomasyon yöneticisi* her betiğin sürümünü, yazarını, makrolarını ve hatasını gösterir.
* Makronun doğrulama işlevi "hayır" diyorsa menüde gri görünür ve kısayol/paletle de çalıştırılmaz; `is_active` işaretli gösterir.
* DependencyControl betikleri çevrimdışı bir uyumluluk katmanıyla çalışır (modül listesi, `getConfigHandler`, alt menüler); satırların `extra` verisi (Aegisub Extradata) korunur.

Ayrıntılar ve desteklenen API: [`automation/README.md`](automation/README.md).

## Kısayollar

Hepsi *Düzen → Ayarlar → Kısayollar* bölümünden değiştirilebilir (bir komuta iki kısayol; çakışma varsa kaydetmez).
Her komut **Ctrl+Shift+P** ile adından da aranabilir (`:123` → 123. satır).

| Kısayol | İşlev |
|---|---|
| Ctrl+Shift+P / Ctrl+, | komut paleti / Ayarlar |
| Ctrl+N / O / S / Shift+S | yeni / aç / kaydet / farklı kaydet |
| Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y) | geri al / yinele |
| Ctrl+C / Ctrl+X / Ctrl+V | satırları kopyala / kes / yapıştır (etkin satırın üstüne) |
| Ctrl+Shift+V | üstüne yapıştır (alanlar seçilir) |
| Ctrl+F (Ctrl+H) | bul ve değiştir |
| Ctrl+I | zamanları kaydır |
| Ctrl+Enter | sonraki satır (sonda yeni satır) |
| Ctrl+D | satırı çoğalt |
| F1 / F2 | satır başlangıcı / sonu = ekrandaki kare (santisaniye yuvarlamasından sonra da tam o kare) |
| F3 / F4 | satırı oynat / bitişi oynat |
| Ctrl+Space | oynat/duraklat |
| Ctrl+←/→ | kare geri/ileri (Shift ile 1 sn) |

**Zamanlama tuşları** (Aegisub ile aynı; metin kutusu odakta değilken):

| Tuş | İşlev |
|---|---|
| S veya R | satırı oynat (sonunda durur) |
| Q / W | başlangıçtan önceki / bitişten sonraki 500 ms |
| E / D | satırın ilk / son 500 ms'si |
| T | oynatma imlecinden satır sonuna |
| Space / H | oynat-duraklat / durdur |
| Z / X | önceki / sonraki satır |
| G veya Enter | sonraki satır (sondaysa bu satırın bittiği yerden yeni satır) |
| 1 / 2 | başlangıç / bitiş = oynatma imleci |
| A / F | zamanlama görünümünü sola / sağa kaydır |

*Karaoke* düğmesi açıkken etkin satırın heceleri görünür: hece sınırı sürüklenir, heceye tıklamak onu çalar.

Video üstündeki araçlar (Konum, Merkez, Döndür Z/XY, Ölçek, Kırp, VClip): Shift konumda ekseni kilitler, döndürmede
15°'lik adım verir, ölçekte oranı korur. VClip'te tıklama nokta ekler, sürükleme taşır, sağ tık siler.

Zamanlama görünümünde sol tık başlangıcı, sağ tık bitişi koyar; kenarlar sürüklenir (anahtar karelere yapışır, Shift tersine çevirir); Ctrl+tık veya orta tık oynatma imlecini taşır; tekerlek kaydırır, Ctrl+tekerlek yakınlaştırır.

Sürüm geçmişi: [`CHANGELOG.md`](CHANGELOG.md) — yol haritası: [`ROADMAP.md`](ROADMAP.md).

## Tüm kodu tek dosyada almak

`python tools/bundle.py` bütün kaynak ağacını `dist/Yuzu_<sürüm>_tum_kod.txt` dosyasına yazar; dosyanın başındaki
GERİ AÇMA bloğu (veya `python tools/bundle.py --unpack dosya.txt`) ağacı bayt bayt aynı şekilde geri kurar.
