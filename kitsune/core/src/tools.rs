//! Subtitle Edit-style quality & batch tools.
use crate::model::*;
use crate::tags::*;

#[derive(Clone, Debug)]
pub struct Issue { pub line: usize, pub kind: &'static str, pub msg: String }

#[derive(Clone, Debug)]
pub struct QcConfig {
    pub max_cps: f64,
    pub min_dur: i64,
    pub max_dur: i64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_gap: i64,
}
impl Default for QcConfig {
    fn default() -> Self { QcConfig { max_cps: 20.0, min_dur: 500, max_dur: 7000, max_line_len: 42, max_lines: 2, min_gap: 0 } }
}

/// Event indices (0-based within events) with problems.
pub fn check(script: &Script, cfg: &QcConfig) -> Vec<Issue> {
    let mut issues = Vec::new();
    let evs: Vec<&Event> = script.events().collect();
    for (i, e) in evs.iter().enumerate() {
        if e.comment { continue; }
        let n = i + 1;
        if e.end < e.start { issues.push(Issue { line: n, kind: "negative-duration", msg: "end before start".into() }); }
        let d = e.duration();
        if d >= 0 && d < cfg.min_dur { issues.push(Issue { line: n, kind: "short", msg: format!("duration {}ms < {}", d, cfg.min_dur) }); }
        if d > cfg.max_dur { issues.push(Issue { line: n, kind: "long", msg: format!("duration {}ms > {}", d, cfg.max_dur) }); }
        let txt = plain(&e.text);
        if txt.trim().is_empty() && !e.text.contains("\\p1") { issues.push(Issue { line: n, kind: "empty", msg: "empty text".into() }); }
        let chars = visible_len(&e.text);
        if d > 0 {
            let cps = chars as f64 * 1000.0 / d as f64;
            if cps > cfg.max_cps { issues.push(Issue { line: n, kind: "cps", msg: format!("{:.1} cps > {}", cps, cfg.max_cps) }); }
        }
        let lines: Vec<&str> = txt.split('\n').collect();
        if lines.len() > cfg.max_lines { issues.push(Issue { line: n, kind: "lines", msg: format!("{} lines", lines.len()) }); }
        for l in &lines { if l.chars().count() > cfg.max_line_len { issues.push(Issue { line: n, kind: "line-length", msg: format!("{} chars", l.chars().count()) }); break; } }
        if txt.contains("  ") { issues.push(Issue { line: n, kind: "double-space", msg: "double space".into() }); }
        if script.style(&e.style).is_none() { issues.push(Issue { line: n, kind: "missing-style", msg: format!("style '{}' not defined", e.style) }); }
        if e.text.matches('{').count() != e.text.matches('}').count() { issues.push(Issue { line: n, kind: "unbalanced-braces", msg: "unbalanced { }".into() }); }
    }
    // overlaps (same layer, sorted by start)
    let mut idx: Vec<usize> = (0..evs.len()).filter(|&i| !evs[i].comment).collect();
    idx.sort_by_key(|&i| evs[i].start);
    for w in idx.windows(2) {
        let (a, b) = (evs[w[0]], evs[w[1]]);
        if a.style == b.style && a.layer == b.layer && b.start < a.end {
            issues.push(Issue { line: w[1] + 1, kind: "overlap", msg: format!("overlaps line {} by {}ms", w[0] + 1, a.end - b.start) });
        } else if a.style == b.style && b.start >= a.end && b.start - a.end < cfg.min_gap {
            issues.push(Issue { line: w[1] + 1, kind: "gap", msg: format!("gap {}ms < {}", b.start - a.end, cfg.min_gap) });
        }
    }
    issues
}

/// Shift all (or selected) events by delta ms.
pub fn shift(script: &mut Script, delta: i64, only: Option<&dyn Fn(usize) -> bool>) {
    for (i, e) in script.events_mut().enumerate() {
        if only.map_or(true, |f| f(i)) { e.start = (e.start + delta).max(0); e.end = (e.end + delta).max(0); }
    }
}

/// Fix overlaps of consecutive same-style events by trimming the earlier end.
pub fn fix_overlaps(script: &mut Script) -> usize {
    let mut evs: Vec<&mut Event> = script.events_mut().collect();
    let mut idx: Vec<usize> = (0..evs.len()).collect();
    idx.sort_by_key(|&i| evs[i].start);
    let mut n = 0;
    for w in idx.windows(2) {
        let (bs, bc, bst, bl) = (evs[w[1]].start, evs[w[1]].comment, evs[w[1]].style.clone(), evs[w[1]].layer);
        let a = &mut evs[w[0]];
        if !a.comment && !bc && a.style == bst && a.layer == bl && bs < a.end && bs > a.start { a.end = bs; n += 1; }
    }
    n
}

/// Enforce minimum gap between consecutive events (trim previous end).
pub fn enforce_gap(script: &mut Script, gap: i64) -> usize {
    let mut n = 0;
    let mut prev: Option<&mut Event> = None;
    for e in script.events_mut().filter(|e| !e.comment) {
        if let Some(p) = prev.take() {
            if e.start >= p.start && e.start - p.end < gap && e.start - gap > p.start { p.end = e.start - gap; n += 1; }
        }
        prev = Some(e);
    }
    n
}

/// Extend durations to reach min CPS-friendly length without hitting next line.
pub fn fix_short_durations(script: &mut Script, min_dur: i64, cps: f64) -> usize {
    let starts: Vec<i64> = script.events().map(|e| e.start).collect();
    let mut n = 0;
    for (i, e) in script.events_mut().enumerate() {
        if e.comment { continue; }
        let need_cps = (visible_len(&e.text) as f64 * 1000.0 / cps).ceil() as i64;
        let want = min_dur.max(need_cps);
        if e.duration() < want {
            let limit = starts.iter().enumerate().filter(|(j, s)| *j != i && **s > e.start).map(|(_, s)| *s).min().unwrap_or(i64::MAX);
            let new_end = (e.start + want).min(limit);
            if new_end > e.end { e.end = new_end; n += 1; }
        }
    }
    n
}

/// Merge consecutive events with identical text and touching/overlapping times.
pub fn merge_duplicates(script: &mut Script) -> usize {
    let mut n = 0;
    let mut out: Vec<Entry> = Vec::new();
    for e in std::mem::take(&mut script.entries) {
        if let Entry::Event(ev) = &e {
            if let Some(Entry::Event(p)) = out.last_mut() {
                if !ev.comment && !p.comment && p.text == ev.text && p.style == ev.style && ev.start <= p.end + 1 {
                    p.end = p.end.max(ev.end); n += 1; continue;
                }
            }
        }
        out.push(e);
    }
    script.entries = out;
    n
}

/// Split a long event at the best newline-less boundary into two by time ratio.
pub fn split_event(ev: &Event, at_char: usize) -> Option<(Event, Event)> {
    let plain_text = ev.text.clone();
    let chars: Vec<char> = plain_text.chars().collect();
    if at_char == 0 || at_char >= chars.len() { return None; }
    let total = chars.len() as f64;
    let mid = ev.start + (ev.duration() as f64 * at_char as f64 / total) as i64;
    let a: String = chars[..at_char].iter().collect();
    let b: String = chars[at_char..].iter().collect();
    let mut e1 = ev.clone(); let mut e2 = ev.clone();
    e1.text = a.trim_end().into(); e1.end = mid;
    e2.text = b.trim_start().into(); e2.start = mid;
    Some((e1, e2))
}

/// Auto-balance text into at most `lines` lines of ~equal length at spaces.
pub fn balance_lines(text: &str, max_len: usize) -> String {
    let t = plain(text).replace('\n', " ");
    if t.chars().count() <= max_len { return text.to_string(); }
    let words: Vec<&str> = t.split_whitespace().collect();
    let total: usize = t.chars().count();
    let mut best = (usize::MAX, 0usize);
    let mut acc = 0;
    for (i, w) in words.iter().enumerate().take(words.len().saturating_sub(1)) {
        acc += w.chars().count() + if i > 0 { 1 } else { 0 };
        let diff = (total as isize - 2 * acc as isize).unsigned_abs();
        if diff < best.0 { best = (diff, i + 1); }
    }
    format!("{}\\N{}", words[..best.1].join(" "), words[best.1..].join(" "))
}

/// Convert SDH-ish cleanup: remove `[text]`, `(text)`, speaker labels `NAME:`.
pub fn remove_hi(text: &str) -> String {
    let mut out = String::new();
    let (mut depth_sq, mut depth_par) = (0i32, 0i32);
    for c in text.chars() {
        match c {
            '[' => depth_sq += 1, ']' if depth_sq > 0 => depth_sq -= 1,
            '(' => depth_par += 1, ')' if depth_par > 0 => depth_par -= 1,
            _ if depth_sq == 0 && depth_par == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Reading speed in chars per second.
pub fn cps(e: &Event) -> f64 { if e.duration() <= 0 { 0.0 } else { visible_len(&e.text) as f64 * 1000.0 / e.duration() as f64 } }

/// Linear resync: map times so that (a1->b1) and (a2->b2) (like SE "Adjust via sync points").
pub fn linear_sync(script: &mut Script, a1: i64, b1: i64, a2: i64, b2: i64) {
    if a2 == a1 { return; }
    let k = (b2 - b1) as f64 / (a2 - a1) as f64;
    for e in script.events_mut() {
        e.start = (b1 as f64 + (e.start - a1) as f64 * k).round().max(0.0) as i64;
        e.end = (b1 as f64 + (e.end - a1) as f64 * k).round().max(0.0) as i64;
    }
}

/// Change frame rate of timing (e.g. 23.976 -> 25).
pub fn change_fps(script: &mut Script, from: f64, to: f64) {
    let k = from / to;
    for e in script.events_mut() { e.start = (e.start as f64 * k).round() as i64; e.end = (e.end as f64 * k).round() as i64; }
}
