//! Zamanlama araçları. `sel` boşsa tüm olaylar etkilenir.
use crate::model::Document;

fn each<'a>(doc: &'a mut Document, sel: &'a [usize]) -> impl Iterator<Item = &'a mut crate::model::Event> {
    let all = sel.is_empty();
    doc.events.iter_mut().enumerate().filter(move |(i, _)| all || sel.contains(i)).map(|(_, e)| e)
}

pub fn shift(doc: &mut Document, sel: &[usize], delta_ms: i64) {
    for e in each(doc, sel) {
        e.start = (e.start + delta_ms).max(0);
        e.end = (e.end + delta_ms).max(0);
    }
}

/// Zamanları `pivot` etrafında `factor` ile ölçekler (FPS dönüşümü / kayma düzeltme).
pub fn scale(doc: &mut Document, sel: &[usize], pivot: i64, factor: f64) {
    let f = |t: i64| (pivot as f64 + (t - pivot) as f64 * factor).round().max(0.0) as i64;
    for e in each(doc, sel) {
        e.start = f(e.start);
        e.end = f(e.end);
    }
}

/// Zaman sırasına göre ardışık satırlar arasında çakışmayı giderir ve en az `min_gap` bırakır.
/// Değişen satır sayısını döner.
pub fn fix_overlaps(doc: &mut Document, min_gap: i64) -> usize {
    let mut idx: Vec<usize> = (0..doc.events.len()).filter(|&i| !doc.events[i].comment).collect();
    idx.sort_by_key(|&i| (doc.events[i].start, doc.events[i].end));
    let mut n = 0;
    for w in idx.windows(2) {
        let (a, b) = (w[0], w[1]);
        let limit = doc.events[b].start - min_gap;
        if doc.events[a].end > limit && limit > doc.events[a].start {
            doc.events[a].end = limit;
            n += 1;
        }
    }
    n
}

/// Çok kısa satırları (sonraki satıra kadar) `min_dur`'a uzatır.
pub fn extend_short(doc: &mut Document, min_dur: i64, min_gap: i64) -> usize {
    let mut idx: Vec<usize> = (0..doc.events.len()).filter(|&i| !doc.events[i].comment).collect();
    idx.sort_by_key(|&i| (doc.events[i].start, doc.events[i].end));
    let mut n = 0;
    for k in 0..idx.len() {
        let i = idx[k];
        if doc.events[i].duration() >= min_dur {
            continue;
        }
        let mut target = doc.events[i].start + min_dur;
        if let Some(&nx) = idx.get(k + 1) {
            target = target.min(doc.events[nx].start - min_gap);
        }
        if target > doc.events[i].end {
            doc.events[i].end = target;
            n += 1;
        }
    }
    n
}

/// Zamanları en yakın kareye oturtur.
pub fn snap_to_frames(doc: &mut Document, sel: &[usize], fps: f64) {
    let snap = |t: i64| ((t as f64 * fps / 1000.0).round() * 1000.0 / fps).round() as i64;
    for e in each(doc, sel) {
        e.start = snap(e.start);
        e.end = snap(e.end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Event;
    fn doc() -> Document {
        let mut d = Document::new();
        for (s, e) in [(0, 2000), (1500, 3000), (3000, 3200)] {
            d.events.push(Event { start: s, end: e, ..Event::default() });
        }
        d
    }
    #[test]
    fn overlaps_and_short() {
        let mut d = doc();
        assert_eq!(fix_overlaps(&mut d, 0), 1);
        assert_eq!(d.events[0].end, 1500);
        assert_eq!(extend_short(&mut d, 700, 0), 1);
        assert_eq!(d.events[2].end, 3700);
    }
    #[test]
    fn shift_clamps() {
        let mut d = doc();
        shift(&mut d, &[0], -500);
        assert_eq!((d.events[0].start, d.events[0].end), (0, 1500));
    }
}
