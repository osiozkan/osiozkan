//! osisub-media: ffmpeg/ffprobe üzerinden ortam bilgisi, ses dalga formu ve libass ile işlenmiş video kareleri.
//!
//! Harici `ffmpeg`/`ffprobe` gerekir (PATH'te ya da `OSISUB_FFMPEG`/`OSISUB_FFPROBE`). Bu sayede her
//! codec/kapsayıcı (MKV, MP4, ...) desteklenir ve altyazı önizlemesi gerçek libass çıktısıdır.
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
        std::fs::remove_dir_all(dir).unwrap();
    }
}
