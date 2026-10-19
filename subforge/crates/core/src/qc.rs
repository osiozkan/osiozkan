//! Quality control: the checks translators/timers/QCers run by hand, automated.
use crate::ass::{Event, Script};
use crate::tags::{self, visible_text};
use crate::time::Ms;
use crate::turkish;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QcConfig {
    pub min_duration_ms: Ms,
    pub max_duration_ms: Ms,
    pub max_cps: f64,
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub min_gap_ms: Ms,
    pub check_overlaps: bool,
    pub check_styles: bool,
    pub check_mojibake: bool,
    pub check_whitespace: bool,
    pub check_tags: bool,
    pub check_empty: bool,
}

impl Default for QcConfig {
    fn default() -> Self {
        QcConfig {
            min_duration_ms: 500,
            max_duration_ms: 7000,
            max_cps: 21.0,
            max_line_chars: 42,
            max_lines: 2,
            min_gap_ms: 0,
            check_overlaps: true,
            check_styles: true,
            check_mojibake: true,
            check_whitespace: true,
            check_tags: true,
            check_empty: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Overlap,
    TooShort,
    TooLong,
    HighCps,
    LongLine,
    TooManyLines,
    SmallGap,
    UnknownStyle,
    Mojibake,
    DoubleSpace,
    EdgeSpace,
    BadBraces,
    EmptyText,
    DuplicateEvent,
    NegativeDuration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub event: usize,
    pub kind: Kind,
    pub severity: Severity,
    pub message: String,
    pub fixable: bool,
}

/// Cheap check for "shows something": any char outside `{}` blocks that isn't whitespace/escape.
fn has_visible(text: &str) -> bool {
    !visible_text(text).trim().is_empty()
}

fn is_dialogue_like(e: &Event) -> bool {
    !e.is_comment() && !e.effect.to_ascii_lowercase().starts_with("karaoke") && !e.effect.eq_ignore_ascii_case("fx")
}

/// Run all enabled checks.
pub fn check(script: &Script, cfg: &QcConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let style_names: HashSet<&str> = script.styles.iter().map(|s| s.name.as_str()).collect();
    let mut push = |event: usize, kind: Kind, severity: Severity, message: String, fixable: bool| {
        out.push(Issue { event, kind, severity, message, fixable });
    };
    for (i, e) in script.events.iter().enumerate() {
        if e.is_comment() {
            continue;
        }
        if e.end < e.start {
            push(i, Kind::NegativeDuration, Severity::Error, "end time is before start time".into(), true);
        }
        let dur = e.end - e.start;
        let vis = visible_text(&e.text);
        let has_text = !vis.trim().is_empty();
        let vis_len = vis.chars().filter(|c| !c.is_whitespace()).count();
        let drawing = e.text.contains("\\p1");
        if cfg.check_empty && !has_text && !drawing && e.text.trim().is_empty() {
            push(i, Kind::EmptyText, Severity::Warning, "empty text".into(), true);
        }
        if cfg.check_styles && !style_names.contains(e.style.as_str()) {
            push(i, Kind::UnknownStyle, Severity::Error, format!("style '{}' is not defined", e.style), false);
        }
        if cfg.check_tags {
            if let Some(m) = tags::check_braces(&e.text) {
                push(i, Kind::BadBraces, Severity::Error, m.into(), false);
            }
        }
        if cfg.check_mojibake && turkish::looks_like_mojibake(&e.text) {
            push(i, Kind::Mojibake, Severity::Error, "text looks like broken UTF-8 (mojibake)".into(), turkish::fix_mojibake(&e.text).is_some());
        }
        if !is_dialogue_like(e) || !has_text {
            continue;
        }
        if dur >= 0 && dur < cfg.min_duration_ms {
            push(i, Kind::TooShort, Severity::Warning, format!("duration {dur} ms < {} ms", cfg.min_duration_ms), false);
        }
        if dur > cfg.max_duration_ms {
            push(i, Kind::TooLong, Severity::Info, format!("duration {dur} ms > {} ms", cfg.max_duration_ms), false);
        }
        if dur > 0 {
            let cps = vis_len as f64 / (dur as f64 / 1000.0);
            if cps > cfg.max_cps {
                push(i, Kind::HighCps, Severity::Warning, format!("{cps:.1} chars/sec > {:.0}", cfg.max_cps), false);
            }
        }
        let ls: Vec<&str> = vis.split('\n').collect();
        if ls.len() > cfg.max_lines {
            push(i, Kind::TooManyLines, Severity::Warning, format!("{} lines > {}", ls.len(), cfg.max_lines), false);
        }
        if let Some(l) = ls.iter().map(|l| l.chars().count()).max() {
            if l > cfg.max_line_chars {
                push(i, Kind::LongLine, Severity::Warning, format!("line of {l} chars > {}", cfg.max_line_chars), false);
            }
        }
        if cfg.check_whitespace {
            if e.text.contains("  ") && !e.text.contains("{") {
                push(i, Kind::DoubleSpace, Severity::Info, "double space".into(), true);
            }
            let trimmed = e.text.trim();
            if trimmed.len() != e.text.len() || e.text.contains(" \\N") || e.text.contains("\\N ") {
                push(i, Kind::EdgeSpace, Severity::Info, "leading/trailing space around text or line break".into(), true);
            }
        }
    }
    // cross-event checks on dialogue
    if cfg.check_overlaps || cfg.min_gap_ms > 0 {
        let mut order: Vec<usize> = (0..script.events.len()).filter(|&i| is_dialogue_like(&script.events[i]) && has_visible(&script.events[i].text)).collect();
        order.sort_by_key(|&i| (script.events[i].start, script.events[i].end));
        let mut groups: std::collections::BTreeMap<(String, i32), Vec<usize>> = Default::default();
        for &i in &order {
            groups.entry((script.events[i].style.clone(), script.events[i].layer)).or_default().push(i);
        }
        for g in groups.values() {
            let mut max_end: Option<(usize, Ms)> = None;
            for &i in g {
                let b = &script.events[i];
                if let Some((_, end)) = max_end {
                    if cfg.check_overlaps && b.start < end {
                        out.push(Issue { event: i, kind: Kind::Overlap, severity: Severity::Warning, message: format!("overlaps previous line by {} ms", end - b.start), fixable: true });
                    } else if cfg.min_gap_ms > 0 && b.start - end < cfg.min_gap_ms {
                        out.push(Issue { event: i, kind: Kind::SmallGap, severity: Severity::Info, message: format!("gap {} ms < {} ms", b.start - end, cfg.min_gap_ms), fixable: true });
                    }
                }
                if max_end.map(|(_, e)| b.end > e).unwrap_or(true) {
                    max_end = Some((i, b.end));
                }
            }
        }
        // exact duplicates
        let mut seen = HashSet::new();
        for &i in &order {
            let e = &script.events[i];
            if !seen.insert((e.start, e.end, e.style.clone(), e.text.clone())) {
                out.push(Issue { event: i, kind: Kind::DuplicateEvent, severity: Severity::Warning, message: "duplicate of another line".into(), fixable: true });
            }
        }
    }
    out.sort_by_key(|i| (i.event, i.kind as u8));
    out
}

/// Apply automatic fixes; returns number of changes.
pub fn autofix(script: &mut Script, cfg: &QcConfig) -> usize {
    let mut n = 0;
    for e in script.events.iter_mut() {
        if e.is_comment() {
            continue;
        }
        if cfg.check_mojibake {
            if let Some(f) = turkish::fix_mojibake(&e.text) {
                e.text = f;
                n += 1;
            }
        }
        if e.end < e.start {
            std::mem::swap(&mut e.start, &mut e.end);
            n += 1;
        }
        if cfg.check_whitespace {
            let before = e.text.clone();
            let mut t = e.text.trim().to_string();
            while t.contains("  ") && !t.contains('{') {
                t = t.replace("  ", " ");
            }
            t = t.replace(" \\N", "\\N").replace("\\N ", "\\N");
            if t != before {
                e.text = t;
                n += 1;
            }
        }
    }
    if cfg.check_overlaps {
        n += crate::timing::fix_overlaps(&mut script.events);
    }
    // drop empty + exact duplicate dialogue lines
    let mut seen = HashSet::new();
    let before = script.events.len();
    script.events.retain(|e| {
        if e.is_comment() {
            return true;
        }
        if cfg.check_empty && e.text.trim().is_empty() {
            return false;
        }
        seen.insert((e.start, e.end, e.style.clone(), e.layer, e.text.clone()))
    });
    n += before - script.events.len();
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;

    fn script(evs: Vec<Event>) -> Script {
        let mut s = Script::new();
        s.events = evs;
        s
    }

    #[test]
    fn finds_issues() {
        let s = script(vec![
            Event::new(0, 100, "Default", "çok kısa"),
            Event::new(1000, 2000, "Default", "Ã§ok"),
            Event::new(1500, 3000, "Nope", "a  b"),
            Event::new(1800, 2600, "Default", "çakışma"),
            Event::new(5000, 5500, "Default", "Bu satır çok fazla karakter içeriyor ve okunamaz derecede hızlı"),
        ]);
        let issues = check(&s, &QcConfig::default());
        let kinds: HashSet<Kind> = issues.iter().map(|i| i.kind).collect();
        for k in [Kind::TooShort, Kind::Mojibake, Kind::UnknownStyle, Kind::Overlap, Kind::DoubleSpace, Kind::HighCps, Kind::LongLine] {
            assert!(kinds.contains(&k), "missing {k:?}");
        }
    }

    #[test]
    fn fixes() {
        let mut s = script(vec![Event::new(0, 2000, "Default", " Ã§ok  gÃ¼zel "), Event::new(1000, 3000, "Default", "x")]);
        let n = autofix(&mut s, &QcConfig::default());
        assert!(n >= 3);
        assert_eq!(s.events[0].text, "çok güzel");
        assert_eq!(s.events[0].end, 1000);
    }
}
