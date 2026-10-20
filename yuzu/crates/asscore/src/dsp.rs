//! Small signal helpers for timing analysis: FFT, cross-correlation, Gaussian smoothing,
//! median. Time series here are 10 ms "activity" arrays (1 = a line / speech covers the slot).

/// In-place iterative radix-2 complex FFT; `re.len()` must be a power of two.
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j |= bit;
        if i < j { re.swap(i, j); im.swap(i, j); }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - xr; im[b] = im[a] - xi;
                re[a] += xr; im[a] += xi;
                let t = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = t;
            }
        }
        len <<= 1;
    }
}

/// Cross-correlation `c[lag] = Σ a[t]·b[t+lag]` for lag in `-max_lag..=max_lag`
/// (index `lag + max_lag`). O(n log n) via FFT.
pub fn xcorr(a: &[f64], b: &[f64], max_lag: usize) -> Vec<f64> {
    let n = a.len().max(b.len());
    if n == 0 { return vec![0.0; 2 * max_lag + 1]; }
    let m = (2 * n + 1).next_power_of_two();
    let (mut ar, mut ai) = (vec![0.0; m], vec![0.0; m]);
    let (mut br, mut bi) = (vec![0.0; m], vec![0.0; m]);
    ar[..a.len()].copy_from_slice(a);
    br[..b.len()].copy_from_slice(b);
    fft(&mut ar, &mut ai);
    fft(&mut br, &mut bi);
    // conj(A)·B, then inverse FFT via conj(fft(conj(x)))/m
    let (mut cr, mut ci) = (vec![0.0; m], vec![0.0; m]);
    for k in 0..m {
        let (xr, xi) = (ar[k], -ai[k]);
        cr[k] = xr * br[k] - xi * bi[k];
        ci[k] = -(xr * bi[k] + xi * br[k]);
    }
    fft(&mut cr, &mut ci);
    (0..=2 * max_lag).map(|i| {
        let lag = i as i64 - max_lag as i64;
        cr[lag.rem_euclid(m as i64) as usize] / m as f64
    }).collect()
}

/// Gaussian kernel (σ in samples), truncated at 3σ, normalised to sum 1 when `normalise`.
pub fn gaussian(sigma: f64, normalise: bool) -> Vec<f64> {
    let r = (3.0 * sigma) as i64;
    let k: Vec<f64> = (-r..=r).map(|x| (-0.5 * (x as f64 / sigma).powi(2)).exp()).collect();
    if normalise { let s: f64 = k.iter().sum(); k.iter().map(|v| v / s).collect() } else { k }
}

/// `numpy.convolve(x, k, mode="same")` for an odd-length kernel.
pub fn convolve_same(x: &[f64], k: &[f64]) -> Vec<f64> {
    let n = x.len();
    let r = k.len() / 2;
    let mut out = vec![0.0; n];
    for (i, &v) in x.iter().enumerate() {
        if v == 0.0 { continue; }
        for (j, &w) in k.iter().enumerate() {
            let t = i as i64 + j as i64 - r as i64;
            if t >= 0 && (t as usize) < n { out[t as usize] += v * w; }
        }
    }
    out
}

/// Median (average of the two middle values for even lengths, as numpy).
pub fn median(v: &[f64]) -> f64 {
    if v.is_empty() { return 0.0; }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xcorr_matches_direct() {
        let a: Vec<f64> = (0..300).map(|i| ((i * 7 % 13) as f64 - 6.0) / 6.0).collect();
        let b: Vec<f64> = (0..300).map(|i| ((i * 5 % 11) as f64 - 5.0) / 5.0).collect();
        let c = xcorr(&a, &b, 40);
        for lag in -40i64..=40 {
            let direct: f64 = (0..300i64).filter(|t| t + lag >= 0 && t + lag < 300).map(|t| a[t as usize] * b[(t + lag) as usize]).sum();
            assert!((c[(lag + 40) as usize] - direct).abs() < 1e-6, "lag {}: {} vs {}", lag, c[(lag + 40) as usize], direct);
        }
    }

    #[test]
    fn shift_is_found() {
        let mut a = vec![0.0; 1000];
        let mut b = vec![0.0; 1000];
        for i in 100..180 { a[i] = 1.0; b[i + 37] = 1.0; }
        for i in 400..470 { a[i] = 1.0; b[i + 37] = 1.0; }
        let c = xcorr(&a, &b, 100);
        let best = c.iter().enumerate().max_by(|x, y| x.1.partial_cmp(y.1).unwrap()).unwrap().0 as i64 - 100;
        assert_eq!(best, 37);
    }

    #[test]
    fn helpers() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        let k = gaussian(2.0, true);
        assert_eq!(k.len(), 13);
        assert!((k.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let y = convolve_same(&[0.0, 0.0, 1.0, 0.0, 0.0], &[0.25, 0.5, 0.25]);
        assert_eq!(y, vec![0.0, 0.25, 0.5, 0.25, 0.0]);
    }
}
