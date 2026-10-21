//! Timing operations and quality checks (the Subtitle Edit / Aegisub "tools" menu).

use crate::ass::{Document, Entry, Event};
use crate::tags::{visible_len, visible_text};
use std::collections::HashMap;

/// Shift events by `delta_ms`. `only` restricts to 0-based event indices (None = all).
pub fn shift(doc: &mut Document, delta_ms: i64, only: Option<&[usize]>) {
    for (i, e) in doc.events_mut().enumerate() {
        if only.map(|o| o.contains(&i)).unwrap_or(true) {
            e.start = (e.start + delta_ms).max(0);
            e.end = (e.end + delta_ms).max(0);
        }
    }
}

/// Linear retime (e.g. 23.976 -> 25 fps): t' = t * to_fps/from_fps inverse (stretching durations).
pub fn change_framerate(doc: &mut Document, from_fps: f64, to_fps: f64) {
    let k = from_fps / to_fps;
    for e in doc.events_mut() {
        e.start = (e.start as f64 * k).round() as i64;
        e.end = (e.end as f64 * k).round() as i64;
    }
}

/// Two-point resync: map `a1 -> b1` and `a2 -> b2` linearly (drift correction).
pub fn resync_two_point(doc: &mut Document, a1: i64, b1: i64, a2: i64, b2: i64) {
    if a2 == a1 {
        return;
    }
    let k = (b2 - b1) as f64 / (a2 - a1) as f64;
    for e in doc.events_mut() {
        e.start = (b1 as f64 + (e.start - a1) as f64 * k).round().max(0.0) as i64;
        e.end = (b1 as f64 + (e.end - a1) as f64 * k).round().max(0.0) as i64;
    }
}

/// Snap start/end to the nearest frame boundary.
pub fn snap_to_frames(doc: &mut Document, fps: f64) {
    let snap = |t: i64| ((t as f64 * fps / 1000.0).round() * 1000.0 / fps).round() as i64;
    for e in doc.events_mut() {
        e.start = snap(e.start);
        e.end = snap(e.end);
    }
}

#[derive(Debug, Clone)]
pub struct QaOptions {
    pub max_cps: f64,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_gap_ms: i64,
}

impl Default for QaOptions {
    fn default() -> Self {
        QaOptions { max_cps: 20.0, min_duration_ms: 500, max_duration_ms: 8000, max_line_len: 42, max_lines: 2, min_gap_ms: 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// 1-based index among events
    pub event: usize,
    pub kind: &'static str,
    pub message: String,
}

pub fn qa(doc: &Document, o: &QaOptions) -> Vec<Issue> {
    let styles: Vec<&str> = doc.styles().map(|s| s.name.as_str()).collect();
    let mut out = Vec::new();
    let evs: Vec<(usize, &Event)> = doc.events().enumerate().filter(|(_, e)| !e.comment).collect();
    // next event on the same layer (file order) for every event: one reverse pass instead of a scan per line
    let mut next_same_layer: Vec<Option<usize>> = vec![None; evs.len()];
    {
        let mut last: HashMap<i32, usize> = HashMap::new();
        for n in (0..evs.len()).rev() {
            next_same_layer[n] = last.get(&evs[n].1.layer).copied();
            last.insert(evs[n].1.layer, n);
        }
    }
    for (n, (i, e)) in evs.iter().enumerate() {
        let id = i + 1;
        let dur = e.end - e.start;
        let mut push = |kind: &'static str, message: String| out.push(Issue { event: id, kind, message });
        if e.end <= e.start {
            push("bad-timing", format!("end ({}) is not after start ({})", e.end, e.start));
        }
        if !styles.contains(&e.style.as_str()) {
            push("missing-style", format!("style '{}' is not defined", e.style));
        }
        let vt = visible_text(&e.text);
        if vt.trim().is_empty() && !e.text.contains("\\p1") {
            push("empty", "line has no visible text".into());
            continue;
        }
        if e.text.contains("\\p1") {
            continue; // drawing
        }
        if dur > 0 {
            if dur < o.min_duration_ms {
                push("short", format!("duration {} ms < {} ms", dur, o.min_duration_ms));
            }
            if dur > o.max_duration_ms {
                push("long", format!("duration {} ms > {} ms", dur, o.max_duration_ms));
            }
            let cps = visible_len(&e.text) as f64 / (dur as f64 / 1000.0);
            if cps > o.max_cps {
                push("cps", format!("{:.1} chars/s > {:.1}", cps, o.max_cps));
            }
        }
        let lines: Vec<&str> = vt.split('\n').collect();
        if lines.len() > o.max_lines {
            push("lines", format!("{} lines > {}", lines.len(), o.max_lines));
        }
        for l in &lines {
            if l.chars().count() > o.max_line_len {
                push("line-length", format!("line of {} chars > {}", l.chars().count(), o.max_line_len));
                break;
            }
        }
        if vt != vt.trim() && !vt.starts_with('\n') {
            push("whitespace", "leading/trailing whitespace".into());
        }
        if vt.contains("  ") {
            push("whitespace", "double space".into());
        }
        // overlap with next event on the same layer
        if let Some((j, nx)) = next_same_layer[n].map(|k| &evs[k]).filter(|(_, x)| x.start >= e.start) {
            if nx.start < e.end && nx.style == e.style {
                push("overlap", format!("overlaps event {} by {} ms", j + 1, e.end - nx.start));
            } else if nx.start - e.end < o.min_gap_ms && nx.start >= e.end {
                push("gap", format!("gap to event {} is {} ms < {} ms", j + 1, nx.start - e.end, o.min_gap_ms));
            }
        }
    }
    out
}

/// Fix overlaps between consecutive same-layer/same-style events by trimming the earlier end.
pub fn fix_overlaps(doc: &mut Document) -> usize {
    let mut fixed = 0;
    let mut last: HashMap<(i32, String), usize> = HashMap::new();
    let idxs: Vec<usize> = doc.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Event(ev) if !ev.comment)).map(|(i, _)| i).collect();
    let mut order = idxs.clone();
    order.sort_by_key(|&i| if let Entry::Event(e) = &doc.entries[i] { e.start } else { 0 });
    for i in order {
        let (key, start) = if let Entry::Event(e) = &doc.entries[i] { ((e.layer, e.style.clone()), e.start) } else { continue };
        if let Some(&p) = last.get(&key) {
            if let Entry::Event(pe) = &mut doc.entries[p] {
                if pe.end > start && start > pe.start {
                    pe.end = start;
                    fixed += 1;
                }
            }
        }
        last.insert(key, i);
    }
    fixed
}

/// Extend too-short lines up to `min_ms` where the next event allows it.
pub fn extend_short(doc: &mut Document, min_ms: i64) -> usize {
    let starts: Vec<i64> = doc.events().map(|e| e.start).collect();
    let mut n = 0;
    for (i, e) in doc.events_mut().enumerate() {
        if e.comment || e.end - e.start >= min_ms {
            continue;
        }
        let limit = starts.iter().enumerate().filter(|(j, s)| *j != i && **s >= e.end).map(|(_, s)| *s).min().unwrap_or(i64::MAX);
        let want = e.start + min_ms;
        let ne = want.min(limit);
        if ne > e.end {
            e.end = ne;
            n += 1;
        }
    }
    n
}

/// Regex find/replace over event text. Returns number of changed events.
pub fn find_replace(doc: &mut Document, pattern: &str, repl: &str, regex_mode: bool, ignore_case: bool) -> anyhow::Result<usize> {
    let pat = if regex_mode { pattern.to_string() } else { regex::escape(pattern) };
    let re = regex::RegexBuilder::new(&pat).case_insensitive(ignore_case).build()?;
    let repl_str = if regex_mode { repl.to_string() } else { repl.replace('$', "$$") };
    let mut n = 0;
    for e in doc.events_mut() {
        let new = re.replace_all(&e.text, repl_str.as_str()).to_string();
        if new != e.text {
            e.text = new;
            n += 1;
        }
    }
    Ok(n)
}

/// Remove all override tags (plain-text export).
pub fn strip_all_tags(doc: &mut Document) {
    for e in doc.events_mut() {
        e.text = crate::tags::strip_tags(&e.text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> Document {
        let mut d = Document::new_default();
        for (s, e, t) in [(0, 1000, "Hello"), (900, 2000, "World"), (2100, 2200, "x  y"), (3000, 4000, "{\\i1}")] {
            d.entries.push(Entry::Event(Event { start: s, end: e, text: t.into(), ..Event::default() }));
        }
        d
    }
    #[test]
    fn qa_finds() {
        let d = doc();
        let issues = qa(&d, &QaOptions::default());
        let kinds: Vec<_> = issues.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&"overlap"));
        assert!(kinds.contains(&"short"));
        assert!(kinds.contains(&"whitespace"));
        assert!(kinds.contains(&"empty"));
    }
    #[test]
    fn fixes() {
        let mut d = doc();
        assert_eq!(fix_overlaps(&mut d), 1);
        assert_eq!(d.events().next().unwrap().end, 900);
        assert_eq!(extend_short(&mut d, 500), 1);
        shift(&mut d, 500, None);
        assert_eq!(d.events().next().unwrap().start, 500);
        assert_eq!(find_replace(&mut d, "world", "Earth", false, true).unwrap(), 1);
    }
    #[test]
    fn resync() {
        let mut d = doc();
        resync_two_point(&mut d, 0, 100, 2000, 2100);
        assert_eq!(d.events().next().unwrap().start, 100);
        change_framerate(&mut d, 23.976, 25.0);
    }
}
