//! Audio decoding to waveform peaks (pure Rust via symphonia; ffmpeg fallback when installed).
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Peak envelope with a fixed resolution of `PEAKS_PER_SEC` buckets per second.
#[derive(Clone, Debug, Default)]
pub struct AudioPeaks {
    pub sample_rate: u32,
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

pub const PEAKS_PER_SEC: usize = 200; // 5 ms buckets

impl AudioPeaks {
    pub fn duration_ms(&self) -> i64 { (self.min.len() * 1000 / PEAKS_PER_SEC) as i64 }
    /// (min,max) over [start_ms, end_ms)
    pub fn range(&self, start_ms: i64, end_ms: i64) -> (f32, f32) {
        let a = ((start_ms.max(0) as usize) * PEAKS_PER_SEC / 1000).min(self.min.len());
        let b = (((end_ms.max(0) as usize) * PEAKS_PER_SEC + 999) / 1000).clamp(a, self.min.len());
        let mut lo = 0f32; let mut hi = 0f32;
        for i in a..b { lo = lo.min(self.min[i]); hi = hi.max(self.max[i]); }
        (lo, hi)
    }
}

struct Acc { rate: u32, per_bucket: f64, pos: f64, cur_min: f32, cur_max: f32, out: AudioPeaks, have: bool }
impl Acc {
    fn new(rate: u32) -> Self { Acc { rate, per_bucket: rate as f64 / PEAKS_PER_SEC as f64, pos: 0.0, cur_min: 0.0, cur_max: 0.0, out: AudioPeaks { sample_rate: rate, ..Default::default() }, have: false } }
    fn push(&mut self, v: f32) {
        self.cur_min = self.cur_min.min(v); self.cur_max = self.cur_max.max(v); self.have = true;
        self.pos += 1.0;
        if self.pos >= self.per_bucket {
            self.pos -= self.per_bucket;
            self.out.min.push(self.cur_min); self.out.max.push(self.cur_max);
            self.cur_min = 0.0; self.cur_max = 0.0; self.have = false;
        }
    }
    fn finish(mut self) -> AudioPeaks { if self.have { self.out.min.push(self.cur_min); self.out.max.push(self.cur_max); } let _ = self.rate; self.out }
}

pub fn decode_peaks(path: &Path, progress: &mut dyn FnMut(f32)) -> Result<AudioPeaks, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) { hint.with_extension(ext); }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| format!("format: {e}"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL).ok_or("no audio track")?.clone();
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("codec: {e}"))?;
    let rate = track.codec_params.sample_rate.ok_or("unknown sample rate")?;
    let total = track.codec_params.n_frames;
    let mut acc = Acc::new(rate);
    let mut buf: Option<SampleBuffer<f32>> = None;
    let mut done = 0u64;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymErr::IoError(_)) | Err(SymErr::ResetRequired) => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id() != track.id { continue; }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let ch = spec.channels.count().max(1);
                if buf.as_ref().map_or(true, |b| b.capacity() < decoded.capacity() * ch) { buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec)); }
                let b = buf.as_mut().unwrap();
                b.copy_interleaved_ref(decoded);
                for frame in b.samples().chunks(ch) { acc.push(frame.iter().sum::<f32>() / ch as f32); }
                done += (b.samples().len() / ch) as u64;
                if let Some(t) = total { if done % 65536 < 4096 { progress((done as f32 / t as f32).min(1.0)); } }
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    progress(1.0);
    Ok(acc.finish())
}

/// Synthetic peaks for tests / demos.
pub fn synth(seconds: usize) -> AudioPeaks {
    let n = seconds * PEAKS_PER_SEC;
    let mut p = AudioPeaks { sample_rate: 48000, ..Default::default() };
    for i in 0..n {
        let t = i as f32 / PEAKS_PER_SEC as f32;
        let env = ((t * 1.3).sin() * 0.5 + 0.5) * (0.4 + 0.6 * ((t * 7.0).sin().abs()));
        p.max.push(env); p.min.push(-env * 0.9);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(path: &Path, secs: u32) {
        let rate = 8000u32; let n = rate * secs;
        let mut d = Vec::new();
        d.extend(b"RIFF"); d.extend((36 + n * 2).to_le_bytes()); d.extend(b"WAVEfmt "); d.extend(16u32.to_le_bytes());
        d.extend(1u16.to_le_bytes()); d.extend(1u16.to_le_bytes()); d.extend(rate.to_le_bytes()); d.extend((rate * 2).to_le_bytes());
        d.extend(2u16.to_le_bytes()); d.extend(16u16.to_le_bytes()); d.extend(b"data"); d.extend((n * 2).to_le_bytes());
        for i in 0..n { let v = ((i as f32 * 0.05).sin() * 16000.0) as i16; d.extend(v.to_le_bytes()); }
        std::fs::write(path, d).unwrap();
    }
    #[test]
    fn decodes_wav() {
        let p = std::env::temp_dir().join("osiosub_test.wav");
        wav(&p, 3);
        let pk = decode_peaks(&p, &mut |_| {}).unwrap();
        assert!((pk.duration_ms() - 3000).abs() < 20, "{}", pk.duration_ms());
        let (lo, hi) = pk.range(0, 1000);
        assert!(hi > 0.4 && lo < -0.4);
    }
}

/// Decode via the `ffmpeg` binary (handles Opus/AC3/DTS/any container). Mono 8 kHz is plenty for a waveform.
pub fn decode_peaks_ffmpeg(path: &Path) -> Result<AudioPeaks, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"]).arg(path)
        .args(["-vn", "-sn", "-ac", "1", "-ar", "8000", "-f", "f32le", "-"])
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut out = child.stdout.take().unwrap();
    let mut acc = Acc::new(8000);
    let mut buf = vec![0u8; 1 << 16];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = out.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        carry.extend_from_slice(&buf[..n]);
        let usable = carry.len() / 4 * 4;
        for c in carry[..usable].chunks_exact(4) { acc.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]])); }
        carry.drain(..usable);
    }
    let _ = child.wait();
    let p = acc.finish();
    if p.min.is_empty() { Err("ffmpeg produced no audio".into()) } else { Ok(p) }
}

/// Best-effort loader: symphonia first, ffmpeg second.
pub fn load_audio(path: &Path, progress: &mut dyn FnMut(f32)) -> Result<AudioPeaks, String> {
    match decode_peaks(path, progress) {
        Ok(p) if !p.min.is_empty() => Ok(p),
        first => decode_peaks_ffmpeg(path).map_err(|e| format!("{} / {}", first.err().unwrap_or_default(), e)),
    }
}

#[cfg(test)]
mod ffmpeg_tests {
    use super::*;
    #[test]
    fn ffmpeg_fallback_if_present() {
        let p = Path::new("/tmp/claude-0/demo/t.mkv");
        if !p.exists() { return; }
        let pk = load_audio(p, &mut |_| {}).unwrap();
        assert!((pk.duration_ms() - 6000).abs() < 300, "{}", pk.duration_ms());
    }
}

// ---------------------------------------------------------------- video (via ffprobe / ffmpeg+libass)

#[derive(Clone, Debug, Default)]
pub struct VideoInfo { pub width: i32, pub height: i32, pub fps: f64, pub duration_ms: i64 }

pub fn probe_video(path: &Path) -> Result<VideoInfo, String> {
    use std::process::Command;
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate:format=duration", "-of", "default=nw=1"])
        .arg(path).output().map_err(|e| format!("ffprobe: {e}"))?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut v = VideoInfo::default();
    for l in s.lines() {
        if let Some((k, val)) = l.split_once('=') {
            match k {
                "width" => v.width = val.trim().parse().unwrap_or(0),
                "height" => v.height = val.trim().parse().unwrap_or(0),
                "r_frame_rate" => { if let Some((a, b)) = val.split_once('/') { let (a, b): (f64, f64) = (a.parse().unwrap_or(0.0), b.parse().unwrap_or(1.0)); if b != 0.0 { v.fps = a / b; } } }
                "duration" => v.duration_ms = (val.trim().parse::<f64>().unwrap_or(0.0) * 1000.0) as i64,
                _ => {}
            }
        }
    }
    if v.width == 0 { return Err("no video stream".into()); }
    Ok(v)
}

/// Render one frame with the subtitles burnt in by libass (through ffmpeg). Returns (w, h, RGBA).
pub fn render_frame(video: &Path, ass_text: &str, at_ms: i64, out_w: i32, src: &VideoInfo, fonts_dir: Option<&Path>) -> Result<(i32, i32, Vec<u8>), String> {
    use std::process::{Command, Stdio};
    use std::io::Read;
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("osiosub_prev_{}_{}.ass", std::process::id(), n));
    std::fs::write(&tmp, ass_text).map_err(|e| e.to_string())?;
    let out_h = ((src.height as f64 * out_w as f64 / src.width.max(1) as f64) / 2.0).round() as i32 * 2;
    let esc = |p: &Path| p.to_string_lossy().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'");
    let mut vf = format!("ass='{}'", esc(&tmp));
    if let Some(d) = fonts_dir { vf = format!("ass='{}':fontsdir='{}'", esc(&tmp), esc(d)); }
    vf.push_str(&format!(",scale={}:{}", out_w, out_h));
    let ss = format!("{:.3}", at_ms.max(0) as f64 / 1000.0);
    let mut child = Command::new("ffmpeg").args(["-v", "error", "-copyts", "-ss", &ss, "-i"]).arg(video)
        .args(["-vf", &vf, "-frames:v", "1", "-an", "-sn", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut buf = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() { let _ = e.read_to_string(&mut err); }
    let _ = child.wait();
    let _ = std::fs::remove_file(&tmp);
    if buf.len() < (out_w * out_h * 4) as usize { return Err(format!("no frame ({})", err.lines().next().unwrap_or(""))); }
    buf.truncate((out_w * out_h * 4) as usize);
    Ok((out_w, out_h, buf))
}

#[cfg(test)]
mod video_tests {
    use super::*;
    #[test]
    fn render_if_ffmpeg() {
        let v = Path::new("/tmp/claude-0/demo/v.mp4");
        if !v.exists() { return; }
        let info = probe_video(v).unwrap();
        assert!(info.width > 0 && info.fps > 0.0);
        let ass = "[Script Info]\nPlayResX: 640\nPlayResY: 360\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Merhaba\n";
        let (_, _, a) = render_frame(v, ass, 2000, 320, &info, None).unwrap();
        let (_, _, b) = render_frame(v, ass, 500, 320, &info, None).unwrap();
        assert_ne!(a, b, "subtitle should change the frame");
    }
}
