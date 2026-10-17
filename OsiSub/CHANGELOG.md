# Değişiklik günlüğü

## 1.2.0 — konuşmaya göre senkron (bsync)
- core/sync.py: parçalara bölme (≥ 4 sn boşluk = sahne arası; ≥ 60 sn = blok), çapa seçimi (ekran yazısı `\\N \\N \\N`,
  \\pos/\\move/\\p tabelaları ve OP/ED satırları çapa değil, parçasıyla kayar), OP/ED aralıklarının ve
  45 sn'den uzun kesintisiz sesin bastırılması, blok başına FFT çapraz ilinti (±60 sn), parça başına ±8 sn / 40 ms
  adımlı DP (kayma değişim cezası, sıra/örtüşme kısıtı), Gauss yumuşatma (giriş/çıkış payı ortalanır), güven,
  0..ses sonu kırpması
- media/vad.py: speech_activity — 5 eşiğin çoğunluk oyu (10 ms)
- ui/sync_dialog.py: Zamanlama > Konuşmaya göre senkron (Ctrl+Shift+B): ayarlar, parça tablosu, düşük güven
  kırmızı/işaretsiz, satıra git, tek geri al; hesaplamadan sonra belge değişirse uygulamaz
- 200 test (6 yeni: parçalı kaymalar, sahne arası kuralı ve sıra, OP/ED + şarkı bastırma, kırpma, yapay konuşmalı
  ses ile uçtan uca, pencere akışı)

### Ölçülmüş sonuçlar
- Yumuşatmasız örtüşme puanı satırların giriş/çıkış payı yüzünden PLATO veriyordu; aynı gerçek kaymaya sahip iki
  parça 1020 ve 1300 ms buldu. σ = 150 ms yumuşatma ile tüm parçalar gerçek değere ±70 ms (payın yarısı ortalanır)
- Ton içeren şarkı VAD'de %100 "konuşma" görünüyor; OP/ED stil adı yoksa yalnız uzun-koşu kuralı yakalar

## 1.1.0 — renk modeliyle nesne/saç maskesi (hairtrack)
- media/colormask.py: tohum karedeki pozitif/negatif fırça darbelerinden CIELAB k-means modeli; en yakın mod
  uzaklık oranı + üst sınır; kapama, alan sınırlı delik doldurma, küçük bileşen atma; rehberli bileşen seçimi
  (tohum karede pozitif tohuma, sonra önceki maskenin genişletilmişine değen); ileri/geri yayılım; kırpma;
  1080p'de yarım ölçekte işlem. Çıktı örtme sonucu biçiminde (aynı \\iclip uygulama yolu)
- Korumalar: düşük ağırlıklı pozitif mod, negatif moda çok yakın pozitif mod ve baskın olmayan çok koyu
  (çizgi-sanat) pozitif mod atılır; tohum karesinde maske kırpmanın yarısından fazlasını kaplar ya da negatif
  tohuma taşarsa uyarı
- Tuval: **Tohum** aracı (sol = nesne, sağ = nesne değil; ekranda ~6 px fırça; tohumlar tek kareye aittir)
- Bölme: örtme yöntemi seçimi, "Tohum çiz", "Tohumları temizle"; iz yokken Uygula kimlik iziyle yalnız maskeyi yazar
- Yeni fikstür hair.mkv (duran tabela + çizgi-sanat kenarlı dalgalı tel) ve 4 test: yer gerçeği, çizgi-sanat
  koruması, libass yakma, tuvalde fareyle tohum çizip uygulama
- 194 test

### Ölçülmüş sonuçlar (tekrar denenmesin)
- Tohum telin siyah kenarını da kapsayınca siyah mod ağırlığı %18 (min_weight %5'i geçiyor) ve tel tabelanın
  yazısına değdiği için bileşen seçimi yazıyı da alıyordu (14 000+ px taşma). Çizgi-sanat koruması ile 303 px
- Pozitif tohum telin dışına (arka plana) taşınca arka plan modu da pozitif oluyor; bu durumda uyarı çıkar
  (kırpma alanının yarısından fazlası) — kendiliğinden düzeltilemez, tohum dikkatli çizilmeli

## 1.0.0 — dağıtım ve sağlamlık
- packaging/osisub.spec + launcher.py: PyInstaller tek klasör; iki exe (pencereli OsiSub.exe, OsiSub-konsol.exe);
  osisub verisi (Lua include/autoload) ve lupa alt modülleri dahil; Whisper bilerek dışarıda.
  Linux'ta derlenip denendi: doctor, kara-templater listeleme, strip-tags makrosu, arayüz açılışı paketli halde çalıştı
- BUILD_EXE.bat: paketler → simge → PyInstaller → bin/ (ffmpeg, ffprobe, libmpv) kopyası → doctor → Inno Setup varsa kurulum
- packaging/osisub.iss: yönetici izni istemeyen kurulum (Türkçe), Başlat menüsü, masaüstü, .ass "Birlikte aç" kaydı, kaldırma
- osisub/paths.py: paketli/kaynak ortamda aynı yol çözümü (bin/ önce exe klasöründe aranır); ffmpeg/libmpv aramaları buna bağlandı
- osisub/diag.py: döner günlük dosyası + Qt iletileri; ana ve arka plan iş parçacıklarında yakalanmamış hata → rapor dosyası,
  arayüzde acil yedek + bilgi (program kapanmaz; hata döngüsünde 3'ten sonra yalnız durum çubuğu)
- ui/recovery.py: otomatik yedek (yalnız değişiklik varsa, atomik, oturuma özel; önceki çökmenin yedeğine dokunmaz),
  açılışta kurtarma penceresi; kurtarılan belge özgün yolla ama "değişmiş" açılır; eski yedekler kendiliğinden temizlenir
- ui/icon.py: çizilen uygulama simgesi (.ico paketleme sırasında üretilir); `--version`
- libass denetiminde ffmpeg artık Windows'ta konsol penceresi açmıyor
- 190 test (6 yeni)

### Bulunan ve düzeltilen hatalar
- Aynı saniyede iki çökme raporu aynı dosya adını alıp ilkini eziyordu → sıra numarası

## 0.9.0 — ECC ve renk anahtarlı izleme, elle iz düzeltme, libass görünürlük denetimi
- media/tracker.py: `ecc` yöntemi (findTransformECC, MOTION_AFFINE, en yakın benzerliğe indirgeme, pencere
  kırpma + ~200 px'e küçültme, korelasyon < 0.6 ya da kare başına ölçek > %15 / dönüş > 10° ise red);
  yer gerçeği: konum 0,16 px, ölçek %0,2, açı 0,03° (Noktalar'dan hassas, örtmeye karşı zayıf)
- media/tracker.py: `color` yöntemi — baskın doygun ton (dairesel histogram + dairesel ortalama ince ayarı)
  ya da verilen renk; tabelanın referans biçimi güçlü açmayla kenara değen kırıntılardan ayrılır; yalnız TAM
  görünen kareler ölçülür (en/boy oranı, görünen oran ≥ %93, log-ölçek eğilimi), örtülenler güvensiz;
  tahmin son tam görünen karelere doğrusal uydurmadan; örtmeden sonra geniş pencerede yeniden yakalama
- media/render_qc.py: libass (ffmpeg `ass`) ile siyah/beyaz zemin yakması → piksel örtücülüğü; clip'li /
  clip'siz oran; zamanda çakışmayan katmanlar, seyrek zamanlar ayrı çağrılar; Windows yolları için kaçış
- Bölme: yeni yöntemler, "İzi geri al", "Yakarak denetle (libass)" + eşik (düşükler seçilir)
- Tuval: İzleme aracında Ctrl+sürükle (bu kare) / Ctrl+Shift+sürükle (referanstan uzaktaki tüm kareler)
- 184 test (3 yeni: ECC ve renk anahtarı yer gerçeği — düz kırmızı, önünden direk geçen tabela —, libass ölçümü;
  bölme testine elle düzeltme, geri alma ve libass denetimi eklendi)

### Ölçülmüş sonuçlar (tekrar denenmesin)
- Renk anahtarında kısmi örtme sırasında konumu "görünen kenardan" ya da biçim ilintisinden kestirmek
  denendi: bilinmeyen kenar yüzünden ilinti PLATO oluyor, tahmine en yakın plato noktası örtenle birlikte
  kayıyor (direk hızında, 30+ px). Kısmi kareleri güvensiz işaretleyip doldurmak dürüst ve daha az hatalı
- Örtme girişinin ilk 1–2 karesi alan eşiğine takılmadan ağırlık merkezini 6 px kaydırıyordu; "tam görünür"
  sınıfı için en/boy oranı + görünen oran + eğilim üçlü koşulu şart
- Biçimi her tam karede gözlemden yenilemek (dönüşü izlemek için) örteni biçime katıyor — kaldırıldı
- Ton tepe noktası kırmızıda (0/180 sarması) arka plan yüzünden 346°'ye kayıyordu → dairesel ortalama ince ayarı
- ECC örtmeli planda 9,5 px'e kadar kayıyor (korumalar yalnız 2 kareyi yakaladı) — örtmeli planda Noktalar

## 0.8.0 — örtme, görünürlük, toplu hareket takibi
- media/occlude.py: warp-fark ile önden geçen nesne maskesi. Yenilik: tek referans kare yerine izin
  karelerinden (en çok 31) ORTANCA temiz görünüm modeli — referans karede örtme olsa da çalışır, olursa
  not düşülür; model="ref" eski yöntem. CIELAB ΔE eşiği, alan kenarı payı, açma/kapama, delik doldurma,
  küçük bileşen atma, genişletme, RDP çokgen; aşırı fark "belirsiz" sayılır (ışık değişimi koruması);
  kare başına görünürlük = alanın kare içi oranı × (1 − örtülü oran)
- core/motion.py: ApplyOptions.occlusion (örtülen karelere \\iclip; satırda zaten \\clip varsa uyarı),
  hide_below (görünürlüğü düşük karelerde satır yok); sıkıştırılmış kipte maskeli kareler tek kare,
  gizli kareler atlanır, düz koşular hoşgörülü parçalara bölünür; örtme sonucu .assmotiontrack'te saklanır
- media/batch.py: toplu izleme — satır başına aralık/referans/alan (dikdörtgen \\clip ya da metin/çizim
  kutusu, %15 pay), sahne kesmesinde durma, iş başına hata yakalama, tersten uygulama (satır indeksleri bozulmaz)
- Bölme: Örtme grubu (ΔE, genişletme, model), görünürlük eğrisi, tuvalde kırmızı maske ve durum
  satırında görünürlük; toplu düğme; izleme sırasında belge değişirse toplu sonuç uygulanmaz; bölme kaydırılabilir
- textmetrics: QGuiApplication yerine QApplication (aynı süreçte sonradan pencere açılabilsin)
- 181 test (5 yeni sınıf/test: önden geçen direkli yer gerçeği videosu, libass yakma, ortanca model, toplu)

### Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- Tek-referans warp-farkı, referans karede örtme varsa örten GİTTİKTEN sonra o yeri "örtülü" sanıyordu
  (36 kare boyunca hayalet maske). Ortanca model bunu ortadan kaldırdı; test bu durumu ikisiyle karşılaştırır
- "Referans örtülü" notu sıkıştırma gürültüsünden %3 yanlış alarm veriyordu → açma sonrası oran, eşik %3
- textmetrics'in QGuiApplication'ı, aynı süreçte sonradan açılan pencereyi çökertiyordu

## 0.7.0 — hareket takibi (assmotion uygulama içinde)
- core/motion.py: iz modeli (kare başına referans→kare benzerlik dönüşümü, karmaşık sayıyla), yeniden
  referanslama, güvensiz kareleri ara değerle doldurma, uçlarda simetrik daralan yumuşatma
- .assmotiontrack (JSON, okunabilir x/y/ölçek/dönüş alanlarıyla) ve Adobe After Effects 6.0 Keyframe Data
  içe/dışa aktarma (Mocha; Anchor Point + Position + Scale + Rotation tam bileşimle)
- Satıra uygulama: \pos (yoksa stil hizasından), \org, \clip/\iclip (dikdörtgen; dönüşte vektöre çevrilir;
  ölçekli vektör clip), \fscx/\fscy, \bord/\shad/\blur (tüm bloklarda ve \t içinde), \frz;
  \t / \fad / \fade zamanları kaydırılır; betik ↔ video çözünürlüğü farkı hesaba katılır;
  kare kare ya da sıkıştırılmış (\move + \t, hoşgörülü doğrusal parçalar) kip; \move'lu satır reddedilir
- media/tracker.py: OpenCV nokta izleyici (Shi-Tomasi + LK, ileri-geri denetim, RANSAC benzerlik + inlier'larla
  en küçük kareler, yeniden tohumlama) ve şablon izleyici; geri yönde 32'lik parça önbelleği; iptal/ilerleme
- ui/motion_panel.py: Hareket takibi bölmesi (Video > Hareket takibi…, Ctrl+Alt+M), eğri grafiği;
  görsel tuvalde yeni **İzleme** aracı (alan çizme, alanın karedeki yeri, iz yolu, güvensiz kareler)
- doctor: OpenCV satırı; requirements: opencv-python
- 176 test (18 yeni): bilinen hareketli sentetik video (x264) ile yer gerçeği, ffmpeg+libass ile yakılan
  satırın tabelayı izlediğinin ölçümü, sıkıştırılmış kipin kare kare kiple libass anlamında eşitliği

### Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- Yumuşatmada uçlarda kesilen pencere ilk/son karede ortalamayı kaydırıyordu; referans uçtaysa yeniden
  referanslama bu kaymayı TÜM izlere yayıyordu (3,4 px). Pencere artık uçlarda simetrik daralıyor
- Şablon izleyici ölçek/dönüş değişen sahnede (doğası gereği) kaçıyor: ölçüldü (85 px), yalnız düz/öteleme
  planları için önerilir; varsayılan yöntem noktalar

## 0.6.0 — terim sözlüğü, konuşma algılama, Whisper
- core/glossary.py + ui/glossary_panel.py: TSV terim sözlüğü (belge klasöründe terimler.tsv ya da genel),
  Türkçe katlamalı (i/İ, ı/I) kelime başı eşleşme, ekli biçimler; QC "Terim sözlüğü" kuralı: yasak varyant
  (düzeltme önerir, varsayılan işaretsiz) + referansta olup çeviride olmayan terim; aynı terim iki kez raporlanmaz
- media/vad.py: model gerektirmeyen VAD (log enerji + 250–3400 Hz bant oranı, histerezis, ortanca süzgeç);
  yer gerçeğiyle ±50 ms; ses açılınca arka planda; ses bölmesinde turkuaz şerit
- Zamanlama son işlemcisi: "Konuşmaya yasla" grubu (diğer adımlardan önce), değişiklikler tek adımda birleşir
- Ses > Konuşmadan boş satırlar oluştur (yalnız boş aralıklar, uzun bölümler bölünür, tek geri al)
- core/asr.py + ui/asr_dialog.py: faster-whisper ile yazıya dökme (model/dil/aralık seçimi, CUDA otomatik),
  sözcüklerden satırlara bölme, VAD yaslaması, çıktı Referans ya da zamanlanmış satır; KURULUM_WHISPER.bat
- 158 test (13 yeni; Whisper testi espeak-ng ile üretilmiş gerçek konuşmayla)

### Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- faster-whisper'ın ses okuyucusu PyAV 19 ile uyumsuz (`metadata_errors`) → ses 16 kHz float dizi olarak veriliyor
- Video açılınca kendi sesi de yükleniyor; ardından ayrı ses açılırsa testler/arayüz bir an ESKİ sesi görebilir —
  nesil sayacı doğru olanı tutuyor, testler süreye bakarak bekliyor
- Whisper sözcük başlangıçları kaba (±300 ms); VAD yaslama penceresi 400 ms

## 0.5.0 — denetim, yazım denetimi, sahne değişimi
- core/qc.py: 19 kural, düzeltme işlevleri yalnız görünen metne (etiket/çizim korunur); aynı satırdaki
  birden çok düzeltme kural sırasıyla güncel metne uygulanır; Türkçe tr_upper/tr_lower
- ui/qc_panel.py: Denetim bölmesi (F7), kural seçimi kalıcı, çift tık satıra git, işaretlileri tek adımda düzelt
- core/spell.py + ui/spell_ui.py: Hunspell tr_TR (spylls), arka planda yükleme (~7 sn), sözlük indirme
  (LibreOffice deposu), Aegisub sözlük klasörünü de arar; kesmeli özel isim, tireli birleşik, kısaltma kuralları;
  kullanıcı sözlüğü; düzenleme kutusunda dalgalı alt çizgi + sağ tık önerileri
- media/scenes.py: ffmpeg scdet ile sahne değişimi (kesmeler yer gerçeğiyle birebir: 48/96/144),
  ses bölmesi + kare şeridi işaretleri, ses yaslaması, zamanlama son işlemcisinde kullanım
- QC bölmesi CPL bölmesiyle aynı sekmede; kare şeridi yüksekliği sınırlandı
- 145 test (17 yeni)

### Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- "Küçük harfle başlıyor" düzeltmesi özgün metinden hesaplanıyordu; aynı satırdaki çift boşluk düzeltmesini
  eziyordu → her metin kuralı güncel metne yeniden uygulanıyor
- Zamanlama penceresinde işaretlenmemiş (checkable) grup içindeki denetimler devre dışı görünür; testte önce grup açılmalı

## 0.4.0 — görsel tipografi
- core/tags.py: ilk override bloğunda etiket okuma/yazma (\t içine dokunmaz; \fr=\frz, \c=\1c eş adları;
  aynı etiketin tüm kopyaları temizlenip tek değer yazılır), \pos/\move/\org/\clip/\iclip ayrıştırma,
  vektör çizim (m/l/b; eksik b koşusu l'ye düşer), stil hizası + kenar boşluklarından varsayılan konum,
  \move'un t anındaki ara konumu
- ui/visual.py: görsel araç tuvali — libmpv'nin altyazılı ekran görüntüsü (oynatmadaki libass çizimiyle aynı),
  libmpv yoksa altyazısız kare; tutamaçlar Qt ile; mpv arama bitmeden görüntü alınmaz (bekle-yeniden dene)
- Araçlar: Artı, Taşı, Döndür Z, Döndür XY, Ölçek, Clip, Vektör clip, Damlalık; Video menüsünde de var; Esc çıkış
- Testler (20 yeni, toplam 128): her araç gerçek fare olaylarıyla; libmpv ile sürükleme sonrası
  kırmızı test metninin çizimde ~200 px kaydığı ölçülüyor

### Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- Vektör clip'te iki noktalı şekilde iki kenar aynı doğrudur; kayan nokta farkı Ctrl+tık'ı sona ekletiyordu
  → eşitlikte önceki kenar seçiliyor
- Çizim testinde testsrc2'nin kendi kırmızı şeridi ölçümü bozuyordu → altyazısız kare taban alınıp çıkarılıyor

## 0.3.0 — kare şeridi, zamanlama son işlemcisi, çeviri modu
- Kare şeridi: görünen kareler ayrı çözücüyle arka planda; 600 küçük resimlik LRU; anahtar kare /
  etkin satır / geçerli kare işaretleri; tıkla-git, tekerlek ±1/±10 (küçük resim içeriği ffmpeg karesiyle sınandı)
- Zamanlama son işlemcisi (core/timing.py + pencere, Ctrl+Shift+T): giriş/çıkış payı, bitişik satırlar
  (eşik, bias, ters dönmeye karşı sınır), anahtar kare yaslama (Aegisub varsayılan eşikleri 5/4/5/11 kare),
  stil süzgeci, yorum atlama, önizleme, tek geri al, ayarlar config.json'da
- Ses bölmesi: Ctrl+sürükle bağlı sınır (bitişik satırın ortak ucu), sürüklenen komşu yaslama adayından çıkarılır
- Çeviri menüsü: referans altyazı (Ctrl+Alt+O), Referans sütunu (Metin ile genişliği paylaşır),
  düzenleme kutusunda referans + Kopyala (Ctrl+Alt+R), Referanstan aktar (core/transfer.py),
  inceleme bölmesi (Referans satırları: eşli/EŞSİZ; Aktarma raporu: çift tıkla satıra git), elle düzeltme
- Aktarma kuralları: EN stil/aktör/zaman/baş etiketleri korunur; karşılıksız EN silinmez; ev sahipsiz TR
  zamana göre araya eklenir (stil: en yakın EN satırınki); çoklu TR \N ile birleşir; belirsiz/zayıf raporlanır;
  tabela/şarkı gibi görünen stiller varsayılan olarak ev sahibi dışında
- core/formats.py: SRT/WebVTT okuma (UTF-8/BOM/UTF-16/cp1254), `<i><b><u><s><font color>` dönüşümü, SRT yazma;
  .srt/.vtt doğrudan açılabiliyor (içe aktarıldı olarak, Farklı kaydet ister); Dosya > SRT olarak dışa aktar
- 108 test (23 yeni)

## 0.2.0 — Aşama 2: video ve ses
- media/: ffprobe bilgisi, paket çözmeden kare dizini (PyAV / ffprobe yedeği), Aegisub normalize ms,
  VFR ayrımı (2 ms eşik), önbellek (boyut+mtime anahtarlı, kullanıcı önbellek klasöründe)
- Ses: ffmpeg ile mono PCM, ilk video karesine hizalama (FFMS2 FFMS_DELAY_FIRST_VIDEO_TRACK),
  1 ms tepe zarfı, 128 bantlı log spektrogram (arka planda), çizim başına ~1,4 ms
- Kare çözme (FrameGrabber): ffmpeg yer gerçeğiyle birebir; ileri adımda aramasız devam
- Video bölmesi: libmpv gömülü, hr-seek, canlı altyazı (sub-reload), sub-delay ile Aegisub ms eşlemesi,
  kare/anahtar kare adımlama, satırı oynat, PNG dışa aktarma; libmpv yoksa kare gösterici
- Ses bölmesi: tık/sürükle zamanlama, yaslama, yakınlaştırma, genlik, çalma (Qt Multimedia)
- Kısayollar: Ctrl+1/2/3/4, Ctrl+G, Ctrl+Space, Alt+←/→, Alt+Shift+←/→; ses bölmesinde Boşluk/S/Q/W/E/D/Z/X
- Lua: video_size, keyframes, frame_from_ms/ms_from_frame (gerçek kare dizininden), get_audio_selection,
  get_frame (arch1t3cht 3.4: width/height/getPixel/getPixelFormatted)
- Zamanlama: seçili satırları kareye oturt
- Proje: Video/Audio File otomatik açma ve (yalnız değiştiyse) kaydetme
- doctor: ffmpeg/ffprobe/PyAV/libmpv denetimi; KURULUM: bin/ klasörüne ffmpeg + libmpv-2.dll
- 85 test (25 yeni: medya yer gerçeği + gerçek libmpv vo=null)

### Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- Spektrogram uzaklaştırılmış görünümde reduceat son dilimi dizinin sonuna kadar alıyordu: sessiz
  aralıkta 5. saniyedeki ton görünüyordu → son dilim sınırlandı (test: sessiz aralıkta 0)
- Qt Multimedia QMediaDevices, ses sunucusu olmayan Linux'ta açılışta segfault → aygıt sorgusu ilk çalmaya ertelendi
- Ses bölmesinde sınırın 6 px yakınına tıklamak zamanı değiştirmiyor, sınırı tutuyor (bilinçli; Aegisub da böyle)
- 4025 ms (kare 97 START) .ass'a 4030 olarak yazılır; frame_from_ms(4030) yine 97 — Aegisub ile aynı

## 0.1.0 — Aşama 1
- Bayt korumalı ASS modeli (BOM/CRLF, bilinmeyen alanlar, düzenlenen satırda değişmeyen alanların özgün metni)
- Automation 4 çalışma zamanı (lupa + LuaJIT): aegisub API, subtitles nesnesi, re/unicode/util/lfs/clipboard, karaskel, parse_karaoke_data, text_extents (GDI tanımı)
- İşlemsel makro çalıştırma (hata/iptal → belge değişmez), ayrı iş parçacığı, ilerleme + günlük penceresi, sert iptal
- Arayüz: ızgara, düzenleme kutusu (Shift+Enter → \N), stil yöneticisi, bul/değiştir, betik özellikleri, zaman kaydırma, satır işlemleri, CPL denetimi bölmesi, harici araçlar, koyu/açık tema
- CLI: doctor, roundtrip, macros, macro
- 60 test

## Bulunan ve düzeltilen hatalar (tekrar düşülmesin)
- `lupa.lua_type` üst modülden çağrılınca luajit21 nesnelerini tanımıyor → her varyantın kendi `lua_type`'ı kullanılmalı
- lupa'nın LuaJIT'i 5.2 uyumu olmadan derlenmiş: `__len` tablolarda çalışmaz, `__ipairs` yok → subtitles nesnesi `newproxy` userdata, `ipairs/pairs` üst yöntem desteğiyle sarıldı (karaskel `ipairs(subs)` kullanır)
- karaskel `subs.script_resolution()` çağırıyor → eklendi
- 23.976 fps'te kayan noktayla 24. kare 1000 ms çıkıyordu (1001 olmalı) → CFR zamanları kesirle (Aegisub gibi tamsayı) hesaplanıyor
- Aynı penceredeki dört QRadioButton Qt'de tek grup oluyor (zaman kaydırmada yön seçimi bozuluyordu) → açık QButtonGroup
- `encoding=None` lupa'da Python `str` Lua'ya userdata olarak gidiyor → tüm dönüşler bayta çevriliyor
