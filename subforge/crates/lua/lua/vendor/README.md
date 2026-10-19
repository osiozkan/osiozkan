These Lua libraries are taken unmodified from the Aegisub project
(https://github.com/Aegisub/Aegisub, `automation/include`) and are distributed
under the 3-clause BSD license reproduced in each file header. They are bundled
so that user scripts calling `include("karaskel.lua")` / `include("utils.lua")`
get exactly Aegisub's behaviour. `aegisub_util.lua` is a Lua port of
`aegisub/util.moon` (ISC license, same notice).

`moonscript/` is the unmodified compiled Lua of MoonScript v0.5.0 (MIT, see
MOONSCRIPT-LICENSE) and `lulpeg.lua` is LuLPeg (pure-Lua LPeg, see LULPEG-LICENSE).
Together they let SubForge load `.moon` scripts without a native lpeg. They are
registered lazily as `package.preload` entries (`src/moon_modules.rs`).
`../aegisub_re.lua` is a port of Aegisub's `aegisub/re.moon` (ISC).
