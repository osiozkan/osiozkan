//! Timing tools: shift, retime, fps conversion, snapping, chaining, split/merge.
use crate::ass::Event;
use crate::time::{Fps, Ms};

pub fn shift(events: &mut [Event], idx: &[usize], delta: Ms, start: bool, end: bool) {
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            if start {
                e.start = (e.start + delta).max(0);
            }
            if end {
                e.end = (e.end + delta).max(0);
            }
            if e.end < e.start {
                e.end = e.start;
            }
        }
    }
}

pub fn shift_frames(events: &mut [Event], idx: &[usize], frames: i64, fps: Fps) {
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            let sf = fps.frame_at(e.start) + frames;
            let ef = fps.frame_at(e.end) + frames;
            e.start = fps.start_ms(sf.max(0));
            e.end = fps.start_ms(ef.max(0));
        }
    }
}

/// Two-point linear resync: maps `a1→b1`, `a2→b2` (fixes drift between releases).
pub fn linear_retime(events: &mut [Event], idx: &[usize], a1: Ms, b1: Ms, a2: Ms, b2: Ms) {
    if a2 == a1 {
        return;
    }
    let k = (b2 - b1) as f64 / (a2 - a1) as f64;
    let f = |t: Ms| (b1 as f64 + (t - a1) as f64 * k).round() as Ms;
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            e.start = f(e.start).max(0);
            e.end = f(e.end).max(e.start);
        }
    }
}

/// Convert timing made for `from` fps to play at `to` fps.
pub fn convert_fps(events: &mut [Event], idx: &[usize], from: Fps, to: Fps) {
    let k = from.as_f64() / to.as_f64();
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            e.start = (e.start as f64 * k).round() as Ms;
            e.end = (e.end as f64 * k).round() as Ms;
        }
    }
}

/// Snap to frame boundaries (start: first ms of its frame, end: last ms of its frame).
pub fn snap_to_frames(events: &mut [Event], idx: &[usize], fps: Fps) {
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            let sf = fps.frame_at(e.start);
            let ef = fps.frame_at(e.end);
            let sf = if fps.start_ms(sf) < e.start { sf + 1 } else { sf };
            e.start = fps.start_ms(sf);
            e.end = fps.start_ms(ef).max(e.start);
        }
    }
}

/// Snap event boundaries to keyframes (ms). Starts snap back/forward within the
/// given windows; ends snap to the keyframe boundary.
pub fn snap_to_keyframes(events: &mut [Event], idx: &[usize], keys: &[Ms], start_before: Ms, start_after: Ms, end_before: Ms, end_after: Ms) -> usize {
    let mut changed = 0;
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            let s = e.start;
            if let Some(&k) = keys.iter().find(|&&k| k >= s - start_before && k <= s + start_after) {
                if k != e.start {
                    e.start = k;
                    changed += 1;
                }
            }
            let en = e.end;
            if let Some(&k) = keys.iter().find(|&&k| k >= en - end_before && k <= en + end_after) {
                // end is exclusive of the keyframe itself
                if k != e.end {
                    e.end = k.max(e.start);
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Lead-in / lead-out padding.
pub fn lead(events: &mut [Event], idx: &[usize], lead_in: Ms, lead_out: Ms) {
    for &i in idx {
        if let Some(e) = events.get_mut(i) {
            e.start = (e.start - lead_in).max(0);
            e.end += lead_out;
        }
    }
}

/// "Make times continuous": close gaps up to `max_gap` between consecutive
/// events (sorted order of `idx` by start). Gaps smaller than `min_gap` after
/// chaining are left as `min_gap` to avoid flicker.
pub fn chain(events: &mut [Event], idx: &[usize], max_gap: Ms, min_gap: Ms) -> usize {
    let mut order: Vec<usize> = idx.iter().copied().filter(|&i| i < events.len()).collect();
    order.sort_by_key(|&i| events[i].start);
    let mut n = 0;
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let gap = events[b].start - events[a].end;
        if gap > 0 && gap <= max_gap {
            events[a].end = events[b].start - min_gap;
            if events[a].end < events[a].start {
                events[a].end = events[a].start;
            }
            n += 1;
        }
    }
    n
}

/// Remove overlaps among same-style, same-layer events by trimming the earlier one.
pub fn fix_overlaps(events: &mut [Event]) -> usize {
    let mut order: Vec<usize> = (0..events.len()).filter(|&i| !events[i].is_comment()).collect();
    order.sort_by_key(|&i| (events[i].start, events[i].end));
    let mut n = 0;
    for w in 0..order.len() {
        for x in w + 1..order.len() {
            let (a, b) = (order[w], order[x]);
            if events[b].start >= events[a].end {
                break;
            }
            if events[a].style == events[b].style && events[a].layer == events[b].layer && events[a].text != events[b].text {
                events[a].end = events[b].start.max(events[a].start);
                n += 1;
                break;
            }
        }
    }
    n
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Start,
    End,
    Style,
    Actor,
    Effect,
    Layer,
}

/// Stable sort; ties keep original order.
pub fn sort(events: &mut [Event], key: SortKey) {
    match key {
        SortKey::Start => events.sort_by_key(|e| (e.start, e.end)),
        SortKey::End => events.sort_by_key(|e| (e.end, e.start)),
        SortKey::Style => events.sort_by(|a, b| a.style.cmp(&b.style).then(a.start.cmp(&b.start))),
        SortKey::Actor => events.sort_by(|a, b| a.actor.cmp(&b.actor).then(a.start.cmp(&b.start))),
        SortKey::Effect => events.sort_by(|a, b| a.effect.cmp(&b.effect).then(a.start.cmp(&b.start))),
        SortKey::Layer => events.sort_by(|a, b| a.layer.cmp(&b.layer).then(a.start.cmp(&b.start))),
    }
}

/// Split by time: returns the second half; the first half's end is trimmed.
pub fn split_at_time(e: &mut Event, t: Ms) -> Option<Event> {
    if t <= e.start || t >= e.end {
        return None;
    }
    let mut second = e.clone();
    second.start = t;
    e.end = t;
    Some(second)
}

/// Split text at char offset (visible text unaffected by tags is the caller's
/// concern; this works on raw text). Time is split proportionally.
pub fn split_at_char(e: &mut Event, pos: usize) -> Option<Event> {
    let chars: Vec<char> = e.text.chars().collect();
    if pos == 0 || pos >= chars.len() {
        return None;
    }
    let (a, b): (String, String) = (chars[..pos].iter().collect(), chars[pos..].iter().collect());
    let a = a.trim_end().to_string();
    let b = b.trim_start().to_string();
    let mid = e.start + ((e.end - e.start) as f64 * (pos as f64 / chars.len() as f64)).round() as Ms;
    let mut second = e.clone();
    e.text = a;
    e.end = mid;
    second.text = b;
    second.start = mid;
    Some(second)
}

/// Merge consecutive events into one (text joined with `\N`, span union).
pub fn merge(events: &[Event], sep: &str) -> Option<Event> {
    let first = events.first()?;
    let mut out = first.clone();
    for e in &events[1..] {
        out.start = out.start.min(e.start);
        out.end = out.end.max(e.end);
        out.text.push_str(sep);
        out.text.push_str(&e.text);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(s: Ms, e: Ms, t: &str) -> Event {
        Event::new(s, e, "Default", t)
    }
    #[test]
    fn shifting_and_retime() {
        let mut v = vec![ev(1000, 2000, "a"), ev(3000, 4000, "b")];
        shift(&mut v, &[0, 1], 500, true, true);
        assert_eq!((v[0].start, v[1].end), (1500, 4500));
        linear_retime(&mut v, &[0, 1], 1500, 1500, 3500, 4500);
        assert_eq!(v[1].start, 4500);
    }
    #[test]
    fn chaining() {
        let mut v = vec![ev(0, 1000, "a"), ev(1100, 2000, "b"), ev(5000, 6000, "c")];
        assert_eq!(chain(&mut v, &[0, 1, 2], 300, 0), 1);
        assert_eq!(v[0].end, 1100);
        assert_eq!(v[1].end, 2000);
    }
    #[test]
    fn splitting() {
        let mut e = ev(0, 1000, "abcdef");
        let s = split_at_char(&mut e, 3).unwrap();
        assert_eq!((e.text.as_str(), s.text.as_str()), ("abc", "def"));
        assert_eq!(e.end, 500);
        assert_eq!(s.start, 500);
    }
    #[test]
    fn overlaps() {
        let mut v = vec![ev(0, 3000, "a"), ev(2000, 4000, "b")];
        assert_eq!(fix_overlaps(&mut v), 1);
        assert_eq!(v[0].end, 2000);
    }
}
