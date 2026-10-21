# SubForge roadmap & nightly log

## Vision (from owner)
Aegisub clone + Subtitle Edit's best features + everything an anime sub tool needs; very optimized; trivial UI;
runs the owner's existing Lua scripts; a new version prepared every night; each run works until 06:00.

## Status
- [x] v0.1 (night 1): ASS parser/writer (round-trip), SRT/VTT, tags/karaoke parsing, timing+QA ops, CLI.
- [x] Automation 4 on LuaJIT: macros/filters, subs API, karaskel (stock one runs), `re`, util, unicode, lfs, dialog stub,
      parse_karaoke_data, include dirs; stock kara-templater/cleantags/strip-tags/edgeblur/auto-leadin verified.
- [x] Real font metrics (ttf-parser, system font scan) for `text_extents`; font usage report + collector.
- [x] Encodings: UTF-8/16 BOM, chardetng detection, Windows-1254 (Turkish) etc.; CLI + UI selectors.
- [x] Local web editor: grid, edit panel, undo/redo (delta updates), styles/properties, tools dialog, QA, macros,
      server-side file browser, save in place + .bak, autosave, token-protected API.
- [x] Waveform (browser-decoded audio) with drag-retime; canvas ASS renderer; visual typesetting; reference subtitle.
- [x] SE-style tools: fix common errors, HI removal, Turkish casing, rebalance, merge/split, stats; resample; lead-in/out,
      chain, min gap, sort, import styles, bilingual merge.
- [ ] kara-templater UI (templates editor, run from UI with preview), karaoke syllable timing mode in waveform.
- [ ] Server-side audio extraction via ffmpeg (large files, mkv), spectrogram, keyframes + scene detect, frame-accurate seek.
- [ ] Spell check backend (hunspell dictionaries), translation memory/glossary, auto-translate hook.
- [ ] libass/JASSUB-exact rendering option; fonts embedding ([Fonts] uuencode); OCR import (sup/idx).
- [ ] Persistent undo (copy-on-write entries) for very large files; macro progress/cancel in UI; lpeg.
- [ ] Packaging: release binaries from CI, Tauri/webview shell so it feels like a desktop app, installer.

## Nightly log
- 2026-10-21 (night 1): repo contained only a profile README; started from scratch in `subforge/`. Everything above
  marked [x] was built and tested in this first run (≈ 60 unit/integration tests + headless-browser UI smoke test).
