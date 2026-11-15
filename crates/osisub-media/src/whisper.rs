//! Sesten altyazı (Subtitle Edit'teki "Audio to text"): whisper.cpp komut satırı aracı
//! (`whisper-cli`; ffmpeg/Tesseract gibi harici, kurulu değilse hata). Ses ffmpeg ile 16 kHz
//! mono WAV'a çevrilir, whisper.cpp SRT yazar, sonuç temizlenip betiğe dönüştürülür.
//! Model (`ggml-*.bin`) kullanıcı tarafından indirilir: `OSISUB_WHISPER_MODEL` ya da
//! ayar klasöründeki `whisper/` klasörü.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn whisper_bin() -> String {
    super::tool_bin("whisper-cli", "OSISUB_WHISPER")
}

pub fn available() -> bool {
    Command::new(whisper_bin()).arg("--help").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Bulunan modeller: `OSISUB_WHISPER_MODEL` (dosya), ayar klasöründeki `whisper/*.bin`, programın
/// yanındaki `models/*.bin`.
pub fn models() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("OSISUB_WHISPER_MODEL") {
        v.push(PathBuf::from(p));
    }
    let mut dirs: Vec<PathBuf> = osisub_core::paths::config_dir().map(|d| d.join("whisper")).into_iter().collect();
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("models"))) {
        dirs.push(d);
    }
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            let mut f: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "bin")).collect();
            f.sort();
            v.extend(f);
        }
    }
    v
}

#[derive(Clone, Debug)]
pub struct Options {
    pub model: PathBuf,
    /// "auto", "ja", "en", "tr"…
    pub lang: String,
    /// Kaynak dilden İngilizceye çevir (whisper `-tr`)
    pub translate: bool,
    /// Satır başına azami karakter (0 = whisper varsayılanı); sözcük sınırında böler
    pub max_len: u32,
    pub threads: u32,
}

/// whisper.cpp argümanları (test edilebilir).
pub fn args(o: &Options, wav: &Path, out_base: &Path) -> Vec<String> {
    let mut a = vec!["-m".into(), o.model.to_string_lossy().into_owned(), "-f".into(), wav.to_string_lossy().into_owned(), "-l".into(), if o.lang.is_empty() { "auto".into() } else { o.lang.clone() }, "-osrt".into(), "-of".into(), out_base.to_string_lossy().into_owned(), "-pp".into()];
    if o.translate {
        a.push("-tr".into());
    }
    if o.max_len > 0 {
        a.extend(["-ml".into(), o.max_len.to_string(), "-sow".into()]);
    }
    if o.threads > 0 {
        a.extend(["-t".into(), o.threads.to_string()]);
    }
    a
}

/// whisper.cpp ilerleme satırı: "whisper_print_progress_callback: progress =  45%" → 0.45
pub fn parse_progress(line: &str) -> Option<f32> {
    let p = line.split("progress =").nth(1)?.trim().trim_end_matches('%').trim().parse::<f32>().ok()?;
    Some((p / 100.0).clamp(0.0, 1.0))
}

/// Ham whisper çıktısını temizler: boş ve yalnızca ses betimi olan satırlar ("[Music]", "(音楽)",
/// "♪") silinir, art arda yinelenen satırlar (whisper'ın yaygın "halüsinasyon" döngüsü) tek satırda
/// birleştirilir, boşluklar sadeleştirilir.
pub fn clean(ds: Vec<osisub_core::Dialogue>) -> Vec<osisub_core::Dialogue> {
    let mut out: Vec<osisub_core::Dialogue> = Vec::new();
    for mut d in ds {
        let t = d.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let bare = t.trim_matches(|c: char| "[]()（）♪♫~～ .".contains(c));
        let sound = (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('（') && t.ends_with('）'));
        if bare.is_empty() || sound {
            continue;
        }
        d.text = t;
        if let Some(prev) = out.last_mut() {
            if prev.text == d.text && d.start_time - prev.end_time <= 1000 {
                prev.end_time = prev.end_time.max(d.end_time);
                continue;
            }
        }
        out.push(d);
    }
    out
}

/// Medya dosyasını yazıya döker. `progress(0..1)` whisper ilerledikçe çağrılır.
pub fn transcribe(media: &Path, o: &Options, mut progress: impl FnMut(f32)) -> Result<osisub_core::Script, String> {
    if !o.model.is_file() {
        return Err(format!("Whisper modeli bulunamadı: {}", o.model.display()));
    }
    let dir = std::env::temp_dir().join(format!("osisub-whisper-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let r = (|| {
        let wav = dir.join("a.wav");
        let st = Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-y", "-i"])
            .arg(media)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
            .arg(&wav)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("ffmpeg çalıştırılamadı: {e}"))?;
        if !st.status.success() {
            return Err(format!("ses çıkarılamadı: {}", String::from_utf8_lossy(&st.stderr).trim()));
        }
        let base = dir.join("out");
        let mut child = Command::new(whisper_bin())
            .args(args(o, &wav, &base))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("whisper çalıştırılamadı ({}): {e}", whisper_bin()))?;
        let mut last = Vec::new();
        if let Some(err) = child.stderr.take() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(p) = parse_progress(&line) {
                    progress(p);
                }
                last.push(line);
                if last.len() > 5 {
                    last.remove(0);
                }
            }
        }
        let st = child.wait().map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(format!("whisper hatası: {}", last.join(" | ")));
        }
        let srt = std::fs::read_to_string(base.with_extension("srt")).map_err(|e| format!("whisper çıktısı okunamadı: {e}"))?;
        progress(1.0);
        Ok(osisub_core::Script::from_dialogues(clean(osisub_core::srt::parse(&srt))))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use osisub_core::Dialogue;

    fn d(s: i64, e: i64, t: &str) -> Dialogue {
        Dialogue { start_time: s, end_time: e, text: t.into(), ..Default::default() }
    }

    #[test]
    fn args_and_progress() {
        let o = Options { model: "m/ggml-small.bin".into(), lang: "ja".into(), translate: true, max_len: 42, threads: 0 };
        let a = args(&o, Path::new("a.wav"), Path::new("out"));
        assert_eq!(a.join(" "), "-m m/ggml-small.bin -f a.wav -l ja -osrt -of out -pp -tr -ml 42 -sow");
        assert_eq!(parse_progress("whisper_print_progress_callback: progress =  45%"), Some(0.45));
        assert_eq!(parse_progress("system_info: n_threads = 4"), None);
    }

    #[test]
    fn clean_drops_sound_tags_and_merges_repeats() {
        let v = clean(vec![d(0, 1000, " [Music] "), d(1000, 2000, "Where  are you?"), d(2100, 3000, "Where are you?"), d(3000, 4000, "♪ ♪"), d(4000, 5000, "(音楽)"), d(9000, 9500, "Where are you?"), d(9500, 9900, "(laughs) Okay.")]);
        let t: Vec<(i64, i64, &str)> = v.iter().map(|d| (d.start_time, d.end_time, d.text.as_str())).collect();
        assert_eq!(t, vec![(1000, 3000, "Where are you?"), (9000, 9500, "Where are you?"), (9500, 9900, "(laughs) Okay.")]);
    }

    /// Sahte whisper-cli (kabuk betiği) ile uçtan uca: ffmpeg ile ses çıkarma, argümanlar, ilerleme, SRT okuma.
    #[cfg(unix)]
    #[test]
    fn transcribe_with_fake_whisper() {
        if !crate::ffmpeg_available() {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("osisub-fake-whisper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("whisper-cli");
        // -of değerini bul, 16 kHz WAV'ı doğrula, ilerleme yaz, SRT üret
        std::fs::write(&fake, "#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in -of) OUT=$2; shift;; -f) WAV=$2; shift;; -l) L=$2; shift;; esac; shift; done\n[ \"$L\" = ja ] || exit 4\nhead -c 44 \"$WAV\" | od -An -tu4 -j24 -N4 | grep -q 16000 || { echo 'wav 16k degil' >&2; exit 3; }\necho 'whisper_print_progress_callback: progress =  50%' >&2\nprintf '1\\n00:00:00,500 --> 00:00:01,800\\nMerhaba\\n\\n2\\n00:00:02,000 --> 00:00:03,000\\n[Music]\\n' > \"$OUT.srt\"\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let media = dir.join("a.wav");
        let st = Command::new(crate::ffmpeg_bin()).args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=f=440:d=3:sample_rate=44100"]).arg(&media).status().unwrap();
        assert!(st.success());
        let model = dir.join("ggml-tiny.bin");
        std::fs::write(&model, b"x").unwrap();
        std::env::set_var("OSISUB_WHISPER", &fake);
        let mut seen = vec![];
        let sc = transcribe(&media, &Options { model, lang: "ja".into(), translate: false, max_len: 0, threads: 0 }, |p| seen.push(p)).unwrap();
        std::env::remove_var("OSISUB_WHISPER");
        let ds: Vec<&Dialogue> = sc.dialogues().collect();
        assert_eq!(ds.len(), 1);
        assert_eq!((ds[0].start_time, ds[0].end_time, ds[0].text.as_str()), (500, 1800, "Merhaba"));
        assert_eq!(seen, vec![0.5, 1.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
