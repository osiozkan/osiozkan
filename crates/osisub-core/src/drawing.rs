//! ASS çizim komutları (`m`, `n`, `l`, `b`, `s`, `p`, `c`) — vektör `\clip` ve `\p1` çizimleri için.
//! Komut yapısı korunur; yalnızca nokta koordinatları düzenlenir.

#[derive(Clone, Debug, PartialEq)]
pub struct Cmd {
    /// 'm' 'n' 'l' 'b' 's' 'p' 'c'
    pub kind: char,
    pub pts: Vec<(f64, f64)>,
}

/// Çizim metnini komutlara ayırır. Bilinmeyen sözcükler atlanır; tek kalan sayı yok sayılır.
pub fn parse(s: &str) -> Vec<Cmd> {
    let mut out: Vec<Cmd> = vec![];
    let mut nums: Vec<f64> = vec![];
    let flush = |out: &mut Vec<Cmd>, nums: &mut Vec<f64>| {
        if let Some(c) = out.last_mut() {
            for p in nums.chunks_exact(2) {
                c.pts.push((p[0], p[1]));
            }
        }
        nums.clear();
    };
    for tok in s.split_whitespace() {
        let t = tok.to_ascii_lowercase();
        if t.len() == 1 && "mnlbspc".contains(t.as_str()) {
            flush(&mut out, &mut nums);
            out.push(Cmd { kind: t.chars().next().unwrap(), pts: vec![] });
        } else if let Ok(v) = tok.parse::<f64>() {
            nums.push(v);
        }
    }
    flush(&mut out, &mut nums);
    out
}

fn num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{}", r)
    }
}

pub fn to_string(cmds: &[Cmd]) -> String {
    let mut parts: Vec<String> = vec![];
    for c in cmds {
        parts.push(c.kind.to_string());
        for &(x, y) in &c.pts {
            parts.push(num(x));
            parts.push(num(y));
        }
    }
    parts.join(" ")
}

/// Tüm noktalar (komut indeksi, nokta indeksi, konum) — düzenleme tutamaçları için.
pub fn points(cmds: &[Cmd]) -> Vec<(usize, usize, (f64, f64))> {
    cmds.iter().enumerate().flat_map(|(ci, c)| c.pts.iter().enumerate().map(move |(pi, &p)| (ci, pi, p))).collect()
}

/// Çokgen yaklaşımı (b eğrileri örneklenir) — ekranda çizmek için. Her `m` yeni bir şekil başlatır.
pub fn polylines(cmds: &[Cmd]) -> Vec<Vec<(f64, f64)>> {
    let mut shapes: Vec<Vec<(f64, f64)>> = vec![];
    let mut cur: Vec<(f64, f64)> = vec![];
    for c in cmds {
        match c.kind {
            'm' => {
                if cur.len() > 1 {
                    shapes.push(std::mem::take(&mut cur));
                }
                cur.clear();
                if let Some(&p) = c.pts.last() {
                    cur.push(p);
                }
            }
            'n' => {
                if let Some(&p) = c.pts.last() {
                    cur.push(p);
                }
            }
            'b' => {
                for seg in c.pts.chunks_exact(3) {
                    let p0 = cur.last().copied().unwrap_or(seg[0]);
                    for k in 1..=12 {
                        let t = k as f64 / 12.0;
                        let u = 1.0 - t;
                        let x = u * u * u * p0.0 + 3.0 * u * u * t * seg[0].0 + 3.0 * u * t * t * seg[1].0 + t * t * t * seg[2].0;
                        let y = u * u * u * p0.1 + 3.0 * u * u * t * seg[0].1 + 3.0 * u * t * t * seg[1].1 + t * t * t * seg[2].1;
                        cur.push((x, y));
                    }
                }
            }
            _ => cur.extend(c.pts.iter().copied()),
        }
    }
    if cur.len() > 1 {
        shapes.push(cur);
    }
    shapes
}

/// Noktayı sona ekler: çizim boşsa `m`, değilse son komut `l` ise ona, değilse yeni `l`.
pub fn push_point(cmds: &mut Vec<Cmd>, p: (f64, f64)) {
    match cmds.last_mut() {
        None => cmds.push(Cmd { kind: 'm', pts: vec![p] }),
        Some(c) if c.kind == 'l' => c.pts.push(p),
        Some(_) => cmds.push(Cmd { kind: 'l', pts: vec![p] }),
    }
}

/// Yeni şekil (kontur) başlatır: `m` noktası (Aegisub vektör kırpma "yeni şekil" kipi).
pub fn push_move(cmds: &mut Vec<Cmd>, p: (f64, f64)) {
    cmds.push(Cmd { kind: 'm', pts: vec![p] });
}

fn bez(p0: (f64, f64), c1: (f64, f64), c2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * p3.0,
        u * u * u * p0.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * p3.1,
    )
}

/// Aegisub vektör kırpma "nokta ekle" kipi: `q`'ya `tol` uzaklıktan yakın en yakın parçayı o noktadan
/// ikiye böler (düz parçaya `l` noktası; eğri de Casteljau ile iki eğriye ayrılır, şekli değişmez).
/// Eklenen yoksa `false`.
pub fn insert_point(cmds: &mut Vec<Cmd>, q: (f64, f64), tol: f64) -> bool {
    let d2 = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2);
    // en iyi: (uzaklık², komut, nokta/parça indeksi, t, nokta)
    let mut best: Option<(f64, usize, usize, f64, (f64, f64))> = None;
    let mut cur: Option<(f64, f64)> = None;
    for (ci, c) in cmds.iter().enumerate() {
        match c.kind {
            'm' | 'n' => cur = c.pts.last().copied().or(cur),
            'l' => {
                for (pi, &b) in c.pts.iter().enumerate() {
                    if let Some(a) = cur {
                        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                        let len2 = dx * dx + dy * dy;
                        let t = if len2 > 0.0 { (((q.0 - a.0) * dx + (q.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                        let pt = (a.0 + dx * t, a.1 + dy * t);
                        let dd = d2(pt, q);
                        if t > 0.0 && t < 1.0 && best.map_or(true, |b| dd < b.0) {
                            best = Some((dd, ci, pi, t, pt));
                        }
                    }
                    cur = Some(b);
                }
            }
            'b' => {
                for (k, seg) in c.pts.chunks_exact(3).enumerate() {
                    if let Some(a) = cur {
                        for j in 1..48 {
                            let t = j as f64 / 48.0;
                            let pt = bez(a, seg[0], seg[1], seg[2], t);
                            let dd = d2(pt, q);
                            if best.map_or(true, |b| dd < b.0) {
                                best = Some((dd, ci, k, t, pt));
                            }
                        }
                    }
                    cur = Some(seg[2]);
                }
            }
            _ => cur = c.pts.last().copied().or(cur),
        }
    }
    let Some((dd, ci, pi, t, pt)) = best.filter(|b| b.0 <= tol * tol) else { return false };
    let _ = dd;
    let r = |p: (f64, f64)| (p.0.round(), p.1.round());
    if cmds[ci].kind == 'l' {
        cmds[ci].pts.insert(pi, r(pt));
    } else {
        let start = if pi == 0 { cmds[..ci].iter().rev().find_map(|x| x.pts.last().copied()) } else { Some(cmds[ci].pts[pi * 3 - 1]) };
        let Some(p0) = start else { return false };
        let (c1, c2, p3) = (cmds[ci].pts[pi * 3], cmds[ci].pts[pi * 3 + 1], cmds[ci].pts[pi * 3 + 2]);
        let lerp = |a: (f64, f64), b: (f64, f64)| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        let (ab, bc, cd) = (lerp(p0, c1), lerp(c1, c2), lerp(c2, p3));
        let (abc, bcd) = (lerp(ab, bc), lerp(bc, cd));
        let m = lerp(abc, bcd);
        cmds[ci].pts.splice(pi * 3..pi * 3 + 3, [r(ab), r(abc), r(m), r(bcd), r(cd), p3]);
    }
    true
}

/// `(ci, pi)` noktasıyla biten düz `l` parçasını kübik eğriye (`b`) çevirir: kontrol noktaları
/// önceki noktadan bu noktaya 1/3 ve 2/3'te. Önceki nokta yoksa ya da nokta `l` değilse `false`.
pub fn line_to_bezier(cmds: &mut Vec<Cmd>, ci: usize, pi: usize) -> bool {
    let Some(c) = cmds.get(ci) else { return false };
    if c.kind != 'l' || pi >= c.pts.len() {
        return false;
    }
    let prev = if pi > 0 { Some(c.pts[pi - 1]) } else { cmds[..ci].iter().rev().find_map(|x| x.pts.last().copied()) };
    let Some(a) = prev else { return false };
    let b = c.pts[pi];
    let lerp = |t: f64| ((a.0 + (b.0 - a.0) * t).round(), (a.1 + (b.1 - a.1) * t).round());
    let before: Vec<(f64, f64)> = c.pts[..pi].to_vec();
    let after: Vec<(f64, f64)> = c.pts[pi + 1..].to_vec();
    let mut repl = vec![];
    if !before.is_empty() {
        repl.push(Cmd { kind: 'l', pts: before });
    }
    repl.push(Cmd { kind: 'b', pts: vec![lerp(1.0 / 3.0), lerp(2.0 / 3.0), b] });
    if !after.is_empty() {
        repl.push(Cmd { kind: 'l', pts: after });
    }
    cmds.splice(ci..=ci, repl);
    true
}

/// Noktayı siler. `b` eğrisinin noktası ise eğri parçası (3 nokta) düz çizgiye (`l` bitiş noktası) çevrilir;
/// boşalan komutlar atılır; ilk `m` silinirse sonraki nokta yeni başlangıç olur.
pub fn remove_point(cmds: &mut Vec<Cmd>, ci: usize, pi: usize) {
    let Some(c) = cmds.get_mut(ci) else { return };
    if c.kind == 'b' {
        let seg = pi / 3;
        if (seg + 1) * 3 <= c.pts.len() {
            let end = c.pts[seg * 3 + 2];
            let before: Vec<(f64, f64)> = c.pts[..seg * 3].to_vec();
            let after: Vec<(f64, f64)> = c.pts[(seg + 1) * 3..].to_vec();
            let mut repl = vec![];
            if !before.is_empty() {
                repl.push(Cmd { kind: 'b', pts: before });
            }
            if pi % 3 != 2 {
                // kontrol noktası silindi: bitiş noktası düz çizgi olarak kalsın
                repl.push(Cmd { kind: 'l', pts: vec![end] });
            }
            if !after.is_empty() {
                repl.push(Cmd { kind: 'b', pts: after });
            }
            cmds.splice(ci..=ci, repl);
        }
    } else if pi < c.pts.len() {
        c.pts.remove(pi);
    }
    cmds.retain(|c| !c.pts.is_empty() || matches!(c.kind, 'c'));
    if let Some(first) = cmds.first_mut() {
        if first.kind != 'm' && first.kind != 'n' && !first.pts.is_empty() {
            let p = first.pts.remove(0);
            let rest = std::mem::take(&mut first.pts);
            let kind = first.kind;
            cmds[0] = Cmd { kind: 'm', pts: vec![p] };
            if !rest.is_empty() && kind != 'b' {
                cmds.insert(1, Cmd { kind, pts: rest });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_points() {
        let s = "m 10 20 l 100 20 100.5 80 b 90 90 50 90 10 80";
        let c = parse(s);
        assert_eq!(c.len(), 3);
        assert_eq!(to_string(&c), s);
        assert_eq!(points(&c).len(), 6);
        assert_eq!(polylines(&c)[0].len(), 1 + 2 + 12);
        // büyük harf ve fazla boşluk
        assert_eq!(to_string(&parse("M 0 0  L 5 5")), "m 0 0 l 5 5");
    }

    #[test]
    fn insert_point_and_new_shape() {
        let mut c = parse("m 0 0 l 100 0 100 100");
        assert!(insert_point(&mut c, (50.0, 3.0), 10.0));
        assert_eq!(to_string(&c), "m 0 0 l 50 0 100 0 100 100");
        assert!(!insert_point(&mut c, (500.0, 500.0), 10.0));
        // eğri ikiye bölünür, uçlar korunur
        let mut c = parse("m 0 0 b 0 100 100 100 100 0");
        let mid = bez((0.0, 0.0), (0.0, 100.0), (100.0, 100.0), (100.0, 0.0), 0.5);
        assert!(insert_point(&mut c, mid, 5.0));
        assert_eq!(c[1].pts.len(), 6);
        assert_eq!(c[1].pts[2], (50.0, 75.0));
        assert_eq!(c[1].pts[5], (100.0, 0.0));
        push_move(&mut c, (10.0, 10.0));
        push_point(&mut c, (20.0, 10.0));
        assert!(to_string(&c).ends_with("m 10 10 l 20 10"));
        assert_eq!(polylines(&c).len(), 2);
    }

    #[test]
    fn edit_points() {
        let mut c = vec![];
        push_point(&mut c, (0.0, 0.0));
        push_point(&mut c, (10.0, 0.0));
        push_point(&mut c, (10.0, 10.0));
        assert_eq!(to_string(&c), "m 0 0 l 10 0 10 10");
        remove_point(&mut c, 1, 0);
        assert_eq!(to_string(&c), "m 0 0 l 10 10");
        remove_point(&mut c, 0, 0);
        assert_eq!(to_string(&c), "m 10 10");
        let mut b = parse("m 0 0 b 1 1 2 2 3 3 l 9 9");
        remove_point(&mut b, 1, 0);
        assert_eq!(to_string(&b), "m 0 0 l 3 3 l 9 9");
    }

    #[test]
    fn line_segment_to_curve() {
        let mut c = parse("m 0 0 l 30 0 30 30 0 30");
        assert!(line_to_bezier(&mut c, 1, 1));
        assert_eq!(to_string(&c), "m 0 0 l 30 0 b 30 10 30 20 30 30 l 0 30");
        // ilk l noktası: önceki nokta m'den
        let mut c = parse("m 0 0 l 30 0");
        assert!(line_to_bezier(&mut c, 1, 0));
        assert_eq!(to_string(&c), "m 0 0 b 10 0 20 0 30 0");
        assert!(!line_to_bezier(&mut c, 0, 0));
    }
}
