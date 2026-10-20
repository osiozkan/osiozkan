//! Quality checks ("Fix common errors" style) and automatic fixes.

use crate::ass::Script;
use crate::tags;
use crate::text;

#[derive(Clone, Debug)]
pub struct QcConfig {
    pub max_cps: f64,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_gap_ms: i64,
}

impl Default for QcConfig {
    fn default() -> Self {
        QcConfig { max_cps: 21.0, min_duration_ms: 700, max_duration_ms: 7000, max_line_len: 42, max_lines: 2, min_gap_ms: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NegativeDuration,
    Overlap,
    TooShort,
    TooLong,
    HighCps,
    LongLine,
    TooManyLines,
    Empty,
    UnbalancedBraces,
    UnknownStyle,
    DoubleSpace,
    SmallGap,
}

#[derive(Clone, Debug)]
pub struct Issue {
    /// 0-based index into `script.events`.
    pub index: usize,
    pub kind: Kind,
    pub message: String,
}

pub fn check(script: &Script, cfg: &QcConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut push = |index: usize, kind: Kind, message: String| out.push(Issue { index, kind, message });
    for (i, e) in script.events.iter().enumerate() {
        if e.comment {
            continue;
        }
        let dur = e.end - e.start;
        let vis = tags::visible_text(&e.text);
        if dur <= 0 {
            push(i, Kind::NegativeDuration, format!("duration {} ms", dur));
        } else {
            if dur < cfg.min_duration_ms {
                push(i, Kind::TooShort, format!("{} ms < {} ms", dur, cfg.min_duration_ms));
            }
            if dur > cfg.max_duration_ms {
                push(i, Kind::TooLong, format!("{} ms > {} ms", dur, cfg.max_duration_ms));
            }
            let cps = text::reading_chars(&e.text) as f64 / (dur as f64 / 1000.0);
            if cps > cfg.max_cps {
                push(i, Kind::HighCps, format!("{:.1} CPS > {:.1}", cps, cfg.max_cps));
            }
        }
        if vis.trim().is_empty() && e.effect.is_empty() && !e.text.contains("\\p1") && !e.text.contains("\\p2") {
            push(i, Kind::Empty, "no visible text".into());
        }
        let ll = text::longest_line(&e.text);
        if ll > cfg.max_line_len {
            push(i, Kind::LongLine, format!("line of {} chars > {}", ll, cfg.max_line_len));
        }
        let lines = vis.lines().count();
        if lines > cfg.max_lines {
            push(i, Kind::TooManyLines, format!("{} lines > {}", lines, cfg.max_lines));
        }
        if tags::has_unbalanced_braces(&e.text) {
            push(i, Kind::UnbalancedBraces, "unbalanced { }".into());
        }
        if script.style(&e.style).is_none() {
            push(i, Kind::UnknownStyle, format!("style '{}' not defined", e.style));
        }
        if vis.contains("  ") {
            push(i, Kind::DoubleSpace, "double space".into());
        }
    }
    // overlaps / gaps are computed per style-agnostic timeline over non-comment events sorted by start
    let mut idx: Vec<usize> = (0..script.events.len()).filter(|&i| !script.events[i].comment).collect();
    idx.sort_by_key(|&i| (script.events[i].start, i));
    for w in idx.windows(2) {
        let (a, b) = (&script.events[w[0]], &script.events[w[1]]);
        if a.layer == b.layer && a.style == b.style {
            if b.start < a.end {
                push(w[1], Kind::Overlap, format!("overlaps previous by {} ms", a.end - b.start));
            } else if cfg.min_gap_ms > 0 && b.start - a.end < cfg.min_gap_ms {
                push(w[1], Kind::SmallGap, format!("gap {} ms < {} ms", b.start - a.end, cfg.min_gap_ms));
            }
        }
    }
    out.sort_by_key(|i| i.index);
    out
}

/// Fix overlapping lines (same layer+style) by trimming the earlier line's end. Returns number of fixes.
pub fn fix_overlaps(script: &mut Script, min_gap_ms: i64) -> usize {
    let mut idx: Vec<usize> = (0..script.events.len()).filter(|&i| !script.events[i].comment).collect();
    idx.sort_by_key(|&i| (script.events[i].start, i));
    let mut fixed = 0;
    for w in idx.windows(2) {
        let (ai, bi) = (w[0], w[1]);
        let (a_layer, a_style, a_end, a_start) = {
            let a = &script.events[ai];
            (a.layer, a.style.clone(), a.end, a.start)
        };
        let b = &script.events[bi];
        if a_layer == b.layer && a_style == b.style && b.start < a_end + min_gap_ms.max(0) {
            let new_end = (b.start - min_gap_ms).max(a_start + 1);
            if new_end != a_end {
                script.events[ai].end = new_end;
                fixed += 1;
            }
        }
    }
    fixed
}

/// Extend too-short lines up to `min_ms` without running into the next line.
pub fn fix_min_duration(script: &mut Script, min_ms: i64) -> usize {
    let mut idx: Vec<usize> = (0..script.events.len()).filter(|&i| !script.events[i].comment).collect();
    idx.sort_by_key(|&i| (script.events[i].start, i));
    let mut fixed = 0;
    for (n, &i) in idx.iter().enumerate() {
        let e = &script.events[i];
        if e.end - e.start >= min_ms {
            continue;
        }
        let limit = idx[n + 1..].iter().map(|&j| &script.events[j]).find(|o| o.style == e.style && o.layer == e.layer).map(|o| o.start);
        let want = e.start + min_ms;
        let new_end = limit.map_or(want, |l| want.min(l));
        if new_end > e.end {
            script.events[i].end = new_end;
            fixed += 1;
        }
    }
    fixed
}

pub fn fix_double_spaces(script: &mut Script) -> usize {
    let re = regex::Regex::new(r" {2,}").unwrap();
    let mut n = 0;
    for e in &mut script.events {
        let t = re.replace_all(&e.text, " ").trim().to_string();
        if t != e.text {
            e.text = t;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;

    fn ev(s: i64, e: i64, t: &str) -> Event {
        Event { start: s, end: e, text: t.into(), ..Event::default() }
    }

    #[test]
    fn detects_and_fixes() {
        let mut s = Script::new();
        s.events = vec![ev(0, 2000, "hello"), ev(1500, 3000, "overlap"), ev(3000, 3200, "short  text")];
        let issues = check(&s, &QcConfig::default());
        let kinds: Vec<Kind> = issues.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&Kind::Overlap));
        assert!(kinds.contains(&Kind::TooShort));
        assert!(kinds.contains(&Kind::DoubleSpace));
        assert_eq!(fix_overlaps(&mut s, 0), 1);
        assert_eq!(s.events[0].end, 1500);
        assert_eq!(fix_min_duration(&mut s, 700), 1);
        assert_eq!(s.events[2].end, 3700);
        assert_eq!(fix_double_spaces(&mut s), 1);
        assert!(check(&s, &QcConfig::default()).iter().all(|i| i.kind != Kind::Overlap));
    }
}
