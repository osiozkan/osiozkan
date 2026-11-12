//! Kalite kontrol (QC) ve toplu düzeltme araçları.
use crate::ass::{Dialogue, Entry, Script};
use crate::tags;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Overlap,
    TooShort,
    TooLong,
    ReadingSpeed,
    LineTooLong,
    TooManyLines,
    SmallGap,
    Empty,
    NegativeDuration,
    UnknownStyle,
    DoubleSpace,
    EdgeSpace,
    /// Başlangıç/bitiş sahne değişiminin (anahtar kare) birkaç kare yakınında ama üstünde değil
    NearKeyframe,
    /// Aynı stildeki ardışık satırlar arasında 1-2 karelik boşluk (titreme)
    Flicker,
    /// Etiket hatası (bozuk/bilinmeyen etiket, süreyle çelişen \fad/\move/\t, karaoke süresi…)
    TagError,
    /// Metinde unutulmuş not/yorum bloğu ya da küçük uyarı (core::taglint)
    Note,
}

impl Kind {
    /// Kalite sekmesindeki süzgeç grubu: "zaman", "metin", "etiket", "not".
    pub fn category(&self) -> &'static str {
        match self {
            Kind::Overlap | Kind::TooShort | Kind::TooLong | Kind::SmallGap | Kind::NegativeDuration | Kind::NearKeyframe | Kind::Flicker => "zaman",
            Kind::ReadingSpeed | Kind::LineTooLong | Kind::TooManyLines | Kind::Empty | Kind::DoubleSpace | Kind::EdgeSpace | Kind::UnknownStyle => "metin",
            Kind::TagError => "etiket",
            Kind::Note => "not",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    /// `Script::entries` içindeki indeks.
    pub entry: usize,
    pub kind: Kind,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Rules {
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub max_cps: f64,
    pub max_line_len: usize,
    pub max_lines: usize,
    pub min_gap_ms: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { min_duration_ms: 833, max_duration_ms: 7000, max_cps: 20.0, max_line_len: 42, max_lines: 2, min_gap_ms: 0 }
    }
}

pub fn cps(d: &Dialogue) -> f64 {
    let dur = d.duration();
    if dur <= 0 {
        return f64::INFINITY;
    }
    tags::visible_len(&d.text) as f64 * 1000.0 / dur as f64
}

/// Kareye dayalı zamanlama denetimleri (video gerekir): sahne değişimine yakın başlangıç/bitiş ve
/// titreme. `keyframes_ms`: anahtar karelerin tam zamanları; `max_frames`: yakınlık eşiği (kare).
pub fn frame_issues(script: &Script, fr: &crate::vfr::Framerate, keyframes_ms: &[i64], max_frames: i64) -> Vec<Issue> {
    let mut kf: Vec<i64> = keyframes_ms.iter().map(|&k| fr.frame_at(k)).collect();
    kf.sort_unstable();
    kf.dedup();
    let near = |f: i64| -> Option<i64> {
        // f'ye en yakın anahtar kare ile işaretli uzaklık (anahtar kare - f)
        let i = kf.partition_point(|&k| k < f);
        [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|j| kf.get(j)).map(|&k| k - f).min_by_key(|d| d.abs())
    };
    let mut out = vec![];
    let mut groups: std::collections::HashMap<(&str, i32), Vec<(i64, i64, usize)>> = std::collections::HashMap::new();
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment || d.effect.eq_ignore_ascii_case("fx") || d.duration() <= 0 {
            continue;
        }
        let (fs, fe) = (fr.frame_at_start(d.start_time), fr.frame_at_end(d.end_time) + 1);
        if !kf.is_empty() {
            if let Some(dl) = near(fs).filter(|dl| *dl != 0 && dl.abs() <= max_frames) {
                out.push(Issue { entry: i, kind: Kind::NearKeyframe, message: format!("Başlangıç sahne değişiminden {} kare {}", dl.abs(), if dl > 0 { "önce" } else { "sonra" }) });
            }
            if let Some(dl) = near(fe).filter(|dl| *dl != 0 && dl.abs() <= max_frames) {
                out.push(Issue { entry: i, kind: Kind::NearKeyframe, message: format!("Bitiş sahne değişiminden {} kare {}", dl.abs(), if dl > 0 { "önce" } else { "sonra" }) });
            }
        }
        if !is_typeset(d) && !is_karaoke(d) {
            groups.entry((d.style.as_str(), d.layer)).or_default().push((fs, fe, i));
        }
    }
    for (_, mut v) in groups {
        v.sort_unstable();
        for w in v.windows(2) {
            let gap = w[1].0 - w[0].1;
            if (1..=2).contains(&gap) {
                out.push(Issue { entry: w[1].2, kind: Kind::Flicker, message: format!("Önceki satırla arada {} kare boşluk (titreme)", gap) });
            }
        }
    }
    out.sort_by_key(|x| x.entry);
    out
}

/// `frame_issues`'in bulduğu sahne değişimi yakınlıklarını düzeltir: ≤`max_frames` kare uzaktaki
/// başlangıç/bitişi anahtar karenin ASS-güvenli sınırına çeker. Değişen satır sayısı.
pub fn snap_to_keyframes(script: &mut Script, fr: &crate::vfr::Framerate, keyframes_ms: &[i64], max_frames: i64) -> usize {
    let mut kf: Vec<i64> = keyframes_ms.iter().map(|&k| fr.frame_at(k)).collect();
    kf.sort_unstable();
    kf.dedup();
    let nearest = |f: i64| -> Option<i64> {
        let i = kf.partition_point(|&k| k < f);
        [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|j| kf.get(j).copied()).min_by_key(|k| (k - f).abs()).filter(|k| (k - f).abs() <= max_frames)
    };
    let mut n = 0;
    for d in script.dialogues_mut() {
        if d.comment || d.effect.eq_ignore_ascii_case("fx") || d.duration() <= 0 {
            continue;
        }
        let (fs, fe) = (fr.frame_at_start(d.start_time), fr.frame_at_end(d.end_time) + 1);
        let (mut s, mut e) = (d.start_time, d.end_time);
        if let Some(k) = nearest(fs).filter(|k| *k != fs) {
            s = fr.time_at_start(k);
        }
        if let Some(k) = nearest(fe).filter(|k| *k != fe) {
            e = fr.time_at_start(k);
        }
        if e > s && (s, e) != (d.start_time, d.end_time) {
            d.start_time = s;
            d.end_time = e;
            n += 1;
        }
    }
    n
}

/// Tipografi (tabela/yazı) satırı: konumlandırılmış ya da çizim. Okuma hızı, uzunluk, süre ve
/// çakışma denetimleri bunlar için anlamsızdır (anime'de tabelalar diyalogla üst üste biner).
pub fn is_typeset(d: &Dialogue) -> bool {
    let t = &d.text;
    t.contains("\\pos(") || t.contains("\\move(") || t.contains("\\p1") || t.contains("\\clip(") || t.contains("\\iclip(") || t.contains("\\org(")
}

/// Karaoke satırı (şarkı sözü; zamanlama müziğe bağlı, okuma hızı anlamsız).
pub fn is_karaoke(d: &Dialogue) -> bool {
    let t = &d.text;
    t.contains("\\k") || t.contains("\\K")
}

pub fn check(script: &Script, rules: &Rules) -> Vec<Issue> {
    let mut issues = Vec::new();
    let styles: Vec<&str> = script.styles().map(|s| s.name.as_str()).collect();
    // Çakışma: aynı stil + katmandaki diyalog satırları zaman sırasıyla karşılaştırılır (dosya sırası değil).
    let mut groups: std::collections::HashMap<(&str, i32), Vec<(i64, i64, usize)>> = std::collections::HashMap::new();
    let mut prev: Option<(usize, &Dialogue)> = None;
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment {
            continue;
        }
        let mut add = |kind: Kind, message: String| issues.push(Issue { entry: i, kind, message });
        if !styles.contains(&d.style.as_str()) {
            add(Kind::UnknownStyle, format!("Bilinmeyen stil: {}", d.style));
        }
        // Şablonlayıcı çıktısı (fx): yalnızca stil denetimi
        if d.effect.eq_ignore_ascii_case("fx") {
            continue;
        }
        for f in crate::taglint::lint(d, &styles) {
            match f {
                crate::taglint::Finding::Error(m) => add(Kind::TagError, m),
                crate::taglint::Finding::Note(m) => add(Kind::Note, m),
            }
        }
        let (typeset, karaoke) = (is_typeset(d), is_karaoke(d));
        let visible = tags::visible_text(&d.text);
        if visible.trim().is_empty() && !(1..=9).any(|k| d.text.contains(&format!("\\p{k}"))) {
            add(Kind::Empty, "Boş metin".into());
        }
        if d.duration() < 0 {
            add(Kind::NegativeDuration, "Bitiş başlangıçtan önce".into());
        } else if typeset || karaoke {
        } else if d.duration() < rules.min_duration_ms {
            add(Kind::TooShort, format!("Süre çok kısa ({} ms)", d.duration()));
        } else if d.duration() > rules.max_duration_ms {
            add(Kind::TooLong, format!("Süre çok uzun ({} ms)", d.duration()));
        }
        let c = cps(d);
        if c > rules.max_cps && !visible.trim().is_empty() && !typeset && !karaoke {
            add(Kind::ReadingSpeed, format!("Okuma hızı {:.1} kar/sn", c));
        }
        if !typeset && tags::max_line_len(&d.text) > rules.max_line_len {
            add(Kind::LineTooLong, format!("Satır {} karakterden uzun", rules.max_line_len));
        }
        if !typeset && visible.lines().count() > rules.max_lines {
            add(Kind::TooManyLines, format!("{} satırdan fazla", rules.max_lines));
        }
        if visible.contains("  ") {
            add(Kind::DoubleSpace, "Çift boşluk".into());
        }
        if visible.lines().any(|l| l != l.trim()) {
            add(Kind::EdgeSpace, "Satır başı/sonu boşluğu".into());
        }
        if !typeset && !karaoke {
            groups.entry((d.style.as_str(), d.layer)).or_default().push((d.start_time, d.end_time, i));
            if let Some((_, p)) = prev {
                if d.style == p.style && d.layer == p.layer && d.start_time >= p.end_time && d.start_time - p.end_time < rules.min_gap_ms {
                    add(Kind::SmallGap, format!("Boşluk {} ms", d.start_time - p.end_time));
                }
            }
            prev = Some((i, d));
        }
    }
    for (_, mut v) in groups {
        v.sort_unstable();
        // şimdiye kadarki en geç biten satır
        let mut far: Option<(i64, usize)> = None;
        for (s, e, i) in v {
            if let Some((fe, _)) = far {
                if s < fe {
                    issues.push(Issue { entry: i, kind: Kind::Overlap, message: format!("Önceki satırla {} ms çakışıyor", fe.min(e) - s) });
                }
            }
            if far.map_or(true, |(fe, _)| e > fe) {
                far = Some((e, i));
            }
        }
    }
    issues.sort_by_key(|x| x.entry);
    issues
}


/// Betik geneli (başlık) denetimleri: satıra bağlı olmayan, yayından önce düzeltilmesi gereken
/// ayarlar. `video`: açık videonun (genişlik, yükseklik).
pub fn script_issues(script: &Script, video: Option<(u32, u32)>) -> Vec<String> {
    script_issues_dar(script, video, None)
}

/// `dar`: anamorfik videoda görüntü en-boy oranı; PlayRes oranı depolama ya da görüntü oranından
/// birine uyuyorsa uyarı verilmez.
pub fn script_issues_dar(script: &Script, video: Option<(u32, u32)>, dar: Option<f64>) -> Vec<String> {
    let mut out = vec![];
    let px = script.info("PlayResX").and_then(|v| v.trim().parse::<f64>().ok());
    let py = script.info("PlayResY").and_then(|v| v.trim().parse::<f64>().ok());
    if px.is_none() || py.is_none() {
        out.push("PlayResX/PlayResY yok: oynatıcılar 384×288 varsayar, konum ve boyutlar kayar".into());
    }
    if !script.info("ScaledBorderAndShadow").map_or(false, |v| v.trim().eq_ignore_ascii_case("yes")) {
        out.push("ScaledBorderAndShadow: yes değil — kenar/gölge kalınlığı video çözünürlüğüne göre değişir".into());
    }
    if let (Some(x), Some(y), Some((vw, vh))) = (px, py, video) {
        if vw > 0 && vh > 0 {
            let (ra, rv) = (x / y, vw as f64 / vh as f64);
            if (ra - rv).abs() / rv > 0.02 && dar.map_or(true, |d| (ra - d).abs() / d > 0.02) {
                out.push(format!("PlayRes en-boy oranı ({}×{}) videoyla ({}×{}) uyuşmuyor: yazılar yatayda esner", x, y, vw, vh));
            }
        }
    }
    if let Some(m) = script.info("YCbCr Matrix") {
        let m = m.trim();
        if let Some((_, vh)) = video {
            if vh >= 720 && m.eq_ignore_ascii_case("TV.601") {
                out.push(format!("YCbCr Matrix: {m}, ama video HD ({vh}p, BT.709) — tabela renkleri kayar; TV.709 olmalı"));
            }
            if vh > 0 && vh < 720 && m.eq_ignore_ascii_case("TV.709") {
                out.push(format!("YCbCr Matrix: {m}, ama video SD ({vh}p, genelde BT.601)"));
            }
        }
    }
    let mut names: Vec<&str> = script.styles().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    for w in names.windows(2) {
        if w[0] == w[1] {
            out.push(format!("Yinelenen stil adı: {} (yalnızca biri kullanılır)", w[0]));
        }
    }
    out.dedup();
    out
}

// ---------------------------------------------------------------- düzeltmeler

/// Seçili (indeks listesi) satırları kaydırır. `None` = tümü.
/// `only`: kaydırılacak entry indeksleri (sıralı olmak zorunda değil).
pub fn shift(script: &mut Script, ms: i64, only: Option<&[usize]>) {
    let only: Option<Vec<usize>> = only.map(|o| { let mut v = o.to_vec(); v.sort_unstable(); v });
    for (i, e) in script.entries.iter_mut().enumerate() {
        if let Entry::Dialogue(d) = e {
            if only.as_ref().map_or(true, |s| s.binary_search(&i).is_ok()) {
                d.start_time = (d.start_time + ms).max(0);
                d.end_time = (d.end_time + ms).max(0);
            }
        }
    }
}

/// Kare cinsinden kaydırma (Aegisub "Shift Times" kare kipi): her satırın görünen ilk/son karesi
/// `frames` kare kaydırılır; VFR'de doğru, santisaniye yuvarlamasında kayma birikmez.
pub fn shift_frames(script: &mut Script, fr: &crate::vfr::Framerate, frames: i64, only: Option<&[usize]>) {
    let only: Option<Vec<usize>> = only.map(|o| { let mut v = o.to_vec(); v.sort_unstable(); v });
    for (i, e) in script.entries.iter_mut().enumerate() {
        if let Entry::Dialogue(d) = e {
            if only.as_ref().map_or(true, |s| s.binary_search(&i).is_ok()) {
                let (fs, fe) = (fr.frame_at_start(d.start_time), fr.frame_at_end(d.end_time));
                let empty = d.end_time <= d.start_time;
                d.start_time = fr.time_at_start((fs + frames).max(0));
                d.end_time = if empty { d.start_time } else { fr.time_at_end((fe + frames).max(0)).max(d.start_time) };
            }
        }
    }
}

/// Doğrusal zaman dönüşümü: iki bağlantı noktasından (kaynak→hedef) kaydırma + ölçekleme (senkron düzeltme).
pub fn linear_retime(script: &mut Script, a: (i64, i64), b: (i64, i64)) {
    if a.0 == b.0 {
        return;
    }
    let scale = (b.1 - a.1) as f64 / (b.0 - a.0) as f64;
    let f = |t: i64| (a.1 as f64 + (t - a.0) as f64 * scale).round().max(0.0) as i64;
    for d in script.dialogues_mut() {
        d.start_time = f(d.start_time);
        d.end_time = f(d.end_time);
    }
}

pub fn fps_convert(script: &mut Script, from_fps: f64, to_fps: f64) {
    let r = from_fps / to_fps;
    for d in script.dialogues_mut() {
        d.start_time = (d.start_time as f64 * r).round() as i64;
        d.end_time = (d.end_time as f64 * r).round() as i64;
    }
}

pub fn sort_by_start(script: &mut Script) {
    let first = script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_)));
    let Some(first) = first else { return };
    let last = script.entries.iter().rposition(|e| matches!(e, Entry::Dialogue(_))).unwrap();
    let mut slice: Vec<Entry> = script.entries.drain(first..=last).collect();
    slice.sort_by_key(|e| match e {
        Entry::Dialogue(d) => (d.start_time, d.layer),
        _ => (i64::MAX, 0),
    });
    script.entries.splice(first..first, slice);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Start,
    End,
    Style,
    Actor,
    Effect,
    Layer,
    Text,
}

/// Diyalog satırlarını anahtara göre (kararlı) sıralar; diğer girdiler yerinde kalır.
pub fn sort_by(script: &mut Script, key: SortKey) {
    let Some(first) = script.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))) else { return };
    let last = script.entries.iter().rposition(|e| matches!(e, Entry::Dialogue(_))).unwrap();
    let mut slice: Vec<Entry> = script.entries.drain(first..=last).collect();
    slice.sort_by(|a, b| match (a, b) {
        (Entry::Dialogue(x), Entry::Dialogue(y)) => match key {
            SortKey::Start => (x.start_time, x.layer).cmp(&(y.start_time, y.layer)),
            SortKey::End => x.end_time.cmp(&y.end_time),
            SortKey::Style => tr_lower(&x.style).cmp(&tr_lower(&y.style)).then(x.start_time.cmp(&y.start_time)),
            SortKey::Actor => tr_lower(&x.actor).cmp(&tr_lower(&y.actor)).then(x.start_time.cmp(&y.start_time)),
            SortKey::Effect => tr_lower(&x.effect).cmp(&tr_lower(&y.effect)).then(x.start_time.cmp(&y.start_time)),
            SortKey::Layer => x.layer.cmp(&y.layer).then(x.start_time.cmp(&y.start_time)),
            SortKey::Text => tr_lower(&tags::strip_tags(&x.text)).cmp(&tr_lower(&tags::strip_tags(&y.text))),
        },
        _ => std::cmp::Ordering::Equal,
    });
    for (k, e) in slice.into_iter().enumerate() {
        script.entries.insert(first + k, e);
    }
}

pub fn snap_to_frames(script: &mut Script, fps: f64) {
    snap_to_framerate(script, &crate::vfr::Framerate::cfr(fps));
}

/// Sabit ya da değişken (timecode) kare hızına göre kareye oturtur.
pub fn snap_to_framerate(script: &mut Script, fr: &crate::vfr::Framerate) {
    // Görünen kareler korunur; zamanlar santisaniye yuvarlamasına dayanıklı kare sınırlarına çekilir.
    for d in script.dialogues_mut() {
        let (s, e) = (fr.snap_start(d.start_time), fr.snap_end(d.end_time));
        d.start_time = s;
        d.end_time = e.max(s);
    }
}

/// Aynı stil/katmandaki ardışık çakışmaları, önceki satırın bitişini kısaltarak giderir.
/// Aynı stil+katmandaki diyalog satırlarında (zaman sırasıyla) öncekinin bitişini sonrakinin
/// başlangıcına çeker. Tipografi, karaoke ve fx satırlarına dokunmaz.
pub fn fix_overlaps(script: &mut Script) -> usize {
    let mut groups: std::collections::HashMap<(String, i32), Vec<(i64, usize)>> = std::collections::HashMap::new();
    for (i, e) in script.entries.iter().enumerate() {
        if let Entry::Dialogue(d) = e {
            if !d.comment && !is_typeset(d) && !is_karaoke(d) && !d.effect.eq_ignore_ascii_case("fx") {
                groups.entry((d.style.clone(), d.layer)).or_default().push((d.start_time, i));
            }
        }
    }
    let mut fixed = 0;
    for (_, mut v) in groups {
        v.sort_unstable();
        for w in v.windows(2) {
            let start = w[1].0;
            if let Entry::Dialogue(pd) = &mut script.entries[w[0].1] {
                if pd.end_time > start && start > pd.start_time {
                    pd.end_time = start;
                    fixed += 1;
                }
            }
        }
    }
    fixed
}

/// Yinelenen (aynı zaman+stil+metin) satırları siler.
pub fn remove_duplicates(script: &mut Script) -> usize {
    let mut seen = std::collections::HashSet::new();
    let before = script.entries.len();
    script.entries.retain(|e| match e {
        Entry::Dialogue(d) => seen.insert((d.start_time, d.end_time, d.style.clone(), d.text.clone(), d.layer, d.comment)),
        _ => true,
    });
    before - script.entries.len()
}

pub fn remove_empty(script: &mut Script) -> usize {
    let before = script.entries.len();
    script.entries.retain(|e| match e {
        Entry::Dialogue(d) => d.comment || !tags::visible_text(&d.text).trim().is_empty() || (1..=9).any(|k| d.text.contains(&format!("\\p{k}"))),
        _ => true,
    });
    before - script.entries.len()
}

/// Çift boşluk, kenar boşluğu ve üç noktayı temizler.
pub fn clean_text(text: &str) -> String {
    let text = crate::overrides::clean(text);
    let mut out = String::new();
    for p in tags::split(&text) {
        match p {
            tags::Part::Block(b) => {
                out.push('{');
                out.push_str(b);
                out.push('}');
            }
            tags::Part::Text(t) => {
                let mut t = t.replace("...", "…");
                while t.contains("  ") {
                    t = t.replace("  ", " ");
                }
                out.push_str(&t.replace(" \\N", "\\N").replace("\\N ", "\\N"));
            }
        }
    }
    out.trim().to_string()
}

/// Türkçe büyük/küçük harf dönüşümü (i/İ, ı/I).
pub fn tr_upper(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'i' => vec!['İ'], 'ı' => vec!['I'], c => c.to_uppercase().collect() }).collect()
}

pub fn tr_lower(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'I' => vec!['ı'], 'İ' => vec!['i'], c => c.to_lowercase().collect() }).collect()
}

/// Uzun düz metni (etiketsiz) en fazla `max_lines` satıra dengeli böler (`\N`).
pub fn balance_lines(text: &str, max_len: usize) -> String {
    let flat = text.replace("\\N", " ");
    let flat = flat.trim();
    if flat.chars().count() <= max_len {
        return flat.to_string();
    }
    let chars: Vec<char> = flat.chars().collect();
    let mid = chars.len() / 2;
    let mut best: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        if *c == ' ' && best.map_or(true, |b| i.abs_diff(mid) < b.abs_diff(mid)) {
            best = Some(i);
        }
    }
    match best {
        Some(i) => format!("{}\\N{}", chars[..i].iter().collect::<String>().trim_end(), chars[i + 1..].iter().collect::<String>().trim_start()),
        None => flat.to_string(),
    }
}

fn html_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Ekip QC'si için tek dosyalık HTML raporu: başlık sorunları + kategoriye göre satır sorunları
/// (satır no = ızgaradaki diyalog sırası, zaman, stil, metin, ileti).
pub fn report_html(script: &Script, title: &str, head: &[String], issues: &[Issue]) -> String {
    let mut ord = vec![0usize; script.entries.len()];
    let mut n = 0;
    for (i, e) in script.entries.iter().enumerate() {
        if let Entry::Dialogue(_) = e {
            n += 1;
        }
        ord[i] = n;
    }
    let mut out = String::new();
    out.push_str(&format!("<!doctype html><html lang=\"tr\"><head><meta charset=\"utf-8\"><title>QC: {}</title><style>\n\
:root{{--bg:#fff;--fg:#1d1d21;--mut:#666;--line:#ddd;--bad:#c0392b;--acc:#b03a6f}}\n\
@media (prefers-color-scheme:dark){{:root{{--bg:#16171c;--fg:#e8e8ec;--mut:#9a9aa5;--line:#2c2d35;--bad:#ff6b5e;--acc:#e56b9a}}}}\n\
body{{background:var(--bg);color:var(--fg);font:14px/1.45 system-ui,sans-serif;margin:24px}}h1{{font-size:20px}}h2{{font-size:16px;color:var(--acc);margin-top:24px}}\n\
table{{border-collapse:collapse;width:100%;table-layout:fixed}}th:nth-child(1){{width:3.5em}}th:nth-child(2){{width:7.5em}}th:nth-child(3){{width:9em}}th:nth-child(5){{width:32%}}td,th{{border-bottom:1px solid var(--line);padding:4px 8px;text-align:left;vertical-align:top}}th{{color:var(--mut);font-weight:600}}\n\
td.n,td.t{{font-family:ui-monospace,monospace;white-space:nowrap}}.m{{color:var(--bad)}}.x{{color:var(--mut)}}</style></head><body>\n", html_esc(title)));
    out.push_str(&format!("<h1>Kalite raporu — {}</h1><p class=\"x\">{} diyalog satırı, {} satır sorunu, {} başlık sorunu</p>\n", html_esc(title), n, issues.len(), head.len()));
    if !head.is_empty() {
        out.push_str("<h2>Betik başlığı</h2><ul>");
        for h in head {
            out.push_str(&format!("<li class=\"m\">{}</li>", html_esc(h)));
        }
        out.push_str("</ul>\n");
    }
    for (cat, name) in [("zaman", "Zamanlama"), ("metin", "Metin"), ("etiket", "Etiketler"), ("not", "Notlar")] {
        let rows: Vec<&Issue> = issues.iter().filter(|i| i.kind.category() == cat).collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("<h2>{} ({})</h2><table><tr><th>#</th><th>Zaman</th><th>Stil</th><th>Metin</th><th>Sorun</th></tr>\n", name, rows.len()));
        for is in rows {
            let (t, st, tx) = match script.entries.get(is.entry) {
                Some(Entry::Dialogue(d)) => (crate::time::format_ass(d.start_time), d.style.clone(), tags::strip_tags(&d.text).replace("\\N", " / ")),
                _ => (String::new(), String::new(), String::new()),
            };
            let tx: String = tx.chars().take(80).collect();
            out.push_str(&format!("<tr><td class=\"n\">{}</td><td class=\"t\">{}</td><td>{}</td><td>{}</td><td class=\"m\">{}</td></tr>\n", ord.get(is.entry).copied().unwrap_or(0), t, html_esc(&st), html_esc(&tx), html_esc(&is.message)));
        }
        out.push_str("</table>\n");
    }
    out.push_str("</body></html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: i64, e: i64, t: &str) -> Dialogue {
        Dialogue { start_time: s, end_time: e, text: t.into(), ..Default::default() }
    }

    #[test]
    fn anime_typesetting_is_not_flagged() {
        let mk = |s: i64, e: i64, style: &str, t: &str| Dialogue { start_time: s, end_time: e, style: style.into(), text: t.into(), ..Default::default() };
        let mut sc = Script::from_dialogues(vec![
            mk(0, 3000, "Default", "Merhaba"),
            // tabela: kısa, uzun, çok satırlı, diyalogla çakışıyor — işaretlenmemeli
            mk(500, 700, "Default", "{\\pos(100,100)}OKUL GİRİŞİ ÇOK UZUN BİR TABELA YAZISI BURADA\\Nikinci\\Nüçüncü"),
            // araya giren tabela yüzünden kaçırılan gerçek çakışma
            mk(2500, 4000, "Default", "Dosya sırasında ikinci diyalog"),
            // karaoke: hızlı ve kısa olabilir
            mk(100, 400, "Default", "{\\k10}ka{\\k20}ra"),
        ]);
        let st = crate::ass::Style { name: "Default".into(), ..Default::default() };
        sc.entries.insert(0, Entry::Style(st));
        let issues = check(&sc, &Rules::default());
        let at = |n: usize| sc.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).nth(n).unwrap().0;
        assert!(!issues.iter().any(|x| x.entry == at(1)), "tabela işaretlendi: {:?}", issues);
        assert!(!issues.iter().any(|x| x.entry == at(3)), "karaoke işaretlendi: {:?}", issues);
        assert!(issues.iter().any(|x| x.entry == at(2) && x.kind == Kind::Overlap), "gerçek çakışma bulunmadı: {:?}", issues);
        let sign_before = sc.dialogues().nth(1).unwrap().clone();
        assert_eq!(fix_overlaps(&mut sc), 1);
        assert_eq!(sc.dialogues().next().unwrap().end_time, 2500);
        assert_eq!(sc.dialogues().nth(1).unwrap(), &sign_before, "tabelaya dokunulmamalı");
        assert!(!check(&sc, &Rules::default()).iter().any(|x| x.kind == Kind::Overlap));
    }

    #[test]
    fn keyframe_and_flicker() {
        let fr = crate::vfr::Framerate::cfr(24.0);
        let t = |f: i64| fr.time_at_start(f);
        // anahtar kare: 48. kare (2000 ms)
        let kf = vec![fr.time_at(48)];
        let s = Script::from_dialogues(vec![
            d(t(46), t(60), "iki kare önce başlıyor"),
            d(t(48), t(70), "tam üstünde"),
            d(t(71), t(90), "bir kare boşluk"),
            d(t(90), t(100), "bitişik"),
        ]);
        let is = frame_issues(&s, &fr, &kf, 3);
        let at = |n: usize| s.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).nth(n).unwrap().0;
        assert!(is.iter().any(|x| x.entry == at(0) && x.kind == Kind::NearKeyframe && x.message.contains("2 kare önce")), "{:?}", is);
        assert!(!is.iter().any(|x| x.entry == at(1)), "{:?}", is);
        // d1 ve d2 farklı satırlar ama aynı stil: 70→71 = 1 kare boşluk
        assert!(is.iter().any(|x| x.entry == at(2) && x.kind == Kind::Flicker), "{:?}", is);
        assert!(!is.iter().any(|x| x.entry == at(3)), "{:?}", is);
        let mut s2 = s.clone();
        assert_eq!(snap_to_keyframes(&mut s2, &fr, &kf, 3), 1);
        assert_eq!(s2.dialogues().next().unwrap().start_time, t(48));
        assert!(!frame_issues(&s2, &fr, &kf, 3).iter().any(|x| x.kind == Kind::NearKeyframe));
    }

    #[test]
    fn frame_shift_vfr() {
        let mut ts: Vec<i64> = (0..48).map(|i| (i as f64 * 1000.0 / 24.0).round() as i64).collect();
        ts.extend((0..120).map(|i| 2000 + (i as f64 * 1000.0 / 60.0).round() as i64));
        let fr = crate::vfr::Framerate::from_timestamps(ts).unwrap();
        // 40-46. karelerde görünen satır → 10 kare ileri: 50-56 (60 fps bölgesine geçer)
        let mut s = Script::from_dialogues(vec![d(fr.time_at_start(40), fr.time_at_end(46), "x")]);
        shift_frames(&mut s, &fr, 10, None);
        let x = s.dialogues().next().unwrap();
        assert_eq!((fr.frame_at_start(x.start_time), fr.frame_at_end(x.end_time)), (50, 56));
        shift_frames(&mut s, &fr, -10, None);
        let x = s.dialogues().next().unwrap();
        assert_eq!((fr.frame_at_start(x.start_time), fr.frame_at_end(x.end_time)), (40, 46));
    }

    #[test]
    fn overlap_uses_time_order_not_file_order() {
        // dosyada ters sırada ama zamanda çakışmayan satırlar
        let s = Script::from_dialogues(vec![d(5000, 6000, "b"), d(0, 2000, "a")]);
        assert!(!check(&s, &Rules::default()).iter().any(|i| i.kind == Kind::Overlap));
    }

    #[test]
    fn overlap_and_fix() {
        let mut s = Script::from_dialogues(vec![d(0, 2000, "a"), d(1500, 3000, "b")]);
        let issues = check(&s, &Rules::default());
        assert!(issues.iter().any(|i| i.kind == Kind::Overlap));
        assert_eq!(fix_overlaps(&mut s), 1);
        assert!(!check(&s, &Rules::default()).iter().any(|i| i.kind == Kind::Overlap));
    }

    #[test]
    fn sort_keys() {
        let mut s = Script::from_dialogues(vec![
            Dialogue { start_time: 2000, text: "b".into(), style: "Zeta".into(), ..Default::default() },
            Dialogue { start_time: 1000, text: "a".into(), style: "alfa".into(), ..Default::default() },
            Dialogue { start_time: 3000, text: "I".into(), style: "İkinci".into(), ..Default::default() },
        ]);
        sort_by(&mut s, SortKey::Style);
        let v: Vec<&str> = s.dialogues().map(|d| d.style.as_str()).collect();
        assert_eq!(v, vec!["alfa", "İkinci", "Zeta"]);
        sort_by(&mut s, SortKey::Start);
        assert_eq!(s.dialogues().map(|d| d.start_time).collect::<Vec<_>>(), vec![1000, 2000, 3000]);
    }

    #[test]
    fn tr_case() {
        assert_eq!(tr_upper("iğne ışık"), "İĞNE IŞIK");
        assert_eq!(tr_lower("İĞNE IŞIK"), "iğne ışık");
    }

    #[test]
    fn retime() {
        let mut s = Script::from_dialogues(vec![d(1000, 2000, "a"), d(11000, 12000, "b")]);
        linear_retime(&mut s, (1000, 2000), (11000, 22000));
        let v: Vec<_> = s.dialogues().map(|d| d.start_time).collect();
        assert_eq!(v, vec![2000, 22000]);
    }

    #[test]
    fn balance() {
        assert_eq!(balance_lines("bir iki üç dört beş altı yedi sekiz dokuz on on bir on iki", 30), "bir iki üç dört beş altı yedi\\Nsekiz dokuz on on bir on iki");
    }

    #[test]
    fn html_report() {
        let s = Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 100, text: "kısa <b>".into(), ..Default::default() },
            Dialogue { start_time: 1000, end_time: 3000, text: "tamam".into(), ..Default::default() },
        ]);
        let issues = check(&s, &Rules::default());
        let h = report_html(&s, "bölüm 01", &["PlayRes yok".into()], &issues);
        assert!(h.contains("<h1>Kalite raporu — bölüm 01</h1>") && h.contains("PlayRes yok"));
        assert!(h.contains("kısa &lt;b&gt;") && !h.contains("kısa <b>"));
        assert!(h.contains("<td class=\"n\">1</td>"));
    }

    #[test]
    fn script_header_checks() {
        let base = "[Script Info]\nScriptType: v4.00+\n{RES}\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: A,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\nStyle: A,Arial,30,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n";
        let bad = Script::parse(&base.replace("{RES}", "YCbCr Matrix: TV.601"));
        let v = script_issues(&bad, Some((1920, 1080)));
        assert!(v.iter().any(|m| m.contains("PlayResX")) && v.iter().any(|m| m.contains("ScaledBorder")) && v.iter().any(|m| m.contains("TV.709")) && v.iter().any(|m| m.contains("Yinelenen")), "{v:?}");
        let ok = Script::parse(&base.replace("{RES}", "PlayResX: 1920\nPlayResY: 1080\nScaledBorderAndShadow: yes\nYCbCr Matrix: TV.709").replace("Style: A,Arial,30", "Style: B,Arial,30"));
        assert!(script_issues(&ok, Some((1920, 1080))).is_empty());
        assert!(script_issues(&ok, Some((1440, 1080))).iter().any(|m| m.contains("en-boy")));
        // anamorfik 1440×1080 (görüntü 16:9): 1920×1080 PlayRes kabul
        assert!(!script_issues_dar(&ok, Some((1440, 1080)), Some(16.0 / 9.0)).iter().any(|m| m.contains("en-boy")));
    }
}
