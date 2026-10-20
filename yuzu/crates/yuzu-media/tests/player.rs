//! libmpv player against generated media: every frame's luma encodes its index, so a rendered
//! pixel tells exactly which frame is on screen. Skipped when ffmpeg or libmpv is missing.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use yuzu_media::player::{Player, VideoFrame};

const FPS: f64 = 24000.0 / 1001.0;

fn fixture() -> Option<PathBuf> {
    if !yuzu_media::ffmpeg_available() || !yuzu_media::mpv::available() { eprintln!("skipped: ffmpeg or libmpv missing"); return None; }
    let dir = std::env::temp_dir().join("yuzu-media-tests");
    std::fs::create_dir_all(&dir).ok()?;
    let p = dir.join("frames.mkv");
    if !p.exists() {
        let tmp = dir.join(format!("frames-{}.mkv", std::process::id()));
        let ok = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=black:size=320x240:rate=24000/1001,geq=lum='16+mod(N\\,50)*4':cb=128:cr=128",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "10", "-c:v", "libx264", "-preset", "veryfast", "-qp", "0",
                "-g", "48", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&tmp).status().ok()?.success();
        if !ok { return None; }
        std::fs::rename(&tmp, &p).ok()?;
    }
    Some(p)
}

fn player() -> (Player, Receiver<()>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let p = Player::new(&[("ao", "null")], Box::new(move || { let _ = tx.lock().unwrap().send(()); })).expect("player");
    (p, rx)
}

fn wait<T>(rx: &Receiver<()>, secs: f64, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    loop {
        if let Some(v) = f() { return Some(v); }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() { return None; }
        let _ = rx.recv_timeout(left.min(Duration::from_millis(50)));
    }
}

/// Frame index encoded in the centre pixel (limited-range luma 16+4n → full-range RGB).
fn index_of(f: &VideoFrame) -> i64 {
    let (x, y) = (f.width / 2, f.height / 2);
    let r = f.rgba[(y * f.width + x) * 4] as f64;
    (r / (4.0 * 255.0 / 219.0)).round() as i64
}

fn open(p: &Player, rx: &Receiver<()>, path: &Path) {
    p.set_output_size(320, 240);
    p.open(path);
    wait(rx, 10.0, || p.state().loaded.then_some(())).expect("file loaded");
}

/// Seek and wait until the frame with this presentation time is on screen.
fn frame_at(p: &Player, rx: &Receiver<()>, ms: f64) -> VideoFrame {
    p.seek(ms, true);
    wait(rx, 10.0, || {
        let f = p.take_frame()?;
        ((f.time_ms - ms).abs() < 2.0 && !p.state().seeking).then_some(f)
    }).unwrap_or_else(|| panic!("no frame at {} ms (state {:?})", ms, p.state()))
}

#[test]
fn frame_exact_seeks() {
    let Some(path) = fixture() else { return };
    let (p, rx) = player();
    open(&p, &rx, &path);
    let st = p.state();
    assert_eq!((st.width, st.height), (320, 240));
    assert!((st.fps - FPS).abs() < 0.01, "{}", st.fps);
    assert!((st.duration_ms - 10_000.0).abs() < 100.0, "{}", st.duration_ms);
    for n in [0i64, 1, 5, 47, 48, 49, 100, 13, 7, 199] {
        // MKV stores milliseconds, so ask for the frame's rounded time
        let t = (n as f64 * 1000.0 / FPS).round();
        let f = frame_at(&p, &rx, t);
        assert_eq!(index_of(&f), n % 50, "frame {} (t={} ms, got pts {})", n, t, f.time_ms);
    }
}

#[test]
fn live_subtitles_redraw_while_paused() {
    let Some(path) = fixture() else { return };
    let (p, rx) = player();
    open(&p, &rx, &path);
    let base = frame_at(&p, &rx, 0.0);
    let centre = |f: &VideoFrame| { let i = (f.height / 2 * f.width + f.width / 2) * 4; (f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]) };
    assert!(centre(&base).0 < 10);
    let ass = |text: &str| format!("[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 240\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n{}", text);
    // a white box over the centre
    p.set_subtitles(&ass("Dialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,{\\pos(0,0)\\p1}m 100 80 l 220 80 220 160 100 160{\\p0}\n")).unwrap();
    let white = wait(&rx, 10.0, || p.take_frame().filter(|f| centre(f).0 > 200)).expect("subtitle drawn");
    assert!(centre(&white).1 > 200 && centre(&white).2 > 200);
    // edit: the box turns red, then disappears — without seeking, as while typing
    p.set_subtitles(&ass("Dialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,{\\pos(0,0)\\c&H0000FF&\\p1}m 100 80 l 220 80 220 160 100 160{\\p0}\n")).unwrap();
    wait(&rx, 10.0, || p.take_frame().filter(|f| { let c = centre(f); c.0 > 200 && c.1 < 60 })).expect("subtitle reloaded");
    p.set_subtitles(&ass("")).unwrap();
    wait(&rx, 10.0, || p.take_frame().filter(|f| centre(f).0 < 10)).expect("subtitle removed");
}

#[test]
fn play_range_stops_on_the_last_frame() {
    let Some(path) = fixture() else { return };
    let (p, rx) = player();
    open(&p, &rx, &path);
    frame_at(&p, &rx, 0.0);
    p.play_range(1000.0, 1500.0);
    // starts playing…
    wait(&rx, 5.0, || (!p.state().paused).then_some(())).expect("playing");
    // …and stops by itself on the last frame before 1.5 s (frame 35, pts 1459.8 ms)
    wait(&rx, 10.0, || { let s = p.state(); (s.paused && s.time_ms > 1400.0).then_some(()) }).expect("stopped");
    std::thread::sleep(Duration::from_millis(300));
    let st = p.state();
    assert!(st.paused, "{:?}", st);
    assert!(st.time_ms <= 1500.5 && st.time_ms >= 1440.0, "{:?}", st);
    let f = wait(&rx, 5.0, || { p.redraw(); p.take_frame() }).unwrap();
    assert_eq!(index_of(&f), 35);
    // normal playback afterwards is not limited by the old end point
    p.set_pause(false);
    wait(&rx, 10.0, || (p.state().time_ms > 2200.0).then_some(())).expect("plays past the old end");
    p.set_pause(true);
}

#[test]
fn audio_decodes_to_pcm_and_spectrum() {
    let Some(path) = fixture() else { return };
    let mut pcm: Vec<i16> = Vec::new();
    yuzu_media::audio::decode(&path, |s| { pcm.extend_from_slice(s); true }).unwrap();
    let secs = pcm.len() as f64 / yuzu_media::audio::RATE as f64;
    assert!((secs - 10.0).abs() < 0.1, "{}", secs);
    let sp = yuzu_media::audio::spectrogram(&pcm, yuzu_media::audio::RATE, 10, 128);
    assert!(sp.columns() >= 990);
    // a 440 Hz tone: the same low row is the brightest everywhere
    let rows: Vec<usize> = (100..900).step_by(100).map(|c| sp.column(c).iter().enumerate().max_by_key(|x| *x.1).unwrap().0).collect();
    assert!(rows.windows(2).all(|w| w[0] == w[1]), "{:?}", rows);
    assert!(rows[0] > 10 && rows[0] < 50, "{:?}", rows);
}
