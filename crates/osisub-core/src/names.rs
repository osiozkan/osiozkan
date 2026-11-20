//! Ad tutarlılığı denetimi: belgede sık geçen bir özel adın (karakter, yer) tek tük geçen ve bir
//! harf farklı yazımlarını bulur ("Hinata" ×40 ↔ "Hinta" ×1). Türkçe eklerde kesme işaretinden
//! (Hinata'ya), saygı eklerinde tireden (Hinata-kun) önceki kısım alınır.
//!
//! Yanlış alarmı azaltmak için: aday ad küçük harfle hiç geçmemeli (cümle başındaki sıradan
//! sözcükler elenir), sık biçim en az bir kez cümle ortasında büyük harfle geçmeli, sık biçim
//! en az 4 kez ve seyrek biçimin en az 4 katı geçmeli; tipografi/karaoke/yorum satırları atlanır.
use crate::ass::{Entry, Script};
use crate::qc::{Issue, Kind};
use std::collections::HashMap;

#[derive(Default)]
struct Stat {
    count: usize,
    mid: bool,
    lines: Vec<usize>,
}

fn lev(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// (sözcük, cümle ortasında mı) — sözcükler kesme işareti/tire öncesi gövdedir.
fn words(text: &str) -> Vec<(String, bool)> {
    let mut out = vec![];
    let mut sentence_start = true;
    let mut cur = String::new();
    let mut cut = false; // kesme/tire sonrası: gövde bitti, ek atlanıyor
    let flush = |cur: &mut String, out: &mut Vec<(String, bool)>, start: bool| {
        if !cur.is_empty() {
            out.push((std::mem::take(cur), !start));
        }
    };
    for c in text.chars() {
        if c.is_alphabetic() {
            if !cut {
                cur.push(c);
            }
        } else if (c == '\'' || c == '’' || c == '-') && !cur.is_empty() {
            cut = true;
        } else {
            let was_word = !cur.is_empty();
            flush(&mut cur, &mut out, sentence_start);
            cut = false;
            if was_word {
                sentence_start = false;
            }
            if matches!(c, '.' | '!' | '?' | '…' | '\n' | '-' | '—' | '"' | '“' | '«') {
                sentence_start = true;
            }
        }
    }
    flush(&mut cur, &mut out, sentence_start);
    out
}

/// Ad tutarsızlıkları: seyrek biçimin geçtiği her satır için bir not.
pub fn check(script: &Script) -> Vec<Issue> {
    let mut caps: HashMap<String, Stat> = HashMap::new();
    let mut lower: std::collections::HashSet<String> = Default::default();
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment || crate::qc::is_typeset(d) || crate::qc::is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx") {
            continue;
        }
        for (w, mid) in words(&crate::tags::visible_text(&d.text)) {
            let first = w.chars().next().unwrap();
            if first.is_uppercase() {
                if w.chars().count() < 4 || w.chars().all(|c| c.is_uppercase()) {
                    continue;
                }
                let s = match caps.get_mut(&w) { Some(s) => s, None => caps.entry(w).or_default() };
                s.count += 1;
                s.mid |= mid;
                if s.lines.last() != Some(&i) {
                    s.lines.push(i);
                }
            } else if w.chars().count() >= 4 && !lower.contains(&w) {
                lower.insert(w);
            }
        }
    }
    let is_lower_word = |w: &str| lower.contains(&crate::qc::tr_lower(w));
    let names: Vec<(&String, &Stat)> = caps.iter().filter(|(w, _)| !is_lower_word(w)).collect();
    let mut out = vec![];
    for (rare, rs) in &names {
        let rc: Vec<char> = rare.chars().collect();
        let best = names.iter()
            .filter(|(w, s)| w != rare && s.mid && s.count >= 4 && s.count >= rs.count * 4)
            .filter(|(w, _)| { let wc: Vec<char> = w.chars().collect(); wc.len().abs_diff(rc.len()) <= 1 && lev(&wc, &rc) == 1 })
            .max_by_key(|(w, s)| (s.count, std::cmp::Reverse((*w).clone())));
        if let Some((freq, fs)) = best {
            for &l in &rs.lines {
                out.push(Issue { entry: l, kind: Kind::Note, message: format!("Ad tutarsız olabilir: \"{rare}\" ({} kez) — belgede {} kez \"{freq}\"", rs.count, fs.count) });
            }
        }
    }
    out.sort_by_key(|x| x.entry);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn sc(lines: &[&str]) -> Script {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for (k, t) in lines.iter().enumerate() {
            s.entries.push(Entry::Dialogue(Dialogue { start_time: k as i64 * 1000, end_time: k as i64 * 1000 + 900, text: t.to_string(), ..Default::default() }));
        }
        s
    }

    #[test]
    fn sozcukler() {
        let w = words("Hinata'ya bak, Kageyama-kun! Bunu Hinata yaptı.");
        assert_eq!(w, vec![("Hinata".into(), false), ("bak".into(), true), ("Kageyama".into(), true), ("Bunu".into(), false), ("Hinata".into(), true), ("yaptı".into(), true)]);
    }

    #[test]
    fn tutarsiz_ad() {
        let mut l = vec!["Bence Hinata haklı.", "Hinata'yı gördün mü?", "Sen de mi Hinata?", "Gel, Hinata!", "Sana Hinata'dan bahsettim."];
        l.push("Hinta nerede?"); // yazım hatası
        l.push("Bana bak."); // Bana/Sana: sıradan sözcükler (küçük harfle de geçer)
        l.push("Sana söyledim, bana inan, sana da.");
        let s = sc(&l);
        let v = check(&s);
        assert_eq!(v.len(), 1, "{:?}", v.iter().map(|i| &i.message).collect::<Vec<_>>());
        assert!(v[0].message.contains("\"Hinta\"") && v[0].message.contains("5 kez \"Hinata\""));
        let hinta = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(d) if d.text.starts_with("Hinta"))).unwrap();
        assert_eq!(v[0].entry, hinta);
        // eşit sıklıkta iki biçim (iki farklı karakter: Yui / Yuki gibi) uyarılmaz
        let s = sc(&["Haruka geldi.", "Ben Haruka.", "Haruki burada.", "Sen Haruki misin?"]);
        assert!(check(&s).is_empty());
    }
}
