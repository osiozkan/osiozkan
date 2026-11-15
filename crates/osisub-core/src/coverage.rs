//! Çizilmiş altyazının kapsama alanı (SubInspector karşılığı için): aynı kare siyah ve beyaz
//! zeminde çizilir; iki görüntünün farkı her pikselin gerçek saydamlığını verir
//! (siyah: a·c, beyaz: a·c + (1−a)·255 → a = 1 − (beyaz − siyah)/255).

/// Bir karede görünen her şeyi kapsayan dikdörtgen (piksel), içeriğin özeti ve tamamen opak mı.
#[derive(Clone, Debug, PartialEq)]
pub struct Bounds {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// Kırpılmış görüntünün (renk + alfa) FNV-1a özeti, 16 onaltılık hane: aynı görünüm → aynı özet
    pub hash: String,
    /// Dikdörtgendeki tüm pikseller opak
    pub solid: bool,
}

/// `black`/`white`: aynı karenin siyah ve beyaz zemindeki RGB24 görüntüleri. Hiçbir şey yoksa None.
pub fn bounds_from_bw(black: &[u8], white: &[u8], w: usize, h: usize) -> Option<Bounds> {
    if black.len() < w * h * 3 || white.len() < w * h * 3 {
        return None;
    }
    let alpha = |i: usize| -> u8 {
        let d = (0..3).map(|c| white[i * 3 + c].saturating_sub(black[i * 3 + c])).max().unwrap_or(0);
        255 - d
    };
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(y * w + x) > 0 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == usize::MAX {
        return None;
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut eat = |b: u8| {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    for b in (bw as u32).to_le_bytes().into_iter().chain((bh as u32).to_le_bytes()) {
        eat(b);
    }
    let mut solid = true;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = y * w + x;
            let a = alpha(i);
            solid &= a == 255;
            eat(a);
            if a > 0 {
                // renk (ön çarpımlı siyah zemin değeri) — saydam pikselde anlamsız, özete girmez
                for c in 0..3 {
                    eat(black[i * 3 + c]);
                }
            }
        }
    }
    Some(Bounds { x: x0 as i64, y: y0 as i64, w: bw as i64, h: bh as i64, hash: format!("{:016x}", hash), solid })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize, bg: u8, rects: &[(usize, usize, usize, usize, u8, f32)]) -> Vec<u8> {
        let mut v = vec![bg; w * h * 3];
        for &(x, y, rw, rh, col, a) in rects {
            for yy in y..y + rh {
                for xx in x..x + rw {
                    for c in 0..3 {
                        let k = (yy * w + xx) * 3 + c;
                        v[k] = (col as f32 * a + v[k] as f32 * (1.0 - a)).round() as u8;
                    }
                }
            }
        }
        v
    }

    #[test]
    fn finds_box_including_translucent_and_dark_pixels() {
        let r = [(10, 5, 20, 4, 255, 1.0), (40, 12, 3, 3, 0, 1.0), (2, 30, 1, 1, 200, 0.3)];
        let (b, wh) = (img(64, 40, 0, &r), img(64, 40, 255, &r));
        let bd = bounds_from_bw(&b, &wh, 64, 40).unwrap();
        // siyah piksel (40,12) siyah zeminde görünmez ama beyazda görünür → kapsanır
        assert_eq!((bd.x, bd.y, bd.w, bd.h), (2, 5, 41, 26));
        assert!(!bd.solid);
        let only = [(10, 5, 20, 4, 255, 1.0)];
        let s = bounds_from_bw(&img(64, 40, 0, &only), &img(64, 40, 255, &only), 64, 40).unwrap();
        assert!(s.solid);
        assert_eq!((s.x, s.y, s.w, s.h), (10, 5, 20, 4));
    }

    #[test]
    fn hash_depends_on_content_not_position() {
        let a = [(10, 5, 20, 4, 255, 1.0)];
        let b = [(30, 9, 20, 4, 255, 1.0)];
        let c = [(10, 5, 20, 4, 128, 1.0)];
        let h = |r: &[(usize, usize, usize, usize, u8, f32)]| bounds_from_bw(&img(64, 40, 0, r), &img(64, 40, 255, r), 64, 40).unwrap().hash;
        assert_eq!(h(&a), h(&b));
        assert_ne!(h(&a), h(&c));
        assert_eq!(h(&a).len(), 16);
    }

    #[test]
    fn empty_frame_is_none() {
        assert_eq!(bounds_from_bw(&img(8, 8, 0, &[]), &img(8, 8, 255, &[]), 8, 8), None);
        assert_eq!(bounds_from_bw(&[], &[], 8, 8), None);
    }
}
