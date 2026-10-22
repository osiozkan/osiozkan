//! Timing operations on dialogue events.
use crate::ass::{LineKind, Script};
use crate::time::Fps;

/// Shift the given line indices (or all dialogue when `only` is empty) by `delta_ms`.
pub fn shift(script: &mut Script, only: &[usize], delta_ms: i64) {
    for (i, l) in script.lines.iter_mut().enumerate() {
        if let LineKind::Dialogue(e) = &mut l.kind {
            if only.is_empty() || only.contains(&i) {
                e.start_time = (e.start_time + delta_ms).max(0);
                e.end_time = (e.end_time + delta_ms).max(0);
            }
        }
    }
}

/// Snap start/end of all dialogue to frame boundaries.
pub fn snap_to_frames(script: &mut Script, fps: Fps) {
    for e in script.events_mut() {
        e.start_time = fps.snap_start(e.start_time);
        e.end_time = fps.snap_start(e.end_time);
    }
}

/// Make consecutive lines (sorted by start) chain when the gap is below `max_gap_ms`.
pub fn chain(script: &mut Script, max_gap_ms: i64) {
    let mut idx: Vec<usize> = script.lines.iter().enumerate().filter(|(_, l)| matches!(&l.kind, LineKind::Dialogue(e) if !e.comment)).map(|(i, _)| i).collect();
    idx.sort_by_key(|&i| if let LineKind::Dialogue(e) = &script.lines[i].kind { e.start_time } else { 0 });
    for w in idx.windows(2) {
        let next_start = if let LineKind::Dialogue(e) = &script.lines[w[1]].kind { e.start_time } else { continue };
        if let LineKind::Dialogue(e) = &mut script.lines[w[0]].kind {
            let gap = next_start - e.end_time;
            if gap > 0 && gap <= max_gap_ms { e.end_time = next_start; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shift_works() {
        let mut s = Script::parse("[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,a\nDialogue: 0,0:00:02.10,0:00:03.00,Default,,0,0,0,,b\n");
        chain(&mut s, 200);
        assert_eq!(s.events().next().unwrap().end_time, 2100);
        shift(&mut s, &[], 500);
        assert_eq!(s.events().next().unwrap().start_time, 1500);
    }
}
