//! Subtitle quality checks (timing, reading speed, line length ...).

use crate::ass::Script;
use crate::tags::{cps_chars, has_tag, visible_lines};

#[derive(Clone, Debug)]
pub struct CheckConfig {
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_cps: f64,
    pub max_chars_per_line: usize,
    pub max_lines: usize,
    pub min_gap_ms: i64,
}

impl Default for CheckConfig {
    fn default() -> Self {
        CheckConfig { min_duration_ms: 500, max_duration_ms: 8000, max_cps: 20.0, max_chars_per_line: 45, max_lines: 2, min_gap_ms: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NegativeDuration,
    ShortDuration,
    LongDuration,
    HighCps,
    LongLine,
    TooManyLines,
    EmptyText,
    UnknownStyle,
    Overlap,
    SmallGap,
}

#[derive(Clone, Debug)]
pub struct Issue {
    /// 0-based index into `Script::entries`.
    pub entry: usize,
    pub kind: Kind,
    pub message: String,
}

pub fn run(sc: &Script, cfg: &CheckConfig) -> Vec<Issue> {
    use crate::ass::Entry;
    let mut issues = Vec::new();
    let mut add = |entry: usize, kind: Kind, message: String| issues.push(Issue { entry, kind, message });
    let styles: std::collections::HashSet<&str> = sc.styles().map(|s| s.name.as_str()).collect();

    // (entry index, start, end, style, layer) of positioned-free dialogue lines
    let mut flow: Vec<(usize, i64, i64, &str, i32)> = Vec::new();

    for (i, e) in sc.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment {
            continue;
        }
        let vis = crate::tags::visible_text(&d.text);
        if vis.trim().is_empty() {
            // Drawings / pure-tag lines are legitimate sign events.
            if !d.text.contains("\\p1") && !d.text.contains("\\p2") && !d.text.is_empty() == false {
                add(i, Kind::EmptyText, "empty text".into());
            }
            continue;
        }
        if !styles.is_empty() && !styles.contains(d.style.as_str()) {
            add(i, Kind::UnknownStyle, format!("style '{}' is not defined", d.style));
        }
        let dur = d.duration();
        if dur < 0 {
            add(i, Kind::NegativeDuration, "end is before start".into());
            continue;
        }
        if dur < cfg.min_duration_ms {
            add(i, Kind::ShortDuration, format!("{dur} ms is shorter than {} ms", cfg.min_duration_ms));
        }
        if dur > cfg.max_duration_ms {
            add(i, Kind::LongDuration, format!("{dur} ms is longer than {} ms", cfg.max_duration_ms));
        }
        if dur > 0 {
            let cps = cps_chars(&d.text) as f64 * 1000.0 / dur as f64;
            if cps > cfg.max_cps {
                add(i, Kind::HighCps, format!("{cps:.1} CPS exceeds {}", cfg.max_cps));
            }
        }
        let lines = visible_lines(&d.text);
        if lines.len() > cfg.max_lines {
            add(i, Kind::TooManyLines, format!("{} lines (max {})", lines.len(), cfg.max_lines));
        }
        if let Some(l) = lines.iter().find(|l| l.chars().count() > cfg.max_chars_per_line) {
            add(i, Kind::LongLine, format!("line of {} chars (max {})", l.chars().count(), cfg.max_chars_per_line));
        }
        let positioned = has_tag(&d.text, "pos") || has_tag(&d.text, "move") || has_tag(&d.text, "org") || !d.effect.is_empty();
        if !positioned {
            flow.push((i, d.start, d.end, d.style.as_str(), d.layer));
        }
    }

    // Overlaps / gaps among non-positioned lines sharing style+layer.
    flow.sort_by_key(|f| (f.3, f.4, f.1));
    for w in flow.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if a.3 != b.3 || a.4 != b.4 {
            continue;
        }
        if b.1 < a.2 {
            add(b.0, Kind::Overlap, format!("overlaps previous line by {} ms", a.2 - b.1));
        } else if cfg.min_gap_ms > 0 && b.1 - a.2 < cfg.min_gap_ms && b.1 != a.2 {
            add(b.0, Kind::SmallGap, format!("gap of {} ms (min {})", b.1 - a.2, cfg.min_gap_ms));
        }
    }
    issues.sort_by_key(|i| i.entry);
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects() {
        let sc = Script::parse("[Script Info]\n\n[V4+ Styles]\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nDialogue: 0,0:00:00.00,0:00:00.20,Default,,0,0,0,,Short\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,A long long long line of text that goes on and on and on forever\nDialogue: 0,0:00:02.50,0:00:04.00,Default,,0,0,0,,overlap\nDialogue: 0,0:00:05.00,0:00:06.00,Nope,,0,0,0,,x\n").unwrap();
        let kinds: Vec<Kind> = run(&sc, &CheckConfig::default()).iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&Kind::ShortDuration));
        assert!(kinds.contains(&Kind::LongLine));
        assert!(kinds.contains(&Kind::HighCps));
        assert!(kinds.contains(&Kind::Overlap));
        assert!(kinds.contains(&Kind::UnknownStyle));
    }
}
