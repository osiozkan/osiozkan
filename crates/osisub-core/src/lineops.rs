//! Aegisub satır işlemleri: zamanları ardışık yap, satırları yeniden birleştir (Recombine Lines),
//! satır değiş tokuşu.
use crate::ass::{Dialogue, Entry, Script};

/// Seçili diyalog satırlarını (girdi sırasıyla) ardışık yapar (Aegisub "Make times continuous").
/// `change_start` ise her satırın başlangıcı öncekinin bitişine çekilir, değilse her satırın
/// bitişi sonrakinin başlangıcına uzatılır. Değişen satır sayısını döndürür.
pub fn make_continuous(script: &mut Script, idx: &[usize], change_start: bool) -> usize {
    let mut v: Vec<usize> = idx.iter().copied().filter(|&i| matches!(script.entries.get(i), Some(Entry::Dialogue(_)))).collect();
    v.sort_unstable();
    v.dedup();
    let mut n = 0;
    for w in v.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (Some(Entry::Dialogue(x)), Some(Entry::Dialogue(y))) = (script.entries.get(a), script.entries.get(b)) else { continue };
        if x.end_time == y.start_time {
            continue;
        }
        let (end, start) = (x.end_time, y.start_time);
        if change_start {
            if let Some(Entry::Dialogue(y)) = script.entries.get_mut(b) {
                y.start_time = end;
                y.end_time = y.end_time.max(end);
            }
        } else if let Some(Entry::Dialogue(x)) = script.entries.get_mut(a) {
            x.end_time = start.max(x.start_time);
        }
        n += 1;
    }
    n
}

fn segments(text: &str) -> Vec<String> {
    text.split("\\N").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Aegisub "Recombine Lines": örtüşen sahnelerde satır parçalara bölünüp ("A", "A\NB", "B")
/// yeniden yazılmışsa her benzersiz parçayı tek satıra çevirir ("A" baştan, "B" sona kadar).
/// Bir parça, zamanca bitişik (ya da örtüşen) satırlarda sürüyorsa tek satırda birleşir.
/// Seçili satırlar silinir, yerine (başlangıca göre sıralı) sonuç satırları ilk seçili satırın
/// yerine konur. Dönüş: yeni satırların girdi indeksleri.
pub fn recombine(script: &mut Script, idx: &[usize]) -> Vec<usize> {
    let mut v: Vec<usize> = idx.iter().copied().filter(|&i| matches!(script.entries.get(i), Some(Entry::Dialogue(d)) if !d.comment)).collect();
    v.sort_unstable();
    v.dedup();
    if v.len() < 2 {
        return vec![];
    }
    let mut lines: Vec<Dialogue> = v.iter().filter_map(|&i| if let Entry::Dialogue(d) = &script.entries[i] { Some(d.clone()) } else { None }).collect();
    lines.sort_by_key(|d| d.start_time); // kararlı: eşit başlangıçta dosya sırası
    // (parça, satır şablonu, başlangıç, bitiş)
    let mut out: Vec<Dialogue> = vec![];
    let mut open: Vec<usize> = vec![]; // `out` içinde hâlâ uzayabilen satırlar
    for d in &lines {
        let segs = segments(&d.text);
        let mut next_open = vec![];
        for s in segs {
            let hit = open.iter().copied().find(|&k| out[k].text == s && out[k].end_time >= d.start_time && !next_open.contains(&k));
            match hit {
                Some(k) => {
                    out[k].end_time = out[k].end_time.max(d.end_time);
                    next_open.push(k);
                }
                None => {
                    let mut n = d.clone();
                    n.text = s;
                    out.push(n);
                    next_open.push(out.len() - 1);
                }
            }
        }
        open = next_open;
    }
    // sıra: başlangıç, sonra oluşum sırası (kararlı)
    out.sort_by_key(|d| d.start_time);
    for &i in v.iter().rev() {
        script.entries.remove(i);
    }
    let at = v[0];
    let n = out.len();
    script.entries.splice(at..at, out.into_iter().map(Entry::Dialogue));
    (at..at + n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(lines: &[(i64, i64, &str)]) -> Script {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for &(a, b, t) in lines {
            s.entries.push(Entry::Dialogue(Dialogue { start_time: a, end_time: b, text: t.into(), ..Default::default() }));
        }
        s
    }
    fn dl(s: &Script) -> Vec<(i64, i64, String)> {
        s.entries.iter().filter_map(|e| if let Entry::Dialogue(d) = e { Some((d.start_time, d.end_time, d.text.clone())) } else { None }).collect()
    }
    fn idx(s: &Script) -> Vec<usize> {
        s.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect()
    }

    #[test]
    fn ardisik() {
        let mut s = sc(&[(0, 900, "a"), (1000, 2000, "b"), (1900, 3000, "c")]);
        let i = idx(&s);
        assert_eq!(make_continuous(&mut s, &i, false), 2);
        assert_eq!(dl(&s).iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), [(0, 1000), (1000, 1900), (1900, 3000)]);
        let mut s = sc(&[(0, 900, "a"), (1000, 2000, "b"), (2500, 3000, "c")]);
        let i = idx(&s);
        assert_eq!(make_continuous(&mut s, &i[..2], true), 1);
        assert_eq!(dl(&s).iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), [(0, 900), (900, 2000), (2500, 3000)]);
    }

    #[test]
    fn yeniden_birlestir() {
        let mut s = sc(&[(0, 2000, "A"), (2000, 4000, "A\\NB"), (4000, 6000, "B"), (7000, 8000, "C")]);
        let i = idx(&s);
        let r = recombine(&mut s, &i);
        assert_eq!(r.len(), 3);
        assert_eq!(dl(&s), vec![(0, 4000, "A".into()), (2000, 6000, "B".into()), (7000, 8000, "C".into())]);
        // aynı metin aralıklı (bitişik değil) ise ayrı kalır; boş parçalar atılır
        let mut s = sc(&[(0, 1000, "X\\N "), (1500, 2000, "X")]);
        let i = idx(&s);
        recombine(&mut s, &i);
        assert_eq!(dl(&s), vec![(0, 1000, "X".into()), (1500, 2000, "X".into())]);
        // üç kişilik örtüşme
        let mut s = sc(&[(0, 1000, "A"), (1000, 2000, "A\\NB"), (2000, 3000, "B\\NC"), (3000, 4000, "C")]);
        let i = idx(&s);
        recombine(&mut s, &i);
        assert_eq!(dl(&s), vec![(0, 2000, "A".into()), (1000, 3000, "B".into()), (2000, 4000, "C".into())]);
        // tek satır: değişiklik yok
        let mut s = sc(&[(0, 1000, "A\\NB")]);
        let i = idx(&s);
        assert!(recombine(&mut s, &i).is_empty());
    }
}
