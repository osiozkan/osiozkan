//! Media helpers built on `ffprobe`/`ffmpeg` (when installed): stream info, audio peaks for the
//! waveform, keyframes. Everything degrades gracefully when ffmpeg is missing.

use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaInfo {
    pub duration_ms: i64,
    pub fps: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub has_video: bool,
    pub has_audio: bool,
}

fn parse_rate(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    if b == 0.0 || a == 0.0 { None } else { Some(a / b) }
}

pub fn parse_probe(json: &str) -> Option<MediaInfo> {
    let v: Value = serde_json::from_str(json).ok()?;
    let mut mi = MediaInfo::default();
    if let Some(d) = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()) {
        mi.duration_ms = (d * 1000.0).round() as i64;
    }
    for s in v["streams"].as_array()? {
        match s["codec_type"].as_str() {
            Some("video") if !mi.has_video && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                mi.has_video = true;
                mi.width = s["width"].as_i64();
                mi.height = s["height"].as_i64();
                mi.fps = s["avg_frame_rate"].as_str().and_then(parse_rate).or_else(|| s["r_frame_rate"].as_str().and_then(parse_rate));
                if mi.duration_ms == 0 {
                    if let Some(d) = s["duration"].as_str().and_then(|d| d.parse::<f64>().ok()) {
                        mi.duration_ms = (d * 1000.0).round() as i64;
                    }
                }
            }
            Some("audio") => mi.has_audio = true,
            _ => {}
        }
    }
    Some(mi)
}

pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe not available: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| "could not parse ffprobe output".into())
}

/// Audio peaks at `bin_ms` resolution: two signed bytes (min, max) per bin, scaled from i16.
pub fn peaks(path: &Path, bin_ms: u32) -> Result<Vec<i8>, String> {
    const SR: u32 = 8000;
    let per = (SR * bin_ms / 1000).max(1) as usize;
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-vn", "-sn", "-dn", "-ac", "1", "-ar", &SR.to_string(), "-f", "s16le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg not available: {}", e))?;
    let mut out = child.stdout.take().unwrap();
    let mut buf = vec![0u8; 64 * 1024];
    let mut carry: Option<u8> = None;
    let (mut mn, mut mx, mut n) = (i16::MAX, i16::MIN, 0usize);
    let mut res: Vec<i8> = Vec::new();
    loop {
        let k = out.read(&mut buf).map_err(|e| e.to_string())?;
        if k == 0 {
            break;
        }
        let mut data: Vec<u8> = Vec::with_capacity(k + 1);
        if let Some(c) = carry.take() {
            data.push(c);
        }
        data.extend_from_slice(&buf[..k]);
        let even = data.len() / 2 * 2;
        if even < data.len() {
            carry = Some(data[even]);
        }
        for ch in data[..even].chunks_exact(2) {
            let s = i16::from_le_bytes([ch[0], ch[1]]);
            mn = mn.min(s);
            mx = mx.max(s);
            n += 1;
            if n == per {
                res.push((mn >> 8) as i8);
                res.push((mx >> 8) as i8);
                mn = i16::MAX;
                mx = i16::MIN;
                n = 0;
            }
        }
    }
    if n > 0 {
        res.push((mn >> 8) as i8);
        res.push((mx >> 8) as i8);
    }
    let _ = child.wait();
    if res.is_empty() {
        return Err("no audio decoded (does the file have an audio track?)".into());
    }
    Ok(res)
}

/// Keyframe timestamps (ms) from the first video stream's packets.
pub fn keyframes(path: &Path) -> Result<Vec<i64>, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe not available: {}", e))?;
    let txt = String::from_utf8_lossy(&out.stdout);
    let mut v: Vec<i64> = txt
        .lines()
        .filter_map(|l| {
            let (t, f) = l.split_once(',')?;
            if f.contains('K') { t.parse::<f64>().ok().map(|t| (t * 1000.0).round() as i64) } else { None }
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    Ok(v)
}

/// Render one frame with libass via ffmpeg's `ass` filter. `media` = video to draw on (seeking to `t_ms`);
/// without it a flat dark frame of `w`x`h` is used. Returns PNG bytes.
pub fn render_frame(ass_file: &Path, media: Option<&Path>, t_ms: i64, w: u32, fontsdir: Option<&Path>) -> Result<Vec<u8>, String> {
    let esc = |p: &Path| p.display().to_string().replace('\\', "/").replace(':', "\\:").replace('\'', "\\'");
    let mut filt = format!("ass='{}'", esc(ass_file));
    if let Some(d) = fontsdir {
        filt.push_str(&format!(":fontsdir='{}'", esc(d)));
    }
    let secs = format!("{:.3}", t_ms.max(0) as f64 / 1000.0);
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-y"]);
    match media {
        Some(m) => {
            cmd.args(["-ss", &secs, "-copyts", "-i"]).arg(m);
            cmd.args(["-vf", &format!("{},scale={}:-2", filt, w)]);
        }
        None => {
            let h = (w as f64 * 9.0 / 16.0).round() as u32 / 2 * 2;
            cmd.args(["-f", "lavfi", "-i", &format!("color=c=0x222222:s={}x{}:r=25", w, h)]);
            cmd.args(["-vf", &format!("setpts=PTS+{}/TB,{}", secs, filt)]);
        }
    }
    cmd.args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"]);
    let out = cmd.output().map_err(|e| format!("ffmpeg not available: {}", e))?;
    if out.stdout.is_empty() {
        return Err(format!("ffmpeg produced no frame: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

/// Scene-change timestamps (ms) using ffmpeg's `select=gt(scene,T)` (T ~ 0.3 is a good default).
pub fn scenes(path: &Path, threshold: f64) -> Result<Vec<i64>, String> {
    let out = Command::new("ffmpeg")
        .args(["-v", "info", "-nostdin", "-i"])
        .arg(path)
        .args(["-an", "-sn", "-vf", &format!("scale=320:-2,select='gt(scene,{})',showinfo", threshold), "-f", "null", "-"])
        .output()
        .map_err(|e| format!("ffmpeg not available: {}", e))?;
    let err = String::from_utf8_lossy(&out.stderr);
    let mut v: Vec<i64> = err
        .lines()
        .filter(|l| l.contains("showinfo") && l.contains("pts_time:"))
        .filter_map(|l| {
            let i = l.find("pts_time:")? + "pts_time:".len();
            let rest = &l[i..];
            let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
            rest[..end].parse::<f64>().ok().map(|t| (t * 1000.0).round() as i64)
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    Ok(v)
}

pub fn mime_for(path: &Path) -> &'static str {
    match path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        Some("ogg") | Some("opus") => "audio/ogg",
        Some("m4a") | Some("aac") => "audio/mp4",
        Some("mka") => "audio/x-matroska",
        _ => "application/octet-stream",
    }
}

/// Parse an HTTP `Range: bytes=a-b` header for a resource of `len` bytes -> (start, end_inclusive).
pub fn parse_range(h: &str, len: u64) -> Option<(u64, u64)> {
    let r = h.strip_prefix("bytes=")?;
    let (a, b) = r.split_once('-')?;
    if len == 0 {
        return None;
    }
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        return Some((len.saturating_sub(n), len - 1));
    }
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
    if start > end || start >= len { None } else { Some((start, end)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_parse_and_range() {
        let j = r#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"avg_frame_rate":"24000/1001","disposition":{"attached_pic":0}},{"codec_type":"audio"}],"format":{"duration":"1420.5"}}"#;
        let m = parse_probe(j).unwrap();
        assert_eq!((m.width, m.height, m.duration_ms, m.has_audio), (Some(1920), Some(1080), 1_420_500, true));
        assert!((m.fps.unwrap() - 23.976).abs() < 0.001);
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=2000-", 1000), None);
        assert_eq!(mime_for(Path::new("a.MKV")), "video/x-matroska");
    }

    #[test]
    fn libass_frame_when_available() {
        let ok = Command::new("ffmpeg").arg("-filters").output().map(|o| String::from_utf8_lossy(&o.stdout).contains(" ass ")).unwrap_or(false);
        if !ok {
            return;
        }
        let dir = std::env::temp_dir().join(format!("sf-frame-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.ass");
        std::fs::write(&f, std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typeset.ass")).unwrap()).unwrap();
        let png = render_frame(&f, None, 1500, 640, None).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 2000, "frame suspiciously empty: {} bytes", png.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scene_detection_when_available() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("sf-scene-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.mp4");
        // 2 s red, then 2 s blue (hard cut at 2.0 s)
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=red:s=160x90:r=25:d=2", "-f", "lavfi", "-i", "color=c=blue:s=160x90:r=25:d=2", "-filter_complex", "[0:v][1:v]concat=n=2:v=1[v]", "-map", "[v]", "-pix_fmt", "yuv420p"])
            .arg(&f)
            .status();
        if !st.map(|s| s.success()).unwrap_or(false) {
            return; // no encoder available
        }
        let sc = scenes(&f, 0.3).unwrap();
        assert!(sc.iter().any(|t| (*t - 2000).abs() <= 80), "scenes: {:?}", sc);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ffmpeg_roundtrip_when_installed() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let wav = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.wav");
        let p = peaks(&wav, 10).unwrap();
        // 12 s -> ~1200 bins x 2; tone at 1-3s loud, 3-5s silent
        assert!(p.len() >= 2 * 1190 && p.len() <= 2 * 1210, "{}", p.len());
        let loud = p[2 * 200 + 1];
        let quiet = p[2 * 400 + 1];
        assert!(loud > 60 && quiet.abs() < 3, "loud={} quiet={}", loud, quiet);
        let mi = probe(&wav).unwrap();
        assert!(mi.has_audio && !mi.has_video && (mi.duration_ms - 12_000).abs() < 100);
    }
}
