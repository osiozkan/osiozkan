# Aegisub Lua (Automation 4) uyumluluğu

osisub, Aegisub betiklerini değişiklik gerekmeden çalıştırmayı hedefler. Motor LuaJIT'tir (Aegisub ile aynı Lua 5.1 anlambilimi; `ffi` ve `bit` yerleşik).

## Desteklenen

**Kayıt:** `aegisub.register_macro(ad, açıklama, işlem, doğrulama?, etkin?)`, `aegisub.register_filter(ad, açıklama, öncelik, işlem, yapılandırma?)`, `script_name/description/author/version/namespace`.

**`subs` nesnesi:** `subs[i]` (okuma/yazma/silme `= nil`), `subs[0] = satır` (sona ekle), `#subs`, `subs.n`, `subs.append`, `subs.insert(i, ...)`, `subs.delete(...)`, `subs.deleterange(a, b)`, `ipairs(subs)`.
Satır sınıfları: `info`, `style`, `dialogue` (yorum dahil), `unknown`. Alanlar Aegisub ile aynıdır (`section = "[Events]"`, `margin_t/margin_b/margin_v`, `extra`, `raw`).

**`aegisub.*`:** `log` (düzeyli, `string.format` biçimli), `cancel`, `set_undo_point`, `progress.set/task/title/is_cancelled`, `dialog.display/open/save`, `text_extents` (gerçek yazı tipi ölçüleri; GDI/VSFilter uyumlu), `frame_from_ms`, `ms_from_frame`, `video_size`, `keyframes`, `get_audio_selection`, `project_properties`, `decode_path` (`?script ?video ?audio ?data ?user ?local ?temp`), `file_name`, `gettext`, `lua_automation_version = 4`.

**Kütüphaneler:** `include("karaskel.lua" | "utils.lua" | "unicode.lua" | "utf8.lua" | "karaskel-auto4.lua" | "utils-auto4.lua")`; `require "re"`, `"aegisub.re"`, `"unicode"`, `"aegisub.unicode"`, `"utf8"`, `"lfs"`, `"ffi"`, `"bit"`, `"aegisub.util"`, `"aegisub.clipboard"`, `"clipboard"`.
- `re`: `find`, `match`, `sub`, `gsub`, `compile` (+ `:find/:match/:sub/:gsub`), bayraklar `ICASE` vb. Motor `fancy-regex` (Perl benzeri; geri başvuru ve ileri/geri bakış var).
- `karaskel`: `collect_head`, `preproc_line` (+ `_text/_size/_pos`), `parse_syllables`, `do_basic_layout`; hece alanları `duration/start_time/end_time/tag/text/text_stripped/text_spacestripped/prespace/postspace/width/height/left/center/right/i`.
- `util`: renk/alfa dönüşümleri, HSV/HSL, enterpolasyon, `copy/deep_copy/clamp/uuid`, `string.trim/headtail/words`.

**İletişim kutuları:** sınıflar `label edit textbox intedit floatedit checkbox dropdown color coloralpha alpha`; düğmeler ve `button_ids`. CLI'de varsayılan değerlerle, `--param ad=değer` ile ezilerek çalışır.

**Filtreler:** GUI'de *Dosya → Dışa aktar (filtreler)…* öncelik sırasıyla çalıştırıp dosyaya yazar.

## Bilinen farklar / eksikler

- `text_extents` yazı tipi sistemde yoksa kaba tahmine düşer (Yazı tipi denetimi penceresi eksikleri gösterir).
- Birkaç Aegisub'a özgü ayrıntı (ör. `aegisub.dialog.display` içinde `selection` sınıfı, Lua `lpeg`) yoktur.
- `kara-templater.lua` ve Aegisub ile gelen otomatik yüklenen betikler pakete dahil değildir; kendi betiğiniz `include` ile aynı kütüphaneleri kullanabilir.
- `unicode.charwidth` vb. nadir işlevler eklenmemiştir.
- `aegisub.get_cursor`/ses seçimi canlı ses kutusundan değil, etkin satırdan türetilir.

## Sorun bildirme

Çalışmayan bir betik varsa: betiğin kendisini ve hata iletisini (Otomasyon sekmesi → Günlük) paylaşın; `crates/osisub-lua/tests/fixtures/` altına regresyon testi olarak eklenir.
