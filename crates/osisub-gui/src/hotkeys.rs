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
}
