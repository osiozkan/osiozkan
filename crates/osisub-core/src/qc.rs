//! Kalite kontrol (QC) ve toplu düzeltme araçları.
use crate::ass::{Dialogue, Entry, Script};
use crate::tags;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Overlap,
    TooShort,
    TooLong,
    ReadingSpeed,
    LineTooLong,
    TooManyLines,
    SmallGap,
    Empty,
    NegativeDuration,
    UnknownStyle,
    DoubleSpace,
    EdgeSpace,
}

#[derive(Clone, Debug)]
pub struct Issue {
    /// `Script::entries` içindeki indeks.
    pub entry: usize,
    pub kind: Kind,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Rules {
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_cps: f64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_gap_ms: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { min_duration_ms: 833, max_duration_ms: 7000, max_cps: 20.0, max_line_len: 42, max_lines: 2, min_gap_ms: 0 }
    }
}

pub fn cps(d: &Dialogue) -> f64 {
    let dur = d.duration();
    if dur <= 0 {
        return f64::INFINITY;
    }
    tags::visible_len(&d.text) as f64 * 1000.0 / dur as f64
}

pub fn check(script: &Script, rules: &Rules) -> Vec<Issue> {
    let mut issues = Vec::new();
    let styles: Vec<&str> = script.styles().map(|s| s.name.as_str()).collect();
    let mut prev: Option<(usize, &Dialogue)> = None;
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment {
            continue;
        }
        let mut add = |kind: Kind, message: String| issues.push(Issue { entry: i, kind, message });
        let visible = tags::visible_text(&d.text);
        if visible.trim().is_empty() && !d.text.contains("\\p1") {
            add(Kind::Empty, "Boş metin".into());
        }
        if d.duration() < 0 {
            add(Kind::NegativeDuration, "Bitiş başlangıçtan önce".into());
        } else if d.duration() < rules.min_duration_ms {
            add(Kind::TooShort, format!("Süre çok kısa ({} ms)", d.duration()));
        } else if d.duration() > rules.max_duration_ms {
            add(Kind::TooLong, format!("Süre çok uzun ({} ms)", d.duration()));
        }
        let c = cps(d);
        if c > rules.max_cps && !visible.trim().is_empty() {
            add(Kind::ReadingSpeed, format!("Okuma hızı {:.1} kar/sn", c));
        }
        if tags::max_line_len(&d.text) > rules.max_line_len {
            add(Kind::LineTooLong, format!("Satır {} karakterden uzun", rules.max_line_len));
        }
        if visible.lines().count() > rules.max_lines {
            add(Kind::TooManyLines, format!("{} satırdan fazla", rules.max_lines));
        }
        if !styles.contains(&d.style.as_str()) {
            add(Kind::UnknownStyle, format!("Bilinmeyen stil: {}", d.style));
        }
        if visible.contains("  ") {
            add(Kind::DoubleSpace, "Çift boşluk".into());
        }
        if visible.lines().any(|l| l != l.trim()) {
            add(Kind::EdgeSpace, "Satır başı/sonu boşluğu".into());
        }
        if let Some((_, p)) = prev {
            if d.style == p.style && d.layer == p.layer && d.start_time < p.end_time {
                add(Kind::Overlap, format!("Önceki satırla {} ms çakışıyor", p.end_time - d.start_time));
            } else if d.start_time >= p.end_time && d.start_time - p.end_time < rules.min_gap_ms {
                add(Kind::SmallGap, format!("Boşluk {} ms", d.start_time - p.end_time));
            }
        }
        prev = Some((i, d));
    }
    issues
}

// ---------------------------------------------------------------- düzeltmeler

/// Seçili (indeks listesi) satırları kaydırır. `None` = tümü.
pub fn shift(script: &mut Script, ms: i64, only: Option<&[usize]>) {
    for (i, e) in script.entries.iter_mut().enumerate() {
        if let Entry::Dialogue(d) = e {
            if only.map_or(true, |s| s.contains(&i)) {
                d.start_time = (d.start_time + ms).max(0);
                d.end_time = (d.end_time + ms).max(0);
            }
        }
    }
}

/// Doğrusal zaman dönüşümü: iki bağlantı noktasından (kaynak→hedef) kaydırma + ölçekleme (senkron düzeltme).
pub fn linear_retime(script: &mut Script, a: (i64, i64), b: (i64, i64)) {
    if a.0 == b.0 {
        return;
    }
    let scale = (b.1 - a.1) as f64 / (b.0 - a.0) as f64;
    let f = |t: i64| (a.1 as f64 + (t - a.0) as f64 * scale).round().max(0.0) as i64;
    for d in script.dialogues_mut() {
        d.start_time = f(d.start_time);
        d.end_time = f(d.end_time);
    }
}

pub fn fps_convert(script: &mut Script, from_fps: f64, to_fps: f64) {
    let r = from_fps / to_fps;
    for d in script.dialogues_mut() {
        d.start_time = (d.start_time as f64 * r).round() as i64;
        d.end_time = (d.end_time as f64 * r).round() as i64;
    }
}

pub fn sort_by_start(script: &mut Script) {
    let first = script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_)));
    let Some(first) = first else { return };
    let last = script.entries.iter().rposition(|e| matches!(e, Entry::Dialogue(_))).unwrap();
    let mut slice: Vec<Entry> = script.entries.drain(first..=last).collect();
    slice.sort_by_key(|e| match e {
        Entry::Dialogue(d) => (d.start_time, d.layer),
        _ => (i64::MAX, 0),
    });
    for (k, e) in slice.into_iter().enumerate() {
        script.entries.insert(first + k, e);
    }
}

pub fn snap_to_frames(script: &mut Script, fps: f64) {
    for d in script.dialogues_mut() {
        d.start_time = crate::time::snap_to_frame(d.start_time, fps);
        d.end_time = crate::time::snap_to_frame(d.end_time, fps);
    }
}

/// Aynı stil/katmandaki ardışık çakışmaları, önceki satırın bitişini kısaltarak giderir.
pub fn fix_overlaps(script: &mut Script) -> usize {
    let mut fixed = 0;
    let mut last: Option<usize> = None;
    for i in 0..script.entries.len() {
        let (start, style, layer) = match &script.entries[i] {
            Entry::Dialogue(d) if !d.comment => (d.start_time, d.style.clone(), d.layer),
            _ => continue,
        };
        if let Some(p) = last {
            if let Entry::Dialogue(pd) = &mut script.entries[p] {
                if pd.style == style && pd.layer == layer && pd.end_time > start && start > pd.start_time {
                    pd.end_time = start;
                    fixed += 1;
                }
            }
        }
        last = Some(i);
    }
    fixed
}

/// Yinelenen (aynı zaman+stil+metin) satırları siler.
pub fn remove_duplicates(script: &mut Script) -> usize {
    let mut seen = std::collections::HashSet::new();
    let before = script.entries.len();
    script.entries.retain(|e| match e {
        Entry::Dialogue(d) => seen.insert((d.start_time, d.end_time, d.style.clone(), d.text.clone(), d.layer, d.comment)),
        _ => true,
    });
    before - script.entries.len()
}

pub fn remove_empty(script: &mut Script) -> usize {
    let before = script.entries.len();
    script.entries.retain(|e| match e {
        Entry::Dialogue(d) => d.comment || !tags::visible_text(&d.text).trim().is_empty() || d.text.contains("\\p1"),
        _ => true,
    });
    before - script.entries.len()
}

/// Çift boşluk, kenar boşluğu ve üç noktayı temizler.
pub fn clean_text(text: &str) -> String {
    let mut out = String::new();
    for p in tags::split(text) {
        match p {
            tags::Part::Block(b) => {
                out.push('{');
                out.push_str(b);
                out.push('}');
            }
            tags::Part::Text(t) => {
                let mut t = t.replace("...", "…");
                while t.contains("  ") {
                    t = t.replace("  ", " ");
                }
                out.push_str(&t.replace(" \\N", "\\N").replace("\\N ", "\\N"));
            }
        }
    }
    out.trim().to_string()
}

/// Türkçe büyük/küçük harf dönüşümü (i/İ, ı/I).
pub fn tr_upper(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'i' => vec!['İ'], 'ı' => vec!['I'], c => c.to_uppercase().collect() }).collect()
}

pub fn tr_lower(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'I' => vec!['ı'], 'İ' => vec!['i'], c => c.to_lowercase().collect() }).collect()
}

/// Uzun düz metni (etiketsiz) en fazla `max_lines` satıra dengeli böler (`\N`).
pub fn balance_lines(text: &str, max_len: usize) -> String {
    let flat = text.replace("\\N", " ");
    let flat = flat.trim();
    if flat.chars().count() <= max_len {
        return flat.to_string();
    }
    let chars: Vec<char> = flat.chars().collect();
    let mid = chars.len() / 2;
    let mut best: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        if *c == ' ' && best.map_or(true, |b| i.abs_diff(mid) < b.abs_diff(mid)) {
            best = Some(i);
        }
    }
    match best {
        Some(i) => format!("{}\\N{}", chars[..i].iter().collect::<String>().trim_end(), chars[i + 1..].iter().collect::<String>().trim_start()),
        None => flat.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: i64, e: i64, t: &str) -> Dialogue {
        Dialogue { start_time: s, end_time: e, text: t.into(), ..Default::default() }
    }

    #[test]
    fn overlap_and_fix() {
        let mut s = Script::from_dialogues(vec![d(0, 2000, "a"), d(1500, 3000, "b")]);
        let issues = check(&s, &Rules::default());
        assert!(issues.iter().any(|i| i.kind == Kind::Overlap));
        assert_eq!(fix_overlaps(&mut s), 1);
        assert!(!check(&s, &Rules::default()).iter().any(|i| i.kind == Kind::Overlap));
    }

    #[test]
    fn tr_case() {
        assert_eq!(tr_upper("iğne ışık"), "İĞNE IŞIK");
        assert_eq!(tr_lower("İĞNE IŞIK"), "iğne ışık");
    }

    #[test]
    fn retime() {
        let mut s = Script::from_dialogues(vec![d(1000, 2000, "a"), d(11000, 12000, "b")]);
        linear_retime(&mut s, (1000, 2000), (11000, 22000));
        let v: Vec<_> = s.dialogues().map(|d| d.start_time).collect();
        assert_eq!(v, vec![2000, 22000]);
    }

    #[test]
    fn balance() {
        assert_eq!(balance_lines("bir iki üç dört beş altı yedi sekiz dokuz on on bir on iki", 30), "bir iki üç dört beş altı yedi\\Nsekiz dokuz on on bir on iki");
    }
}
