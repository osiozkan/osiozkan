//! Bitmap altyazıda (PGS/VobSub OCR) italik algılama.
//!
//! Tesseract italiği bildirmez. İtalik yazının dikey çizgileri sağa yatıktır: görüntüyü ters yönde
//! kaydırınca (shear) sütun izdüşümü en keskin hâle gelir. Her metin satırı için birkaç eğimde
//! izdüşümün "keskinliği" (sütun sayılarının kareleri toplamı) ölçülür; en iyi eğim belirgin biçimde
//! sıfırdan büyükse satır italiktir.

/// Mürekkep eşiği: `ocr_gray` çıktısında metin koyu (0), zemin beyaz (255).
const INK: u8 = 128;

/// Mürekkep içeren yatay şeritler (metin satırları): (y0, y1) yarı açık. Aradaki boşluk
/// `min_gap` satırdan kısaysa birleşir (ğ/İ noktaları satırdan kopmasın); çok ince şeritler atılır.
pub fn text_lines(w: usize, h: usize, px: &[u8]) -> Vec<(usize, usize)> {
    if w == 0 || px.len() < w * h {
        return vec![];
    }
    // Satır arası her zaman boş değildir (alt uzantılar + kenar çizgisi sonraki satıra değer):
    // en yoğun satırın %8'inden az mürekkepli satırlar boşluk sayılır.
    let counts: Vec<usize> = (0..h).map(|y| px[y * w..(y + 1) * w].iter().filter(|&&p| p < INK).count()).collect();
    let thr = (counts.iter().copied().max().unwrap_or(0) * 8 / 100).max(1);
    let rows: Vec<bool> = counts.iter().map(|&c| c >= thr).collect();
    let mut bands: Vec<(usize, usize)> = vec![];
    let mut y = 0;
    while y < h {
        if rows[y] {
            let s = y;
            while y < h && rows[y] { y += 1; }
            bands.push((s, y));
        } else {
            y += 1;
        }
    }
    // küçük boşlukları birleştir (yükseklik ölçeğine göre)
    let tallest = bands.iter().map(|b| b.1 - b.0).max().unwrap_or(0);
    let min_gap = (tallest / 4).max(2);
    let mut out: Vec<(usize, usize)> = vec![];
    for b in bands {
        match out.last_mut() {
            Some(l) if b.0 - l.1 < min_gap => l.1 = b.1,
            _ => out.push(b),
        }
    }
    out.retain(|b| b.1 - b.0 >= (tallest / 3).max(4));
    out
}

fn sharpness(w: usize, px: &[u8], y0: usize, y1: usize, slope: f32) -> f64 {
    let yc = (y0 + y1) as f32 / 2.0;
    let pad = ((y1 - y0) as f32 * slope.abs()).ceil() as usize + 1;
    let mut cols = vec![0u32; w + 2 * pad];
    for y in y0..y1 {
        let shift = (slope * (y as f32 - yc)).round() as isize;
        for (x, &p) in px[y * w..(y + 1) * w].iter().enumerate() {
            if p < INK {
                let nx = x as isize + shift + pad as isize;
                if nx >= 0 && (nx as usize) < cols.len() {
                    cols[nx as usize] += 1;
                }
            }
        }
    }
    cols.iter().map(|&c| (c as f64) * (c as f64)).sum()
}

/// Satırın eğimi: en keskin izdüşümü veren kaydırma (piksel/piksel; italik ≈ 0.15–0.3).
pub fn slant(w: usize, px: &[u8], y0: usize, y1: usize) -> (f32, f64) {
    let base = sharpness(w, px, y0, y1, 0.0);
    if base == 0.0 {
        return (0.0, 1.0);
    }
    let mut best = (0.0f32, base);
    for i in -8..=8 {
        let s = i as f32 * 0.05; // -0.40 … 0.40
        let v = sharpness(w, px, y0, y1, s);
        if v > best.1 {
            best = (s, v);
        }
    }
    (best.0, best.1 / base)
}

/// Satır italik mi? Eğim ≥ 0.1 ve keskinlik düz hâline göre belirgin (≥ %6) artmalı.
pub fn is_italic_line(w: usize, px: &[u8], y0: usize, y1: usize) -> bool {
    let (s, gain) = slant(w, px, y0, y1);
    s >= 0.1 && gain >= 1.06
}

/// Görüntüdeki her metin satırı için italik bayrağı (yukarıdan aşağı).
pub fn line_flags(w: usize, h: usize, px: &[u8]) -> Vec<bool> {
    text_lines(w, h, px).into_iter().map(|(a, b)| is_italic_line(w, px, a, b)).collect()
}

/// P5 PGM baytlarından (`pgs::to_pgm`, hardsub görüntüleri) satır bayrakları; bozuksa boş.
pub fn flags_from_pgm(pgm: &[u8]) -> Vec<bool> {
    // başlık: "P5" boşluk W boşluk H boşluk 255 tek boşluk
    let mut fields: Vec<usize> = vec![];
    let mut i = 2;
    if !pgm.starts_with(b"P5") {
        return vec![];
    }
    while fields.len() < 3 && i < pgm.len() {
        while i < pgm.len() && pgm[i].is_ascii_whitespace() { i += 1; }
        let s = i;
        while i < pgm.len() && pgm[i].is_ascii_digit() { i += 1; }
        match std::str::from_utf8(&pgm[s..i]).ok().and_then(|t| t.parse().ok()) {
            Some(v) => fields.push(v),
            None => return vec![],
        }
    }
    i += 1;
    let (w, h) = (fields.first().copied().unwrap_or(0), fields.get(1).copied().unwrap_or(0));
    match w.checked_mul(h) {
        Some(n) if n > 0 && i + n <= pgm.len() => line_flags(w, h, &pgm[i..i + n]),
        _ => vec![],
    }
}

/// OCR metnine italik etiketi ekler. Metin satırı sayısı (`\N`) bayraklarla eşleşiyorsa satır
/// satır, değilse çoğunluğa göre tüm metin. Hepsi italikse `{\i1}…` (Aegisub/SE gibi kapanış yok).
pub fn apply(text: &str, flags: &[bool]) -> String {
    if flags.is_empty() || text.is_empty() || !flags.iter().any(|&f| f) {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split("\\N").collect();
    if flags.iter().all(|&f| f) || (lines.len() != flags.len() && flags.iter().filter(|&&f| f).count() * 2 > flags.len()) {
        return format!("{{\\i1}}{}", text);
    }
    if lines.len() != flags.len() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut cur = false;
    for (i, (l, &f)) in lines.iter().zip(flags).enumerate() {
        if i > 0 {
            out.push_str("\\N");
        }
        if f != cur {
            out.push_str(if f { "{\\i1}" } else { "{\\i0}" });
            cur = f;
        }
        out.push_str(l);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Yapay "harfler": 3 px kalın dikey çubuklar; `slope` ile yatırılır (üst sağa).
    fn bars(w: usize, h: usize, rows: &[(usize, usize, f32)]) -> Vec<u8> {
        let mut px = vec![255u8; w * h];
        for &(y0, y1, slope) in rows {
            let yc = (y0 + y1) as f32 / 2.0;
            for y in y0..y1 {
                let shift = (slope * (yc - y as f32)).round() as isize;
                for k in 0..12 {
                    let x0 = 10 + k * 14;
                    for dx in 0..3 {
                        let x = x0 as isize + dx + shift;
                        if x >= 0 && (x as usize) < w { px[y * w + x as usize] = 0; }
                    }
                }
                // yatay çizgi (e/t gibi) satır ortasında — düz metinde de olur
                if y == (y0 + y1) / 2 { for x in 10..170 { px[y * w + x] = 0; } }
            }
        }
        px
    }

    #[test]
    fn duz_ve_italik_satir() {
        let (w, h) = (200, 100);
        let px = bars(w, h, &[(10, 40, 0.0), (60, 90, 0.22)]);
        assert_eq!(text_lines(w, h, &px), vec![(10, 40), (60, 90)]);
        assert_eq!(line_flags(w, h, &px), vec![false, true]);
    }

    #[test]
    fn noktali_harf_satirdan_kopmaz() {
        let (w, h) = (200, 60);
        let mut px = bars(w, h, &[(20, 50, 0.0)]);
        // İ noktası: satırın 3 px üstünde
        for x in 10..13 { px[15 * w + x] = 0; px[16 * w + x] = 0; }
        assert_eq!(text_lines(w, h, &px).len(), 1, "nokta ayrı satır sayılmamalı");
    }

    #[test]
    fn etiket_ekleme() {
        assert_eq!(apply("Merhaba", &[true]), "{\\i1}Merhaba");
        assert_eq!(apply("Merhaba", &[false]), "Merhaba");
        assert_eq!(apply("A\\NB", &[false, true]), "A\\N{\\i1}B");
        assert_eq!(apply("A\\NB", &[true, false]), "{\\i1}A\\N{\\i0}B");
        assert_eq!(apply("A\\NB", &[true, true]), "{\\i1}A\\NB");
        // eşleşmeyen satır sayısı: çoğunluk
        assert_eq!(apply("AB", &[true, false]), "AB");
        assert_eq!(apply("", &[true]), "");
        assert!(line_flags(0, 0, &[]).is_empty());
        let (w, h) = (200, 100);
        let px = bars(w, h, &[(10, 40, 0.0), (60, 90, 0.22)]);
        let mut pgm = format!("P5\n{} {}\n255\n", w, h).into_bytes();
        pgm.extend(&px);
        assert_eq!(flags_from_pgm(&pgm), vec![false, true]);
        assert!(flags_from_pgm(b"P5\n99999 99999\n255\n").is_empty());
        assert!(flags_from_pgm(b"P6").is_empty());
    }
}
