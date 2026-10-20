//! Speech detection without a model, on the timing view's PCM (OsiSub's VAD, as in bsync).
//!
//! 10 ms hop, 512-sample windows: log energy (dB) and the share of energy in 250–3400 Hz.
//! Noise floor = 10th percentile of the energy; speech = floor + `on_db` and band share above
//! `min_ratio`; it ends below floor + `off_db` (hysteresis). A 5-frame majority filter, islands
//! shorter than `min_speech_ms` dropped, gaps shorter than `merge_gap_ms` closed.
//!
//! Limits: speech under music, shouting/whisper mixes — thresholds alone are not enough; the
//! result is for proposals (sync, snapping), never applied blindly.

use crate::audio::fft;

pub struct Features {
    pub rate: u32,
    pub hop: usize,
    pub energy: Vec<f32>,
    pub ratio: Vec<f32>,
}

const WIN: usize = 512;

/// Per-frame energy and speech-band share.
pub fn features(pcm: &[i16], rate: u32) -> Features {
    let hop = (rate as usize / 100).max(1);
    let n = if pcm.len() >= WIN { (pcm.len() - WIN) / hop + 1 } else { 0 };
    let window: Vec<f32> = (0..WIN).map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (WIN - 1) as f64).cos()) as f32).collect();
    let bin_hz = rate as f32 / WIN as f32;
    let (b0, b1) = ((250.0 / bin_hz).ceil() as usize, ((3400.0 / bin_hz).floor() as usize).min(WIN / 2));
    let (mut energy, mut ratio) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut re, mut im) = (vec![0f32; WIN], vec![0f32; WIN]);
    for f in 0..n {
        let base = f * hop;
        for i in 0..WIN { re[i] = pcm[base + i] as f32 / 32768.0 * window[i]; im[i] = 0.0; }
        fft(&mut re, &mut im);
        let mut tot = 1e-12f32;
        let mut band = 0f32;
        for k in 0..=WIN / 2 {
            let p = re[k] * re[k] + im[k] * im[k];
            tot += p;
            if k >= b0 && k <= b1 { band += p; }
        }
        energy.push(10.0 * (tot / WIN as f32).log10());
        ratio.push(band / tot);
    }
    Features { rate, hop, energy, ratio }
}

fn percentile(v: &[f32], q: f32) -> f32 {
    if v.is_empty() { return 0.0; }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = q / 100.0 * (s.len() - 1) as f32;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    s[lo] + (s[hi] - s[lo]) * (pos - lo as f32)
}

/// Speech segments (ms) for one threshold set.
pub fn segments(f: &Features, on_db: f32, off_db: f32, min_ratio: f32, min_speech_ms: i64, merge_gap_ms: i64) -> Vec<(i64, i64)> {
    let n = f.energy.len();
    if n == 0 { return vec![]; }
    let floor = percentile(&f.energy, 10.0);
    let mut active = vec![false; n];
    let mut state = false;
    for i in 0..n {
        let on = f.energy[i] > floor + on_db && f.ratio[i] > min_ratio;
        let off = f.energy[i] > floor + off_db;
        state = on || (state && off);
        active[i] = state;
    }
    if n >= 5 {
        let at = |i: i64| active[i.clamp(0, n as i64 - 1) as usize] as u8;
        active = (0..n as i64).map(|i| (-2..=2).map(|d| at(i + d)).sum::<u8>() >= 3).collect();
    }
    let hop_ms = f.hop as f64 * 1000.0 / f.rate as f64;
    let centre = WIN as f64 / 2.0 * 1000.0 / f.rate as f64 - hop_ms / 2.0;
    let mut segs: Vec<(i64, i64)> = Vec::new();
    let mut i = 0;
    while i < n {
        if active[i] {
            let mut j = i;
            while j < n && active[j] { j += 1; }
            segs.push(((i as f64 * hop_ms + centre) as i64, (j as f64 * hop_ms + centre) as i64));
            i = j;
        } else { i += 1; }
    }
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (a, b) in segs {
        match merged.last_mut() {
            Some(m) if a - m.1 < merge_gap_ms => m.1 = b,
            _ => merged.push((a, b)),
        }
    }
    merged.into_iter().filter(|(a, b)| b - a >= min_speech_ms).map(|(a, b)| (a.max(0), b)).collect()
}

/// Default single-threshold speech segments.
pub fn speech_segments(pcm: &[i16], rate: u32) -> Vec<(i64, i64)> {
    segments(&features(pcm, rate), 12.0, 7.0, 0.35, 120, 200)
}

/// bsync's ensemble: several onset thresholds, majority vote per 10 ms. 0/1 per 10 ms slot.
pub fn speech_activity(pcm: &[i16], rate: u32) -> Vec<f32> {
    let f = features(pcm, rate);
    let total = (pcm.len() as i64 * 1000) / rate.max(1) as i64;
    let n = (total / 10 + 1) as usize;
    let thresholds = [8.0f32, 10.0, 12.0, 14.0, 16.0];
    let mut votes = vec![0u8; n];
    for th in thresholds {
        for (a, b) in segments(&f, th, (th - 5.0).max(3.0), 0.35, 120, 200) {
            let (a, b) = ((a / 10).max(0) as usize, ((b / 10) as usize).min(n));
            for v in votes.iter_mut().take(b).skip(a) { *v += 1; }
        }
    }
    votes.into_iter().map(|v| if v as usize * 2 > thresholds.len() { 1.0 } else { 0.0 }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speech-band noise bursts (with a 4 Hz syllable envelope) on a quiet noise floor.
    fn synth(bursts: &[(i64, i64)], total_ms: i64, rate: u32) -> Vec<i16> {
        let n = (total_ms * rate as i64 / 1000) as usize;
        let mut seed = 12345u64;
        let mut noise = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; (seed % 20001) as f64 / 10000.0 - 1.0 };
        let mut x: Vec<f64> = (0..n).map(|_| noise() * 0.002).collect();
        for &(a, b) in bursts {
            let (i, j) = ((a * rate as i64 / 1000) as usize, (b * rate as i64 / 1000) as usize);
            // band-limit by mixing a few harmonics in the speech band plus filtered noise
            for k in i..j.min(n) {
                let t = k as f64 / rate as f64;
                let env = 0.6 + 0.4 * (2.0 * std::f64::consts::PI * 4.0 * t).sin().abs();
                let v = (2.0 * std::f64::consts::PI * 300.0 * t).sin() * 0.4 + (2.0 * std::f64::consts::PI * 900.0 * t).sin() * 0.3 + (2.0 * std::f64::consts::PI * 2100.0 * t).sin() * 0.2 + noise() * 0.05;
                x[k] += 0.25 * env * v;
            }
        }
        x.into_iter().map(|v| (v * 32767.0).clamp(-32768.0, 32767.0) as i16).collect()
    }

    #[test]
    fn finds_bursts_within_tolerance() {
        let bursts = [(1000, 2500), (4000, 4600), (7000, 9800)];
        let pcm = synth(&bursts, 12_000, 16_000);
        let segs = speech_segments(&pcm, 16_000);
        assert_eq!(segs.len(), 3, "{:?}", segs);
        for ((a, b), (x, y)) in segs.iter().zip(bursts) {
            assert!((a - x).abs() <= 60 && (b - y).abs() <= 60, "{:?} vs {:?}", (a, b), (x, y));
        }
        let act = speech_activity(&pcm, 16_000);
        assert_eq!(act.len(), 1201);
        assert_eq!(act[150], 1.0);
        assert_eq!(act[300], 0.0);
        assert_eq!(act[800], 1.0);
    }

    #[test]
    fn silence_has_no_speech() {
        let pcm = synth(&[], 5000, 16_000);
        assert!(speech_segments(&pcm, 16_000).is_empty());
        assert!(speech_segments(&[], 16_000).is_empty());
    }
}
