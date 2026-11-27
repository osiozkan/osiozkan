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
    for d in &lines {
        let segs = segments(&d.text);
        let mut next_open = vec![];
        for s in segs {
            // parça, bitişi bu satırın başlangıcına ulaşan (araya başka satır girse de) çıktıyı uzatır
            let hit = (0..out.len()).rev().find(|&k| out[k].text == s && out[k].end_time >= d.start_time && !next_open.contains(&k));
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

/// Zamanı başka bir satırla çakışan diyalog satırları (Aegisub "Select overlaps"): yorumlar,
/// tipografi/karaoke/fx satırları dışarıda; `same_style` ise yalnızca aynı stil+katman içinde.
pub fn overlapping(script: &Script, same_style: bool) -> Vec<usize> {
    let mut v: Vec<(i64, i64, usize, String)> = script.entries.iter().enumerate().filter_map(|(i, e)| match e {
        Entry::Dialogue(d) if !d.comment && !crate::qc::is_typeset(d) && !crate::qc::is_karaoke(d) && !d.effect.eq_ignore_ascii_case("fx") && d.end_time > d.start_time => {
            Some((d.start_time, d.end_time, i, if same_style { format!("{}\u{0}{}", d.style, d.layer) } else { String::new() }))
        }
        _ => None,
    }).collect();
    v.sort_by(|a, b| a.3.cmp(&b.3).then(a.0.cmp(&b.0)));
    let mut out = vec![];
    let mut k = 0;
    while k < v.len() {
        // aynı gruptaki satırlar başlangıca göre sıralı: en geç biten öncekiyle karşılaştır
        let mut far: Option<(i64, usize)> = None;
        let g = v[k].3.clone();
        while k < v.len() && v[k].3 == g {
            let (s, e, i, _) = (v[k].0, v[k].1, v[k].2, ());
            if let Some((fe, fi)) = far {
                if s < fe {
                    out.push(i);
                    out.push(fi);
                }
            }
            if far.is_none_or(|(fe, _)| e > fe) {
                far = Some((e, i));
            }
            k += 1;
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Etiket blokları dışındaki boşlukların bayt konumları.
fn text_spaces(t: &str) -> Vec<usize> {
    let mut v = vec![];
    let mut pos = 0;
    for part in crate::tags::split(t) {
        match part {
            crate::tags::Part::Block(b) => pos += b.len() + 2,
            crate::tags::Part::Text(x) => {
                v.extend(x.match_indices(' ').map(|(k, _)| pos + k));
                pos += x.len();
            }
        }
    }
    v
}

/// Subtitle Edit "sözcüğü komşu satıra taşı": `forward` ise `a`'nın SON sözcüğü `b`'nin başına, değilse
/// `b`'nin İLK sözcüğü `a`'nın sonuna taşınır. Baştaki etiket blokları (`{\an8}`) yerinde kalır, satır
/// içi etiketler sözcükle gider; iki satır `max` karaktere göre yeniden dengelenir. Taşınacak sözcük yoksa None.
pub fn move_word(a: &str, b: &str, forward: bool, max: usize) -> Option<(String, String)> {
    use crate::linebreak::{balance, unbreak};
    // iki konuşmacılı satır ("- Ne?\N- Hiç."): düzleştirmek çizgi yapısını bozar — dokunulmaz
    let dialog = |t: &str| t.split("\\N").skip(1).any(|seg| { let v = crate::tags::strip_tags(seg); let v = v.trim_start(); v.starts_with('-') || v.starts_with('–') || v.starts_with('—') });
    if dialog(a) || dialog(b) {
        return None;
    }
    let (fa, fb) = (unbreak(a), unbreak(b));
    let (la, lb) = (crate::tags::leading_blocks(&fa).to_string(), crate::tags::leading_blocks(&fb).to_string());
    let (ba, bb) = (fa[la.len()..].trim().to_string(), fb[lb.len()..].trim().to_string());
    let (na, nb) = if forward {
        if crate::tags::strip_tags(&ba).trim().is_empty() { return None; }
        let (rest, w) = match text_spaces(&ba).last() { Some(&k) => (ba[..k].trim_end(), ba[k + 1..].trim()), None => ("", ba.as_str()) };
        (format!("{la}{rest}"), format!("{lb}{w}{}{bb}", if bb.is_empty() { "" } else { " " }))
    } else {
        if crate::tags::strip_tags(&bb).trim().is_empty() { return None; }
        let (w, rest) = match text_spaces(&bb).first() { Some(&k) => (bb[..k].trim_end(), bb[k + 1..].trim()), None => (bb.as_str(), "") };
        (format!("{la}{ba}{}{w}", if ba.is_empty() { "" } else { " " }), format!("{lb}{rest}"))
    };
    Some((balance(&na, max), balance(&nb, max)))
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
    fn cakisanlar() {
        let mut s = sc(&[(0, 1000, "a"), (500, 1500, "b"), (2000, 3000, "c"), (2900, 3100, "d"), (5000, 6000, "e")]);
        let i = idx(&s);
        assert_eq!(overlapping(&s, true), vec![i[0], i[1], i[2], i[3]]);
        if let Entry::Dialogue(d) = &mut s.entries[i[1]] { d.style = "Diger".into(); }
        assert_eq!(overlapping(&s, true), vec![i[2], i[3]]);
        assert_eq!(overlapping(&s, false).len(), 4);
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
        // araya başka satır girse de örtüşen parça birleşir
        let mut s = sc(&[(0, 2000, "A"), (0, 2000, "B"), (2000, 4000, "A\\NB")]);
        let i = idx(&s);
        recombine(&mut s, &i);
        assert_eq!(dl(&s), vec![(0, 4000, "A".into()), (0, 4000, "B".into())]);
        // tek satır: değişiklik yok
        let mut s = sc(&[(0, 1000, "A\\NB")]);
        let i = idx(&s);
        assert!(recombine(&mut s, &i).is_empty());
    }

    #[test]
    fn move_word_between_lines() {
        assert_eq!(move_word("Ben de seninle", "gelmek istiyorum.", true, 42), Some(("Ben de".into(), "seninle gelmek istiyorum.".into())));
        assert_eq!(move_word("Ben de", "seninle gelmek istiyorum.", false, 42), Some(("Ben de seninle".into(), "gelmek istiyorum.".into())));
        // baştaki etiket yerinde kalır, satır içi etiket sözcükle gider; \N açılır
        assert_eq!(move_word("{\\an8}Bir\\N{\\i1}iki{\\i0}", "{\\an8}üç", true, 42), Some(("{\\an8}Bir".into(), "{\\an8}{\\i1}iki{\\i0} üç".into())));
        assert_eq!(move_word("tek", "", true, 42), Some(("".into(), "tek".into())));
        assert_eq!(move_word("a", "{\\i1}", false, 42), None);
        // etiket içindeki boşluk sözcük sınırı sayılmaz
        assert_eq!(move_word("x {\\fnArial Black}y", "z", true, 42), Some(("x".into(), "{\\fnArial Black}y z".into())));
        assert_eq!(move_word("- Ne?\\N- Hiç.", "Sonra gel.", true, 42), None);
        assert_eq!(move_word("Tamam.", "- Ne?\\N- Hiç.", false, 42), None);
        // uzun sonuç dengelenir
        let (_, nb) = move_word("bir iki üç", "dört beş altı yedi sekiz dokuz on onbir oniki onüç ondört", true, 30).unwrap();
        assert!(nb.contains("\\N"));
    }
}
