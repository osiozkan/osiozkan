# SubForge

Anime subtitle authoring toolkit written from scratch in Rust: an Aegisub-compatible ASS core with
**Automation 4 (Lua) compatibility**, plus Subtitle Edit style batch tools and QA.

> Built nightly by a scheduled Claude Code routine (22:00 → 06:00). See `ROADMAP.md` for state and next steps.

## Build / test

```
cargo build --release
cargo test
target/release/subforge --help
```

## CLI

| command | what it does |
|---|---|
| `info FILE` | script info, styles, event statistics |
| `convert IN OUT` | ass/ssa ⇄ srt/vtt/txt (by extension) |
| `shift`, `fps`, `resync`, `snap` | timing tools (offset, framerate change, two-point drift fix, frame snap) |
| `qa FILE` | CPS, duration, overlaps, line length/count, missing styles, whitespace |
| `fix FILE` | sort, fix overlaps, extend short lines |
| `replace FILE FIND WITH [--regex -i]` | find & replace in event text |
| `macros SCRIPT.lua` | list macros/filters registered by a Lua script |
| `run FILE SCRIPT.lua "Macro" [-p name=value] [--select 3,4]` | run an Aegisub Lua macro headless |

## Lua compatibility (Automation 4)

Implemented: `aegisub.register_macro/register_filter`, `subs` (index/`#`/append/insert/delete/deleterange/n),
line classes `info`/`style`/`dialogue`, `aegisub.log/progress/cancel/dialog.display` (headless: defaults or `-p`),
`aegisub.text_extents` (approximate), `aegisub.re` / `re` (fancy-regex), `aegisub.util`, `unicode`, `include("karaskel.lua")`
(`collect_head`, `preproc_line` incl. syllables), `utils.lua` globals, LuaJIT-style `bit`, `require` relative to the script.
Errors/cancel roll the document back, like Aegisub.

Known gaps: real font metrics, video/audio/keyframe API, `lpeg`, `ffi`, `lfs`, GUI dialogs, `kara-templater` (planned).
