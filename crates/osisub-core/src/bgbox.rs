//! Satırın arkasına kutu (Subtitle Edit "ASSA set background box"): libass ile ölçülen gerçek
//! sınırlara göre `\p1` çizimi; köşe yarıçapı (kübik eğri) ve dolgu.
use crate::ass::Dialogue;
use crate::coverage::Bounds;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxStyle {
    pub pad_x: i64,
    pub pad_y: i64,
    pub radius: i64,
    /// RGB
    pub color: [u8; 3],
    /// 0 = opak, 255 = tam saydam (ASS alfa)
    pub alpha: u8,
}

impl Default for BoxStyle {
    fn default() -> Self {
        BoxStyle { pad_x: 12, pad_y: 8, radius: 0, color: [0, 0, 0], alpha: 0x60 }
    }
}

/// (x0,y0)-(x1,y1) dikdörtgeninin çizimi; `r` > 0 ise yuvarlak köşeli.
pub fn rect_path(x0: i64, y0: i64, x1: i64, y1: i64, r: i64) -> String {
    let r = r.max(0).min((x1 - x0) / 2).min((y1 - y0) / 2);
    if r == 0 {
        return format!("m {x0} {y0} l {x1} {y0} {x1} {y1} {x0} {y1}");
    }
    // çeyrek daire ≈ kübik eğri, kontrol noktası uzaklığı 0.5523·r
    let k = (r as f64 * 0.4477).round() as i64; // köşeden kontrol noktasına
    format!(
        "m {a} {y0} l {b} {y0} b {c} {y0} {x1} {d} {x1} {e} l {x1} {f} b {x1} {g} {c} {y1} {b} {y1} l {a} {y1} b {h} {y1} {x0} {g} {x0} {f} l {x0} {e} b {x0} {d} {h} {y0} {a} {y0}",
        a = x0 + r,
        b = x1 - r,
        c = x1 - k,
        d = y0 + k,
        e = y0 + r,
        f = y1 - r,
        g = y1 - k,
        h = x0 + k,
    )
}

/// Kutu satırı: aynı zaman/katman/stil, sol üst çapa; satırın `\fad`/`\fade`'i kopyalanır. Metin satırının
/// ÖNÜNE eklenmelidir (aynı katmanda önce gelen altta çizilir).
pub fn box_line(d: &Dialogue, b: &Bounds, st: &BoxStyle) -> Dialogue {
    let (x0, y0, x1, y1) = (b.x - st.pad_x, b.y - st.pad_y, b.x + b.w + st.pad_x, b.y + b.h + st.pad_y);
    let fad = ["fad", "fade"].iter().find_map(|n| crate::overrides::get_initial(&d.text, n).map(|t| format!("\\{}({})", n, t.args().join(",")))).unwrap_or_default();
    let mut o = d.clone();
    o.text = format!(
        "{{\\an7\\pos(0,0)\\bord0\\shad0\\blur0\\1c&H{:02X}{:02X}{:02X}&\\1a&H{:02X}&{}\\p1}}{}",
        st.color[2], st.color[1], st.color[0], st.alpha, fad, rect_path(x0, y0, x1, y1, st.radius)
    );
    o.effect = "kutu".into();
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_line() {
        assert_eq!(rect_path(0, 0, 100, 50, 0), "m 0 0 l 100 0 100 50 0 50");
        let r = rect_path(0, 0, 100, 50, 10);
        assert!(r.starts_with("m 10 0 l 90 0 b 96 0 100 4 100 10 l 100 40"), "{r}");
        // yarıçap kutunun yarısıyla sınırlı
        assert!(rect_path(0, 0, 100, 20, 50).starts_with("m 10 0"));
        let mut d = Dialogue::default();
        d.text = "{\\fad(200,300)}Merhaba".into();
        let b = Bounds { x: 100, y: 200, w: 300, h: 40, hash: String::new(), solid: false };
        let o = box_line(&d, &b, &BoxStyle { color: [0x11, 0x22, 0x33], ..Default::default() });
        assert_eq!(o.text, "{\\an7\\pos(0,0)\\bord0\\shad0\\blur0\\1c&H332211&\\1a&H60&\\fad(200,300)\\p1}m 88 192 l 412 192 412 248 88 248");
    }
}
