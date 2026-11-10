//! İki altyazı sürümünü karşılaştırma (çevirmen ↔ editör, eski ↔ yeni). Satırlar zaman örtüşmesiyle
//! eşlenir (metin düzeltmeleri ve küçük zamanlama oynamalarına dayanıklı); eşleşmeyenler eklendi/silindi.
use crate::ass::{Dialogue, Entry, Script};

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Yalnızca yeni sürümde (yeni sürümdeki entry indeksi)
    Added { new: usize },
    /// Yalnızca eski sürümde
    Removed { old: usize },
    /// Eşleşen satırda farklılık
    Modified { old: usize, new: usize, text: bool, timing: bool, style: bool, actor: bool },
}

fn lines(s: &Script) -> Vec<(usize, &Dialogue)> {
    let mut v: Vec<(usize, &Dialogue)> = s.entries.iter().enumerate().filter_map(|(i, e)| match e {
        Entry::Dialogue(d) if !d.comment => Some((i, d)),
        _ => None,
    }).collect();
    v.sort_by_key(|(_, d)| (d.start_time, d.end_time));
    v
}

fn overlap(a: &Dialogue, b: &Dialogue) -> i64 {
    (a.end_time.min(b.end_time) - a.start_time.max(b.start_time)).max(0)
}

/// Farklar (eski sırasına göre; eklenenler zamanlarına göre araya yerleşir).
pub fn compare(old: &Script, new: &Script) -> Vec<Change> {
    let (a, b) = (lines(old), lines(new));
    let mut used = vec![false; b.len()];
    let mut pair: Vec<Option<usize>> = vec![None; a.len()];
    // 1) birebir aynı metin + örtüşme; 2) en çok örtüşen (kısa satırın yarısından fazla) ya da aynı zaman
    for pass in 0..2 {
        for (i, (_, x)) in a.iter().enumerate() {
            if pair[i].is_some() {
                continue;
            }
            // b başlangıca göre sıralı: 60 sn geriden başlayan aday penceresi (ikili arama başlangıç anahtarıyla)
            let lo = b.partition_point(|(_, y)| y.start_time < x.start_time - 60_000);
            let best = b[lo..]
                .iter()
                .enumerate()
                .take_while(|(_, (_, y))| y.start_time < x.end_time + 10_000)
                .map(|(k, (_, y))| (lo + k, y))
                .filter(|(j, y)| {
                    if used[*j] {
                        return false;
                    }
                    let ov = overlap(x, y);
                    let short = x.duration().min(y.duration()).max(1);
                    if pass == 0 { x.text == y.text && (ov > 0 || (x.start_time, x.end_time) == (y.start_time, y.end_time)) } else { ov * 2 > short || (x.start_time, x.end_time) == (y.start_time, y.end_time) }
                })
                .max_by_key(|(_, y)| overlap(x, y));
            if let Some((j, _)) = best {
                used[j] = true;
                pair[i] = Some(j);
            }
        }
    }
    let mut out: Vec<(i64, Change)> = vec![];
    for (i, (oi, x)) in a.iter().enumerate() {
        match pair[i] {
            Some(j) => {
                let (ni, y) = b[j];
                let (text, timing, style, actor) = (x.text != y.text, (x.start_time, x.end_time) != (y.start_time, y.end_time), x.style != y.style, x.actor != y.actor);
                if text || timing || style || actor {
                    out.push((x.start_time, Change::Modified { old: *oi, new: ni, text, timing, style, actor }));
                }
            }
            None => out.push((x.start_time, Change::Removed { old: *oi })),
        }
    }
    for (j, (ni, y)) in b.iter().enumerate() {
        if !used[j] {
            out.push((y.start_time, Change::Added { new: *ni }));
        }
    }
    out.sort_by_key(|(t, _)| *t);
    out.into_iter().map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: i64, e: i64, t: &str) -> Dialogue {
        Dialogue { start_time: s, end_time: e, text: t.into(), style: "Default".into(), ..Default::default() }
    }

    #[test]
    fn detects_changes() {
        let old = Script::from_dialogues(vec![d(0, 2000, "Merhaba"), d(2000, 4000, "Nasılsın?"), d(5000, 6000, "Silinecek"), d(7000, 8000, "Aynı")]);
        let new = Script::from_dialogues(vec![d(0, 2000, "Merhaba!"), d(2100, 4000, "Nasılsın?"), d(6500, 6900, "Yeni satır"), d(7000, 8000, "Aynı")]);
        let c = compare(&old, &new);
        let kinds: Vec<&str> = c.iter().map(|x| match x { Change::Added { .. } => "+", Change::Removed { .. } => "-", Change::Modified { text: true, .. } => "metin", Change::Modified { timing: true, .. } => "zaman", _ => "?" }).collect();
        assert_eq!(kinds, vec!["metin", "zaman", "-", "+"]);
        assert!(compare(&old, &old).is_empty());
    }

    #[test]
    fn reordered_file_same_content() {
        let a = Script::from_dialogues(vec![d(0, 1000, "a"), d(1000, 2000, "b")]);
        let b = Script::from_dialogues(vec![d(1000, 2000, "b"), d(0, 1000, "a")]);
        assert!(compare(&a, &b).is_empty());
    }
}
