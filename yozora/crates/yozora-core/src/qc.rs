//! Kalite kontrol: anime/TV altyazısı için yaygın hataları tarar.
use crate::ass::Document;
use crate::tags;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct QcConfig {
    /// Saniyedeki en yüksek karakter sayısı (boşluksuz).
    pub max_cps: f64,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_line_chars: usize,
    pub max_lines: usize,
    /// Aynı katman/stildeki ardışık satırlar arasındaki en küçük boşluk (0 = kapalı).
    pub min_gap_ms: i64,
}

impl Default for QcConfig {
    fn default() -> Self {
        QcConfig { max_cps: 20.0, min_duration_ms: 500, max_duration_ms: 8000, max_line_chars: 42, max_lines: 2, min_gap_ms: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    /// 1 tabanlı olay indeksi.
    pub line: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

pub fn check(doc: &Document, cfg: &QcConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut push = |line: usize, severity, code, message: String| out.push(Issue { line, severity, code, message });

    for (i, e) in doc.events.iter().enumerate() {
        if e.comment {
            continue;
        }
        let n = i + 1;
        let dur = e.end - e.start;
        if dur < 0 {
            push(n, Severity::Error, "negative-duration", format!("bitiş başlangıçtan önce ({} ms)", dur));
            continue;
        }
        if doc.style(&e.style).is_none() {
            push(n, Severity::Error, "missing-style", format!("'{}' stili tanımlı değil", e.style));
        }
        let vis = tags::visible_text(&e.text);
        let has_drawing = tags::parse_blocks(&e.text).iter().any(|b| match b {
            tags::Block::Override(t) => t.iter().any(|x| x.name == "p" && x.arg.trim() != "0"),
            _ => false,
        });
        if vis.trim().is_empty() {
            if !has_drawing && e.effect.is_empty() {
                push(n, Severity::Warning, "empty-text", "boş metin".into());
            }
            continue;
        }
        if dur < cfg.min_duration_ms {
            push(n, Severity::Warning, "too-short", format!("süre çok kısa ({} ms)", dur));
        }
        if dur > cfg.max_duration_ms {
            push(n, Severity::Warning, "too-long", format!("süre çok uzun ({} ms)", dur));
        }
        if dur > 0 {
            let cps = tags::char_count(&e.text) as f64 / (dur as f64 / 1000.0);
            if cps > cfg.max_cps {
                push(n, Severity::Warning, "high-cps", format!("okuma hızı yüksek: {:.1} kar/sn (sınır {})", cps, cfg.max_cps));
            }
        }
        let lens = tags::line_lengths(&e.text);
        if lens.len() > cfg.max_lines {
            push(n, Severity::Warning, "too-many-lines", format!("{} satır (sınır {})", lens.len(), cfg.max_lines));
        }
        if let Some(m) = lens.iter().max() {
            if *m > cfg.max_line_chars {
                push(n, Severity::Warning, "long-line", format!("satır {} karakter (sınır {})", m, cfg.max_line_chars));
            }
        }
        if vis.contains("  ") {
            push(n, Severity::Info, "double-space", "çift boşluk".into());
        }
        if vis != vis.trim_matches(|c: char| c == ' ') && !vis.starts_with('\n') {
            push(n, Severity::Info, "edge-space", "baş/son boşluk".into());
        }
        if vis.contains("...") {
            push(n, Severity::Info, "ascii-ellipsis", "'...' yerine '…' kullanın".into());
        }
        if vis.contains('"') {
            push(n, Severity::Info, "straight-quote", "düz tırnak; tipografik tırnak önerilir".into());
        }
    }

    // Örtüşme / boşluk: aynı katman ve stil içinde, başlangıca göre sıralı.
    let mut idx: Vec<usize> = (0..doc.events.len()).filter(|&i| !doc.events[i].comment).collect();
    idx.sort_by_key(|&i| (doc.events[i].start, doc.events[i].end));
    let mut last_by_key: std::collections::HashMap<(i32, String), usize> = Default::default();
    for &i in &idx {
        let e = &doc.events[i];
        if tags::visible_text(&e.text).trim().is_empty() {
            continue;
        }
        let key = (e.layer, e.style.clone());
        if let Some(&p) = last_by_key.get(&key) {
            let pe = &doc.events[p];
            if e.start < pe.end {
                // Konumlu (\pos/\move/\an) satırlar kasıtlı örtüşebilir.
                let positioned = |t: &str| t.contains("\\pos") || t.contains("\\move") || t.contains("\\an");
                if !positioned(&e.text) && !positioned(&pe.text) {
                    out.push(Issue {
                        line: i + 1,
                        severity: Severity::Warning,
                        code: "overlap",
                        message: format!("{}. satırla örtüşüyor ({} ms)", p + 1, pe.end - e.start),
                    });
                }
            } else if cfg.min_gap_ms > 0 && e.start - pe.end < cfg.min_gap_ms && e.start != pe.end {
                out.push(Issue {
                    line: i + 1,
                    severity: Severity::Info,
                    code: "small-gap",
                    message: format!("{}. satırla aradaki boşluk {} ms", p + 1, e.start - pe.end),
                });
            }
        }
        last_by_key.insert(key, i);
    }
    out.sort_by_key(|i| i.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;

    fn ev(s: i64, e: i64, t: &str) -> Event {
        Event { start: s, end: e, text: t.into(), ..Event::default() }
    }

    #[test]
    fn detects_problems() {
        let mut d = Document::new_default();
        d.events.push(ev(0, 1000, "Normal satır"));
        d.events.push(ev(500, 700, "Çok hızlı okunan uzun bir cümle burada"));
        d.events.push(ev(5000, 4000, "negatif"));
        d.events.push(Event { style: "Yok".into(), ..ev(6000, 8000, "stil") });
        let issues = check(&d, &QcConfig::default());
        let codes: Vec<_> = issues.iter().map(|i| i.code).collect();
        assert!(codes.contains(&"overlap"));
        assert!(codes.contains(&"high-cps"));
        assert!(codes.contains(&"too-short"));
        assert!(codes.contains(&"negative-duration"));
        assert!(codes.contains(&"missing-style"));
    }

    #[test]
    fn positioned_overlap_ok() {
        let mut d = Document::new_default();
        d.events.push(ev(0, 2000, "{\\an8}üst"));
        d.events.push(ev(500, 2500, "alt"));
        assert!(!check(&d, &QcConfig::default()).iter().any(|i| i.code == "overlap"));
    }
}
