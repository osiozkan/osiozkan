//! Sesle yeniden zamanlama (TV → BD gibi farklı kurgu/gecikmeli sürümler için; "Sushi" benzeri):
//! altyazının zamanlandığı eski sesteki her satır grubu, yeni seste FFT çapraz-ilintiyle aranır
//! ve grup o kadar kaydırılır. Gruplar zaman sırasıyla işlenir; arama bir öncekinin kaymasının
//! çevresinde yapılır, güveni düşük eşleşmede önceki kayma kullanılır.
use crate::Waveform;
use rustfft::{num_complex::Complex, FftPlanner};

/// İlinti için örnekleme seyreltmesi (8 kHz → 2 kHz).
const DECIM: usize = 4;

#[derive(Clone, Debug)]
pub struct ResyncOptions {
    /// Bir önceki gruba göre aranacak en büyük kayma farkı (ms)
    pub search_ms: i64,
    /// İlk grup için arama aralığı (ms)
    pub first_search_ms: i64,
    /// Aralarında bundan kısa boşluk olan satırlar aynı gruptur (ms)
    pub group_gap_ms: i64,
    /// Şablonun iki yanına eklenen ses (ms)
    pub pad_ms: i64,
    /// Bu ilintinin altı güvensiz sayılır (0..1)
    pub min_score: f64,
}

impl Default for ResyncOptions {
    fn default() -> Self {
        ResyncOptions { search_ms: 10_000, first_search_ms: 60_000, group_gap_ms: 1500, pad_ms: 300, min_score: 0.45 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub start_ms: i64,
    pub end_ms: i64,
    pub offset_ms: i64,
    pub score: f64,
    /// eşleşme güvensizdi; önceki kayma kullanıldı
    pub fallback: bool,
}

fn decimate(w: &Waveform) -> Vec<f32> {
    w.samples.chunks(DECIM).map(|c| c.iter().map(|&v| v as f32).sum::<f32>() / c.len() as f32).collect()
}

fn ms_to_idx(ms: i64, rate: f64) -> i64 {
    (ms as f64 * rate / 1000.0).round() as i64
}

/// `tpl` şablonunun `reg` içindeki en iyi normalize ilinti konumu ve puanı.
fn best_match(tpl: &[f32], reg: &[f32], planner: &mut FftPlanner<f32>) -> Option<(usize, f64)> {
    if tpl.len() < 16 || reg.len() < tpl.len() {
        return None;
    }
    let mean = tpl.iter().sum::<f32>() / tpl.len() as f32;
    let t: Vec<f32> = tpl.iter().map(|v| v - mean).collect();
    let tnorm = t.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
    if tnorm < 1e-6 {
        return None; // sessiz şablon
    }
    let n = (reg.len() + t.len()).next_power_of_two();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let mut a: Vec<Complex<f32>> = reg.iter().map(|&v| Complex::new(v, 0.0)).chain(std::iter::repeat(Complex::new(0.0, 0.0))).take(n).collect();
    let mut b: Vec<Complex<f32>> = t.iter().map(|&v| Complex::new(v, 0.0)).chain(std::iter::repeat(Complex::new(0.0, 0.0))).take(n).collect();
    fft.process(&mut a);
    fft.process(&mut b);
    for (x, y) in a.iter_mut().zip(b.iter()) {
        *x *= y.conj();
    }
    ifft.process(&mut a);
    // bölgenin kayan pencere enerjisi (ortalama çıkarılmış şablonla ilinti ortalamadan bağımsız)
    let m = t.len();
    let mut pre = vec![0f64; reg.len() + 1];
    let mut pre2 = vec![0f64; reg.len() + 1];
    for (i, &v) in reg.iter().enumerate() {
        pre[i + 1] = pre[i] + v as f64;
        pre2[i + 1] = pre2[i] + (v as f64) * (v as f64);
    }
    let mut best: Option<(usize, f64)> = None;
    for k in 0..=(reg.len() - m) {
        let s = pre[k + m] - pre[k];
        let s2 = pre2[k + m] - pre2[k];
        let var = (s2 - s * s / m as f64).max(0.0);
        if var < 1e-6 {
            continue;
        }
        let c = a[k].re as f64 / n as f64;
        let score = c / (tnorm * var.sqrt());
        if best.map_or(true, |(_, b)| score > b) {
            best = Some((k, score));
        }
    }
    best
}

/// Satır aralıklarını (başlangıç, bitiş) gruplara ayırıp her grubun kaymasını bulur.
pub fn compute_offsets(old: &Waveform, new: &Waveform, lines: &[(i64, i64)], o: &ResyncOptions) -> Vec<Group> {
    let rate = old.sample_rate as f64 / DECIM as f64;
    let (src, dst) = (decimate(old), decimate(new));
    let mut iv: Vec<(i64, i64)> = lines.iter().copied().filter(|(a, b)| b > a).collect();
    iv.sort_unstable();
    // gruplar: (aralık, üye satırlar)
    let mut groups: Vec<((i64, i64), Vec<(i64, i64)>)> = vec![];
    for (a, b) in iv {
        match groups.last_mut() {
            Some((g, m)) if a - g.1 < o.group_gap_ms => {
                g.1 = g.1.max(b);
                m.push((a, b));
            }
            _ => groups.push(((a, b), vec![(a, b)])),
        }
    }
    let mut planner = FftPlanner::new();
    let mut out = vec![];
    let mut prev: Option<i64> = None;
    let find = |a: i64, b: i64, prev: Option<i64>, planner: &mut FftPlanner<f32>| -> (i64, f64, bool) {
        let (ta, tb) = (ms_to_idx(a - o.pad_ms, rate).max(0), ms_to_idx(b + o.pad_ms, rate).min(src.len() as i64));
        let center = prev.unwrap_or(0);
        let search = if prev.is_some() { o.search_ms } else { o.first_search_ms };
        let (ra, rb) = (ms_to_idx(a - o.pad_ms + center - search, rate).max(0), ms_to_idx(b + o.pad_ms + center + search, rate).min(dst.len() as i64));
        let found = if tb > ta && rb > ra { best_match(&src[ta as usize..tb as usize], &dst[ra as usize..rb as usize], planner) } else { None };
        match found {
            Some((k, s)) if s >= o.min_score => {
                let off_idx = ra + k as i64 - ta;
                ((off_idx as f64 * 1000.0 / rate).round() as i64, s, false)
            }
            Some((_, s)) => (center, s, true),
            None => (center, 0.0, true),
        }
    };
    for ((a, b), members) in groups {
        let (offset, score, fallback) = find(a, b, prev, &mut planner);
        // grup bir kurgu kesimine denk gelmiş olabilir: zayıfsa satır satır yeniden dene
        if (fallback || score < 0.7) && members.len() > 1 {
            for (ma, mb) in members {
                let (off, sc, fb) = find(ma, mb, prev, &mut planner);
                if !fb {
                    prev = Some(off);
                }
                out.push(Group { start_ms: ma, end_ms: mb, offset_ms: off, score: sc, fallback: fb });
            }
            continue;
        }
        if !fallback {
            prev = Some(offset);
        }
        out.push(Group { start_ms: a, end_ms: b, offset_ms: offset, score, fallback });
    }
    out
}

/// Bir zamanın bağlı olduğu grubun kayması (gruplar arası boşlukta en yakın grup).
pub fn offset_for(groups: &[Group], t: i64) -> i64 {
    groups
        .iter()
        .min_by_key(|g| if t >= g.start_ms && t <= g.end_ms { 0 } else { (g.start_ms - t).abs().min((g.end_ms - t).abs()) })
        .map_or(0, |g| g.offset_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Konuşmaya benzer, tekrar etmeyen sinyal (xorshift gürültü × yavaş zarf).
    fn speechy(secs: usize, seed: u64) -> Vec<i16> {
        let n = secs * 8000;
        let mut x = seed;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let noise = ((x >> 40) as i64 % 2000 - 1000) as f64;
            let env = ((i as f64 / 8000.0 * 3.1).sin() * (i as f64 / 8000.0 * 0.7).cos()).abs() + 0.05;
            out.push((noise * env * 8.0) as i16);
        }
        out
    }

    #[test]
    fn finds_shift_and_inserted_cut() {
        let src = speechy(60, 0x1234_5678_9abc);
        // yeni sürüm: başa 1 sn sessizlik (gecikme), 20. saniyeye 3 sn ek sahne (gürültü)
        let mut dst: Vec<i16> = vec![0; 8000];
        dst.extend_from_slice(&src[..20 * 8000]);
        dst.extend(speechy(3, 0xdead_beef));
        dst.extend_from_slice(&src[20 * 8000..]);
        let (old, new) = (Waveform { sample_rate: 8000, samples: src }, Waveform { sample_rate: 8000, samples: dst });
        let lines = vec![(5000, 7000), (7500, 9000), (12_000, 14_000), (25_000, 27_000), (40_000, 42_500)];
        let g = compute_offsets(&old, &new, &lines, &ResyncOptions::default());
        assert_eq!(g.len(), 4, "{g:?}"); // 5-9 sn tek grup
        for x in &g {
            let want = if x.start_ms < 20_000 { 1000 } else { 4000 };
            assert!((x.offset_ms - want).abs() <= 2, "{x:?}");
            assert!(!x.fallback && x.score > 0.9, "{x:?}");
        }
        assert_eq!(offset_for(&g, 26_000), g[2].offset_ms);
    }

    #[test]
    fn group_spanning_a_cut_is_split() {
        let src = speechy(40, 0xabcdef);
        // 10.5 sn'de 2 sn ek sahne: 9-10 ve 11-12 sn satırları aynı grupta ama farklı kayma
        let mut dst = src[..10_500 * 8].to_vec();
        dst.extend(speechy(2, 0x55));
        dst.extend_from_slice(&src[10_500 * 8..]);
        let (old, new) = (Waveform { sample_rate: 8000, samples: src }, Waveform { sample_rate: 8000, samples: dst });
        let g = compute_offsets(&old, &new, &[(9000, 10_000), (11_000, 12_000)], &ResyncOptions::default());
        assert_eq!(g.len(), 2, "{g:?}");
        assert!(g[0].offset_ms.abs() <= 2 && (g[1].offset_ms - 2000).abs() <= 2, "{g:?}");
    }

    #[test]
    fn silent_template_falls_back() {
        let old = Waveform { sample_rate: 8000, samples: vec![0; 8000 * 10] };
        let new = Waveform { sample_rate: 8000, samples: speechy(10, 7) };
        let g = compute_offsets(&old, &new, &[(1000, 2000)], &ResyncOptions::default());
        assert!(g[0].fallback && g[0].offset_ms == 0);
    }
}
