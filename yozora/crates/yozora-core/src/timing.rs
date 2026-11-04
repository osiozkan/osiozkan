//! Zamanlama işlemleri: kaydırma, TPP (Timing Post-Processor), birleştirme, bölme.
use crate::ass::{Document, Event};

/// Seçili olayları `delta_ms` kadar kaydırır. `which`: etkilenecek indeksler (0 tabanlı).
pub fn shift(doc: &mut Document, which: &[usize], delta_ms: i64, start: bool, end: bool) {
    for &i in which {
        if let Some(e) = doc.events.get_mut(i) {
            if start {
                e.start = (e.start + delta_ms).max(0);
            }
            if end {
                e.end = (e.end + delta_ms).max(0);
            }
        }
    }
}

/// Başlangıca göre kararlı sıralama (yorum satırları dahil).
pub fn sort_by_start(doc: &mut Document) {
    doc.events.sort_by_key(|e| (e.start, e.layer));
}

#[derive(Debug, Clone)]
pub struct TppOptions {
    pub lead_in: i64,
    pub lead_out: i64,
    pub snap_start_before: i64,
    pub snap_start_after: i64,
    pub snap_end_before: i64,
    pub snap_end_after: i64,
    /// Bu değerden küçük ya da eşit boşluklar kapatılır (0 = kapalı).
    pub link_gap: i64,
    /// 0..=100: boşluğun hangi oranında bölüneceği (0 = sonraki başlangıç geri çekilir, 100 = önceki bitiş uzar).
    pub link_bias: i64,
}

impl Default for TppOptions {
    fn default() -> Self {
        // Anime'de yaygın kullanılan Aegisub TPP varsayılanlarına yakın.
        TppOptions { lead_in: 120, lead_out: 500, snap_start_before: 350, snap_start_after: 0, snap_end_before: 0, snap_end_after: 900, link_gap: 0, link_bias: 0 }
    }
}

fn snap(value: i64, keyframes: &[i64], before: i64, after: i64) -> i64 {
    // before: anahtar karenin value'dan en çok bu kadar sonra olmasına izin ver (value'yu ileri çek).
    // after:  anahtar karenin value'dan en çok bu kadar önce olmasına izin ver (value'yu geri çek).
    let mut best: Option<(i64, i64)> = None;
    for &k in keyframes {
        let d = k - value;
        let ok = (d >= 0 && d <= before) || (d < 0 && -d <= after);
        if ok {
            let a = d.abs();
            if best.map_or(true, |(ba, _)| a < ba) {
                best = Some((a, k));
            }
        }
    }
    best.map(|(_, k)| k).unwrap_or(value)
}

/// TPP'yi seçili (yorum olmayan) olaylara uygular. `keyframes` ms cinsinden, sıralı olmalıdır.
pub fn tpp(doc: &mut Document, which: &[usize], opt: &TppOptions, keyframes: &[i64]) {
    for &i in which {
        let Some(e) = doc.events.get_mut(i) else { continue };
        if e.comment {
            continue;
        }
        e.start = (e.start - opt.lead_in).max(0);
        e.end += opt.lead_out;
        if !keyframes.is_empty() {
            e.start = snap(e.start, keyframes, opt.snap_start_before, opt.snap_start_after);
            e.end = snap(e.end, keyframes, opt.snap_end_before, opt.snap_end_after);
        }
        if e.end < e.start {
            e.end = e.start;
        }
    }
    if opt.link_gap > 0 {
        let mut order: Vec<usize> = which.iter().copied().filter(|&i| i < doc.events.len() && !doc.events[i].comment).collect();
        order.sort_by_key(|&i| doc.events[i].start);
        for w in order.windows(2) {
            let (a, b) = (w[0], w[1]);
            let gap = doc.events[b].start - doc.events[a].end;
            if gap > 0 && gap <= opt.link_gap {
                let mid = doc.events[a].end + gap * opt.link_bias / 100;
                doc.events[a].end = mid;
                doc.events[b].start = mid;
            }
        }
    }
}

/// Ardışık seçili olayları tek olayda birleştirir (metinler `\N` ile). Birleşik olay ilk indekste kalır.
pub fn join(doc: &mut Document, which: &[usize]) -> Option<usize> {
    let mut w: Vec<usize> = which.iter().copied().filter(|&i| i < doc.events.len()).collect();
    w.sort_unstable();
    w.dedup();
    let first = *w.first()?;
    if w.len() < 2 {
        return Some(first);
    }
    let start = w.iter().map(|&i| doc.events[i].start).min()?;
    let end = w.iter().map(|&i| doc.events[i].end).max()?;
    let text = w.iter().map(|&i| doc.events[i].text.clone()).collect::<Vec<_>>().join("\\N");
    for &i in w.iter().skip(1).rev() {
        doc.events.remove(i);
    }
    let e = &mut doc.events[first];
    e.start = start;
    e.end = end;
    e.text = text;
    Some(first)
}

/// Olayı `at_ms` anında ikiye böler; ikinci yarı hemen sonrasına eklenir. Metin `split_text_at` karakter indeksinden bölünür
/// (verilmezse iki yarı da aynı metni taşır).
pub fn split(doc: &mut Document, index: usize, at_ms: i64, split_text_at: Option<usize>) -> bool {
    let Some(e) = doc.events.get(index) else { return false };
    if at_ms <= e.start || at_ms >= e.end {
        return false;
    }
    let mut a: Event = e.clone();
    let mut b: Event = e.clone();
    a.end = at_ms;
    b.start = at_ms;
    if let Some(pos) = split_text_at {
        let chars: Vec<char> = e.text.chars().collect();
        let pos = pos.min(chars.len());
        a.text = chars[..pos].iter().collect::<String>().trim_end().to_string();
        b.text = chars[pos..].iter().collect::<String>().trim_start().to_string();
    }
    doc.events[index] = a;
    doc.events.insert(index + 1, b);
    true
}

/// Küçük boşlukları kapatır (Aegisub "Make times continuous").
pub fn make_continuous(doc: &mut Document, which: &[usize], max_gap: i64) {
    let mut order: Vec<usize> = which.iter().copied().filter(|&i| i < doc.events.len() && !doc.events[i].comment).collect();
    order.sort_by_key(|&i| doc.events[i].start);
    for w in order.windows(2) {
        let gap = doc.events[w[1]].start - doc.events[w[0]].end;
        if gap > 0 && gap <= max_gap {
            doc.events[w[0]].end = doc.events[w[1]].start;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(s: i64, e: i64, t: &str) -> Event {
        Event { start: s, end: e, text: t.into(), ..Event::default() }
    }
    #[test]
    fn shifting() {
        let mut d = Document::new_default();
        d.events = vec![ev(1000, 2000, "a"), ev(3000, 4000, "b")];
        shift(&mut d, &[1], -500, true, true);
        assert_eq!((d.events[1].start, d.events[1].end), (2500, 3500));
        shift(&mut d, &[0], -5000, true, true);
        assert_eq!(d.events[0].start, 0);
    }
    #[test]
    fn tpp_snap_and_link() {
        let mut d = Document::new_default();
        d.events = vec![ev(1000, 2000, "a"), ev(2300, 3000, "b")];
        let opt = TppOptions { lead_in: 100, lead_out: 100, snap_start_before: 200, snap_start_after: 0, snap_end_before: 0, snap_end_after: 0, link_gap: 500, link_bias: 50 };
        tpp(&mut d, &[0, 1], &opt, &[1000]);
        assert_eq!(d.events[0].start, 1000); // 900 -> 1000'e çekildi
        // 2100..2200 arası boşluk ikiye bölünür.
        assert_eq!(d.events[0].end, d.events[1].start);
        assert_eq!(d.events[0].end, 2150);
    }
    #[test]
    fn join_split() {
        let mut d = Document::new_default();
        d.events = vec![ev(0, 1000, "a"), ev(1000, 2000, "b"), ev(5000, 6000, "z")];
        assert_eq!(join(&mut d, &[0, 1]), Some(0));
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].text, "a\\Nb");
        assert!(split(&mut d, 0, 1000, Some(1)));
        assert_eq!(d.events[0].text, "a");
        assert_eq!(d.events[1].text, "\\Nb");
        assert!(!split(&mut d, 0, 0, None));
    }
}
