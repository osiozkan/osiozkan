# OsiSub kurulumu — hiçbir şey bilmiyormuş gibi, adım adım (Windows)

## 1. Python'u kur (bir kez)

1. Tarayıcıda **python.org/downloads** adresine git, sarı **Download Python 3.x** düğmesine bas.
2. İnen dosyayı çalıştır. İlk ekranda en alttaki **"Add python.exe to PATH"** kutusunu MUTLAKA işaretle.
3. **Install Now** de, bitince kapat.

> PATH nedir? Windows'un "python" yazınca programı nerede arayacağını bildiği liste.
> O kutuyu işaretlemezsen komutlar "tanınmıyor" hatası verir.

## 2. OsiSub dosyalarını yerleştir

`OsiSub_tum_kod.txt` dosyasından ağacı kurmak için:

1. Bir klasör aç, ör. `C:\OsiSub-kur\`, `OsiSub_tum_kod.txt`'yi içine koy.
2. Txt'nin en üstündeki **GERİ AÇMA** bloğunu (iki `# ----` satırı arası) Not Defteri'ne kopyala,
   aynı klasöre **`unbundle.py`** adıyla kaydet (Kayıt türü: *Tüm dosyalar*).
3. Klasörün adres çubuğuna `cmd` yazıp Enter'a bas — o klasörde siyah bir pencere (komut istemi) açılır.
4. Şunu yaz, Enter:
   ```
   python unbundle.py OsiSub_tum_kod.txt C:\OsiSub
   ```
   `C:\OsiSub` klasörü tüm dosyalarla oluşur.

## 3. Gerekli paketleri kur (bir kez)

`C:\OsiSub` klasöründeki **`KURULUM.bat`** dosyasına çift tıkla.
PySide6 (arayüz), lupa (Lua motoru), regex, numpy, PyAV, python-mpv, spylls (yazım denetimi) ve OpenCV (hareket takibi) kurulur, ardından ortam denetimi çalışır.
Her satırda **[TAMAM]** görmelisin.

## 3b. Video ve ses için iki dosya (bir kez)

Video oynatma, dalga formu ve canlı altyazı önizlemesi için **ffmpeg** ve **libmpv** gerekir.
İkisini de `C:\OsiSub\bin\` klasörüne koyacaksın (klasör yoksa oluştur).

**ffmpeg + ffprobe**
1. **gyan.dev/ffmpeg/builds** adresine git, "release builds" altındaki **ffmpeg-release-essentials.zip**'i indir.
2. Zip'i aç; içindeki `bin` klasöründen **ffmpeg.exe** ve **ffprobe.exe**'yi `C:\OsiSub\bin\` içine kopyala.

**libmpv**
1. **sourceforge.net/projects/mpv-player-windows/files/libmpv/** adresine git.
2. Adı `mpv-dev-x86_64-…` ile başlayan en yeni `.7z` dosyasını indir
   (`-v3-` yazanlar yalnız yeni işlemcilerde çalışır; emin değilsen `v3` OLMAYANI al).
3. `.7z` dosyasını aç (Windows 11 doğrudan açar; açmazsa **7-zip.org**'dan 7-Zip kur).
4. İçindeki **libmpv-2.dll**'i `C:\OsiSub\bin\` içine kopyala.

Sonra `KURULUM.bat`'ı bir kez daha çalıştır: `ffmpeg`, `ffprobe` ve `libmpv` satırları **[TAMAM]** olmalı.
libmpv olmadan da program çalışır, video yalnız kare kare görünür (oynatma ve altyazı önizlemesi olmaz).

## 3c. Türkçe yazım denetimi (isteğe bağlı, bir kez)

Programı açınca **Araçlar > Türkçe sözlüğü indir (~38 MB)**'a tıkla. Sözlük
`%APPDATA%\OsiSub\dictionaries\` klasörüne iner ve kendiliğinden yüklenir (her açılışta ~7 sn,
arka planda). Aegisub'a daha önce Türkçe sözlük kurduysan (`%APPDATA%\Aegisub\dictionaries\tr_TR.dic`)
OsiSub onu doğrudan kullanır, indirmen gerekmez.

## 3d. Whisper — konuşmayı yazıya dökme (isteğe bağlı)

`KURULUM_WHISPER.bat`'a çift tıkla (~200 MB paket). Model ilk kullanımda indirilir
(`%APPDATA%\OsiSub\models\`): **small** ~460 MB (önerilen), **medium** ~1.5 GB, **large-v3** ~3 GB.
NVIDIA ekran kartın varsa CUDA ile çok daha hızlı çalışır (CUDA kurulu değilse işlemci kullanılır).
Kullanım: **Ses > Konuşmayı yazıya dök (Whisper)…** — Japonca dökümü Referans sütununa yükler.

## 3e. Hareket takibi (0.7.0 ile geldi)

0.6.0'dan güncelliyorsan `KURULUM.bat`'ı **bir kez daha** çalıştır (OpenCV eklenir, ~40 MB).
Kullanım: satırı seç → **Video > Hareket takibi…** (Ctrl+Alt+M) → videoda izlenecek alanı sürükleyerek çiz
(satırda dikdörtgen \clip varsa alan oradan alınır) → **İzle** → **Uygula**. Geri al tek adımda döndürür.
Mocha'dan "After Effects Transform Data" kopyaladıysan bölmedeki **AE yapıştır** ile alırsın.

## 3f. Tek klasörlük .exe ve kurulum dosyası (1.0 ile geldi, isteğe bağlı)

3. ve 3b. adımlar tamamsa (ffmpeg.exe, ffprobe.exe, libmpv-2.dll `C:\OsiSub\bin\` içinde):

1. **`BUILD_EXE.bat`**'a çift tıkla (birkaç dakika). Sonunda ortam denetimi çalışır; her satırda **[TAMAM]**.
2. Sonuç: **`C:\OsiSub\dist\OsiSub\OsiSub.exe`**. Bu klasör artık Python'a İHTİYAÇ DUYMAZ —
   başka bir bilgisayara kopyalayıp doğrudan çalıştırabilirsin. Yanındaki `OsiSub-konsol.exe doctor`
   ortam denetimidir.
3. (İsteğe bağlı) **Inno Setup 6** kuruluysa (jrsoftware.org, ücretsiz) aynı betik
   **`dist\OsiSub-<sürüm>-Kurulum.exe`** de üretir: yönetici izni istemez, Başlat menüsüne ve isteğe bağlı
   masaüstüne kısayol koyar, ".ass dosyalarını OsiSub ile aç" seçeneği vardır, Programlar'dan kaldırılır.

Not: Whisper paketli .exe'ye dahil değildir (~1 GB); onu kullanmak için kaynak kurulumla (BASLAT.bat) çalıştır.

## Otomatik yedek ve kurtarma (1.0)

Kaydedilmemiş değişiklik varsa her 60 saniyede bir yedek alınır (`%APPDATA%\OsiSub\autosave\`).
Program çökerse ya da elektrik giderse bir sonraki açılışta **Kurtarma** penceresi çıkar; açtığın yedek
özgün dosyanın üstüne kendiliğinden YAZILMAZ — kontrol edip Kaydet'e basarsın. Yardım menüsünden de açılır.
Beklenmeyen bir hata olursa program kapanmaz: acil yedek alınır ve rapor
`%APPDATA%\OsiSub\logs\cokme-….txt` olarak yazılır (Yardım > Günlük klasörünü aç). Raporu bana gönder.
Aralık `config.json > autosave_seconds` ile değişir (0 = kapalı).

## 4. Çalıştır

**`BASLAT.bat`**'a çift tıkla. Bir `.ass` dosyasını BASLAT.bat'ın üstüne sürükleyip bırakırsan o dosyayla açılır.

## Sık sorunlar

| Belirti | Çözüm |
|---|---|
| `'python' is not recognized` | Python'u PATH kutusu işaretli olarak yeniden kur (1. adım). |
| `lupa (Lua motoru) — Lua (JIT yok…)` | `py -m pip install --upgrade lupa` çalıştır. JIT yoksa çoğu betik yine çalışır, FFI kullananlar (Yutils, ILL…) çalışmaz. |
| Automation menüsünde betiğim yok | Betiği `%APPDATA%\OsiSub\automation\autoload\` içine koy (Win+R → `%APPDATA%\OsiSub` yaz → Enter), sonra Automation → Betikleri yeniden yükle. |
| Video açılıyor ama oynamıyor, "kare gösterici" yazıyor | libmpv-2.dll `C:\OsiSub\bin\` içinde değil (3b). |
| Ses bölmesi boş kalıyor | ffmpeg.exe / ffprobe.exe `C:\OsiSub\bin\` içinde değil (3b). |
| Ses çalmıyor | Windows ses aygıtını kontrol et; Ses bölmesi dalga formunu yine gösterir. |
| Yanlış yazılmış kelimelerin altı çizilmiyor | Araçlar menüsünde "Yazım denetimi: sözlük yok" yazıyorsa sözlüğü indir (3c). |
| Doğru bir özel ismin altı çiziliyor | Kelimeye sağ tıkla → "sözlüğe ekle". Kullanıcı sözlüğü: `%APPDATA%\OsiSub\kullanici_sozlugu.txt` |
| Hareket takibi: "yeterli doku yok" | Alan düz renkli; alanı yazı/kenar içerecek şekilde büyüt ya da yöntemi "Şablon" yap. |
| Hareket takibinde eğride kırmızı çentikler | O karelerde iz güvensiz (örtme, bulanıklık); değerler komşu karelerden doldurulur. Gerekirse aralığı daralt ya da referansı başka kareye al. |
| Örtme bulunmuyor / her yer örtülü çıkıyor | ΔE eşiğini değiştir: ışığı değişen sahnede yükselt (30–40), örten tabelaya renkçe yakınsa düşür (14–18). Örten nesne karelerin yarısından çoğunda aynı yerde duruyorsa "Yalnız referans kare" modelini seç ve referansı temiz bir kareye al. |
| İz bir karede kaymış | İzleme aracında Ctrl ile o karede alanı doğru yere sürükle (Ctrl+Shift: sonraki kareler de); "İzi geri al" ile geri al. |
| Renk anahtarı "baskın doygun renk yok" | Tabela düz renkli değil ya da çok soluk; Noktalar ya da ECC yöntemini kullan. |
| "Yakarak denetle" hata veriyor | ffmpeg.exe `C:\OsiSub\bin\` içinde ve libass destekli olmalı (gyan.dev "essentials" yapısında var). |
| Renk modeli maskesi tabelanın yazısını/arka planı da alıyor | Pozitif tohumu (yeşil) yalnız nesnenin İÇİNE çiz; negatif tohumu (kırmızı) maskeye girmemesi gereken renklerin üstüne çiz (tabelanın yazısı, arka plan). Önce izleme alanını (Videoda çiz) satırın çevresine daralt. |
| Senkronda bir parça yanlış kaydı | O parçanın kutusunu kaldırıp Uygula; ya da İnce arama aralığını küçült. Şarkı/OP sesi karışıyorsa OP/ED stil desenine o stilin adını ekle. |
| Toplu izlemede "alan görüntü dışında" | Satırın \\pos'u ekran dışında ya da metin boş; satıra dikdörtgen \\clip koyup alanı elle ver. |
| Yazı tipi toplayıcıda "BULUNAMADI" | Yazı tipi sistemde kurulu değil. Dosyasını .ass'ın yanındaki `fonts\` klasörüne koy (orası da taranır) ya da Windows'a kur, sonra "Yeniden tara". |
| Yazı tipi toplayıcıda "Eksik karakter: ğ ş" | O yazı tipinde Türkçe harfler yok; libass başka yazı tipinden tamamlar (farklı görünür). Türkçe destekli bir yazı tipi seç ya da yazı tipini düzenlet. |
| Videoya yakmada "ass" hatası | ffmpeg libass'lı olmalı (gyan.dev "essentials" yapısında var). Yazı tipi yanlış çıkıyorsa "Kullanılan yazı tiplerini topla" işaretli olsun. |
| Kaynakla karşılaştırmada her şey EKSİK | Kayma 15 sn'den büyük ya da dosyalar aynı bölümün değil. Önce Zamanlama > Referansa göre senkron (Ctrl+Alt+B) ya da Seçenekler'de kaymayı elle ver. |
| MicroDVD (.sub) zamanları kaymış | .sub kare tabanlıdır: önce videoyu aç (kare hızı oradan alınır) ya da dosyanın ilk satırına {1}{1}23.976 yaz. |
| Bir kısayol çalışmıyor | Ayarlar > Kısayollar'da çakışma kırmızı görünür. Ctrl+Alt+harf/rakam birleşimleri Türkçe klavyede AltGr (@ € { } [ ] \\ \| ₺) ile çakışabilir. |
| BUILD_EXE.bat "PyInstaller" hatası | Önce KURULUM.bat'ı çalıştır; virüs koruması dist klasörünü kilitliyorsa klasörü dışlama listesine ekleyip yeniden dene. |
| .exe açılmıyor, hiçbir şey olmuyor | `OsiSub-konsol.exe` çalıştır: hata metni pencerede görünür; `%APPDATA%\OsiSub\logs\` içindeki son raporu gönder. |
| Betik hata veriyor | Automation → Automation betikleri penceresinde hata metni görünür. Bana o metni gönder. |

## Ayarlar

**Dosya > Ayarlar… (Ctrl+,)** — tema, CPL/CPS sınırları, yazı boyları, otomatik yedek, kısayollar ve harici araçlar
pencereden değişir, hemen uygulanır. (Hepsi `%APPDATA%\OsiSub\config.json` dosyasında durur; elle düzenlemek
gerekirse uygulama kapalıyken düzenle.) Bir komutun yerini bilmiyorsan **Ctrl+Shift+P** (komut paleti) ile ara.

Harici araç örneği (assmotion):
```json
"external_tools": [
  {"name": "assmotion: etkin satırı izle", "program": "python",
   "args": "C:/araclar/assmotion.py auto --ass {ass} --video {video} --line {active}",
   "reload": true}
]
```
Yer tutucular: `{ass} {dir} {video} {audio} {sel} {active} {start_ms} {end_ms} {start_frame} {end_frame}`.
