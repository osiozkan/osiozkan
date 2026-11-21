//! Gömülü (hardsub) altyazı algılama: videonun alt bölgesinden örneklenen gri karelerde altyazı
//! metnini (parlak dolgu + koyu kenar) maskeler, maskenin kaba imzasındaki değişimlerden satır
//! sınırlarını bulur ve OCR için siyah-üstüne-beyaz görüntü üretir. Akış halinde çalışır: bellekte
//! yalnızca açık satırın temsilci karesi tutulur (24 dk'lık bölüm GB'larca kare demek).

/// Algılama ayarları.
#[derive(Clone, Debug)]
pub struct Options {
    /// Metin dolgusu en az bu parlaklıkta (0-255)
    pub bright: u8,
    /// Kenar (outline) en fazla bu parlaklıkta
    pub dark: u8,
    /// Parlak pikselin bu yarıçapta koyu komşusu olmalı (kenarlı altyazı metni)
    pub radius: usize,
    /// İmza hücresi boyu (piksel)
    pub cell: usize,
    /// Satır sayılması için en az etkin hücre
    pub min_cells: usize,
    /// İki imza arasında satır değişimi sayılacak oran (0-1)
    pub change: f32,
    /// En kısa satır (ms)
    pub min_ms: i64,
}

impl Default for Options {
    fn default() -> Self {
        Options { bright: 200, dark: 80, radius: 2, cell: 8, min_cells: 6, change: 0.35, min_ms: 300 }
    }
}

/// Altyazı metni maskesi: parlak piksellerin bağlı bileşenlerinden, çevresi koyu kenarla
/// (outline) çevrili ve harf boyutunda olanlar. Beyaz zemine bitişik metin dolgusu zeminle tek
/// bileşen olur ve çevresi koyu olmadığından elenir; kenarlı ama uzun çizgiler (ör. sahne öğesi)
/// boyut sınırına takılır.
pub fn text_mask(gray: &[u8], w: usize, h: usize, o: &Options) -> Vec<bool> {
    let n = w * h;
    if gray.len() < n || w == 0 || h == 0 {
        return vec![false; n];
    }
    // her pikselin `radius` komşuluğundaki en koyu değer (ayrılabilir min süzgeci)
    let r = o.radius;
    let mut hmin = vec![255u8; n];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
            hmin[y * w + x] = *gray[y * w + a..=y * w + b].iter().min().unwrap();
        }
    }
    let mut near_dark = vec![false; n];
    for x in 0..w {
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
            near_dark[y * w + x] = (a..=b).map(|yy| hmin[yy * w + x]).min().unwrap() <= o.dark;
        }
    }
    let bright: Vec<bool> = gray[..n].iter().map(|&v| v >= o.bright).collect();
    let mut label = vec![u32::MAX; n];
    let mut out = vec![false; n];
    let mut stack: Vec<usize> = vec![];
    let mut comp: Vec<usize> = vec![];
    let mut id = 0u32;
    let mut accepted: Vec<((usize, usize, usize, usize), Vec<usize>)> = vec![];
    for start in 0..n {
        if !bright[start] || label[start] != u32::MAX {
            continue;
        }
        comp.clear();
        stack.push(start);
        label[start] = id;
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let (mut border, mut enclosed) = (0usize, 0usize);
        while let Some(p) = stack.pop() {
            comp.push(p);
            let (x, y) = (p % w, p / w);
            x0 = x0.min(x); x1 = x1.max(x); y0 = y0.min(y); y1 = y1.max(y);
            let mut nb = |q: usize, border: &mut usize, enclosed: &mut usize, stack: &mut Vec<usize>| {
                if bright[q] {
                    if label[q] == u32::MAX { label[q] = id; stack.push(q); }
                } else {
                    *border += 1;
                    if near_dark[q] { *enclosed += 1; }
                }
            };
            if x > 0 { nb(p - 1, &mut border, &mut enclosed, &mut stack); } else { border += 1; }
            if x + 1 < w { nb(p + 1, &mut border, &mut enclosed, &mut stack); } else { border += 1; }
            if y > 0 { nb(p - w, &mut border, &mut enclosed, &mut stack); } else { border += 1; }
            if y + 1 < h { nb(p + w, &mut border, &mut enclosed, &mut stack); } else { border += 1; }
        }
        id += 1;
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        let glyph_sized = bh <= (h * 6 / 10).max(4) && bw <= (w / 3).max(4) && comp.len() >= 3;
        if glyph_sized && border > 0 && enclosed * 100 >= border * 85 {
            accepted.push(((x0, y0, x1, y1), comp.clone()));
        }
    }
    // harf iç boşluğu (o, e, a'nın içi beyaz zeminde): kutusu başka bir kabul edilmiş bileşenin
    // kutusunun içinde kalan (yatayda kesin, dikeyde u/n gibi açık harflerde kenar ortak olabilir)
    // daha küçük bileşen metin değildir
    for (k, (b, c)) in accepted.iter().enumerate() {
        let inside = accepted.iter().enumerate().any(|(m, (o, oc))| m != k && oc.len() > c.len() && o.0 < b.0 && o.2 > b.2 && o.1 <= b.1 && o.3 >= b.3);
        if !inside {
            for &p in c { out[p] = true; }
        }
    }
    out
}

/// Maskenin hücre imzası: her `cell×cell` hücre, içinde yeterince metin pikseli varsa etkin.
pub fn signature(mask: &[bool], w: usize, h: usize, cell: usize) -> Vec<bool> {
    let (cw, ch) = (w.div_ceil(cell), h.div_ceil(cell));
    let mut cnt = vec![0u16; cw * ch];
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                cnt[(y / cell) * cw + x / cell] += 1;
            }
        }
    }
    let need = (cell * cell / 12).max(2) as u16;
    cnt.into_iter().map(|c| c >= need).collect()
}

/// İki imzanın farkı: değişen etkin hücrelerin, iki imzadaki etkin hücre sayısının büyüğüne oranı.
pub fn diff(a: &[bool], b: &[bool]) -> f32 {
    let changed = a.iter().zip(b).filter(|(x, y)| x != y).count();
    let act = a.iter().filter(|x| **x).count().max(b.iter().filter(|x| **x).count()).max(1);
    changed as f32 / act as f32
}

/// Bulunan satır: (başlangıç ms, bitiş ms, OCR görüntüsü PGM).
#[derive(Clone, Debug)]
pub struct Found {
    pub start: i64,
    pub end: i64,
    pub pgm: Vec<u8>,
}

/// Akış halinde satır sınırı bulucu: kareleri sırayla `push` edin, sonra `finish`.
pub struct Segmenter {
    o: Options,
    w: usize,
    h: usize,
    cur: Option<(i64, Vec<bool>, Vec<u8>, Vec<bool>, usize)>, // (başlangıç, imza, temsilci kare, maske, örnek sayısı)
    last_t: i64,
    /// Son örnekten bu yana gelen ara kareler (kare-doğru sınır için; yalnızca olay olunca maskelenir)
    between: Vec<(i64, Vec<u8>)>,
    pub found: Vec<Found>,
}

impl Segmenter {
    pub fn new(w: usize, h: usize, o: Options) -> Self {
        Segmenter { o, w, h, cur: None, last_t: 0, between: vec![], found: vec![] }
    }

    fn close(&mut self, end: i64) {
        if let Some((start, _, gray, mask, _)) = self.cur.take() {
            if end - start >= self.o.min_ms {
                self.found.push(Found { start, end, pgm: ocr_pgm(&gray, &mask, self.w, self.h) });
            }
        }
    }

    /// Örnekler arasındaki kare (ör. 5 örnek/sn ile 24 fps videoda aradaki 3-4 kare): yalnızca
    /// saklanır; sonraki `push` bir satır sınırı bulursa sınır bu karelerde kareye inceltilir.
    pub fn push_between(&mut self, t: i64, gray: &[u8]) {
        if self.between.len() < 64 {
            self.between.push((t, gray.to_vec()));
        }
    }

    /// Ara karelerin imzaları (yalnızca sınır inceltilirken bir kez hesaplanır).
    fn between_sigs(&self) -> Vec<(i64, Vec<bool>, usize)> {
        self.between.iter().map(|(t, g)| {
            let m = text_mask(g, self.w, self.h, &self.o);
            let sig = signature(&m, self.w, self.h, self.o.cell);
            let a = sig.iter().filter(|x| **x).count();
            (*t, sig, a)
        }).collect()
    }

    /// `sig` imzalı satırın göründüğü ilk ara kare (yoksa `t`).
    fn first_showing(&self, bs: &[(i64, Vec<bool>, usize)], sig: &[bool], t: i64) -> i64 {
        // sondan geriye: satır bu örnekte görünüyor; kesintisiz göründüğü en erken ara kare
        let mut first = t;
        for (bt, bsig, a) in bs.iter().rev() {
            if *a >= self.o.min_cells && diff(sig, bsig) <= self.o.change { first = *bt } else { break }
        }
        first
    }

    /// `sig` imzalı (önceki örnekte görünen) satırın artık görünmediği ilk ara kare (yoksa `t`).
    fn first_gone(&self, bs: &[(i64, Vec<bool>, usize)], sig: &[bool], t: i64) -> i64 {
        bs.iter().find(|(_, bsig, a)| *a < self.o.min_cells || diff(sig, bsig) > self.o.change).map_or(t, |b| b.0)
    }

    /// `t`: karenin zamanı (ms), `gray`: w×h gri piksel.
    pub fn push(&mut self, t: i64, gray: &[u8]) {
        let mask = text_mask(gray, self.w, self.h, &self.o);
        let sig = signature(&mask, self.w, self.h, self.o.cell);
        let active = sig.iter().filter(|x| **x).count();
        self.last_t = t;
        let showing = active >= self.o.min_cells;
        let changed = match &self.cur {
            Some((_, s, ..)) => !showing || diff(s, &sig) > self.o.change,
            None => showing,
        };
        if !changed {
            self.between.clear();
        }
        if !showing {
            if changed {
                let bs = self.between_sigs();
                let end = self.cur.as_ref().map_or(t, |c| self.first_gone(&bs, &c.1, t));
                self.close(end);
            }
            self.between.clear();
            return;
        }
        if changed {
            let bs = if self.between.is_empty() { vec![] } else { self.between_sigs() };
            let end = self.cur.as_ref().map(|c| self.first_gone(&bs, &c.1, t));
            // yeni satır eskisi kaybolmadan başlayamaz (çapraz geçişte eski satırın bitişi)
            let start = self.first_showing(&bs, &sig, t).max(end.unwrap_or(i64::MIN));
            let end = end.unwrap_or(t);
            self.close(end);
            self.between.clear();
            self.cur = Some((start, sig, gray.to_vec(), mask, 1));
        } else if let Some(c) = self.cur.as_mut() {
            c.4 += 1;
            // ikinci örnek temsilci olur (ilk kare belirme/geçiş anında yakalanmış olabilir)
            if c.4 == 2 {
                c.2 = gray.to_vec();
                c.3 = mask;
                c.1 = sig;
            }
        }
    }

    /// Son açık satırı `end` (video sonu) ile kapatır.
    pub fn finish(mut self, end: i64) -> Vec<Found> {
        let e = end.max(self.last_t);
        self.close(e);
        self.found
    }
}

/// OCR görüntüsü: metin kutusuna kırpılmış, koyu metin / beyaz zemin. Maskenin 1 piksel
/// genişletilmiş alanında gri değer ters çevrilir (kenar yumuşatması korunur), dışı beyaz;
/// kenar boşluklu ve 2× doğrusal büyütülmüş (Tesseract küçük metinde zayıf).
pub fn ocr_pgm(gray: &[u8], mask: &[bool], w: usize, h: usize) -> Vec<u8> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 {
        return crate::pgs::to_pgm(1, 1, &[255]);
    }
    let pad = 10usize;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let (iw, ih) = (cw + 2 * pad, ch + 2 * pad);
    // kırpılmış tek ölçekli görüntü
    let mut img = vec![255u8; iw * ih];
    for y in 0..ch {
        for x in 0..cw {
            let (sx, sy) = (x0 + x, y0 + y);
            let mut on = false;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (xx, yy) = (sx as i64 + dx, sy as i64 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h && mask[yy as usize * w + xx as usize] { on = true; }
                }
            }
            if on {
                // parlak metin → koyu; koyu kenar → açık (kontrast germe: 80..230 → 255..0)
                let g = gray[sy * w + sx] as i32;
                img[(y + pad) * iw + x + pad] = (255 - ((g - 80).clamp(0, 150) * 255 / 150)) as u8;
            }
        }
    }
    // 2× doğrusal büyütme
    let (ow, oh) = (iw * 2, ih * 2);
    let mut px = vec![255u8; ow * oh];
    for y in 0..oh {
        let fy = (y as f32 + 0.5) / 2.0 - 0.5;
        let (ya, t) = (fy.floor().max(0.0) as usize, fy - fy.floor());
        let yb = (ya + 1).min(ih - 1);
        for x in 0..ow {
            let fx = (x as f32 + 0.5) / 2.0 - 0.5;
            let (xa, u) = (fx.floor().max(0.0) as usize, fx - fx.floor());
            let xb = (xa + 1).min(iw - 1);
            let v = |xx: usize, yy: usize| img[yy * iw + xx] as f32;
            let top = v(xa, ya) * (1.0 - u) + v(xb, ya) * u;
            let bot = v(xa, yb) * (1.0 - u) + v(xb, yb) * u;
            px[y * ow + x] = (top * (1.0 - t) + bot * t).round() as u8;
        }
    }
    crate::pgs::to_pgm(ow as u32, oh as u32, &px)
}

/// OCR gürültüsü: yalnızca işaretten oluşan başıboş parçaları ("~~", "_", tek ".") atar,
/// noktalama öncesi boşluğu siler. Satırlar `\N` ile ayrılmış kalır.
pub fn tidy(text: &str) -> String {
    text.split("\\N")
        .map(|l| {
            let toks: Vec<&str> = l.split_whitespace().filter(|t| t.chars().any(|c| c.is_alphanumeric()) || *t == "-" || *t == "—" || *t == "♪" || t.chars().all(|c| ".,!?;:…".contains(c))).collect();
            let mut out = toks.join(" ");
            for p in [" .", " ,", " !", " ?", " :", " ;"] {
                while out.contains(p) { out = out.replace(p, &p[1..]); }
            }
            while out.contains("..") && !out.contains("...") { out = out.replace("..", "."); }
            out.replace(". .", ".").replace(".,", ",")
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\\N")
}

/// OCR sonrası: boş metinli satırları atar, aynı metinli bitişik satırları (kararma/titreme
/// bölmesi) birleştirir. Girdi ve çıktı: (başlangıç, bitiş, metin).
pub fn merge_texts(v: Vec<(i64, i64, String)>, max_gap_ms: i64) -> Vec<(i64, i64, String)> {
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect::<String>();
    let mut out: Vec<(i64, i64, String)> = vec![];
    for (s, e, t) in v {
        if norm(&t).chars().count() < 2 {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if s - last.1 <= max_gap_ms && norm(&last.2) == norm(&t) {
                last.1 = e;
                continue;
            }
        }
        out.push((s, e, t));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Koyu kenarlı beyaz dikdörtgen "metin" çizer.
    fn frame(w: usize, h: usize, boxes: &[(usize, usize, usize, usize)], bg: u8) -> Vec<u8> {
        let mut g = vec![bg; w * h];
        for &(x, y, bw, bh) in boxes {
            for yy in y.saturating_sub(2)..(y + bh + 2).min(h) {
                for xx in x.saturating_sub(2)..(x + bw + 2).min(w) {
                    g[yy * w + xx] = 20;
                }
            }
            for yy in y..y + bh {
                for xx in x..x + bw {
                    g[yy * w + xx] = 240;
                }
            }
        }
        g
    }

    #[test]
    fn maske_kenarli_metni_secer() {
        let (w, h) = (120, 40);
        // ince harf çizgileri (altyazı vuruşu birkaç piksel)
        let g = frame(w, h, &[(10, 10, 30, 5)], 128);
        let m = text_mask(&g, w, h, &Options::default());
        assert!(m[12 * w + 20]);
        assert!(!m[2 * w + 2]);
        // kenarsız parlak bölge (gökyüzü) metin değil
        let g = vec![230u8; w * h];
        assert!(text_mask(&g, w, h, &Options::default()).iter().all(|x| !x));
    }

    #[test]
    fn satirlari_bolumler() {
        let (w, h) = (160, 48);
        let o = Options { min_cells: 3, ..Default::default() };
        let mut s = Segmenter::new(w, h, o);
        let glyphs = |x0: usize, n: usize| (0..n).map(|k| (x0 + k * 12, 14, 7, 12)).collect::<Vec<_>>();
        let a = frame(w, h, &glyphs(10, 5), 90);
        let b = frame(w, h, &glyphs(85, 6), 90);
        let blank = vec![90u8; w * h];
        let seq = [&blank, &a, &a, &a, &a, &b, &b, &b, &blank, &blank, &a, &blank];
        for (k, f) in seq.iter().enumerate() {
            s.push(k as i64 * 200, f);
        }
        let v = s.finish(seq.len() as i64 * 200);
        let t: Vec<(i64, i64)> = v.iter().map(|f| (f.start, f.end)).collect();
        // son "a" tek örnek (200 ms) < 300 ms: atılır
        assert_eq!(t, vec![(200, 1000), (1000, 1600)]);
        assert!(v[0].pgm.starts_with(b"P5"));
    }

    #[test]
    fn ara_karelerle_kare_dogru_sinir() {
        let (w, h) = (160, 48);
        let o = Options { min_cells: 3, min_ms: 0, ..Default::default() };
        let glyphs = |x0: usize, n: usize| (0..n).map(|k| (x0 + k * 12, 14, 7, 12)).collect::<Vec<_>>();
        let a = frame(w, h, &glyphs(10, 5), 90);
        let b = frame(w, h, &glyphs(85, 6), 90);
        let blank = vec![90u8; w * h];
        // 25 kare: boş 0-6, a 7-14, b 15-19, boş 20-24; her 5. kare örnek (40 ms/kare)
        let at = |k: usize| if k < 7 { &blank } else if k < 15 { &a } else if k < 20 { &b } else { &blank };
        let mut s = Segmenter::new(w, h, o.clone());
        for k in 0..25 {
            let t = k as i64 * 40;
            if k % 5 == 0 { s.push(t, at(k)) } else { s.push_between(t, at(k)) }
        }
        let v: Vec<(i64, i64)> = s.finish(1000).iter().map(|f| (f.start, f.end)).collect();
        assert_eq!(v, vec![(7 * 40, 15 * 40), (15 * 40, 20 * 40)]);
        // ara kare verilmezse eski davranış (örnek zamanları)
        let mut s = Segmenter::new(w, h, o);
        for k in (0..25).step_by(5) { s.push(k as i64 * 40, at(k)) }
        let v: Vec<(i64, i64)> = s.finish(1000).iter().map(|f| (f.start, f.end)).collect();
        assert_eq!(v, vec![(400, 600), (600, 800)]);
    }

    #[test]
    fn gurultu_temizligi() {
        assert_eq!(tidy("It's me. Open the. door, ~~\\Nplease, it's cold outside."), "It's me. Open the. door,\\Nplease, it's cold outside.");
        assert_eq!(tidy("Fine. Just this once. ."), "Fine. Just this once.");
        assert_eq!(tidy("- Hey !\\N_ \\N- What ?"), "- Hey!\\N- What?");
        assert_eq!(tidy("Wait..."), "Wait...");
    }

    #[test]
    fn metin_birlestirme() {
        let v = vec![(0, 1000, "Hello".to_string()), (1000, 2000, "hello.".to_string()), (2500, 3000, "x".to_string()), (3000, 4000, "World".to_string())];
        assert_eq!(merge_texts(v, 100), vec![(0, 2000, "Hello".to_string()), (3000, 4000, "World".to_string())]);
    }
}
