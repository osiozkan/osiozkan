# Değişiklik günlüğü

## 0.7.0 (hazırlanıyor)
- **Yazım denetimi** (Araçlar → Yazım denetimi): Hunspell sözlükleriyle (saf Rust `spellbook`), Türkçe sözlük LibreOffice deposundan ilk kullanımda indirilir ve sayısal bayrakları yamalanır; `Ahmet'in` gibi ek almış özel adlar kabul edilir; öneriler, tümünü düzelt, yoksay, kullanıcı sözlüğüne ekle. Yeni crate: `yuzu-spell`.
- **MoonScript**: `.moon` betikleri ve `require` ile `.moon` modülleri çalışır (gömülü MoonScript 0.5.0 ve saf Lua LuLPeg; lisanslar `crates/yuzu-lua/lua/vendor/`). Autoload klasörlerinde `.moon` dosyaları da taranır.
- **Konuşmadan satır oluştur** (Video menüsü / palet): sesteki konuşma bölümleri için, mevcut satırlarla çakışmayanlara boş satır ekler; tek geri alma adımı.

## 0.6.0 — Lua otomasyonu (2026-10-20)
- **Hata izi**: makro, doğrulama ve yükleme hataları yığın iziyle gelir (`dosya.lua:12: in function 'f'`);
  çağrı düzeneğinin kendi çerçeveleri gizlenir.
- **Doğrulama ve is_active**: doğrulama işlevi olan makrolar o seçim için uygun değilse Otomasyon menüsünde gri;
  `is_active` doğruysa ✔ ile işaretli. Doğrulama otomasyon iş parçacığında tek seferde, belge bir kez kurularak
  yapılır. Paletten ya da kısayolla çalıştırılan uygun olmayan makro çalıştırılmaz, durum çubuğu nedenini söyler.
- **Otomatik yeniden yükleme**: betik klasörlerindeki ya da `include` altındaki bir `.lua` değişince, eklenince ya
  da silinince betikler 1,5 sn içinde yeniden yüklenir (Ayarlar → Betik klasörleri; varsayılan açık).
- **Otomasyon yöneticisi**: betikler, sürüm, yazar, makro sayısı, dosya; seçilen betiğin makroları (doğrular /
  işaretlenebilir) ya da hatası (yığın iziyle).
- **Aegisub API eksikleri**: `aegisub.set_undo_point` (yoktu; bunu çağıran çok sayıda betik hata veriyordu),
  `aegisub.file_name`, `aegisub.set_status_text`, `aegisub.get_frame` (açık bir "desteklenmiyor" hatası);
  `require "aegisub.unicode"`, `"aegisub.clipboard"`, `"aegisub.lfs"`; `aegisub.util.trim/headtail/words`.
- **Extradata**: `[Aegisub Extradata]` satırların `extra` tablosu olarak betiklere gider ve geri yazılır
  (Aegisub'ın `e…`/`u…` kodlamasıyla; kullanılmayan kayıtlar atılır). Satır başındaki `{=1}` işaretçisi ızgarada
  ve düzenleyicide gösterilmez, düzenlemede ve görsel araçlarda korunur.
- `subs[0] = satır` sona ekler, `subs[-i] = satır` i'nin önüne ekler (Aegisub kuralları).
- **DependencyControl** katmanı: modül listesi ilk konumsal öğe olarak verildiğinde yanlış okunuyordu (düzeltildi);
  `getConfigHandler` (JSON, `?user/config/<namespace>.json`), `registerMacros` betik adında alt menüye.
- **json** modülü (encode/decode); kullanıcının kendi `json.lua`'sı varsa o kullanılır.
- Derleyici uyarıları sıfırlandı (kullanılmayan doğrulama kodu artık kullanılıyor).
- Testler: Lua 6 → 13 (yığın izi, doğrulama/is_active, extradata, dizin kuralları, DependencyControl + json +
  yapılandırma, LuaJIT ffi/bit, tipik bir Aegisub 3.x betiği), asscore +4 (extradata kodlamaları, işaretçi),
  arayüz 32 → 35 (gri makro ve çalıştırılmama, değişince yeniden yükleme + yöneticide hata, gizli işaretçi).

## 0.5.0 — Aegisub eşdeğerliği (2026-10-20)
- **Satır panosu** (Aegisub gibi): Ctrl+C / Ctrl+X seçili satırları `Dialogue:` satırları olarak panoya alır,
  Ctrl+V etkin satırın üstüne yapıştırır (düz metin satırları yeni satır olur). Metin kutusundayken panoda altyazı
  satırları varsa Ctrl+V yine satır yapıştırır. **Üstüne yapıştır** (Ctrl+Shift+V): hangi alanların (yorum, katman,
  başlangıç, bitiş, stil, aktör, kenarlar, efekt, metin) yazılacağı seçilir; tek satır seçiliyse etkin satırdan
  aşağı, birden çok satır seçiliyse seçili satırların üstüne. Seçim ayarlarda saklanır.
- **Satır seç** (Aegisub'un Select Lines'ı): metin / stil / aktör / efekt / etiketsiz metin; içerir, tam olarak,
  düzenli ifade; harf duyarlı, eşleşmeyenler, diyalog/yorum; seçimi değiştir, ekle, çıkar, kesiştir.
- **Stil deposu** (stil kataloğu): stil yöneticisi iki sütunlu — solda depo (`catalog/<ad>.sty`, Aegisub biçimi),
  sağda betik; iki yönlü kopyalama (aynı adlı stil değiştirilir), depo oluşturma/silme, başka betikten stil alma,
  sıralama, yukarı/aşağı taşıma, seçili satırlara uygulama, renk/kenarlık/gölgeyle örnek önizleme.
- **Çoklu değiştir** (Subtitle Edit): kayıtlı kural grupları (normal, harf duyarsız, düzenli ifade), sırayla
  uygulanır; etiketlere dokunmama, yalnız seçili satırlar, yorumları atlama; önce/sonra önizleme, satır satır
  seçerek uygulama, tek geri al. Kurallar `multi_replace.tsv` dosyasında.
- **Çeviri asistanı**: satır satır, etiket dışındaki her metin parçası için; özgün metin vurgulu, referans altyazı
  varsa altında; Enter kabul eder ve ilerler, Shift+Enter satır sonu (`\N`), boş Enter özgünü korur; özgünü
  yerleştir, satırı oynat, videoyu satıra götür.
- **Stil asistanı**: birkaç harf yazıp Enter ile etkin satıra stil verir ve sonraki satıra geçer.
- **Görsel dizgi araçları** genişledi: `\move`'un iki ucu sürüklenir, pos ⇄ move çevirme; Z döndürme artık göreli
  (Shift ile 15°), X/Y eğme (`\frx`/`\fry`), ölçek (`\fscx`/`\fscy`, Shift oranı korur), dikdörtgen clip
  köşelerinden ve içinden düzenlenir, **vektör clip** (tıkla ekle, sürükle taşı, sağ tık sil; ölçekli çizimler
  okunur), clip'i ters çevirme/kaldırma. Her sürükleme tek geri alma adımı.
- **Karaoke kipi** (zamanlama görünümü): etkin satırın heceleri ve sınırları görünür; sınır sürüklenerek hece
  süreleri ayarlanır (toplam değişmez), heceye tıklamak onu çalar.
- Düz aramada (bul, satır seç, çoklu değiştir) Türkçe büyük/küçük harf: "ışık" "IŞIK"ı, "izmir" "İzmir"i bulur.
- Zaman kaydırma: "seçimden itibaren" gerçekten çalışıyor (önceden "tüm satırlar" gibi davranıyordu); kare kipinde
  "yalnız başlangıç / bitiş" seçeneği uygulanıyor; "500" artık 500 ms (önceden 500 saniye okunuyordu).
- Stil/aktör/efekt alanındaki virgül, satırı bozmasın diye Aegisub gibi `;` olarak yazılır.
- Arayüz yazı tipine sembol (oklar vb.) ve CJK yedeği eklendi (Segoe UI Symbol/Arial, DejaVu Sans; Yu Gothic,
  Noto CJK, IPA Gothic, WenQuanYi…).
- Hata: zamanlama görünümünde kenar sürüklemesi bırakılırken ikinci bir geri alma adımı oluşuyordu.
- Testler: asscore 106 → 117 (pano, üstüne yapıştır, satır seç, katalog, çoklu değiştir, move/vektör clip),
  arayüz 21 → 32 (pano olayları, üstüne yapıştır, satır seç, stil deposu, çoklu değiştir, çeviri ve stil asistanı,
  kaydırma kapsamları, görsel araç sürüklemeleri, karaoke sınırları).

## 0.4.0 — Osi'nin araçları (2026-10-20)
- **Referans altyazı** (*Çeviri → Referans paneli…*, `--reference dosya` ile açılışta): İngilizce (veya başka dilde)
  altyazı yanda açılır; ızgarada isteğe bağlı **Referans sütunu**, düzenleyicinin üstünde etkin satırın karşılığı.
  **Kayma** elle girilir ya da *Kaymayı bul* ile kapsama ilintisinden (±15 sn) bulunur; düşük güvende uygulanmaz.
  *Referans metnini belgeye aktar*: her referans satırı en çok çakıştığı satıra yazılır (önizleme → uygula, tek geri al;
  baştaki etiketleri koruma, özgün metni `{EN: …}` olarak saklama, referansın zamanlarını kullanma, ev sahipsiz
  referans satırlarını ekleme seçenekleri).
- **Kaynakla karşılaştır** (`asscore::compare`, Osi'nin aligncompare'i): kaynak (referans ya da seçilen dosya) ile
  çeviri satırları zamana göre eşlenir (1:1, 1:2, 2:1 … gruplar); kayma otomatik bulunur. Bayraklar: EKSİK, FAZLA,
  İNGİLİZCE? (çevrilmemiş satır), UZUNLUK, BOŞ, ZAMAN, SAYI. Pencerede özet, bayrak süzgeçleri, tıklayınca satıra git,
  *bayraklı satırları seç*, *eksik satırları yorum olarak ekle* (tekrar eklemez), *kaymayı referansa uygula*,
  **HTML rapor / CSV / işaretli kopya** (bayraklar satırın Efekt alanında).
- **Parçalı senkron** (`asscore::sync`, bsync / bsnap): kayma yalnız sahne aralarında değişir; her sahne için en iyi
  kayma dinamik programlamayla seçilir (değişim cezası, sıra korunur). Zaman kaynağı **referans altyazı** ya da
  **sesteki konuşma** (`yuzu_media::vad`: modelsiz konuşma algılama, 5 eşikli oylama, arka planda). Tabelalar, ekran
  yazıları ve OP/ED sahneleriyle birlikte kayar ama hizalamaya katılmaz. Plan → sahne listesi (kayma, fark payı,
  notlar) → tek geri alla uygula; belge planlamadan sonra değiştiyse uygulamaz.
- **Terim denetimi** (`asscore::glossary`, termcheck): `terimler.tsv` (kaynak, türkçe, yasaklar, not) betiğin
  yanında ya da ayar klasöründe ise kendiliğinden yüklenir. Yasaklı karşılıklar Türkçe ek korunarak düzeltilir
  ("Stantlar" → "Standlar"; yalnız etiket dışındaki metin); referansta geçen terimin çevirisi yoksa uyarır. Bulgular
  kalite denetiminde de çıkar.
- **Osi CPL kuralı**: tek satır 42 karakteri aşarsa uyarı; `\N` ile bölünmüş satırın bir parçası 42'yi aşarsa hata.
  Izgarada **CPL sütunu** (en uzun parça; renkli), kalite denetiminde "43+16" gibi parça uzunlukları.
- **Komut satırı**: `yuzu compare` (HTML/CSV/işaretli kopya, `--only-flags`, eşikler), `yuzu sync-ref`
  (referansa parçalı senkron), `yuzu glossary` (`--ref` ile eksik terimler, `--fix` ile düzeltme).
- Testler: karşılaştırma (kayma ±20 ms içinde, 4 senaryo), senkron, terimler, aktarım, CPL, konuşma algılama;
  arayüz testleri: referans kayması + sütun + eksik satırlar, sahne sahne referansa senkron, terim dosyasının
  kendiliğinden bulunması ve düzeltme (arayüz testleri 18 → 21, asscore 106).

## 0.3.0 — kullanıcı deneyimi (2026-10-20)
- **Türkçe arayüz** (`i18n.rs`): tüm menüler, paneller, pencereler, durum çubuğu ve kalite denetimi iletileri
  Türkçe; *Ayarlar*'dan İngilizceye geçilir (anında). Bir test, arayüze konan her metnin çevirisi olduğunu denetler.
- **Komut listesi** (`commands.rs`): 90 komut tek listede; menüler, kısayollar ve palet buradan beslenir. Menülerde
  kısayollar sağda yazar. Tek tuşlar ve oklar metin kutusu düzenlenirken çalışmaz (Ctrl+←/→ artık sözcük atlamayı
  bozmaz); Ctrl+Shift+Z gibi çok değiştiricili kısayollar önce denenir.
- **Komut paleti** (Ctrl+Shift+P): komutlar ve Otomasyon makroları; Türkçe karakter duyarsız bulanık arama
  ("tum satirlari sec"), son kullanılanlar üstte, `:123` satıra gider.
- **Ayarlar** (Ctrl+,): dil, tema, arayüz boyutu, otomatik yedek, CPS/CPL sınırları, kare hızı; ses düzeyi, anahtar
  kare yapıştırma, etkin satırı izleme, spektrum, karaoke gecikmesi; betik klasörleri; **kısayol düzenleyici**
  (tuşa basarak kaydet, ikinci kısayol, sıfırla, arama; çakışma varsa kaydetmez). Ayar dosyasına yalnız değişen
  kısayollar yazılır.
- Düğme ipuçları kısayolları ayarlardan okur. Ok tuşları menülerde ⏴⏵⏶⏷ ile yazılır (← → varsayılan yazı tipinde yoktu).
- Lua iletişim kutularının varsayılan Tamam/İptal düğmeleri Türkçe (betiğe özgün ad döner).
- Testler: kısayol okuma/yazma, çakışma, kapsam kuralları, bulanık arama, palet, ayar dosyası, kısayol değiştirme,
  Ctrl+Shift+Z, çeviri tamlığı (arayüz testleri 6 → 18).

## 0.2.0 — medya motoru (2026-10-20)
- `yuzu-media::mpv` + `player`: libmpv çalışma anında yüklenir (derlemede bağımlılık yok). Kare doğruluğunda
  atlama (10/10 karede piksel düzeyinde doğrulandı), sesle eşzamanlı oynatma, satır oynatmada tam son karede durma,
  duraklatılmışken bile **canlı altyazı** (her düzenlemede en geç 120 ms'de yenilenir). libmpv yoksa eski ffmpeg yolu.
- `yuzu-media::audio`: ses bir kez 16 kHz PCM'e çözülür; dalga formu ve **mel ölçekli spektrum** (FFT, dosyaya göre
  normalleştirilmiş). Zamanlama görünümünde Dalga formu / Spektrum düğmesi, ses düzeyi.
- Aegisub zaman–kare kuralları (`Fps::line_start/line_end/frame_for_start/frame_for_end/frame_at`): F1/F2, kareyle
  kaydırma, karelere yapıştırma ve anahtar kare yapıştırması santisaniye yuvarlamasından sonra da doğru kareye düşer
  (önceden bir kare kayabiliyordu). Lua `aegisub.frame_from_ms`/`ms_from_frame` Aegisub ile aynı anlamda.
- Aegisub zamanlama tuşları (S R Q W E D T H Space Z X G Enter 1 2 A F) ve faresi (sol tık başlangıç, sağ tık bitiş,
  Ctrl/orta tık oynatma imleci); yakınlaşınca kare çizgileri; oynatırken görünüm imleci izler.
- Karaoke "tap" zamanlaması libmpv ile gerçek oynatma konumunu kullanır.
- Lua'ya video yolu, boyutu, anahtar kareler ve video konumu verilir (`aegisub.project_properties`, `keyframes`,
  `video_size`).
- Hata: ızgarada satırın yazısına tıklamak satırı seçmiyordu (seçilebilir etiketler tıklamayı yutuyordu).
- `tools/bundle.py`: tüm kodu tek .txt'ye paketle / geri aç.
- Testler: 4 libmpv oynatıcı testi (gerçek video), 3 ses/spektrum testi, kare kuralları testi (5 kare hızı × 3000
  kare), 6 ekransız arayüz testi (tuşlar, tıklamalar, kare doğru F1/F2, anahtar kare yapıştırma, ızgara).

## 0.1.0 — ilk gece (2026-10-17)
- `asscore`: ASS/SSA/SRT/VTT okuma-yazma, etiket ayrıştırma, karaoke, QC, zamanlama araçları
- `yuzu-lua`: Aegisub uyumlu Lua otomasyonu (macro/filter, karaskel, re, unicode, util, dialog köprüsü)
- `yuzu-media`: ffmpeg tabanlı video kare/ses/anahtar kare
- `yuzu-gui`: egui arayüzü (tablo, düzenleyici, dalga formu, video, stiller, QC, otomasyon menüsü)
- `asscore::formats` (MicroDVD, SubViewer2, SBV, LRC, TTML, SAMI, CSV, JSON, metin) ve `asscore::fixes` (yaygın hata düzeltmeleri, Türkçe büyük/küçük harf, Netflix QC)
- `yuzu-fonts`: font toplama/gömme; `automation/`: kara-templater ve stok makrolar
- `yuzu` CLI: info/convert/shift/qc/fix/resample/macros/run
