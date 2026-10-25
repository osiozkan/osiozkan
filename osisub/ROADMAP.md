# OsiSub – roadmap & nightly log

Goal: a from-scratch anime subtitle tool = **Aegisub parity** + **Subtitle Edit's best features** + full support for
the owner's existing **Aegisub Lua automation scripts**, with a very easy UI and good performance.

Stack: Node (ESM) + browser UI, Lua 5.3 via `fengari` running server-side (`src/core/lua.js`), core formats in `src/core/*`
(no Node deps, shared by browser). Run: `npm install && npm start` → http://localhost:8765. Tests: `npm test`.
UI smoke test (needs Playwright + running server): `node scripts/smoke.cjs`.

## Status (v0.1.0)
- [x] ASS/SSA parse+write (roundtrip tested), SRT/VTT import/export, plain-text export, encoding detection (UTF-8/16/cp1254)
- [x] Grid (virtualised), edit pane, undo/redo, insert/delete/duplicate/join/split/comment, find&replace (regex), case change, cleanup
- [x] Timing: shift (s/ms/frames), fps convert, snap to frames, fix overlaps, min/max duration + CPS extend, sort
- [x] QC: CPS, duration, line length/count, unknown style, overlap, braces, double spaces
- [x] Styles manager (basic), script properties
- [x] Video/audio open, waveform (zoom/seek), plain-text subtitle overlay, set start/end from playhead
- [x] Original Aegisub libs vendored (`karaskel-auto4`, `utils-auto4`, `cleantags`) + native modules reimplemented (`aegisub.util/unicode/re/clipboard`, `parse_karaoke_data`); LuaJIT compat shims (loadstring, setfenv, bit, %d truncation, 100.0→100 in tags)
- [x] **Lua automation (Aegisub API v4)**: register_macro/filter, `subs` (index/insert/append/delete/deleterange, dot-syntax like Aegisub),
      `aegisub.dialog.display` (coroutine-based, rendered as HTML modal), progress/log/debug.out, text_extents (estimated),
      `include("utils.lua"|"karaskel.lua"|"unicode.lua")`, `require 'aegisub.re'`, `aegisub.re`, unicode, clipboard

- [x] Subtitle Edit-style fixes (`src/core/fixes.js`): balance line breaks, split long lines, merge short lines, remove HI text, punctuation, dedupe, min gap, keyframe snap (core only; keyframes UI TODO); waveform edge dragging (drag edges, Shift+click = set start, right-click = set end); autosave/recovery

- [x] Automation folder (`automation/autoload`, `automation/include`, `OSISUB_SCRIPTS`) auto-loaded at startup; color/coloralpha dialog controls

## Known gaps / TODO (priority order)
1. **Fonts UI**: text metrics now come from real fonts (opentype.js; `fonts/` dir, `OSISUB_FONTS`, system dirs; index cached in ~/.cache/osisub). Add font upload/manager in the UI, .ttc support, GDI-exact metric parity checks vs Aegisub on Windows.
2. **More real-world script testing**: original Aegisub `karaskel`/`utils`/`kara-templater` pass. Need the owner's own scripts as fixtures (ask owner to drop them into `examples/user-scripts/`), plus `lfs`, `ffi`, `aegisub.io`, `aegisub.dialog.open/save`, `aegisub.get_audio_selection`, `aegisub.frame_from_ms` accuracy with loaded video fps, and dialog classes (color picker, coloralpha, `hotkey`...).
3. ~~ASS renderer preview~~ done (JASSUB/libass-wasm bundled by `npm run build:vendor`, installed fonts served via `/api/fonts`). TODO: dummy video (blank canvas at chosen resolution/fps) so styling can be previewed without a video; visual typesetting tools (drag \pos, \clip, rotation) on top of the video; verified only in Playwright `channel:'chromium'` (old headless shell does not capture OffscreenCanvas).
4. Audio: real waveform+spectrum with draggable start/end markers, keyframe/scene-change snapping, timing hotkeys (Aegisub-style), audio karaoke.
5. Subtitle Edit features: OCR import (Tesseract), spell check, auto-translate/merge short lines, "fix common errors", batch convert, compare, bluray sup import, speech-to-text.
6. Aegisub features: styling assistant, translation assistant, resolution resampler, timing post-processor, automation `lfs`/`ffi`-less scripts, attachments (fonts/graphics), dummy video, video context menu visual typesetting tools.
7. Hotkey customisation, themes, i18n (Turkish UI), project save/restore of editor state, autosave.
8. Packaging as desktop app (Tauri/Electron) once feature-stable.

## Nightly log
- **2026-10-25 (night 1)**: Project bootstrapped from empty repo (no earlier work existed in this repo). Core, UI, Lua runtime, tests. 
- **night 1 (cont.)**: vendored original Aegisub karaskel/utils/cleantags; original `kara-templater.lua` runs end-to-end in tests (retime, \pos, $x/$y); real font metrics; LuaJIT compat; ASS model aligned with Aegisub (only info/style/dialogue visible to Lua, `raw` for other sections).
- **night 1 (cont. 2)**: libass-wasm (JASSUB) live preview with installed fonts, auto-refresh on edit; `scripts/video-check.cjs`, `scripts/devserver.sh`.
- **night 1 (cont. 3)**: autoload folder, include resolution, color dialogs, typical-macro regression test. **Owner action wanted**: drop your own Lua scripts into `automation/autoload/` (or `examples/user-scripts/`) so they become regression fixtures.
