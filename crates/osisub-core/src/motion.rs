//! Hareket izleme verisi uygulama (Aegisub-Motion'ın temel işlevi): Mocha / After Effects
//! "Keyframe Data" metnindeki konum/ölçek/döndürme izini seçili satırlara kare kare uygular.
//! Her satır, göründüğü her kare için bir satıra bölünür; `\pos`, `\fscx/\fscy`, `\frz`
//! (isteğe bağlı `\bord/\shad/\blur`) o karenin izine göre yeniden hesaplanır. Satır içi
//! `\t`/`\move` zamanları ve `\fad` yeni başlangıca göre kaydırılır (görünüm değişmez).
use crate::ass::{Dialogue, Script};
use crate::overrides::{self, Seg, Tag};
use crate::vfr::Framerate;

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub x: f64,
    pub y: f64,
    /// yüzde
    pub sx: f64,
    pub sy: f64,
    /// derece (AE: saat yönü pozitif)
    pub rot: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MotionData {
    pub fps: f64,
    pub width: f64,
    pub height: f64,
    /// kare 0'dan başlayarak (ardışık; eksik kareler önceki değerle doldurulur)
    pub frames: Vec<Track>,
}

#[derive(Clone, Debug)]
pub struct MotionOptions {
    pub position: bool,
    pub scale: bool,
    pub rotation: bool,
    /// ölçekle birlikte kenar/gölge/bulanıklık da ölçeklensin
    pub scale_border: bool,
    /// izin 0. karesinin denk geldiği video karesi (Aegisub-Motion: seçimin ilk karesi)
    pub start_frame: i64,
    /// `\clip`/`\iclip`'i de izle (dikdörtgen; dönüş varsa vektöre çevrilir)
    pub clip: bool,
}

impl Default for MotionOptions {
    fn default() -> Self {
        MotionOptions { position: true, scale: true, rotation: true, scale_border: true, start_frame: 0, clip: true }
    }
}

fn section_rows(lines: &[&str], name: &str) -> Vec<Vec<f64>> {
    let Some(i) = lines.iter().position(|l| l.trim().eq_ignore_ascii_case(name)) else { return vec![] };
    let mut out = vec![];
    for l in &lines[i + 1..] {
        let t = l.trim();
        if t.is_empty() {
            if out.is_empty() {
                continue;
            }
            break;
        }
        if t.starts_with("Frame") {
            continue;
        }
        let v: Vec<f64> = t.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if v.is_empty() {
            break;
        }
        out.push(v);
    }
    out
}

/// "Adobe After Effects 6.0 Keyframe Data" metnini okur (Mocha'nın "AE Transform Data" dışa aktarımı).
pub fn parse_ae(text: &str) -> Result<MotionData, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    if !lines.iter().any(|l| l.contains("Keyframe Data")) {
        return Err("After Effects Keyframe Data bulunamadı (Mocha → Export Tracking Data → After Effects Transform Data)".into());
    }
    let header = |key: &str| -> Option<f64> {
        lines.iter().find_map(|l| {
            let t = l.trim();
            t.strip_prefix(key).and_then(|r| r.trim().parse().ok())
        })
    };
    let fps = header("Units Per Second").unwrap_or(23.976);
    let width = header("Source Width").ok_or("Source Width yok")?;
    let height = header("Source Height").ok_or("Source Height yok")?;
    let pos = section_rows(&lines, "Position");
    let scale = section_rows(&lines, "Scale");
    let rot = section_rows(&lines, "Rotation");
    if pos.is_empty() {
        return Err("Position verisi yok".into());
    }
    let max_frame = pos.iter().chain(scale.iter()).chain(rot.iter()).filter_map(|r| r.first()).fold(0.0f64, |m, &f| m.max(f)) as usize;
    if max_frame > 1_000_000 {
        return Err("kare sayısı çok büyük".into());
    }
    let mut frames = vec![Track { x: f64::NAN, y: f64::NAN, sx: f64::NAN, sy: f64::NAN, rot: f64::NAN }; max_frame + 1];
    for r in &pos {
        if let (Some(&f), Some(&x), Some(&y)) = (r.first(), r.get(1), r.get(2)) {
            frames[f as usize].x = x;
            frames[f as usize].y = y;
        }
    }
    for r in &scale {
        if let (Some(&f), Some(&x), Some(&y)) = (r.first(), r.get(1), r.get(2)) {
            frames[f as usize].sx = x;
            frames[f as usize].sy = y;
        }
    }
    for r in &rot {
        if let (Some(&f), Some(&d)) = (r.first(), r.get(1)) {
            frames[f as usize].rot = d;
        }
    }
    // eksik değerleri önceki (yoksa sonraki / varsayılan) değerle doldur
    let mut last = Track { x: f64::NAN, y: f64::NAN, sx: 100.0, sy: 100.0, rot: 0.0 };
    let first_pos = frames.iter().find(|t| !t.x.is_nan()).map(|t| (t.x, t.y)).unwrap_or((0.0, 0.0));
    last.x = first_pos.0;
    last.y = first_pos.1;
    for t in frames.iter_mut() {
        if t.x.is_nan() { t.x = last.x; t.y = last.y; }
        if t.sx.is_nan() { t.sx = last.sx; t.sy = last.sy; }
        if t.rot.is_nan() { t.rot = last.rot; }
        last = t.clone();
    }
    Ok(MotionData { fps, width, height, frames })
}

/// Satırın (ilk) konumu: `\pos`, yoksa `\move`'un başlangıcı, yoksa hizalama + kenar boşluklarından.
pub fn line_position(script: &Script, d: &Dialogue) -> (f64, f64) {
    let (rx, ry) = script.res();
    let (rx, ry) = (rx as f64, ry as f64);
    for name in ["pos", "move"] {
        if let Some(t) = overrides::get_initial(&d.text, name) {
            let a = t.args();
            if let (Some(x), Some(y)) = (a.first().and_then(|v| v.trim().parse().ok()), a.get(1).and_then(|v| v.trim().parse().ok())) {
                return (x, y);
            }
        }
    }
    let st = script.style_by_name(&d.style).cloned().unwrap_or_default();
    let an = overrides::get_initial(&d.text, "an").and_then(|t| t.number()).map(|n| n as i32).unwrap_or(st.align).clamp(1, 9);
    let ml = if d.margin_l > 0 { d.margin_l } else { st.margin_l } as f64;
    let mr = if d.margin_r > 0 { d.margin_r } else { st.margin_r } as f64;
    let mv = if d.margin_t > 0 { d.margin_t } else { st.margin_t } as f64;
    let x = match (an - 1) % 3 { 0 => ml, 1 => (rx - mr + ml) / 2.0, _ => rx - mr };
    let y = match (an - 1) / 3 { 0 => ry - mv, 1 => ry / 2.0, _ => mv };
    (x, y)
}

fn r2(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Etiketin sayısal değerini (yoksa stil değerini) okur.
fn initial_num(text: &str, name: &str, default: f64) -> f64 {
    overrides::get_initial(text, name).and_then(|t| t.number()).unwrap_or(default)
}

/// Satır içindeki zamanlı etiketleri `off` ms kaydırır (`\t`, `\move`), `\fad`'ı `\fade`'e çevirir.
fn shift_timed_tags(text: &str, off: i64, dur: i64) -> String {
    if off == 0 {
        return text.to_string();
    }
    let mut segs = overrides::parse(text);
    for s in segs.iter_mut() {
        let Seg::Block(tags) = s else { continue };
        for t in tags.iter_mut() {
            match t.name.as_str() {
                "t" => {
                    let a = t.args();
                    let nums: Vec<Option<f64>> = a[..a.len().saturating_sub(1)].iter().map(|x| x.trim().parse().ok()).collect();
                    let last = a.last().cloned().unwrap_or_default();
                    match nums.as_slice() {
                        [Some(t1), Some(t2)] => t.arg = format!("({},{},{})", *t1 as i64 - off, *t2 as i64 - off, last),
                        [Some(t1), Some(t2), Some(acc)] => t.arg = format!("({},{},{},{})", *t1 as i64 - off, *t2 as i64 - off, r2(*acc), last),
                        // zamansız \t tüm satır boyunca: kaydırılmış açık aralık
                        [] => t.arg = format!("({},{},{})", -off, dur - off, last),
                        [Some(acc)] => t.arg = format!("({},{},{},{})", -off, dur - off, r2(*acc), last),
                        _ => {}
                    }
                }
                "fad" => {
                    let a: Vec<f64> = t.args().iter().filter_map(|x| x.trim().parse().ok()).collect();
                    if a.len() == 2 {
                        let (fi, fo) = (a[0] as i64, a[1] as i64);
                        *t = Tag::new("fade", &format!("(255,0,255,{},{},{},{})", -off, fi - off, dur - fo - off, dur - off));
                    }
                }
                _ => {}
            }
        }
    }
    overrides::to_string(&segs)
}

/// Kırpmayı izle: her nokta p → T + R·S·(p − T0). Dikdörtgen kırpma dönüş yoksa dikdörtgen kalır.
fn track_clip(text: &str, map: &dyn Fn((f64, f64)) -> (f64, f64), rotated: bool) -> String {
    let mut out = text.to_string();
    for name in ["clip", "iclip"] {
        let Some(t) = overrides::get_initial(&out, name) else { continue };
        let a = t.args();
        let nums: Option<Vec<f64>> = if a.len() == 4 { a.iter().map(|x| x.trim().parse().ok()).collect() } else { None };
        let new_arg = if let Some(v) = nums {
            let (p1, p2) = (map((v[0], v[1])), map((v[2], v[3])));
            if rotated {
                let (q1, q2) = (map((v[2], v[1])), map((v[0], v[3])));
                format!("(m {} {} l {} {} {} {} {} {})", r2(p1.0), r2(p1.1), r2(q1.0), r2(q1.1), r2(p2.0), r2(p2.1), r2(q2.0), r2(q2.1))
            } else {
                format!("({},{},{},{})", r2(p1.0.min(p2.0)), r2(p1.1.min(p2.1)), r2(p1.0.max(p2.0)), r2(p1.1.max(p2.1)))
            }
        } else {
            // vektör: (çizim) ya da (ölçek, çizim); ölçek 2^(s-1) koordinat çarpanı
            let (scale, draw) = match a.len() {
                1 => (1.0, a[0].clone()),
                2 => (a[0].trim().parse::<f64>().map(|s| 2f64.powf(s - 1.0)).unwrap_or(1.0), a[1].clone()),
                _ => continue,
            };
            let mut cmds = crate::drawing::parse(&draw);
            for c in cmds.iter_mut() {
                for p in c.pts.iter_mut() {
                    let q = map((p.0 / scale, p.1 / scale));
                    *p = (q.0, q.1);
                }
            }
            format!("({})", crate::drawing::to_string(&cmds))
        };
        out = overrides::set_initial(&out, name, &new_arg);
    }
    out
}

/// İzi `lines` satırlarına uygular; her satır göründüğü kare sayısı kadar satıra bölünür.
/// Satırlar kendi içlerinde sıralı döner. Hata: iz satırın karelerini kapsamıyorsa.
pub fn apply(script: &Script, lines: &[Dialogue], data: &MotionData, fr: &Framerate, opts: &MotionOptions) -> Result<Vec<Dialogue>, String> {
    let (rx, ry) = script.res();
    let (kx, ky) = (rx as f64 / data.width.max(1.0), ry as f64 / data.height.max(1.0));
    let n_frames = data.frames.len() as i64;
    let mut out = vec![];
    for d in lines {
        let f0 = fr.frame_at_start(d.start_time);
        let f1 = fr.frame_at_end(d.end_time);
        if f1 < f0 {
            continue;
        }
        let (i0, i1) = (f0 - opts.start_frame, f1 - opts.start_frame);
        if i0 < 0 || i1 >= n_frames {
            return Err(format!(
                "İz {} kare (video karesi {}–{}), satır {}–{}. kareler arasında: izi seçimin ilk karesinden başlatın",
                n_frames,
                opts.start_frame,
                opts.start_frame + n_frames - 1,
                f0,
                f1
            ));
        }
        let st = script.style_by_name(&d.style).cloned().unwrap_or_default();
        let (ox, oy) = line_position(script, d);
        let base = &data.frames[0];
        let (bx, by) = (base.x * kx, base.y * ky);
        let fscx0 = initial_num(&d.text, "fscx", st.scale_x);
        let fscy0 = initial_num(&d.text, "fscy", st.scale_y);
        let frz0 = initial_num(&d.text, "frz", initial_num(&d.text, "fr", st.angle));
        let bord0 = initial_num(&d.text, "bord", st.outline);
        let shad0 = initial_num(&d.text, "shad", st.shadow);
        let blur0 = overrides::get_initial(&d.text, "blur").and_then(|t| t.number());
        let dur = d.duration();
        // \move'lu satırlar kare kare \pos'a çevrilir (hareket iz tarafından verilir)
        let text0 = if overrides::get_initial(&d.text, "move").is_some() { overrides::remove_initial(&d.text, "move") } else { d.text.clone() };
        for f in f0..=f1 {
            let t = &data.frames[(f - opts.start_frame) as usize];
            let ratio_x = t.sx / base.sx.max(1e-9);
            let ratio_y = t.sy / base.sy.max(1e-9);
            let drot = t.rot - base.rot;
            let mut text = text0.clone();
            if opts.clip {
                let (sx, sy) = if opts.scale { (ratio_x, ratio_y) } else { (1.0, 1.0) };
                let a = if opts.rotation { drot.to_radians() } else { 0.0 };
                let (c, s) = (a.cos(), a.sin());
                let (tx, ty) = if opts.position { (t.x * kx, t.y * ky) } else { (bx, by) };
                let map = |p: (f64, f64)| {
                    let (dx, dy) = ((p.0 - bx) * sx, (p.1 - by) * sy);
                    (tx + dx * c - dy * s, ty + dx * s + dy * c)
                };
                text = track_clip(&text, &map, a != 0.0);
            }
            if opts.position {
                // başlangıçtaki iz noktasına göre ofset; ölçek ve dönüşle birlikte dönüştürülür
                let (mut dx, mut dy) = (ox - bx, oy - by);
                if opts.scale {
                    dx *= ratio_x;
                    dy *= ratio_y;
                }
                if opts.rotation && drot != 0.0 {
                    let a = drot.to_radians();
                    let (c, s) = (a.cos(), a.sin());
                    (dx, dy) = (dx * c - dy * s, dx * s + dy * c);
                }
                text = overrides::set_initial(&text, "pos", &format!("({},{})", r2(t.x * kx + dx), r2(t.y * ky + dy)));
            }
            if opts.scale && (ratio_x != 1.0 || ratio_y != 1.0 || overrides::get_initial(&text, "fscx").is_some()) {
                text = overrides::set_initial(&text, "fscx", &r2(fscx0 * ratio_x));
                text = overrides::set_initial(&text, "fscy", &r2(fscy0 * ratio_y));
                if opts.scale_border {
                    let k = (ratio_x + ratio_y) / 2.0;
                    if bord0 != 0.0 { text = overrides::set_initial(&text, "bord", &r2(bord0 * k)); }
                    if shad0 != 0.0 { text = overrides::set_initial(&text, "shad", &r2(shad0 * k)); }
                    if let Some(b) = blur0 { text = overrides::set_initial(&text, "blur", &r2(b * k)); }
                }
            }
            if opts.rotation && (drot != 0.0 || overrides::get_initial(&text, "frz").is_some()) {
                // AE saat yönü pozitif, ASS \frz saat yönünün tersi
                text = overrides::remove_initial(&text, "fr");
                text = overrides::set_initial(&text, "frz", &r2(frz0 - drot));
            }
            let start = fr.time_at_start(f).max(d.start_time);
            let end = fr.time_at_end(f).min(d.end_time);
            let mut nd = d.clone();
            nd.text = shift_timed_tags(&text, start - d.start_time, dur);
            nd.start_time = start;
            nd.end_time = end;
            out.push(nd);
        }
    }
    Ok(out)
}

/// Köşe pini izi (Mocha "CC Power Pin" / AE "Corner Pin" dışa aktarımı): her kare için
/// sol üst, sağ üst, sağ alt, sol alt köşeler (kaynak video koordinatı).
#[derive(Clone, Debug, PartialEq)]
pub struct CornerPinData {
    pub fps: f64,
    pub width: f64,
    pub height: f64,
    pub frames: Vec<[(f64, f64); 4]>,
}

/// "Effects  CC Power Pin #1  Top Left #2" ya da "Effects  Corner Pin #1  Upper Left #2" gibi
/// bölümlerin satırları.
fn pin_rows(lines: &[&str], corner: &[&str]) -> Vec<Vec<f64>> {
    let Some(i) = lines.iter().position(|l| {
        let t = l.trim().to_ascii_lowercase();
        t.starts_with("effects") && (t.contains("pin")) && corner.iter().any(|c| t.contains(c))
    }) else {
        return vec![];
    };
    section_rows(&lines[i..], lines[i].trim())
}

/// Köşe pini verisini okur; eksik kareler önceki değerle doldurulur.
pub fn parse_corner_pin(text: &str) -> Result<CornerPinData, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    if !lines.iter().any(|l| l.contains("Keyframe Data")) {
        return Err("After Effects Keyframe Data bulunamadı (Mocha → Export Tracking Data → CC Power Pin / Corner Pin)".into());
    }
    let header = |key: &str| -> Option<f64> { lines.iter().find_map(|l| l.trim().strip_prefix(key).and_then(|r| r.trim().parse().ok())) };
    let fps = header("Units Per Second").unwrap_or(23.976);
    let width = header("Source Width").ok_or("Source Width yok")?;
    let height = header("Source Height").ok_or("Source Height yok")?;
    // ekran sırası: sol üst, sağ üst, sağ alt, sol alt
    let names: [&[&str]; 4] = [&["top left", "upper left"], &["top right", "upper right"], &["bottom right", "lower right"], &["bottom left", "lower left"]];
    let rows: Vec<Vec<Vec<f64>>> = names.iter().map(|n| pin_rows(&lines, n)).collect();
    if rows.iter().any(|r| r.is_empty()) {
        return Err("Dört köşenin (Top Left/Top Right/Bottom Left/Bottom Right) verisi bulunamadı".into());
    }
    let max_frame = rows.iter().flatten().filter_map(|r| r.first()).fold(0.0f64, |m, &f| m.max(f)) as usize;
    if max_frame > 1_000_000 {
        return Err("kare sayısı çok büyük".into());
    }
    let mut frames = vec![[(f64::NAN, f64::NAN); 4]; max_frame + 1];
    for (c, rs) in rows.iter().enumerate() {
        for r in rs {
            if let (Some(&f), Some(&x), Some(&y)) = (r.first(), r.get(1), r.get(2)) {
                if f >= 0.0 {
                    frames[f as usize][c] = (x, y);
                }
            }
        }
    }
    for c in 0..4 {
        let first = frames.iter().map(|q| q[c]).find(|p| !p.0.is_nan()).unwrap_or((0.0, 0.0));
        let mut last = first;
        for q in frames.iter_mut() {
            if q[c].0.is_nan() { q[c] = last; }
            last = q[c];
        }
    }
    Ok(CornerPinData { fps, width, height, frames })
}

/// 4 nokta eşlemesinden izdüşümsel dönüşüm (homografi) matrisi: `src[i]` → `dst[i]`.
pub fn homography(src: &[(f64, f64); 4], dst: &[(f64, f64); 4]) -> Option<[f64; 9]> {
    // 8×8 doğrusal sistem: h33 = 1
    let mut a = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let ((x, y), (u, v)) = (src[i], dst[i]);
        a[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        a[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    for c in 0..8 {
        let p = (c..8).max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap())?;
        if a[p][c].abs() < 1e-12 {
            return None;
        }
        a.swap(c, p);
        for r in 0..8 {
            if r != c {
                let k = a[r][c] / a[c][c];
                for k2 in c..9 {
                    a[r][k2] -= k * a[c][k2];
                }
            }
        }
    }
    let mut h = [0.0; 9];
    for i in 0..8 {
        h[i] = a[i][8] / a[i][i];
    }
    h[8] = 1.0;
    Some(h)
}

pub fn apply_homography(h: &[f64; 9], p: (f64, f64)) -> (f64, f64) {
    let w = h[6] * p.0 + h[7] * p.1 + h[8];
    ((h[0] * p.0 + h[1] * p.1 + h[2]) / w, (h[3] * p.0 + h[4] * p.1 + h[5]) / w)
}

/// Köşe pini izini uygular: her satır kare kare bölünür ve satırın kutusu (`boxes[i]` =
/// genişlik×yükseklik, `\fscx/\fscy` 100 iken) o karedeki dörtgene perspektifle oturtulur
/// (`\an7\pos\org\frx\fry\frz\fscx\fscy\fax`). Satırda `\clip`/`\iclip` varsa ilk kareden o
/// kareye homografiyle taşınır (dikdörtgen kırpma vektöre çevrilir). `dist`: betik koordinatında
/// kamera uzaklığı (`perspective::dist_for`).
pub fn apply_corner_pin(lines: &[Dialogue], boxes: &[(f64, f64)], script: &Script, data: &CornerPinData, fr: &Framerate, start_frame: i64, dist: f64) -> Result<Vec<Dialogue>, String> {
    use crate::perspective;
    let (rx, ry) = script.res();
    let (kx, ky) = (rx as f64 / data.width.max(1.0), ry as f64 / data.height.max(1.0));
    let scaled = |q: &[(f64, f64); 4]| q.map(|p| (p.0 * kx, p.1 * ky));
    let n_frames = data.frames.len() as i64;
    let mut out = vec![];
    for (d, &(w, h)) in lines.iter().zip(boxes) {
        let f0 = fr.frame_at_start(d.start_time);
        let f1 = fr.frame_at_end(d.end_time);
        if f1 < f0 {
            continue;
        }
        if f0 - start_frame < 0 || f1 - start_frame >= n_frames {
            return Err(format!("İz {} kare (video karesi {}–{}), satır {}–{}. kareler arasında", n_frames, start_frame, start_frame + n_frames - 1, f0, f1));
        }
        let q0 = scaled(&data.frames[(f0 - start_frame) as usize]);
        let mut base = d.text.clone();
        for n in ["pos", "move", "org", "frx", "fry", "frz", "fr", "fscx", "fscy", "fax", "fay", "an", "a"] {
            base = overrides::remove_initial(&base, n);
        }
        let dur = d.duration();
        for f in f0..=f1 {
            let q = scaled(&data.frames[(f - start_frame) as usize]);
            let (p, org, err) = perspective::fit(&q, w, h, dist).ok_or(format!("kare {f}: perspektif çözümü bulunamadı"))?;
            if err > 2.0 {
                return Err(format!("kare {f}: dörtgen perspektifle oturtulamadı (hata {err:.1} px)"));
            }
            let mut text = base.clone();
            if let Some(hm) = homography(&q0, &q) {
                text = track_clip(&text, &|pt| apply_homography(&hm, pt), true);
            }
            for t in overrides::parse_block(&perspective::tags(&p, org)) {
                text = overrides::set_initial(&text, &t.name, &t.arg);
            }
            let start = fr.time_at_start(f).max(d.start_time);
            let end = fr.time_at_end(f).min(d.end_time);
            let mut nd = d.clone();
            nd.text = shift_timed_tags(&text, start - d.start_time, dur);
            nd.start_time = start;
            nd.end_time = end;
            out.push(nd);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AE: &str = "Adobe After Effects 6.0 Keyframe Data\n\n\tUnits Per Second\t23.976\n\tSource Width\t1280\n\tSource Height\t720\n\tSource Pixel Aspect Ratio\t1\n\tComp Pixel Aspect Ratio\t1\n\nPosition\n\tFrame\tX pixels\tY pixels\tZ pixels\n\t0\t100\t100\t0\n\t1\t110\t100\t0\n\t2\t120\t104\t0\n\nScale\n\tFrame\tX percent\tY percent\tZ percent\n\t0\t100\t100\t100\n\t1\t100\t100\t100\n\t2\t200\t200\t100\n\nRotation\n\tFrame\tDegrees\n\t0\t0\n\t2\t90\n\nEnd of Keyframe Data\n";

    fn script() -> Script {
        Script::parse("[Script Info]\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Sign,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,5,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n")
    }

    #[test]
    fn parses_ae_data_and_fills_gaps() {
        let m = parse_ae(AE).unwrap();
        assert_eq!((m.fps, m.width, m.height, m.frames.len()), (23.976, 1280.0, 720.0, 3));
        assert_eq!(m.frames[1].rot, 0.0); // 1. kare dönüş eksik → önceki
        assert_eq!(m.frames[2], Track { x: 120.0, y: 104.0, sx: 200.0, sy: 200.0, rot: 90.0 });
        assert!(parse_ae("rastgele metin").is_err());
    }

    #[test]
    fn applies_frame_by_frame() {
        let s = script();
        let fr = Framerate::cfr(24.0);
        let m = parse_ae(AE).unwrap();
        // kare 10–12 boyunca görünen tabela; iz kare 10'dan başlıyor. 1280→1920 ölçek 1.5
        let d = Dialogue { start_time: fr.time_at_start(10), end_time: fr.time_at_end(12), style: "Sign".into(), text: "{\\pos(165,150)\\fad(0,100)}Tabela".into(), ..Default::default() };
        let out = apply(&s, &[d.clone()], &m, &fr, &MotionOptions { start_frame: 10, ..Default::default() }).unwrap();
        assert_eq!(out.len(), 3);
        // kareler bitişik ve satır aralığını kaplıyor
        assert_eq!(out[0].start_time, d.start_time);
        assert_eq!(out[2].end_time, d.end_time);
        assert!(out.windows(2).all(|w| w[0].end_time == w[1].start_time));
        // kare 0: iz (150,150), ofset (15,0) → (165,150); kare 1: iz x+15 → (180,150)
        assert!(out[0].text.contains("\\pos(165,150)"), "{}", out[0].text);
        assert!(out[1].text.contains("\\pos(180,150)"), "{}", out[1].text);
        // kare 2: iz (180,156), ofset ×2 ölçek → (30,0), 90° dön → (0,30) → (180,186); \fscx 200, \frz -90, \bord 4
        let t2 = &out[2].text;
        assert!(t2.contains("\\pos(180,186)") && t2.contains("\\fscx200") && t2.contains("\\frz-90") && t2.contains("\\bord4"), "{t2}");
        // \fad → kaydırılmış \fade
        assert!(out[1].text.contains("\\fade(255,0,255,"), "{}", out[1].text);
        assert!(!out[1].text.contains("\\fad("), "{}", out[1].text);
        // iz satırı kapsamıyorsa hata
        assert!(apply(&s, &[d], &m, &fr, &MotionOptions { start_frame: 11, ..Default::default() }).is_err());
    }

    #[test]
    fn default_position_from_alignment() {
        let s = script();
        let d = Dialogue { style: "Sign".into(), text: "x".into(), ..Default::default() };
        assert_eq!(line_position(&s, &d), (960.0, 540.0)); // \an5
        let d = Dialogue { style: "Sign".into(), text: "{\\an7}x".into(), ..Default::default() };
        assert_eq!(line_position(&s, &d), (10.0, 10.0));
    }

    #[test]
    fn tracks_clips() {
        let s = script();
        let fr = Framerate::cfr(24.0);
        let m = parse_ae(AE).unwrap();
        let d = Dialogue { start_time: fr.time_at_start(10), end_time: fr.time_at_end(12), style: "Sign".into(), text: "{\\pos(165,150)\\clip(150,140,180,160)}x".into(), ..Default::default() };
        let mut o = MotionOptions { start_frame: 10, ..Default::default() };
        let out = apply(&s, &[d.clone()], &m, &fr, &o).unwrap();
        assert!(out[0].text.contains("\\clip(150,140,180,160)"), "{}", out[0].text);
        assert!(out[1].text.contains("\\clip(165,140,195,160)"), "{}", out[1].text); // +15 x
        // kare 2: 90° dönüş → vektöre çevrilir; iz noktası (180,156), (150,140)−(150,150)=(0,−10) ×2 → (0,−20) → döndür (20,0)
        assert!(out[2].text.contains("\\clip(m 200 156 "), "{}", out[2].text);
        // vektör kırpma ölçek parametresiyle
        let d2 = Dialogue { text: "{\\pos(165,150)\\iclip(2,m 300 280 l 360 280)}x".into(), ..d };
        o.rotation = false;
        o.scale = false;
        let out = apply(&s, &[d2], &m, &fr, &o).unwrap();
        assert!(out[1].text.contains("\\iclip(m 165 140 l 195 140)"), "{}", out[1].text);
    }

    const PIN: &str = "Adobe After Effects 6.0 Keyframe Data\n\n\tUnits Per Second\t24\n\tSource Width\t1280\n\tSource Height\t720\n\tSource Pixel Aspect Ratio\t1\n\tComp Pixel Aspect Ratio\t1\n\nEffects\tCC Power Pin #1\tTop Left #2\n\tFrame\tX pixels\tY pixels\n\t0\t100\t100\n\t1\t110\t100\n\nEffects\tCC Power Pin #1\tTop Right #3\n\tFrame\tX pixels\tY pixels\n\t0\t300\t100\n\t1\t310\t90\n\nEffects\tCC Power Pin #1\tBottom Left #4\n\tFrame\tX pixels\tY pixels\n\t0\t100\t150\n\t1\t110\t150\n\nEffects\tCC Power Pin #1\tBottom Right #5\n\tFrame\tX pixels\tY pixels\n\t0\t300\t150\n\t1\t310\t160\n\nEnd of Keyframe Data\n";

    #[test]
    fn parses_power_pin() {
        let m = parse_corner_pin(PIN).unwrap();
        assert_eq!(m.frames.len(), 2);
        assert_eq!(m.frames[0], [(100.0, 100.0), (300.0, 100.0), (300.0, 150.0), (100.0, 150.0)]);
        assert_eq!(m.frames[1][1], (310.0, 90.0));
        // AE Corner Pin adları
        let ae = PIN.replace("CC Power Pin", "Corner Pin").replace("Top Left", "Upper Left").replace("Top Right", "Upper Right").replace("Bottom Left", "Lower Left").replace("Bottom Right", "Lower Right");
        assert_eq!(parse_corner_pin(&ae).unwrap(), m);
        assert!(parse_corner_pin(AE).is_err());
    }

    #[test]
    fn homography_maps_corners() {
        let a = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)];
        let b = [(3.0, 1.0), (20.0, -2.0), (22.0, 9.0), (1.0, 7.0)];
        let h = homography(&a, &b).unwrap();
        for i in 0..4 {
            let p = apply_homography(&h, a[i]);
            assert!((p.0 - b[i].0).abs() < 1e-9 && (p.1 - b[i].1).abs() < 1e-9);
        }
    }

    #[test]
    fn corner_pin_fits_each_frame() {
        let s = script();
        let fr = Framerate::cfr(24.0);
        let m = parse_corner_pin(PIN).unwrap();
        let d = Dialogue { start_time: fr.time_at_start(5), end_time: fr.time_at_end(6), style: "Sign".into(), text: "{\\an5\\pos(10,10)\\clip(150,150,450,225)}Tabela".into(), ..Default::default() };
        let dist = crate::perspective::dist_for(1080.0, 1080.0);
        let out = apply_corner_pin(&[d], &[(200.0, 40.0)], &s, &m, &fr, 5, dist).unwrap();
        assert_eq!(out.len(), 2);
        for (i, o) in out.iter().enumerate() {
            assert!(o.text.contains("\\an7") && o.text.contains("\\frx") && !o.text.contains("\\an5"), "{}", o.text);
            // oturtulan kutunun köşeleri hedef dörtgende (1280→1920 ölçek 1.5)
            let num = |n: &str| overrides::get_initial(&o.text, n).and_then(|t| t.number()).unwrap();
            let pos = overrides::get_initial(&o.text, "pos").unwrap().args();
            let org = overrides::get_initial(&o.text, "org").unwrap().args();
            let pf = |v: &String| v.trim().parse::<f64>().unwrap();
            let p = crate::perspective::Params { pos: (pf(&pos[0]), pf(&pos[1])), frx: num("frx"), fry: num("fry"), frz: num("frz"), fscx: num("fscx"), fscy: num("fscy"), fax: num("fax") };
            let c = crate::perspective::corners(&p, (pf(&org[0]), pf(&org[1])), dist, 200.0, 40.0);
            for (a, b) in c.iter().zip(m.frames[i].iter()) {
                assert!((a.0 - b.0 * 1.5).abs() < 0.1 && (a.1 - b.1 * 1.5).abs() < 0.1, "kare {i}: {a:?} {b:?}");
            }
        }
        // kare 0'da kırpma aynı kalır (vektöre çevrilmiş), kare 1'de homografiyle taşınır
        assert!(out[0].text.contains("\\clip(m 150 150 l 450 150 450 225 150 225)"), "{}", out[0].text);
        assert!(out[1].text.contains("\\clip(m 165 150 l 465 135 465 240 165 225)"), "{}", out[1].text);
    }
}
