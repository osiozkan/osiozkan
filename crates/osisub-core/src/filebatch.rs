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

/// Toplu iş başlamadan çakışmaları bulur: bir dosyanın çıktısı BAŞKA bir girdinin üzerine yazacaksa
/// (ör. `ep1.srt` → `ep1.ass` varken) ya da iki girdi aynı çıktıyı üretiyorsa o girdi için hata.
/// Üzerine yazma izni yalnızca dosyanın kendi girdisini kapsar; başka girdi asla ezilmez.
pub fn conflicts(files: &[PathBuf], job: &Job) -> Vec<Option<String>> {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let inputs: Vec<PathBuf> = files.iter().map(|f| canon(f)).collect();
    let mut seen: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();
    files.iter().enumerate().map(|(i, f)| {
        let out = out_path(f, job);
        let oc = canon(&out);
        if let Some(j) = inputs.iter().position(|x| *x == oc) {
            if j != i {
                return Some(format!("çıktı {} başka bir girdi dosyasının üzerine yazacak", out.display()));
            }
        }
        if let Some(&k) = seen.get(&oc) {
            return Some(format!("çıktı {} aynı zamanda {} dosyasının çıktısı", out.display(), files[k].display()));
        }
        seen.insert(oc, i);
        None
    }).collect()
}

/// Tek dosyayı işler; çıktı yolunu ve raporu döndürür.
pub fn run_file(input: &Path, job: &Job) -> Result<(PathBuf, String), String> {
    let out = out_path(input, job);
    if !job.overwrite && out.exists() {
        return Err(if same_file(&out, input) {
            "çıktı girdinin üzerine yazacak (son ek verin ya da üzerine yazmaya izin verin)".into()
        } else {
            format!("{} zaten var (üzerine yazmaya izin verin ya da son ek verin)", out.display())
        });
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
        // ep1.srt → ep1.ass varken: başka girdi ezilmez
        let b = a.with_extension("srt");
        std::fs::write(&b, "1\n00:00:01,000 --> 00:00:02,000\nx\n").unwrap();
        let j = Job { out_ext: "ass".into(), overwrite: true, ..Job::default() };
        let c = conflicts(&[a.clone(), b.clone()], &j);
        assert!(c[0].is_none() && c[1].is_some(), "{:?}", c);
        // var olan başka dosyanın üzerine izinsiz yazılmaz
        let job = Job { out_ext: "srt".into(), ..Default::default() };
        assert!(run_file(&a, &job).unwrap_err().contains("zaten var"));
        std::fs::remove_file(&b).unwrap();
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

/// Toplu kalite raporu için tek dosyanın denetimi (varsayılan kurallar + çevrilmemiş satırlar +
/// betik başlığı): (dosya adı, betik, başlık sorunları, satır sorunları).
pub fn qc_file(input: &Path) -> Result<(String, Script, Vec<String>, Vec<crate::qc::Issue>), String> {
    let s = crate::io::load(input)?;
    let mut issues = crate::qc::check(&s, &crate::qc::Rules::default());
    issues.extend(crate::untranslated::check(&s, None));
    issues.sort_by_key(|i| i.entry);
    let head = crate::qc::script_issues_dar(&s, None, None);
    let name = input.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok((name, s, head, issues))
}

#[cfg(test)]
mod qc_report_tests {
    #[test]
    fn multi_file_report_has_summary_and_sections() {
        let dir = std::env::temp_dir().join(format!("osisub-fbqc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("01.srt");
        let b = dir.join("02.srt");
        std::fs::write(&a, "1\n00:00:01,000 --> 00:00:01,100\nÇok kısa <satır>\n\n").unwrap();
        std::fs::write(&b, "1\n00:00:01,000 --> 00:00:03,000\nTemiz satır.\n\n").unwrap();
        let files: Vec<_> = [&a, &b].iter().map(|p| super::qc_file(p).unwrap()).collect();
        assert!(!files[0].3.is_empty());
        let h = crate::qc::report_html_multi("Sezon 1", &files);
        assert!(h.contains("Toplu kalite raporu — Sezon 1"));
        assert!(h.contains("<a href=\"#f0\">01.srt</a>") && h.contains("<hr id=\"f1\">"));
        assert!(h.contains("&lt;satır&gt;") || h.contains("Çok kısa"));
        assert_eq!(h.matches("<html").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
