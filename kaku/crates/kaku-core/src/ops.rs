//! Timing & bulk-edit operations (Subtitle Edit / Aegisub style).

use crate::ass::{Kind, Line, Script};
use crate::tags;
use crate::time::Fps;

/// Shift every selected line (`None` = all) by `delta` ms.
pub fn shift(script: &mut Script, delta: i64, sel: Option<&[usize]>) {
    for (i, l) in script.lines_mut().enumerate() {
        if sel.map_or(true, |s| s.contains(&i)) {
            l.start = (l.start + delta).max(0);
            l.end = (l.end + delta).max(0);
        }
    }
}

/// Linear retime (two-point sync): map `a0->b0` and `a1->b1`.
pub fn linear_sync(script: &mut Script, a0: i64, b0: i64, a1: i64, b1: i64) {
    if a1 == a0 {
        return;
    }
    let k = (b1 - b0) as f64 / (a1 - a0) as f64;
    let map = |t: i64| (b0 as f64 + (t - a0) as f64 * k).round() as i64;
    for l in script.lines_mut() {
        l.start = map(l.start).max(0);
        l.end = map(l.end).max(0);
    }
}

/// Change frame rate: times are scaled by `from/to`.
pub fn change_fps(script: &mut Script, from: Fps, to: Fps) {
    let k = from.as_f64() / to.as_f64();
    for l in script.lines_mut() {
        l.start = (l.start as f64 * k).round() as i64;
        l.end = (l.end as f64 * k).round() as i64;
    }
}

/// Snap line starts/ends to the nearest video frame boundary.
pub fn snap_to_frames(script: &mut Script, fps: Fps) {
    for l in script.lines_mut() {
        l.start = fps.frame_to_ms(fps.ms_to_frame(l.start + 1).max(0));
        l.end = fps.frame_to_ms(fps.ms_to_frame(l.end + 1).max(0));
    }
}

/// Make each line's end touch the next line's start when the gap is below `max_gap` ms (chaining).
pub fn close_gaps(script: &mut Script, max_gap: i64) -> usize {
    let mut idx: Vec<usize> = Vec::new();
    for (i, l) in script.lines().enumerate() {
        if l.kind == Kind::Dialogue {
            idx.push(i);
        }
    }
    let mut lines: Vec<&mut Line> = script.lines_mut().collect();
    let mut order: Vec<usize> = idx.clone();
    order.sort_by_key(|&i| lines[i].start);
    let mut n = 0;
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let gap = lines[b].start - lines[a].end;
        if gap > 0 && gap <= max_gap && lines[a].style == lines[b].style {
            lines[a].end = lines[b].start;
            n += 1;
        }
    }
    n
}

/// Enforce a minimum gap (e.g. 1 frame) between consecutive same-style lines by trimming the earlier end.
pub fn enforce_min_gap(script: &mut Script, min_gap: i64) -> usize {
    let mut lines: Vec<&mut Line> = script.lines_mut().filter(|l| l.kind == Kind::Dialogue).collect();
    lines.sort_by_key(|l| l.start);
    let mut n = 0;
    for i in 0..lines.len().saturating_sub(1) {
        let next_start = lines[i + 1].start;
        if lines[i].style == lines[i + 1].style && next_start - lines[i].end < min_gap && lines[i].end > lines[i].start {
            let new_end = (next_start - min_gap).max(lines[i].start + 1);
            if new_end < lines[i].end {
                lines[i].end = new_end;
                n += 1;
            }
        }
    }
    n
}

/// Extend short lines up to reading speed `cps` (capped by `max_dur` and not past next line).
pub fn extend_to_cps(script: &mut Script, cps: f64, max_dur: i64) -> usize {
    let n_lines = script.lines().count();
    let starts: Vec<i64> = script.lines().map(|l| l.start).collect();
    let mut n = 0;
    for (i, l) in script.lines_mut().enumerate() {
        if l.kind != Kind::Dialogue || tags::has_drawing(&l.text) {
            continue;
        }
        let chars = tags::visible_len(&l.text) as f64;
        let need = (chars / cps * 1000.0).ceil() as i64;
        if l.duration() < need {
            let mut new_end = (l.start + need).min(l.start + max_dur);
            // Don't run into the next-starting line.
            let limit = (0..n_lines).filter(|&j| j != i && starts[j] >= l.end).map(|j| starts[j]).min();
            if let Some(lim) = limit {
                new_end = new_end.min(lim);
            }
            if new_end > l.end {
                l.end = new_end;
                n += 1;
            }
        }
    }
    n
}

/// Split a line at `at` ms into two lines (text split at `char_idx` of visible text approximated by byte index in raw text).
pub fn split_line(l: &Line, at: i64, text_idx: usize) -> Option<(Line, Line)> {
    if at <= l.start || at >= l.end || text_idx > l.text.len() || !l.text.is_char_boundary(text_idx) {
        return None;
    }
    let mut a = l.clone();
    let mut b = l.clone();
    a.end = at;
    b.start = at;
    a.text = l.text[..text_idx].trim_end().to_string();
    b.text = l.text[text_idx..].trim_start().to_string();
    Some((a, b))
}

/// Merge consecutive lines (by index list into `lines()`), joining text with `\N` or space.
pub fn merge_lines(script: &mut Script, first: usize, count: usize, sep: &str) {
    let mut lines: Vec<&mut Line> = script.lines_mut().collect();
    if count < 2 || first + count > lines.len() {
        return;
    }
    let mut text = lines[first].text.clone();
    let mut end = lines[first].end;
    for k in 1..count {
        text.push_str(sep);
        text.push_str(&lines[first + k].text);
        end = end.max(lines[first + k].end);
    }
    lines[first].text = text;
    lines[first].end = end;
    // Mark the rest for removal via empty text; removal is done below.
    for k in 1..count {
        lines[first + k].effect = "\u{0}kaku-del".into();
    }
    drop(lines);
    script.entries.retain(|e| !matches!(e, crate::ass::Entry::Line(l) if l.effect == "\u{0}kaku-del"));
}

/// Remove empty / zero-duration lines. Returns removed count.
pub fn remove_empty(script: &mut Script) -> usize {
    let before = script.entries.len();
    script.entries.retain(|e| match e {
        crate::ass::Entry::Line(l) => !(l.kind == Kind::Dialogue && (tags::strip_tags(&l.text).trim().is_empty() && !tags::has_drawing(&l.text) && l.effect.is_empty() || l.end <= l.start)),
        _ => true,
    });
    before - script.entries.len()
}

/// Remove duplicate lines (same start, end, style, text).
pub fn remove_duplicates(script: &mut Script) -> usize {
    let mut seen = std::collections::HashSet::new();
    let before = script.entries.len();
    script.entries.retain(|e| match e {
        crate::ass::Entry::Line(l) => seen.insert((l.start, l.end, l.style.clone(), l.text.clone(), l.kind == Kind::Comment, l.layer)),
        _ => true,
    });
    before - script.entries.len()
}

/// Fix common Turkish/Latin typography problems: double spaces, space before punctuation, `...` → `…` (optional).
pub fn fix_common_errors(script: &mut Script) -> usize {
    use regex::Regex;
    let multi = Regex::new(r" {2,}").unwrap();
    let before_punct = Regex::new(r" +([,.;:!?])").unwrap();
    let mut n = 0;
    for l in script.lines_mut() {
        if l.kind != Kind::Dialogue || tags::has_drawing(&l.text) {
            continue;
        }
        let segs = tags::parse(&l.text);
        let mut out = String::new();
        for s in &segs {
            match s {
                tags::Seg::Text(t) => {
                    let t = multi.replace_all(t, " ");
                    let t = before_punct.replace_all(&t, "$1");
                    out.push_str(&t);
                }
                b => out.push_str(&tags::render(std::slice::from_ref(b))),
            }
        }
        let out = out.replace("\\N ", "\\N").replace(" \\N", "\\N");
        if out != l.text {
            l.text = out;
            n += 1;
        }
    }
    n
}

/// Indices (among `lines()`) of dialogue lines that overlap another line in time on the same layer.
pub fn find_overlaps(script: &Script) -> Vec<usize> {
    let lines: Vec<&Line> = script.lines().collect();
    let mut order: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].kind == Kind::Dialogue && !tags::has_drawing(&lines[i].text)).collect();
    order.sort_by_key(|&i| lines[i].start);
    let mut hit = std::collections::BTreeSet::new();
    for (a, &i) in order.iter().enumerate() {
        for &j in &order[a + 1..] {
            if lines[j].start >= lines[i].end {
                break;
            }
            if lines[i].layer == lines[j].layer {
                hit.insert(i);
                hit.insert(j);
            }
        }
    }
    hit.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sc(v: &[(i64, i64, &str)]) -> Script {
        let mut s = Script::new_default();
        for (a, b, t) in v {
            s.push_line(Line::new(*a, *b, *t));
        }
        s
    }
    #[test]
    fn shifting() {
        let mut s = sc(&[(1000, 2000, "a"), (3000, 4000, "b")]);
        shift(&mut s, -1500, Some(&[0, 1]));
        let v: Vec<_> = s.lines().map(|l| (l.start, l.end)).collect();
        assert_eq!(v, [(0, 500), (1500, 2500)]);
    }
    #[test]
    fn sync() {
        let mut s = sc(&[(1000, 2000, "a"), (11000, 12000, "b")]);
        linear_sync(&mut s, 1000, 2000, 11000, 22000);
        let v: Vec<_> = s.lines().map(|l| l.start).collect();
        assert_eq!(v, [2000, 22000]);
    }
    #[test]
    fn gaps_and_cps() {
        let mut s = sc(&[(0, 1000, "abcdefghijklmnop"), (1200, 2000, "x")]);
        assert_eq!(close_gaps(&mut s, 300), 1);
        assert_eq!(s.lines().next().unwrap().end, 1200);
        let mut s = sc(&[(0, 500, "abcdefghijklmnopqrst"), (10000, 11000, "x")]);
        assert_eq!(extend_to_cps(&mut s, 20.0, 7000), 1);
        assert_eq!(s.lines().next().unwrap().end, 1000);
    }
    #[test]
    fn cleanup() {
        let mut s = sc(&[(0, 1000, "a  b ,c"), (0, 1000, ""), (5, 5, "z")]);
        assert_eq!(fix_common_errors(&mut s), 1);
        assert_eq!(remove_empty(&mut s), 2);
        assert_eq!(s.lines().next().unwrap().text, "a b,c");
    }
    #[test]
    fn overlaps() {
        let s = sc(&[(0, 1000, "a"), (500, 1500, "b"), (2000, 3000, "c")]);
        assert_eq!(find_overlaps(&s), vec![0, 1]);
    }

    #[test]
    fn merging() {
        let mut s = sc(&[(0, 1000, "a"), (1000, 2000, "b"), (3000, 4000, "c")]);
        merge_lines(&mut s, 0, 2, "\\N");
        let v: Vec<_> = s.lines().map(|l| l.text.clone()).collect();
        assert_eq!(v, ["a\\Nb", "c"]);
    }
}
