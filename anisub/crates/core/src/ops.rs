//! Bulk editing operations (the Subtitle Edit style toolbox).

use crate::ass::{AssFile, Event, Line};
use crate::time::Ms;

pub fn shift(file: &mut AssFile, delta: Ms) {
    for e in file.events_mut() {
        e.start = (e.start + delta).max(0);
        e.end = (e.end + delta).max(0);
    }
}

/// Linear resync: map `a0 -> b0` and `a1 -> b1` for every event time.
pub fn resync(file: &mut AssFile, a0: Ms, b0: Ms, a1: Ms, b1: Ms) {
    if a1 == a0 {
        return;
    }
    let f = |t: Ms| (b0 as f64 + (t - a0) as f64 * (b1 - b0) as f64 / (a1 - a0) as f64).round() as Ms;
    for e in file.events_mut() {
        e.start = f(e.start).max(0);
        e.end = f(e.end).max(0);
    }
}

pub fn sort_by_start(file: &mut AssFile) {
    let (mut evs, mut others): (Vec<Line>, Vec<Line>) =
        file.lines.drain(..).partition(|l| matches!(l, Line::Dialogue(_)));
    evs.sort_by_key(|l| match l {
        Line::Dialogue(e) => (e.start, e.end),
        _ => unreachable!(),
    });
    // events go last among known sections; raw sections are position-independent on write.
    others.extend(evs);
    file.lines = others;
}

/// Merge consecutive events with identical text and touching/overlapping times.
pub fn merge_duplicates(file: &mut AssFile) -> usize {
    let mut removed = 0;
    let mut out: Vec<Line> = Vec::with_capacity(file.lines.len());
    for l in file.lines.drain(..) {
        if let Line::Dialogue(e) = &l {
            if let Some(Line::Dialogue(p)) = out.iter_mut().rev().find(|x| matches!(x, Line::Dialogue(_))) {
                if !e.comment && !p.comment && p.text == e.text && p.style == e.style && e.start <= p.end + 1 {
                    p.end = p.end.max(e.end);
                    removed += 1;
                    continue;
                }
            }
        }
        out.push(l);
    }
    file.lines = out;
    removed
}

/// Make events end exactly where the next one starts when the gap is below `max_gap`.
pub fn close_gaps(file: &mut AssFile, max_gap: Ms) {
    let mut idx: Vec<usize> = file
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| matches!(l, Line::Dialogue(e) if !e.comment).then_some(i))
        .collect();
    idx.sort_by_key(|&i| match &file.lines[i] {
        Line::Dialogue(e) => e.start,
        _ => 0,
    });
    for w in idx.windows(2) {
        let next_start = match &file.lines[w[1]] {
            Line::Dialogue(e) => e.start,
            _ => continue,
        };
        if let Line::Dialogue(e) = &mut file.lines[w[0]] {
            let gap = next_start - e.end;
            if gap > 0 && gap <= max_gap {
                e.end = next_start;
            }
        }
    }
}

/// Plain substring find/replace on event text. Returns number of changed events.
pub fn replace_text(file: &mut AssFile, from: &str, to: &str) -> usize {
    let mut n = 0;
    for e in file.events_mut() {
        if e.text.contains(from) {
            e.text = e.text.replace(from, to);
            n += 1;
        }
    }
    n
}

pub fn new_event(start: Ms, end: Ms, text: &str) -> Event {
    Event { start, end, text: text.into(), ..Event::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn f() -> AssFile {
        let mut f = AssFile::new_default();
        for (s, e, t) in [(2000, 3000, "b"), (0, 1000, "a"), (3000, 3500, "b")] {
            f.lines.push(Line::Dialogue(new_event(s, e, t)));
        }
        f
    }
    #[test]
    fn ops() {
        let mut a = f();
        sort_by_start(&mut a);
        assert_eq!(a.events().next().unwrap().text, "a");
        assert_eq!(merge_duplicates(&mut a), 1);
        assert_eq!(a.events().count(), 2);
        close_gaps(&mut a, 1500);
        assert_eq!(a.events().next().unwrap().end, 2000);
        shift(&mut a, -500);
        assert_eq!(a.events().next().unwrap().start, 0);
        resync(&mut a, 0, 0, 1000, 2000);
        assert_eq!(a.events().nth(1).unwrap().start, 3000);
    }
}
