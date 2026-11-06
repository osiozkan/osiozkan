//! Altyazı kalite denetimleri (sorun listesi).
use crate::ass::Document;
use crate::tags;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    NonPositiveDuration,
    Overlap,
    TooFast,       // CPS yüksek
    TooShort,
    TooLong,
    LineTooLong,
    TooManyLines,
    UnknownStyle,
    EmptyText,
    DoubleSpace,
    UnbalancedBraces,
}

#[derive(Debug, Clone)]
pub struct Issue {
    /// events indeksi (0 tabanlı)
    pub index: usize,
    pub kind: Kind,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Rules {
    pub max_cps: f64,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_line_chars: usize,
    pub max_lines: usize,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { max_cps: 20.0, min_duration_ms: 500, max_duration_ms: 7000, max_line_chars: 42, max_lines: 2 }
    }
}

pub fn run(doc: &Document, r: &Rules) -> Vec<Issue> {
    let mut out = vec![];
    let mut push = |index, kind, message: String| out.push(Issue { index, kind, message });
    // Çakışma için yalnızca diyalog olaylarını başlangıca göre sıralı tara
    let mut order: Vec<usize> = (0..doc.events.len()).filter(|&i| !doc.events[i].is_comment()).collect();
    order.sort_by_key(|&i| doc.events[i].start);
    for w in order.windows(2) {
        let (a, b) = (&doc.events[w[0]], &doc.events[w[1]]);
        if b.start < a.end && a.layer == b.layer && a.style == b.style {
            push(w[1], Kind::Overlap, format!("Önceki satırla {} ms çakışıyor", a.end - b.start));
        }
    }
    for (i, e) in doc.events.iter().enumerate() {
        if e.is_comment() { continue; }
        if e.duration() <= 0 {
            push(i, Kind::NonPositiveDuration, "Süre sıfır ya da negatif".into());
        }
        if doc.style(&e.style).is_none() {
            push(i, Kind::UnknownStyle, format!("Tanımsız stil: {}", e.style));
        }
        let vis = tags::visible_chars(&e.text);
        if vis == 0 && !e.text.contains("\\p1") {
            push(i, Kind::EmptyText, "Metin boş".into());
            continue;
        }
        let dur = e.duration();
        if dur > 0 {
            let cps = vis as f64 / (dur as f64 / 1000.0);
            if cps > r.max_cps && e.effect.is_empty() {
                push(i, Kind::TooFast, format!("Okuma hızı {:.1} karakter/sn (sınır {})", cps, r.max_cps));
            }
            if dur < r.min_duration_ms { push(i, Kind::TooShort, format!("Süre {} ms", dur)); }
            if dur > r.max_duration_ms { push(i, Kind::TooLong, format!("Süre {} ms", dur)); }
        }
        if tags::max_line_len(&e.text) > r.max_line_chars {
            push(i, Kind::LineTooLong, format!("Satır {} karakteri aşıyor", r.max_line_chars));
        }
        if tags::line_count(&e.text) > r.max_lines {
            push(i, Kind::TooManyLines, format!("{} satırdan fazla", r.max_lines));
        }
        if tags::plain(&e.text).contains("  ") {
            push(i, Kind::DoubleSpace, "Çift boşluk".into());
        }
        let (o, c) = (e.text.matches('{').count(), e.text.matches('}').count());
        if o != c {
            push(i, Kind::UnbalancedBraces, "Eşleşmeyen süslü parantez".into());
        }
    }
    out.sort_by_key(|i| i.index);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;
    #[test]
    fn finds_issues() {
        let mut d = Document::new();
        d.events.push(Event { start: 0, end: 1000, text: "a".repeat(60), ..Event::default() });
        d.events.push(Event { start: 500, end: 600, text: "x".into(), ..Event::default() });
        d.events.push(Event { start: 9000, end: 10000, text: "y".into(), style: "Yok".into(), ..Event::default() });
        let kinds: Vec<_> = run(&d, &Rules::default()).into_iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&Kind::TooFast));
        assert!(kinds.contains(&Kind::LineTooLong));
        assert!(kinds.contains(&Kind::UnknownStyle));
        assert!(kinds.contains(&Kind::Overlap));
        assert!(kinds.contains(&Kind::TooShort));
    }
}
