//! osisub-media: ffmpeg/ffprobe üzerinden ortam bilgisi, ses dalga formu ve libass ile işlenmiş video kareleri.
//!
//! Harici `ffmpeg`/`ffprobe` gerekir (PATH'te ya da `OSISUB_FFMPEG`/`OSISUB_FFPROBE`). Bu sayede her
//! codec/kapsayıcı (MKV, MP4, ...) desteklenir ve altyazı önizlemesi gerçek libass çıktısıdır.
pub mod audio;

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn ffmpeg_bin() -> String {
    std::env::var("OSISUB_FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
}

pub fn ffprobe_bin() -> String {
    std::env::var("OSISUB_FFPROBE").unwrap_or_else(|_| "ffprobe".into())
}

pub fn ffmpeg_available() -> bool {
    Command::new(ffmpeg_bin()).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_video: bool,
    pub has_audio: bool,
}

fn parse_rate(s: &str) -> f64 {
    match s.split_once('/') {
        Some((a, b)) => {
            let (a, b): (f64, f64) = (a.parse().unwrap_or(0.0), b.parse().unwrap_or(1.0));
            if b == 0.0 { 0.0 } else { a / b }
        }
        None => s.parse().unwrap_or(0.0),
    }
}

pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    let out = Command::new(ffprobe_bin())
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe çalıştırılamadı: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut info = MediaInfo::default();
    if let Some(d) = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()) {
        info.duration_ms = (d * 1000.0) as i64;
    }
    for s in v["streams"].as_array().cloned().unwrap_or_default() {
        match s["codec_type"].as_str() {
            Some("video") if !info.has_video && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                info.has_video = true;
                info.width = s["width"].as_u64().unwrap_or(0) as u32;
                info.height = s["height"].as_u64().unwrap_or(0) as u32;
                info.fps = parse_rate(s["avg_frame_rate"].as_str().unwrap_or("0"));
                if info.fps <= 0.0 {
                    info.fps = parse_rate(s["r_frame_rate"].as_str().unwrap_or("0"));
                }
            }
            Some("audio") => info.has_audio = true,
            _ => {}
        }
    }
    Ok(info)
}

/// Mono 16-bit PCM dalga biçimi.
#[derive(Clone, Debug, Default)]
pub struct Waveform {
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

pub const WAVE_RATE: u32 = 8000;

pub fn load_waveform(path: &Path) -> Result<Waveform, String> {
    let mut child = Command::new(ffmpeg_bin())
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &WAVE_RATE.to_string(), "-f", "s16le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg çalıştırılamadı: {}", e))?;
    let mut bytes = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let _ = child.wait();
    let samples = bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    Ok(Waveform { sample_rate: WAVE_RATE, samples })
}

impl Waveform {
    pub fn duration_ms(&self) -> i64 {
        self.samples.len() as i64 * 1000 / self.sample_rate.max(1) as i64
    }

    /// `[start_ms, start_ms + px * ms_per_px)` aralığı için piksel başına (min, max).
    pub fn peaks(&self, start_ms: f64, ms_per_px: f64, px: usize) -> Vec<(i16, i16)> {
        let sr = self.sample_rate as f64 / 1000.0;
        (0..px)
            .map(|i| {
                let a = ((start_ms + i as f64 * ms_per_px) * sr).max(0.0) as usize;
                let b = (((start_ms + (i + 1) as f64 * ms_per_px) * sr) as usize).max(a + 1).min(self.samples.len());
                if a >= self.samples.len() || a >= b {
                    return (0, 0);
                }
                let s = &self.samples[a..b];
                // Çok uzun kovalar için seyrek örnekle.
                let step = (s.len() / 256).max(1);
                let (mut lo, mut hi) = (i16::MAX, i16::MIN);
                for v in s.iter().step_by(step) {
                    lo = lo.min(*v);
                    hi = hi.max(*v);
                }
                (lo, hi)
            })
            .collect()
    }
}

/// Spektrogram: `px` sütun × `bins` satır (satır 0 = en yüksek frekans), değerler 0..=255 (dB eşlemesi).
/// Pencere 512 örnek (Hann). `bins` ≤ 256 (0 .. örnekleme_hızı/2).
pub fn spectrogram(w: &Waveform, start_ms: f64, ms_per_px: f64, px: usize, bins: usize) -> Vec<u8> {
    use rustfft::{num_complex::Complex, FftPlanner};
    const N: usize = 512;
    let bins = bins.clamp(1, N / 2);
    let fft = FftPlanner::<f32>::new().plan_fft_forward(N);
    let hann: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos()).collect();
    let sr = w.sample_rate as f64 / 1000.0;
    let mut img = vec![0u8; px * bins];
    let mut buf = vec![Complex { re: 0.0f32, im: 0.0 }; N];
    for x in 0..px {
        let center = ((start_ms + (x as f64 + 0.5) * ms_per_px) * sr) as i64;
        let a = center - (N as i64) / 2;
        for i in 0..N {
            let idx = a + i as i64;
            let v = if idx >= 0 && (idx as usize) < w.samples.len() { w.samples[idx as usize] as f32 / 32768.0 } else { 0.0 };
            buf[i] = Complex { re: v * hann[i], im: 0.0 };
        }
        if center < 0 || center as usize >= w.samples.len() {
            continue;
        }
        fft.process(&mut buf);
        for b in 0..bins {
            // bins'e eşit aralıklı bölüştür: her görüntü satırı birkaç FFT kutusunun en büyüğü
            let (k0, k1) = (b * (N / 2) / bins, ((b + 1) * (N / 2) / bins).max(b * (N / 2) / bins + 1));
            let mag = (k0..k1).map(|k| buf[k].norm()).fold(0.0f32, f32::max) / (N as f32 / 4.0);
            let db = 20.0 * (mag + 1e-9).log10();
            let v = ((db + 90.0) / 70.0).clamp(0.0, 1.0);
            img[(bins - 1 - b) * px + x] = (v * 255.0) as u8;
        }
    }
    img
}

#[derive(Clone, Debug)]
pub struct VadOptions {
    /// Gürültü tabanının üstünde konuşma sayılacak eşik (dB).
    pub margin_db: f64,
    /// Mutlak alt sınır (dBFS); bunun altı hep sessizlik.
    pub floor_db: f64,
    pub min_speech_ms: i64,
    /// Bundan kısa sessizlikler konuşmayı bölmez.
    pub min_silence_ms: i64,
    /// Bulunan aralıkların her iki yanına eklenen pay.
    pub pad_ms: i64,
}

impl Default for VadOptions {
    fn default() -> Self {
        VadOptions { margin_db: 12.0, floor_db: -50.0, min_speech_ms: 300, min_silence_ms: 350, pad_ms: 80 }
    }
}

/// Basit enerji tabanlı konuşma etkinliği tespiti. (başlangıç_ms, bitiş_ms) aralıkları döndürür.
pub fn detect_speech(w: &Waveform, o: &VadOptions) -> Vec<(i64, i64)> {
    let frame = (w.sample_rate as usize / 50).max(1); // 20 ms
    let db: Vec<f64> = w
        .samples
        .chunks(frame)
        .map(|c| {
            let e: f64 = c.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum::<f64>() / c.len() as f64;
            10.0 * (e + 1e-12).log10()
        })
        .collect();
    if db.is_empty() {
        return vec![];
    }
    let mut sorted = db.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let noise = sorted[sorted.len() / 5];
    let thr = (noise + o.margin_db).max(o.floor_db);
    let ms = |i: usize| (i as i64) * 20;
    let mut segs: Vec<(i64, i64)> = vec![];
    let mut start: Option<usize> = None;
    for (i, &v) in db.iter().enumerate() {
        match (v > thr, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                segs.push((ms(s), ms(i)));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        segs.push((ms(s), ms(db.len())));
    }
    // kısa sessizlikleri birleştir, kısa konuşmaları at
    let mut merged: Vec<(i64, i64)> = vec![];
    for (a, b) in segs {
        match merged.last_mut() {
            Some(l) if a - l.1 < o.min_silence_ms => l.1 = b,
            _ => merged.push((a, b)),
        }
    }
    let total = w.duration_ms();
    merged.into_iter().filter(|(a, b)| b - a >= o.min_speech_ms).map(|(a, b)| ((a - o.pad_ms).max(0), (b + o.pad_ms).min(total))).collect()
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// `time_ms` anındaki kareyi (varsa `ass` altyazısı libass ile işlenmiş olarak) `width` genişliğinde RGBA verir.
pub fn render_frame(video: &Path, ass: Option<&Path>, time_ms: i64, width: u32, info: &MediaInfo) -> Result<Frame, String> {
    if !info.has_video || info.width == 0 {
        return Err("video yok".into());
    }
    let width = (width.max(16) / 2) * 2;
    let height = ((width as f64 * info.height as f64 / info.width as f64 / 2.0).round() as u32).max(1) * 2;
    let mut vf = String::new();
    if let Some(a) = ass {
        let p = a.to_string_lossy().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'");
        vf.push_str(&format!("ass='{}',", p));
    }
    vf.push_str(&format!("scale={}:{}", width, height));
    // Hızlı arama için önce kaba -ss (girdiden önce), sonra hassas -ss (girdiden sonra).
    // `-copyts` zaman damgalarını korur; yoksa ass filtresi kaba aramanın ofsetini görür ve yanlış satırı çizer.
    let coarse = (time_ms - 2000).max(0) as f64 / 1000.0;
    let fine = time_ms.max(0) as f64 / 1000.0 - coarse;
    let out = Command::new(ffmpeg_bin())
        .args(["-v", "error", "-ss", &format!("{:.3}", coarse), "-copyts", "-i"])
        .arg(video)
        .args(["-ss", &format!("{:.3}", coarse + fine), "-an", "-sn", "-vf", &vf, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("ffmpeg çalıştırılamadı: {}", e))?;
    let need = (width * height * 4) as usize;
    if out.stdout.len() < need {
        return Err(format!("kare okunamadı: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(Frame { width, height, rgba: out.stdout[..need].to_vec() })
}

/// Bir zaman aralığı için kare akışı: ffmpeg çocuk süreci + kare baytlarını yollayan kanal.
pub struct PlayStream {
    pub child: std::process::Child,
    pub frames: std::sync::mpsc::Receiver<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

impl Drop for PlayStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `[start_ms, start_ms+dur_ms)` aralığını `width` genişliğinde RGBA kare akışı olarak başlatır
/// (altyazı libass ile işlenmiş). Küçültme önce yapılır, ass `original_size` ile doğru ölçeklenir (hız için).
/// Not: `-copyts` ile zaman damgaları mutlak kaldığından süre `-to` (mutlak bitiş) ile sınırlanır.
pub fn spawn_play(video: &Path, ass: Option<&Path>, start_ms: i64, dur_ms: i64, width: u32, info: &MediaInfo) -> Result<PlayStream, String> {
    if !info.has_video || info.width == 0 {
        return Err("video yok".into());
    }
    let width = (width.max(16) / 2) * 2;
    let height = ((width as f64 * info.height as f64 / info.width as f64 / 2.0).round() as u32).max(1) * 2;
    let fps = if info.fps > 0.0 { info.fps } else { 24.0 };
    let mut vf = format!("scale={}:{},", width, height);
    if let Some(a) = ass {
        let p = a.to_string_lossy().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'");
        vf.push_str(&format!("ass='{}':original_size={}x{},", p, width, height));
    }
    vf.push_str(&format!("fps={}", fps));
    let mut child = Command::new(ffmpeg_bin())
        .args(["-v", "error", "-ss", &format!("{:.3}", start_ms.max(0) as f64 / 1000.0), "-copyts", "-i"])
        .arg(video)
        .args(["-to", &format!("{:.3}", (start_ms.max(0) + dur_ms.max(1)) as f64 / 1000.0), "-an", "-sn", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg çalıştırılamadı: {}", e))?;
    let mut out = child.stdout.take().ok_or("stdout yok")?;
    let (tx, rx) = std::sync::mpsc::channel();
    let size = (width * height * 4) as usize;
    std::thread::spawn(move || loop {
        let mut buf = vec![0u8; size];
        if out.read_exact(&mut buf).is_err() || tx.send(buf).is_err() {
            break;
        }
    });
    Ok(PlayStream { child, frames: rx, width, height, fps })
}

/// Aegisub anahtar kare dosyasını (`# keyframe format v1`, `fps X`, satır başına kare numarası) ya da
/// yalnızca kare numaraları içeren metni ms'ye çevirir. `fallback_fps` dosyada fps yoksa kullanılır.
pub fn parse_keyframe_file(text: &str, fallback_fps: f64) -> Vec<i64> {
    let mut fps = fallback_fps;
    let mut frames: Vec<i64> = vec![];
    for l in text.lines() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(v) = l.strip_prefix("fps") {
            if let Ok(f) = v.trim().parse::<f64>() {
                if f > 0.0 {
                    fps = f;
                }
            }
            continue;
        }
        // x264/xvid istatistik biçimi: "in:0 out:0 type:I ..." → I/i türündekiler anahtar kare
        if l.starts_with("in:") {
            if l.contains("type:I") || l.contains("type:i") {
                if let Some(n) = l.split_whitespace().next().and_then(|t| t.strip_prefix("in:")).and_then(|t| t.parse().ok()) {
                    frames.push(n);
                }
            }
            continue;
        }
        if let Ok(n) = l.split_whitespace().next().unwrap_or("").parse::<i64>() {
            frames.push(n);
        }
    }
    frames.sort_unstable();
    frames.dedup();
    frames.into_iter().map(|f| (f as f64 * 1000.0 / fps).round() as i64).collect()
}

/// Anahtar kareleri Aegisub biçiminde yazar.
pub fn write_keyframe_file(keyframes_ms: &[i64], fps: f64) -> String {
    let mut out = format!("# keyframe format v1\nfps {}\n", fps);
    for &ms in keyframes_ms {
        out.push_str(&format!("{}\n", (ms as f64 * fps / 1000.0).round() as i64));
    }
    out
}

/// Anahtar kare zamanları (ms). Büyük dosyalarda birkaç saniye sürebilir.
pub fn keyframes(path: &Path) -> Result<Vec<i64>, String> {
    let out = Command::new(ffprobe_bin())
        .args(["-v", "error", "-select_streams", "v:0", "-skip_frame", "nokey", "-show_entries", "frame=pts_time", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let mut v: Vec<i64> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().trim_end_matches(',').parse::<f64>().ok()).map(|s| (s * 1000.0).round() as i64).collect();
    v.sort_unstable();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrogram_peaks_at_tone() {
        // 8 kHz'de 1 kHz sinüs → 256 kutudan (0..4 kHz) 64. kutuda enerji
        let sr = 8000usize;
        let samples: Vec<i16> = (0..sr).map(|i| ((2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr as f32).sin() * 16000.0) as i16).collect();
        let w = Waveform { sample_rate: sr as u32, samples };
        let img = spectrogram(&w, 100.0, 10.0, 20, 256);
        let col = 10;
        let (mut best, mut bv) = (0, 0u8);
        for row in 0..256 {
            let v = img[row * 20 + col];
            if v > bv { bv = v; best = row; }
        }
        let bin = 255 - best;
        assert!((62..=66).contains(&bin), "bin {}", bin);
        assert!(bv > 200);
    }

    #[test]
    fn vad_finds_speech() {
        // 1 sn sessiz, 1 sn "ses", 1 sn sessiz, 0.5 sn kısa tık (atılmalı), 1 sn "ses"
        let sr = 8000usize;
        let mut s: Vec<i16> = vec![0; sr];
        s.extend((0..sr).map(|i| if i % 2 == 0 { 8000 } else { -8000 }));
        s.extend(vec![0; sr]);
        s.extend((0..sr / 10).map(|i| if i % 2 == 0 { 8000 } else { -8000 }));
        s.extend(vec![0; sr]);
        s.extend((0..sr).map(|i| if i % 2 == 0 { 8000 } else { -8000 }));
        let w = Waveform { sample_rate: sr as u32, samples: s };
        let v = detect_speech(&w, &VadOptions::default());
        assert_eq!(v.len(), 2, "{:?}", v);
        assert!((v[0].0 - 920).abs() <= 40 && (v[0].1 - 2080).abs() <= 40, "{:?}", v);
        assert!(v[1].0 > 3500);
    }

    #[test]
    fn keyframe_files() {
        let t = "# keyframe format v1\nfps 25\n0\n50\n125\n";
        assert_eq!(parse_keyframe_file(t, 23.976), vec![0, 2000, 5000]);
        assert_eq!(parse_keyframe_file("0\n24\n", 24.0), vec![0, 1000]);
        assert_eq!(parse_keyframe_file("in:0 out:0 type:I\nin:1 out:1 type:P\nin:48 out:48 type:i\n", 24.0), vec![0, 2000]);
        let w = write_keyframe_file(&[0, 2000, 5000], 25.0);
        assert_eq!(parse_keyframe_file(&w, 1.0), vec![0, 2000, 5000]);
    }

    #[test]
    fn peaks_basic() {
        let w = Waveform { sample_rate: 1000, samples: (0..1000).map(|i| if i % 2 == 0 { 100 } else { -100 }).collect() };
        let p = w.peaks(0.0, 100.0, 10);
        assert!(p.iter().all(|&(lo, hi)| lo == -100 && hi == 100));
        assert_eq!(w.duration_ms(), 1000);
        // Aralık dışı
        assert_eq!(w.peaks(5000.0, 10.0, 2), vec![(0, 0), (0, 0)]);
    }

    #[test]
    fn ffmpeg_roundtrip_if_available() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("osisub_media_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mp4 = dir.join("t.mkv");
        let st = Command::new(ffmpeg_bin())
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=24:duration=4", "-f", "lavfi", "-i", "sine=frequency=440:duration=4", "-shortest"])
            .arg(&mp4)
            .status()
            .unwrap();
        assert!(st.success());
        let info = probe(&mp4).unwrap();
        assert!(info.has_video && info.has_audio);
        assert_eq!((info.width, info.height), (320, 180));
        assert!((info.fps - 24.0).abs() < 0.01);
        let w = load_waveform(&mp4).unwrap();
        assert!((w.duration_ms() - 4000).abs() < 100);
        assert!(w.samples.iter().any(|&s| s.abs() > 1000));
        let ass = dir.join("s.ass");
        std::fs::write(&ass, "[Script Info]\nPlayResX: 320\nPlayResY: 180\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,40,&H0000FFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,5,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,HELLO\n").unwrap();
        let plain = render_frame(&mp4, None, 1000, 320, &info).unwrap();
        let with = render_frame(&mp4, Some(&ass), 1000, 320, &info).unwrap();
        assert_eq!(plain.rgba.len(), with.rgba.len());
        assert_ne!(plain.rgba, with.rgba, "altyazı karede görünmeli");
        // Zaman doğruluğu: HELLO yalnızca 0–2 sn arası; 1 sn'de görünür, 2.5 sn'de görünmez (kaba arama ofseti olsa bile).
        let late_plain = render_frame(&mp4, None, 2_500, 320, &info).unwrap();
        let late_with = render_frame(&mp4, Some(&ass), 2_500, 320, &info).unwrap();
        assert_eq!(late_plain.rgba, late_with.rgba, "2.5 sn'de altyazı olmamalı");
        // Akış: 1 sn'lik aralık ≈ 24 kare
        let st = spawn_play(&mp4, Some(&ass), 500, 1000, 160, &info).unwrap();
        let mut n = 0;
        while let Ok(f) = st.frames.recv_timeout(std::time::Duration::from_secs(10)) {
            assert_eq!(f.len(), (st.width * st.height * 4) as usize);
            n += 1;
        }
        assert!((22..=26).contains(&n), "kare sayısı {}", n);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
