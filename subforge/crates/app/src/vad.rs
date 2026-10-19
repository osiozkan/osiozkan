//! Simple energy-based speech segment finder on the waveform envelope.

#[derive(Debug, Clone, Copy)]
pub struct VadOpts {
    /// Speech threshold as a multiple of the estimated noise floor.
    pub threshold: f64,
    pub min_speech_ms: i64,
    pub min_silence_ms: i64,
    pub pad_ms: i64,
}

impl Default for VadOpts {
    fn default() -> Self {
        VadOpts { threshold: 3.0, min_speech_ms: 250, min_silence_ms: 350, pad_ms: 60 }
    }
}

/// `peaks`: (min, max) per `window_ms`. Returns (start_ms, end_ms) segments.
pub fn detect(peaks: &[(i16, i16)], window_ms: i64, o: &VadOpts) -> Vec<(i64, i64)> {
    if peaks.is_empty() {
        return vec![];
    }
    let amp: Vec<f64> = peaks.iter().map(|&(a, b)| (a as f64).abs().max((b as f64).abs())).collect();
    // smooth over ~50 ms
    let k = (50 / window_ms.max(1)).max(1) as usize;
    let sm: Vec<f64> = (0..amp.len()).map(|i| amp[i.saturating_sub(k / 2)..(i + k / 2 + 1).min(amp.len())].iter().sum::<f64>() / ((i + k / 2 + 1).min(amp.len()) - i.saturating_sub(k / 2)) as f64).collect();
    let mut sorted = sm.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let floor = sorted[sorted.len() / 5].max(30.0);
    let thr = floor * o.threshold;
    let mut segs: Vec<(i64, i64)> = vec![];
    let mut start: Option<usize> = None;
    for (i, &v) in sm.iter().enumerate() {
        match (v >= thr, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                segs.push((s as i64 * window_ms, i as i64 * window_ms));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        segs.push((s as i64 * window_ms, sm.len() as i64 * window_ms));
    }
    // merge short silences, drop short speech
    let mut merged: Vec<(i64, i64)> = vec![];
    for s in segs {
        match merged.last_mut() {
            Some(last) if s.0 - last.1 < o.min_silence_ms => last.1 = s.1,
            _ => merged.push(s),
        }
    }
    merged.into_iter().filter(|s| s.1 - s.0 >= o.min_speech_ms).map(|s| ((s.0 - o.pad_ms).max(0), s.1 + o.pad_ms)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_two_utterances() {
        let mut peaks = vec![(-20i16, 20i16); 600]; // 6 s of noise at 10 ms
        for p in peaks[100..250].iter_mut() {
            *p = (-8000, 8000);
        }
        for p in peaks[400..520].iter_mut() {
            *p = (-6000, 6000);
        }
        // a blip too short to count
        peaks[330] = (-9000, 9000);
        let s = detect(&peaks, 10, &VadOpts::default());
        assert_eq!(s.len(), 2, "{s:?}");
        assert!((s[0].0 - 940).abs() <= 100 && (s[0].1 - 2560).abs() <= 100);
        assert!((s[1].0 - 3940).abs() <= 100);
    }

    #[test]
    fn silence_gives_nothing() {
        assert!(detect(&vec![(-10, 10); 500], 10, &VadOpts::default()).is_empty());
    }
}
