//! Kalite kontrol: altyazıda sık karşılaşılan hataları yakalar.
use crate::model::Document;
use crate::tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Issue {
    /// `doc.events` içindeki sıfır tabanlı indeks.
    pub event: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub max_cps: f64,
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub min_duration_ms: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config { max_cps: 20.0, max_line_chars: 42, max_lines: 2, min_duration_ms: 500 }
    }
}

pub fn check(doc: &Document, cfg: &Config) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut add = |event, severity, code, message: String| out.push(Issue { event, severity, code, message });

    for (i, e) in doc.events.iter().enumerate() {
        if e.comment {
            continue;
        }
        let dur = e.duration();
        if dur <= 0 {
            add(i, Severity::Error, "duration", format!("Süre sıfır veya negatif ({} ms)", dur));
            continue;
        }
        if !tags::balanced_braces(&e.text) {
            add(i, Severity::Error, "braces", "Dengesiz { } — etiket bloğu kapanmamış".into());
        }
        if doc.style(&e.style).is_none() {
            add(i, Severity::Error, "style", format!("Tanımsız stil: {}", e.style));
        }
        let vis = tags::visible_text(&e.text);
        if vis.trim().is_empty() {
            add(i, Severity::Warning, "empty", "Görünür metin yok".into());
            continue;
        }
        if dur < cfg.min_duration_ms {
            add(i, Severity::Warning, "short", format!("Çok kısa ({} ms)", dur));
        }
        let cps = tags::char_count(&e.text) as f64 * 1000.0 / dur as f64;
        if cps > cfg.max_cps {
            add(i, Severity::Warning, "cps", format!("Okuma hızı {:.1} kar/sn (sınır {})", cps, cfg.max_cps));
        }
        let lines: Vec<&str> = vis.lines().collect();
        if lines.len() > cfg.max_lines {
            add(i, Severity::Warning, "lines", format!("{} satır (sınır {})", lines.len(), cfg.max_lines));
        }
        if let Some(l) = lines.iter().map(|l| l.trim().chars().count()).max() {
            if l > cfg.max_line_chars {
                add(i, Severity::Warning, "width", format!("Satır {} karakter (sınır {})", l, cfg.max_line_chars));
            }
        }
        if vis.contains("  ") {
            add(i, Severity::Info, "space", "Çift boşluk".into());
        }
        if e.text.starts_with("\\N") || e.text.ends_with("\\N") || vis.starts_with(' ') || vis.ends_with(' ') {
            add(i, Severity::Info, "trim", "Baş/son boşluk veya satır sonu".into());
        }
    }

    // Çakışma: aynı katmandaki satırlar (zaman sırasıyla).
    let mut idx: Vec<usize> = (0..doc.events.len()).filter(|&i| !doc.events[i].comment).collect();
    idx.sort_by_key(|&i| (doc.events[i].start, doc.events[i].end));
    for w in idx.windows(2) {
        let (a, b) = (&doc.events[w[0]], &doc.events[w[1]]);
        if a.layer == b.layer && a.end > b.start && b.start >= a.start {
            out.push(Issue {
                event: w[1],
                severity: Severity::Warning,
                code: "overlap",
                message: format!("Önceki satırla {} ms çakışıyor", a.end - b.start),
            });
        }
    }
    out.sort_by_key(|i| (i.event, std::cmp::Reverse(i.severity)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Event;
    #[test]
    fn finds_problems() {
        let mut d = Document::new();
        d.events.push(Event { start: 0, end: 1000, text: "{\\i1abc".into(), ..Event::default() });
        d.events.push(Event { start: 500, end: 600, text: "x".repeat(60), style: "Yok".into(), ..Event::default() });
        let codes: Vec<_> = check(&d, &Config::default()).iter().map(|i| i.code).collect();
        for c in ["braces", "short", "cps", "width", "style", "overlap"] {
            assert!(codes.contains(&c), "{c} yok: {codes:?}");
        }
    }
}
