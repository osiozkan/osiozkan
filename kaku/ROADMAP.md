# Roadmap & nightly log

## Done
- [x] ASS/SSA parse/write (flat entry model, unknown sections preserved, SSA alignment mapping)
- [x] SRT / WebVTT import/export, UTF-8/UTF-16/CP1254 decoding
- [x] Override tag parser, visible-text helpers, drawing detection
- [x] Karaoke: parse/build syllables, auto split, strip
- [x] Timing ops: shift, 2-point linear sync, fps change, frame snap, gap closing, min gap, CPS extension, split/merge
- [x] QC checks, find/replace (regex, tag-aware), undo/redo
- [x] Lua automation: macros, filters, `subs` userdata, dialogs (headless), `karaskel`, `util`, `re`, `unicode`, `include`
- [x] CLI (`kaku info|convert|check|shift|fix|fps|template|macros|run`)
- [x] GUI (egui): grid, tag-highlighting editor, QC panel, style editor, find/replace, shift, karaoke auto-split, Lua macro menu with real dialogs
- [x] Karaoke templater (Aegisub-compatible: code once/line/syl/char, template pre-line/line/syl/char, retime, $vars, !code!, loops, fx tags)
- [x] Real font metrics for `aegisub.text_extents` (fontdb + ttf-parser)
- [x] Media via ffmpeg/ffprobe (`kaku-media`): probe, waveform envelope, keyframes + snapping, libass-rendered frame preview, ffplay range playback; GUI video preview + draggable waveform timing
- [x] CI workflow (tests + release builds for Win/Linux/macOS on `kaku-v*` tags)

## Next (priority order)
1. GUI polish: Turkish/English UI, shortcuts for timing, multi-window styles manager, drag-reorder, theme.
2. Media polish: spectrogram, VFR timecodes, scene-change detection, real-time video playback (mpv embedding), audio scrubbing/karaoke k-timing on waveform.
3. Templater gaps: `mixin`, furigana (`furi`), `fxgroup`, `$`-var coverage vs. Aegisub docs; verify against real Aegisub output.
4. Renderer (libass binding) for preview, `aegisub.*` remaining APIs (`ffi` helpers, `lfs`, clipboard).
5. Styles manager: import/export, resample resolution, translation assistant, spellcheck (hunspell), Turkish typography fixes.
6. Auto4 `.lua` compatibility test-suite using the user's scripts (drop them into `examples/user_scripts/`).

## Next night — start here
- Extended "fix common errors" (space after punctuation, ellipsis, dialogue dashes, sentence caps, min/max duration, merge same text) with GUI checkbox window.
- Templater: `mixin`, `furi`, `fxgroup`; compare against real Aegisub output once user scripts are available (`examples/user_scripts/`).
- Spectrogram, VFR timecodes, scene-change detection, real playback.
- Translation workflow (two-column view), spellcheck.

## Nightly log
- 2026-10-27: project bootstrapped (core, lua runtime, CLI, tests).
- 2026-10-27 (night 1, later): GUI (egui, TR/EN), media pane (video preview via libass, waveform retiming, karaoke syllable dragging, click-to-\pos), templater, resample, TPP, fonts check/collect/embed/extract, styles manager, LRC/SUB/SBV, autosave/recent/drag-drop, F5 script reload.
