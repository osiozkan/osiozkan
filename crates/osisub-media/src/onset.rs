//! Hece başlangıcı (onset) zarfı: karaoke hecelerini sese oturtmak için (`core::karaoke::snap_to_onsets`).
//! 8 kHz dalga verisinde 10 ms adımlı, 32 ms Hann pencereli spektral akı (log genlikteki artışların
//! toplamı), yerel ortalaması çıkarılmış ve sıfırın altı kırpılmış.
use crate::Waveform;
use rustfft::{num_complex::Complex, FftPlanner};

/// `[start_ms, end_ms)` aralığı için santisaniye başına onset gücü (eleman 0 = start_ms).
pub fn onset_envelope(w: &Waveform, start_ms: i64, end_ms: i64) -> Vec<f32> {
    let sr = w.sample_rate.max(1) as i64;
    let hop = (sr / 100).max(1) as usize; // 10 ms
    let n = ((sr * 32 / 1000) as usize).next_power_of_two().max(64);
    let frames = ((end_ms - start_ms).max(0) / 10) as usize;
    if frames == 0 {
        return vec![];
    }
    let fft = FftPlanner::<f32>::new().plan_fft_forward(n);
    let hann: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos()).collect();
    let s0 = start_ms * sr / 1000;
    let mut prev: Vec<f32> = vec![0.0; n / 2];
    let mut flux = Vec::with_capacity(frames + 1);
    let mut buf = vec![Complex::new(0.0f32, 0.0); n];
    // bir önceki kareden başla ki ilk karenin akısı anlamlı olsun
    for f in 0..=frames {
        // pencere merkezi = kare zamanı
        let c = s0 + (f as i64 - 1) * hop as i64;
        for (i, b) in buf.iter_mut().enumerate() {
            let k = c - n as i64 / 2 + i as i64;
            let v = if k >= 0 && (k as usize) < w.samples.len() { w.samples[k as usize] as f32 / 32768.0 } else { 0.0 };
            *b = Complex::new(v * hann[i], 0.0);
        }
        fft.process(&mut buf);
        let mut sum = 0.0;
        for (i, p) in prev.iter_mut().enumerate() {
            let m = (1.0 + 100.0 * buf[i].norm()).ln();
            sum += (m - *p).max(0.0);
            *p = m;
        }
        if f > 0 {
            flux.push(sum);
        }
    }
    // yerel ortalama (±8 kare) çıkar
    let mut out = vec![0.0f32; flux.len()];
    for i in 0..flux.len() {
        let (a, b) = (i.saturating_sub(8), (i + 9).min(flux.len()));
        let mean = flux[a..b].iter().sum::<f32>() / (b - a) as f32;
        out[i] = (flux[i] - mean).max(0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legato şarkı: her hecede perde değişir ve kısa bir vurgu (genlik artışı) vardır.
    fn song(onsets_ms: &[i64], end_ms: i64) -> Waveform {
        let sr = 8000;
        let freqs = [262.0f32, 330.0, 392.0, 294.0, 440.0, 349.0];
        let mut s = vec![0i16; (end_ms * sr / 1000) as usize];
        for (k, &o) in onsets_ms.iter().enumerate() {
            let e = onsets_ms.get(k + 1).copied().unwrap_or(end_ms);
            for i in (o * sr / 1000) as usize..(e * sr / 1000) as usize {
                let t = (i as i64 - o * sr / 1000) as f32 / sr as f32;
                let env = 0.35 + 0.65 * (-t * 12.0).exp();
                let f = freqs[k % freqs.len()];
                let v = env * ((2.0 * std::f32::consts::PI * f * t).sin() + 0.4 * (4.0 * std::f32::consts::PI * f * t).sin());
                s[i] = (v * 9000.0) as i16;
            }
        }
        Waveform { sample_rate: sr as u32, samples: s }
    }

    #[test]
    fn peaks_at_syllable_onsets_and_snaps_karaoke() {
        // satır 2000 ms'de başlar; heceler 0, 310, 720, 1050, 1400 ms'de (satıra göre)
        let line = 2000;
        let rel = [0i64, 310, 720, 1050, 1400];
        let abs: Vec<i64> = rel.iter().map(|r| line + r).collect();
        let w = song(&abs, line + 1800);
        let env = onset_envelope(&w, line, line + 1800);
        assert_eq!(env.len(), 180);
        for &r in &rel[1..] {
            let i = (r / 10) as usize;
            let local = env[i.saturating_sub(2)..=(i + 2)].iter().cloned().fold(0.0, f32::max);
            assert!(local > 0.0, "onset {r}");
        }
        use osisub_core::karaoke::{snap_to_onsets, Syl};
        // kötü ilk tahmin: sınırlar 40, 70, 110, 135 (gerçek 31, 72, 105, 140)
        let syl = |d: i64, t: &str| Syl { dur: d, kind: "kf".into(), text: t.into() };
        let s = vec![syl(40, "ki"), syl(30, "mi "), syl(40, "no "), syl(25, "ko"), syl(45, "e")];
        let r = snap_to_onsets(&s, &env, 15);
        let mut acc = 0;
        let mut got = vec![];
        for x in &r {
            got.push(acc);
            acc += x.dur;
        }
        assert_eq!(acc, 180);
        for (g, e) in got.iter().zip(rel.iter()) {
            assert!((g * 10 - e).abs() <= 20, "sınırlar {got:?} beklenen {rel:?}");
        }
    }
}
