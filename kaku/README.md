# Kaku (書く)

Anime-subtitle toolkit written from scratch in Rust. Goals:

* **Aegisub parity** – ASS/SSA model that mirrors Aegisub's flat `subs[i]` view, karaoke tags, styles, timing.
* **Your Lua scripts keep working** – an Aegisub-compatible automation runtime on LuaJIT
  (`aegisub.register_macro/filter`, `karaskel`, `util`, `re`, `unicode`, `aegisub.dialog.display`, `subs.insert/delete/append`…).
* **Subtitle Edit's strengths** – SRT/VTT conversion, QC (CPS, overlap, length, missing styles), bulk fixes, fps change, linear sync, gap/CPS tools.
* **Fast and scriptable** – single static binary, no GUI needed for batch work.

## Layout

| crate | purpose |
|-------|---------|
| `kaku-core` | ASS/SRT/VTT, override tags, karaoke, ops, QC, search/replace, undo history |
| `kaku-lua`  | Aegisub automation runtime (LuaJIT via `mlua`) |
| `kaku-cli`  | `kaku` command line tool |

## Usage

```
cargo run -p kaku-cli -- info examples/sample.ass
kaku check  file.ass --cps 20
kaku fix    file.ass -o fixed.ass
kaku shift  file.ass -1500
kaku convert file.ass file.srt
kaku macros  my_script.lua
kaku run     my_script.lua file.ass -m "Macro name" -o out.ass --set suffix=! --button Apply
```

See [ROADMAP.md](ROADMAP.md) for status and the nightly log.
