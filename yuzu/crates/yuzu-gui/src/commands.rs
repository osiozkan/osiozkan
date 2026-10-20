//! Every user-invokable action with its menu group and default keys. Menus, the command
//! palette, the shortcut editor and the key handler all work from this one list, so a
//! rebinding shows up everywhere.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// When a binding may fire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum When {
    /// Even while a text box is being edited (Ctrl+S, F-keys…).
    Always,
    /// Only while no text box has focus (single letters, arrows, Delete…).
    NotTyping,
}

pub struct CommandDef {
    pub id: &'static str,
    /// English label; translated with `i18n::t` for display.
    pub label: &'static str,
    pub group: &'static str,
    /// Default bindings ("Ctrl+O", "F1", "S"…).
    pub keys: &'static [&'static str],
    /// Forced scope; None = derived from the key (see [`default_when`]).
    pub when: Option<When>,
}

const fn c(id: &'static str, label: &'static str, group: &'static str, keys: &'static [&'static str]) -> CommandDef {
    CommandDef { id, label, group, keys, when: None }
}

const fn c_when(id: &'static str, label: &'static str, group: &'static str, keys: &'static [&'static str], when: When) -> CommandDef {
    CommandDef { id, label, group, keys, when: Some(when) }
}

pub const COMMANDS: &[CommandDef] = &[
    // File
    c("new", "New", "File", &["Ctrl+N"]),
    c("open", "Open…", "File", &["Ctrl+O"]),
    c("save", "Save", "File", &["Ctrl+S"]),
    c("save_as", "Save as…", "File", &["Ctrl+Shift+S"]),
    c("export_srt", "Export SRT…", "File", &[]),
    c("export_vtt", "Export WebVTT…", "File", &[]),
    c("load_reference", "Load reference / original subtitle…", "File", &[]),
    c("close_reference", "Close reference", "File", &[]),
    c("properties", "Script properties…", "File", &[]),
    c("quit", "Quit", "File", &["Ctrl+Q"]),
    // Edit
    c("undo", "Undo", "Edit", &["Ctrl+Z"]),
    c("redo", "Redo", "Edit", &["Ctrl+Shift+Z", "Ctrl+Y"]),
    c("insert_after", "Insert line after", "Edit", &[]),
    c("insert_before", "Insert line before", "Edit", &[]),
    c("duplicate", "Duplicate", "Edit", &["Ctrl+D"]),
    c("join", "Join selected", "Edit", &[]),
    c("delete", "Delete", "Edit", &["Delete"]),
    c("copy_lines", "Copy lines", "Edit", &["Ctrl+C"]),
    c("cut_lines", "Cut lines", "Edit", &["Ctrl+X"]),
    c("paste_lines", "Paste lines", "Edit", &["Ctrl+V"]),
    c("paste_over", "Paste over…", "Edit", &["Ctrl+Shift+V"]),
    c("find", "Find & replace…", "Edit", &["Ctrl+F", "Ctrl+H"]),
    c("multi_replace", "Multiple replace…", "Edit", &[]),
    c("select_lines", "Select lines…", "Edit", &[]),
    c_when("select_all", "Select all lines", "Edit", &["Ctrl+A"], When::NotTyping),
    c("prev_line", "Previous line", "Edit", &["Up"]),
    c("next_line", "Next line", "Edit", &["Down"]),
    c_when("commit_next", "Next line (new at the end)", "Edit", &["Ctrl+Enter"], When::Always),
    c("palette", "Command palette…", "Edit", &["Ctrl+Shift+P"]),
    c("prefs", "Preferences…", "Edit", &["Ctrl+Comma"]),
    // Timing
    c("shift", "Shift times…", "Timing", &["Ctrl+I"]),
    c("snap_frames", "Snap to frames", "Timing", &[]),
    c("post_processor", "Timing post-processor…", "Timing", &[]),
    c("snap_keyframes_sel", "Snap selected to keyframes (±300 ms)", "Timing", &[]),
    c("sort_start", "Sort by start", "Timing", &[]),
    c("sort_end", "Sort by end", "Timing", &[]),
    c("sort_style", "Sort by style", "Timing", &[]),
    // Timing keys (Aegisub's audio keys)
    c("play_line", "Play line", "Timing keys", &["S", "R"]),
    c("play_before", "Play 500 ms before the start", "Timing keys", &["Q"]),
    c("play_after", "Play 500 ms after the end", "Timing keys", &["W"]),
    c("play_first", "Play the first 500 ms", "Timing keys", &["E"]),
    c("play_last", "Play the last 500 ms", "Timing keys", &["D"]),
    c("play_to_end", "Play from the playhead to the line end", "Timing keys", &["T"]),
    c("stop", "Stop", "Timing keys", &["H"]),
    c("play_pause_key", "Play / pause", "Timing keys", &["Space"]),
    c("prev_line_key", "Previous line", "Timing keys", &["Z"]),
    c("next_line_key", "Next line", "Timing keys", &["X"]),
    c("commit", "Next line (new at the end)", "Timing keys", &["G", "Enter"]),
    c("start_at_playhead", "Line start = playhead", "Timing keys", &["1"]),
    c("end_at_playhead", "Line end = playhead", "Timing keys", &["2"]),
    c("scroll_left", "Scroll timing view left", "Timing keys", &["A"]),
    c("scroll_right", "Scroll timing view right", "Timing keys", &["F"]),
    // Tools
    c("styles", "Styles manager…", "Tools", &[]),
    c("style_assist", "Styling assistant…", "Tools", &[]),
    c("qc", "Quality check…", "Tools", &[]),
    c("resample", "Resample resolution…", "Tools", &[]),
    c("fix", "Fix common errors…", "Tools", &[]),
    c("spell", "Spell check…", "Tools", &[]),
    c("netflix_qc", "Netflix-style quality check", "Tools", &[]),
    c("fonts", "Fonts collector / embed…", "Tools", &[]),
    c("karaoke", "Karaoke editor…", "Tools", &[]),
    c("clean_tags", "Clean tags (selected)", "Tools", &[]),
    c("strip_tags", "Strip all tags (selected)", "Tools", &[]),
    c("strip_karaoke", "Strip karaoke (selected)", "Tools", &[]),
    c("dedupe", "Remove duplicate lines", "Tools", &[]),
    c("trim", "Trim whitespace (all)", "Tools", &[]),
    // Translation (Osi's tools)
    c("translate_assist", "Translation assistant…", "Translation", &[]),
    c("reference", "Reference panel…", "Translation", &[]),
    c("toggle_ref_column", "Reference column in the grid", "Translation", &[]),
    c("compare", "Compare with the source (aligncompare)…", "Translation", &[]),
    c("sync_ref", "Sync to the reference…", "Translation", &[]),
    c("sync_speech", "Sync to speech…", "Translation", &[]),
    c("glossary", "Term check…", "Translation", &[]),
    // Automation
    c("automation_manager", "Automation manager…", "Automation", &[]),
    c("reload_scripts", "Reload scripts", "Automation", &[]),
    c("add_script_folder", "Add script folder…", "Automation", &[]),
    c("console", "Console", "Automation", &[]),
    // Video
    c("open_video", "Open video…", "Video", &[]),
    c("open_audio", "Open audio…", "Video", &[]),
    c("close_media", "Close media", "Video", &[]),
    c("set_start", "Set line start from video", "Video", &["F1"]),
    c("set_end", "Set line end from video", "Video", &["F2"]),
    c("play_line_f", "Play line", "Video", &["F3"]),
    c("play_end_f", "Play the end of the line", "Video", &["F4"]),
    c("play_pause", "Play / pause", "Video", &["Ctrl+Space"]),
    c("frame_prev", "Previous frame", "Video", &["Ctrl+Left"]),
    c("frame_next", "Next frame", "Video", &["Ctrl+Right"]),
    c("seek_back", "Back 1 second", "Video", &["Ctrl+Shift+Left"]),
    c("seek_fwd", "Forward 1 second", "Video", &["Ctrl+Shift+Right"]),
    c("goto_start", "Go to line start", "Video", &[]),
    c("goto_end", "Go to line end", "Video", &[]),
    c("toggle_spectrum", "Waveform / spectrum", "Video", &[]),
    c("toggle_snap", "Snap to keyframes", "Video", &[]),
    c("toggle_follow", "Follow active line", "Video", &[]),
    // View / Help
    c("toggle_preview", "Preview panel", "View", &[]),
    c("toggle_dark", "Dark theme", "View", &[]),
    c("shortcuts", "Keyboard shortcuts…", "Help", &[]),
    c("about", "About Yuzu", "Help", &[]),
];

pub fn find(id: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Navigation and text keys belong to a focused text box; letters without Ctrl/Alt too.
pub fn default_when(sc: &KeyboardShortcut) -> When {
    let m = sc.modifiers;
    let nav = matches!(sc.logical_key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End
        | Key::PageUp | Key::PageDown | Key::Delete | Key::Backspace | Key::Tab);
    let plain = !(m.ctrl || m.command || m.alt || m.mac_cmd);
    let fkey = matches!(sc.logical_key, Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12);
    if nav || (plain && !fkey) { When::NotTyping } else { When::Always }
}

pub fn when_of(cmd: &CommandDef, sc: &KeyboardShortcut) -> When {
    cmd.when.unwrap_or_else(|| default_when(sc))
}

fn key_text(k: Key) -> &'static str {
    match k {
        Key::ArrowLeft => "Left", Key::ArrowRight => "Right", Key::ArrowUp => "Up", Key::ArrowDown => "Down",
        Key::Comma => "Comma", Key::Period => "Period", Key::Minus => "Minus", Key::Plus => "Plus",
        k => k.name(),
    }
}

/// Portable text form ("Ctrl+Shift+P"), also used in the settings file.
pub fn to_text(sc: &KeyboardShortcut) -> String {
    let m = sc.modifiers;
    let mut s = String::new();
    if m.ctrl || m.command || m.mac_cmd { s.push_str("Ctrl+"); }
    if m.alt { s.push_str("Alt+"); }
    if m.shift { s.push_str("Shift+"); }
    s.push_str(key_text(sc.logical_key));
    s
}

/// Short form for menus ("Ctrl+⏴"). The arrow glyphs are the ones egui's built-in fonts have
/// (← and → are missing there).
pub fn display(sc: &KeyboardShortcut) -> String {
    let s = to_text(sc);
    let (mods, key) = match s.rfind('+') { Some(i) if i + 1 < s.len() => (&s[..=i], &s[i + 1..]), _ => ("", s.as_str()) };
    let key = match key { "Left" => "⏴", "Right" => "⏵", "Up" => "⏶", "Down" => "⏷", "Comma" => ",", "Period" => ".", k => k };
    format!("{}{}", mods, key)
}

pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    // "Ctrl++" (plus key) arrives as ["Ctrl", "", ""]
    let (mods, key) = if s.ends_with("++") { (&parts[..parts.len().saturating_sub(2)], "Plus") } else { (&parts[..parts.len() - 1], parts[parts.len() - 1]) };
    let mut m = Modifiers::NONE;
    for p in mods {
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => m = m | Modifiers::COMMAND,
            "shift" => m = m | Modifiers::SHIFT,
            "alt" | "option" => m = m | Modifiers::ALT,
            _ => return None,
        }
    }
    let key = match key { "←" | "⏴" => Key::ArrowLeft, "→" | "⏵" => Key::ArrowRight, "↑" | "⏶" => Key::ArrowUp, "↓" | "⏷" => Key::ArrowDown, k => Key::from_name(k)? };
    Some(KeyboardShortcut::new(m, key))
}

/// Same key + modifiers (COMMAND and CTRL count as the same "Ctrl").
pub fn same(a: &KeyboardShortcut, b: &KeyboardShortcut) -> bool {
    to_text(a) == to_text(b)
}

/// Current bindings: defaults with the user's overrides applied.
#[derive(Clone, Debug)]
pub struct Keymap {
    pub bindings: Vec<(&'static str, Vec<KeyboardShortcut>)>,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap { bindings: COMMANDS.iter().map(|c| (c.id, c.keys.iter().filter_map(|k| parse(k)).collect())).collect() }
    }
}

impl Keymap {
    /// Apply overrides `id = "Ctrl+A|Ctrl+B"` ("" = no shortcut). Unknown ids are ignored.
    pub fn with_overrides(over: &[(String, String)]) -> Keymap {
        let mut k = Keymap::default();
        for (id, v) in over {
            if let Some(b) = k.bindings.iter_mut().find(|(i, _)| i == id) {
                b.1 = v.split('|').filter_map(parse).collect();
            }
        }
        k
    }

    /// The bindings that differ from the defaults (what the settings file stores).
    pub fn overrides(&self) -> Vec<(String, String)> {
        let def = Keymap::default();
        self.bindings.iter().zip(def.bindings.iter()).filter(|((_, a), (_, b))| {
            a.len() != b.len() || a.iter().zip(b.iter()).any(|(x, y)| !same(x, y))
        }).map(|((id, a), _)| (id.to_string(), a.iter().map(to_text).collect::<Vec<_>>().join("|"))).collect()
    }

    pub fn get(&self, id: &str) -> &[KeyboardShortcut] {
        self.bindings.iter().find(|(i, _)| *i == id).map(|(_, v)| v.as_slice()).unwrap_or(&[])
    }

    pub fn set(&mut self, id: &str, keys: Vec<KeyboardShortcut>) {
        if let Some(b) = self.bindings.iter_mut().find(|(i, _)| *i == id) { b.1 = keys; }
    }

    /// First binding as menu text.
    pub fn label(&self, id: &str) -> Option<String> {
        self.get(id).first().map(display)
    }

    /// Pairs of commands sharing a shortcut: (shortcut text, id a, id b).
    pub fn conflicts(&self) -> Vec<(String, &'static str, &'static str)> {
        let mut out = Vec::new();
        let all: Vec<(&'static str, &KeyboardShortcut)> = self.bindings.iter().flat_map(|(id, v)| v.iter().map(move |s| (*id, s))).collect();
        for (i, (a, sa)) in all.iter().enumerate() {
            for (b, sb) in all.iter().skip(i + 1) {
                if a != b && same(sa, sb) { out.push((display(sa), *a, *b)); }
            }
        }
        out
    }
}

// ---------------------------------------------------------------- fuzzy matching (palette)

/// Lower-case and fold Turkish letters so "kirma" finds "Kırma", "yazi tipi" "Yazı tipi".
pub fn fold(s: &str) -> String {
    s.chars().flat_map(|c| {
        let r = match c {
            'ı' | 'İ' | 'I' | 'î' | 'Î' => 'i', 'ş' | 'Ş' => 's', 'ğ' | 'Ğ' => 'g', 'ü' | 'Ü' | 'û' => 'u', 'ö' | 'Ö' => 'o',
            'ç' | 'Ç' => 'c', 'â' | 'Â' => 'a',
            c => return c.to_lowercase().collect::<Vec<_>>(),
        };
        vec![r]
    }).collect()
}

/// Score of `query` against `text` (both folded); None when it does not match.
/// Substring matches beat word matches beat in-order letter matches; word starts count extra.
pub fn score(query: &str, text: &str) -> Option<f32> {
    if query.is_empty() { return Some(0.0); }
    let len_pen = text.chars().count() as f32 * 0.05;
    if let Some(i) = text.find(query) {
        let at_word = i == 0 || !text[..i].chars().last().map(|c| c.is_alphanumeric()).unwrap_or(false);
        return Some(100.0 + if at_word { 30.0 } else { 15.0 } - text[..i].chars().count() as f32 * 0.5 - len_pen);
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.len() > 1 && words.iter().all(|w| text.contains(w)) { return Some(80.0 - len_pen); }
    let tc: Vec<char> = text.chars().collect();
    let (mut pos, mut total, mut run) = (0usize, 0.0f32, 0f32);
    let mut prev: Option<usize> = None;
    for qc in query.chars().filter(|c| !c.is_whitespace()) {
        let j = (pos..tc.len()).find(|&j| tc[j] == qc)?;
        if prev == Some(j.wrapping_sub(1)) { run += 1.0; total += 2.0 + run; } else { run = 0.0; total += 1.0; }
        if j == 0 || !tc[j - 1].is_alphanumeric() { total += 4.0; }
        prev = Some(j);
        pos = j + 1;
    }
    Some(total - len_pen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_round_trip() {
        for s in ["Ctrl+O", "Ctrl+Shift+P", "F1", "Space", "Ctrl+Left", "Ctrl+Comma", "Alt+Shift+X", "Enter", "1"] {
            let k = parse(s).unwrap_or_else(|| panic!("{}", s));
            assert_eq!(to_text(&k), s);
        }
        assert_eq!(display(&parse("Ctrl+Left").unwrap()), "Ctrl+⏴");
        assert_eq!(display(&parse("Ctrl+Comma").unwrap()), "Ctrl+,");
        assert_eq!(parse("Ctrl+⏵"), parse("Ctrl+Right"));
        assert_eq!(display(&parse("PageUp").unwrap()), "PageUp");
        assert_eq!(display(&parse("Up").unwrap()), "⏶");
        assert!(parse("Hyper+X").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn defaults_have_no_conflicts_and_all_parse() {
        for c in COMMANDS { for k in c.keys { assert!(parse(k).is_some(), "{}: {}", c.id, k); } }
        let k = Keymap::default();
        assert!(k.conflicts().is_empty(), "{:?}", k.conflicts());
        let mut ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate command id");
    }

    #[test]
    fn overrides_round_trip_and_conflicts() {
        let mut k = Keymap::default();
        assert!(k.overrides().is_empty());
        k.set("qc", vec![parse("Ctrl+Shift+Q").unwrap()]);
        k.set("delete", vec![]);
        let o = k.overrides();
        assert_eq!(o.len(), 2);
        let k2 = Keymap::with_overrides(&o);
        assert_eq!(k2.label("qc").as_deref(), Some("Ctrl+Shift+Q"));
        assert!(k2.get("delete").is_empty());
        k.set("styles", vec![parse("Ctrl+S").unwrap()]);
        let c = k.conflicts();
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].1, c[0].2), ("save", "styles"));
    }

    #[test]
    fn scope_rules() {
        assert_eq!(default_when(&parse("S").unwrap()), When::NotTyping);
        assert_eq!(default_when(&parse("Ctrl+Left").unwrap()), When::NotTyping);
        assert_eq!(default_when(&parse("Delete").unwrap()), When::NotTyping);
        assert_eq!(default_when(&parse("Ctrl+S").unwrap()), When::Always);
        assert_eq!(default_when(&parse("F3").unwrap()), When::Always);
        assert_eq!(when_of(find("select_all").unwrap(), &parse("Ctrl+A").unwrap()), When::NotTyping);
    }

    #[test]
    fn fuzzy() {
        assert_eq!(fold("Yazı Tipi İĞŞÇÖÜ"), "yazi tipi igscou");
        assert!(score("kirma", &fold("Satır kırmalarını dengele")).is_some());
        assert!(score("xyz", &fold("Satır kırmalarını dengele")).is_none());
        let a = score("yazi tipi", &fold("Yazı tipi toplayıcı / gömme…")).unwrap();
        let b = score("yazi tipi", &fold("Kısa satırları birleştir")).unwrap_or(-1.0);
        assert!(a > b);
        // word start beats a match in the middle of a word
        assert!(score("ses", &fold("Ses aç…")).unwrap() > score("ses", &fold("Boşlukları kes")).unwrap());
    }
}
