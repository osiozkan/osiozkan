//! Videoya hizala (Aegisub `time/align`, `DialogAlignToVideo`): bir tabelanın bir pikselinin rengi ve
//! o rengin çevresindeki alanın sınırları (yatay/dikey) önceki ve sonraki karelerde aranır; tabelanın
//! göründüğü kare aralığı bulunur. Renk karşılaştırması Lab uzayında, sınırlar ±5 px toleranslı;
//! arama Aegisub'daki gibi 2'şer kare adımlarla (kaynaktaki davranış birebir).

/// Aegisub `rgb2lab`.
pub fn rgb2lab(c: [u8; 3]) -> [f64; 3] {
    let (r, g, b) = (c[0] as f64, c[1] as f64, c[2] as f64);
    let x = (0.412453 * r + 0.357580 * g + 0.180423 * b) / 255.0;
    let y = (0.212671 * r + 0.715160 * g + 0.072169 * b) / 255.0;
    let z = (0.019334 * r + 0.119193 * g + 0.950227 * b) / 255.0;
    let (xr, yr, zr) = (x / 0.950456, y, z / 1.088854);
    let l = if yr > 0.008856 { 116.0 * yr.powf(1.0 / 3.0) - 16.0 } else { 903.3 * yr };
    let f = |t: f64| if t > 0.008856 { t.powf(1.0 / 3.0) } else { 7.787 * t + 16.0 / 116.0 };
    [l, 500.0 * (f(xr) - f(yr)), 200.0 * (f(yr) - f(zr))]
}

pub fn close(p: [u8; 3], lab: [f64; 3], tol: f64) -> bool {
    let q = rgb2lab(p);
    ((q[0] - lab[0]).powi(2) + (q[1] - lab[1]).powi(2) + (q[2] - lab[2]).powi(2)).sqrt() <= tol
}

/// Bir karenin (x, y)'den geçen satırı ve sütunu.
#[derive(Clone, Debug, Default)]
pub struct Cross {
    pub row: Vec<[u8; 3]>,
    pub col: Vec<[u8; 3]>,
}

/// Aegisub `calculate_point`: (x, y) rengi tutuyorsa sol/sağ/üst/alt ilk uymayan konumlar
/// (kenara kadar uyuyorsa o yön x ya da y kalır — Aegisub'daki gibi).
pub fn extent(c: &Cross, x: usize, y: usize, lab: [f64; 3], tol: f64) -> Option<[i64; 4]> {
    if !close(*c.row.get(x)?, lab, tol) {
        return None;
    }
    let (mut l, mut r, mut u, mut d) = (x as i64, x as i64, y as i64, y as i64);
    if let Some(i) = (x + 1..c.row.len()).find(|&i| !close(c.row[i], lab, tol)) {
        r = i as i64;
    }
    if let Some(i) = (0..x).rev().find(|&i| !close(c.row[i], lab, tol)) {
        l = i as i64;
    }
    if let Some(i) = (y + 1..c.col.len()).find(|&i| !close(c.col[i], lab, tol)) {
        d = i as i64;
    }
    if let Some(i) = (0..y).rev().find(|&i| !close(c.col[i], lab, tol)) {
        u = i as i64;
    }
    Some([l, r, u, d])
}

/// Aegisub `check_exists`: karede aynı renk ve sınırlar (±5 px) var mı.
pub fn same(c: &Cross, x: usize, y: usize, lab: [f64; 3], tol: f64, base: [i64; 4]) -> bool {
    match extent(c, x, y, lab, tol) {
        Some(a) => (0..4).all(|k| (a[k] - base[k]).abs() <= 5),
        None => false,
    }
}

/// Aegisub `process` içindeki aralık araması: `cur` karesinden geriye ve ileriye 2'şer adımla.
/// `check(kare)` o karede tabela var mı. Dönüş: (ilk kare, son kare).
pub fn span(cur: i64, n_frames: i64, mut check: impl FnMut(i64) -> bool) -> (i64, i64) {
    let mut pos = cur;
    loop {
        pos -= 2;
        if !(pos >= 0 && check(pos)) {
            break;
        }
    }
    pos += 1;
    pos = pos.max(0);
    let left = if check(pos) { pos } else { pos + 1 };
    let mut pos = cur;
    loop {
        pos += 2;
        if !(pos < n_frames && check(pos)) {
            break;
        }
    }
    pos -= 1;
    pos = pos.min(n_frames - 1);
    let right = if check(pos) { pos } else { pos - 1 };
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_finds_visible_range() {
        // tabela 37..=81 karelerinde
        let vis = |f: i64| (37..=81).contains(&f);
        assert_eq!(span(50, 200, vis), (37, 81));
        assert_eq!(span(37, 200, vis), (37, 81));
        assert_eq!(span(80, 200, vis), (37, 81));
        // videonun başı/sonu
        assert_eq!(span(3, 10, |_| true), (0, 9));
    }

    #[test]
    fn extent_and_same() {
        let red = [220, 30, 30];
        let lab = rgb2lab(red);
        let mut row = vec![[0, 0, 0]; 100];
        for p in &mut row[20..60] {
            *p = red;
        }
        let mut col = vec![[0, 0, 0]; 50];
        for p in &mut col[10..30] {
            *p = [215, 35, 28];
        }
        let c = Cross { row, col };
        let e = extent(&c, 40, 20, lab, 20.0).unwrap();
        assert_eq!(e, [19, 60, 9, 30]);
        assert!(same(&c, 40, 20, lab, 20.0, e));
        // tabela 10 px kaydı: aynı değil
        let mut c2 = c.clone();
        c2.row.rotate_right(10);
        assert!(!same(&c2, 40, 20, lab, 20.0, e));
        assert!(extent(&c, 5, 20, lab, 20.0).is_none());
    }
}
