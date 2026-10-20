//! Decoded audio for the timing view: 16 kHz mono PCM kept in memory, waveform peaks and a
//! mel-scaled spectrogram (speech onsets and syllables are much easier to see than on a waveform).

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

/// Sample rate used for analysis (waveform, spectrum, speech detection).
pub const RATE: u32 = 16_000;

/// Stream the audio of `path` as mono `RATE` Hz s16 samples; `sink` returns false to stop.
pub fn decode(path: &Path, mut sink: impl FnMut(&[i16]) -> bool) -> Result<(), String> {
    let mut child = crate::command("ffmpeg").ok_or("ffmpeg not found")?
        .args(["-v", "error", "-i"]).arg(path)
        .args(["-vn", "-sn", "-dn", "-ac", "1", "-ar", &RATE.to_string(), "-f", "s16le", "-"])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut carry: Option<u8> = None;
    let mut samples: Vec<i16> = Vec::with_capacity(32 * 1024);
    let mut got_any = false;
    loop {
        let n = so.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        got_any = true;
        samples.clear();
        let mut bytes = &buf[..n];
        if let Some(lo) = carry.take() {
            samples.push(i16::from_le_bytes([lo, bytes[0]]));
            bytes = &bytes[1..];
        }
        let pairs = bytes.len() / 2;
        samples.extend((0..pairs).map(|i| i16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]])));
        if bytes.len() % 2 == 1 { carry = Some(bytes[bytes.len() - 1]); }
        if !sink(&samples) { let _ = child.kill(); let _ = child.wait(); return Ok(()); }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !got_any && !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

/// (min, max) per `bin_ms` milliseconds, scaled to i8.
pub fn peaks(samples: &[i16], rate: u32, bin_ms: i64) -> Vec<(i8, i8)> {
    let per = (rate as i64 * bin_ms / 1000).max(1) as usize;
    samples.chunks(per).map(|c| {
        let (mut mn, mut mx) = (i16::MAX, i16::MIN);
        for &v in c { mn = mn.min(v); mx = mx.max(v); }
        ((mn >> 8) as i8, (mx >> 8) as i8)
    }).collect()
}

/// In-place iterative radix-2 complex FFT (`re.len()` must be a power of two).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
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
        let (wr, wi) = (ang.cos() as f32, ang.sin() as f32);
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
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

/// Spectrogram: one column of `bands` bytes (0 = silence … 255 = loudest) every `hop_ms`.
/// Rows are mel-spaced between 60 Hz and Nyquist, row 0 = lowest frequency.
#[derive(Clone, Debug, Default)]
pub struct Spectrogram {
    pub hop_ms: i64,
    pub bands: usize,
    pub data: Vec<u8>,
}

impl Spectrogram {
    pub fn columns(&self) -> usize { if self.bands == 0 { 0 } else { self.data.len() / self.bands } }
    pub fn column(&self, i: usize) -> &[u8] { &self.data[i * self.bands..(i + 1) * self.bands] }
}

const WIN: usize = 512;

fn mel(f: f64) -> f64 { 2595.0 * (1.0 + f / 700.0).log10() }
fn mel_inv(m: f64) -> f64 { 700.0 * (10f64.powf(m / 2595.0) - 1.0) }

/// FFT bin ranges for each mel band.
fn band_bins(rate: u32, bands: usize) -> Vec<(usize, usize)> {
    let nyq = rate as f64 / 2.0;
    let (m0, m1) = (mel(60.0), mel(nyq));
    let bin_hz = rate as f64 / WIN as f64;
    (0..bands).map(|b| {
        let lo = mel_inv(m0 + (m1 - m0) * b as f64 / bands as f64);
        let hi = mel_inv(m0 + (m1 - m0) * (b + 1) as f64 / bands as f64);
        let a = (lo / bin_hz).floor() as usize;
        let z = ((hi / bin_hz).ceil() as usize).max(a + 1).min(WIN / 2);
        (a.min(WIN / 2 - 1), z)
    }).collect()
}

/// Compute a spectrogram (512-point Hann window, `hop_ms` hop). Levels are normalised to the
/// file: the loudest 0.5 % map to white, 70 dB below that to black.
pub fn spectrogram(samples: &[i16], rate: u32, hop_ms: i64, bands: usize) -> Spectrogram {
    let hop = (rate as i64 * hop_ms / 1000).max(1) as usize;
    let cols = samples.len().div_ceil(hop);
    let window: Vec<f32> = (0..WIN).map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / WIN as f64).cos()) as f32).collect();
    let bins = band_bins(rate, bands);
    let mut db = vec![0f32; cols * bands];
    let (mut re, mut im) = (vec![0f32; WIN], vec![0f32; WIN]);
    let mut power = vec![0f32; WIN / 2 + 1];
    for c in 0..cols {
        // window centred on the column's time
        let center = (c * hop) as isize;
        for i in 0..WIN {
            let k = center + i as isize - (WIN / 2) as isize;
            let v = if k >= 0 && (k as usize) < samples.len() { samples[k as usize] as f32 / 32768.0 } else { 0.0 };
            re[i] = v * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for (k, p) in power.iter_mut().enumerate() { *p = re[k] * re[k] + im[k] * im[k]; }
        for (b, &(a, z)) in bins.iter().enumerate() {
            let m = power[a..z].iter().copied().fold(0f32, f32::max);
            db[c * bands + b] = 10.0 * (m + 1e-12).log10();
        }
    }
    // normalise
    let mut sorted: Vec<f32> = db.iter().copied().step_by(7).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let top = sorted.get(sorted.len().saturating_sub(1).min(sorted.len() * 995 / 1000)).copied().unwrap_or(0.0);
    // a full-scale tone is about +42 dB here; never stretch near-silence up to white
    let top = top.max(-30.0);
    let floor = top - 70.0;
    let data = db.iter().map(|&v| (((v - floor) / 70.0).clamp(0.0, 1.0) * 255.0) as u8).collect();
    Spectrogram { hop_ms, bands, data }
}

/// Short-time energy (dB, one value per `hop_ms`) for speech detection and timing helpers.
pub fn energy_db(samples: &[i16], rate: u32, hop_ms: i64) -> Vec<f32> {
    let hop = (rate as i64 * hop_ms / 1000).max(1) as usize;
    samples.chunks(hop).map(|c| {
        let e: f64 = c.iter().map(|&v| (v as f64 / 32768.0).powi(2)).sum::<f64>() / c.len().max(1) as f64;
        (10.0 * (e + 1e-12).log10()) as f32
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, secs: f64) -> Vec<i16> {
        (0..(RATE as f64 * secs) as usize).map(|i| ((2.0 * std::f64::consts::PI * freq * i as f64 / RATE as f64).sin() * 12000.0) as i16).collect()
    }

    #[test]
    fn fft_finds_the_tone() {
        let n = 512;
        let (mut re, mut im): (Vec<f32>, Vec<f32>) = ((0..n).map(|i| (2.0 * std::f32::consts::PI * 32.0 * i as f32 / n as f32).cos()).collect(), vec![0.0; n]);
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..n / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        let peak = mags.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        assert_eq!(peak, 32);
        assert!((mags[32] - 256.0).abs() < 1.0, "{}", mags[32]);
    }

    #[test]
    fn spectrogram_rows_follow_frequency() {
        // 0.5 s of 300 Hz, then 0.5 s of 3 kHz: the bright row must move up
        let mut s = tone(300.0, 0.5);
        s.extend(tone(3000.0, 0.5));
        let sp = spectrogram(&s, RATE, 10, 64);
        assert_eq!(sp.columns(), 100);
        let brightest = |c: usize| sp.column(c).iter().enumerate().max_by_key(|x| *x.1).unwrap().0;
        let (lo, hi) = (brightest(20), brightest(80));
        assert!(hi > lo + 15, "low tone row {} high tone row {}", lo, hi);
        // silence is dark
        let sil = spectrogram(&vec![0i16; 8000], RATE, 10, 64);
        assert!(sil.data.iter().all(|&v| v < 10));
    }

    #[test]
    fn peaks_and_energy() {
        let s = tone(440.0, 1.0);
        let p = peaks(&s, RATE, 10);
        assert_eq!(p.len(), 100);
        assert!(p[50].1 > 40 && p[50].0 < -40);
        let e = energy_db(&s, RATE, 10);
        assert_eq!(e.len(), 100);
        assert!(e[10] > -20.0);
        assert!(energy_db(&vec![0i16; 1600], RATE, 10)[0] < -100.0);
    }
}
