//! Media helpers built on the `ffmpeg`/`ffprobe` executables (found on PATH or via `KAKU_FFMPEG` / `KAKU_FFPROBE`).
//!
//! Using the external tools keeps Kaku small while supporting every container/codec the user's ffmpeg does
//! (MKV/MP4, FLAC/AAC/Opus, …) and gives us libass rendering through ffmpeg's `ass` filter.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub enum MediaError {
    ToolMissing(String),
    Failed(String),
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaError::ToolMissing(t) => write!(f, "{t} not found on PATH (install ffmpeg or set KAKU_FFMPEG/KAKU_FFPROBE)"),
            MediaError::Failed(m) => write!(f, "{m}"),
        }
    }
}
impl std::error::Error for MediaError {}

fn tool(name: &str, env: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| name.to_string())
}

fn run(cmd: &mut Command, name: &str) -> Result<std::process::Output, MediaError> {
    cmd.output().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { MediaError::ToolMissing(name.into()) } else { MediaError::Failed(e.to_string()) })
}

#[derive(Debug, Clone, Default)]
pub struct AudioStream {
    pub index: usize,
    pub codec: String,
    pub lang: String,
    pub channels: u32,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    /// Frame rate as (num, den) of the first video stream.
    pub fps: Option<(u32, u32)>,
    pub audio: Vec<AudioStream>,
    pub has_video: bool,
}

fn parse_ratio(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
    if a == 0 || b == 0 { None } else { Some((a, b)) }
}

pub fn probe(path: &Path) -> Result<MediaInfo, MediaError> {
    let out = run(
        Command::new(tool("ffprobe", "KAKU_FFPROBE")).args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path),
        "ffprobe",
    )?;
    if !out.status.success() {
        return Err(MediaError::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| MediaError::Failed(e.to_string()))?;
    let mut info = MediaInfo::default();
    info.duration_ms = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()).map(|d| (d * 1000.0).round() as i64).unwrap_or(0);
    for s in v["streams"].as_array().cloned().unwrap_or_default() {
        match s["codec_type"].as_str() {
            Some("video") if !info.has_video && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                info.has_video = true;
                info.width = s["width"].as_u64().unwrap_or(0) as u32;
                info.height = s["height"].as_u64().unwrap_or(0) as u32;
                info.fps = s["avg_frame_rate"].as_str().and_then(parse_ratio).or_else(|| s["r_frame_rate"].as_str().and_then(parse_ratio));
            }
            Some("audio") => info.audio.push(AudioStream {
                index: s["index"].as_u64().unwrap_or(0) as usize,
                codec: s["codec_name"].as_str().unwrap_or("").to_string(),
                lang: s["tags"]["language"].as_str().unwrap_or("").to_string(),
                channels: s["channels"].as_u64().unwrap_or(0) as u32,
                sample_rate: s["sample_rate"].as_str().and_then(|x| x.parse().ok()).unwrap_or(0),
            }),
            _ => {}
        }
    }
    Ok(info)
}

/// Audio envelope: per-bin (min, max) normalised to i16, `bin_ms` milliseconds per bin.
#[derive(Debug, Clone, Default)]
pub struct Waveform {
    pub bin_ms: i64,
    pub bins: Vec<(i16, i16)>,
}

impl Waveform {
    pub fn duration_ms(&self) -> i64 {
        self.bins.len() as i64 * self.bin_ms
    }
    /// Peak (min,max) over `[start_ms, end_ms)`.
    pub fn range(&self, start_ms: i64, end_ms: i64) -> (i16, i16) {
        let a = (start_ms.max(0) / self.bin_ms) as usize;
        let b = ((end_ms.max(0) + self.bin_ms - 1) / self.bin_ms) as usize;
        let mut lo = 0i16;
        let mut hi = 0i16;
        for &(l, h) in self.bins.iter().take(b.max(a + 1)).skip(a) {
            lo = lo.min(l);
            hi = hi.max(h);
        }
        (lo, hi)
    }
    /// Simple energy-based silence search: nearest bin ≥ `from_ms` (searching in `dir` ±1) whose peak is below `thresh`.
    pub fn find_silence(&self, from_ms: i64, dir: i64, thresh: i16, max_ms: i64) -> Option<i64> {
        let mut t = from_ms;
        let limit = from_ms + dir * max_ms;
        while (dir > 0 && t <= limit) || (dir < 0 && t >= limit) {
            if t < 0 || t >= self.duration_ms() {
                return None;
            }
            let (lo, hi) = self.range(t, t + self.bin_ms);
            if hi.max(-lo) < thresh {
                return Some(t);
            }
            t += dir * self.bin_ms;
        }
        None
    }
}

/// Decode audio via ffmpeg to mono 8 kHz and reduce to an envelope.
pub fn waveform(path: &Path, audio_stream: Option<usize>, bin_ms: i64) -> Result<Waveform, MediaError> {
    const RATE: i64 = 8000;
    let per_bin = (RATE * bin_ms / 1000).max(1) as usize;
    let mut cmd = Command::new(tool("ffmpeg", "KAKU_FFMPEG"));
    cmd.args(["-v", "error", "-i"]).arg(path).arg("-vn");
    if let Some(i) = audio_stream {
        cmd.args(["-map", &format!("0:{i}")]);
    }
    cmd.args(["-ac", "1", "-ar", &RATE.to_string(), "-f", "s16le", "-"]).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { MediaError::ToolMissing("ffmpeg".into()) } else { MediaError::Failed(e.to_string()) })?;
    let mut out = child.stdout.take().unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    let mut bins: Vec<(i16, i16)> = Vec::new();
    let (mut lo, mut hi, mut n) = (0i16, 0i16, 0usize);
    let mut carry: Option<u8> = None;
    loop {
        let k = out.read(&mut buf).map_err(|e| MediaError::Failed(e.to_string()))?;
        if k == 0 {
            break;
        }
        let mut data: &[u8] = &buf[..k];
        let mut tmp;
        if let Some(c) = carry.take() {
            tmp = vec![c];
            tmp.extend_from_slice(data);
            data = &tmp[..];
        }
        let mut chunks = data.chunks_exact(2);
        for c in &mut chunks {
            let s = i16::from_le_bytes([c[0], c[1]]);
            lo = lo.min(s);
            hi = hi.max(s);
            n += 1;
            if n == per_bin {
                bins.push((lo, hi));
                lo = 0;
                hi = 0;
                n = 0;
            }
        }
        carry = chunks.remainder().first().copied();
    }
    if n > 0 {
        bins.push((lo, hi));
    }
    let status = child.wait().map_err(|e| MediaError::Failed(e.to_string()))?;
    if bins.is_empty() && !status.success() {
        return Err(MediaError::Failed("ffmpeg could not decode audio".into()));
    }
    Ok(Waveform { bin_ms, bins })
}

/// Keyframe times in ms (container-level, no decoding).
pub fn keyframes(path: &Path) -> Result<Vec<i64>, MediaError> {
    let out = run(
        Command::new(tool("ffprobe", "KAKU_FFPROBE")).args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0"]).arg(path),
        "ffprobe",
    )?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut v: Vec<i64> = text
        .lines()
        .filter_map(|l| {
            let (t, f) = l.split_once(',')?;
            if f.contains('K') { t.parse::<f64>().ok().map(|x| (x * 1000.0).round() as i64) } else { None }
        })
        .collect();
    v.sort_unstable();
    Ok(v)
}

fn escape_filter_path(p: &Path) -> String {
    // ffmpeg filter-graph escaping: backslash→/, ':' and '\'' and ',' must be escaped.
    p.to_string_lossy().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'").replace(',', "\\,").replace('[', "\\[").replace(']', "\\]")
}

/// Render one frame at `time_ms` as PNG bytes. If `ass` is given, subtitles are burned in with libass.
/// `fonts_dir` is passed to the `ass` filter for custom fonts. `max_width` downsizes the output.
pub fn render_frame(video: &Path, time_ms: i64, ass: Option<&Path>, fonts_dir: Option<&Path>, max_width: u32) -> Result<Vec<u8>, MediaError> {
    let mut vf = Vec::new();
    if let Some(a) = ass {
        // Use setpts so that the subtitle clock matches the seeked timestamp (input seeking resets pts to 0).
        vf.push(format!("setpts=PTS+{:.3}/TB", time_ms as f64 / 1000.0));
        let mut f = format!("ass='{}'", escape_filter_path(a));
        if let Some(d) = fonts_dir {
            f.push_str(&format!(":fontsdir='{}'", escape_filter_path(d)));
        }
        vf.push(f);
    }
    if max_width > 0 {
        vf.push(format!("scale='min({max_width},iw)':-2"));
    }
    let mut cmd = Command::new(tool("ffmpeg", "KAKU_FFMPEG"));
    cmd.args(["-v", "error", "-ss", &format!("{:.3}", time_ms.max(0) as f64 / 1000.0), "-i"]).arg(video).args(["-frames:v", "1"]);
    if !vf.is_empty() {
        cmd.args(["-vf", &vf.join(",")]);
    }
    cmd.args(["-f", "image2pipe", "-vcodec", "png", "-"]);
    let out = run(&mut cmd, "ffmpeg")?;
    if out.stdout.is_empty() {
        return Err(MediaError::Failed(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(out.stdout)
}

/// Play an audio range with `ffplay` (non-blocking). Returns the child process so the caller can kill it.
pub fn play_range(media: &Path, start_ms: i64, end_ms: i64) -> Result<std::process::Child, MediaError> {
    Command::new(tool("ffplay", "KAKU_FFPLAY"))
        .args(["-v", "quiet", "-nodisp", "-autoexit", "-ss", &format!("{:.3}", start_ms as f64 / 1000.0), "-t", &format!("{:.3}", (end_ms - start_ms).max(100) as f64 / 1000.0)])
        .arg(media)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { MediaError::ToolMissing("ffplay".into()) } else { MediaError::Failed(e.to_string()) })
}

/// Snap subtitle times to nearby keyframes (common anime fansub step): starts snap *back* within
/// `before_ms`, ends snap *forward* within `after_ms`. Returns changed line count.
pub fn snap_to_keyframes(script: &mut kaku_core::ass::Script, kf: &[i64], before_ms: i64, after_ms: i64) -> usize {
    let mut n = 0;
    for l in script.lines_mut() {
        let mut changed = false;
        // nearest keyframe at or before start, within window
        if let Some(&k) = kf.iter().rev().find(|&&k| k <= l.start) {
            if l.start - k <= before_ms && l.start != k {
                l.start = k;
                changed = true;
            }
        }
        if let Some(&k) = kf.iter().find(|&&k| k >= l.end) {
            if k - l.end <= after_ms && l.end != k {
                l.end = k;
                changed = true;
            }
        }
        if changed {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaku_core::ass::{Line, Script};

    #[test]
    fn keyframe_snapping() {
        let mut s = Script::new_default();
        s.push_line(Line::new(1010, 2990, "x"));
        s.push_line(Line::new(5000, 6000, "y"));
        let kf = [1000, 3000, 9000];
        assert_eq!(snap_to_keyframes(&mut s, &kf, 250, 250), 1);
        let v: Vec<_> = s.lines().map(|l| (l.start, l.end)).collect();
        assert_eq!(v, [(1000, 3000), (5000, 6000)]);
    }

    #[test]
    fn waveform_range_and_silence() {
        let w = Waveform { bin_ms: 10, bins: vec![(-100, 100), (0, 0), (0, 0), (-5000, 5000)] };
        assert_eq!(w.range(0, 40), (-5000, 5000));
        assert_eq!(w.find_silence(0, 1, 50, 100), Some(10));
        assert_eq!(w.find_silence(30, -1, 50, 100), Some(20));
    }

    #[test]
    fn real_media_if_ffmpeg() {
        let dir = std::env::temp_dir().join(format!("kaku-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t.mkv");
        let ok = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=3:size=320x180:rate=24", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-g", "24", "-shortest"])
            .arg(&f)
            .status();
        if !matches!(ok, Ok(s) if s.success()) {
            return;
        }
        let info = probe(&f).unwrap();
        assert!(info.has_video && info.width == 320 && info.fps == Some((24, 1)));
        assert_eq!(info.audio.len(), 1);
        let w = waveform(&f, None, 10).unwrap();
        assert!(w.duration_ms() > 2500, "{}", w.duration_ms());
        assert!(w.range(500, 600).1 > 1000);
        let kf = keyframes(&f).unwrap();
        assert!(kf.len() >= 2 && kf[0] < 50, "{kf:?}");
        let png = render_frame(&f, 1500, None, None, 0).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let _ = std::fs::remove_dir_all(dir);
    }
}
