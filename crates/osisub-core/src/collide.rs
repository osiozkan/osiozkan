//! Ekran çakışması: libass konumsuz satırları birbirinin üstüne bindirmez, ama `\pos`/`\move`'lu
//! tabela/tipografi satırları için bu korumayı yapmaz. Aynı anda görünen bir konumlu satır ile
//! bir diyalog satırı ekranda üst üste biniyorsa (ör. üstte `\an8` diyalog ile tabela) bildirilir.
//! Sınırlar gerçek çizimle ölçülür (`coverage::Bounds`; çizici dışarıdan verilir).
use crate::ass::{Dialogue, Entry, Script};
use crate::coverage::Bounds;
use crate::qc::{is_typeset, Issue, Kind};

/// Denetlenecek çiftler: (konumlu satırın girdisi, diyalog girdisi, ikisinin de göründüğü an ms).
/// Yorum ve boş satırlar atlanır; iki konumlu satır (bilinçli katmanlama) ya da iki konumsuz satır
/// (libass çarpışmayı kendisi önler) çift sayılmaz.
pub fn pairs(script: &Script) -> Vec<(usize, usize, i64)> {
    let mut lines: Vec<(usize, &Dialogue)> = script
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Entry::Dialogue(d) if !d.comment && d.end_time > d.start_time && (d.text.contains("\\p") || !crate::tags::strip_tags(&d.text).trim().is_empty()) => Some((i, d)),
            _ => None,
        })
        .collect();
    lines.sort_by_key(|(_, d)| d.start_time);
    let mut out = Vec::new();
    for (k, (i, a)) in lines.iter().enumerate() {
        for (j, b) in &lines[k + 1..] {
            if b.start_time >= a.end_time {
                break;
            }
            let (ta, tb) = (is_typeset(a), is_typeset(b));
            if ta == tb {
                continue;
            }
            let (s, e) = (a.start_time.max(b.start_time), a.end_time.min(b.end_time));
            if e <= s {
                continue;
            }
            let t = s + (e - s) / 2;
            if ta { out.push((*i, *j, t)) } else { out.push((*j, *i, t)) }
        }
    }
    out
}

/// Yalnızca `entry` satırını içeren betik (başlık, stiller, yazı tipleri korunur).
pub fn single_line(script: &Script, entry: usize) -> Script {
    let mut s = script.clone();
    let mut k = 0;
    s.entries.retain(|e| {
        let keep = !matches!(e, Entry::Dialogue(_)) || k == entry;
        k += 1;
        keep
    });
    s
}

fn intersect(a: &Bounds, b: &Bounds) -> Option<(i64, i64, i64, i64)> {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// Çiftleri çizerek denetler. `bounds_of(girdi, anlar)`: o satır tek başına çizildiğinde her an
/// için sınırlar (her satır bir kez çağrılır). En az 4×4 piksellik kesişim sorun sayılır;
/// sorun diyalog satırına yazılır.
pub fn check(script: &Script, mut bounds_of: impl FnMut(usize, &[i64]) -> Result<Vec<Option<Bounds>>, String>) -> Result<Vec<Issue>, String> {
    let ps = pairs(script);
    let mut need: std::collections::BTreeMap<usize, Vec<i64>> = Default::default();
    for &(a, b, t) in &ps {
        need.entry(a).or_default().push(t);
        need.entry(b).or_default().push(t);
    }
    let mut got: std::collections::HashMap<(usize, i64), Option<Bounds>> = Default::default();
    for (entry, mut times) in need {
        times.sort();
        times.dedup();
        let bs = bounds_of(entry, &times)?;
        for (t, b) in times.into_iter().zip(bs) {
            got.insert((entry, t), b);
        }
    }
    let line_no = |e: usize| script.entries[..e].iter().filter(|x| matches!(x, Entry::Dialogue(_))).count() + 1;
    let mut out: Vec<Issue> = Vec::new();
    for (sign, dlg, t) in ps {
        let (Some(Some(a)), Some(Some(b))) = (got.get(&(sign, t)), got.get(&(dlg, t))) else { continue };
        if let Some((x, y, w, h)) = intersect(a, b) {
            if w >= 4 && h >= 4 && !out.iter().any(|i| i.entry == dlg && i.message.contains(&format!("#{} ", line_no(sign)))) {
                out.push(Issue {
                    entry: dlg,
                    kind: Kind::ScreenCollision,
                    message: format!("Ekranda #{} (konumlu satır) ile üst üste biniyor ({}×{} px, {},{}; {})", line_no(sign), w, h, x, y, crate::time::format_ass(t)),
                });
            }
        }
    }
    out.sort_by_key(|i| i.entry);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(lines: &[(i64, i64, &str)]) -> Script {
        Script::from_dialogues(lines.iter().map(|&(s, e, t)| Dialogue { start_time: s, end_time: e, text: t.into(), ..Default::default() }).collect())
    }

    #[test]
    fn pairs_only_positioned_vs_free() {
        let s = sc(&[(0, 2000, "{\\an8}Diyalog"), (1000, 3000, "{\\pos(320,20)}TABELA"), (1500, 2500, "{\\pos(10,10)}başka tabela"), (5000, 6000, "{\\pos(1,1)}yalnız"), (1200, 1800, "alt diyalog")]);
        let base = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        let p = pairs(&s);
        // (tabela, diyalog, an): iki tabela birbiriyle, iki diyalog birbiriyle eşleşmez
        assert_eq!(p, vec![(base + 1, base, 1500), (base + 2, base, 1750), (base + 1, base + 4, 1500), (base + 2, base + 4, 1650)]);
    }

    #[test]
    fn reports_real_overlap_once_on_dialogue() {
        let s = sc(&[(0, 2000, "{\\an8}Diyalog"), (1000, 3000, "{\\pos(320,20)}TABELA"), (0, 3000, "{\\pos(10,400)}uzak tabela")]);
        let base = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        let bx = |x, y, w, h| Some(Bounds { x, y, w, h, hash: String::new(), solid: true });
        let mut calls = 0;
        let r = check(&s, |e, times| {
            calls += 1;
            Ok(times
                .iter()
                .map(|_| match e - base {
                    0 => bx(200, 10, 240, 40),
                    1 => bx(280, 5, 80, 30),
                    _ => bx(0, 390, 50, 20),
                })
                .collect())
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].entry, base);
        assert_eq!(r[0].kind, Kind::ScreenCollision);
        assert!(r[0].message.contains("#2 ") && r[0].message.contains("80×25"), "{}", r[0].message);
    }

    #[test]
    fn single_line_keeps_header() {
        let s = sc(&[(0, 1, "a"), (0, 1, "b"), (0, 1, "c")]);
        let e = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(d) if d.text == "b")).unwrap();
        let one = single_line(&s, e);
        assert_eq!(one.dialogues().map(|d| d.text.as_str()).collect::<Vec<_>>(), vec!["b"]);
        assert_eq!(one.styles().count(), s.styles().count());
    }
}
