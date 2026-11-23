//! Izgara katlamaları (Aegisub/arch1t3cht "folds"): bir satır bloğu (ör. büyük bir tabelanın
//! katmanları) tek satıra katlanır. Aegisub ile aynı biçimde saklanır: açan ve kapatan satırın
//! Extradata'sında `_aegi_folddata` = `yan;katlı;kimlik` (yan 0 = açan, 1 = kapatan). Böylece
//! Aegisub'da oluşturulan katlamalar osisub'da, osisub'dakiler Aegisub'da görünür.
//! Geçerlilik `FoldController::FixFolds` / görünürlük `LinkFolds` mantığıyla.
use crate::ass::{Entry, Script};
use std::collections::{BTreeMap, HashSet};

pub const KEY: &str = "_aegi_folddata";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fold {
    /// Açan ve kapatan satırın girdi indeksi
    pub start: usize,
    pub end: usize,
    pub collapsed: bool,
    pub id: i64,
}

/// Satırın katlama işareti: (kapatan mı, katlı mı, kimlik).
fn marker(all: &BTreeMap<u32, (String, String)>, e: &Entry) -> Option<(bool, bool, i64)> {
    let Entry::Dialogue(d) = e else { return None };
    let v = d.extra.iter().find_map(|id| all.get(id).filter(|(k, _)| k == KEY).map(|(_, v)| v))?;
    let f: Vec<&str> = v.split(';').collect();
    if f.len() != 3 {
        return None;
    }
    let side: i64 = f[0].trim().parse().ok()?;
    let col: i64 = f[1].trim().parse().ok()?;
    let id: i64 = f[2].trim().parse().ok()?;
    Some((side != 0, col != 0, id))
}

/// Geçerli katlamalar (açan satıra göre sıralı). Eşleşmeyen/çapraz işaretler yok sayılır;
/// kapatan, yığındaki açanını bulursa aradaki (içteki) eşleşmemiş açanlar geçersizdir.
pub fn list(script: &Script) -> Vec<Fold> {
    let all = script.extradata();
    let mut stack: Vec<(usize, bool, i64)> = vec![];
    let mut done: HashSet<i64> = HashSet::new();
    // Aegisub FixFolds: tamamlanmış bir kimlik yeniden görülürse (ör. katlı blok kopyalanmış)
    // yeni kimlikle ayrı bir katlama sayılır
    let mut remap: std::collections::HashMap<i64, i64> = Default::default();
    let mut max_id = script.entries.iter().filter_map(|e| marker(&all, e)).map(|m| m.2).max().unwrap_or(0);
    let mut out = vec![];
    for (i, e) in script.entries.iter().enumerate() {
        let Some((side, col, mut id)) = marker(&all, e) else { continue };
        let mut guard = 0;
        while let Some(&n) = remap.get(&id) {
            id = n;
            guard += 1;
            if guard > 10_000 { break; }
        }
        if done.contains(&id) {
            max_id += 1;
            remap.insert(id, max_id);
            id = max_id;
        }
        if !side {
            if !stack.iter().any(|s| s.2 == id) {
                stack.push((i, col, id));
            }
        } else if let Some(p) = stack.iter().rposition(|s| s.2 == id) {
            for s in stack.drain(p + 1..) {
                done.insert(s.2);
            }
            let (st, c, _) = stack.pop().unwrap();
            done.insert(id);
            out.push(Fold { start: st, end: i, collapsed: c, id });
        } else {
            done.insert(id);
        }
    }
    out.sort_by_key(|f| f.start);
    out
}

/// Katlı katlamaların içinde kalıp ızgarada gizlenen girdiler (açan satır görünür kalır).
pub fn hidden(script: &Script) -> HashSet<usize> {
    let mut h = HashSet::new();
    let folds = list(script);
    let mut i = 0;
    while i < folds.len() {
        let f = &folds[i];
        if f.collapsed {
            h.extend(f.start + 1..=f.end);
            // içteki katlamalar zaten gizli
            let end = f.end;
            while i + 1 < folds.len() && folds[i + 1].start <= end {
                i += 1;
            }
        }
        i += 1;
    }
    h
}

/// `line`'ın en içteki katlaması: satır bir açansa kendisininki, değilse onu içeren en içteki.
pub fn innermost_at(folds: &[Fold], line: usize) -> Option<usize> {
    if let Some(k) = folds.iter().position(|f| f.start == line) {
        return Some(k);
    }
    folds.iter().enumerate().filter(|(_, f)| f.start < line && line <= f.end).max_by_key(|(_, f)| f.start).map(|(k, _)| k)
}

/// Satırın Extradata'sındaki bir anahtarı ayarlar/siler. Extradata bölümü olaylardan önce de
/// olabildiğinden girdi indeksleri kayabilir: dönüş satırın yeni indeksi.
pub fn set_line_value(script: &mut Script, idx: usize, key: &str, value: Option<&str>) -> usize {
    let Some(Entry::Dialogue(d)) = script.entries.get(idx) else { return idx };
    let mut vals = script.extra_values(&d.extra);
    match value {
        Some(v) => vals.insert(key.to_string(), v.to_string()),
        None => vals.remove(key),
    };
    let k = script.entries[..idx].iter().filter(|e| !is_extra(e)).count();
    let ids = script.intern_extra(&vals);
    let at = locate(script, k);
    if let Some(Entry::Dialogue(d)) = script.entries.get_mut(at) {
        d.extra = ids;
    }
    script.clean_extradata();
    locate(script, k)
}

fn is_extra(e: &Entry) -> bool {
    e.section() == crate::extradata::SEC_EXTRADATA
}

fn locate(script: &Script, k: usize) -> usize {
    let mut n = 0;
    for (i, e) in script.entries.iter().enumerate() {
        if is_extra(e) {
            continue;
        }
        if n == k {
            return i;
        }
        n += 1;
    }
    script.entries.len()
}

/// Birden çok satırın değerini indeks kaymasına dayanıklı biçimde yazar.
fn set_many(script: &mut Script, items: Vec<(usize, Option<String>)>) {
    let ks: Vec<(usize, Option<String>)> = items.into_iter().map(|(i, v)| (script.entries[..i].iter().filter(|e| !is_extra(e)).count(), v)).collect();
    for (k, v) in ks {
        let at = locate(script, k);
        set_line_value(script, at, KEY, v.as_deref());
    }
}

fn value(side: bool, collapsed: bool, id: i64) -> String {
    format!("{};{};{}", side as i32, collapsed as i32, id)
}

/// Aegisub `grid/fold/create`: `start`..=`end` satırlarını katlar (katlı olarak). Uçlardan biri zaten
/// bir katlamanın ucuysa ya da aralık başka bir katlamayı yarıda kesiyorsa `false`.
pub fn add(script: &mut Script, start: usize, end: usize) -> bool {
    if start >= end || !matches!(script.entries.get(start), Some(Entry::Dialogue(_))) || !matches!(script.entries.get(end), Some(Entry::Dialogue(_))) {
        return false;
    }
    let folds = list(script);
    if folds.iter().any(|f| [f.start, f.end].contains(&start) || [f.start, f.end].contains(&end)) {
        return false;
    }
    // aralık her katlamayı ya tamamen içermeli ya hiç kesmemeli
    if folds.iter().any(|f| (f.start > start && f.start < end) != (f.end > start && f.end < end)) {
        return false;
    }
    let all = script.extradata();
    let id = script.entries.iter().filter_map(|e| marker(&all, e)).map(|m| m.2).max().unwrap_or(0) + 1;
    set_many(script, vec![(start, Some(value(false, true, id))), (end, Some(value(true, true, id)))]);
    true
}

/// Satırlardaki (en içteki) katlamaların durumunu değiştirir: `f(eski katlı) -> yeni katlı`.
/// `None` dönen işlev katlamayı kaldırır. Dönüş: etkilenen katlama sayısı.
pub fn update_at(script: &mut Script, lines: &[usize], f: impl Fn(bool) -> Option<bool>) -> usize {
    let folds = list(script);
    let mut ks: Vec<usize> = lines.iter().filter_map(|&l| innermost_at(&folds, l)).collect();
    ks.sort_unstable();
    ks.dedup();
    apply(script, ks.iter().map(|&k| &folds[k]).cloned().collect(), f)
}

/// Tüm katlamalar için `update_at`.
pub fn update_all(script: &mut Script, f: impl Fn(bool) -> Option<bool>) -> usize {
    let folds = list(script);
    apply(script, folds, f)
}

fn apply(script: &mut Script, folds: Vec<Fold>, f: impl Fn(bool) -> Option<bool>) -> usize {
    let mut items = vec![];
    for fo in &folds {
        let nv = f(fo.collapsed);
        items.push((fo.start, nv.map(|c| value(false, c, fo.id))));
        items.push((fo.end, nv.map(|c| value(true, c, fo.id))));
    }
    let n = folds.len();
    if n > 0 {
        set_many(script, items);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn sc(n: usize) -> (Script, usize) {
        let s = Script::from_dialogues((0..n).map(|k| Dialogue { text: format!("s{k}"), ..Default::default() }).collect());
        let f = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        (s, f)
    }

    #[test]
    fn add_toggle_nest_and_roundtrip_with_aegisub_format() {
        let (mut s, f) = sc(8);
        assert!(add(&mut s, f + 1, f + 4));
        let l = list(&s);
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].start, l[0].end, l[0].collapsed), (f + 1, f + 4, true));
        assert_eq!(hidden(&s), (f + 2..=f + 4).collect());
        // kesişen katlama reddedilir, içteki kabul edilir
        assert!(!add(&mut s, f + 3, f + 6));
        assert!(!add(&mut s, f + 1, f + 6));
        assert!(add(&mut s, f + 2, f + 3));
        assert_eq!(list(&s).len(), 2);
        // aç: dıştaki açılır, içteki katlı kalır → yalnız f+3 gizli
        assert_eq!(update_at(&mut s, &[f + 1], |_| Some(false)), 1);
        assert_eq!(hidden(&s), [f + 3].into_iter().collect());
        // f+3 (içteki kapatan) için en içteki katlama içtekidir
        update_at(&mut s, &[f + 3], |c| Some(!c));
        assert!(hidden(&s).is_empty());
        // Aegisub biçimi: kaydet → oku
        let text = s.to_ass_string();
        assert!(text.contains("_aegi_folddata"), "{text}");
        let back = Script::parse(&text);
        let fb = back.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        let lb = list(&back);
        assert_eq!(lb.iter().map(|x| (x.start - fb, x.end - fb, x.collapsed)).collect::<Vec<_>>(), vec![(1, 4, false), (2, 3, false)]);
        // tümünü kaldır
        let mut back = back;
        assert_eq!(update_all(&mut back, |_| None), 2);
        assert!(list(&back).is_empty());
        assert!(!back.to_ass_string().contains("_aegi_folddata"));
    }

    #[test]
    fn duplicated_fold_ids_become_separate_folds() {
        let (mut s, f) = sc(6);
        assert!(add(&mut s, f, f + 1));
        // aynı kimlikli ikinci çift (kopyalanmış katlı blok)
        let v = folds_val(&s, f);
        set_line_value(&mut s, f + 3, KEY, Some(&v.0));
        set_line_value(&mut s, f + 4, KEY, Some(&v.1));
        let l = list(&s);
        assert_eq!(l.iter().map(|x| (x.start - f, x.end - f)).collect::<Vec<_>>(), vec![(0, 1), (3, 4)]);
        assert_ne!(l[0].id, l[1].id);
        // ikincisini açmak dosyaya yeni kimliği yazar, ilki katlı kalır
        update_at(&mut s, &[f + 3], |_| Some(false));
        let l = list(&s);
        assert_eq!((l[0].collapsed, l[1].collapsed), (true, false));
    }

    fn folds_val(s: &Script, f: usize) -> (String, String) {
        let get = |i: usize| match &s.entries[i] { Entry::Dialogue(d) => s.extra_values(&d.extra).get(KEY).cloned().unwrap(), _ => unreachable!() };
        (get(f), get(f + 1))
    }

    #[test]
    fn invalid_markers_ignored() {
        let (mut s, f) = sc(5);
        // eşsiz kapatan ve kapanmamış açan
        set_line_value(&mut s, f, KEY, Some("1;1;7"));
        set_line_value(&mut s, f + 1, KEY, Some("0;1;8"));
        set_line_value(&mut s, f + 2, KEY, Some("bozuk"));
        assert!(list(&s).is_empty());
        assert!(hidden(&s).is_empty());
        // yeni katlama mevcut en büyük kimlikten sonra
        assert!(add(&mut s, f + 3, f + 4));
        assert_eq!(list(&s)[0].id, 9);
    }
}
