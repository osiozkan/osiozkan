//! Japonca saygı ekleri (-san, -kun, -chan…) çeviride: hangi adın hangi ekle kaç kez geçtiğini
//! listeler (ekip kararına uyum ve tutarlılık için) ve istenirse ekleri toplu kaldırır.
//! Yalnızca kısa çizgiyle ada bitişik ekler sayılır ("Hinata-kun"); tek başına "Sensei" dokunulmaz.
use crate::ass::{Entry, Script};

pub const SUFFIXES: &[&str] = &["san", "kun", "chan", "sama", "senpai", "sempai", "sensei", "dono", "tan", "chin", "nee", "nii", "neesan", "niisan", "neechan", "niichan", "han", "shi", "jou", "hime"];

/// Metindeki (ad, ek) çiftleri ve bayt aralıkları (çizgi dahil eki kapsar).
fn scan(text: &str) -> Vec<(String, String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let b: Vec<(usize, char)> = text.char_indices().collect();
    let mut in_tag = false;
    for (k, &(i, c)) in b.iter().enumerate() {
        match c {
            '{' => in_tag = true,
            '}' => in_tag = false,
            _ => {}
        }
        if in_tag || c != '-' || k == 0 || !b[k - 1].1.is_alphabetic() {
            continue;
        }
        // ek: çizgiden sonra harfler
        let mut e = k + 1;
        while e < b.len() && b[e].1.is_alphabetic() {
            e += 1;
        }
        let suf: String = b[k + 1..e].iter().map(|x| x.1).collect::<String>().to_lowercase();
        if !SUFFIXES.contains(&suf.as_str()) {
            continue;
        }
        let mut s = k;
        while s > 0 && b[s - 1].1.is_alphabetic() {
            s -= 1;
        }
        let name: String = b[s..k].iter().map(|x| x.1).collect();
        if !name.chars().next().is_some_and(|c| c.is_uppercase()) {
            continue; // "o-nee" gibi küçük harfli parçalar ad değildir
        }
        let end = if e < b.len() { b[e].0 } else { text.len() };
        out.push((name, suf, i..end));
    }
    out
}

/// Eki kaldırılmış metin ("Hinata-kun geldi" → "Hinata geldi").
pub fn strip(text: &str) -> String {
    let mut out = text.to_string();
    for (_, _, r) in scan(text).into_iter().rev() {
        out.replace_range(r, "");
    }
    out
}

/// Ad başına kullanım: (ad, [(ek ya da "" = eksiz, sayı)]). Yalnızca en az bir kez ekle geçen adlar.
pub fn usage(script: &Script) -> Vec<(String, Vec<(String, usize)>)> {
    let mut m: std::collections::BTreeMap<String, std::collections::BTreeMap<String, usize>> = Default::default();
    let texts: Vec<&str> = script.dialogues().filter(|d| !d.comment).map(|d| d.text.as_str()).collect();
    for t in &texts {
        for (n, s, _) in scan(t) {
            *m.entry(n).or_default().entry(s).or_default() += 1;
        }
    }
    // eksiz geçişler: ekin kaldırılmış metninde ad bütün sözcük olarak
    for t in &texts {
        let mut rest = t.to_string();
        for (n, _, r) in scan(t).into_iter().rev() {
            rest.replace_range(r.start - n.len()..r.end, " ");
        }
        let plain = crate::tags::strip_tags(&rest);
        for w in plain.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
            if let Some(e) = m.get_mut(w) {
                *e.entry(String::new()).or_default() += 1;
            }
        }
    }
    m.into_iter().map(|(n, v)| (n, v.into_iter().collect())).collect()
}

/// Tüm diyaloglardan ekleri kaldırır (yorumlar hariç); değişen satır sayısı.
pub fn strip_all(script: &mut Script) -> usize {
    let mut n = 0;
    for e in script.entries.iter_mut() {
        if let Entry::Dialogue(d) = e {
            if d.comment {
                continue;
            }
            let t = strip(&d.text);
            if t != d.text {
                d.text = t;
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    #[test]
    fn strips_only_name_suffixes() {
        assert_eq!(strip("Hinata-kun, bekle! Sensei geldi. Kageyama-SAMA {\\i1}Yachi-chan{\\i0}"), "Hinata, bekle! Sensei geldi. Kageyama {\\i1}Yachi{\\i0}");
        assert_eq!(strip("iki-üç kişi, Tanaka-senpai'nin"), "iki-üç kişi, Tanaka'nin");
        assert_eq!(strip("{\\fn-san}yok"), "{\\fn-san}yok");
        assert_eq!(strip("o-nee bir şey"), "o-nee bir şey");
    }

    #[test]
    fn usage_counts_with_and_without_suffix() {
        let mk = |t: &str| Dialogue { text: t.into(), end_time: 1, ..Default::default() };
        let mut s = Script::from_dialogues(vec![mk("Hinata-kun!"), mk("Hinata, koş."), mk("Hinata-kun ve Kageyama"), mk("Yachi-san"), mk("Kageyama-kun")]);
        let u = usage(&s);
        let h = &u.iter().find(|x| x.0 == "Hinata").unwrap().1;
        assert_eq!(h, &vec![(String::new(), 1), ("kun".to_string(), 2)]);
        let k = &u.iter().find(|x| x.0 == "Kageyama").unwrap().1;
        assert_eq!(k, &vec![(String::new(), 1), ("kun".to_string(), 1)]);
        assert_eq!(strip_all(&mut s), 4);
        assert!(usage(&s).is_empty());
    }
}
