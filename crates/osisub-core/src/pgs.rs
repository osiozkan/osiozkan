//! PGS (Blu-ray `.sup`, HDMV Presentation Graphics) okuyucu.
//!
//! Bölümler: `PG` + PTS/DTS (90 kHz) + tür + boyut. PCS (0x16) ekranda gösterilecek nesneleri,
//! PDS (0x14) YCbCrA paleti, ODS (0x15) RLE ile sıkıştırılmış bitmap'i taşır; nesnesiz bir PCS
//! ekranı temizler. Çıktı her görünüm için (başlangıç, bitiş, konum, 8 bit gri + alfa) bitmap'tir;
//! OCR için `ocr_gray` metni koyu, arka planı beyaz yapar.
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Bitmap {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Piksel başına (parlaklık 0–255, alfa 0–255)
    pub la: Vec<(u8, u8)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PgsSub {
    pub start_ms: i64,
    pub end_ms: i64,
    /// Video çözünürlüğü (PCS)
    pub screen_w: u32,
    pub screen_h: u32,
    /// Aynı anda gösterilen nesneler (genelde 1, bazen üst + alt satır)
    pub images: Vec<Bitmap>,
}

impl PgsSub {
    /// Görüntü ekranın üst yarısında mı (→ `\an8`)?
    pub fn is_top(&self) -> bool {
        let Some(top) = self.images.iter().map(|b| b.y).min() else { return false };
        let bottom = self.images.iter().map(|b| b.y + b.height).max().unwrap_or(0);
        (top + bottom) / 2 < self.screen_h / 2
    }
}

fn be16(b: &[u8], i: usize) -> u32 {
    ((b[i] as u32) << 8) | b[i + 1] as u32
}

/// RLE çözücü: palet indeksleri (genişlik × yükseklik; eksik satırlar 0).
pub fn decode_rle(data: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    let (mut x, mut y, mut i) = (0usize, 0usize, 0usize);
    let mut put = |x: &mut usize, y: usize, n: usize, c: u8| {
        for _ in 0..n {
            if *x < w && y < h {
                out[y * w + *x] = c;
            }
            *x += 1;
        }
    };
    while i < data.len() && y < h {
        let b = data[i];
        i += 1;
        if b != 0 {
            put(&mut x, y, 1, b);
            continue;
        }
        let Some(&f) = data.get(i) else { break };
        i += 1;
        if f == 0 {
            x = 0;
            y += 1;
            continue;
        }
        let long = f & 0x40 != 0;
        let mut n = (f & 0x3F) as usize;
        if long {
            let Some(&l) = data.get(i) else { break };
            i += 1;
            n = (n << 8) | l as usize;
        }
        let c = if f & 0x80 != 0 {
            let Some(&c) = data.get(i) else { break };
            i += 1;
            c
        } else {
            0
        };
        put(&mut x, y, n, c);
    }
    out
}

/// RLE kodlayıcı (test ve örnek dosya üretimi için).
pub fn encode_rle(px: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..h {
        let row = &px[y * w..(y + 1) * w];
        let mut x = 0;
        while x < w {
            let c = row[x];
            let mut n = 1;
            while x + n < w && row[x + n] == c && n < 0x3FFF {
                n += 1;
            }
            if c != 0 && n < 3 {
                for _ in 0..n {
                    out.push(c);
                }
            } else {
                out.push(0);
                let long = n > 63;
                let mut f = if long { 0x40 | ((n >> 8) as u8 & 0x3F) } else { n as u8 };
                if c != 0 {
                    f |= 0x80;
                }
                out.push(f);
                if long {
                    out.push((n & 0xFF) as u8);
                }
                if c != 0 {
                    out.push(c);
                }
            }
            x += n;
        }
        out.push(0);
        out.push(0);
    }
    out
}

struct Obj {
    w: usize,
    h: usize,
    data: Vec<u8>,
}

/// `.sup` dosyasını çözer. Bozuk bölümler atlanır.
pub fn parse(b: &[u8]) -> Result<Vec<PgsSub>, String> {
    if b.len() < 13 || &b[..2] != b"PG" {
        return Err("PGS (.sup) değil".into());
    }
    let mut palettes: HashMap<u8, [(u8, u8); 256]> = HashMap::new();
    let mut objects: HashMap<u16, Obj> = HashMap::new();
    let mut out: Vec<PgsSub> = Vec::new();
    // Bekleyen kompozisyon: (pts, ekran, palet, [(nesne, x, y, kırpma)])
    type Comp = (i64, u32, u32, u8, Vec<(u16, u32, u32)>);
    let mut pending: Option<Comp> = None;
    let mut open: Option<usize> = None; // bitişi bekleyen çıktı
    let mut i = 0;
    while i + 13 <= b.len() {
        if &b[i..i + 2] != b"PG" {
            // eşzamanlamayı yeniden bul
            i += 1;
            continue;
        }
        let pts = (((b[i + 2] as u64) << 24) | ((b[i + 3] as u64) << 16) | ((b[i + 4] as u64) << 8) | b[i + 5] as u64) as i64;
        let ms = pts / 90;
        let ty = b[i + 10];
        let size = be16(b, i + 11) as usize;
        let p0 = i + 13;
        let Some(seg) = b.get(p0..p0 + size) else { break };
        i = p0 + size;
        match ty {
            0x16 if seg.len() >= 11 => {
                let (sw, sh) = (be16(seg, 0), be16(seg, 2));
                let pal = seg[9];
                let n = seg[10] as usize;
                let mut objs = Vec::new();
                let mut k = 11;
                for _ in 0..n {
                    if k + 8 > seg.len() {
                        break;
                    }
                    let id = be16(seg, k) as u16;
                    let cropped = seg[k + 3] & 0x40 != 0;
                    let (x, y) = (be16(seg, k + 4), be16(seg, k + 6));
                    objs.push((id, x, y));
                    k += if cropped { 16 } else { 8 };
                }
                pending = Some((ms, sw, sh, pal, objs));
            }
            0x14 if seg.len() >= 2 => {
                let id = seg[0];
                let pal = palettes.entry(id).or_insert([(0, 0); 256]);
                for e in seg[2..].chunks_exact(5) {
                    // Y (16–235) → 0–255 parlaklık; renk bileşenleri OCR için gereksiz
                    let l = ((e[1].clamp(16, 235) as u32 - 16) * 255 / 219) as u8;
                    pal[e[0] as usize] = (l, e[4]);
                }
            }
            0x15 if seg.len() >= 4 => {
                let id = be16(seg, 0) as u16;
                let seq = seg[3];
                if seq & 0x80 != 0 {
                    if seg.len() < 11 {
                        continue;
                    }
                    let (w, h) = (be16(seg, 7) as usize, be16(seg, 9) as usize);
                    objects.insert(id, Obj { w, h, data: seg[11..].to_vec() });
                } else if let Some(o) = objects.get_mut(&id) {
                    o.data.extend_from_slice(&seg[4..]);
                }
            }
            0x80 => {
                // Görüntü kümesi tamam: kompozisyonu uygula
                let Some((t, sw, sh, pal_id, objs)) = pending.take() else { continue };
                if let Some(k) = open.take() {
                    if out[k].end_ms <= out[k].start_ms {
                        out[k].end_ms = t;
                    }
                }
                if objs.is_empty() {
                    continue;
                }
                let pal = palettes.get(&pal_id).copied().unwrap_or([(0, 0); 256]);
                let mut images = Vec::new();
                for (id, x, y) in objs {
                    let Some(o) = objects.get(&id) else { continue };
                    if o.w == 0 || o.h == 0 || o.w * o.h > 16_000_000 {
                        continue;
                    }
                    let idx = decode_rle(&o.data, o.w, o.h);
                    images.push(Bitmap { x, y, width: o.w as u32, height: o.h as u32, la: idx.iter().map(|&c| pal[c as usize]).collect() });
                }
                if images.is_empty() {
                    continue;
                }
                out.push(PgsSub { start_ms: t, end_ms: t, screen_w: sw, screen_h: sh, images });
                open = Some(out.len() - 1);
            }
            _ => {}
        }
    }
    // Bitişi olmayan son altyazı: 5 sn varsay
    if let Some(k) = open {
        if out[k].end_ms <= out[k].start_ms {
            out[k].end_ms = out[k].start_ms + 5000;
        }
    }
    Ok(out)
}

/// OCR için tek gri görüntü (P5 PGM): nesneler dikey sırayla birleştirilir, metin koyu, arka plan
/// beyaz, `pad` piksel kenar boşluğu. (genişlik, yükseklik, pikseller)
pub fn ocr_gray(sub: &PgsSub, pad: u32) -> (u32, u32, Vec<u8>) {
    let mut imgs: Vec<&Bitmap> = sub.images.iter().collect();
    imgs.sort_by_key(|b| b.y);
    let x0 = imgs.iter().map(|b| b.x).min().unwrap_or(0);
    let x1 = imgs.iter().map(|b| b.x + b.width).max().unwrap_or(0);
    let y0 = imgs.iter().map(|b| b.y).min().unwrap_or(0);
    let y1 = imgs.iter().map(|b| b.y + b.height).max().unwrap_or(0);
    let (w, h) = (x1 - x0 + 2 * pad, y1 - y0 + 2 * pad);
    let mut px = vec![255u8; (w * h) as usize];
    for b in imgs {
        for yy in 0..b.height {
            for xx in 0..b.width {
                let (l, a) = b.la[(yy * b.width + xx) as usize];
                // parlak + opak → koyu; kenar (koyu) ve saydam → beyaz
                let ink = (l as u32 * a as u32 / 255) as u8;
                let tx = b.x - x0 + pad + xx;
                let ty = b.y - y0 + pad + yy;
                let k = (ty * w + tx) as usize;
                px[k] = px[k].min(255 - ink);
            }
        }
    }
    (w, h, px)
}

/// OCR metinlerinden betik: boş metinli görüntüler atlanır, üstteki görüntüler `{\\an8}` alır,
/// PlayRes PGS ekran boyutuna ayarlanır.
pub fn to_script(subs: &[PgsSub], texts: &[String]) -> crate::ass::Script {
    let mut ds = Vec::new();
    for (s, t) in subs.iter().zip(texts) {
        if t.trim().is_empty() {
            continue;
        }
        let text = if s.is_top() { format!("{{\\an8}}{}", t) } else { t.clone() };
        ds.push(crate::ass::Dialogue { start_time: s.start_ms, end_time: s.end_ms, text, ..Default::default() });
    }
    let mut sc = crate::ass::Script::from_dialogues(ds);
    if let Some(s) = subs.first() {
        if s.screen_w > 0 && s.screen_h > 0 {
            sc.set_info("PlayResX", &s.screen_w.to_string());
            sc.set_info("PlayResY", &s.screen_h.to_string());
        }
    }
    sc
}

pub fn to_pgm(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
    let mut v = format!("P5\n{} {}\n255\n", w, h).into_bytes();
    v.extend_from_slice(px);
    v
}

/// Basit `.sup` üretici (testler ve örnekler için): her öğe (başlangıç, bitiş, x, y, w, h, gri+alfa).
pub fn encode(subs: &[(i64, i64, u32, u32, u32, u32, Vec<(u8, u8)>)], screen: (u32, u32)) -> Vec<u8> {
    let mut out = Vec::new();
    let seg = |out: &mut Vec<u8>, ms: i64, ty: u8, data: &[u8]| {
        let pts = (ms * 90) as u32;
        out.extend_from_slice(b"PG");
        out.extend_from_slice(&pts.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.push(ty);
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
    };
    for (n, (st, en, x, y, w, h, la)) in subs.iter().enumerate() {
        // palet: (parlaklık, alfa) çiftlerinden
        let mut pal: Vec<(u8, u8)> = vec![(0, 0)];
        let idx: Vec<u8> = la
            .iter()
            .map(|p| {
                if p.1 == 0 {
                    return 0;
                }
                match pal.iter().position(|q| q == p) {
                    Some(k) => k as u8,
                    None if pal.len() < 255 => {
                        pal.push(*p);
                        (pal.len() - 1) as u8
                    }
                    None => 1,
                }
            })
            .collect();
        let mut pcs = Vec::new();
        pcs.extend_from_slice(&(screen.0 as u16).to_be_bytes());
        pcs.extend_from_slice(&(screen.1 as u16).to_be_bytes());
        pcs.push(0x10);
        pcs.extend_from_slice(&(n as u16 * 2).to_be_bytes());
        pcs.extend_from_slice(&[0x80, 0, 0, 1]);
        pcs.extend_from_slice(&[0, 0, 0, 0]);
        pcs.extend_from_slice(&(*x as u16).to_be_bytes());
        pcs.extend_from_slice(&(*y as u16).to_be_bytes());
        seg(&mut out, *st, 0x16, &pcs);
        let mut pds = vec![0u8, 0];
        for (k, (l, a)) in pal.iter().enumerate() {
            let yv = 16 + (*l as u32 * 219 / 255) as u8;
            pds.extend_from_slice(&[k as u8, yv, 128, 128, *a]);
        }
        seg(&mut out, *st, 0x14, &pds);
        let rle = encode_rle(&idx, *w as usize, *h as usize);
        // ODS: ilk parça + gerekirse devam parçaları (bölüm en çok 65535 bayt)
        let total = rle.len() + 4;
        let mut first = vec![0, 0, 0];
        let chunks: Vec<&[u8]> = rle.chunks(60000).collect();
        first.push(if chunks.len() <= 1 { 0xC0 } else { 0x80 });
        first.extend_from_slice(&[(total >> 16) as u8, (total >> 8) as u8, total as u8]);
        first.extend_from_slice(&(*w as u16).to_be_bytes());
        first.extend_from_slice(&(*h as u16).to_be_bytes());
        first.extend_from_slice(chunks.first().copied().unwrap_or(&[]));
        seg(&mut out, *st, 0x15, &first);
        for (k, c) in chunks.iter().enumerate().skip(1) {
            let mut d = vec![0, 0, 0, if k + 1 == chunks.len() { 0x40 } else { 0 }];
            d.extend_from_slice(c);
            seg(&mut out, *st, 0x15, &d);
        }
        seg(&mut out, *st, 0x80, &[]);
        // temizle
        let mut clr = Vec::new();
        clr.extend_from_slice(&(screen.0 as u16).to_be_bytes());
        clr.extend_from_slice(&(screen.1 as u16).to_be_bytes());
        clr.push(0x10);
        clr.extend_from_slice(&(n as u16 * 2 + 1).to_be_bytes());
        clr.extend_from_slice(&[0, 0, 0, 0]);
        seg(&mut out, *en, 0x16, &clr);
        seg(&mut out, *en, 0x80, &[]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_roundtrip() {
        let (w, h) = (130, 3);
        let mut px = vec![0u8; w * h];
        for (i, p) in px.iter_mut().enumerate() {
            *p = match i % 130 {
                0..=69 => 0,
                70 => 5,
                71..=72 => 6,
                _ => 7,
            };
        }
        let enc = encode_rle(&px, w, h);
        assert_eq!(decode_rle(&enc, w, h), px);
    }

    #[test]
    fn parse_encoded_file() {
        let la: Vec<(u8, u8)> = (0..40 * 10).map(|i| if i % 3 == 0 { (255, 255) } else if i % 3 == 1 { (0, 255) } else { (0, 0) }).collect();
        let sup = encode(&[(1000, 2500, 100, 900, 40, 10, la.clone()), (3000, 4000, 100, 50, 40, 10, la.clone())], (1920, 1080));
        let subs = parse(&sup).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!((subs[0].start_ms, subs[0].end_ms), (1000, 2500));
        assert_eq!((subs[1].start_ms, subs[1].end_ms), (3000, 4000));
        assert!(!subs[0].is_top() && subs[1].is_top());
        assert_eq!(subs[0].images[0].la[0], (255, 255));
        assert_eq!(subs[0].images[0].la[1], (0, 255));
        assert_eq!(subs[0].images[0].la[2].1, 0);
        let (w, h, g) = ocr_gray(&subs[0], 4);
        assert_eq!((w, h), (48, 18));
        assert_eq!(g[(4 * w + 4) as usize], 0); // beyaz dolgu → koyu
        assert_eq!(g[(4 * w + 5) as usize], 255); // siyah kenar → beyaz
        assert!(to_pgm(w, h, &g).starts_with(b"P5\n48 18\n255\n"));
        assert!(parse(b"bozuk").is_err());
    }
}
