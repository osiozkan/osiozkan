//! Timing operations on selections of events.
use crate::ass::{AssFile, Event};
use crate::time::FrameRate;

pub fn shift(f: &mut AssFile, idx: &[usize], delta_ms: i64, start: bool, end: bool) {
    for &i in idx {
        if let Some(e) = f.events.get_mut(i) {
            if start { e.start = (e.start + delta_ms).max(0); }
            if end { e.end = (e.end + delta_ms).max(0); }
        }
    }
}

/// Linear retime: map two (old,new) anchor points.
pub fn linear(f: &mut AssFile, idx: &[usize], old1: i64, new1: i64, old2: i64, new2: i64) {
    if old2 == old1 { return; }
    let k = (new2 - new1) as f64 / (old2 - old1) as f64;
    for &i in idx {
        if let Some(e) = f.events.get_mut(i) {
            e.start = (new1 as f64 + (e.start - old1) as f64 * k).round().max(0.0) as i64;
            e.end = (new1 as f64 + (e.end - old1) as f64 * k).round().max(0.0) as i64;
        }
    }
}

/// Convert timings between frame rates (e.g. 23.976 -> 25).
pub fn change_fps(f: &mut AssFile, from: f64, to: f64) {
    let k = from / to;
    for e in &mut f.events {
        e.start = (e.start as f64 * k).round() as i64;
        e.end = (e.end as f64 * k).round() as i64;
    }
}

pub fn sort_by_start(f: &mut AssFile) {
    f.events.sort_by_key(|e| (e.start, e.end));
}

/// Make consecutive lines (sorted by time) contiguous when the gap is < `max_gap_ms`.
/// Returns number of lines changed.
pub fn close_gaps(f: &mut AssFile, max_gap_ms: i64) -> usize {
    let mut order: Vec<usize> = (0..f.events.len()).filter(|&i| !f.events[i].comment).collect();
    order.sort_by_key(|&i| f.events[i].start);
    let mut n = 0;
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let gap = f.events[b].start - f.events[a].end;
        if gap > 0 && gap < max_gap_ms { f.events[a].end = f.events[b].start; n += 1; }
    }
    n
}

/// Enforce a minimum gap between consecutive lines by trimming the earlier line's end (never below min_dur).
pub fn enforce_gap(f: &mut AssFile, min_gap_ms: i64, min_dur_ms: i64) -> usize {
    let mut order: Vec<usize> = (0..f.events.len()).filter(|&i| !f.events[i].comment).collect();
    order.sort_by_key(|&i| f.events[i].start);
    let mut n = 0;
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let gap = f.events[b].start - f.events[a].end;
        if gap < min_gap_ms {
            let new_end = (f.events[b].start - min_gap_ms).max(f.events[a].start + min_dur_ms);
            if new_end != f.events[a].end { f.events[a].end = new_end; n += 1; }
        }
    }
    n
}

/// Snap start/end to the nearest frame boundary.
pub fn snap_to_frames(f: &mut AssFile, idx: &[usize], fr: FrameRate) {
    for &i in idx {
        if let Some(e) = f.events.get_mut(i) {
            e.start = fr.start_ms_for_frame(fr.frame_from_ms(e.start + 1));
            e.end = fr.end_ms_for_frame(fr.frame_from_ms(e.end + 1));
        }
    }
}

/// Split an event at `at_ms` (and optionally at text position `char_pos`).
pub fn split(f: &mut AssFile, i: usize, at_ms: i64, char_pos: Option<usize>) -> bool {
    let Some(e) = f.events.get(i).cloned() else { return false };
    if at_ms <= e.start || at_ms >= e.end { return false; }
    let (t1, t2) = match char_pos {
        Some(p) => {
            let chars: Vec<char> = e.text.chars().collect();
            let p = p.min(chars.len());
            (chars[..p].iter().collect::<String>().trim_end().to_string(), chars[p..].iter().collect::<String>().trim_start().to_string())
        }
        None => (e.text.clone(), e.text.clone()),
    };
    let mut a = e.clone(); a.end = at_ms; a.text = t1;
    let mut b = e; b.start = at_ms; b.text = t2;
    f.events[i] = a;
    f.events.insert(i + 1, b);
    true
}

/// Merge events i..=j into one (texts joined with `\N`).
pub fn merge(f: &mut AssFile, i: usize, j: usize, sep: &str) {
    if i >= j || j >= f.events.len() { return; }
    let first = f.events[i].clone();
    let mut m = Event { text: String::new(), ..first };
    let slice: Vec<Event> = f.events.drain(i..=j).collect();
    m.start = slice.iter().map(|e| e.start).min().unwrap();
    m.end = slice.iter().map(|e| e.end).max().unwrap();
    m.text = slice.iter().map(|e| e.text.as_str()).collect::<Vec<_>>().join(sep);
    f.events.insert(i, m);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mk(a: i64, b: i64, t: &str) -> Event { Event { start: a, end: b, text: t.into(), ..Default::default() } }
    #[test]
    fn ops() {
        let mut f = AssFile::new_default();
        f.events = vec![mk(0, 1000, "a"), mk(1100, 2000, "b")];
        assert_eq!(close_gaps(&mut f, 200), 1);
        assert_eq!(f.events[0].end, 1100);
        shift(&mut f, &[0, 1], 500, true, true);
        assert_eq!(f.events[1].end, 2500);
        assert!(split(&mut f, 0, 1000, Some(1)));
        assert_eq!(f.events.len(), 3);
        merge(&mut f, 0, 1, "\\N");
        assert_eq!(f.events.len(), 2);
    }
}
