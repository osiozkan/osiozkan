//! Resim altyazıları için OCR: Tesseract komut satırı aracı (ffmpeg gibi harici; kurulu değilse hata).
//! Görüntüler tek süreçte toplu işlenir (liste dosyası); sayfalar çıktıda `\f` ile ayrılır.
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn tesseract_bin() -> String {
    super::tool_bin("tesseract", "OSISUB_TESSERACT")
}

pub fn tesseract_available() -> bool {
    Command::new(tesseract_bin()).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Kurulu dil verileri (ör. "eng", "tur", "jpn").
pub fn languages() -> Vec<String> {
    let Ok(o) = Command::new(tesseract_bin()).arg("--list-langs").stderr(Stdio::null()).output() else { return vec![] };
    String::from_utf8_lossy(&o.stdout).lines().skip(1).map(|l| l.trim().to_string()).filter(|l| !l.is_empty() && l != "osd").collect()
}

/// OCR çıktısındaki yaygın hataları düzeltir ve satırları `\N` ile birleştirir.
pub fn clean_text(s: &str) -> String {
    let lines: Vec<String> = s
        .lines()
        .map(|l| {
            let l = l.trim().replace('|', "I").replace("’", "'").replace("‘", "'").replace("“", "\"").replace("”", "\"");
            // satır başındaki tek "-" diyalog çizgisi: "-Merhaba" → "- Merhaba"
            match l.strip_prefix('-') {
                Some(r) if !r.starts_with(' ') && !r.is_empty() => format!("- {}", r),
                _ => l,
            }
        })
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\\N")
}

/// PGM (P5) görüntülerini OCR'lar; her görüntü için temizlenmiş metin.
pub fn ocr_pgms(images: &[Vec<u8>], lang: &str) -> Result<Vec<String>, String> {
    if images.is_empty() {
        return Ok(vec![]);
    }
    let dir: PathBuf = std::env::temp_dir().join(format!("osisub-ocr-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut list = String::new();
    for (i, img) in images.iter().enumerate() {
        let p = dir.join(format!("{:06}.pgm", i));
        std::fs::write(&p, img).map_err(|e| e.to_string())?;
        list.push_str(&p.to_string_lossy());
        list.push('\n');
    }
    let lf = dir.join("list.txt");
    std::fs::write(&lf, list).map_err(|e| e.to_string())?;
    let out = Command::new(tesseract_bin())
        .arg(&lf)
        .arg("stdout")
        .args(["-l", if lang.is_empty() { "eng" } else { lang }, "--psm", "6"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let _ = std::fs::remove_dir_all(&dir);
    let out = out.map_err(|e| format!("tesseract çalıştırılamadı ({}): {}", tesseract_bin(), e))?;
    if !out.status.success() {
        return Err(format!("tesseract hatası: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut pages: Vec<String> = text.split('\x0c').map(clean_text).collect();
    pages.resize(images.len(), String::new());
    Ok(pages)
}

/// PGS dosyası (`.sup`) ya da video (ilk/verilen PGS izi) → OCR'lanmış betik.
/// `track`: ffmpeg akış dizini (videoda; None = ilk PGS izi). `lang`: Tesseract dili (ör. "eng", "eng+jpn").
pub fn ocr_file(path: &std::path::Path, track: Option<u32>, lang: &str) -> Result<osisub_core::Script, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let sup = if bytes.starts_with(b"PG") {
        bytes
    } else {
        let tracks = crate::mux::subtitle_tracks(path)?;
        let t = match track {
            Some(n) => tracks.iter().find(|t| t.index == n),
            None => tracks.iter().find(|t| t.codec == "hdmv_pgs_subtitle"),
        }
        .ok_or("PGS altyazı izi bulunamadı")?;
        let tmp = std::env::temp_dir().join(format!("osisub-{}-{}.sup", std::process::id(), t.index));
        crate::mux::extract_pgs(path, t, &tmp)?;
        let b = std::fs::read(&tmp).map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&tmp);
        b?
    };
    let subs = osisub_core::pgs::parse(&sup)?;
    let pgms: Vec<Vec<u8>> = subs
        .iter()
        .map(|s| {
            let (w, h, px) = osisub_core::pgs::ocr_gray(s, 12);
            osisub_core::pgs::to_pgm(w, h, &px)
        })
        .collect();
    let texts = ocr_pgms(&pgms, lang)?;
    Ok(osisub_core::pgs::to_script(&subs, &texts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_common_ocr_noise() {
        assert_eq!(clean_text("  |'m here.\n\n-Wait!\n"), "I'm here.\\N- Wait!");
        assert_eq!(clean_text("“Quote”"), "\"Quote\"");
    }
}

/// Gerçek Tesseract ile uçtan uca: libass ile çizilen metin → PGS → OCR (tesseract yoksa atlanır).
#[cfg(test)]
mod e2e {
    #[test]
    fn rendered_text_roundtrips_through_pgs_ocr() {
        if !super::tesseract_available() || !crate::ffmpeg_available() {
            eprintln!("tesseract/ffmpeg yok: atlandı");
            return;
        }
        let dir = std::env::temp_dir().join(format!("osisub-ocr-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ass = dir.join("t.ass");
        std::fs::write(&ass, "[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,3,0,2,10,10,40,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,Where are you going?\\NI will be right back.\n").unwrap();
        let out = std::process::Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-f", "lavfi", "-i", "color=black:s=1280x720", "-vf"])
            .arg(format!("ass={},format=gray", ass.display()))
            .args(["-frames:v", "1", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(out.stdout.len(), 1280 * 720);
        // Siyah zemin = saydam; libass'in kenarı koyu, dolgu parlak
        let la: Vec<(u8, u8)> = out.stdout.iter().map(|&l| (l, if l > 8 { 255 } else { 0 })).collect();
        let sup = osisub_core::pgs::encode(&[(1000, 3000, 0, 0, 1280, 720, la)], (1280, 720));
        if let Ok(p) = std::env::var("OSISUB_DUMP_SUP") {
            std::fs::write(p, &sup).unwrap();
        }
        let subs = osisub_core::pgs::parse(&sup).unwrap();
        let (w, h, px) = osisub_core::pgs::ocr_gray(&subs[0], 12);
        let texts = super::ocr_pgms(&[osisub_core::pgs::to_pgm(w, h, &px)], "eng").unwrap();
        assert_eq!(texts[0], "Where are you going?\\NI will be right back.");
        let sc = osisub_core::pgs::to_script(&subs, &texts);
        let d = sc.dialogues().next().unwrap();
        assert_eq!((d.start_time, d.end_time), (1000, 3000));
        assert_eq!(sc.info("PlayResX"), Some("1280"));
    }
}
