# Automation 4 compatibility (subforge-lua)

Runtime: LuaJIT 2.1 via mlua (`ffi`, `bit`, `jit` available). Scripts run unmodified; the goal is to
behave like Aegisub 3.2's Lua 4 host. Reference sources were read from `Aegisub/Aegisub` (`automation/include`,
`libaegisub/lua/modules`).

## Supported

Loading
- `.lua` and `.moon` scripts (see MoonScript below). `script_name/description/author/version` globals are read after load.
- `aegisub.register_macro(name, desc, process, validate, is_active)`, `aegisub.register_filter(name, desc, priority, process, config)`.
  Filters get `(subs, config)`; `config(subs, old_settings)` returns a dialog table (`LuaScript::filter_dialog`).
- Macros receive `(subs, selection, active_line)` and may return `selection, active`.
- `include("x.lua")` searches the script directory, host include dirs and `$SUBFORGE_INCLUDE`, then the bundled
  `utils.lua`, `karaskel.lua`, `cleantags.lua`; each file loads once. `require` finds modules next to the script and in
  the include dirs (`?.lua`, `?/init.lua`, `?.moon`).
- Bundled (Aegisub's own files): `karaskel`, `utils`, `cleantags`; `aegisub.util` (port of util.moon).

Modules (global and via `require`)
- `unicode` / `aegisub.unicode`: `len chars codepoint charwidth to_upper_case to_lower_case to_fold_case`
  (+ short aliases `to_upper/to_lower/to_fold`, Turkish-aware `to_upper_tr/to_lower_tr/title_tr`).
- `re` / `aegisub.re`: port of Aegisub's `re.moon` over a native fancy-regex core. `find match gfind gmatch sub split gsplit compile`
  and the flag constants `re.ICASE NOSUB COLLATE NEWLINE_ALT NO_MOD_M NO_MOD_S MOD_S MOD_X NO_EMPTY_SUBEXPRESSIONS`.
  Boost-perl defaults (`.` matches newline, `^`/`$` per line). Replacement formats: `$1 ${1} $& \1 \n \U \L \E \u \l`.
  Note `re.find` returns whole matches (`{str, first, last}`) and `re.match` the capture groups of the first match, exactly like Aegisub.
- `lfs` / `aegisub.lfs` (superset), `aegisub.ffi`, `aegisub.argcheck` (no-op), `aegisub.clipboard`.
- `lpeg` (pure-Lua LuLPeg) and `moonscript` / `moonscript.*` (loaded lazily; zero cost for plain Lua scripts).
- `l0.DependencyControl`: stand-in for the registration/require surface (`requireModules`, `registerMacro(s)`, `register`,
  `checkVersion`, `getLogger`...). No updater, feeds or persisted config.

`aegisub.*`
- `log`, `debug.out` (level optional; format applied only when varargs are given, `%` is safe), `progress.set/task/title/is_cancelled`
  (`task` is `string.format`ted), `cancel`, `gettext`, `lua_supports_utf8`, `set_undo_point`.
- `dialog.display` (all classes are forwarded to the `Host`; result types: text for edit/textbox/dropdown/color/coloralpha/alpha,
  numbers for intedit/floatedit/slider, booleans for checkbox; cancel returns `false`), `dialog.open`, `dialog.save`.
- `text_extents` (real font metrics via fontdb), `parse_karaoke_data`, `frame_from_ms`, `ms_from_frame` (nil without video/fps),
  `video_size`, `keyframes`, `get_audio_selection`, `project_properties`, `file_name`, `decode_path` (`?script ?video ?audio ?temp ?user ?data`),
  `clipboard.get/set`.

`subs`
- `#subs`, `subs.n`, `subs[i]` (fresh table each read), `subs[i] = line`, `subs[i] = nil`, `subs[#subs+1]`, `subs[0] = line` (append),
  `subs[-n] = line` (insert before n), `ipairs(subs)`, `insert/append/delete/deleterange`, `subs.script_resolution()`.
- Line classes `info` (key/value/raw), `style`, `dialogue` (incl. comments via `comment=true`, `extra` table, `section`).
- Integer fields (`start_time`, `end_time`, `layer`, margins, `align`...) are truncated, like Aegisub's `lua_tointeger`.
  Strings convertible to numbers are accepted.

## Known gaps / differences

- No native Lua C modules: `clipper` (polygon boolean ops), `requireffi`-loaded libraries (`libpotrace`, `lodepng`, `turbojpeg`, ...)
  and Aegisub-internal `aegisub.__*_impl` modules. Scripts needing them (ILL, ASSFoundation image/shape features) fail at `require`.
- Real DependencyControl is not available (the stand-in only covers common calls). `DependencyControl.moon` itself does not parse with MoonScript 0.5.0.
- MoonScript is v0.5.0 (Aegisub ships a newer 0.5.x); newer syntax may fail to parse, and error line numbers refer to the generated Lua.
  Compilation is done in pure Lua (LuLPeg), roughly 0.1 to 0.3 s for a typical 10 KB file in a debug build; results are not cached.
- `margin_v` is emitted for old scripts but input prefers `margin_t` (a script that changes only `margin_v` of a line it read is ignored).
- `frame_from_ms`/`ms_from_frame` use `subforge_core::time::Fps` (floor / ceil); Aegisub's exact-vs-start rounding may differ by 1 ms.
- `ms_from_frame` and friends return nothing when the host has no fps; `Host::fps` is the only video timing hook.
- `style.relative_to` is always 2 (video); `aegisub.project_properties` returns constants for zoom/AR/scroll.
- Dialog `color`/`coloralpha`/`alpha` value normalisation (`#RRGGBB`, `&HAABBGGRR`) is the host's job; `Headless` echoes defaults.
- `string.format` misuse in `aegisub.log` is tolerated (Aegisub would raise).
- `aegisub.unload`, mutexes, `aegisub.get_cursor...` style extensions of forks are not provided.

## Host trait additions

`Host::clipboard_get` / `clipboard_set` (default: process-local buffer). GUI hosts should override them to reach the system clipboard.

## Testing and fixtures

- `cargo test -p subforge-lua`. Tests: `tests/automation.rs` (core behaviour, real kara-templater), `tests/api.rs` (API semantics),
  `tests/moon.rs` (MoonScript and real Aegisub macros), `tests/filters.rs` (filters, DependencyControl stand-in).
- Try any script quickly: `cargo run -p subforge-lua --example probe -- script.lua [macro-index] [sample.ass]`
  lists macros and, with an index, runs it headlessly (dialogs answer with their defaults) and prints the resulting ASS.
- Adding a fixture: put permissively licensed (BSD/MIT/ISC) scripts in `tests/fixtures/<origin>/` with their license header
  unmodified, add a test that loads it, lists macros and runs one on a small `Script`. Fix incompatibilities in the runtime, never in the fixture.
  Do not commit scripts without a clear license (e.g. unanimated's `ua.*.lua`); test those locally with `probe`.

## Verified scripts

| Script | Result |
| --- | --- |
| Aegisub `kara-templater.lua`, `cleantags-autoload.lua`, `macro-1-edgeblur.lua`, `macro-2-mkfullwitdh.lua`, `strip-tags.lua` | pass (fixtures) |
| Aegisub `select-overlaps.moon` | pass (fixture; MoonScript) |
| unanimated `ua.Relocator.lua` (143 KB, unlicensed, run locally only) | loads, 15 macros registered, the non-GUI ones run |
