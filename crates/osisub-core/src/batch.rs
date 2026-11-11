//! Subtitle Edit tarzı toplu satır işlemleri: aynı metinli ardışık satırları birleştir, kısa
//! satırları birleştir, süreleri okuma hızına göre yeniden hesapla. Yorum, tipografi, karaoke
//! ve fx satırlarına dokunulmaz (`qc::is_typeset/is_karaoke`).
use crate::ass::{Dialogue, Entry, Script};
use crate::{qc, tags};

fn plain_dialogue(d: &Dialogue) -> bool {
    !d.comment && !qc::is_typeset(d) && !qc::is_karaoke(d) && !d.effect.eq_ignore_ascii_case("fx")
}

/// `only`: işlenecek entry indeksleri (None = tümü). Ardışık çiftler (dosya sırasıyla, aynı stil)
/// `can(a, b)` doğruysa birleştirilir; `merge(a, b)` a'yı günceller. Birleştirilen sayısı.
fn merge_pairs(script: &mut Script, only: Option<&[usize]>, can: impl Fn(&Dialogue, &Dialogue) -> bool, merge: impl Fn(&mut Dialogue, &Dialogue)) -> usize {
    let allowed = |i: usize| only.map_or(true, |o| o.contains(&i));
    let mut remove = vec![false; script.entries.len()];
    let mut n = 0;
    let mut cur: Option<usize> = None;
    for i in 0..script.entries.len() {
        let Entry::Dialogue(d) = &script.entries[i] else { continue };
        if !allowed(i) || !plain_dialogue(d) {
            cur = None;
            continue;
        }
        if let Some(c) = cur {
            let (Entry::Dialogue(a), Entry::Dialogue(b)) = (&script.entries[c], &script.entries[i]) else { unreachable!() };
            if a.style == b.style && can(a, b) {
                let b = b.clone();
                if let Entry::Dialogue(a) = &mut script.entries[c] {
                    merge(a, &b);
                }
                remove[i] = true;
                n += 1;
                continue;
            }
        }
        cur = Some(i);
    }
    let mut k = 0;
    script.entries.retain(|_| {
        let keep = !remove[k];
        k += 1;
        keep
    });
    n
}

/// Aynı metni (etiketler dahil) taşıyan ve aralarında en çok `max_gap_ms` boşluk olan ardışık
/// satırları tek satırda birleştirir (OCR / otomatik altyazı çıktısında sık görülür).
pub fn merge_same_text(script: &mut Script, max_gap_ms: i64, only: Option<&[usize]>) -> usize {
    merge_pairs(
        script,
        only,
        |a, b| b.start_time - a.end_time <= max_gap_ms && b.start_time >= a.start_time && a.text.trim() == b.text.trim(),
        |a, b| a.end_time = a.end_time.max(b.end_time),
    )
}

/// Kısa ardışık satırları (görünen uzunlukları toplamı ≤ `max_chars`, boşluk ≤ `max_gap_ms`,
/// ikisi de tek satır) `\N` ile birleştirir.
pub fn merge_short_lines(script: &mut Script, max_chars: usize, max_gap_ms: i64, only: Option<&[usize]>) -> usize {
    merge_pairs(
        script,
        only,
        |a, b| {
            let (va, vb) = (tags::visible_text(&a.text), tags::visible_text(&b.text));
            b.start_time >= a.end_time
                && b.start_time - a.end_time <= max_gap_ms
                && !va.contains('\n')
                && !vb.contains('\n')
                && va.trim().chars().count() + vb.trim().chars().count() <= max_chars
                && a.actor == b.actor
        },
        |a, b| {
            a.text = format!("{}\\N{}", a.text.trim_end(), b.text.trim_start());
            a.end_time = a.end_time.max(b.end_time);
        },
    )
}

/// Süreleri okuma hızına göre yeniden hesaplar: bitiş = başlangıç + uzunluk/cps, [min, max]
/// aralığında; aynı stildeki bir sonraki satıra `gap_ms` kalana kadar uzar, asla taşmaz.
/// Yalnızca uzatır (`extend_only`) ya da kısaltır da. Değişen satır sayısı.
pub fn recalc_durations(script: &mut Script, cps: f64, min_ms: i64, max_ms: i64, gap_ms: i64, extend_only: bool, only: Option<&[usize]>) -> usize {
    let allowed = |i: usize| only.map_or(true, |o| o.contains(&i));
    // aynı stilde, zamanca sonraki satırın başlangıcı
    let mut by_style: std::collections::HashMap<String, Vec<(i64, usize)>> = std::collections::HashMap::new();
    for (i, e) in script.entries.iter().enumerate() {
        if let Entry::Dialogue(d) = e {
            if !d.comment {
                by_style.entry(d.style.clone()).or_default().push((d.start_time, i));
            }
        }
    }
    let mut next_start: std::collections::HashMap<usize, i64> = std::collections::HashMap::new();
    for v in by_style.values_mut() {
        v.sort_unstable();
        for w in v.windows(2) {
            next_start.insert(w[0].1, w[1].0);
        }
    }
    let mut n = 0;
    for (i, e) in script.entries.iter_mut().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if !allowed(i) || !plain_dialogue(d) {
            continue;
        }
        let len = tags::visible_len(&d.text) as f64;
        let want = ((len / cps.max(1.0)) * 1000.0).round() as i64;
        let want = want.clamp(min_ms, max_ms.max(min_ms));
        let mut end = d.start_time + want;
        if let Some(&ns) = next_start.get(&i) {
            if ns > d.start_time {
                end = end.min(ns - gap_ms);
            }
        }
        // "yalnızca uzat": mevcut bitişten kısa sonuç (ör. zaten çakışan satır) hiç uygulanmaz
        if extend_only && end <= d.end_time {
            continue;
        }
        if end > d.start_time && end != d.end_time {
            d.end_time = end;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(lines: &[(i64, i64, &str)]) -> Script {
        Script::from_dialogues(lines.iter().map(|&(a, b, t)| Dialogue { start_time: a, end_time: b, text: t.into(), style: "Default".into(), ..Default::default() }).collect())
    }
    fn dump(s: &Script) -> Vec<(i64, i64, String)> {
        s.dialogues().map(|d| (d.start_time, d.end_time, d.text.clone())).collect()
    }

    #[test]
    fn same_text() {
        let mut x = s(&[(0, 1000, "Merhaba"), (1000, 2000, "Merhaba"), (2100, 3000, "{\\i1}Merhaba"), (5000, 6000, "Merhaba"), (6000, 7000, "Başka")]);
        assert_eq!(merge_same_text(&mut x, 200, None), 1);
        // italik satır (etiket farklı) birleşmez; boşluk > 200 ms olan da
        assert_eq!(dump(&x), vec![(0, 2000, "Merhaba".into()), (2100, 3000, "{\\i1}Merhaba".into()), (5000, 6000, "Merhaba".into()), (6000, 7000, "Başka".into())]);
    }

    #[test]
    fn short_lines() {
        let mut x = s(&[(0, 1000, "Evet."), (1100, 2000, "Hayır."), (2050, 3000, "Bu satır uzun olduğu için birleşmemeli"), (3000, 3500, "Tamam"), (3500, 4000, "{\\pos(1,1)}Tabela")]);
        assert_eq!(merge_short_lines(&mut x, 20, 300, None), 1);
        assert_eq!(dump(&x)[0], (0, 2000, "Evet.\\NHayır.".into()));
        assert_eq!(x.dialogues().count(), 4);
    }

    #[test]
    fn recalc() {
        // 20 kar/sn: 10 karakter → 500 ms → min 1000'e çıkar; 60 karakter → 3000 ms ama sonraki satıra 100 ms kala durur
        let mut x = s(&[(0, 300, "0123456789"), (2000, 2100, &"x".repeat(60)), (4000, 4500, "son")]);
        assert_eq!(recalc_durations(&mut x, 20.0, 1000, 7000, 100, true, None), 3);
        let d = dump(&x);
        assert_eq!((d[0].1, d[1].1, d[2].1), (1000, 3900, 5000));
        // yalnızca uzat: zaten sonraki satıra taşan satır kısaltılmaz
        let mut z = s(&[(0, 3200, "Günaydın! Bugün hava çok güzel, değil mi?"), (3000, 5000, "x")]);
        recalc_durations(&mut z, 15.0, 1000, 7000, 84, true, None);
        assert_eq!(dump(&z)[0].1, 3200);
        // kısaltma da serbestse uzun satır kısalır
        let mut y = s(&[(0, 9000, "kısa")]);
        recalc_durations(&mut y, 20.0, 1000, 7000, 100, false, None);
        assert_eq!(dump(&y)[0].1, 1000);
    }
}
