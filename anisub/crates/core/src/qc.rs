//! Quality-control checks (reading speed, line length, overlaps, ...).

use crate::ass::AssFile;
use crate::tags::plain_text;

#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub max_cps: f64,
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { max_cps: 20.0, max_line_chars: 42, max_lines: 2, min_duration_ms: 500, max_duration_ms: 7000 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// 1-based index among dialogue events (file order).
    pub event: usize,
    pub kind: &'static str,
    pub detail: String,
}

pub fn cps(text: &str, duration_ms: i64) -> f64 {
    if duration_ms <= 0 {
        return f64::INFINITY;
    }
    let chars = plain_text(text).chars().filter(|c| *c != '\n').count();
    chars as f64 * 1000.0 / duration_ms as f64
}

pub fn check(file: &AssFile, r: &Rules) -> Vec<Issue> {
    let mut issues = Vec::new();
    let evs: Vec<_> = file.events().enumerate().filter(|(_, e)| !e.comment).collect();
    for (i, e) in &evs {
        let n = i + 1;
        let d = e.duration();
        if d <= 0 {
            issues.push(Issue { event: n, kind: "duration", detail: "zero or negative duration".into() });
        } else if d < r.min_duration_ms {
            issues.push(Issue { event: n, kind: "duration", detail: format!("too short ({d} ms)") });
        } else if d > r.max_duration_ms {
            issues.push(Issue { event: n, kind: "duration", detail: format!("too long ({d} ms)") });
        }
        let c = cps(&e.text, d);
        if c > r.max_cps && !e.text.is_empty() {
            issues.push(Issue { event: n, kind: "cps", detail: format!("{c:.1} chars/s") });
        }
        let pt = plain_text(&e.text);
        let lines: Vec<&str> = pt.split('\n').collect();
        if lines.len() > r.max_lines {
            issues.push(Issue { event: n, kind: "lines", detail: format!("{} lines", lines.len()) });
        }
        for l in lines {
            let len = l.chars().count();
            if len > r.max_line_chars {
                issues.push(Issue { event: n, kind: "length", detail: format!("line of {len} chars") });
            }
        }
        if e.text.trim().is_empty() {
            issues.push(Issue { event: n, kind: "empty", detail: "empty text".into() });
        }
    }
    let mut sorted = evs.clone();
    sorted.sort_by_key(|(_, e)| (e.start, e.end));
    for w in sorted.windows(2) {
        let (ia, a) = w[0];
        let (ib, b) = w[1];
        if b.start < a.end && a.layer == b.layer && a.style == b.style {
            issues.push(Issue {
                event: ib + 1,
                kind: "overlap",
                detail: format!("overlaps event {} by {} ms", ia + 1, a.end - b.start),
            });
        }
    }
    issues.sort_by_key(|i| i.event);
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;
    use crate::ops::new_event;
    #[test]
    fn detects() {
        let mut f = AssFile::new_default();
        f.lines.push(Line::Dialogue(new_event(0, 1000, "{\\i1}çok uzun bir satır metni burada yazıyor işte tamam")));
        f.lines.push(Line::Dialogue(new_event(500, 2000, "x\\Ny\\Nz")));
        let kinds: Vec<_> = check(&f, &Rules::default()).into_iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&"cps"));
        assert!(kinds.contains(&"length"));
        assert!(kinds.contains(&"lines"));
        assert!(kinds.contains(&"overlap"));
        assert!((cps("abcd", 2000) - 2.0).abs() < 1e-9);
    }
}
