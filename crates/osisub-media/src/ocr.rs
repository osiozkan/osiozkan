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

/// Resim altyazı dosyası ya da video → görüntüler. Desteklenen: PGS `.sup`, VobSub `.idx`/`.sub`
/// (aynı adlı eşi varsa birlikte okunur), videoda PGS/VobSub izi (`track`: ffmpeg akış dizini;
/// None = ilk resim izi; `.idx` için idx iz numarası).
pub fn load_bitmap_subs(path: &std::path::Path, track: Option<u32>) -> Result<Vec<osisub_core::pgs::PgsSub>, String> {
    use osisub_core::vobsub;
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if ext == "idx" || ext == "sub" {
        let idx_p = path.with_extension("idx");
        let sub_p = path.with_extension("sub");
        let sub = std::fs::read(&sub_p).map_err(|e| format!("{}: {}", sub_p.display(), e))?;
        if sub.starts_with(&[0, 0, 1, 0xBA]) || sub.starts_with(&[0, 0, 1, 0xBD]) {
            let idx = std::fs::read_to_string(&idx_p).map(|t| vobsub::parse_idx(&t)).unwrap_or_default();
            return Ok(vobsub::parse(&sub, &idx, track.map(|t| t as u8)));
        }
        if ext == "idx" {
            return Err(format!("{}: VobSub (MPEG-PS) değil", sub_p.display()));
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    if bytes.starts_with(b"PG") {
        return osisub_core::pgs::parse(&bytes);
    }
    let tracks = crate::mux::subtitle_tracks(path)?;
    let t = match track {
        Some(n) => tracks.iter().find(|t| t.index == n),
        None => tracks.iter().find(|t| crate::mux::is_bitmap_sub(&t.codec)),
    }
    .ok_or("PGS/VobSub altyazı izi bulunamadı")?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("osisub-{}-{}-{}.{}", std::process::id(), nanos, t.index, if t.codec == "dvd_subtitle" { "sub" } else { "sup" }));
    let r = if t.codec == "dvd_subtitle" {
        crate::mux::extract_vobsub(path, t, &tmp).and_then(|idx_text| {
            let b = std::fs::read(&tmp).map_err(|e| e.to_string())?;
            Ok(vobsub::parse(&b, &vobsub::parse_idx(&idx_text), None))
        })
    } else {
        crate::mux::extract_pgs(path, t, &tmp).and_then(|_| osisub_core::pgs::parse(&std::fs::read(&tmp).map_err(|e| e.to_string())?))
    };
    let _ = std::fs::remove_file(&tmp);
    r
}

/// PGS/VobSub dosyası ya da video → OCR'lanmış betik (bkz. `load_bitmap_subs`).
/// `lang`: Tesseract dili (ör. "eng", "eng+jpn").
pub fn ocr_file(path: &std::path::Path, track: Option<u32>, lang: &str) -> Result<osisub_core::Script, String> {
    let subs = load_bitmap_subs(path, track)?;
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

    /// DVD VobSub: libass ile çizilen metin → 4 renkli SPU → .idx/.sub ve MKV izi → OCR.
    #[test]
    fn rendered_text_roundtrips_through_vobsub_ocr() {
        if !super::tesseract_available() || !crate::ffmpeg_available() {
            eprintln!("tesseract/ffmpeg yok: atlandı");
            return;
        }
        use osisub_core::vobsub;
        let dir = std::env::temp_dir().join(format!("osisub-vobsub-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ass = dir.join("t.ass");
        std::fs::write(&ass, "[Script Info]\nPlayResX: 720\nPlayResY: 480\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,34,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,30,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:05.00,Default,,0,0,0,,The ship leaves at dawn.\n").unwrap();
        let out = std::process::Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-f", "lavfi", "-i", "color=0x404040:s=720x480", "-vf"])
            .arg(format!("ass={},format=gray", ass.display()))
            .args(["-frames:v", "1", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert_eq!(out.stdout.len(), 720 * 480);
        // 4 renge indir: zemin (gri) → 0, parlak dolgu → 1, koyu kenar → 2, ara ton → 3
        let all: Vec<u8> = out.stdout.iter().map(|&l| if l > 200 { 1 } else if l < 40 { 2 } else if l == 0x40 { 0 } else { 3 }).collect();
        let rows: Vec<usize> = (0..480).filter(|y| all[y * 720..y * 720 + 720].iter().any(|&c| c != 0)).collect();
        let (y0, y1) = (rows[0], *rows.last().unwrap() + 1);
        let px = all[y0 * 720..y1 * 720].to_vec();
        let spu = vobsub::encode_spu(0, y0 as u32, 720, (y1 - y0) as u32, &px, [0, 1, 2, 3], [0, 15, 15, 8], 2000);
        std::fs::write(dir.join("v.sub"), vobsub::mux(&[(0, 1500, spu)])).unwrap();
        let idx = "# VobSub index file, v7\nsize: 720x480\npalette: 000000, f0f0f0, 101010, 808080, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0\nid: en, index: 0\ntimestamp: 00:00:01:500, filepos: 000000000\n";
        std::fs::write(dir.join("v.idx"), idx).unwrap();
        let sc = super::ocr_file(&dir.join("v.idx"), None, "eng").unwrap();
        let d = sc.dialogues().next().unwrap();
        assert_eq!(d.text, "The ship leaves at dawn.");
        assert_eq!(d.start_time, 1500);
        assert!((d.end_time - 3500).abs() < 15);
        assert_eq!(sc.info("PlayResY"), Some("480"));
        // MKV içindeki DVD izi: ffmpeg ile çıkarılır, palet CodecPrivate'tan
        let mkv = dir.join("v.mkv");
        let st = std::process::Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=black:s=720x480:d=5:r=10", "-i"])
            .arg(dir.join("v.idx"))
            .args(["-map", "0", "-map", "1", "-c:v", "mpeg4", "-c:s", "copy"])
            .arg(&mkv)
            .status()
            .unwrap();
        assert!(st.success());
        let sc = super::ocr_file(&mkv, None, "eng").unwrap();
        let d = sc.dialogues().next().unwrap();
        assert_eq!(d.text, "The ship leaves at dawn.");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
