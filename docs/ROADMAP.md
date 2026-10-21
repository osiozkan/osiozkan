# nightsub — yol haritası

Hedef: Aegisub'ın tüm yeteneklerini + Subtitle Edit'in iyi özelliklerini tek, hızlı ve kolay arayüzlü
bir anime altyazı aracında toplamak; mevcut Aegisub Lua (Automation 4) betikleri **değişmeden** çalışmalı.

Teknoloji: Rust (çekirdek + CLI + GUI/egui), LuaJIT 2.1 (mlua, vendored, 5.2-compat) — Aegisub'ın kullandığı Lua ile aynı sınıf.
Video/ses: sistemdeki `ffmpeg`/`ffprobe`/`ffplay` (libass tabanlı `subtitles` filtresiyle gerçek ASS render).

## Durum (✅ bitti · 🟡 kısmen · ⬜ yok)

### Çekirdek (`nsub-core`)
| Alan | Durum |
|---|---|
| ASS/SSA okuma-yazma (SSA v4 dönüşümü, `[Aegisub Project Garbage]` vb. korunur, proje özellikleri) | ✅ |
| SRT / WebVTT / MicroDVD / LRC içe-dışa aktarım, kodlama tespiti (Türkçe cp1254 dahil) | ✅ |
| Override tag ayrıştırıcı, karaoke (`\k`) ayrıştırıcı (AssKaraoke ile aynı mantık) | ✅ |
| Zaman: kaydır, iki noktalı senkron, fps dönüşümü, kareye yapıştır, lead-in/out | ✅ |
| Bul/değiştir (regex, etiket atlama), böl/birleştir, seç, sırala | ✅ |
| Kalite kontrol (CPS, süre, satır uzunluğu, çakışma, bilinmeyen stil, …) + otomatik düzeltme | ✅ |
| Subtitle Edit tarzı düzeltmeler: harf dönüşümü (Türkçe İ/ı), HI temizliği, satır dengeleme, uzun satır bölme, kopya birleştirme, CPS düzeltme | ✅ |
| Çözünürlük yeniden örnekleme (stil + `\pos/\move/\clip/\fs/\bord/\t` + çizimler) | ✅ |
| Kullanılmayan stilleri silme | ✅ |
| Extradata (`[Aegisub Extradata]`) | ⬜ (ham olarak korunur, Lua `extra` boş) |

### Lua otomasyon (`nsub-lua`)
| Alan | Durum |
|---|---|
| `aegisub.*` API, `subs` nesnesi (Aegisub ile aynı indeksleme/append/insert/delete kuralları), `parse_karaoke_data` | ✅ |
| register_macro / register_filter, dialog (GUI'de gerçek pencere, ayrı iş parçacığı), ilerleme, iptal | ✅ |
| Aegisub'ın kendi `include/*.lua` dosyaları gömülü (utils, karaskel, cleantags, …) — birebir | ✅ |
| `aegisub.util/unicode/re/lfs` (re: fancy-regex, lookaround destekli) | ✅ |
| LPeg + MoonScript (`.moon` betik ve modüller) | ✅ |
| Aegisub'ın stok betikleri çalışıyor: kara-templater, cleantags, strip-tags, … (`AEGISUB_AUTOMATION=… cargo test`) | ✅ |
| DependencyControl: çevrimdışı uyumluluk katmanı (config handler, logger, requireModules, registerMacros) | 🟡 |
| Gerçek DependencyControl 0.7 (`--real-depctrl`) | ⬜ deneysel, çalışmıyor |
| SubInspector (satır sınırı ölçümü) | ⬜ (taslak modül hata verir) |
| `aegisub.get_audio_selection`, `aegisub.video_size` gerçek video ile | 🟡 |
| Kullanıcının mevcut Aegisub klasörü (`%APPDATA%\Aegisub\automation`) otomatik yüklenir | ✅ |

### Arayüz (`nsub-gui`)
| Alan | Durum |
|---|---|
| Satır tablosu (100k+ satır akıcı), düzenleyici, geri al/yinele, otomatik kaydetme | ✅ |
| Stil yöneticisi, bul/değiştir, zaman kaydırma, Araçlar menüsü (düzelt, seç, post-processor, özellikler, yazı tipleri) | ✅ |
| Video önizleme (ffmpeg + libass), kare/saniye adımları, ses dalga formu, sürüklenebilir başlangıç/bitiş, anahtar kare yapışması | ✅ |
| Satırı çal (ffplay), Boşluk = çal/durdur | ✅ |
| Çeviri modu: kaynak (referans) altyazı yan yana | ✅ |
| Türkçe/İngilizce arayüz, koyu/açık tema, ayarlar kalıcı | ✅ |
| Video oynatma (gerçek zamanlı), ses spektrumu | ⬜ |
| Görsel typesetting (video üstünde `\pos` sürükleme, döndürme, klip) | ⬜ |
| Karaoke: eşit bölme, etiket silme | ✅ · görsel zamanlama düzenleyici ⬜ |
| Yazım denetimi (Türkçe Hunspell), makine çevirisi | ⬜ |
| Klavye kısayolu özelleştirme | ⬜ |
| Windows/macOS/Linux sürüm paketleri (GitHub Actions `release.yml`) | 🟡 (yazıldı, henüz çalıştırılmadı) |

## Her gece için kural
1. `docs/NIGHTLY.md` sonuna o gecenin kaydını ekle (ne yapıldı, ne kaldı, bilinen sorunlar).
2. `cargo test --workspace` yeşil olmadan push etme.
3. Sabah 06:00'ya (TR) kadar çalış; bitince özet bildirimi gönder.
4. Uyumluluk testi için: `GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 https://github.com/arch1t3cht/aegisub` ve
   `AEGISUB_AUTOMATION=<klon>/automation cargo test -p nsub-lua --test stock_scripts`.
