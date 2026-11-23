# Aegisub Lua (Automation 4) uyumluluğu

osisub, Aegisub betiklerini değişiklik gerekmeden çalıştırmayı hedefler. Motor LuaJIT'tir (Aegisub ile aynı Lua 5.1 anlambilimi; `ffi` ve `bit` yerleşik).

## Desteklenen

**Kayıt:** `aegisub.register_macro(ad, açıklama, işlem, doğrulama?, etkin?)`, `aegisub.register_filter(ad, açıklama, öncelik, işlem, yapılandırma?)`, `script_name/description/author/version/namespace`.

**`subs` nesnesi:** `subs[i]` (okuma/yazma/silme `= nil`), `subs[0] = satır` (sona ekle), `#subs`, `subs.n`, `subs.append`, `subs.insert(i, ...)`, `subs.delete(...)`, `subs.deleterange(a, b)`, `ipairs(subs)`.
Satır sınıfları: `info`, `style`, `dialogue` (yorum dahil). Aegisub 3 gibi `[Script Info]` içindeki `;` yorumları, `[Aegisub Project Garbage]`, `[Fonts]`/`[Graphics]` ekleri ve Extradata Lua'ya **gösterilmez** (dizinler bunlar atlanarak sayılır; dosyada korunur). Sınıfsız ya da bilinmeyen sınıflı tablo yazmak hata verir (`Table lacks 'class' field`). Alanlar Aegisub ile aynıdır (`section = "[Events]"`, `margin_t/margin_b/margin_v`, `extra`, `raw`).

`line.extra` (Aegisub Extradata) gerçek veridir: dosyadaki `[Aegisub Extradata]` bölümünden okunur, `subs[i] = line` / `subs.append` / `subs.insert` ile yazılınca bölüme işlenir ve satıra `{=kimlik}` ön eki olarak kaydedilir (Aegisub ile aynı `e`/`u` kodlaması; ön ek arayüzde görünmez). Aegisub-Motion, ASSFoundation ve satır başına ayar saklayan betikler verilerini korur. `subs.append` satırı Aegisub gibi kendi bölümünün sonuna ekler (olaylar `[Fonts]`'tan sonra düşmez).

**`aegisub.*`:** `log` (düzeyli, `string.format` biçimli), `cancel`, `set_undo_point`, `progress.set/task/title/is_cancelled`, `dialog.display/open/save`, `text_extents` (gerçek yazı tipi ölçüleri; GDI/VSFilter uyumlu), `frame_from_ms`, `ms_from_frame`, `video_size`, `keyframes`, `get_audio_selection`, `project_properties` (Aegisub'ın 14 alanı: `active_row`, `export_filters`, `style_storage`, `scroll_position` dahil), `set_status_text`, **`aegisub.gui.get_cursor/set_cursor/get_selection/set_selection/is_modified`** (düzenleme kutusundaki imleç/seçim, 1 tabanlı UTF-8 bayt; ayarlananlar makro bitince uygulanır — 2026-11-23), `decode_path` (`?script ?video ?audio ?data ?user ?local ?temp`), `file_name`, `gettext`, `lua_automation_version = 4`.

**Kütüphaneler:** `include("karaskel.lua" | "utils.lua" | "unicode.lua" | "utf8.lua" | "karaskel-auto4.lua" | "utils-auto4.lua")`; `require "re"`, `"aegisub.re"`, `"unicode"`, `"aegisub.unicode"`, `"utf8"`, `"lfs"`, `"ffi"`, `"bit"`, `"aegisub.util"`, `"aegisub.clipboard"`, `"clipboard"` (arayüzde gerçek sistem panosu; CLI'de betik içi bellek).
- `re`: `find`, `match`, `sub`, `gsub`, `compile` (+ `:find/:match/:sub/:gsub`), bayraklar `ICASE` vb. Motor `fancy-regex` (Perl benzeri; geri başvuru ve ileri/geri bakış var).
- `karaskel`: `collect_head`, `preproc_line` (+ `_text/_size/_pos`), `parse_syllables`, `do_basic_layout`; hece alanları `duration/start_time/end_time/tag/text/text_stripped/text_spacestripped/prespace/postspace/width/height/left/center/right/i`.
- `util`: renk/alfa dönüşümleri, HSV/HSL, enterpolasyon, `copy/deep_copy/clamp/uuid`, `string.trim/headtail/words`.

**İletişim kutuları:** sınıflar `label edit textbox intedit floatedit checkbox dropdown color coloralpha alpha`; düğmeler ve `button_ids`. CLI'de varsayılan değerlerle, `--param ad=değer` ile ezilerek çalışır.

**Filtreler:** GUI'de *Dosya → Dışa aktar (filtreler)…* öncelik sırasıyla çalıştırıp dosyaya yazar.

## osisub'a özgü API (`require "osisub"`)

Aegisub'da olmayan, osisub çekirdeğini betiklerden kullanmak için (betiğiniz Aegisub'da da çalışacaksa `local ok, osisub = pcall(require, "osisub")` ile koşullu kullanın):

| İşlev | Ne yapar |
|---|---|
| `osisub.tags.clean(metin)` | Gereksiz etiketleri siler (`{\fs20\fs30}` → `{\fs30}`, tekrarlanan `\i1`…) |
| `osisub.tags.lint(metin, süre_ms?)` | Etiket hataları: `{ {kind="error"\|"note", message=…}, … }` |
| `osisub.tags.strip_notes(metin)`, `osisub.tags.visible(metin)` | Not bloklarını sil / görünür metin |
| `osisub.text.upper/lower/title/sentence(metin, türkçe?)` | Harf dönüşümü (varsayılan Türkçe i/İ ı/I; etiketler, `\N` korunur) |
| `osisub.text.balance(metin, en_çok?)`, `osisub.text.unbreak(metin)` | İki dengeli satıra kır / satır sonlarını kaldır |
| `osisub.text.tr_fix(metin)` | Türkçe yaygın yazım hataları → `(düzeltilmiş, sayı)` |
| `osisub.furigana.strip(metin)`, `osisub.furigana.has(metin)` | `漢字\|かんじ` okunuşları |
| `osisub.romaji_to_kana(romaji)` | Hepburn/Kunrei → hiragana |
| `osisub.time.format(ms)`, `osisub.time.parse(metin)` | `0:00:00.00` ↔ ms |
| `osisub.render.bounds(satırlar, anlar)` | libass ile satır sınırları (SubInspector karşılığı) |

## 2026-11-21 bağımsız uyumluluk incelemesi (düzeltildi)
- `subs.delete(tablo)` (ör. `subs.delete(sel)`) artık tablodaki tüm indeksleri siler (eskiden hata).
- `re.split / gsplit / gmatch / gfind` (modül ve derlenmiş nesne); boost'taki `\h` (yatay boşluk); `re.ICASE` bakış ifadeli desende de çalışır.
- `extract_color` Aegisub sırasıyla (`&HAA&` saydamlık, eşleşmezse `nil`); `ass_color`/`interpolate_color` `%02X` gibi keser (0.5 arası siyah-beyaz → `&H7F7F7F&`); `interpolate` 0–1 dışında sınırlı; `deep_copy` metatablo kopyalamaz.
- İletişim kutusu `color`/`coloralpha` değerleri `#RRGGBB` / `#RRGGBBAA` döner (Aegisub `GetHexFormatted`); `#RRGGBB` başlangıç değeri doğru gösterilir.
- `unicode.to_upper_case / to_lower_case / to_fold_case`.
- Stil satırı renkleri `&HAABBGGRR&` (sonda `&`, Aegisub gibi).

## Bilinen farklar / eksikler

- `text_extents` yazı tipi sistemde yoksa kaba tahmine düşer (Yazı tipi denetimi penceresi eksikleri gösterir).
- Birkaç Aegisub'a özgü ayrıntı (ör. `aegisub.dialog.display` içinde `selection` sınıfı, Lua `lpeg`) yoktur.
- `kara-templater.lua` pakete dahildir (yerleşik); diğer Aegisub otomatik betikleri (ör. `strip-tags`, `select-overlaps`) dahil değildir.
- `unicode.charwidth` vb. nadir işlevler eklenmemiştir.
- `aegisub.get_cursor`/ses seçimi canlı ses kutusundan değil, etkin satırdan türetilir.

## Sorun bildirme

Çalışmayan bir betik varsa: betiğin kendisini ve hata iletisini (Otomasyon sekmesi → Günlük) paylaşın; `crates/osisub-lua/tests/fixtures/` altına regresyon testi olarak eklenir.

## 2026-11-10 eklemeleri
- **Gerçek Aegisub `karaskel-auto4.lua`** (BSD lisanslı, telif başlığı korunarak) artık `karaskel.lua` olarak gömülü; önceki yeniden yazımın eksikleri (0. hece `line.kara[0]`, furigana `line.furi`, çok vurgulu `#` heceleri, satır içi `\-fx`) giderildi.
- **`kara-templater.lua` yerleşik** olarak gelir (Otomasyon menüsünde "Apply karaoke template"). Kendi sürümünüzü `automation/autoload/kara-templater.lua` olarak koyarsanız yerleşik olanın yerine o yüklenir.
- Yeni API: `aegisub.debug.out`, `aegisub.parse_karaoke_data`, `subs.script_resolution()`, `unicode.charwidth`, `unicode.chars`; `include("utils.lua")` artık Aegisub'daki genel takma adları da tanımlar (`table.copy`, `copy_line`, `table.copy_deep`, `ass_color`, `interpolate_color`, `HSV_to_RGB` …).
- Test: `crates/osisub-lua/tests/templater.rs` gerçek kara-templater ile `code once`, `template syl noblank`, `template line`, `!retime()!`, `$sx/$sy/$sstart/$send` değişkenlerini doğrular.
- **MoonScript (`.moon`) betikleri** çalışır: Aegisub gibi MoonScript 0.5.0 derleyicisi gömülüdür (LPeg yerine saf Lua LuLPeg). `.moon` dosyaları otomatik yükleme klasöründen, "Lua betiği yükle" ile ya da sürükle-bırak ile açılabilir.
- **`require` include klasörlerinde arar**: `automation/include/a/b.lua`, `a/b/init.lua`, `a/b.moon`, `a/b/init.moon` (ör. `require "l0.DependencyControl"` tarzı modül düzeni). 

## DependencyControl (l0.DependencyControl)
osisub gerçek DepCtrl yerine **yerleşik bir uyumluluk katmanı** kullanır (gerçeği Windows DLL'leri ve ağ güncellemesi ister):
- `DependencyControl{ ...bağımlılıklar... }`, `requireModules()` (isteğe bağlı modüller `optional = true` → nil; eksik zorunlu modüller adı/sürümü/adresiyle tek hata iletisinde listelenir; düşük sürüm uyarısı), `register()`, `registerMacro(s)` (alt menü: "Betik adı/Makro"), `getConfigHandler()` (`?user/config/<namespace>.json`, `c`, `load`, `write`, `getSectionHandler`), `getLogger()`/`logger`, `checkVersion`, `parseVersion`, `getVersionString`.
- Betik ortamındaki `script_name/description/author/version/namespace` okunur; eksikse DepCtrl kaydından doldurulur.
- Güncelleme/indirme YAPILMAZ: modülleri (ASSFoundation, a-mo, l0.Functional…) `automation/include` klasörüne kurun.
- Yedek `json` modülü (encode/decode) gömülüdür; include klasöründe kendi `json.lua`'nız varsa o kullanılır.
- **Aegisub klasörleri otomatik:** kurulu Aegisub'ın `automation/include` klasörü her zaman aranır, `automation/autoload` betikleri de (Otomasyon sekmesinden kapatılabilir) yüklenir. Windows `%APPDATA%\Aegisub`, Linux `~/.aegisub`, macOS `~/Library/Application Support/Aegisub`; `OSISUB_AEGISUB_DIR` ile değiştirilebilir.
- Makro adlarındaki `/` Aegisub gibi iç içe alt menü oluşturur.
- **`BM.BadMutex` ve `PT.PreciseTimer` yerleşik** (2026-11-12): gerçekleri ffi ile yerel DLL yükler; osisub saf karşılıklarını sunar (`lock/tryLock/unlock`; `PT()`, `:timeElapsed()` saniye, `PT.sleep(ms)`), DepCtrl sürüm kaydıyla. ASSFoundation / a-mo'nun bu bağımlılıkları artık karşılanır.
- **`SubInspector.Inspector` yerleşik** (2026-11-15): gerçeği libass'i ffi ile yükleyen bir DLL'dir (ASSFoundation `getLineBounds`, Aegisub-Motion kullanır). osisub satırları betiğin başlığı/stilleriyle, video olmadan libass (ffmpeg) ile çizer: `SubInspector(subs)`, `:getBounds(satırlar)` → `bounds, times` (her an için `{x, y, w, h, hash, solid}` ya da `false`; `si_exhaustive = true` olan satırda her kare, yoksa ilk an). Kapsama siyah/beyaz zemin farkından gerçek saydamlıkla ölçülür (kenarlık, gölge, yarı saydam pikseller dahil); `hash` görünüme bağlıdır, konuma değil. Kare zamanları açık videodan (yoksa 23.976). ffmpeg gerekir.
- Doğrulanmayan: gerçek ASSFoundation / Aegisub-Motion modülleriyle uçtan uca çalıştırma (modüller depoda yok).

## Belgeye bağlı betikler
Aegisub'ın `[Aegisub Project Garbage]` → `Automation Scripts:` alanı okunur ve yazılır: dosya açılınca betikler yüklenir; Otomasyon sekmesindeki "belgeye bağlı" kutusu betiği bu alana ekler/çıkarır (kaydedince yazılır).

## aegisub.get_frame (Aegisub 3.3+ / arch1t3cht)
`aegisub.get_frame(kare, altyazılı?)` açık videodan tam çözünürlükte kare döndürür: `frame:width()`, `frame:height()`, `frame:getPixel(x, y)` (0xRRGGBB), `frame:getPixelFormatted(x, y)` ("&HBBGGRR&"). Videodan renk alan betikler çalışır. Video açık değilse `nil`.


## 2026-11-22 uyumluluk düzeltmeleri
- `aegisub.parse_karaoke_data`: Aegisub 3 `AssKaraoke` ile birebir — `[0]` her zaman boş (`tag=""`), ilk `\k` öncesi metin `[1]` olur, süresi 0 ve metni boş heceler atlanır (etiketleri sonrakine geçer), `\k`'dan önceki etiketler önceki hecenin sonuna yazılır, `tag` ters eğik çizgili (`"\\k"`, `"\\kf"`; `\K` → `\kf`), süre tam sayıya kesilir.
- Satır zamanları yazılırken Aegisub gibi kesilir ve santisaniyeye yuvarlanır (`1234.7` → `1230`), [0, 10 sa) aralığına sıkıştırılır.
- `re.sub(s, p, işlev)`: gruplu desende işlev her yakalama grubu için `(str, first, last)` ile çağrılır; dize dönmezse özgün metin kalır. `re.ICASE` vb. bayraklar sayı değil işaretli nesne (konumsal bağımsız değişkenlerle karışmaz); `compiled:match(s, başlangıç)`; boş desen hata verir.
- `aegisub.dialog.display`: "Cancel" etiketli özel düğme etiketini döndürür (false yalnızca `button_ids.cancel` ya da varsayılan OK/Cancel'da); `intedit` sınırlara kıstırılır ve tam sayı döner; `checkbox` yalnızca gerçek boolean; `textbox`/`edit` değeri dizedir, `text` önceliklidir; `dropdown` varsayılanı `""`.
- `util.HSV_to_RGB` (yuvarlanmamış, `|H|`), `util.HSL_to_RGB` (S/L kıstırılır), `string.trim` (iki değer), `string.words` (headtail tabanlı) util.moon'dan birebir.
- `unicode.len/codepoint/chars/charwidth` bayt tabanlı (geçersiz UTF-8'de hata yok); `aegisub.log` yarım UTF-8'i yazar.
- `aegisub.decode_path("?video/x")` video yokken yolu değiştirmeden döndürür; `aegisub.video_size()` video yokken tek `nil` döner.
- **Uyumluluk uyarıları:** Aegisub'da hata verip osisub'da çalışan kullanımlar (`subs[0]`, `subs[#subs+5]` okuma/yazma, aralık dışı `subs.insert`/`subs.delete`, `subs.bilinmeyen`) günlüğe "Uyumluluk uyarısı" düşer (makro başına en çok 5). Burada geliştirdiğiniz betiğin Aegisub'da da çalışacağından emin olmak için Otomasyon günlüğüne bakın.
