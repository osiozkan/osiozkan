//! Video/audio access through the system `ffmpeg`/`ffprobe` (no linked codecs, so any format the
//! user's ffmpeg understands works). Subtitle rendering uses ffmpeg's libass-backed `subtitles` filter.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_ms: i64,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// RGBA, row-major.
    pub rgba: Vec<u8>,
}

/// Min/max sample peaks at a fixed rate (peaks per second).
#[derive(Clone, Debug, Default)]
pub struct Peaks {
    pub per_second: u32,
    pub min: Vec<i16>,
    pub max: Vec<i16>,
}

impl Peaks {
    pub fn duration_ms(&self) -> i64 {
        (self.min.len() as i64 * 1000) / self.per_second.max(1) as i64
    }

    /// Peaks for a time window reduced to `columns` buckets: (min, max) per column, normalised to -1..1.
    pub fn window(&self, start_ms: i64, end_ms: i64, columns: usize) -> Vec<(f32, f32)> {
        let mut out = Vec::with_capacity(columns);
        let ps = self.per_second as f64;
        let span = (end_ms - start_ms).max(1) as f64;
        for c in 0..columns {
            let t0 = start_ms as f64 + span * c as f64 / columns as f64;
            let t1 = start_ms as f64 + span * (c + 1) as f64 / columns as f64;
            let i0 = ((t0 * ps / 1000.0).floor().max(0.0)) as usize;
            let i1 = (((t1 * ps / 1000.0).ceil()) as usize).max(i0 + 1);
            let (mut lo, mut hi) = (0i16, 0i16);
            for i in i0..i1.min(self.min.len()) {
                lo = lo.min(self.min[i]);
                hi = hi.max(self.max[i]);
            }
            out.push((lo as f32 / 32768.0, hi as f32 / 32768.0));
        }
        out
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &["", ".exe"] } else { &[""] };
    std::env::split_paths(&paths).flat_map(|p| exts.iter().map(move |e| p.join(format!("{name}{e}")))).find(|p| p.is_file())
}

pub fn ffmpeg_available() -> bool {
    which("ffmpeg").is_some() && which("ffprobe").is_some()
}

/// Keyframe (I-frame) times in ms, via ffprobe.
pub fn keyframes(path: &Path) -> Result<Vec<i64>, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-skip_frame", "nokey", "-show_entries", "frame=pts_time", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe not found: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(parse_keyframes(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_keyframes(text: &str) -> Vec<i64> {
    let mut v: Vec<i64> = text
        .lines()
        .filter_map(|l| l.split(',').next())
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .map(|s| (s * 1000.0).round() as i64)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Nearest keyframe to `ms` (within `max_dist_ms`).
pub fn nearest_keyframe(keys: &[i64], ms: i64, max_dist_ms: i64) -> Option<i64> {
    if keys.is_empty() {
        return None;
    }
    let i = keys.partition_point(|&k| k < ms);
    let cands = [i.checked_sub(1).and_then(|j| keys.get(j)), keys.get(i)];
    cands.into_iter().flatten().copied().min_by_key(|k| (k - ms).abs()).filter(|k| (k - ms).abs() <= max_dist_ms)
}

/// Play `[start_ms, end_ms)` of the media's audio with `ffplay` (no window). Kill the child to stop.
pub fn play_range(media: &Path, start_ms: i64, end_ms: i64) -> Result<std::process::Child, String> {
    if which("ffplay").is_none() {
        return Err("ffplay not found (install ffmpeg)".into());
    }
    Command::new("ffplay")
        .args(["-v", "quiet", "-nodisp", "-autoexit", "-ss", &format!("{:.3}", start_ms.max(0) as f64 / 1000.0), "-t", &format!("{:.3}", (end_ms - start_ms).max(100) as f64 / 1000.0)])
        .arg(media)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())
}

pub fn probe(path: &Path) -> Result<VideoInfo, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate,duration:format=duration", "-of", "default=noprint_wrappers=1"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe not found: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

pub fn parse_probe(text: &str) -> Result<VideoInfo, String> {
    let mut width = 0;
    let mut height = 0;
    let mut fps = 0.0;
    let mut dur = None;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k {
            "width" => width = v.parse().unwrap_or(0),
            "height" => height = v.parse().unwrap_or(0),
            "r_frame_rate" => {
                fps = match v.split_once('/') {
                    Some((n, d)) => n.parse::<f64>().unwrap_or(0.0) / d.parse::<f64>().unwrap_or(1.0).max(1.0),
                    None => v.parse().unwrap_or(0.0),
                }
            }
            "duration" => {
                if dur.is_none() {
                    dur = v.parse::<f64>().ok()
                }
            }
            _ => {}
        }
    }
    if width == 0 || height == 0 {
        return Err("no video stream".into());
    }
    Ok(VideoInfo { width, height, fps, duration_ms: (dur.unwrap_or(0.0) * 1000.0) as i64 })
}

/// Escape a path for use inside an ffmpeg filter argument (`subtitles=filename='…'`).
pub fn escape_filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    s.replace(':', "\\:").replace('\'', "\\'").replace('[', "\\[").replace(']', "\\]").replace(',', "\\,")
}

fn read_exact_frame(mut child: std::process::Child, w: u32, h: u32) -> Result<Frame, String> {
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let mut out = child.stdout.take().ok_or("no stdout")?;
    out.read_exact(&mut buf).map_err(|_| {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        format!("ffmpeg produced no frame: {}", err.lines().last().unwrap_or(""))
    })?;
    let _ = child.kill();
    let _ = child.wait();
    Ok(Frame { width: w, height: h, rgba: buf })
}

/// Grab one frame at `time_ms`, scaled to `size`, optionally with `ass_file` rendered on top (libass).
pub fn grab_frame(video: &Path, time_ms: i64, size: (u32, u32), ass_file: Option<&Path>, fonts_dir: Option<&Path>) -> Result<Frame, String> {
    let t = format!("{:.3}", time_ms.max(0) as f64 / 1000.0);
    let mut vf = format!("scale={}:{}", size.0, size.1);
    if let Some(a) = ass_file {
        // Input seeking resets timestamps to 0; restore them so the subtitles filter sees real time.
        vf.push_str(&format!(",setpts=PTS+{}/TB", t));
        // render at the target size so \pos etc. are scaled by libass against PlayRes
        vf.push_str(&format!(",subtitles=filename='{}':original_size={}x{}", escape_filter_path(a), size.0, size.1));
        if let Some(f) = fonts_dir {
            vf.push_str(&format!(":fontsdir='{}'", escape_filter_path(f)));
        }
    }
    let child = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &t, "-i"])
        .arg(video)
        .args(["-an", "-frames:v", "1", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg not found: {e}"))?;
    read_exact_frame(child, size.0, size.1)
}

/// Render subtitles over a solid background (for when no video is loaded).
pub fn render_on_blank(ass_file: &Path, time_ms: i64, size: (u32, u32), rgb: u32) -> Result<Frame, String> {
    let t = format!("{:.3}", time_ms.max(0) as f64 / 1000.0);
    let src = format!("color=c=0x{:06x}:s={}x{}:r=25:d={:.3}", rgb, size.0, size.1, time_ms.max(0) as f64 / 1000.0 + 2.0);
    let vf = format!("subtitles=filename='{}':original_size={}x{}", escape_filter_path(ass_file), size.0, size.1);
    let child = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", &src, "-vf", &vf, "-ss", &t, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg not found: {e}"))?;
    read_exact_frame(child, size.0, size.1)
}

/// Decode the audio track and reduce it to min/max peaks. `on_chunk` is called periodically with
/// the peaks so far, so a UI can draw the waveform while decoding continues.
pub fn audio_peaks(media: &Path, per_second: u32, mut on_chunk: impl FnMut(&Peaks)) -> Result<Peaks, String> {
    const RATE: u32 = 8000;
    let block = (RATE / per_second.max(1)) as usize;
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(media)
        .args(["-vn", "-ac", "1", "-ar", &RATE.to_string(), "-f", "s16le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg not found: {e}"))?;
    let mut out = child.stdout.take().ok_or("no stdout")?;
    let mut peaks = Peaks { per_second, ..Default::default() };
    let mut buf = vec![0u8; 64 * 1024];
    let mut carry: Vec<u8> = vec![];
    let (mut lo, mut hi, mut n) = (0i16, 0i16, 0usize);
    let mut last_report = 0usize;
    loop {
        let r = out.read(&mut buf).map_err(|e| e.to_string())?;
        if r == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..r]);
        let usable = carry.len() / 2 * 2;
        for ch in carry[..usable].chunks_exact(2) {
            let s = i16::from_le_bytes([ch[0], ch[1]]);
            lo = lo.min(s);
            hi = hi.max(s);
            n += 1;
            if n == block {
                peaks.min.push(lo);
                peaks.max.push(hi);
                lo = 0;
                hi = 0;
                n = 0;
            }
        }
        carry.drain(..usable);
        if peaks.min.len() >= last_report + per_second as usize * 5 {
            last_report = peaks.min.len();
            on_chunk(&peaks);
        }
    }
    if n > 0 {
        peaks.min.push(lo);
        peaks.max.push(hi);
    }
    let _ = child.wait();
    if peaks.min.is_empty() {
        return Err("no audio decoded".into());
    }
    Ok(peaks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_output() {
        let v = parse_probe("width=1920\nheight=1080\nr_frame_rate=24000/1001\nduration=12.5\nduration=13.0\n").unwrap();
        assert_eq!((v.width, v.height, v.duration_ms), (1920, 1080, 12500));
        assert!((v.fps - 23.976).abs() < 0.001);
        assert!(parse_probe("nothing").is_err());
    }

    #[test]
    fn keyframe_helpers() {
        let k = parse_keyframes("0.000000,\n2.002000,\n4.004000\n");
        assert_eq!(k, vec![0, 2002, 4004]);
        assert_eq!(nearest_keyframe(&k, 2100, 200), Some(2002));
        assert_eq!(nearest_keyframe(&k, 3000, 200), None);
        assert_eq!(nearest_keyframe(&k, 5000, 2000), Some(4004));
    }

    #[test]
    fn escapes() {
        assert_eq!(escape_filter_path(Path::new("C:\\a b\\it's.ass")), "C\\:/a b/it\\'s.ass");
    }

    #[test]
    fn peaks_window() {
        let p = Peaks { per_second: 100, min: vec![-100, -200, 0, -50], max: vec![100, 300, 10, 50] };
        let w = p.window(0, 40, 2);
        assert!(w[0].1 > w[1].1 * 0.0);
        assert_eq!(p.duration_ms(), 40);
    }

    /// End-to-end with a synthetic clip, skipped when ffmpeg isn't installed.
    #[test]
    fn ffmpeg_roundtrip() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("nsub-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("t.mkv");
        let ok = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=24:duration=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-shortest"])
            .arg(&clip)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return;
        }
        let info = probe(&clip).unwrap();
        assert_eq!((info.width, info.height), (320, 180));
        assert!((info.fps - 24.0).abs() < 0.01);
        let f = grab_frame(&clip, 1000, (160, 90), None, None).unwrap();
        assert_eq!(f.rgba.len(), 160 * 90 * 4);
        let keys = keyframes(&clip).unwrap();
        assert!(!keys.is_empty() && keys[0] <= 100, "{keys:?}");
        let pk = audio_peaks(&clip, 100, |_| {}).unwrap();
        assert!(pk.duration_ms() > 2500);
        assert!(pk.max.iter().any(|&m| m > 2000), "sine should be audible");
        // subtitle rendering changes pixels
        let ass = dir.join("s.ass");
        std::fs::write(&ass, "[Script Info]\nPlayResX: 320\nPlayResY: 180\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,40,&H0000FF00,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,5,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,HELLO\n").unwrap();
        // outside the event's time range nothing is drawn; inside it, pixels change
        let before = grab_frame(&clip, 1000, (160, 90), Some(&ass), None).unwrap();
        assert_eq!(f.rgba, before.rgba);
        let plain = grab_frame(&clip, 2500, (160, 90), None, None).unwrap();
        let during = grab_frame(&clip, 2500, (160, 90), Some(&ass), None).unwrap();
        assert_ne!(plain.rgba, during.rgba, "subtitle must be visible at its own time");
        let b = render_on_blank(&ass, 2500, (160, 90), 0x202020).unwrap();
        assert!(b.rgba.chunks(4).any(|p| p[1] > 200 && p[0] < 80), "green text expected");
        std::fs::remove_dir_all(dir).ok();
    }
}
