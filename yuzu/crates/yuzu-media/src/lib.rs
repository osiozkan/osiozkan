//! Media helpers built on the ffmpeg/ffprobe command line tools (no native linking required).
//!
//! * `probe`      – duration, frame rate, size
//! * `keyframes`  – keyframe times from packet flags
//! * `peaks`      – audio min/max envelope for the waveform
//! * `render_frame` – a PNG/RGBA frame with subtitles burned in by libass (ffmpeg `ass` filter)
//! * [`player`]     – libmpv playback (frame-exact, A/V synced, live subtitles) when libmpv is installed
//! * [`audio`]      – in-memory PCM, waveform peaks and spectrogram for the timing view

pub mod audio;
pub mod mpv;
pub mod player;
pub mod vad;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    // Next to our own executable first (bundled), then PATH.
    if let Ok(me) = std::env::current_exe() {
        if let Some(d) = me.parent() {
            let p = d.join(&exe);
            if p.exists() { return Some(p); }
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(&exe)).find(|p| p.exists())
}

pub fn ffmpeg_available() -> bool { which("ffmpeg").is_some() && which("ffprobe").is_some() }

pub(crate) fn command(name: &str) -> Option<Command> { cmd(name) }

fn cmd(name: &str) -> Option<Command> {
    let mut c = Command::new(which(name)?);
    c.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    Some(c)
}

#[derive(Clone, Debug, Default)]
pub struct MediaInfo {
    pub width: i32,
    pub height: i32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub duration_ms: i64,
    pub has_video: bool,
    pub has_audio: bool,
    pub frames: i64,
}

fn parse_ratio(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    if a == 0 || b == 0 { None } else { Some((a, b)) }
}

pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    let out = cmd("ffprobe").ok_or("ffprobe not found")?
        .args(["-v", "error", "-show_entries", "stream=codec_type,width,height,r_frame_rate,avg_frame_rate,nb_frames:format=duration", "-of", "default=nw=1"])
        .arg(path).output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).to_string()); }
    let txt = String::from_utf8_lossy(&out.stdout);
    let mut mi = MediaInfo::default();
    // Keys repeat per stream; codec_type comes first inside each stream block in default output? Order is by field.
    let mut cur_video = false;
    for line in txt.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        match k {
            "codec_type" => {
                cur_video = v == "video";
                if v == "video" { mi.has_video = true; } else if v == "audio" { mi.has_audio = true; }
            }
            "width" if cur_video => { if let Ok(w) = v.parse() { if mi.width == 0 { mi.width = w; } } }
            "height" if cur_video => { if let Ok(h) = v.parse() { if mi.height == 0 { mi.height = h; } } }
            "avg_frame_rate" | "r_frame_rate" if cur_video => {
                if mi.fps_num == 0 { if let Some((n, d)) = parse_ratio(v) { mi.fps_num = n; mi.fps_den = d; } }
            }
            "nb_frames" if cur_video => { if let Ok(n) = v.parse() { if mi.frames == 0 { mi.frames = n; } } }
            "duration" => { if let Ok(d) = v.parse::<f64>() { mi.duration_ms = (d * 1000.0) as i64; } }
            _ => {}
        }
    }
    if mi.fps_num == 0 { mi.fps_num = 24000; mi.fps_den = 1001; }
    Ok(mi)
}

/// Keyframe times in ms.
pub fn keyframes(path: &Path) -> Result<Vec<i64>, String> {
    let out = cmd("ffprobe").ok_or("ffprobe not found")?
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0"])
        .arg(path).output().map_err(|e| e.to_string())?;
    let txt = String::from_utf8_lossy(&out.stdout);
    let mut v: Vec<i64> = txt.lines().filter_map(|l| {
        let (t, f) = l.split_once(',')?;
        if f.contains('K') { Some((t.parse::<f64>().ok()? * 1000.0).round() as i64) } else { None }
    }).collect();
    v.sort_unstable();
    Ok(v)
}

/// Audio envelope: one (min, max) pair of i8 per `bin_ms` milliseconds, mono.
pub struct Peaks {
    pub bin_ms: i64,
    pub data: Vec<(i8, i8)>,
}

pub const PEAK_RATE: u32 = 8000;

/// Streams audio through ffmpeg and calls `progress(bins_so_far)` periodically via `sink`.
pub fn peaks(path: &Path, bin_ms: i64, mut sink: impl FnMut(&[(i8, i8)]) -> bool) -> Result<(), String> {
    let mut child = cmd("ffmpeg").ok_or("ffmpeg not found")?
        .args(["-v", "error", "-i"]).arg(path)
        .args(["-vn", "-ac", "1", "-ar", &PEAK_RATE.to_string(), "-f", "s16le", "-"])
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let per_bin = (PEAK_RATE as i64 * bin_ms / 1000).max(1) as usize;
    let mut buf = vec![0u8; per_bin * 2 * 256];
    let mut carry: Vec<u8> = Vec::new();
    let mut out: Vec<(i8, i8)> = Vec::new();
    loop {
        let n = so.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        carry.extend_from_slice(&buf[..n]);
        let samples = carry.len() / 2;
        let bins = samples / per_bin;
        for b in 0..bins {
            let mut mn = i16::MAX; let mut mx = i16::MIN;
            for s in 0..per_bin {
                let i = (b * per_bin + s) * 2;
                let v = i16::from_le_bytes([carry[i], carry[i + 1]]);
                mn = mn.min(v); mx = mx.max(v);
            }
            out.push(((mn >> 8) as i8, (mx >> 8) as i8));
        }
        carry.drain(..bins * per_bin * 2);
        if !out.is_empty() {
            if !sink(&out) { let _ = child.kill(); return Ok(()); }
            out.clear();
        }
    }
    let _ = child.wait();
    Ok(())
}

pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Render one frame at `time_ms` scaled to `out_w` wide, with `ass_path` rendered on top (if any).
pub fn render_frame(video: &Path, time_ms: i64, out_w: i32, out_h: i32, ass_path: Option<&Path>) -> Result<Frame, String> {
    let mut vf = format!("scale={}:{}", out_w, out_h);
    if let Some(a) = ass_path {
        let p = a.to_string_lossy().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'");
        vf.push_str(&format!(",ass='{}'", p));
    }
    let t = format!("{:.3}", time_ms.max(0) as f64 / 1000.0);
    let out = cmd("ffmpeg").ok_or("ffmpeg not found")?
        .args(["-v", "error", "-ss", &t, "-i"]).arg(video)
        .args(["-frames:v", "1", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
    let need = (out_w * out_h * 4) as usize;
    if out.stdout.len() < need { return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()); }
    Ok(Frame { width: out_w as usize, height: out_h as usize, rgba: out.stdout[..need].to_vec() })
}

/// Spawn a pipe producing consecutive RGBA frames starting at `start_ms` (for realtime playback).
pub fn spawn_playback(video: &Path, start_ms: i64, out_w: i32, out_h: i32, fps: f64, ass_path: Option<&Path>) -> Result<Child, String> {
    let mut vf = format!("fps={:.5},scale={}:{}", fps, out_w, out_h);
    if let Some(a) = ass_path {
        let p = a.to_string_lossy().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'");
        vf.push_str(&format!(",ass='{}'", p));
    }
    let t = format!("{:.3}", start_ms.max(0) as f64 / 1000.0);
    cmd("ffmpeg").ok_or("ffmpeg not found")?
        .args(["-v", "error", "-ss", &t, "-i"]).arg(video)
        .args(["-an", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())
}

/// Play audio of [start_ms, end_ms) using ffplay (no window). Returns the child so it can be stopped.
pub fn play_audio(path: &Path, start_ms: i64, end_ms: Option<i64>) -> Result<Child, String> {
    let mut c = cmd("ffplay").ok_or("ffplay not found")?;
    c.args(["-v", "quiet", "-nodisp", "-autoexit", "-vn", "-ss", &format!("{:.3}", start_ms.max(0) as f64 / 1000.0)]);
    if let Some(e) = end_ms { c.args(["-t", &format!("{:.3}", ((e - start_ms).max(1)) as f64 / 1000.0)]); }
    c.arg(path).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())
}

/// Parse an ffmpeg-style timecodes file (v2) into per-frame times (ms).
pub fn parse_timecodes_v2(text: &str) -> Vec<f64> {
    text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.trim().parse::<f64>().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ratio() {
        assert_eq!(parse_ratio("24000/1001"), Some((24000, 1001)));
        assert_eq!(parse_ratio("0/0"), None);
    }
    #[test]
    fn real_media_roundtrip() {
        let p = std::path::Path::new("/tmp/yt/clip.mp4");
        if !p.exists() || !ffmpeg_available() { return; }
        let mi = probe(p).unwrap();
        assert_eq!((mi.width, mi.height), (640, 360));
        assert!(mi.has_audio && mi.has_video);
        assert_eq!((mi.fps_num, mi.fps_den), (24000, 1001));
        let k = keyframes(p).unwrap();
        assert!(k.len() >= 5 && k[0] == 0, "{:?}", k);
        let mut bins = 0;
        peaks(p, 10, |b| { bins += b.len(); true }).unwrap();
        assert!(bins > 500, "{}", bins);
        let f = render_frame(p, 1000, 320, 180, None).unwrap();
        assert_eq!(f.rgba.len(), 320 * 180 * 4);
    }
    #[test]
    fn tc2() {
        assert_eq!(parse_timecodes_v2("# timecode format v2\n0\n41.708\n83.417\n").len(), 3);
    }
}
