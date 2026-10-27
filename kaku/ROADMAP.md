# Roadmap & nightly log

## Done
- [x] ASS/SSA parse/write (flat entry model, unknown sections preserved, SSA alignment mapping)
- [x] SRT / WebVTT import/export, UTF-8/UTF-16/CP1254 decoding
- [x] Override tag parser, visible-text helpers, drawing detection
- [x] Karaoke: parse/build syllables, auto split, strip
- [x] Timing ops: shift, 2-point linear sync, fps change, frame snap, gap closing, min gap, CPS extension, split/merge
- [x] QC checks, find/replace (regex, tag-aware), undo/redo
- [x] Lua automation: macros, filters, `subs` userdata, dialogs (headless), `karaskel`, `util`, `re`, `unicode`, `include`
- [x] CLI

## Next (priority order)
1. **GUI** (egui): grid, edit box with tag highlighting, style manager, find/replace, QC panel, Lua macro menu with real dialogs.
2. Video/audio: mpv or ffmpeg based preview, waveform/spectrogram, keyframes, timecodes (VFR).
3. Real text metrics (`aegisub.text_extents`) via font loading (system fonts) – currently an approximation.
4. Renderer (libass binding) for preview, `aegisub.*` remaining APIs (`ffi` helpers, `lfs`, clipboard).
5. Styles manager: import/export, resample resolution, translation assistant, spellcheck (hunspell), Turkish typography fixes.
6. Auto4 `.lua` compatibility test-suite using the user's scripts (drop them into `examples/user_scripts/`).

## Nightly log
- 2026-10-27: project bootstrapped (core, lua runtime, CLI, tests).
