//! Plain text: one event per non-empty line.
use super::{new_script, plain_lines, sorted_events};
use crate::ass::{Event, Script};
use crate::time::Ms;

/// Each non-empty line becomes an event of `duration` ms, placed back to back starting at 0.
pub fn parse_txt(text: &str, duration: Ms) -> Script {
    let mut script = new_script();
    let duration = duration.max(1);
    let mut t = 0;
    for l in text.trim_start_matches('\u{feff}').lines() {
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        script.events.push(Event::new(t, t + duration, "Default", &l.replace('{', "(").replace('}', ")")));
        t += duration;
    }
    script
}

/// One line per event; multi-line events are joined with a space.
pub fn write_txt(script: &Script) -> String {
    let mut o = String::new();
    for e in sorted_events(script) {
        let l = plain_lines(e).join(" ");
        if !l.is_empty() {
            o.push_str(&l);
            o.push('\n');
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_roundtrip() {
        let s = parse_txt("\u{feff}İlk satır\n\n  ışık {x} ğüş  \r\nSon\n", 2500);
        assert_eq!(s.events.len(), 3);
        assert_eq!((s.events[1].start, s.events[1].end), (2500, 5000));
        assert_eq!(s.events[1].text, "ışık (x) ğüş");
        let out = write_txt(&s);
        assert_eq!(out, "İlk satır\nışık (x) ğüş\nSon\n");
        let mut s2 = Script::new();
        s2.events.push(Event::new(0, 1, "Default", "{\\i1}a{\\i0}\\Nb"));
        assert_eq!(write_txt(&s2), "a b\n");
    }
}
