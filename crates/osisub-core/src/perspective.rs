//! Perspektif uydurma: düz bir dikdörtgeni (metin/çizim kutusu) ekrandaki 4 köşeli bir dörtgene
//! oturtan `\frx \fry \frz \fscx \fscy \fax \pos \org` değerlerini bulur. İleri model libass'ın
//! dönüşüm zinciridir (ölçek → eğme → `\org` çevresinde frz, frx, fry → 20000 birim kamera
//! uzaklığıyla izdüşüm); ters çözüm Levenberg–Marquardt ile yapılır.
//!
//! Kamera uzaklığı libass'ta 20000 (26.6 sabit nokta) = **312.5 render pikseli**dir; betik
//! koordinatında `312.5 × PlayResY / LayoutResY` (LayoutRes yoksa render yüksekliği). Bu yüzden
//! perspektifli satırlar betiğe `LayoutResX/Y` yazılarak oynatıcı çözünürlüğünden bağımsız yapılır.
//! (libass çıktısıyla karşılaştırıldı: aynı ve yarım render çözünürlüğünde IoU ≥ 0.99.)

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub pos: (f64, f64),
    pub frx: f64,
    pub fry: f64,
    pub frz: f64,
    pub fscx: f64,
    pub fscy: f64,
    pub fax: f64,
}

/// libass kamera uzaklığı (render pikseli).
pub const DIST_PX: f64 = 312.5;

/// Betik koordinatında kamera uzaklığı.
pub fn dist_for(play_res_y: f64, layout_res_y: f64) -> f64 {
    DIST_PX * play_res_y / layout_res_y.max(1.0)
}

/// `\an7` ile `pos`'a konmuş w×h kutunun bir noktasının (u,v) ekrandaki izdüşümü.
pub fn project(p: &Params, org: (f64, f64), dist: f64, u: f64, v: f64) -> (f64, f64) {
    // libass: eğme ölçekten önce (fax × fscx/fscy ölçekli uzayda) → x = (u + fax·v)·fscx
    let (x, y) = ((u + p.fax * v) * p.fscx / 100.0, v * p.fscy / 100.0);
    let (px, py) = (p.pos.0 + x - org.0, p.pos.1 + y - org.1);
    let (frx, fry, frz) = (p.frx.to_radians(), p.fry.to_radians(), p.frz.to_radians());
    let (sx, cx) = (-frx.sin(), frx.cos());
    let (sy, cy) = (fry.sin(), fry.cos());
    let (sz, cz) = (-frz.sin(), frz.cos());
    let x2 = px * cz - py * sz;
    let y2 = px * sz + py * cz;
    let y3 = y2 * cx;
    let z3 = y2 * sx;
    let x4 = x2 * cy - z3 * sy;
    let z4 = x2 * sy + z3 * cy;
    let k = dist / (z4 + dist);
    (org.0 + x4 * k, org.1 + y3 * k)
}

/// Kutunun dört köşesinin izdüşümü (sol üst, sağ üst, sağ alt, sol alt).
pub fn corners(p: &Params, org: (f64, f64), dist: f64, w: f64, h: f64) -> [(f64, f64); 4] {
    [project(p, org, dist, 0.0, 0.0), project(p, org, dist, w, 0.0), project(p, org, dist, w, h), project(p, org, dist, 0.0, h)]
}

fn to_vec(p: &Params) -> [f64; 8] {
    [p.pos.0, p.pos.1, p.frx, p.fry, p.frz, p.fscx, p.fscy, p.fax]
}
fn from_vec(v: &[f64; 8]) -> Params {
    Params { pos: (v[0], v[1]), frx: v[2], fry: v[3], frz: v[4], fscx: v[5], fscy: v[6], fax: v[7] }
}

/// Köşe hataları + düzenlileştirme: aşırı eğme/3B açı yerine sade çözüm tercih edilir
/// (aynı dörtgene uyan dejenere çözümler — ör. fry ≈ 90°, dev \fscx — libass'ta görünmez).
const NRES: usize = 11;
fn residual(v: &[f64; 8], org: (f64, f64), dist: f64, w: f64, h: f64, quad: &[(f64, f64); 4]) -> [f64; NRES] {
    let c = corners(&from_vec(v), org, dist, w, h);
    let mut r = [0.0; NRES];
    for i in 0..4 {
        r[2 * i] = c[i].0 - quad[i].0;
        r[2 * i + 1] = c[i].1 - quad[i].1;
    }
    r[8] = v[7] * 2.0; // fax
    r[9] = (v[2] / 60.0).powi(3) * 2.0; // frx
    r[10] = (v[3] / 60.0).powi(3) * 2.0; // fry
    r
}

/// 8×8 doğrusal sistem (Gauss eleme, kısmi pivot).
fn solve8(mut a: [[f64; 8]; 8], mut b: [f64; 8]) -> Option<[f64; 8]> {
    for c in 0..8 {
        let p = (c..8).max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap())?;
        if a[p][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..8 {
            let f = a[r][c] / a[c][c];
            for k in c..8 {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; 8];
    for c in (0..8).rev() {
        let s: f64 = (c + 1..8).map(|k| a[c][k] * x[k]).sum();
        x[c] = (b[c] - s) / a[c][c];
    }
    Some(x)
}

/// Dörtgenin ağırlık merkezi (varsayılan `\org`).
pub fn centroid(q: &[(f64, f64); 4]) -> (f64, f64) {
    (q.iter().map(|p| p.0).sum::<f64>() / 4.0, q.iter().map(|p| p.1).sum::<f64>() / 4.0)
}

/// `quad` (sol üst, sağ üst, sağ alt, sol alt — ekran koordinatı) için w×h kutuyu oturtan değerler.
/// `dist`: betik koordinatında kamera uzaklığı (`dist_for`).
/// Dönüş: (değerler, `\org`, en büyük köşe hatası piksel). Uygun çözüm yoksa `None`.
pub fn fit(quad: &[(f64, f64); 4], w: f64, h: f64, dist: f64) -> Option<(Params, (f64, f64), f64)> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let org = centroid(quad);
    let d = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let top = (quad[1].0 - quad[0].0, quad[1].1 - quad[0].1);
    let init = Params {
        pos: quad[0],
        frx: 0.0,
        fry: 0.0,
        frz: -top.1.atan2(top.0).to_degrees(),
        fscx: (d(quad[0], quad[1]) + d(quad[3], quad[2])) / 2.0 / w * 100.0,
        fscy: (d(quad[0], quad[3]) + d(quad[1], quad[2])) / 2.0 / h * 100.0,
        fax: 0.0,
    };
    // \frz başlangıcıyla pos'u, dönmüş kutunun sol üstü quad[0]'a gelecek biçimde düzelt
    let mut v = to_vec(&init);
    let mut lambda = 1e-3;
    let cost = |r: &[f64; NRES]| r.iter().map(|x| x * x).sum::<f64>();
    let mut r = residual(&v, org, dist, w, h, quad);
    let mut c = cost(&r);
    let steps = [0.01, 0.01, 1e-4, 1e-4, 1e-4, 1e-3, 1e-3, 1e-6];
    for _ in 0..500 {
        if c < 1e-12 {
            break;
        }
        // sayısal Jacobi
        let mut j = [[0.0; 8]; NRES];
        for k in 0..8 {
            let mut vp = v;
            vp[k] += steps[k];
            let rp = residual(&vp, org, dist, w, h, quad);
            for i in 0..NRES {
                j[i][k] = (rp[i] - r[i]) / steps[k];
            }
        }
        let mut jtj = [[0.0; 8]; 8];
        let mut jtr = [0.0; 8];
        for a in 0..8 {
            for b in 0..8 {
                jtj[a][b] = (0..NRES).map(|i| j[i][a] * j[i][b]).sum();
            }
            jtr[a] = -(0..NRES).map(|i| j[i][a] * r[i]).sum::<f64>();
        }
        let mut improved = false;
        for _ in 0..12 {
            let mut m = jtj;
            for a in 0..8 {
                m[a][a] += lambda * (1.0 + jtj[a][a]);
            }
            let Some(dx) = solve8(m, jtr) else {
                lambda *= 10.0;
                continue;
            };
            let mut nv = v;
            for k in 0..8 {
                nv[k] += dx[k];
            }
            // açıları makul aralıkta tut (|frx|,|fry| < 89°)
            nv[2] = nv[2].clamp(-89.0, 89.0);
            nv[3] = nv[3].clamp(-89.0, 89.0);
            let nr = residual(&nv, org, dist, w, h, quad);
            let nc = cost(&nr);
            if nc < c {
                v = nv;
                r = nr;
                c = nc;
                lambda = (lambda / 3.0).max(1e-12);
                improved = true;
                break;
            }
            lambda *= 4.0;
        }
        if !improved {
            break;
        }
    }
    let p = from_vec(&v);
    let err = corners(&p, org, dist, w, h).iter().zip(quad.iter()).map(|(a, b)| d(*a, *b)).fold(0.0, f64::max);
    (err.is_finite() && p.fscx > 0.0 && p.fscy > 0.0).then_some((p, org, err))
}

fn n(v: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, v);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// Bulunan değerlerin ASS etiketleri (`\an7` dahil).
pub fn tags(p: &Params, org: (f64, f64)) -> String {
    format!(
        "\\an7\\pos({},{})\\org({},{})\\frx{}\\fry{}\\frz{}\\fscx{}\\fscy{}\\fax{}",
        n(p.pos.0, 2),
        n(p.pos.1, 2),
        n(org.0, 2),
        n(org.1, 2),
        n(p.frx, 3),
        n(p.fry, 3),
        n(p.frz, 3),
        n(p.fscx, 3),
        n(p.fscy, 3),
        n(p.fax, 5)
    )
}

/// Vektör kırpmadan dörtgen: ilk 4 nokta (m x y l x y x y x y). Sıra saat yönünde değilse
/// düzeltilir; sol üst köşe en küçük x+y olan noktadır.
pub fn quad_from_clip(drawing: &str) -> Option<[(f64, f64); 4]> {
    let pts: Vec<(f64, f64)> = crate::drawing::points(&crate::drawing::parse(drawing)).into_iter().map(|(_, _, p)| p).collect();
    if pts.len() < 4 {
        return None;
    }
    let mut q = [pts[0], pts[1], pts[2], pts[3]];
    // ekran koordinatında (y aşağı) saat yönü: alan işareti pozitif
    let area: f64 = (0..4).map(|i| q[i].0 * q[(i + 1) % 4].1 - q[(i + 1) % 4].0 * q[i].1).sum();
    if area < 0.0 {
        q.swap(1, 3);
    }
    let start = (0..4).min_by(|&a, &b| (q[a].0 + q[a].1).partial_cmp(&(q[b].0 + q[b].1)).unwrap()).unwrap();
    q.rotate_left(start);
    Some(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_known_transform_roundtrip() {
        // 2B ölçek+dönüş: tam çözülmeli
        let truth = Params { pos: (300.0, 200.0), frx: 0.0, fry: 0.0, frz: 15.0, fscx: 120.0, fscy: 80.0, fax: 0.0 };
        let org = (350.0, 230.0);
        let q = corners(&truth, org, 312.5, 200.0, 50.0);
        let (p, _, err) = fit(&q, 200.0, 50.0, 312.5).unwrap();
        assert!(err < 0.01, "hata {err}: {p:?}");
        // 3B: frx/fry'li bir dörtgen
        let truth = Params { pos: (500.0, 300.0), frx: 25.0, fry: -35.0, frz: 8.0, fscx: 140.0, fscy: 110.0, fax: 0.1 };
        let q = corners(&truth, (620.0, 340.0), 625.0, 300.0, 80.0);
        let (p, org, err) = fit(&q, 300.0, 80.0, 625.0).unwrap();
        assert!(err < 0.05, "hata {err}: {p:?}");
        let back = corners(&p, org, 625.0, 300.0, 80.0);
        for (a, b) in back.iter().zip(q.iter()) {
            assert!((a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05);
        }
    }

    #[test]
    fn freehand_trapezoid() {
        // elle çizilmiş yamuk (perspektifli tabela)
        let q = [(400.0, 300.0), (800.0, 260.0), (820.0, 420.0), (390.0, 400.0)];
        let (p, _, err) = fit(&q, 400.0, 100.0, 312.5).unwrap();
        assert!(p.frx.abs() < 80.0 && p.fry.abs() < 80.0 && p.fscx < 1000.0, "{p:?}");
        assert!(err < 0.5, "hata {err}: {p:?}");
        assert!(tags(&p, centroid(&q)).starts_with("\\an7\\pos("));
    }

    #[test]
    fn dist_scales_with_layout_res() {
        assert_eq!(dist_for(720.0, 360.0), 625.0);
        assert_eq!(dist_for(1080.0, 1080.0), 312.5);
    }

    #[test]
    fn quad_from_vector_clip() {
        // saat yönünün tersi verilen ve sol üstten başlamayan
        let q = quad_from_clip("m 820 420 l 800 260 400 300 390 400").unwrap();
        assert_eq!(q, [(400.0, 300.0), (800.0, 260.0), (820.0, 420.0), (390.0, 400.0)]);
        assert!(quad_from_clip("m 0 0 l 1 1").is_none());
    }
}
