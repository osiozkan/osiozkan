# Üçüncü taraf bileşenler

osisub'a gömülü olarak dağıtılan Lua kaynakları ve lisansları:

| Bileşen | Konum | Lisans |
|---|---|---|
| Aegisub `karaskel-auto4.lua`, `kara-templater.lua` (Niels Martin Hansen, Rodrigo Braz Monteiro) | `crates/osisub-lua/lua/karaskel.lua`, `crates/osisub-lua/lua/autoload/kara-templater.lua` | BSD 3 maddeli (dosya başlığında) |
| MoonScript 0.5.0 derleyicisi (Leaf Corcoran) | `crates/osisub-lua/lua/vendor/moonscript/` | MIT (`vendor/moonscript/LICENSE`) |
| LuLPeg (Pierre-Yves Gérardy; LPeg'in saf Lua sürümü, Roberto Ierusalimschy) | `crates/osisub-lua/lua/vendor/lulpeg.lua` | Romantic WTF Public License / MIT (dosya sonunda) |


## FFmpeg (yalnızca Windows paketi)
Windows sürüm paketi `ffmpeg/` klasöründe BtbN/FFmpeg-Builds'in GPL derlemesini (ffmpeg, ffprobe, ffplay) getirir; ayrı programlar olarak çağrılır. Lisans: GPL-3.0 (paketteki `ffmpeg/LICENSE.txt`), kaynak: https://ffmpeg.org ve https://github.com/BtbN/FFmpeg-Builds. osisub ffmpeg'i önce `OSISUB_FFMPEG`/`OSISUB_FFPROBE`/`OSISUB_FFPLAY`, sonra programın yanındaki `ffmpeg/` klasörü, sonra PATH'te arar.
