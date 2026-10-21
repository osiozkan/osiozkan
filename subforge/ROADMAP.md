# SubForge roadmap & nightly log

## Vision (from owner)
Aegisub clone + Subtitle Edit's best features + everything an anime sub tool needs; very optimized; trivial UI;
runs the owner's existing Lua scripts; a new version prepared every night; each run works until 06:00.

## Status
- [x] ASS parser/writer (round-trip, SSA v4 import), SRT/VTT, tags/karaoke parsing, timing+QA ops, CLI.
- [x] Automation 4 on LuaJIT: macros/filters, subs API, `re`, util, unicode, lfs, parse_karaoke_data, include dirs,
      **interactive dialogs** (worker thread + browser form), progress/cancel, print capture.
      Aegisub's stock kara-templater/karaskel/cleantags/strip-tags/edgeblur/auto-leadin are **bundled and verified**.
- [x] Real font metrics (ttf-parser, system font scan) for `text_extents`; font usage report + collector; glyph/missing-font QA.
- [x] Encodings: UTF-8/16 BOM, chardetng detection, Windows-1254 (Turkish) etc.; CLI + UI selectors.
- [x] Local web editor: grid (50k lines, delta updates), edit panel, undo/redo, styles (live preview)/properties, tools,
      QA, macros, find/select/copy/paste, server-side file browser, save in place + .bak, autosave/recover, settings,
      token-protected API, Turkish/English UI, auto-open browser.
- [x] ffmpeg: probe, waveform peaks, keyframes (snap), range streaming, frame stepping, libass exact frame preview.
- [x] Canvas ASS renderer; visual typesetting; reference subtitle + bilingual merge; karaoke tab + bundled templater.
- [x] SE-style tools: fix common errors, HI removal, Turkish casing, rebalance, merge/split, stats; resample; lead-in/out,
      chain, min gap, sort, import styles.
- [ ] Scene-change detection (ffmpeg select filter) as snap targets; spectrogram.
- [ ] Spell check backend (hunspell dictionaries), translation memory/glossary, auto-translate hook.
- [ ] Fonts embedding ([Fonts] uuencode), OCR import (sup/idx), lpeg module, DependencyControl compatibility.
- [ ] Persistent undo (copy-on-write entries) for very large files; keyboard-shortcut customisation.
- [ ] Packaging: release binaries from CI (workflow exists, builds artifacts), webview/Tauri shell, installer.

## Nightly log
- 2026-10-21 (night 1): repo contained only a profile README; started from scratch in `subforge/`. Everything above
  marked [x] was built and tested in this first run (≈ 70 unit/integration tests + a headless-browser UI smoke test
  with ~60 checks). An independent review agent audited the code at the end of the night (findings fixed below).
