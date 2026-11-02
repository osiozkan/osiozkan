//! Bulk editing operations on a [`Script`].

use crate::ass::{Entry, Event, Script};

/// Shift every (or selected) dialogue event by `delta` ms, clamping at 0.
pub fn shift(sc: &mut Script, delta: i64, only: Option<&[usize]>) {
    for (i, e) in sc.entries.iter_mut().enumerate() {
        if let Entry::Dialogue(d) = e {
            if only.map_or(true, |s| s.contains(&i)) {
                d.start = (d.start + delta).max(0);
                d.end = (d.end + delta).max(0);
            }
        }
    }
}

/// Stable sort of the dialogue entries by start time (non-dialogue entries stay in place).
pub fn sort_by_start(sc: &mut Script) {
    let idx: Vec<usize> = sc.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
    let mut evs: Vec<Event> = idx.iter().map(|&i| if let Entry::Dialogue(d) = &sc.entries[i] { d.clone() } else { unreachable!() }).collect();
    evs.sort_by_key(|e| e.start);
    for (i, e) in idx.into_iter().zip(evs) {
        sc.entries[i] = Entry::Dialogue(e);
    }
}

/// Trim the end of each flowing line so it does not run into the next one of the same style/layer.
/// Returns how many lines were changed.
pub fn fix_overlaps(sc: &mut Script) -> usize {
    let mut order: Vec<usize> = sc.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(d) if !d.comment)).map(|(i, _)| i).collect();
    let key = |sc: &Script, i: usize| match &sc.entries[i] { Entry::Dialogue(d) => (d.style.clone(), d.layer, d.start), _ => unreachable!() };
    order.sort_by_key(|&i| key(sc, i));
    let mut changed = 0;
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (ka, kb) = (key(sc, a), key(sc, b));
        if ka.0 == kb.0 && ka.1 == kb.1 {
            if let Entry::Dialogue(d) = &mut sc.entries[a] {
                if d.end > kb.2 && kb.2 > d.start {
                    d.end = kb.2;
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Split event at absolute time `at`; the text is duplicated. Returns false if `at` is outside the event.
pub fn split_at(sc: &mut Script, entry: usize, at: i64) -> bool {
    let Some(Entry::Dialogue(d)) = sc.entries.get(entry).cloned() else { return false };
    if at <= d.start || at >= d.end {
        return false;
    }
    let mut a = d.clone();
    let mut b = d;
    a.end = at;
    b.start = at;
    sc.entries[entry] = Entry::Dialogue(a);
    sc.entries.insert(entry + 1, Entry::Dialogue(b));
    true
}

/// Merge consecutive events with identical style/text that touch or overlap. Returns merge count.
pub fn merge_duplicates(sc: &mut Script) -> usize {
    let mut merged = 0;
    let mut i = 0;
    while i < sc.entries.len() {
        let next = i + 1;
        let can = match (&sc.entries[i], sc.entries.get(next)) {
            (Entry::Dialogue(a), Some(Entry::Dialogue(b))) => !a.comment && !b.comment && a.text == b.text && a.style == b.style && a.layer == b.layer && b.start <= a.end,
            _ => false,
        };
        if can {
            let end = if let Entry::Dialogue(b) = &sc.entries[next] { b.end } else { 0 };
            if let Entry::Dialogue(a) = &mut sc.entries[i] {
                a.end = a.end.max(end);
            }
            sc.entries.remove(next);
            merged += 1;
        } else {
            i += 1;
        }
    }
    merged
}

/// Snap start/end of every dialogue event to frame boundaries for the given fps.
pub fn snap_to_frames(sc: &mut Script, fps: f64) {
    use crate::time::{frame_to_ms, ms_to_frame};
    let snap = |ms: i64| frame_to_ms((ms as f64 * fps / 1000.0).round() as i64, fps);
    let _ = ms_to_frame;
    for d in sc.events_mut() {
        d.start = snap(d.start);
        d.end = snap(d.end);
    }
}

/// Convert dialogue tag-less text between ASS and plain: replaces `\N` with a space.
pub fn unbreak(text: &str) -> String {
    text.replace("\\N", " ").replace("\\n", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sc() -> Script {
        Script::parse("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:03.00,0:00:05.00,Default,,0,0,0,,b\nDialogue: 0,0:00:01.00,0:00:04.00,Default,,0,0,0,,a\n").unwrap()
    }
    #[test]
    fn sort_and_overlap() {
        let mut s = sc();
        sort_by_start(&mut s);
        assert_eq!(s.events().next().unwrap().text, "a");
        assert_eq!(fix_overlaps(&mut s), 1);
        assert_eq!(s.events().next().unwrap().end, 3000);
    }
    #[test]
    fn shift_clamps() {
        let mut s = sc();
        shift(&mut s, -2000, None);
        assert_eq!(s.events().next().unwrap().start, 1000);
        assert_eq!(s.events().nth(1).unwrap().start, 0);
    }
    #[test]
    fn split_merge() {
        let mut s = sc();
        assert!(split_at(&mut s, 0, 4000));
        assert_eq!(s.events().count(), 3);
        assert_eq!(merge_duplicates(&mut s), 1);
        assert_eq!(s.events().count(), 2);
    }
}
