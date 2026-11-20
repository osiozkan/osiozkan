//! Gömülü (hardsub) altyazıyı metne çevir: ffmpeg videonun alt bölgesini saniyede birkaç kare,
//! gri ve küçültülmüş olarak boruya akıtır; `osisub_core::hardsub::Segmenter` satır sınırlarını
//! bulur, satır başına bir görüntü Tesseract ile okunur (`ocr::ocr_pgms`).
use osisub_core::hardsub::{merge_texts, Options, Segmenter};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct HardsubOptions {
    /// Taranan bölge: videonun yüksekliğine oranla üst ve alt sınır (0 = üst, 1 = alt)
    pub top: f64,
    pub bottom: f64,
    /// Saniyedeki örnek kare (zaman çözünürlüğü = 1000/fps ms)
    pub fps: f64,
    /// Örnek genişliği (piksel; orantılı küçültülür)
    pub width: u32,
    /// Tesseract dili ("eng", "tur", "eng+jpn"…)
    pub lang: String,
    pub detect: Options,
    /// Ayrıca üst %30'u da tara; oradan gelen satırlar `{\an8}` alır (tabela çevirileri/notlar)
    pub also_top: bool,
}

impl Default for HardsubOptions {
    fn default() -> Self {
        HardsubOptions { top: 0.70, bottom: 1.0, fps: 5.0, width: 1280, lang: "eng".into(), detect: Options::default(), also_top: false }
    }
}

fn even(v: f64) -> u32 {
    ((v / 2.0).round() as u32 * 2).max(2)
}

/// `extract` + `also_top` ise ikinci geçişte üst bölge (`{\an8}` satırları); sonuç zamana göre sıralı.
pub fn extract_all(video: &Path, o: &HardsubOptions, progress: &dyn Fn(f32), cancel: &AtomicBool) -> Result<osisub_core::Script, String> {
    // bölge zaten üstse ikinci geçiş aynı alanı tarar (yinelenen satırlar)
    if !o.also_top || o.top < 0.3 {
        return extract(video, o, progress, cancel);
    }
    let mut s = extract(video, o, &|f| progress(f * 0.5), cancel)?;
    let top = HardsubOptions { top: 0.0, bottom: 0.30, also_top: false, ..o.clone() };
    let t = extract(video, &top, &|f| progress(0.5 + f * 0.5), cancel)?;
    let mut ds: Vec<osisub_core::ass::Dialogue> = s.dialogues().cloned().collect();
    ds.extend(t.dialogues().cloned().map(|mut d| { d.text = format!("{{\\an8}}{}", d.text); d }));
    ds.sort_by_key(|d| d.start_time);
    s = osisub_core::Script::from_dialogues(ds);
    Ok(s)
}

/// Videodaki gömülü altyazıyı satırlara çevirir. `progress(0..1)` aşama ilerlemesi; `cancel`
/// ayarlanırsa durur. Dönüş: zamanlanmış ve OCR'lanmış betik.
pub fn extract(video: &Path, o: &HardsubOptions, progress: &dyn Fn(f32), cancel: &AtomicBool) -> Result<osisub_core::Script, String> {
    if !(o.fps.is_finite() && o.fps > 0.0) {
        return Err("Örnekleme hızı (fps) 0'dan büyük olmalı".into());
    }
    let info = crate::probe(video)?;
    if !info.has_video || info.width == 0 || info.height == 0 {
        return Err("Videoda görüntü akışı yok".into());
    }
    let (vw, vh) = (info.width as f64, info.height as f64);
    let (top, bottom) = (o.top.clamp(0.0, 0.95), o.bottom.clamp(0.05, 1.0));
    if bottom <= top {
        return Err("Bölge geçersiz (alt sınır üstten büyük olmalı)".into());
    }
    let ch = even(vh * (bottom - top)).min(info.height);
    let cy = ((vh * top).round() as u32).min(info.height - ch);
    let ow = even((o.width as f64).min(vw));
    let oh = even(ch as f64 * ow as f64 / vw);
    let vf = format!("fps={},crop={}:{}:0:{},scale={}:{}:flags=area,format=gray", o.fps, info.width, ch, cy, ow, oh);
    // -noautorotate: crop/scale boyutları ffprobe'un saklanan boyutuyla aynı kalsın (dönmüş videoda
    // ffmpeg aksi halde kareyi döndürür ve kırpma "too big" hatasıyla sessizce boş sonuç verir).
    // stderr geçici dosyaya: hata iletisi raporlanır, boru tıkanması olmaz.
    let errf = std::env::temp_dir().join(format!("osisub-hardsub-{}-{}.log", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    let err_out = std::fs::File::create(&errf).map(Stdio::from).unwrap_or(Stdio::null());
    let mut child = Command::new(crate::ffmpeg_bin())
        .args(["-v", "error", "-nostdin", "-noautorotate", "-i"])
        .arg(video)
        .args(["-an", "-sn", "-vf", &vf, "-f", "rawvideo", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(err_out)
        .spawn()
        .map_err(|e| format!("ffmpeg çalıştırılamadı: {e}"))?;
    let mut out = child.stdout.take().ok_or("ffmpeg çıktısı yok")?;
    let (w, h) = (ow as usize, oh as usize);
    let mut seg = Segmenter::new(w, h, o.detect.clone());
    let mut buf = vec![0u8; w * h];
    let mut k: i64 = 0;
    let step = 1000.0 / o.fps;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("İptal edildi".into());
        }
        match out.read_exact(&mut buf) {
            Ok(()) => {}
            Err(_) => break,
        }
        let t = (k as f64 * step).round() as i64;
        seg.push(t, &buf);
        k += 1;
        if k % 25 == 0 && info.duration_ms > 0 {
            progress((t as f32 / info.duration_ms as f32).min(1.0) * 0.8);
        }
    }
    let _ = child.wait();
    let err_text = std::fs::read_to_string(&errf).unwrap_or_default();
    let _ = std::fs::remove_file(&errf);
    // hiç kare yoksa ffmpeg hatasını bildir (boş "başarılı" sonuç yerine)
    if k == 0 {
        return Err(format!("ffmpeg kare üretmedi: {}", err_text.lines().last().unwrap_or("bilinmeyen hata")));
    }
    let end = if info.duration_ms > 0 { info.duration_ms } else { (k as f64 * step) as i64 };
    let found = seg.finish(end);
    progress(0.85);
    let pgms: Vec<Vec<u8>> = found.iter().map(|f| f.pgm.clone()).collect();
    // teşhis: OSISUB_HARDSUB_DEBUG=<klasör> OCR görüntülerini yazar
    if let Ok(d) = std::env::var("OSISUB_HARDSUB_DEBUG") {
        for (i, p) in pgms.iter().enumerate() {
            let _ = std::fs::write(Path::new(&d).join(format!("{i:04}.pgm")), p);
        }
    }
    let texts = crate::ocr::ocr_pgms(&pgms, &o.lang)?;
    progress(1.0);
    let lines = merge_texts(found.iter().zip(texts).map(|(f, t)| (f.start, f.end, osisub_core::hardsub::tidy(&t))).collect(), step.ceil() as i64 + 1);
    let ds: Vec<osisub_core::ass::Dialogue> = lines.into_iter().map(|(s, e, t)| osisub_core::ass::Dialogue { start_time: s, end_time: e, text: t, ..Default::default() }).collect();
    Ok(osisub_core::Script::from_dialogues(ds))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gerçek uçtan uca: libass ile altyazı yakılmış video → algıla → Tesseract
    /// (ffmpeg ya da tesseract yoksa atlanır).
    #[test]
    fn burned_subtitles_are_recovered() {
        if !crate::ffmpeg_available() || !crate::ocr::tesseract_available() {
            eprintln!("ffmpeg/tesseract yok, atlandı");
            return;
        }
        let dir = std::env::temp_dir().join(format!("osisub-hardsub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ass = dir.join("s.ass");
        std::fs::write(&ass, "[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,3,0,2,20,20,40,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Where are you going tonight?\nDialogue: 0,0:00:03.00,0:00:05.50,Default,,0,0,0,,I will find the hidden door.\nDialogue: 0,0:00:07.00,0:00:09.00,Default,,0,0,0,,Thank you very much\n").unwrap();
        let vid = dir.join("v.mp4");
        let st = Command::new(crate::ffmpeg_bin())
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=24:duration=10", "-vf"])
            .arg(format!("subtitles={}", ass.display()))
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
            .arg(&vid)
            .status()
            .unwrap();
        assert!(st.success());
        let s = extract(&vid, &HardsubOptions::default(), &|_| {}, &AtomicBool::new(false)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let d: Vec<_> = s.dialogues().map(|d| (d.start_time, d.end_time, d.text.clone())).collect();
        eprintln!("{d:?}");
        assert_eq!(d.len(), 3, "{d:?}");
        let want = [(1000, 3000, "Where are you going tonight?"), (3000, 5500, "I will find the hidden door."), (7000, 9000, "Thank you very much")];
        for ((s, e, t), (ws, we, wt)) in d.iter().zip(want) {
            assert!((s - ws).abs() <= 250 && (e - we).abs() <= 250, "{s}-{e} ≠ {ws}-{we}");
            let norm = |x: &str| x.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
            // karakter benzerliği (OCR kusursuz değil; zor zeminde tek harf kayabilir)
            let (a, b): (Vec<char>, Vec<char>) = (norm(t).chars().collect(), norm(wt).chars().collect());
            let mut prev: Vec<usize> = (0..=b.len()).collect();
            for (i, ca) in a.iter().enumerate() {
                let mut cur = vec![i + 1; b.len() + 1];
                for (j, cb) in b.iter().enumerate() { cur[j + 1] = (prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1); }
                prev = cur;
            }
            let sim = 1.0 - prev[b.len()] as f64 / b.len() as f64;
            assert!(sim >= 0.9, "OCR: {t} (benzerlik {sim:.2})");
        }
    }
}
