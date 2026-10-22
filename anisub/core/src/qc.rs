//! Quality-control checks (CPS, overlaps, durations, line length, undefined styles ...).
use crate::ass::Script;
use crate::tags;

#[derive(Debug, Clone, PartialEq)]
pub enum Severity { Error, Warning }

#[derive(Debug, Clone)]
pub struct Issue {
    /// Index into `Script::lines`.
    pub line: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct QcConfig {
    pub max_cps: f64,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_line_chars: usize,
    pub max_lines: usize,
}

impl Default for QcConfig {
    fn default() -> Self {
        Self { max_cps: 20.0, min_duration_ms: 500, max_duration_ms: 7000, max_line_chars: 42, max_lines: 2 }
    }
}

pub fn check(script: &Script, cfg: &QcConfig) -> Vec<Issue> {
    use crate::ass::LineKind;
    let mut issues = Vec::new();
    let mut spans: Vec<(usize, i64, i64, i32, &str)> = Vec::new();
    for (i, l) in script.lines.iter().enumerate() {
        let LineKind::Dialogue(e) = &l.kind else { continue };
        if e.comment { continue; }
        let dur = e.end_time - e.start_time;
        let mut push = |sev, code, msg: String| issues.push(Issue { line: i, severity: sev, code, message: msg });
        if dur <= 0 { push(Severity::Error, "negative-duration", format!("duration {dur} ms")); }
        else {
            if dur < cfg.min_duration_ms { push(Severity::Warning, "short", format!("only {dur} ms")); }
            if dur > cfg.max_duration_ms { push(Severity::Warning, "long", format!("{dur} ms")); }
            let chars = tags::visible_chars(&e.text);
            let cps = chars as f64 / (dur as f64 / 1000.0);
            if cps > cfg.max_cps { push(Severity::Warning, "cps", format!("{cps:.1} chars/s")); }
        }
        let plain = tags::plain_text(&e.text);
        let rows: Vec<&str> = plain.split('\n').collect();
        if rows.len() > cfg.max_lines { push(Severity::Warning, "too-many-lines", format!("{} lines", rows.len())); }
        if let Some(r) = rows.iter().find(|r| r.chars().count() > cfg.max_line_chars) { push(Severity::Warning, "line-length", format!("{} chars", r.chars().count())); }
        if script.style(&e.style).is_none() { push(Severity::Error, "unknown-style", format!("style '{}' is not defined", e.style)); }
        if e.text.matches('{').count() != e.text.matches('}').count() { push(Severity::Error, "unbalanced-braces", "unbalanced { }".into()); }
        if plain.trim().is_empty() && e.effect.is_empty() && !e.text.contains("\\p1") { push(Severity::Warning, "empty", "empty line".into()); }
        if e.text.contains("  ") { push(Severity::Warning, "double-space", "double space".into()); }
        spans.push((i, e.start_time, e.end_time, e.layer, &e.style));
    }
    // overlaps on the same layer
    spans.sort_by_key(|s| (s.3, s.1));
    for w in spans.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a.3 == b.3 && b.1 < a.2 && b.4 == a.4 {
            issues.push(Issue { line: b.0, severity: Severity::Warning, code: "overlap", message: format!("overlaps line {} by {} ms", a.0 + 1, a.2 - b.1) });
        }
    }
    issues.sort_by_key(|i| i.line);
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_problems() {
        let s = Script::parse("[Script Info]\nPlayResX: 1\n\n[V4+ Styles]\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nDialogue: 0,0:00:00.00,0:00:00.20,Default,,0,0,0,,This is far too much text for this\nDialogue: 0,0:00:00.10,0:00:03.00,Nope,,0,0,0,,ok\n");
        let codes: Vec<_> = check(&s, &QcConfig::default()).iter().map(|i| i.code).collect();
        assert!(codes.contains(&"cps") && codes.contains(&"short") && codes.contains(&"unknown-style") && codes.contains(&"overlap") == false);
    }
}
