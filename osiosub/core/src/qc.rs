//! Quality-control checks (timing, readability, typography, styles).
use crate::ass::AssFile;
use crate::tags;

#[derive(Clone, Debug)]
pub struct QcConfig {
    pub max_cps: f64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_dur_ms: i64,
    pub max_dur_ms: i64,
    pub min_gap_ms: i64,
}
impl Default for QcConfig {
    fn default() -> Self {
        Self { max_cps: 17.0, max_line_len: 42, max_lines: 2, min_dur_ms: 833, max_dur_ms: 7000, min_gap_ms: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity { Info, Warning, Error }

#[derive(Clone, Debug)]
pub struct Issue {
    pub line: usize, // 0-based event index
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

pub fn cps(text: &str, dur_ms: i64) -> f64 {
    if dur_ms <= 0 { return f64::INFINITY; }
    tags::visible_len(text) as f64 * 1000.0 / dur_ms as f64
}

pub fn check(f: &AssFile, c: &QcConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut push = |line, severity, code, message: String| out.push(Issue { line, severity, code, message });
    let mut order: Vec<usize> = (0..f.events.len()).filter(|&i| !f.events[i].comment).collect();
    order.sort_by_key(|&i| (f.events[i].start, f.events[i].end));
    for (k, &i) in order.iter().enumerate() {
        let e = &f.events[i];
        let dur = e.end - e.start;
        let vis = tags::visible_text(&e.text);
        if e.end <= e.start { push(i, Severity::Error, "neg-dur", "bitiş başlangıçtan önce / süre sıfır".into()); }
        if vis.trim().is_empty() { push(i, Severity::Warning, "empty", "boş satır".into()); continue; }
        if dur > 0 && dur < c.min_dur_ms { push(i, Severity::Warning, "short", format!("süre çok kısa ({} ms)", dur)); }
        if dur > c.max_dur_ms { push(i, Severity::Warning, "long", format!("süre çok uzun ({} ms)", dur)); }
        let r = cps(&e.text, dur);
        if r > c.max_cps { push(i, Severity::Warning, "cps", format!("okuma hızı {:.1} CPS (> {})", r, c.max_cps)); }
        let lens = tags::line_lengths(&e.text);
        if lens.len() > c.max_lines { push(i, Severity::Warning, "lines", format!("{} satır (> {})", lens.len(), c.max_lines)); }
        if let Some(m) = lens.iter().max() { if *m > c.max_line_len { push(i, Severity::Warning, "len", format!("satır {} karakter (> {})", m, c.max_line_len)); } }
        if vis.contains("  ") { push(i, Severity::Info, "dblspace", "çift boşluk".into()); }
        if vis.lines().any(|l| l != l.trim()) { push(i, Severity::Info, "trim", "satır başı/sonunda boşluk".into()); }
        if [" ,", " .", " !", " ?", " ;", " :"].iter().any(|p| vis.contains(p)) { push(i, Severity::Info, "space-punct", "noktalama öncesi boşluk".into()); }
        if vis.contains("...") { push(i, Severity::Info, "ellipsis", "'...' yerine '…' kullanılabilir".into()); }
        if f.style(&e.style).is_none() { push(i, Severity::Error, "style", format!("tanımsız stil: {}", e.style)); }
        if let Some(&j) = order.get(k + 1) {
            let n = &f.events[j];
            if n.start < e.end && n.layer == e.layer && n.style == e.style && e.effect.is_empty() && n.effect.is_empty() {
                push(i, Severity::Warning, "overlap", format!("sonraki satırla çakışıyor ({} ms)", e.end - n.start));
            } else if n.start >= e.end && n.start - e.end < c.min_gap_ms {
                push(i, Severity::Info, "gap", format!("satırlar arası boşluk {} ms", n.start - e.end));
            }
        }
    }
    out.sort_by_key(|i| i.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;
    #[test]
    fn finds() {
        let mut f = AssFile::new_default();
        f.events.push(Event { start: 0, end: 500, text: "çok  uzun bir cümle yazıyorum burada...".into(), ..Default::default() });
        f.events.push(Event { start: 400, end: 3000, text: "ok".into(), style: "Yok".into(), ..Default::default() });
        let codes: Vec<_> = check(&f, &QcConfig::default()).iter().map(|i| i.code).collect();
        for c in ["short", "cps", "dblspace", "ellipsis", "style"] { assert!(codes.contains(&c), "{c} in {codes:?}"); }
    }
}
