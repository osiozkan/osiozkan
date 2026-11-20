//! Satır yer imleri (Subtitle Edit "Bookmarks"): satırın Extradata'sında `osisub.bookmark`
//! anahtarıyla saklanır (değer = not, boş olabilir). Extradata satıra bağlı olduğundan satır
//! eklenip silinince, sıralanınca ya da kopyalanınca yer imi satırla birlikte gider; Aegisub
//! dosyayı açıp kaydederken de korur.
use crate::ass::{Entry, Script};
use std::collections::{BTreeMap, HashSet};

pub const KEY: &str = "osisub.bookmark";

/// Yer imi kaydı olan Extradata kimlikleri (ızgarada hızlı denetim için).
pub fn ids(script: &Script) -> HashSet<u32> {
    script.extradata().into_iter().filter(|(_, (k, _))| k == KEY).map(|(id, _)| id).collect()
}

/// Satırın yer imi notu (`Some("")` = notsuz yer imi).
pub fn get(script: &Script, idx: usize) -> Option<String> {
    let Some(Entry::Dialogue(d)) = script.entries.get(idx) else { return None };
    if d.extra.is_empty() {
        return None;
    }
    script.extra_values(&d.extra).remove(KEY)
}

/// Yer imini koyar (`Some(not)`) ya da kaldırır (`None`). Satır değilse `false`.
pub fn set(script: &mut Script, idx: usize, note: Option<&str>) -> bool {
    let Some(Entry::Dialogue(d)) = script.entries.get(idx) else { return false };
    let mut vals: BTreeMap<String, String> = script.extra_values(&d.extra);
    match note {
        Some(n) => { vals.insert(KEY.to_string(), n.to_string()); }
        None => { vals.remove(KEY); }
    }
    // intern_extra/clean_extradata yalnızca Extradata girdisi ekler/siler; bölüm olaylardan önce de
    // olabilir. Satırı, öncesindeki Extradata dışı girdi sayısıyla yeniden bul.
    let k = script.entries[..idx].iter().filter(|e| !is_extra(e)).count();
    let new_ids = script.intern_extra(&vals);
    let at = locate(script, k);
    if let Some(Entry::Dialogue(d)) = script.entries.get_mut(at) {
        d.extra = new_ids;
    }
    script.clean_extradata();
    true
}

fn is_extra(e: &Entry) -> bool {
    e.section() == crate::extradata::SEC_EXTRADATA
}

/// Öncesinde `k` tane Extradata dışı girdi bulunan Extradata dışı ilk girdinin indeksi.
fn locate(script: &Script, k: usize) -> usize {
    let mut n = 0;
    for (i, e) in script.entries.iter().enumerate() {
        if is_extra(e) { continue; }
        if n == k { return i; }
        n += 1;
    }
    script.entries.len()
}

/// Tüm yer imleri: (girdi indeksi, not), dosya sırasıyla.
pub fn list(script: &Script) -> Vec<(usize, String)> {
    let all = script.extradata();
    script.entries.iter().enumerate().filter_map(|(i, e)| {
        let Entry::Dialogue(d) = e else { return None };
        d.extra.iter().rev().find_map(|id| all.get(id).filter(|(k, _)| k == KEY).map(|(_, v)| (i, v.clone())))
    }).collect()
}

/// `from`dan sonraki (ya da önceki) yer imi; sona gelince başa sarar.
pub fn next(script: &Script, from: usize, forward: bool) -> Option<usize> {
    let l: Vec<usize> = list(script).into_iter().map(|x| x.0).collect();
    if forward {
        l.iter().copied().find(|&i| i > from).or(l.first().copied())
    } else {
        l.iter().rev().copied().find(|&i| i < from).or(l.last().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn sc(n: usize) -> Script {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for k in 0..n {
            s.entries.push(Entry::Dialogue(Dialogue { start_time: k as i64 * 1000, end_time: k as i64 * 1000 + 900, text: format!("satır {k}"), ..Default::default() }));
        }
        s
    }

    #[test]
    fn koy_kaldir_gez() {
        let mut s = sc(5);
        let d: Vec<usize> = s.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
        assert!(set(&mut s, d[1], Some("çeviri kontrol")));
        assert!(set(&mut s, d[3], Some("")));
        assert_eq!(get(&s, d[1]).as_deref(), Some("çeviri kontrol"));
        assert_eq!(get(&s, d[0]), None);
        assert_eq!(list(&s), vec![(d[1], "çeviri kontrol".into()), (d[3], String::new())]);
        assert_eq!(next(&s, d[1], true), Some(d[3]));
        assert_eq!(next(&s, d[3], true), Some(d[1]));
        assert_eq!(next(&s, d[1], false), Some(d[3]));
        assert_eq!(ids(&s).len(), 2);
        // kaydet/aç gidiş-dönüşü
        let t = s.to_ass_string();
        let s2 = Script::parse(&t);
        assert_eq!(list(&s2).len(), 2);
        // not değiştir, kaldır: kullanılmayan Extradata temizlenir
        set(&mut s, d[1], Some("yeni not"));
        assert_eq!(get(&s, d[1]).as_deref(), Some("yeni not"));
        set(&mut s, d[1], None);
        set(&mut s, d[3], None);
        assert!(list(&s).is_empty());
        assert!(s.extradata().is_empty());
    }

    #[test]
    fn extradata_once_gelirse() {
        // [Aegisub Extradata] olaylardan önce: satır indeksi kayar
        let src = "[Script Info]\nScriptType: v4.00+\n\n[Aegisub Extradata]\nData: 1,x,ex\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{=1}a\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,b\n";
        let mut s = Script::parse(src);
        let b = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(d) if d.text == "b")).unwrap();
        set(&mut s, b, Some("not"));
        let l = list(&s);
        assert_eq!(l.len(), 1);
        assert!(matches!(&s.entries[l[0].0], Entry::Dialogue(d) if d.text == "b"));
        let a = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(d) if d.text == "a")).unwrap();
        let Entry::Dialogue(d) = &s.entries[a] else { panic!() };
        assert_eq!(s.extra_values(&d.extra).get("x").map(|v| v.as_str()), Some("x"));
    }

    #[test]
    fn diger_extradata_korunur() {
        let mut s = sc(2);
        let i = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        let mut v = BTreeMap::new();
        v.insert("a-mo".to_string(), "veri".to_string());
        let id = s.intern_extra(&v);
        if let Entry::Dialogue(d) = &mut s.entries[i] { d.extra = id; }
        set(&mut s, i, Some("x"));
        set(&mut s, i, None);
        let Entry::Dialogue(d) = &s.entries[i] else { panic!() };
        assert_eq!(s.extra_values(&d.extra).get("a-mo").map(|s| s.as_str()), Some("veri"));
    }
}
