//! Media access through the system `ffmpeg`/`ffprobe` binaries (no native
//! codec libraries to link; libass inside ffmpeg gives true ASS rendering).
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub duration_ms: i64,
    pub fps: Option<f64>,
    pub width: i32,
    pub height: i32,
    pub has_video: bool,
    pub has_audio: bool,
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

fn parse_rate(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    (b > 0.0 && a > 0.0).then(|| a / b)
}

pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe not available: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut info = MediaInfo::default();
    if let Some(d) = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()) {
        info.duration_ms = (d * 1000.0) as i64;
    }
    for s in v["streams"].as_array().cloned().unwrap_or_default() {
        match s["codec_type"].as_str() {
            Some("video") if !info.has_video && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                info.has_video = true;
                info.width = s["width"].as_i64().unwrap_or(0) as i32;
                info.height = s["height"].as_i64().unwrap_or(0) as i32;
                info.fps = s["avg_frame_rate"].as_str().and_then(parse_rate).or_else(|| s["r_frame_rate"].as_str().and_then(parse_rate));
            }
            Some("audio") => info.has_audio = true,
            _ => {}
        }
    }
    Ok(info)
}

/// Keyframe timestamps in ms (packet scan; fast, no decoding).
pub fn keyframes(path: &Path) -> Result<Vec<i64>, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    let mut v = vec![];
    for l in String::from_utf8_lossy(&out.stdout).lines() {
        let mut it = l.split(',');
        let (t, f) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
        if f.contains('K') {
            if let Ok(t) = t.parse::<f64>() {
                v.push((t * 1000.0).round() as i64);
            }
        }
    }
    v.sort_unstable();
    v.dedup();
    Ok(v)
}

/// Peak envelope: for every `window_ms`, (min, max) in i16.
#[derive(Debug, Clone, Default)]
pub struct Waveform {
    pub window_ms: i64,
    pub peaks: Vec<(i16, i16)>,
    pub done: bool,
    pub error: Option<String>,
}

pub type SharedWaveform = Arc<Mutex<Waveform>>;

/// Decode audio on a background thread, filling the waveform progressively.
pub fn spawn_waveform(path: &Path, window_ms: i64) -> SharedWaveform {
    const RATE: usize = 8000;
    let shared = Arc::new(Mutex::new(Waveform { window_ms, ..Default::default() }));
    let sh = shared.clone();
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let child = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-vn", "-ac", "1", "-ar", &RATE.to_string(), "-f", "s16le", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let mut w = sh.lock().unwrap();
                w.error = Some(format!("ffmpeg: {e}"));
                w.done = true;
                return;
            }
        };
        let mut out = child.stdout.take().unwrap();
        let win = RATE * window_ms as usize / 1000;
        let mut buf = vec![0u8; 64 * 1024];
        let mut carry: Vec<i16> = Vec::new();
        let mut odd: Option<u8> = None;
        loop {
            let n = match out.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let mut bytes = &buf[..n];
            let mut tmp;
            if let Some(b) = odd.take() {
                tmp = vec![b];
                tmp.extend_from_slice(bytes);
                bytes = &tmp;
            }
            let pairs = bytes.len() / 2;
            if bytes.len() % 2 == 1 {
                odd = Some(bytes[bytes.len() - 1]);
            }
            for i in 0..pairs {
                carry.push(i16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]));
            }
            let mut new = vec![];
            while carry.len() >= win {
                let chunk: Vec<i16> = carry.drain(..win).collect();
                new.push((*chunk.iter().min().unwrap(), *chunk.iter().max().unwrap()));
            }
            if !new.is_empty() {
                sh.lock().unwrap().peaks.extend(new);
            }
        }
        if !carry.is_empty() {
            let (mn, mx) = (*carry.iter().min().unwrap(), *carry.iter().max().unwrap());
            sh.lock().unwrap().peaks.push((mn, mx));
        }
        let _ = child.wait();
        sh.lock().unwrap().done = true;
    });
    shared
}

fn escape_filter_path(p: &str) -> String {
    p.replace('\\', "/").replace(':', "\\:").replace('\'', "\\'").replace(',', "\\,").replace('[', "\\[").replace(']', "\\]")
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Grab one frame at `ms`, scaled to `width`, optionally with subtitles burned in
/// by libass from `ass_path`.
pub fn grab_frame(video: &Path, ms: i64, width: i32, src_w: i32, src_h: i32, ass_path: Option<&Path>, fonts_dir: Option<&Path>) -> Result<Frame, String> {
    let w = (width.max(16) / 2 * 2) as usize;
    let h = ((w as f64 * src_h.max(1) as f64 / src_w.max(1) as f64).round() as usize / 2) * 2;
    let mut vf = String::new();
    if let Some(a) = ass_path {
        vf.push_str(&format!("ass=filename='{}'", escape_filter_path(&a.to_string_lossy())));
        if let Some(f) = fonts_dir {
            vf.push_str(&format!(":fontsdir='{}'", escape_filter_path(&f.to_string_lossy())));
        }
        vf.push(',');
    }
    vf.push_str(&format!("scale={w}:{h}"));
    // -ss after -i is slow but exact with the ass filter timeline; use input seek + setpts offset
    let t = (ms.max(0) as f64) / 1000.0;
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{t:.3}"), "-copyts", "-i"])
        .arg(video)
        .args(["-vf", &vf, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if out.stdout.len() < w * h * 4 {
        return Err(format!("no frame: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(Frame { width: w, height: h, rgba: out.stdout[..w * h * 4].to_vec() })
}

/// Render `text` with `style` over a mid-gray checker background using libass.
pub fn render_style_preview(style: &subforge_core::Style, text: &str, width: usize, height: usize) -> Result<Frame, String> {
    let mut sc = subforge_core::Script::new();
    sc.info_set("PlayResX", &(width * 2).to_string());
    sc.info_set("PlayResY", &(height * 2).to_string());
    let mut st = style.clone();
    st.name = "Default".into();
    sc.styles = vec![st];
    sc.events.push(subforge_core::Event::new(0, 5000, "Default", text));
    let path = std::env::temp_dir().join(format!("sf-style-{}-{}.ass", std::process::id(), width));
    std::fs::write(&path, sc.to_ass()).map_err(|e| e.to_string())?;
    let vf = format!("ass=filename='{}'", escape_filter_path(&path.to_string_lossy()));
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", &format!("testsrc2=size={width}x{height}:rate=1:duration=1,format=gray,eq=brightness=-0.35:contrast=0.5,format=rgba")])
        .args(["-vf", &vf, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    let _ = std::fs::remove_file(&path);
    if out.stdout.len() < width * height * 4 {
        return Err(format!("preview failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(Frame { width, height, rgba: out.stdout[..width * height * 4].to_vec() })
}

/// Real-time playback: ffmpeg decodes (with libass burn-in when `ass_path` is given) at
/// native speed (`-re`) and frames arrive on `rx`; audio is played by ffplay when present.
pub struct Playback {
    pub rx: std::sync::mpsc::Receiver<Frame>,
    pub start_ms: i64,
    pub fps: f64,
    pub started: std::time::Instant,
    pub width: usize,
    pub height: usize,
    video: Option<std::process::Child>,
    audio: Option<std::process::Child>,
}

impl Playback {
    pub fn stop(&mut self) {
        for c in [self.video.take(), self.audio.take()].into_iter().flatten() {
            let mut c = c;
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Media time now, from the wall clock.
    pub fn now_ms(&self) -> i64 {
        self.start_ms + self.started.elapsed().as_millis() as i64
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn start_playback(video: &Path, start_ms: i64, end_ms: Option<i64>, width: i32, src_w: i32, src_h: i32, fps: f64, ass_path: Option<&Path>, with_audio: bool) -> Result<Playback, String> {
    let w = (width.max(16) / 2 * 2) as usize;
    let h = ((w as f64 * src_h.max(1) as f64 / src_w.max(1) as f64).round() as usize / 2) * 2;
    let mut vf = String::new();
    if let Some(a) = ass_path {
        vf.push_str(&format!("ass=filename='{}',", escape_filter_path(&a.to_string_lossy())));
    }
    vf.push_str(&format!("scale={w}:{h}"));
    let t = start_ms.max(0) as f64 / 1000.0;
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-re", "-ss", &format!("{t:.3}"), "-copyts", "-i"]).arg(video);
    if let Some(e) = end_ms {
        cmd.args(["-to", &format!("{:.3}", e as f64 / 1000.0)]);
    }
    cmd.args(["-an", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "rgba", "-"]).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut out = child.stdout.take().ok_or("no stdout")?;
    let (tx2, rx2) = std::sync::mpsc::channel::<Frame>();
    std::thread::spawn(move || {
        let size = w * h * 4;
        loop {
            let mut buf = vec![0u8; size];
            if out.read_exact(&mut buf).is_err() {
                break;
            }
            if tx2.send(Frame { width: w, height: h, rgba: buf }).is_err() {
                break;
            }
        }
    });
    let audio = if with_audio { play_audio(video, start_ms, end_ms) } else { None };
    Ok(Playback { rx: rx2, start_ms, fps, started: std::time::Instant::now(), width: w, height: h, video: Some(child), audio })
}

/// Play [start,end] of the media's audio with ffplay (if installed). Returns the child.
pub fn play_audio(path: &Path, start_ms: i64, end_ms: Option<i64>) -> Option<std::process::Child> {
    let mut c = Command::new("ffplay");
    c.args(["-v", "quiet", "-nodisp", "-autoexit", "-ss", &format!("{:.3}", start_ms as f64 / 1000.0)]);
    if let Some(e) = end_ms {
        c.args(["-t", &format!("{:.3}", (e - start_ms).max(0) as f64 / 1000.0)]);
    }
    c.arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_clip() -> Option<std::path::PathBuf> {
        if !ffmpeg_available() {
            return None;
        }
        let p = std::env::temp_dir().join(format!("sf-clip-{}.mkv", std::process::id()));
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=24:duration=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-g", "24", "-shortest"])
            .arg(&p)
            .status()
            .ok()?;
        st.success().then_some(p)
    }

    #[test]
    fn probe_waveform_frame() {
        let Some(p) = make_clip() else { return };
        let info = probe(&p).unwrap();
        assert!(info.has_video && info.has_audio);
        assert_eq!((info.width, info.height), (320, 180));
        assert!((info.fps.unwrap() - 24.0).abs() < 0.01);
        let k = keyframes(&p).unwrap();
        assert!(k.len() >= 3 && k[0] < 50);
        let wf = spawn_waveform(&p, 10);
        for _ in 0..100 {
            if wf.lock().unwrap().done {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let w = wf.lock().unwrap();
        assert!(w.done && w.peaks.len() > 250, "peaks {}", w.peaks.len());
        assert!(w.peaks.iter().any(|(a, b)| b - a > 1000));
        let mut pb = start_playback(&p, 0, Some(1000), 160, 320, 180, 24.0, None, false).unwrap();
        let mut n = 0;
        while let Ok(f) = pb.rx.recv_timeout(std::time::Duration::from_millis(1500)) {
            assert_eq!((f.width, f.height), (160, 90));
            n += 1;
        }
        pb.stop();
        assert!((15..=26).contains(&n), "frames {n}");
        let sp = render_style_preview(&subforge_core::Style::default(), "Örnek Sample", 320, 90).unwrap();
        assert_eq!((sp.width, sp.height), (320, 90));
        let f = grab_frame(&p, 1000, 160, 320, 180, None, None).unwrap();
        assert_eq!((f.width, f.height), (160, 90));
    }
}

/// Copy of `script` containing only the events visible within `[from, to)` — rendering a
/// preview frame of a 200k-line karaoke script then costs the same as a small one.
pub fn preview_script(script: &subforge_core::Script, from: i64, to: i64) -> subforge_core::Script {
    subforge_core::Script {
        info: script.info.clone(),
        styles: script.styles.clone(),
        events: script.events.iter().filter(|e| !e.is_comment() && e.end > from && e.start < to).cloned().collect(),
        raw_sections: script.raw_sections.clone(),
        order: script.order.clone(),
        is_ssa: script.is_ssa,
    }
}
