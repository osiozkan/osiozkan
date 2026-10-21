# SubForge roadmap & nightly log

## Vision (from owner)
Aegisub clone + Subtitle Edit's best features + everything an anime sub tool needs; very optimized; trivial UI;
runs the owner's existing Lua scripts; a new version prepared every night; each run works until 06:00.

## Status
- [x] v0.1 (night 1): ASS parser/writer (round-trip), SRT/VTT, tags/karaoke parsing, timing+QA ops, CLI,
      Automation 4 Lua layer (macros, filters, subs API, karaskel, re, util, unicode, dialog stub), tests.
- [ ] Local web UI (`subforge serve`): event grid, editing, undo/redo, styles, QA, macros, video preview.
- [ ] Real font metrics for `text_extents` (fontdue/ttf-parser, system font lookup) — needed for accurate karaskel.
- [ ] kara-templater (`karaoke templater` code-line/template/fx executor) — highest value for anime users.
- [ ] Audio waveform/spectrogram + timing by drag, keyframes, video frame snapping (ffmpeg probe).
- [ ] Spell check, translation memory/glossary, SE-style tools: merge/split, auto-balance lines, OCR import (sup/idx).
- [ ] .ass rendering preview (libass FFI or wasm JASSUB in UI).
- [ ] Packaging (Windows/macOS/Linux), Tauri shell.

## Nightly log
- 2026-10-21: repo contained only a profile README; started from scratch in `subforge/`. v0.1 above.
