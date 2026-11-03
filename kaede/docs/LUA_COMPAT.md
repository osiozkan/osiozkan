# Aegisub Lua uyumluluğu

Çalışma zamanı: **LuaJIT 2.1** (Aegisub ile aynı) — `ffi`, `bit`, `goto`, `jit` mevcuttur. Her betik kendi Lua
durumunda (state) çalışır; `include("x.lua")` ve `require "x"` arama yolu: kütüphane klasörü, betik klasörü,
`<betik>/include`, `~/.kaede/automation/include`, Aegisub include klasörleri, `KAEDE_INCLUDE`.

## Desteklenenler
- Başlık globalleri: `script_name`, `script_description`, `script_author`, `script_version`, `script_namespace`
- `aegisub.register_macro(name, desc, process, validate, is_active)`, `aegisub.register_filter(...)`
- `subs`: `subs[i]` oku/yaz, `#subs`, `subs.n`, `ipairs(subs)`, `subs.append/insert/delete/deleterange`, `subs[i]=nil`
  - satır sınıfları: `info`, `style`, `dialogue` (alanlar Aegisub 3.x ile aynı: `start_time`, `end_time`, `margin_t/b`, `extra`, `raw`, `section`…)
  - indeksler Aegisub gibi **tüm girdiler üzerinden** (bilgi + stiller + diyaloglar); seçim/aktif satır buna göre geçirilir
- Dönüş: `return selection, active` yeni seçimi uygular
- `aegisub.log / debug.out`, `progress.*`, `cancel()`, `dialog.display` (label, edit, intedit, floatedit, textbox, dropdown, checkbox, color, coloralpha, alpha; düğmeler + `button_ids`), `dialog.open/save`
- `aegisub.text_extents` (Qt yazı tipi ölçüleri), `frame_from_ms`, `ms_from_frame`, `video_size`, `keyframes`, `project_properties`, `decode_path` (`?script ?data ?user ?video ?audio ?temp`), `clipboard`, `gettext`, `set_undo_point`
- Kütüphaneler: `karaskel` (collect_head, preproc_line*, parse_karaoke_data), `util`, `utils` (eski), `utf8`, `unicode`, `re` (Perl söz dizimi, Python `re` üstünden), `lfs`, `clipboard`, `ffi`, `bit`
- Aegisub'ın geri al davranışı: makro tek geri-al adımıdır; `cancel()` değişiklikleri geri çevirir

## Bilinen farklar / yapılacaklar
- `re` motoru Boost değil Python `re`; Perl-uyumlu ifadelerde fark nadirdir. `\U \L` biçim dizileri henüz yok.
- `.moon` (MoonScript) betikleri henüz yüklenmez (derlenmiş `.lua` çalışır).
- DependencyControl'ün Windows'a özgü `ffi` çağrıları çalışmayabilir.
- `karaskel.parse_karaoke_data` furigana (`|<`) ayrıştırmasını henüz içermez.
- `aegisub.get_audio_selection` yalnızca dalga biçimi seçimi bağlanınca dolar.
