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

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace("\\N", " / ")
}

/// Sözcük düzeyinde fark (LCS): `<del>` silinen, `<ins>` eklenen sözcükler.
pub fn word_diff_html(a: &str, b: &str) -> String {
    let (x, y): (Vec<&str>, Vec<&str>) = (a.split_whitespace().collect(), b.split_whitespace().collect());
    if x.len() * y.len() > 250_000 {
        return format!("<del>{}</del> <ins>{}</ins>", esc(a), esc(b));
    }
    let mut l = vec![vec![0u16; y.len() + 1]; x.len() + 1];
    for i in (0..x.len()).rev() {
        for j in (0..y.len()).rev() {
            l[i][j] = if x[i] == y[j] { l[i + 1][j + 1] + 1 } else { l[i + 1][j].max(l[i][j + 1]) };
        }
    }
    // (tür, sözcük): 0 aynı, 1 silinen, 2 eklenen — eşitlikte önce silinen
    let (mut i, mut j, mut ops): (usize, usize, Vec<(u8, &str)>) = (0, 0, Vec::new());
    while i < x.len() || j < y.len() {
        if i < x.len() && j < y.len() && x[i] == y[j] {
            ops.push((0, x[i]));
            i += 1;
            j += 1;
        } else if i < x.len() && (j == y.len() || l[i + 1][j] >= l[i][j + 1]) {
            ops.push((1, x[i]));
            i += 1;
        } else {
            ops.push((2, y[j]));
            j += 1;
        }
    }
    // ardışık aynı türdeki sözcükleri tek blokta topla
    let mut out: Vec<String> = vec![];
    let mut k = 0;
    while k < ops.len() {
        let kind = ops[k].0;
        let mut e = k;
        while e < ops.len() && ops[e].0 == kind { e += 1; }
        let words = ops[k..e].iter().map(|o| esc(o.1)).collect::<Vec<_>>().join(" ");
        out.push(match kind { 0 => words, 1 => format!("<del>{words}</del>"), _ => format!("<ins>{words}</ins>") });
        k = e;
    }
    out.join(" ")
}

/// Karşılaştırma raporu (tek dosyalık HTML): editör değişikliklerini çevirmene göndermek için.
pub fn report_html(title: &str, old: &Script, new: &Script, changes: &[Change]) -> String {
    let d = |s: &Script, i: usize| match s.entries.get(i) { Some(Entry::Dialogue(d)) => Some(d.clone()), _ => None };
    let t = |d: &Dialogue| format!("{}–{}", crate::time::format_ass(d.start_time), crate::time::format_ass(d.end_time));
    let mut rows = String::new();
    let (mut a, mut r, mut m) = (0, 0, 0);
    for c in changes {
        match c {
            Change::Added { new: n } => if let Some(x) = d(new, *n) { a += 1; rows.push_str(&format!("<tr class=a><td>+</td><td>{}</td><td><ins>{}</ins></td></tr>\n", t(&x), esc(&crate::tags::visible_text(&x.text)))); },
            Change::Removed { old: o } => if let Some(x) = d(old, *o) { r += 1; rows.push_str(&format!("<tr class=r><td>−</td><td>{}</td><td><del>{}</del></td></tr>\n", t(&x), esc(&crate::tags::visible_text(&x.text)))); },
            Change::Modified { old: o, new: n, text, timing, style, actor } => if let (Some(x), Some(y)) = (d(old, *o), d(new, *n)) {
                m += 1;
                let mut what = vec![];
                if *timing { what.push(format!("zaman {} → {}", t(&x), t(&y))); }
                if *style { what.push(format!("stil {} → {}", esc(&x.style), esc(&y.style))); }
                if *actor { what.push(format!("konuşan {} → {}", esc(&x.actor), esc(&y.actor))); }
                let body = if *text { word_diff_html(&crate::tags::visible_text(&x.text), &crate::tags::visible_text(&y.text)) } else { esc(&crate::tags::visible_text(&y.text)) };
                let note = if what.is_empty() { String::new() } else { format!("<div class=n>{}</div>", what.join(" · ")) };
                rows.push_str(&format!("<tr class=m><td>~</td><td>{}</td><td>{body}{note}</td></tr>\n", t(&y)));
            },
        }
    }
    format!("<!doctype html><html lang=tr><meta charset=utf-8><title>{t0}</title><style>body{{font:14px system-ui;margin:24px;background:#fff;color:#222}}table{{border-collapse:collapse;width:100%}}td{{padding:4px 8px;border-bottom:1px solid #ddd;vertical-align:top}}td:nth-child(2){{white-space:nowrap;font-family:monospace;color:#666}}ins{{background:#d4f7d4;text-decoration:none}}del{{background:#fbd5d5}}.n{{color:#a66;font-size:12px}}@media(prefers-color-scheme:dark){{body{{background:#1b1b1b;color:#ddd}}td{{border-color:#333}}ins{{background:#1f4d27}}del{{background:#5a2323}}}}</style><h1>{t0}</h1><p>+{a} eklendi · −{r} silindi · ~{m} değişti</p><table>{rows}</table></html>", t0 = esc(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sozcuk_farki_ve_rapor() {
        assert_eq!(word_diff_html("Bugün hava çok güzel", "Bugün hava gerçekten çok güzel"), "Bugün hava <ins>gerçekten</ins> çok güzel");
        assert_eq!(word_diff_html("a <b>", "a c"), "a <del>&lt;b&gt;</del> <ins>c</ins>");
        assert_eq!(word_diff_html("bir iki üç", "dört beş"), "<del>bir iki üç</del> <ins>dört beş</ins>");
        let old = Script::from_dialogues(vec![Dialogue { start_time: 0, end_time: 1000, text: "Merhaba dunya".into(), ..Default::default() }, Dialogue { start_time: 5000, end_time: 6000, text: "Silinen".into(), ..Default::default() }]);
        let new = Script::from_dialogues(vec![Dialogue { start_time: 0, end_time: 1000, text: "Merhaba dünya".into(), ..Default::default() }]);
        let ch = compare(&old, &new);
        let h = report_html("Bölüm 1", &old, &new, &ch);
        assert!(h.contains("Merhaba <del>dunya</del> <ins>dünya</ins>"), "{h}");
        assert!(h.contains("<del>Silinen</del>") && h.contains("−1 silindi"));
    }

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
