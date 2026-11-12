//! Kullanıcı tanımlı kısayollar: "Ctrl+Shift+K" gibi metinler ↔ egui tuş birleşimleri.
//! Şimdilik Lua makrolarına kısayol atamak için kullanılır (Aegisub'daki gibi).
use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// Yalnızca değiştirici tuş mu (Ctrl/Shift/Alt/Super)? Bunlar tek başına kısayol olamaz.
pub fn is_modifier(k: Key) -> bool {
    matches!(k.name(), "ShiftLeft" | "ShiftRight" | "ControlLeft" | "ControlRight" | "AltLeft" | "AltRight" | "SuperLeft" | "SuperRight")
}

/// "Ctrl+Alt+Shift+K" biçimini çözer. Büyük/küçük harf duyarsız; "Cmd" = Ctrl.
pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => m = m | Modifiers::COMMAND,
            "alt" | "option" => m = m | Modifiers::ALT,
            "shift" => m = m | Modifiers::SHIFT,
            _ => {
                if key.is_some() {
                    return None;
                }
                let k = key_from_name(part)?;
                if is_modifier(k) {
                    return None;
                }
                key = Some(k);
            }
        }
    }
    Some(KeyboardShortcut::new(m, key?))
}

fn key_from_name(n: &str) -> Option<Key> {
    // egui adları: "A", "F5", "Enter", "Space", "ArrowUp" …; tek harfleri büyük harfe çevir
    let up = if n.chars().count() == 1 { n.to_uppercase() } else { n.to_string() };
    Key::from_name(&up).or_else(|| Key::ALL.iter().copied().find(|k| k.name().eq_ignore_ascii_case(n)))
}

pub fn format(sc: &KeyboardShortcut) -> String {
    let mut parts: Vec<&str> = vec![];
    if sc.modifiers.command || sc.modifiers.ctrl {
        parts.push("Ctrl");
    }
    if sc.modifiers.alt {
        parts.push("Alt");
    }
    if sc.modifiers.shift {
        parts.push("Shift");
    }
    let k = sc.logical_key.name();
    parts.push(k);
    parts.join("+")
}

/// Uygulamanın sabit kısayollarıyla çakışıyor mu? (Uyarı göstermek için.)
pub fn conflicts_builtin(sc: &KeyboardShortcut) -> bool {
    let m = sc.modifiers;
    let ctrl = m.command || m.ctrl;
    match (ctrl, m.alt, m.shift, sc.logical_key) {
        (true, false, false, Key::O | Key::S | Key::Z | Key::Y | Key::D | Key::Enter | Key::M | Key::F | Key::H | Key::G | Key::C | Key::X | Key::V | Key::A | Key::Equals | Key::Plus | Key::Minus) => true,
        (true, false, true, Key::S | Key::Z) => true,
        (false, false, false, Key::F5 | Key::F6 | Key::F7 | Key::Comma | Key::Period | Key::Space | Key::Delete | Key::Escape) => true,
        (false, true, false, Key::ArrowUp | Key::ArrowDown) => true,
        (false, false, true, Key::F5) => true,
        _ => false,
    }
}

/// Aegisub komut adı → osisub komut kimliği (COMMANDS içindeki).
pub const AEGISUB_COMMANDS: &[(&str, &str)] = &[
    ("video/frame/next", "frame_next"),
    ("video/frame/prev", "frame_prev"),
    ("grid/line/next", "line_next"),
    ("grid/line/prev", "line_prev"),
    ("time/snap/start_video", "set_start"),
    ("time/snap/end_video", "set_end"),
    ("video/play/line", "play_line"),
    ("audio/play/selection", "play_sel"),
    ("audio/play/selection/before", "play_before"),
    ("audio/play/selection/after", "play_after"),
    ("audio/play/selection/begin", "play_begin"),
    ("audio/play/selection/end", "play_end"),
    ("audio/stop", "play_stop"),
    ("time/next", "commit_next"),
    ("time/prev", "line_prev"),
    ("video/play", "play_head"),
    ("edit/line/split/estimate", "split_cursor"),
    ("edit/line/split/video", "split_cursor_frame"),
    ("edit/line/duplicate/shift", "dup_shift"),
    ("tool/line/select", "select_lines"),
    ("video/tool/cross", "tool_none"),
    ("video/tool/drag", "tool_pos"),
    ("video/tool/clip", "tool_clip"),
    ("video/tool/vector_clip", "tool_vclip"),
    ("video/tool/rotate/z", "tool_rotate"),
    ("video/tool/rotate/xy", "tool_rotatexy"),
    ("video/tool/perspective", "tool_persp"),
    ("edit/line/paste/over", "paste_over"),
    ("video/frame/next/keyframe", "kf_next"),
    ("video/frame/prev/keyframe", "kf_prev"),
    ("tool/assistant/styling", "style_assist"),
    ("tool/kanji_timer", "kanji_timer"),
    ("video/tool/scale", "tool_scale"),
];

/// Aegisub'ın ses kipindeki varsayılan tuşları (osisub'da metin yazılmıyorken çalışır).
/// İlk açılışta, kullanıcının o komuta ya da tuşa ataması yoksa eklenir.
pub const DEFAULT_COMMAND_KEYS: &[(&str, &str)] = &[
    ("play_sel", "S"),
    ("play_before", "Q"),
    ("play_after", "W"),
    ("play_begin", "E"),
    ("play_end", "D"),
    ("play_stop", "H"),
    ("commit_next", "X"),
    ("line_prev", "Z"),
];

/// `DEFAULT_COMMAND_KEYS`'i ekler (komutun ya da tuşun ataması varsa atlar). Eklenen sayısı.
pub fn apply_defaults(binds: &mut Vec<crate::settings::MacroHotkey>, command_script: &str) -> usize {
    let mut n = 0;
    for (cmd, keys) in DEFAULT_COMMAND_KEYS {
        let taken_cmd = binds.iter().any(|h| h.script == command_script && h.name == *cmd);
        let taken_key = binds.iter().any(|h| parse(&h.keys) == parse(keys));
        if !taken_cmd && !taken_key {
            binds.push(crate::settings::MacroHotkey { keys: keys.to_string(), script: command_script.to_string(), name: cmd.to_string() });
            n += 1;
        }
    }
    n
}

/// Aegisub tuş adını ("Ctrl-Shift-K", "Return", "Page_Up", "KP_Enter") osisub biçimine çevirir.
pub fn from_aegisub_combo(modifiers: &[String], key: &str) -> Option<String> {
    let k = match key {
        "Return" | "KP_Enter" | "Enter" => "Enter",
        "Left" => "ArrowLeft",
        "Right" => "ArrowRight",
        "Up" => "ArrowUp",
        "Down" => "ArrowDown",
        "Page_Up" | "PageUp" | "Prior" => "PageUp",
        "Page_Down" | "PageDown" | "Next" => "PageDown",
        "Back" | "BackSpace" => "Backspace",
        "Delete" | "KP_Delete" => "Delete",
        "Escape" => "Escape",
        "Tab" => "Tab",
        "Space" | "space" => "Space",
        other => other,
    };
    let mut parts: Vec<&str> = vec![];
    for m in modifiers {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" => parts.push("Ctrl"),
            "alt" | "meta" => parts.push("Alt"),
            "shift" => parts.push("Shift"),
            _ => return None,
        }
    }
    parts.push(k);
    let s = parts.join("+");
    parse(&s).map(|sc| format(&sc))
}

/// Aegisub `hotkey.json` (3.2 "Ctrl-Shift-K" dizileri ya da 3.0 {modifiers, key} nesneleri) içinden
/// osisub'ın bildiği komutların kısayollarını çıkarır. Dönüş: (komut kimliği, tuş birleşimi) çiftleri,
/// atlanan (bilinmeyen / sabit kısayolla çakışan) kayıtlar. "automation/lua/<betik>/<makro>" kayıtları
/// makro adı (betikten bağımsız) olarak `macro:<ad>` biçiminde döner.
pub fn import_aegisub(json: &str) -> Result<(Vec<(String, String)>, Vec<String>), String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("hotkey.json okunamadı: {e}"))?;
    let obj = v.as_object().ok_or("hotkey.json: nesne bekleniyordu")?;
    // Aegisub bağlamları: Default önce, sonra diğerleri (osisub tek bağlamlı)
    let mut ctxs: Vec<(&String, &serde_json::Value)> = obj.iter().collect();
    ctxs.sort_by_key(|(n, _)| match n.as_str() { "Default" => 0, "Video" => 1, "Subtitle Grid" => 2, "Audio" => 3, _ => 4 });
    let mut out: Vec<(String, String)> = vec![];
    let mut skipped = vec![];
    for (ctx, cmds) in ctxs {
        let Some(cmds) = cmds.as_object() else { continue };
        for (cmd, keys) in cmds {
            let target = if let Some(rest) = cmd.strip_prefix("automation/lua/") {
                match rest.rsplit_once('/') { Some((_, m)) if !m.is_empty() => format!("macro:{m}"), _ => { skipped.push(cmd.clone()); continue } }
            } else if let Some((_, id)) = AEGISUB_COMMANDS.iter().find(|(a, _)| a == cmd) {
                id.to_string()
            } else {
                continue; // osisub'da karşılığı olmayan (ya da sabit kısayollu) komut
            };
            for k in keys.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                let combo = match k {
                    serde_json::Value::String(s) => {
                        let mut parts: Vec<String> = s.split('-').map(|x| x.to_string()).collect();
                        // "Ctrl--" gibi eksi tuşu
                        if s.ends_with("--") { parts.retain(|x| !x.is_empty()); parts.push("Minus".into()); }
                        let key = parts.pop().unwrap_or_default();
                        from_aegisub_combo(&parts, &key)
                    }
                    serde_json::Value::Object(o) => {
                        let mods: Vec<String> = o.get("modifiers").and_then(|m| m.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                        o.get("key").and_then(|x| x.as_str()).and_then(|key| from_aegisub_combo(&mods, key))
                    }
                    _ => None,
                };
                let Some(combo) = combo else { skipped.push(format!("{ctx}: {cmd} = {k}")); continue };
                if parse(&combo).map_or(true, |sc| conflicts_builtin(&sc)) {
                    skipped.push(format!("{cmd} = {combo} (sabit kısayol)"));
                    continue;
                }
                if out.iter().any(|(_, c)| *c == combo) { continue; } // aynı tuş başka bağlamda zaten atandı
                out.push((target.clone(), combo));
            }
        }
    }
    Ok((out, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format() {
        let sc = parse("ctrl+shift+k").unwrap();
        assert_eq!(sc.logical_key, Key::K);
        assert!(sc.modifiers.command && sc.modifiers.shift && !sc.modifiers.alt);
        assert_eq!(format(&sc), "Ctrl+Shift+K");
        assert_eq!(format(&parse("Alt+F9").unwrap()), "Alt+F9");
        assert_eq!(parse("F4").unwrap().logical_key, Key::F4);
        assert!(parse("Ctrl+").is_none());
        assert!(parse("Ctrl+K+L").is_none());
        assert!(parse("Ctrl+Bilinmeyen").is_none());
        assert!(parse("ControlLeft").is_none());
        assert!(conflicts_builtin(&parse("Ctrl+S").unwrap()));
        assert!(!conflicts_builtin(&parse("Ctrl+Shift+K").unwrap()));
    }

    #[test]
    fn imports_aegisub_hotkeys() {
        let j = r#"{
          "Default": { "video/frame/next": ["Right"], "time/snap/start_video": ["Ctrl-3"], "app/exit": ["Ctrl-Q"],
                       "automation/lua/kara.lua/Do it": ["Ctrl-Shift-K"], "video/play/line": ["F5"] },
          "Video": { "video/frame/prev": [{"modifiers": [], "key": "Left"}], "video/tool/drag": [{"modifiers": ["Alt"], "key": "Page_Up"}],
                     "grid/line/next": ["Right"] }
        }"#;
        let (got, skipped) = import_aegisub(j).unwrap();
        assert!(got.contains(&("frame_next".into(), "Right".into())), "{got:?}");
        assert!(got.contains(&("set_start".into(), "Ctrl+3".into())));
        assert!(got.contains(&("frame_prev".into(), "Left".into())));
        assert!(got.contains(&("tool_pos".into(), "Alt+PageUp".into())));
        assert!(got.contains(&("macro:Do it".into(), "Ctrl+Shift+K".into())));
        // F5 sabit kısayol, app/exit bilinmiyor; aynı tuş (Right) iki kez atanmaz
        assert!(!got.iter().any(|(c, _)| c == "play_line"));
        assert!(skipped.iter().any(|s| s.contains("F5")));
        assert_eq!(got.iter().filter(|(_, k)| k == "Right").count(), 1);
        assert!(import_aegisub("[").is_err());
    }

    #[test]
    fn defaults_respect_user_bindings() {
        use crate::settings::MacroHotkey;
        let mut b = vec![
            MacroHotkey { keys: "S".into(), script: "".into(), name: "Benim makrom".into() },
            MacroHotkey { keys: "Ctrl+Q".into(), script: "@komut".into(), name: "play_before".into() },
        ];
        let n = apply_defaults(&mut b, "@komut");
        assert_eq!(n, DEFAULT_COMMAND_KEYS.len() - 2);
        assert!(!b.iter().any(|h| h.name == "play_sel"));
        assert!(!b.iter().any(|h| h.keys == "Q"));
        assert!(b.iter().any(|h| h.name == "commit_next" && h.keys == "X"));
        assert_eq!(apply_defaults(&mut b, "@komut"), 0);
    }
}
