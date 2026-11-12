//! Toplu dosya işleme (Subtitle Edit "Batch convert"): birden çok altyazı dosyasına aynı
//! işlemleri uygulayıp istenen biçimde kaydeder. Sıra: zaman kaydır → sık hatalar → yorumları sil
//! → yeniden örnekle → kaydet.
use crate::ass::{Entry, Script};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Job {
    /// çıktı uzantısı (`ass`, `srt`, `vtt`…); boş = girdiyle aynı
    pub out_ext: String,
    /// çıktı klasörü; None = girdinin klasörü
    pub out_dir: Option<PathBuf>,
    /// çıktı adına eklenecek son ek (ör. `_tr`); aynı ada yazmayı önlemek için
    pub suffix: String,
    pub shift_ms: i64,
    pub fixes: Option<crate::fixes::Fixes>,
    pub remove_comments: bool,
    pub resample: Option<(i32, i32)>,
    /// girdinin üzerine yazmaya izin
    pub overwrite: bool,
}

impl Default for Job {
    fn default() -> Self {
        Job { out_ext: String::new(), out_dir: None, suffix: String::new(), shift_ms: 0, fixes: None, remove_comments: false, resample: None, overwrite: false }
    }
}

/// Bir dosyanın çıktı yolu.
pub fn out_path(input: &Path, job: &Job) -> PathBuf {
    let ext = if job.out_ext.trim().is_empty() { crate::io::ext_of(input) } else { job.out_ext.trim().trim_start_matches('.').to_lowercase() };
    let stem = input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "çıktı".into());
    let dir = job.out_dir.clone().or_else(|| input.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    dir.join(format!("{stem}{}.{ext}", job.suffix))
}

/// Betiğe işlemleri uygular (dosya G/Ç'siz; test edilebilir). Kısa rapor döner.
pub fn process(s: &mut Script, job: &Job) -> String {
    let mut notes = vec![];
    if job.shift_ms != 0 {
        crate::qc::shift(s, job.shift_ms, None);
        notes.push(format!("kaydırma {:+} ms", job.shift_ms));
    }
    if let Some(f) = &job.fixes {
        let r = crate::fixes::apply(s, f);
        notes.push(format!("{} satır düzeltildi", r.changed_lines + r.timing_changes));
    }
    if job.remove_comments {
        let before = s.entries.len();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(d) if d.comment));
        notes.push(format!("{} yorum silindi", before - s.entries.len()));
    }
    if let Some((x, y)) = job.resample {
        crate::timing::resample(s, x, y);
        notes.push(format!("{x}×{y}"));
    }
    notes.join(", ")
}

/// Tek dosyayı işler; çıktı yolunu ve raporu döndürür.
pub fn run_file(input: &Path, job: &Job) -> Result<(PathBuf, String), String> {
    let out = out_path(input, job);
    if !job.overwrite && out.exists() && same_file(&out, input) {
        return Err("çıktı girdinin üzerine yazacak (son ek verin ya da üzerine yazmaya izin verin)".into());
    }
    let mut s = crate::io::load(input)?;
    let note = process(&mut s, job);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::io::save(&s, &out)?;
    Ok((out, note))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_convert_and_fix() {
        let dir = std::env::temp_dir().join(format!("osisub-filebatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = "[Script Info]\nPlayResX: 640\nPlayResY: 360\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Merhaba  dünya\nComment: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,not\n";
        let a = dir.join("b01.ass");
        std::fs::write(&a, src).unwrap();
        // aynı ada yazma engellenir
        assert!(run_file(&a, &Job::default()).is_err());
        let job = Job { out_ext: "srt".into(), shift_ms: 500, fixes: Some(Default::default()), remove_comments: true, ..Default::default() };
        let (out, note) = run_file(&a, &job).unwrap();
        assert_eq!(out, dir.join("b01.srt"));
        let t = std::fs::read_to_string(&out).unwrap();
        assert!(t.contains("00:00:01,500 --> 00:00:02,500") && t.contains("Merhaba dünya") && !t.contains("not"), "{t}");
        assert!(note.contains("+500"), "{note}");
        // son ek + klasör + yeniden örnekleme
        let job = Job { suffix: "_1080".into(), out_dir: Some(dir.join("cikti")), resample: Some((1920, 1080)), ..Default::default() };
        let (out, _) = run_file(&a, &job).unwrap();
        assert_eq!(out, dir.join("cikti").join("b01_1080.ass"));
        assert!(std::fs::read_to_string(&out).unwrap().contains("PlayResX: 1920"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
