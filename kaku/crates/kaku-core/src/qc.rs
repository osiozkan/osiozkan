//! Quality-control checks (reading speed, overlaps, lengths, style references…).

use crate::ass::{Kind, Script};
use crate::tags;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Issue {
    /// Index among `script.lines()` (0-based).
    pub line: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct QcConfig {
    pub max_cps: f64,
    pub min_duration: i64,
    pub max_duration: i64,
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub min_gap: i64,
}

impl Default for QcConfig {
    fn default() -> Self {
        QcConfig { max_cps: 20.0, min_duration: 500, max_duration: 7000, max_line_chars: 42, max_lines: 2, min_gap: 0 }
    }
}

pub fn check(script: &Script, cfg: &QcConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let styles: HashSet<&str> = script.styles().map(|s| s.name.as_str()).collect();
    let lines: Vec<_> = script.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        if l.kind != Kind::Dialogue {
            continue;
        }
        let mut push = |sev, code, msg: String| out.push(Issue { line: i, severity: sev, code, message: msg });
        if l.end < l.start {
            push(Severity::Error, "negative-duration", "End time is before start time".into());
        }
        if !styles.contains(l.style.as_str()) {
            push(Severity::Error, "missing-style", format!("Style '{}' is not defined", l.style));
        }
        if tags::has_drawing(&l.text) {
            continue;
        }
        let vis = tags::visible_text(&l.text);
        if vis.trim().is_empty() {
            if l.effect.is_empty() {
                push(Severity::Warning, "empty", "Line has no visible text".into());
            }
            continue;
        }
        let dur = l.duration();
        if dur > 0 {
            let cps = tags::visible_len(&l.text) as f64 / (dur as f64 / 1000.0);
            if cps > cfg.max_cps {
                push(Severity::Warning, "cps", format!("Reading speed {:.1} cps exceeds {:.1}", cps, cfg.max_cps));
            }
        }
        if dur >= 0 && dur < cfg.min_duration {
            push(Severity::Warning, "short", format!("Duration {} ms below {} ms", dur, cfg.min_duration));
        }
        if dur > cfg.max_duration {
            push(Severity::Warning, "long", format!("Duration {} ms above {} ms", dur, cfg.max_duration));
        }
        let rows: Vec<&str> = vis.lines().collect();
        if rows.len() > cfg.max_lines {
            push(Severity::Warning, "too-many-lines", format!("{} text lines (max {})", rows.len(), cfg.max_lines));
        }
        if let Some(w) = rows.iter().map(|r| r.chars().count()).max() {
            if w > cfg.max_line_chars {
                push(Severity::Warning, "line-length", format!("Line of {} chars exceeds {}", w, cfg.max_line_chars));
            }
        }
        if vis.contains("  ") {
            push(Severity::Info, "double-space", "Double space".into());
        }
        // Unbalanced override braces.
        let (open, close) = (l.text.matches('{').count(), l.text.matches('}').count());
        if open != close {
            push(Severity::Error, "unbalanced-braces", "Unbalanced { } in text".into());
        }
    }
    // Overlap and min-gap between same-layer dialogue lines (sorted by start).
    let mut order: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].kind == Kind::Dialogue && !tags::has_drawing(&lines[i].text)).collect();
    order.sort_by_key(|&i| lines[i].start);
    for w in order.windows(2) {
        let (a, b) = (lines[w[0]], lines[w[1]]);
        if a.layer == b.layer && a.style == b.style && a.end > b.start {
            out.push(Issue { line: w[1], severity: Severity::Warning, code: "overlap", message: format!("Overlaps previous line by {} ms", a.end - b.start) });
        } else if cfg.min_gap > 0 && a.style == b.style && b.start - a.end < cfg.min_gap && b.start >= a.end {
            out.push(Issue { line: w[1], severity: Severity::Info, code: "gap", message: format!("Gap {} ms below {} ms", b.start - a.end, cfg.min_gap) });
        }
    }
    out.sort_by_key(|i| i.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;
    #[test]
    fn detects() {
        let mut s = Script::new_default();
        s.push_line(Line::new(0, 500, "abcdefghijklmnopqrstuvwxyz"));
        s.push_line(Line::new(400, 3000, "ok"));
        let mut l = Line::new(4000, 5000, "x");
        l.style = "Nope".into();
        s.push_line(l);
        let issues = check(&s, &QcConfig::default());
        let codes: Vec<_> = issues.iter().map(|i| i.code).collect();
        assert!(codes.contains(&"cps"));
        assert!(codes.contains(&"overlap"));
        assert!(codes.contains(&"missing-style"));
    }
}
