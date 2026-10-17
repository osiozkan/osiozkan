# OsiSub

Aegisub'ın yerini almak üzere sıfırdan yazılan altyazı düzenleyici.
Python + PySide6 arayüz, LuaJIT üzerinde **Aegisub Automation 4 uyumlu** betik motoru.

Sürüm: **1.2.0** (konuşmaya göre senkron — bsync uygulama içinde)

## Neler hazır

| Alan | Durum |
|---|---|
| **Bayt korumalı .ass modeli** | Dokunulmayan her satır dosyaya okunduğu gibi yazılır. UTF-8 BOM, CRLF, `Marked` gibi bilinmeyen alanlar, `0000` kenar boşlukları korunur. Düzenlenen satırda bile **değişmeyen alanlar** özgün metniyle kalır. |
| **Automation 4 (Lua)** | Her betik kendi LuaJIT durumunda. `aegisub.*` API'si, subtitles nesnesi (1 tabanlı, `insert/append/delete/deleterange`, negatif indeksle ekleme, `ipairs(subs)`, `script_resolution`), `dialog.display/open/save`, `progress`, `debug.out`, `cancel`, `set_undo_point`, `text_extents` (GDI tanımıyla: hücre yüksekliği = fontsize), `parse_karaoke_data`, `frame_from_ms/ms_from_frame` (Aegisub START anlamı), `include()`, `require`. Modüller: `aegisub.re` (bayt konumlu, boost Perl davranışı), `aegisub.unicode`, `aegisub.util`, `aegisub.lfs`, `aegisub.clipboard`, karaskel, utils, cleantags. |
| **İşlemsel makro** | Makro belgenin çalışma kopyasında koşar; hata ya da `aegisub.cancel()` olursa belge hiç değişmez. Başarıda tek geri al adımı. |
| **Aegisub'ın kendi betikleri** | kara-templater, strip-tags, edgeblur, fullwidth, auto-leadin, cleantags değiştirilmeden çalışıyor (testli). Kurulu Aegisub'ın autoload klasörü de taranır. |
| **Arayüz** | Izgara (10 binlerce satırda akıcı), düzenleme kutusu, stil yöneticisi, bul/değiştir, betik özellikleri, zaman kaydırma, satır işlemleri (ekle, çoğalt, birleştir, \N'den böl, kopyala/yapıştır, yorum), son açılanlar, sürükle-bırak, koyu/açık tema. |
| **CPL (Osi kuralları)** | Izgarada CPL sütunu ("43+16"), düzenleme kutusunda Toplam / En uzun kutuları, canlı **CPL denetimi** bölmesi. 43+ uyarı; \N ile bölünmüşte 42'yi aşan parça hata. Türkçe doğru sayım, boşluk ve noktalama dahil. |
| **Video bölmesi** | libmpv gömülü oynatıcı, kareye tam arama, **canlı altyazı önizlemesi** (her düzenlemede 150 ms içinde). Kare/anahtar kare adımlama, satırı oynat, Ctrl+1/2 satır başı/sonu, **Ctrl+3/4 başlangıç/bitiş = geçerli kare** (Aegisub anlamıyla), kareye/zamana git, altyazılı/altyazısız kareyi PNG kaydet. libmpv yoksa kare gösterici yedeği. |
| **Kare dizini** | Paketler çözülmeden (PyAV, yedek ffprobe), B-kare sıralaması, anahtar kareler, gerçek VFR ayrımı. Aegisub ms tanımı: ilk kare 0 (MP4'te 21 ms gecikme gibi durumlar; mpv'de sub-delay ile eşlenir). 24 dk bölüm: 0,4 sn, önbellekten 0,05 sn. |
| **Ses bölmesi** | Ctrl+sürükle ile bitişik satırın ortak ucu birlikte taşınır. Dalga formu (1 ms tepe zarfı, örnek düzeyine kadar yakınlaşma) ve spektrogram (128 log bant), satır blokları, anahtar kare çizgileri, video konumu. Sol tık başlangıç / sağ tık bitiş / sınır sürükleme; anahtar kareye, satır sınırlarına ve video konumuna yaslama (Shift kapatır); seçimi/öncesini/sonrasını çalma. Ses ekseni Aegisub gibi videonun ilk karesine hizalı. 24 dk: dalga formu ~5 sn, spektrogram arkadan, önbellekten anında. |
| **Kare şeridi** | Geçerli karenin çevresindeki kareler küçük resim olarak; yalnız görünenler arka planda çözülür (atlas yok). Anahtar kare (mor), etkin satırın kareleri (mavi), geçerli kare (sarı). Tıkla-git, tekerlekle ±1/±10 kare. |
| **Zamanlama son işlemcisi** | Aegisub'daki gibi: giriş/çıkış payı, bitişik satırları birleştirme (eşik + birleşme noktası), anahtar karelere yaslama (başlangıç/bitiş için önce/sonra kare eşikleri). Stil süzgeci, önizleme, tek geri al adımı, ayarlar hatırlanır. |
| **Çeviri modu** | Referans altyazı (.ass/.srt/.vtt) yükle: ızgarada **Referans** sütunu, düzenleme kutusunda etkin satırın referansı + "Kopyala". **Referanstan aktar**: TR satırları EN .ass'ın satırlarına yazılır; stil/aktör/baştaki etiketler ve EN zamanları korunur, karşılıksız EN satırı asla silinmez, ev sahipsiz TR yeni satır olarak eklenir; çoklu/belirsiz/zayıf eşleşmeler rapora düşer. Elle düzeltme: seçili referans satırını etkin satıra yaz / \N ile ekle / yeni satır. |
| **Görsel tipografi** | Video bölmesinin üstündeki araç çubuğu: Artı, Taşı (\\pos/\\move; Ctrl+sürükle \\move'a çevirir; seçili tüm satırlar birlikte), Döndür Z (\\frz, Shift 15° adım, \\org tutamacı), Döndür XY (\\frx/\\fry), Ölçek (\\fscx/\\fscy), dikdörtgen Clip (Ters: \\iclip), Vektör clip (ekle/sürükle/sağ tık sil/Ctrl+tık araya ekle), Damlalık (\\1c–\\4c, panoya da kopyalar). Tuvalde mpv'nin libass çizimi gösterilir; her sürükleme tek geri al adımı. Esc normal görünüme döner. |
| **Denetim (QC, F7)** | 19 kural: boş satır, çift boşluk, baş/son/\\N çevresi boşluk, noktalamadan önce boşluk, noktalamadan sonra eksik boşluk, üç nokta (../..../…), tekrarlanan kelime, küçük harfle başlayan cümle (Türkçe i/İ doğru), diyalog çizgisi, baştaki/sondaki/çift \\N, dengesiz parantez/tırnak/etiket, kısa/uzun süre, CPS, CPL, satır sayısı, çakışma, küçük boşluk, yazım. Düzeltmeler yalnız görünen metne uygulanır (etiketler ve çizimler korunur); işaretliler tek geri al adımıyla düzeltilir. |
| **Türkçe yazım denetimi** | Hunspell tr_TR sözlüğü (saf Python spylls): düzenleme kutusunda kırmızı dalgalı alt çizgi, sağ tıkla öneriler / sözlüğe ekle / yoksay. Özel isim + kesme ("Naruto'yu", "İstanbul'da") ve tireli birleşikler doğru ele alınır. Sözlük uygulamadan indirilir ya da Aegisub'ınki kullanılır. |
| **Sahne değişimi** | Video > Sahne değişimlerini bul (ffmpeg scdet, arka planda, önbellekli): ses bölmesinde turuncu çizgi, kare şeridinde turuncu şerit, ses bölmesinde yaslama adayı, zamanlama son işlemcisinde anahtar kare gibi kullanılabilir. Kodlayıcının anahtar kare koymadığı kesmeleri de bulur. |
| **Terim sözlüğü** | Seriye özel terimler (TSV: kaynak / Türkçe / yasak varyantlar / not), belgenin klasöründeki `terimler.tsv` kendiliğinden yüklenir. Denetim kuralı: yasak varyant (Türkçe ekleri korunarak düzeltme önerir: "Stantlar" → "Standlar") ve referansta geçip çeviride olmayan terim. Seçili metni sözlüğe ekleme. |
| **Konuşma algılama (VAD)** | Ses açılınca konuşma bölümleri arka planda bulunur (24 dk ~1 sn), ses bölmesinin altında turkuaz şerit. Zamanlama son işlemcisinde "Konuşmaya yasla" (önce uygulanır); "Konuşmadan boş satırlar oluştur" (sıfırdan zamanlama için, yalnız boş aralıklara). |
| **Whisper** | Ses > Konuşmayı yazıya dök: faster-whisper (tiny…large-v3, CUDA varsa GPU), Japonca/İngilizce/…; sözcük zamanlarından satırlara bölme, sınırlar VAD'e yaslanır; çıktı Referans sütununa ya da yeni satırlar olarak (tek geri al). |
| **Hareket takibi (Ctrl+Alt+M)** | assmotion'ın uygulama içi hali. Seçili satırların kare aralığı otomatik alınır; izlenecek alan videoda **İzleme** aracıyla çizilir ya da satırın dikdörtgen \\clip'inden alınır. Yöntemler: **Noktalar** (köşe noktaları + Lucas-Kanade, her karede doğrudan referans→kare RANSAC benzerlik dönüşümü: konum + ölçek + dönüş; kayma birikmez, ileri-geri tutarlılık denetimi, nokta azalınca yeniden tohumlama) ve **Şablon** (yalnız konum). Referans kareden ileri ve geri izler, iptal edilebilir. Eğri grafiği (Δx, Δy, ölçek, dönüş; güvensiz kareler kırmızı, tıkla-git), tuvalde alanın o karedeki yeri ve izin yolu. **Uygula**: \\pos, \\org, \\clip/\\iclip (dönüşte dikdörtgen clip vektöre çevrilir, ölçekli vektör clip desteklenir), \\fscx/\\fscy, \\bord/\\shad/\\blur, \\frz; \\t, \\fad, \\fade zamanları her parçaya göre kaydırılır. Kip: **kare kare** ya da **sıkıştırılmış** (\\move + \\t parçaları, ±0,5 px / ±0,5% / ±0,3° hoşgörü). Yumuşatma, "satır hangi karede doğru" seçimi, tek geri al. İz **.assmotiontrack** (JSON) olarak saklanır; **Mocha / After Effects Keyframe Data** yapıştırılır ve dışa aktarılır (gerçek Aegisub'da Aegisub-Motion ile de kullanılabilir). Yer gerçeğiyle: 300 px kayma, 0,86→1,38 ölçek, 28° dönüşte konum < 0,5 px, ölçek < %1, açı < 0,5°; libass ile yakılan satır tabelayı < 1 px izliyor. |
| **Örtme (önden geçen nesne)** | İzden sonra **Örtenleri bul**: izin karelerinden ortanca "temiz görünüm" modeli kurulur (referans karede örtme olsa da çalışır), her kareye çarpıtılıp CIELAB farkı alınır; açma/kapama/delik doldurma/genişletme, kontur → çokgen. Uygulamada örtülen karelere **\\iclip** yazılır (sıkıştırılmış kipte o kareler tek kare kalır), istenirse **görünürlüğü eşiğin altındaki karelerde satır üretilmez**. Tuvalde maske kırmızı, eğride görünürlük. Yer gerçeği: direk alana belirgin girdiği her karede bulunan maske gerçeği %95+ kapsar; libass yakmasında direğin arkasında tek yeşil piksel kalmaz, direk yokken yanlış alarm yok. Sonuç .assmotiontrack içinde saklanır. |
| **İzleyiciler (0.9.0)** | **ECC** (assmotion xtrack): şablon afin hizalanır, en yakın benzerliğe indirgenir; desenli ve örtmesiz planlarda en hassası (yer gerçeği: konum 0,16 px, ölçek %0,2, açı 0,03°); korelasyon ve ani ölçek/dönüş korumalı. **Renk anahtarı** (assmotion track): düz renkli tabelalar için konum + ölçek; alandaki baskın ton kendiliğinden bulunur; yalnız TAM görünen kareler ölçülür, örtülen kareler güvensiz işaretlenip ara değerle dolar, örtmeden sonra tabela yeniden yakalanır. **Elle düzeltme**: İzleme aracında Ctrl+sürükle o kareyi, Ctrl+Shift+sürükle referanstan uzaktaki tüm kareleri kaydırır; "İzi geri al". |
| **libass ile görünürlük denetimi** | Seçili satırlar ffmpeg'in `ass` süzgeciyle siyah ve beyaz zemine yakılır, farktan piksel örtücülüğü çıkar; \\clip/\\iclip'li ve clip'siz hali karşılaştırılarak satırın gerçekte ne kadarı göründüğü ölçülür; eşiğin altındakiler seçilir (silmek/gizlemek kolay). |
| **Renk modeliyle maske (hairtrack, 1.1)** | Örtme grubunda "Renk modeli": videoda **Tohum** aracıyla nesnenin (saç, el…) üstüne sol, çevresine sağ tıkla çizilir; pozitif/negatif renkler CIELAB k-means ile modellenir, kare kare maske önceki maskenin yakınından yayılır (başka yerdeki aynı renkler alınmaz), sınırlı delik doldurma, izleme alanıyla kırpma. Çizgi-sanat koruması: tohum siyah kontura taşarsa o mod atılır (tabelanın yazısı maskeye girmez). İz GEREKMEZ: duran tabelada Uygula kimlik iziyle yalnız \\iclip yazar. Yer gerçeği: telin içi her karede %95+ bulunur, dışına taşma < 60 px; libass yakmasında tel önde kalır. |
| **Konuşmaya göre senkron (bsync, 1.2)** | Zamanlama > Konuşmaya göre senkron (Ctrl+Shift+B). Osi'nin kuralları: kayma sabit değil, yalnız ≥ 4 sn altyazı boşluklarında (sahne aralarında) değişir; ekran yazısı (`\\N \\N \\N`) ve tabelalar çapa olmaz ama parçasıyla kayar; OP/ED (stil/aktör/efekt adı) ve uzun kesintisiz ses (şarkı) bastırılır; hiçbir satır 0'ın önüne ya da ses sonundan sonraya düşmez; yalnız Start/End değişir. VAD topluluğu (5 eşiğin çoğunluğu) → blok başına çapraz ilinti → parça başına ±8 sn DP (değişim cezası, sıra korunur). Parça tablosu: kayma, güven (düşükler kırmızı ve işaretsiz), çift tıkla satıra git; tek geri al. 24 dk: VAD ~5 sn, hizalama < 0,5 sn. |
| **Toplu hareket takibi** | Seçili her satır kendi kare aralığı ve kendi alanıyla (dikdörtgen \\clip ya da yazı tipi ölçümüyle metin/çizim kutusu) izlenir ve uygulanır; referans satırın ilk karesi; sahne kesmesi bulunduysa iz kesmede durur ve uyarılır; isteğe bağlı örtme; tek geri al. |
| **SRT / VTT** | SRT ve WebVTT açma (UTF-8, BOM, cp1254), `<i>`/`<font color>` dönüşümü; SRT dışa aktarma. |
| **Proje dosyası** | .ass açılınca `[Aegisub Project Garbage]` içindeki Video/Audio File kendiliğinden açılır; kayıtta yalnız değiştiyse yazılır. |
| **Harici araçlar** | `config.json > external_tools` ile assmotion, hairtrack, bsync gibi komut satırı araçları `{ass} {video} {sel} {active} {start_ms}…` yer tutucularıyla menüden çalıştırılır; araç dosyayı değiştirirse yeniden yüklenir. |
| **Dağıtım (1.0)** | `BUILD_EXE.bat` → PyInstaller ile Python gerektirmeyen tek klasör (`OsiSub.exe` pencereli, `OsiSub-konsol.exe` komut satırı); Inno Setup varsa yönetici izni istemeyen `OsiSub-1.0.0-Kurulum.exe` (Başlat menüsü, masaüstü kısayolu, .ass ilişkilendirme). ffmpeg/libmpv `bin/` klasöründen alınır. Paketli sürümde yol çözümü exe klasörüne göre. |
| **Sağlamlık (1.0)** | 60 sn'de bir otomatik yedek (yalnız değişiklik varsa, atomik), açılışta kurtarma penceresi (yedek özgün dosyanın üstüne kendiliğinden yazılmaz), yakalanmamış hatada program kapanmaz: acil yedek + `logs/cokme-*.txt` raporu; döner günlük dosyası (`logs/osisub.log`, Qt iletileri dahil); uygulama simgesi. |
| **Komut satırı** | `doctor`, `roundtrip`, `macros`, `macro`, `--version` (makroyu arayüzsüz çalıştırıp çıktıyı Aegisub'ınkiyle bayt bayt karşılaştırmak için). |
| **Testler** | 200 test (çekirdek, Automation API, Aegisub betikleri, ekransız arayüz, medya yer gerçeği, gerçek libmpv). |

## Henüz olmayanlar (sıradaki aşamalar)


derinlik tabanlı örtme, ekran dışına taşmanın ölçülmesi, MoonScript betikleri,
DependencyControl. Ayrıntı: [docs/YOL_HARITASI.md](docs/YOL_HARITASI.md).

## Çalıştırma

Kurulum adımları (sıfırdan, Windows): [KURULUM.md](KURULUM.md)

```
python -m osisub                     # arayüz
python -m osisub dosya.ass           # dosyayla aç
python -m osisub doctor              # ortam denetimi
python -m unittest discover -s tests -t .
```

## Klasörler

```
osisub/
  paths.py diag.py  paketli/kaynak yol çözümü · günlük ve çökme raporu
  core/          asstime, assfile (bayt korumalı model), textutil (CPL/CPS), undo, framemap,
                 timing (son işlemci), formats (SRT/VTT), transfer (referanstan aktarma), tags (etiket okuma/yazma),
                 qc (denetim kuralları), spell (yazım denetimi), glossary (terim sözlüğü), asr (Whisper), sync (bsync),
                 motion (hareket izi, .assmotiontrack, AE verisi, satıra uygulama)
  media/         probe, frameindex, audio (Peaks, Spectrogram), frames (FrameGrabber), scenes, vad, tracker (OpenCV izleme), occlude (örtme),
                 batch (toplu izleme), render_qc (libass ile görünürlük), colormask (hairtrack), cache
  automation/    runtime (LuaScript, ScriptManager), host, regex_bridge, textmetrics
    lua/         osisub_prelude.lua (aegisub tablosu + subs), osisub_karaoke.lua
      include/   Aegisub include dosyaları + aegisub/*.lua uygulamalarımız
      autoload/  Aegisub ile gelen betikler
  ui/            mainwindow, context, grid, editbox, widgets, dialogs, panels, video, audio, media_ctl,
                 framestrip, reference, timing_dialog, visual, qc_panel, spell_ui, glossary_panel, asr_dialog,
                 motion_panel,
                 automation_ui (GuiHost, LuaDialog, ilerleme), theme, app
tests/           test_core, test_automation, test_ui, test_media, test_video_ui, test_timing_transfer, test_tags, test_visual, test_qc_spell, test_glossary_speech, test_motion, test_release, test_colormask, test_sync, lua/, fixtures/
tools/bundle.py  tüm kodu tek .txt'ye paketler / geri açar
packaging/       osisub.spec (PyInstaller), launcher.py, osisub.iss (Inno Setup); BUILD_EXE.bat çalıştırır
```

## Kendi betiklerin

`%APPDATA%\OsiSub\automation\autoload\` klasörüne koyduğun `.lua` dosyaları açılışta yüklenir
(Osi menüsü betiklerin dahil). Kurulu Aegisub'ın `automation\autoload` klasörü de taranır;
aynı adlı dosyada OsiSub'ınki önce gelir.

## Üçüncü taraf

`osisub/automation/lua/include` ve `autoload` altındaki Aegisub dosyaları kendi BSD/ISC
lisans başlıklarıyla birlikte, değiştirilmeden dağıtılır. `include/aegisub/*.lua`
Aegisub'ın MoonScript modüllerinin düz Lua karşılıklarıdır (özgün telif başlıkları korunmuştur).
