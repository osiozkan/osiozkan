//! Video kapsayıcısıyla altyazı işleri (ffmpeg): gömülü altyazı izlerini listeleme/çıkarma,
//! gömülü yazı tiplerini çıkarma, MKV'ye gömme (softsub, yazı tipi ekleriyle) ve videoya yakma
//! (hardsub, ilerleme bildirimli).
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq)]
pub struct SubTrack {
    /// ffmpeg akış dizini (0:N)
    pub index: u32,
    pub codec: String,
    pub language: String,
    pub title: String,
    pub default: bool,
}

impl SubTrack {
    /// Çıkarılacak dosya uzantısı (metin tabanlı izler); resim tabanlı (PGS/VobSub) → None.
    pub fn text_ext(&self) -> Option<&'static str> {
        match self.codec.as_str() {
            "ass" | "ssa" => Some("ass"),
            "subrip" | "srt" => Some("srt"),
            "webvtt" => Some("vtt"),
            "mov_text" | "text" => Some("srt"),
            _ => None,
        }
    }
    pub fn label(&self) -> String {
        let mut s = format!("#{} {}", self.index, self.codec);
        if !self.language.is_empty() {
            s.push_str(&format!(" [{}]", self.language));
        }
        if !self.title.is_empty() {
            s.push_str(&format!(" — {}", self.title));
        }
        if self.default {
            s.push_str(" (varsayılan)");
        }
        s
    }
}

pub fn subtitle_tracks(path: &Path) -> Result<Vec<SubTrack>, String> {
    let out = Command::new(crate::ffprobe_bin())
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-select_streams", "s"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe çalıştırılamadı: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    Ok(v["streams"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|s| SubTrack {
            index: s["index"].as_u64().unwrap_or(0) as u32,
            codec: s["codec_name"].as_str().unwrap_or("?").to_string(),
            language: s["tags"]["language"].as_str().unwrap_or("").to_string(),
            title: s["tags"]["title"].as_str().unwrap_or("").to_string(),
            default: s["disposition"]["default"].as_i64() == Some(1),
        })
        .collect())
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("ffmpeg çalıştırılamadı: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let e = String::from_utf8_lossy(&out.stderr);
        Err(e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ffmpeg hatası").to_string())
    }
}

/// Metin tabanlı altyazı izini dosyaya çıkarır (ASS izleri kayıpsız kopyalanır).
pub fn extract_subtitle(video: &Path, track: &SubTrack, out: &Path) -> Result<(), String> {
    let ext = track.text_ext().ok_or_else(|| format!("{} resim tabanlı; metne çevirmek için OCR gerekir", track.codec))?;
    let codec = if ext == "ass" && matches!(track.codec.as_str(), "ass" | "ssa") { "copy" } else { ext };
    let codec = if codec == "vtt" { "webvtt" } else if codec == "srt" { "srt" } else { codec };
    run(Command::new(crate::ffmpeg_bin()).args(["-v", "error", "-y", "-i"]).arg(video).args(["-map", &format!("0:{}", track.index), "-c:s", codec]).arg(out))
}

/// Gömülü yazı tiplerini (MKV ekleri) klasöre çıkarır; çıkarılan dosyaları döndürür.
pub fn extract_attachments(video: &Path, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let before: Vec<PathBuf> = std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    // ffmpeg -dump_attachment ekleri yazdıktan sonra "çıktı yok" hatası verir; yok sayılır
    let _ = Command::new(crate::ffmpeg_bin()).current_dir(dir).args(["-v", "error", "-y", "-dump_attachment:t", "", "-i"]).arg(std::fs::canonicalize(video).unwrap_or(video.to_path_buf())).stdin(Stdio::null()).output();
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| !before.contains(p)).collect();
    v.sort();
    Ok(v)
}

fn font_mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("otf") => "application/vnd.ms-opentype",
        Some("ttc") => "application/x-truetype-font",
        _ => "application/x-truetype-font",
    }
}

#[derive(Clone, Debug, Default)]
pub struct MuxOptions {
    /// ISO 639-2 dil kodu (ör. "tur")
    pub language: String,
    pub title: String,
    /// Kaynaktaki mevcut altyazı izlerini koru
    pub keep_existing_subs: bool,
    /// Eklenecek yazı tipi dosyaları
    pub fonts: Vec<PathBuf>,
    /// Yeni iz varsayılan olsun
    pub default_track: bool,
}

/// Videoya altyazıyı MKV izi olarak gömer (yeniden kodlama yok).
pub fn mux_mkv(video: &Path, subtitle: &Path, out: &Path, o: &MuxOptions) -> Result<(), String> {
    let mut cmd = Command::new(crate::ffmpeg_bin());
    cmd.args(["-v", "error", "-y", "-i"]).arg(video).arg("-i").arg(subtitle);
    cmd.args(["-map", "0:v?", "-map", "0:a?"]);
    let existing = if o.keep_existing_subs { subtitle_tracks(video).map(|t| t.len()).unwrap_or(0) } else { 0 };
    if o.keep_existing_subs {
        cmd.args(["-map", "0:s?", "-map", "0:t?"]);
    }
    cmd.args(["-map", "1:0", "-c", "copy"]);
    let si = existing; // yeni izin altyazı dizini
    if !o.language.is_empty() {
        cmd.args([format!("-metadata:s:s:{si}"), format!("language={}", o.language)]);
    }
    if !o.title.is_empty() {
        cmd.args([format!("-metadata:s:s:{si}"), format!("title={}", o.title)]);
    }
    if o.default_track {
        for k in 0..existing {
            cmd.args([format!("-disposition:s:{k}"), "0".into()]);
        }
        cmd.args([format!("-disposition:s:{si}"), "default".into()]);
    }
    let existing_att = if o.keep_existing_subs { attachment_count(video) } else { 0 };
    for (k, f) in o.fonts.iter().enumerate() {
        cmd.arg("-attach").arg(f);
        cmd.args([format!("-metadata:s:t:{}", existing_att + k), format!("mimetype={}", font_mime(f))]);
    }
    cmd.arg(out);
    run(&mut cmd)
}

fn attachment_count(video: &Path) -> usize {
    Command::new(crate::ffprobe_bin())
        .args(["-v", "error", "-select_streams", "t", "-show_entries", "stream=index", "-of", "csv=p=0"])
        .arg(video)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

/// ffmpeg süzgeç yolu için kaçış (ass=...:fontsdir=...).
fn filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    s.replace(':', "\\:").replace('\'', "\\'").replace(',', "\\,").replace('[', "\\[").replace(']', "\\]")
}

#[derive(Clone, Debug)]
pub struct BurnOptions {
    pub crf: u32,
    pub preset: String,
    pub fonts_dir: Option<PathBuf>,
}

impl Default for BurnOptions {
    fn default() -> Self {
        BurnOptions { crf: 18, preset: "medium".into(), fonts_dir: None }
    }
}

/// Altyazıyı videoya yakar (x264 yeniden kodlama, ses kopyalanır). `progress(oran 0..1)` çağrılır;
/// `cancel` true dönerse işlem durdurulur.
pub fn burn(video: &Path, ass: &Path, out: &Path, o: &BurnOptions, duration_ms: i64, mut progress: impl FnMut(f64), cancel: impl Fn() -> bool) -> Result<(), String> {
    let mut vf = format!("ass='{}'", filter_path(ass));
    if let Some(d) = &o.fonts_dir {
        vf.push_str(&format!(":fontsdir='{}'", filter_path(d)));
    }
    let mut child = Command::new(crate::ffmpeg_bin())
        .args(["-v", "error", "-y", "-nostats", "-progress", "pipe:1", "-i"])
        .arg(video)
        .args(["-map", "0:v:0", "-map", "0:a?", "-vf", &vf, "-c:v", "libx264", "-crf", &o.crf.to_string(), "-preset", &o.preset, "-pix_fmt", "yuv420p", "-c:a", "copy", "-sn"])
        .arg(out)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg çalıştırılamadı: {e}"))?;
    let stdout = child.stdout.take().ok_or("stdout yok")?;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(out);
            return Err("İptal edildi".into());
        }
        if let Some(v) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
            if let Ok(us) = v.trim().parse::<f64>() {
                if duration_ms > 0 {
                    progress((us / 1000.0 / duration_ms as f64).clamp(0.0, 1.0));
                }
            }
        }
    }
    let st = child.wait_with_output().map_err(|e| e.to_string())?;
    if st.status.success() {
        progress(1.0);
        Ok(())
    } else {
        let e = String::from_utf8_lossy(&st.stderr);
        Err(e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ffmpeg hatası").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASS: &str = "[Script Info]\nPlayResX: 320\nPlayResY: 240\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,30,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,5,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Merhaba dünya\n";

    #[test]
    fn mux_extract_burn_roundtrip() {
        if !crate::ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("osisub_mux_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("v.mkv");
        let ok = Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=24:duration=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest"])
            .arg(&video)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return; // libx264 yok
        }
        let sub = dir.join("s.ass");
        std::fs::write(&sub, ASS).unwrap();
        let font = dir.join("Test.ttf");
        std::fs::write(&font, b"not-a-real-font-but-bytes").unwrap();
        let muxed = dir.join("m.mkv");
        mux_mkv(&video, &sub, &muxed, &MuxOptions { language: "tur".into(), title: "Türkçe".into(), fonts: vec![font], default_track: true, ..Default::default() }).unwrap();
        let tracks = subtitle_tracks(&muxed).unwrap();
        assert_eq!(tracks.len(), 1, "{tracks:?}");
        assert_eq!((tracks[0].codec.as_str(), tracks[0].language.as_str(), tracks[0].title.as_str(), tracks[0].default), ("ass", "tur", "Türkçe", true));
        // ikinci iz ekle, ilkini koru
        let muxed2 = dir.join("m2.mkv");
        mux_mkv(&muxed, &sub, &muxed2, &MuxOptions { language: "eng".into(), keep_existing_subs: true, ..Default::default() }).unwrap();
        let t2 = subtitle_tracks(&muxed2).unwrap();
        assert_eq!(t2.iter().map(|t| t.language.as_str()).collect::<Vec<_>>(), vec!["tur", "eng"]);
        // çıkar: ASS içeriği korunur
        let out = dir.join("x.ass");
        extract_subtitle(&muxed2, &t2[0], &out).unwrap();
        let back = std::fs::read_to_string(&out).unwrap();
        assert!(back.contains("Merhaba dünya") && back.contains("Style: Default,DejaVu Sans,30"), "{back}");
        // ekler
        let att = extract_attachments(&muxed2, &dir.join("fonts")).unwrap();
        assert_eq!(att.len(), 1, "{att:?}");
        assert_eq!(std::fs::read(&att[0]).unwrap(), b"not-a-real-font-but-bytes");
        // yak (ilerleme bildirimiyle)
        let burned = dir.join("b.mp4");
        let mut last = 0.0;
        burn(&video, &sub, &burned, &BurnOptions { preset: "ultrafast".into(), ..Default::default() }, 2000, |p| last = p, || false).unwrap();
        assert_eq!(last, 1.0);
        assert!(std::fs::metadata(&burned).unwrap().len() > 1000);
        assert!(subtitle_tracks(&burned).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
