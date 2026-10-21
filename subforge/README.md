# SubForge

Anime subtitle authoring toolkit written from scratch in Rust: an Aegisub-compatible ASS core with
**Automation 4 (Lua/LuaJIT) compatibility**, Subtitle Edit style batch tools, QA, and a local web editor.

> Built nightly by a scheduled Claude Code routine (22:00 → 06:00). `ROADMAP.md` has the state and next steps.

## Build / run

```
cargo build --release
target/release/subforge serve [file.ass]      # editor UI at http://127.0.0.1:8765/
target/release/subforge --help
cargo test                                      # unit + integration (Lua) tests
PW_PATH=/path/to/node_modules/playwright tests/run_ui_smoke.sh   # headless-browser UI test
scripts/fetch-aegisub-scripts.sh DIR && AEGISUB_SCRIPTS=DIR cargo test   # run Aegisub's stock scripts unmodified
```

## Editor (`subforge serve`)

Event grid (virtualized, 50k+ lines), inline edit panel, undo/redo, styles & properties dialogs, server-side file
browser (open / save in place with `.bak` / save as), encoding auto-detect + Windows-1254 etc., waveform with
drag-to-retime, video preview with a canvas ASS renderer (pos/move/an/frz/fad/\t/karaoke/drawings), visual
typesetting (drag = `\pos`, Shift-drag = `\clip`), reference subtitle for translation, bilingual merge, QA panel,
Lua macro runner (scripts from disk keep their real path, so `include`/`require` just work), crash-recovery autosave
(`~/.cache/subforge/autosave`). The API is protected by a per-run token + loopback Host/Origin checks.

## CLI

`info convert shift fps resync snap qa fix replace resample fonts clean stats macros run serve` — see `--help`.
Global `--in-encoding` / `--out-encoding` (e.g. `windows-1254` for legacy Turkish files).

* `clean --fix --hi --case sentence --turkish --rebalance 42 --merge-gap 250 --split 84`
* `fonts FILE --collect DIR` copies the installed font files a script needs (reports MISSING ones)
* `resample FILE 1920 1080` rescales styles, margins, `\pos \move \clip \fs \bord ...` and `\p` drawings
* `run FILE SCRIPT.lua "Macro" [-p name=value] [--select 3,4] [-I include-dir]` runs an Aegisub macro headless

## Lua compatibility (Automation 4)

LuaJIT runtime (ffi, bit, jit). `aegisub.register_macro/register_filter`, `subs` (index, `#`, `ipairs`, append/insert/
delete/deleterange/n/script_resolution), line classes info/style/dialogue, `aegisub.log/progress/cancel/dialog.display`
(headless: defaults or `-p`; UI: auto-confirm), `aegisub.text_extents` (real font metrics: winAscent+winDescent, GDI
sizing), `aegisub.parse_karaoke_data`, `aegisub.re`/`re` (fancy-regex), `aegisub.util`, `unicode`, `lfs`, `clipboard`
stub, `include()` search path (script dir, `include/`, `-I`). Errors/cancel roll the document back like Aegisub.
**Verified unmodified:** Aegisub's stock `kara-templater.lua` + `karaskel*.lua`, `cleantags`, `strip-tags`,
`macro-1-edgeblur`, `karaoke-auto-leadin` (see `tests/aegisub_stock.rs`).

Gaps: `lpeg`, video/audio/keyframe API, GUI dialogs for scripts, DependencyControl, libass-exact rendering.
