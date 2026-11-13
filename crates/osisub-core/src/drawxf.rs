//! `\p` çizim noktaları ↔ ekran (betik) koordinatı dönüşümü — her hizalama, ölçek, eğme ve
//! döndürmeyle (video çizim aracı için).
//!
//! libass ile ölçülen kural: hizalama kaydırması yalnızca çizimin sınır kutusu *boyutunu*
//! kullanır (başlangıç noktası değil): `\an5` için nokta `pos + S·((x, y) − (w/2, h/2))`.
//! Eğme (`\fax/\fay`) çizimin kendi başlangıcına göre ve hizalamadan önce uygulanır; ardından
//! ölçek, `\org` çevresinde döndürme ve izdüşüm (libass zinciri; `tests/drawxf_libass.rs` gerçek
//! libass çıktısıyla ≤ 2 px doğrular). Ters dönüşüm Newton yinelemesiyle (düzlemsel izdüşüm tersinir).
use crate::ass::{Dialogue, Script};
use crate::overrides as ov;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawXf {
    pub pos: (f64, f64),
    pub org: (f64, f64),
    pub dist: f64,
    pub frx: f64,
    pub fry: f64,
    pub frz: f64,
    pub fscx: f64,
    pub fscy: f64,
    pub fax: f64,
    pub fay: f64,
    /// çizim birimi → piksel (`\p2` → 0.5)
    pub k: f64,
    /// hizalama kaydırması (çizim biriminde): (ax·w, ay·h)
    pub off: (f64, f64),
}

impl DrawXf {
    /// Satırın etiketlerinden. `pos`: satır konumu (\pos ya da varsayılan), `drawing`: çizim metni,
    /// `dist`: kamera uzaklığı (betik koordinatında; bkz. `perspective::dist_for`).
    pub fn from_line(script: &Script, d: &Dialogue, pos: (f64, f64), drawing: &str, dist: f64) -> Option<DrawXf> {
        let num = |n: &str| ov::get_initial(&d.text, n).and_then(|t| t.number());
        let pscale = num("p").unwrap_or(0.0);
        if pscale <= 0.0 {
            return None;
        }
        let st = script.style_by_name(&d.style).cloned().unwrap_or_default();
        let an = num("an").map(|v| v as i32).unwrap_or(st.align).clamp(1, 9);
        let pts: Vec<(f64, f64)> = crate::drawing::points(&crate::drawing::parse(drawing)).into_iter().map(|p| p.2).collect();
        let (w, h) = if pts.is_empty() {
            (0.0, 0.0)
        } else {
            let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
            let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
            (x1 - x0, y1 - y0)
        };
        let ax = match (an - 1) % 3 {
            0 => 0.0,
            1 => 0.5,
            _ => 1.0,
        };
        let ay = match (an - 1) / 3 {
            0 => 1.0,
            1 => 0.5,
            _ => 0.0,
        };
        let org = ov::get_initial(&d.text, "org")
            .map(|t| t.args())
            .and_then(|a| Some((a.first()?.trim().parse::<f64>().ok()?, a.get(1)?.trim().parse::<f64>().ok()?)))
            .unwrap_or(pos);
        Some(DrawXf {
            pos,
            org,
            dist,
            frx: num("frx").unwrap_or(0.0),
            fry: num("fry").unwrap_or(0.0),
            frz: num("frz").or(num("fr")).unwrap_or(st.angle),
            fscx: num("fscx").unwrap_or(st.scale_x),
            fscy: num("fscy").unwrap_or(st.scale_y),
            fax: num("fax").unwrap_or(0.0),
            fay: num("fay").unwrap_or(0.0),
            k: 1.0 / 2f64.powf(pscale - 1.0),
            off: (ax * w, ay * h),
        })
    }

    /// Çizim noktası → betik koordinatı.
    pub fn fwd(&self, x: f64, y: f64) -> (f64, f64) {
        // libass (ölçüldü): eğme çizimin kendi başlangıcına göre, hizalama kaydırmasından önce;
        // ardından ölçek
        let (u0, v0) = (x * self.k, y * self.k);
        let u = u0 + self.fax * v0 - self.off.0 * self.k;
        let v = v0 + self.fay * u0 - self.off.1 * self.k;
        let (sx, sy) = (u * self.fscx / 100.0, v * self.fscy / 100.0);
        let (px, py) = (self.pos.0 + sx - self.org.0, self.pos.1 + sy - self.org.1);
        let (frx, fry, frz) = (self.frx.to_radians(), self.fry.to_radians(), self.frz.to_radians());
        let (s1, c1) = (-frx.sin(), frx.cos());
        let (s2, c2) = (fry.sin(), fry.cos());
        let (s3, c3) = (-frz.sin(), frz.cos());
        let x2 = px * c3 - py * s3;
        let y2 = px * s3 + py * c3;
        let y3 = y2 * c1;
        let z3 = y2 * s1;
        let x4 = x2 * c2 - z3 * s2;
        let z4 = x2 * s2 + z3 * c2;
        let kk = self.dist / (z4 + self.dist);
        (self.org.0 + x4 * kk, self.org.1 + y3 * kk)
    }

    /// Betik koordinatı → çizim noktası (Newton; `guess` başlangıç). Yakınsamazsa None.
    pub fn inv(&self, s: (f64, f64), guess: (f64, f64)) -> Option<(f64, f64)> {
        let (mut x, mut y) = guess;
        for _ in 0..30 {
            let f = self.fwd(x, y);
            let (ex, ey) = (f.0 - s.0, f.1 - s.1);
            if ex.abs() < 1e-6 && ey.abs() < 1e-6 {
                return Some((x, y));
            }
            let e = 1e-3;
            let fx = self.fwd(x + e, y);
            let fy = self.fwd(x, y + e);
            let (a, c) = ((fx.0 - f.0) / e, (fx.1 - f.1) / e);
            let (b, d) = ((fy.0 - f.0) / e, (fy.1 - f.1) / e);
            let det = a * d - b * c;
            if det.abs() < 1e-12 {
                return None;
            }
            x -= (d * ex - b * ey) / det;
            y -= (-c * ex + a * ey) / det;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
        }
        let f = self.fwd(x, y);
        ((f.0 - s.0).abs() < 1e-3 && (f.1 - s.1).abs() < 1e-3).then_some((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(tags: &str, dr: &str) -> (Script, Dialogue) {
        let s = Script::new_default();
        let p = if tags.contains("\\p2") { "" } else { "\\p1" };
        let d = Dialogue { text: format!("{{{}{}}}{}", tags, p, dr), ..Default::default() };
        (s, d)
    }

    #[test]
    fn alignment_uses_bbox_size_only() {
        // libass ölçümü (1280×720, pos 640,360): \an5 "m 50 50 l 150 50 150 150 50 150" → 640..740
        let dr = "m 50 50 l 150 50 150 150 50 150";
        let (s, d) = line("\\an5\\pos(640,360)\\fscx100\\fscy100\\frz0", dr);
        let t = DrawXf::from_line(&s, &d, (640.0, 360.0), dr, 1e9).unwrap();
        assert_eq!(t.fwd(50.0, 50.0), (640.0, 360.0));
        let (s, d) = line("\\an3\\pos(640,360)\\fscx100\\fscy100\\frz0", "m -20 -10 l 80 -10 80 90 -20 90");
        let t = DrawXf::from_line(&s, &d, (640.0, 360.0), "m -20 -10 l 80 -10 80 90 -20 90", 1e9).unwrap();
        assert_eq!(t.fwd(-20.0, -10.0), (520.0, 250.0));
    }

    #[test]
    fn inverse_roundtrips_with_rotation_and_shear() {
        let dr = "m 0 0 l 200 0 200 80 0 80";
        let (s, d) = line("\\an5\\pos(640,360)\\org(500,300)\\frz25\\frx20\\fry-15\\fax0.3\\fay0.1\\fscx120\\fscy80", dr);
        let t = DrawXf::from_line(&s, &d, (640.0, 360.0), dr, 312.5).unwrap();
        for q in [(0.0, 0.0), (200.0, 0.0), (37.5, 61.0), (-40.0, 120.0)] {
            let sc = t.fwd(q.0, q.1);
            let back = t.inv(sc, (100.0, 40.0)).unwrap();
            assert!((back.0 - q.0).abs() < 1e-4 && (back.1 - q.1).abs() < 1e-4, "{q:?} → {back:?}");
        }
        // \p2: koordinatlar yarı ölçekte
        let (s, d) = line("\\an7\\pos(10,10)\\fscx100\\fscy100\\frz0\\p2", dr);
        let t = DrawXf::from_line(&s, &d, (10.0, 10.0), dr, 1e9).unwrap();
        assert_eq!(t.fwd(200.0, 80.0), (110.0, 50.0));
    }
}
