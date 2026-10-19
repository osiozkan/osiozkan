//! Flat, Aegisub-style view of a script: [info..., styles..., events...].
use subforge_core::ass::{Event, RawSection, Script, Style};

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Info(String, String),
    Style(Style),
    Event(Event),
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub lines: Vec<Line>,
    rest: Script, // everything else (raw sections, order, flags) carried through
}

impl Doc {
    pub fn from_script(s: &Script) -> Doc {
        let mut lines = Vec::with_capacity(s.info.len() + s.styles.len() + s.events.len());
        lines.extend(s.info.iter().map(|(k, v)| Line::Info(k.clone(), v.clone())));
        lines.extend(s.styles.iter().cloned().map(Line::Style));
        lines.extend(s.events.iter().cloned().map(Line::Event));
        let mut rest = s.clone();
        rest.info.clear();
        rest.styles.clear();
        rest.events.clear();
        Doc { lines, rest }
    }

    pub fn into_script(self) -> Script {
        let mut s = self.rest;
        for l in self.lines {
            match l {
                Line::Info(k, v) => s.info.push((k, v)),
                Line::Style(st) => s.styles.push(st),
                Line::Event(e) => s.events.push(e),
            }
        }
        s
    }

    pub fn to_script(&self) -> Script {
        self.clone().into_script()
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for l in &self.lines {
            match l {
                Line::Info(..) => c.0 += 1,
                Line::Style(_) => c.1 += 1,
                Line::Event(_) => c.2 += 1,
            }
        }
        c
    }

    /// 1-based flat index of the n-th (0-based) event, as Aegisub scripts see it.
    pub fn event_to_flat(&self, ev: usize) -> usize {
        let (i, s, _) = self.counts();
        i + s + ev + 1
    }

    /// 0-based event index for a 1-based flat index, if it's an event.
    pub fn flat_to_event(&self, flat: usize) -> Option<usize> {
        let (i, s, _) = self.counts();
        flat.checked_sub(1)?.checked_sub(i + s)
    }

    pub fn raw_sections(&self) -> &[RawSection] {
        &self.rest.raw_sections
    }

    pub fn play_res(&self) -> (i32, i32) {
        let mut tmp = Script::new();
        tmp.info = self
            .lines
            .iter()
            .filter_map(|l| if let Line::Info(k, v) = l { Some((k.clone(), v.clone())) } else { None })
            .collect();
        tmp.play_res()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let mut s = Script::new();
        s.events.push(Event::new(0, 1000, "Default", "a"));
        let d = Doc::from_script(&s);
        assert_eq!(d.counts().2, 1);
        assert_eq!(d.event_to_flat(0), d.counts().0 + d.counts().1 + 1);
        assert_eq!(d.flat_to_event(d.event_to_flat(0)), Some(0));
        let back = d.into_script();
        assert_eq!(back.events, s.events);
        assert_eq!(back.info, s.info);
    }
}
