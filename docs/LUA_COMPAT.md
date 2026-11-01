# Aegisub Auto4 (Lua) uyumluluğu

Motor: **LuaJIT 2.1** (Aegisub ile aynı Lua 5.1 anlambilimi; `bit`, `ffi`, `unpack`, `loadstring`, `setfenv` vardır).

| Özellik | Durum |
|---|---|
| `script_name/description/author/version` | ✅ |
| `aegisub.register_macro(name, desc, process, validate, is_active)` | ✅ (`is_active` henüz kullanılmıyor) |
| `aegisub.register_filter(name, desc, priority, process, config)` | 🟡 kayıt var, GUI'den çağrı yok |
| `subs[i]`, `subs[i]=l`, `#subs`, `subs.n`, `insert/append/delete/deleterange`, `subs[0]=l` | ✅ (nokta ve iki nokta sözdizimi) |
| Satır sınıfları: `info`, `style`, `dialogue` (`comment` alanıyla), `unknown`; `section`, `raw`, `extra` | ✅ |
| `margin_t/margin_b` + `margin_v` takma adı | ✅ |
| Seçim/aktif satır dönüşü (`return sel, active`), ekleme/silmede seçim yeniden eşleme | ✅ |
| `aegisub.cancel()` | ✅ |
| `aegisub.log([level,] fmt, ...)`, `aegisub.debug.out` | ✅ günlük penceresi |
| `aegisub.progress.set/task/title/is_cancelled` | ✅ |
| `aegisub.dialog.display` (label, edit, intedit, floatedit, textbox, dropdown, checkbox, color, coloralpha, alpha) | ✅ (+slider) |
| `aegisub.dialog.open/save` | ✅ |
| `aegisub.text_extents(style, text)` | ✅ Qt yazı tipi ölçüleri (kurulu yazı tipleri) |
| `aegisub.frame_from_ms/ms_from_frame/video_size/keyframes/project_properties/decode_path` | ✅ (CFR; VFR ⬜) |
| `aegisub.clipboard` / `require "aegisub.clipboard"` | ✅ |
| `include("karaskel.lua")`, `include("utils.lua")`, `require "aegisub.util"`, `require "re"`, `require "unicode"`, `lfs` | ✅ |
| `karaskel.collect_head/preproc_line(+_text/_size/_pos)/parse_syllables` | ✅ (kendi uygulamamız; Aegisub çıktısıyla birebir doğrulanmadı) |
| Lua dizeleri bayt düzeyinde (geçersiz UTF-8 parçaları dahil) | ✅ |
| `aegisub.re` (Boost yerine Python `re`; `find/gfind/match/sub/split/compile`) | 🟡 Perl söz dizimi farkları olabilir |
| MoonScript (`.moon`) | ⬜ |
| `lpeg`, `lua-cjson` dış modüller | ⬜ |

Doğrulanmamış varsayımlar (kullanıcının betikleriyle test edilmeli):
`util.HSV_to_RGB` aralıkları (H derece, S/V 0–1), `karaskel` alan adları, `re.match` anlambilimi.
