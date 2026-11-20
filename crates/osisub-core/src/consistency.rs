//! Çeviri tutarlılığı: referansta (kaynak altyazı) aynı olan satırlar farklı çevrilmişse
//! (tekrarlanan replikler, şarkı sözleri, büyü/teknik adları) azınlıktaki çevirileri not olarak
//! gösterir. Bağlama göre değişebilecek kısa satırlar ("What?", "Yes.") atlanır: kaynak en az 3
//! sözcük olmalı. Satırlar referansa zaman örtüşmesiyle (en çok örtüşen) eşlenir.
use crate::ass::{Entry, Script};
use crate::qc::{is_karaoke, is_typeset, Issue, Kind};
use std::collections::HashMap;

fn norm(s: &str) -> String {
    crate::tags::visible_text(s).to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn check(script: &Script, reference: Option<&Script>) -> Vec<Issue> {
    let Some(reference) = reference else { return vec![] };
    let mut refs: Vec<(i64, i64, String, String)> = reference.dialogues().filter(|d| !d.comment).map(|d| (d.start_time, d.end_time, norm(&d.text), crate::tags::visible_text(&d.text).replace('\n', " "))).collect();
    refs.sort_by_key(|r| r.0);
    // kaynak → (çeviri biçimi → satırlar)
    let mut groups: HashMap<String, (String, HashMap<String, (String, Vec<usize>)>)> = HashMap::new();
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment || is_typeset(d) || is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx") {
            continue;
        }
        let end = refs.partition_point(|r| r.0 < d.end_time);
        let best = refs[..end].iter().rev().take(10).filter(|r| r.1 > d.start_time).max_by_key(|r| r.1.min(d.end_time) - r.0.max(d.start_time));
        let Some(r) = best else { continue };
        if r.2.split_whitespace().count() < 3 {
            continue;
        }
        let t = norm(&d.text);
        if t.is_empty() || t == r.2 {
            continue; // boş ya da çevrilmemiş (ayrı denetim)
        }
        let g = groups.entry(r.2.clone()).or_insert_with(|| (r.3.clone(), HashMap::new()));
        let v = g.1.entry(t).or_insert_with(|| (crate::tags::visible_text(&d.text).replace('\n', " "), vec![]));
        v.1.push(i);
    }
    let mut out = vec![];
    for (_, (src, variants)) in groups {
        if variants.len() < 2 {
            continue;
        }
        // en sık biçim "doğru" kabul edilir; eşitlikte metin sırası (kararlı)
        let mut vs: Vec<(&String, &(String, Vec<usize>))> = variants.iter().collect();
        vs.sort_by(|a, b| b.1 .1.len().cmp(&a.1 .1.len()).then(a.0.cmp(b.0)));
        let (main_text, main_lines) = (&vs[0].1 .0, vs[0].1 .1.len());
        for (_, (_, lines)) in &vs[1..] {
            for &l in lines {
                out.push(Issue { entry: l, kind: Kind::Note, message: format!("Tutarsız çeviri: aynı kaynak (\"{src}\") başka yerde {main_lines} kez \"{main_text}\"") });
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

    fn d(a: i64, b: i64, t: &str) -> Dialogue {
        Dialogue { start_time: a, end_time: b, text: t.into(), ..Default::default() }
    }

    #[test]
    fn tutarsiz_ceviri() {
        let en = Script::from_dialogues(vec![
            d(0, 1000, "I will become the Hokage!"),
            d(5000, 6000, "I will become the Hokage!"),
            d(9000, 10000, "I will become the Hokage!"),
            d(12000, 13000, "What?"),
            d(14000, 15000, "What?"),
        ]);
        let tr = Script::from_dialogues(vec![
            d(0, 1000, "Hokage olacağım!"),
            d(5000, 6000, "{\\i1}Hokage olacağım!{\\i0}"),
            d(9100, 9900, "Ben Hokage olacağım!"),
            d(12000, 13000, "Ne?"),
            d(14000, 15000, "Efendim?"),
        ]);
        let v = check(&tr, Some(&en));
        assert_eq!(v.len(), 1, "{:?}", v.iter().map(|i| &i.message).collect::<Vec<_>>());
        let third = tr.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).nth(2).unwrap().0;
        assert_eq!(v[0].entry, third);
        assert!(v[0].message.contains("2 kez \"Hokage olacağım!\""));
        assert!(check(&tr, None).is_empty());
    }
}
