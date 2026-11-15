//! VobSub (DVD `.idx` + `.sub`) okuyucu.
//!
//! `.idx` metin dosyası: video boyutu (`size:`), 16 renkli RGB palet (`palette:`) ve izler
//! (`id: en, index: 0`). `.sub` bir MPEG program akışıdır: her altyazı (SPU) özel akış 1
//! (`0xBD`) PES paketlerine bölünür; ilk baytı alt akış numarasıdır (`0x20 + index`).
//! SPU: boyut, denetim dizisi konumu, 2 bitlik (4 renk) satır-aralıklı RLE görüntü; denetim
//! komutları başlangıç/bitiş gecikmesi, renk/saydamlık eşlemesi ve konumu verir.
//! Çıktı PGS okuyucusuyla aynı türdür (`PgsSub`), böylece OCR yolu ortaktır.
use crate::pgs::{Bitmap, PgsSub};

#[derive(Clone, Debug, PartialEq)]
pub struct IdxTrack {
    pub lang: String,
    pub index: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Idx {
    pub width: u32,
    pub height: u32,
    /// RGB
    pub palette: [(u8, u8, u8); 16],
    pub tracks: Vec<IdxTrack>,
}

impl Default for Idx {
    fn default() -> Self {
        // Palet yoksa: 0 siyah, 1 beyaz, gerisi gri tonları (çoğu DVD'de 1 = metin, 2/3 = kenar)
        let mut palette = [(0u8, 0u8, 0u8); 16];
        for (i, p) in palette.iter_mut().enumerate() {
            let v = match i {
                0 => 0,
                1 => 255,
                2 => 0,
                3 => 128,
                _ => (i * 16) as u8,
            };
            *p = (v, v, v);
        }
        Idx { width: 720, height: 480, palette, tracks: vec![] }
    }
}

/// `.idx` dosyasını ayrıştırır (bilinmeyen satırlar yok sayılır). Matroska'da aynı metin izin
/// CodecPrivate verisidir.
pub fn parse_idx(s: &str) -> Idx {
    let mut idx = Idx::default();
    for line in s.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "size" => {
                if let Some((w, h)) = v.split_once('x') {
                    if let (Ok(w), Ok(h)) = (w.trim().parse(), h.trim().parse()) {
                        idx.width = w;
                        idx.height = h;
                    }
                }
            }
            "palette" => {
                for (i, c) in v.split(',').take(16).enumerate() {
                    if let Ok(n) = u32::from_str_radix(c.trim(), 16) {
                        idx.palette[i] = ((n >> 16) as u8, (n >> 8) as u8, n as u8);
                    }
                }
            }
            "id" => {
                // "id: en, index: 0"
                let lang = v.split(',').next().unwrap_or("").trim().to_string();
                let index = v.split("index:").nth(1).and_then(|n| n.trim().parse().ok()).unwrap_or(idx.tracks.len() as u8);
                idx.tracks.push(IdxTrack { lang, index });
            }
            _ => {}
        }
    }
    idx
}

fn be16(b: &[u8], i: usize) -> usize {
    ((b[i] as usize) << 8) | b[i + 1] as usize
}

fn read_pts(b: &[u8]) -> i64 {
    (((b[0] as i64 >> 1) & 7) << 30) | ((b[1] as i64) << 22) | ((b[2] as i64 >> 1) << 15) | ((b[3] as i64) << 7) | (b[4] as i64 >> 1)
}

/// `.sub` (MPEG-PS) akışından alt akış başına SPU paketleri: (alt akış, PTS ms, paket).
pub fn demux(b: &[u8]) -> Vec<(u8, i64, Vec<u8>)> {
    let mut out = Vec::new();
    // alt akış → (pts, beklenen boyut, tampon)
    let mut cur: std::collections::HashMap<u8, (i64, usize, Vec<u8>)> = Default::default();
    let mut i = 0;
    while i + 6 <= b.len() {
        if !(b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1) {
            i += 1;
            continue;
        }
        let code = b[i + 3];
        match code {
            0xBA => {
                if i + 14 <= b.len() && b[i + 4] & 0xC0 == 0x40 {
                    i += 14 + (b[i + 13] & 7) as usize;
                } else {
                    i += 12;
                }
            }
            0xB9 => break,
            0xBD => {
                let len = be16(b, i + 4);
                let end = (i + 6 + len).min(b.len());
                let p = i + 6;
                let data;
                let mut pts = None;
                if p + 3 <= end && b[p] & 0xC0 == 0x80 {
                    // MPEG-2 PES başlığı
                    let flags = b[p + 1];
                    let hlen = b[p + 2] as usize;
                    if flags & 0x80 != 0 && p + 8 <= end {
                        pts = Some(read_pts(&b[p + 3..p + 8]) / 90);
                    }
                    data = p + 3 + hlen;
                } else {
                    // MPEG-1: doldurma baytları, isteğe bağlı STD ve PTS
                    let mut q = p;
                    while q < end && b[q] == 0xFF {
                        q += 1;
                    }
                    if q < end && b[q] & 0xC0 == 0x40 {
                        q += 2;
                    }
                    if q + 5 <= end && b[q] & 0xF0 == 0x20 {
                        pts = Some(read_pts(&b[q..q + 5]) / 90);
                        q += 5;
                    } else if q + 10 <= end && b[q] & 0xF0 == 0x30 {
                        pts = Some(read_pts(&b[q..q + 5]) / 90);
                        q += 10;
                    } else if q < end && b[q] == 0x0F {
                        q += 1;
                    }
                    data = q;
                }
                if data < end {
                    let sid = b[data];
                    let payload = &b[data + 1..end];
                    if (0x20..0x40).contains(&sid) {
                        let e = cur.entry(sid).or_insert((0, 0, Vec::new()));
                        if pts.is_some() || e.2.is_empty() {
                            *e = (pts.unwrap_or(e.0), 0, Vec::new());
                        }
                        e.2.extend_from_slice(payload);
                        if e.1 == 0 && e.2.len() >= 2 {
                            e.1 = be16(&e.2, 0);
                        }
                        if e.1 >= 4 && e.2.len() >= e.1 {
                            let mut pkt = std::mem::take(&mut e.2);
                            pkt.truncate(e.1);
                            out.push((sid - 0x20, e.0, pkt));
                            e.1 = 0;
                        }
                    }
                }
                i = end;
            }
            c if c >= 0xBB => {
                i += 6 + be16(b, i + 4);
            }
            _ => i += 1,
        }
    }
    out
}

/// Çözülmüş tek SPU: konum, boyut, 2 bitlik pikseller, renk ve saydamlık eşlemesi, gecikmeler (ms).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Spu {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub px: Vec<u8>,
    /// 4 renk için palet indeksi (0 = arka plan, 1 = metin, 2/3 = vurgu/kenar)
    pub colors: [u8; 4],
    /// 0–15
    pub alpha: [u8; 4],
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub forced: bool,
}

fn decode_field(b: &[u8], mut off: usize, end: usize, w: usize, rows: impl Iterator<Item = usize>, px: &mut [u8]) {
    let mut nib = 0usize; // off'tan itibaren yarım bayt sayacı
    let get = |off: &mut usize, nib: &mut usize| -> usize {
        if *off >= end {
            return 0;
        }
        let v = if *nib % 2 == 0 { b[*off] >> 4 } else { b[*off] & 0xF };
        *nib += 1;
        if *nib % 2 == 0 {
            *off += 1;
        }
        v as usize
    };
    for y in rows {
        let mut x = 0;
        while x < w {
            let mut v = get(&mut off, &mut nib);
            if v < 0x4 {
                v = (v << 4) | get(&mut off, &mut nib);
                if v < 0x10 {
                    v = (v << 4) | get(&mut off, &mut nib);
                    if v < 0x40 {
                        v = (v << 4) | get(&mut off, &mut nib);
                    }
                }
            }
            let c = (v & 3) as u8;
            let mut n = v >> 2;
            if n == 0 || x + n > w {
                n = w - x;
            }
            px[y * w + x..y * w + x + n].fill(c);
            x += n;
            if off >= end {
                return;
            }
        }
        // satır sonunda bayt hizası
        if nib % 2 == 1 {
            nib += 1;
            off += 1;
        }
    }
}

/// Tek SPU paketini çözer (bozuk/eksik paket → None).
pub fn decode_spu(p: &[u8]) -> Option<Spu> {
    if p.len() < 4 {
        return None;
    }
    let size = be16(p, 0).min(p.len());
    let mut dcsq = be16(p, 2);
    let mut spu = Spu { colors: [0, 1, 2, 3], alpha: [0, 15, 15, 15], ..Default::default() };
    let (mut top, mut bot) = (0usize, 0usize);
    let mut started = false;
    let mut guard = 0;
    while dcsq + 4 <= size && guard < 64 {
        guard += 1;
        let delay = (be16(p, dcsq) as i64 * 1024) / 90;
        let next = be16(p, dcsq + 2);
        let mut i = dcsq + 4;
        while i < size {
            let cmd = p[i];
            i += 1;
            match cmd {
                0x00 => {
                    spu.forced = true;
                    spu.start_ms = delay;
                    started = true;
                }
                0x01 => {
                    spu.start_ms = delay;
                    started = true;
                }
                0x02 => spu.end_ms = Some(delay),
                0x03 if i + 2 <= size => {
                    spu.colors = [p[i + 1] & 0xF, p[i + 1] >> 4, p[i] & 0xF, p[i] >> 4];
                    i += 2;
                }
                0x04 if i + 2 <= size => {
                    spu.alpha = [p[i + 1] & 0xF, p[i + 1] >> 4, p[i] & 0xF, p[i] >> 4];
                    i += 2;
                }
                0x05 if i + 6 <= size => {
                    let x1 = ((p[i] as u32) << 4) | (p[i + 1] as u32 >> 4);
                    let x2 = ((p[i + 1] as u32 & 0xF) << 8) | p[i + 2] as u32;
                    let y1 = ((p[i + 3] as u32) << 4) | (p[i + 4] as u32 >> 4);
                    let y2 = ((p[i + 4] as u32 & 0xF) << 8) | p[i + 5] as u32;
                    spu.x = x1;
                    spu.y = y1;
                    spu.width = (x2 + 1).saturating_sub(x1);
                    spu.height = (y2 + 1).saturating_sub(y1);
                    i += 6;
                }
                0x06 if i + 4 <= size => {
                    top = be16(p, i);
                    bot = be16(p, i + 2);
                    i += 4;
                }
                0x07 if i + 2 <= size => i += be16(p, i).max(2),
                _ => break, // 0xFF ya da bilinmeyen/eksik → dizi sonu
            }
        }
        if next <= dcsq {
            break;
        }
        dcsq = next;
    }
    if !started && spu.end_ms.is_none() {
        return None;
    }
    let (w, h) = (spu.width as usize, spu.height as usize);
    if w == 0 || h == 0 || w * h > 4096 * 4096 || top >= size || bot >= size {
        return None;
    }
    let data_end = be16(p, 2).min(size);
    let mut px = vec![0u8; w * h];
    decode_field(p, top, data_end, w, (0..h).step_by(2), &mut px);
    decode_field(p, bot, data_end, w, (1..h).step_by(2), &mut px);
    spu.px = px;
    Some(spu)
}

/// `.sub` baytları + `.idx` bilgisi → görüntüler (PGS türünde). `track`: idx izi (None = ilk iz).
/// Bitiş komutu olmayan altyazı bir sonrakinin başlangıcına (en çok 5 sn) kadar sürer.
pub fn parse(sub: &[u8], idx: &Idx, track: Option<u8>) -> Vec<PgsSub> {
    let packets = demux(sub);
    let track = track.or_else(|| idx.tracks.first().map(|t| t.index)).or_else(|| packets.first().map(|p| p.0)).unwrap_or(0);
    let mut out: Vec<PgsSub> = Vec::new();
    let mut open: Option<usize> = None;
    for (sid, pts, pkt) in packets {
        if sid != track {
            continue;
        }
        let Some(spu) = decode_spu(&pkt) else { continue };
        let start = pts + spu.start_ms;
        if let Some(k) = open.take() {
            let s0 = out[k].start_ms;
            out[k].end_ms = out[k].end_ms.min(start).max(s0 + 1);
        }
        let la: Vec<(u8, u8)> = spu
            .px
            .iter()
            .map(|&c| {
                let (r, g, b) = idx.palette[spu.colors[c as usize] as usize & 15];
                let l = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8;
                (l, spu.alpha[c as usize] * 17)
            })
            .collect();
        let end = match spu.end_ms {
            Some(e) if e > spu.start_ms => pts + e,
            _ => {
                open = Some(out.len());
                start + 5000
            }
        };
        out.push(PgsSub {
            start_ms: start,
            end_ms: end,
            screen_w: idx.width,
            screen_h: idx.height,
            images: vec![Bitmap { x: spu.x, y: spu.y, width: spu.width, height: spu.height, la }],
        });
    }
    out
}

// ---- Üretici (testler ve örnekler için) ----

fn rle_field(px: &[u8], w: usize, rows: impl Iterator<Item = usize>, out: &mut Vec<u8>) {
    let mut nibs: Vec<u8> = Vec::new();
    for y in rows {
        let row = &px[y * w..y * w + w];
        let mut x = 0;
        while x < w {
            let c = row[x] & 3;
            let mut n = 1;
            while x + n < w && row[x + n] & 3 == c && n < 255 {
                n += 1;
            }
            if x + n == w && n > 3 {
                nibs.extend_from_slice(&[0, 0, 0, c]); // satır sonuna kadar
            } else {
                let v = (n << 2) | c as usize;
                let k = if n < 4 { 1 } else if n < 16 { 2 } else if n < 64 { 3 } else { 4 };
                for s in (0..k).rev() {
                    nibs.push(((v >> (4 * s)) & 0xF) as u8);
                }
            }
            x += n;
        }
        if nibs.len() % 2 == 1 {
            nibs.push(0);
        }
    }
    for p in nibs.chunks(2) {
        out.push((p[0] << 4) | p[1]);
    }
}

/// SPU paketi üretir: pikseller 0–3, `colors`/`alpha` eşlemesi, gecikmeler ms.
#[allow(clippy::too_many_arguments)]
pub fn encode_spu(x: u32, y: u32, w: u32, h: u32, px: &[u8], colors: [u8; 4], alpha: [u8; 4], end_ms: i64) -> Vec<u8> {
    let (wu, hu) = (w as usize, h as usize);
    let mut data = Vec::new();
    rle_field(px, wu, (0..hu).step_by(2), &mut data);
    let bot = 4 + data.len();
    rle_field(px, wu, (1..hu).step_by(2), &mut data);
    let dcsq1 = 4 + data.len();
    let (x2, y2) = (x + w - 1, y + h - 1);
    let mut c1 = vec![0, 0, 0, 0, 0x01, 0x03, (colors[3] << 4) | colors[2], (colors[1] << 4) | colors[0], 0x04, (alpha[3] << 4) | alpha[2], (alpha[1] << 4) | alpha[0]];
    c1.extend_from_slice(&[0x05, (x >> 4) as u8, (((x & 0xF) << 4) | (x2 >> 8)) as u8, x2 as u8, (y >> 4) as u8, (((y & 0xF) << 4) | (y2 >> 8)) as u8, y2 as u8]);
    c1.extend_from_slice(&[0x06, 0, 4, (bot >> 8) as u8, bot as u8, 0xFF]);
    let dcsq2 = dcsq1 + c1.len();
    c1[2] = (dcsq2 >> 8) as u8;
    c1[3] = dcsq2 as u8;
    let date = (end_ms * 90 / 1024) as u16;
    let c2 = [(date >> 8) as u8, date as u8, (dcsq2 >> 8) as u8, dcsq2 as u8, 0x02, 0xFF];
    let size = dcsq2 + c2.len();
    let mut p = vec![(size >> 8) as u8, size as u8, (dcsq1 >> 8) as u8, dcsq1 as u8];
    p.extend(data);
    p.extend(c1);
    p.extend_from_slice(&c2);
    p
}

/// SPU paketlerini `.sub` (MPEG-2 PS) akışına sarar: (alt akış, PTS ms, paket).
pub fn mux(packets: &[(u8, i64, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (sid, pts_ms, pkt) in packets {
        for (n, chunk) in pkt.chunks(2000).enumerate() {
            let start = out.len();
            out.extend_from_slice(&[0, 0, 1, 0xBA, 0x44, 0, 4, 0, 4, 1, 0x01, 0x89, 0xC3, 0xF8]);
            let pts = (pts_ms * 90) as u64;
            let hdr: Vec<u8> = if n == 0 {
                vec![0x81, 0x80, 5, 0x21 | (((pts >> 30) & 7) << 1) as u8, (pts >> 22) as u8, (((pts >> 15) << 1) | 1) as u8, (pts >> 7) as u8, ((pts << 1) | 1) as u8]
            } else {
                vec![0x81, 0x00, 0]
            };
            let len = hdr.len() + 1 + chunk.len();
            out.extend_from_slice(&[0, 0, 1, 0xBD, (len >> 8) as u8, len as u8]);
            out.extend(hdr);
            out.push(0x20 + sid);
            out.extend_from_slice(chunk);
            // 2048 bayta tamamla (gerçek dosyalardaki gibi; padding akışı)
            let used = out.len() - start;
            if used % 2048 != 0 && 2048 - used % 2048 >= 6 {
                let pad = 2048 - used % 2048 - 6;
                out.extend_from_slice(&[0, 0, 1, 0xBE, (pad >> 8) as u8, pad as u8]);
                out.extend(std::iter::repeat_n(0xFF, pad));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(w: usize, h: usize) -> Vec<u8> {
        (0..w * h).map(|i| ((i % w) / 7 + (i / w) / 3) as u8 % 4).collect()
    }

    #[test]
    fn idx_parses_size_palette_tracks() {
        let s = "# VobSub index file, v7\nsize: 720x576\norg: 0, 0\npalette: 000000, ffffff, 808080, ff0000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0102ff\n\nid: en, index: 0\ntimestamp: 00:00:01:000, filepos: 000000000\nid: tr, index: 1\n";
        let idx = parse_idx(s);
        assert_eq!((idx.width, idx.height), (720, 576));
        assert_eq!(idx.palette[1], (255, 255, 255));
        assert_eq!(idx.palette[3], (255, 0, 0));
        assert_eq!(idx.palette[15], (1, 2, 255));
        assert_eq!(idx.tracks, vec![IdxTrack { lang: "en".into(), index: 0 }, IdxTrack { lang: "tr".into(), index: 1 }]);
    }

    #[test]
    fn spu_roundtrip_exact_pixels() {
        for (w, h) in [(1, 1), (5, 2), (64, 9), (300, 37), (701, 50)] {
            let px = pattern(w, h);
            let p = encode_spu(10, 400, w as u32, h as u32, &px, [0, 1, 2, 3], [0, 15, 15, 8], 2500);
            let s = decode_spu(&p).unwrap();
            assert_eq!((s.x, s.y, s.width, s.height), (10, 400, w as u32, h as u32));
            assert_eq!(s.px, px, "{}x{}", w, h);
            assert_eq!(s.alpha, [0, 15, 15, 8]);
            assert_eq!(s.start_ms, 0);
            // 1024/90 ms birimine yuvarlanır
            assert!((s.end_ms.unwrap() - 2500).abs() < 12);
        }
    }

    #[test]
    fn long_runs_and_full_rows() {
        let (w, h) = (720, 4);
        let mut px = vec![0u8; w * h];
        px[w + 100..w + 400].fill(1); // 300 piksel (255'ten uzun, bölünür)
        px[2 * w..3 * w].fill(2); // tüm satır
        let p = encode_spu(0, 0, w as u32, h as u32, &px, [0, 1, 2, 3], [0, 15, 15, 15], 1000);
        assert_eq!(decode_spu(&p).unwrap().px, px);
    }

    #[test]
    fn sub_stream_demux_and_parse() {
        let px = pattern(200, 30);
        let a = encode_spu(100, 380, 200, 30, &px, [0, 1, 2, 3], [0, 15, 15, 15], 2000);
        let b = encode_spu(100, 20, 200, 30, &px, [0, 1, 2, 3], [0, 15, 15, 15], 1500);
        // büyük paket: birden çok PES'e bölünür
        let big_px = pattern(700, 120);
        let c = encode_spu(10, 300, 700, 120, &big_px, [0, 1, 2, 3], [0, 15, 15, 15], 3000);
        assert!(c.len() > 2000);
        let other = encode_spu(0, 0, 10, 2, &pattern(10, 2), [0, 1, 2, 3], [0, 15, 15, 15], 100);
        let sub = mux(&[(0, 1000, a), (1, 1200, other), (0, 5000, b), (0, 9000, c)]);
        assert_eq!(sub.len() % 2048, 0);
        let packets = demux(&sub);
        assert_eq!(packets.len(), 4);
        let mut idx = parse_idx("size: 720x480\npalette: 000000, ffffff, 000000, 808080\nid: en, index: 0\nid: tr, index: 1\n");
        let subs = parse(&sub, &idx, None);
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0].start_ms, 1000);
        assert!((subs[0].end_ms - 3000).abs() < 12);
        assert!(!subs[0].is_top());
        assert!(subs[1].is_top());
        assert_eq!(subs[2].images[0].width, 700);
        // metin rengi (1) beyaz ve opak; arka plan saydam
        let la = &subs[0].images[0].la;
        assert!(la.contains(&(255, 255)) && la.contains(&(0, 0)));
        let k = big_px.iter().position(|&c| c == 3).unwrap();
        assert_eq!(subs[2].images[0].la[k], (128, 255));
        // ikinci iz
        idx.tracks.reverse();
        let t1 = parse(&sub, &idx, None);
        assert_eq!(t1.len(), 1);
        assert_eq!(t1[0].start_ms, 1200);
    }

    #[test]
    fn missing_stop_ends_at_next_start() {
        let px = pattern(20, 4);
        let mut a = encode_spu(0, 0, 20, 4, &px, [0, 1, 2, 3], [0, 15, 15, 15], 0);
        // ikinci denetim dizisini (bitiş) kaldır: ilk dizinin "sonraki" işaretçisi kendini göstersin
        let dcsq1 = be16(&a, 2);
        a[dcsq1 + 2] = (dcsq1 >> 8) as u8;
        a[dcsq1 + 3] = dcsq1 as u8;
        let b = encode_spu(0, 0, 20, 4, &px, [0, 1, 2, 3], [0, 15, 15, 15], 1000);
        let subs = parse(&mux(&[(0, 1000, a), (0, 2500, b)]), &Idx::default(), Some(0));
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].end_ms, 2500);
    }

    #[test]
    fn garbage_does_not_panic() {
        let mut seed = 12345u64;
        let mut r = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let good = mux(&[(0, 1000, encode_spu(5, 5, 50, 10, &pattern(50, 10), [0, 1, 2, 3], [0, 15, 15, 15], 900))]);
        for _ in 0..3000 {
            let mut b = good.clone();
            for _ in 0..(r() % 8 + 1) {
                let i = r() as usize % b.len();
                b[i] = r() as u8;
            }
            b.truncate(r() as usize % (b.len() + 1));
            let _ = parse(&b, &Idx::default(), None);
            let n = r() as usize % 64;
            let junk: Vec<u8> = (0..n).map(|_| r() as u8).collect();
            let _ = decode_spu(&junk);
        }
    }
}
