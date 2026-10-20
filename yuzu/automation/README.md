# Yuzu automation

## Running Aegisub scripts

Yuzu runs Aegisub's Automation 4 Lua scripts on LuaJIT. Scripts are loaded from the folders listed in
*Preferences → Script folders* (by default also `~/.aegisub/automation/autoload` and
`%APPDATA%\Aegisub\automation\autoload`), libraries from the `include` folder next to each of them.

* **Reloading**: scripts reload by themselves when a script or a library under `include` changes,
  appears or goes away (*Automation manager → Reload when changed*; checked every 1.5 s).
* **Errors** come with a stack traceback (load errors too) in the console and in
  *Automation → Automation manager*, which lists every script with version, author, macros and its error.
* **Validation**: a macro's validation function greys it out in the Automation menu for selections it
  refuses, and a macro run from the palette or a shortcut is not run then (as in Aegisub). An
  `is_active` function shows the item checked.
* **API**: `aegisub.register_macro / register_filter / text_extents / frame_from_ms / ms_from_frame /
  video_size / keyframes / project_properties / decode_path / get_audio_selection / file_name /
  set_undo_point / set_status_text / cancel / gettext / dialog.display / dialog.open / dialog.save /
  progress.* / debug.out / log`; `subs` with `n`, `#`, indexing, `insert`, `append`, `delete`,
  `deleterange`, `subs[0] = line` (append), `subs[-i] = line` (insert before i); line tables with all
  Aegisub fields including `extra` (kept in `[Aegisub Extradata]`, written the way Aegisub writes it).
* **Modules**: `aegisub.util` / `utils`, `aegisub.unicode` / `unicode`, `aegisub.re` / `re`,
  `aegisub.clipboard` / `clipboard`, `karaskel`, `cleantags`, `lfs` (subset), `json` (only when no
  `json.lua` of your own is installed), LuaJIT's `ffi`, `bit` and `jit`.
* **DependencyControl**: scripts written for it load offline through a compatible shim: the module list
  (`DependencyControl{ {"a-mo.LineCollection", {"l0.ASSFoundation", version = "0.4"}, "json"}, ... }`),
  `requireModules` (optional modules may be missing), `register`, `registerMacro(s)` (in a submenu named
  after the script, as DependencyControl does), `getConfigHandler` (JSON in `?user/config/<namespace>.json`),
  `checkVersion`, `getLogger`. Nothing is downloaded or updated: install the modules a script needs into
  `include` yourself.

Not supported yet: MoonScript (`.moon`) scripts, `aegisub.get_frame`, export filter configuration
dialogs.

## Karaoke templater

`autoload/kara-templater.lua` is an Aegisub 3.x compatible karaoke templater (macro
"Apply karaoke template" and export filter "Karaoke template").

```
cargo run -p yuzu-cli -- run automation/autoload/kara-templater.lua 0 in.ass out.ass
automation/tests/run.sh          # builds the CLI and runs all test cases
```

## What it does

* Karaoke lines are lines with Effect `karaoke` (commented or not). Each one is processed with
  `karaskel` and the generated lines (Effect `fx`) are inserted right after it; the original is turned
  into a comment (Effect stays `karaoke`).
* Output of a previous run (non-comment lines with Effect exactly `fx`) is deleted first, so the
  macro can be re-run (verified by the tests). The selection is ignored; the whole file is processed.
* Template / code lines are Comment lines whose Effect field starts with `code`, `template` or `mixed`.

## Line types

| Effect | Meaning |
|---|---|
| `code once` | run once, before the first karaoke line |
| `code line` / `syl` / `char` / `furi` | run per line / syllable / character / furigana syllable |
| `code all` | run in the line, syl, char and furi phases for every style |
| `template line` | one output line per karaoke line: template text + the line text (karaoke tags removed) |
| `template pre-line` | its text is prepended to every line generated for that karaoke line |
| `template syl` | one output line per syllable: template text + syllable text |
| `template char` (or `template syl char`) | one output line per character of each syllable (time split evenly) |
| `template furi` | one output line per furigana syllable |
| `template all` | same as `template syl all` |
| `mixed` | same as `template syl multi` |

Modifiers (after the type, any order): `all` (any style, otherwise the template line's style must equal
the karaoke line's), `style NAME`, `noblank` / `no-blank` (skip blank or zero-length syllables),
`loop N` / `repeat N`, `fx NAME` (only syllables carrying `\-NAME` in their `\k` block; without it a
template applies to every syllable), `fxgroup NAME` (only when `fxgroup.NAME` is truthy),
`multi` (the output of all syllables is concatenated into one line per loop iteration, line times are the
union of the retimed pieces), `notext`, `keeptags` (keep the syllable's own override tags), `layer N`
(otherwise the template line's Layer field), `if EXPR` / `unless EXPR` (EXPR is one token, or
`!expr with spaces!`). A non-empty Actor on a template line restricts it to karaoke lines with the same actor.
Generated lines get the layer of the template line and Effect `fx`.

## Template text

* `!expr!` evaluates a Lua expression (or statement) in the template environment; numbers are written
  with up to 3 decimals, `nil` as nothing.
* `$name` is a variable (usable inside expressions too, e.g. `!$sdur/2!`):
  `sstart send smid sdur skdur` (syllable, ms from line start, kdur in cs), `start end mid dur kdur`
  (syllable in syl contexts, the line in line contexts), `lstart lend lmid ldur lkdur` (line),
  `x y left center right top middle bottom width height` (syllable, or character/furigana in those contexts;
  `x`/`y` follow the style alignment), `lleft lcenter ... lx ly` (line), `sleft ... sbottom`,
  `li` (karaoke line number), `si` (syllable), `ci cn` (char index/count), `syln`, `furin`, `j maxj`,
  `layer actor style effect margin_l margin_r margin_t margin_b`.
* Environment (templates and code): `orgline` (processed karaoke line, with `kara`, `furi`, `styleref`),
  `line` (the output line being built), `syl` (current syllable; the character / furigana syllable in
  those contexts), `basesyl` (the syllable a char/furi belongs to), `char`, `meta`, `styles`, `util`,
  `unicode`, `fxgroup`, `i maxi j maxj`, and everything global (`math`, `string`, `table`, ...).
  Functions: `retime(mode, a, b)`, `relayer(n)`, `maxloop(n)`, `format_time(ms)`, `remember(k, v)`,
  `recall(k, default)`, `remember_if`.
* `retime` modes: `syl presyl postsyl line preline postline start2syl syl2end set abs sylpct`.
* Furigana syntax in the karaoke line: `{\k100}漢#か|ん` (base`#`furi1`|`furi2; the base syllable's time is
  split evenly between the furigana parts; a leading `<` on a part is ignored).

## Known gaps / deviations

* The exact semantics of `pre-line`, `multi`, `mixed`, `fx` matching and `line` text were rebuilt from
  memory of Aegisub 3.x and the manual; edge cases may differ. `mixed`, `all` as a type, and `layer N` are
  Yuzu extensions/interpretations.
* `code word` / word templates are not supported (warning). `template once` does not exist.
* Generated lines with `end <= start` are dropped. Number formatting is `%.3f`-trimmed, not Aegisub's.
* Furigana text per part cannot carry its own `\k` timing; parts split evenly. Furigana output uses the
  karaoke line's style unless the template line's style is `<style>-furigana` and exists.
* `x`, `left`, widths depend on the fonts available on the machine (Yuzu measures text itself).
* Errors in expressions/code abort the whole run (Aegisub also aborts, with a different message).
* The macro ignores the selection (as Aegisub does) and does not return a new selection.
