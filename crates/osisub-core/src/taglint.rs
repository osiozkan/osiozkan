//! Etiket denetimi (anime tipografi/karaoke QC): yayından önce yakalanması gereken etiket hataları.
//! Her bulgu satır başına kısa bir Türkçe iletidir; `qc::check` bunları `Kind::TagError` /
//! `Kind::Note` olarak raporlar.
use crate::ass::Dialogue;
use crate::overrides::{self, Seg, Tag};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// Etiket/sözdizimi hatası ya da süreyle çelişen animasyon
    Error(String),
    /// Metinde unutulmuş not/yorum bloğu ({çevirmen notu})
    Note(String),
}

const NUMERIC: &[&str] = &[
    "fscx", "fscy", "fsp", "fax", "fay", "frx", "fry", "frz", "fr", "fs", "fe", "xbord", "ybord", "bord", "xshad", "yshad", "shad", "blur", "be", "an", "a", "q",
    "p", "pbo", "kf", "ko", "kt", "k", "K", "b", "i", "u", "s",
];

fn is_num(s: &str) -> bool {
    let s = s.trim();
    let s = s.strip_prefix('+').unwrap_or(s);
    !s.is_empty() && s.parse::<f64>().is_ok()
}

fn is_hex_color(arg: &str, max_digits: usize) -> bool {
    let a = arg.trim();
    if a.is_empty() {
        return true;
    }
    let a = a.trim_end_matches('&');
    let a = a.strip_prefix('&').unwrap_or(a);
    let Some(h) = a.strip_prefix('H').or_else(|| a.strip_prefix('h')) else { return false };
    !h.is_empty() && h.len() <= max_digits && h.chars().all(|c| c.is_ascii_hexdigit())
}

fn nums(t: &Tag) -> Option<Vec<f64>> {
    t.args().iter().map(|a| a.trim().parse::<f64>().ok()).collect()
}

/// Tek etiketin sözdizimi denetimi. `dur`: satır süresi (ms).
fn check_tag(t: &Tag, dur: i64, out: &mut Vec<Finding>, styles: &[&str], in_t: bool) {
    let err = |out: &mut Vec<Finding>, m: String| out.push(Finding::Error(m));
    if t.name.is_empty() {
        if let Some(raw) = t.arg.strip_prefix('\\') {
            let shown: String = raw.chars().take(12).collect();
            if !shown.is_empty() && shown != "N" && shown != "n" && shown != "h" {
                err(out, format!("Bilinmeyen etiket: \\{shown}"));
            }
        }
        return;
    }
    let n = t.name.as_str();
    let arg = t.arg.trim();
    if NUMERIC.contains(&n) {
        // boş argüman = stil varsayılanına dön (geçerli)
        if !arg.is_empty() && !is_num(arg) {
            err(out, format!("\\{n} argümanı sayı değil: \\{n}{arg}"));
        } else if n == "an" && !arg.is_empty() && !(1..=9).contains(&(arg.parse::<f64>().unwrap_or(0.0) as i64)) {
            err(out, format!("\\an 1-9 olmalı: \\an{arg}"));
        }
        return;
    }
    match n {
        "c" | "1c" | "2c" | "3c" | "4c" => {
            if !is_hex_color(arg, 8) {
                err(out, format!("Geçersiz renk: \\{n}{arg}"));
            }
        }
        "alpha" | "1a" | "2a" | "3a" | "4a" => {
            if !is_hex_color(arg, 8) {
                err(out, format!("Geçersiz saydamlık: \\{n}{arg}"));
            }
        }
        "pos" | "org" => match nums(t) {
            Some(v) if v.len() == 2 => {}
            _ => err(out, format!("\\{n} iki sayı almalı: \\{n}{arg}")),
        },
        "move" => match nums(t) {
            Some(v) if v.len() == 4 => {}
            Some(v) if v.len() == 6 => {
                let (t1, t2) = (v[4], v[5]);
                if t1 > t2 {
                    err(out, format!("\\move zamanları ters ({t1} > {t2})"));
                } else if dur > 0 && t1 >= dur as f64 {
                    err(out, format!("\\move satır bittikten sonra başlıyor ({t1} ms ≥ {dur} ms)"));
                }
            }
            _ => err(out, format!("\\move 4 ya da 6 sayı almalı: \\move{arg}")),
        },
        "fad" => match nums(t) {
            Some(v) if v.len() == 2 => {
                if v[0] < 0.0 || v[1] < 0.0 {
                    err(out, "\\fad negatif süre".into());
                } else if dur > 0 && v[0] + v[1] > dur as f64 {
                    err(out, format!("\\fad({},{}) satır süresinden ({dur} ms) uzun", v[0], v[1]));
                }
            }
            _ => err(out, format!("\\fad iki sayı almalı: \\fad{arg}")),
        },
        "fade" => match nums(t) {
            Some(v) if v.len() == 7 => {}
            _ => err(out, format!("\\fade 7 sayı almalı: \\fade{arg}")),
        },
        "clip" | "iclip" => {
            let a = t.args();
            let ok = match a.len() {
                4 => a.iter().all(|x| is_num(x)),
                1 => !a[0].trim().is_empty(),
                2 => is_num(&a[0]) && !a[1].trim().is_empty(),
                _ => false,
            };
            if !ok || !arg.starts_with('(') || !arg.ends_with(')') {
                err(out, format!("Geçersiz \\{n}: \\{n}{arg}"));
            }
        }
        "t" => {
            if in_t {
                err(out, "\\t içinde \\t (iç içe animasyon çalışmaz)".into());
            }
            if !arg.starts_with('(') || !arg.ends_with(')') {
                err(out, format!("\\t parantezsiz: \\t{arg}"));
                return;
            }
            let a = t.args();
            let tags = a.last().cloned().unwrap_or_default();
            let times: Vec<&String> = a[..a.len().saturating_sub(1)].iter().collect();
            let tv: Option<Vec<f64>> = times.iter().map(|x| x.trim().parse::<f64>().ok()).collect();
            match tv {
                Some(v) if v.len() <= 3 => {
                    if v.len() >= 2 {
                        if v[0] > v[1] {
                            err(out, format!("\\t zamanları ters ({} > {})", v[0], v[1]));
                        } else if dur > 0 && v[0] >= dur as f64 {
                            err(out, format!("\\t satır bittikten sonra başlıyor ({} ms ≥ {dur} ms)", v[0]));
                        }
                    }
                }
                _ => err(out, format!("\\t argümanları geçersiz: \\t{arg}")),
            }
            if !tags.trim_start().starts_with('\\') {
                err(out, "\\t içinde etiket yok".into());
            } else {
                for inner in overrides::parse_block(tags.trim()) {
                    if matches!(inner.name.as_str(), "pos" | "move" | "org" | "fad" | "fade" | "an" | "a" | "q" | "r" | "fn" | "p") {
                        err(out, format!("\\{} \\t ile canlandırılamaz", inner.name));
                    } else {
                        check_tag(&inner, dur, out, styles, true);
                    }
                }
            }
        }
        "r" => {
            if !arg.is_empty() && !styles.is_empty() && !styles.contains(&arg) {
                err(out, format!("\\r bilinmeyen stil: {arg}"));
            }
        }
        _ => {}
    }
}

/// Satırın etiket denetimi. `styles`: belgedeki stil adları (boşsa \r denetlenmez).
pub fn lint(d: &Dialogue, styles: &[&str]) -> Vec<Finding> {
    let mut out = vec![];
    let dur = d.duration();
    let text = &d.text;
    // Süslü parantez dengesi (tags::split kapanmamış '{'yi metin sayar)
    let mut depth = 0i32;
    let mut bad_brace = false;
    for c in text.chars() {
        match c {
            '{' => {
                depth += 1;
                if depth > 1 {
                    bad_brace = true;
                }
            }
            '}' => {
                depth -= 1;
                if depth < 0 {
                    bad_brace = true;
                    depth = 0;
                }
            }
            _ => {}
        }
    }
    if bad_brace || depth != 0 {
        out.push(Finding::Error("Eşleşmeyen süslü parantez { }".into()));
    }
    let segs = overrides::parse(text);
    let mut pos_count = 0;
    let mut k_sum: i64 = 0;
    let mut has_k = false;
    for s in &segs {
        let Seg::Block(tags) = s else { continue };
        if tags.is_empty() {
            continue;
        }
        // Yalnızca serbest metin içeren blok = not/yorum
        if tags.len() == 1 && tags[0].name.is_empty() && !tags[0].arg.starts_with('\\') {
            let note = tags[0].arg.trim();
            if !note.is_empty() {
                let short: String = note.chars().take(40).collect();
                out.push(Finding::Note(format!("Metinde not/yorum: {{{short}}}")));
            }
            continue;
        }
        for t in tags {
            match t.name.as_str() {
                "pos" | "move" => pos_count += 1,
                "k" | "K" | "kf" | "ko" => {
                    has_k = true;
                    k_sum += t.number().unwrap_or(0.0).round() as i64;
                }
                _ => {}
            }
            check_tag(t, dur, &mut out, styles, false);
        }
    }
    if pos_count > 1 {
        out.push(Finding::Error("Birden fazla \\pos/\\move (yalnızca biri uygulanır)".into()));
    }
    if has_k && !d.effect.eq_ignore_ascii_case("fx") && !d.comment {
        let k_ms = k_sum * 10;
        if k_ms > dur + 10 {
            out.push(Finding::Error(format!("Karaoke heceleri ({k_ms} ms) satırdan ({dur} ms) uzun")));
        } else if dur - k_ms > 500 {
            out.push(Finding::Note(format!("Karaoke heceleri satırdan {} ms kısa", dur - k_ms)));
        }
    }
    out.dedup();
    out
}

fn is_note_block(tags: &[Tag]) -> bool {
    !tags.is_empty() && tags.iter().all(|t| t.name.is_empty() && !t.arg.starts_with('\\'))
}

/// Metindeki not/yorum bloklarını ({çevirmen notu}) siler; ardında kalan çift boşluğu teke indirir.
pub fn strip_notes(text: &str) -> String {
    let segs: Vec<Seg> = overrides::parse(text).into_iter().filter(|s| !matches!(s, Seg::Block(t) if is_note_block(t))).collect();
    let mut out = overrides::to_string(&segs);
    if out != text {
        while out.contains("  ") {
            out = out.replace("  ", " ");
        }
        out = out.trim().to_string();
    }
    out
}

/// Güvenli etiket onarımı: boş `{}` bloklarını siler, bitişik etiket bloklarını birleştirir (not
/// blokları ayrı kalır), satır süresini aşan `\fad`'ı orantılı kısaltır. Değiştiyse yeni metni döndürür.
pub fn repair(d: &Dialogue) -> Option<String> {
    let dur = d.duration().max(0) as f64;
    let mut out: Vec<Seg> = vec![];
    for s in overrides::parse(&d.text) {
        match s {
            Seg::Block(tags) if tags.is_empty() => {}
            Seg::Block(mut tags) => {
                for t in tags.iter_mut() {
                    if t.name == "fad" {
                        if let Some(v) = nums(t).filter(|v| v.len() == 2 && v[0] >= 0.0 && v[1] >= 0.0) {
                            if v[0] + v[1] > dur && dur > 0.0 {
                                let k = dur / (v[0] + v[1]);
                                t.arg = format!("({},{})", (v[0] * k).floor() as i64, (v[1] * k).floor() as i64);
                            }
                        }
                    }
                }
                match out.last_mut() {
                    Some(Seg::Block(prev)) if !is_note_block(prev) && !is_note_block(&tags) => prev.extend(tags),
                    _ => out.push(Seg::Block(tags)),
                }
            }
            Seg::Text(t) if t.is_empty() => {}
            s => out.push(s),
        }
    }
    let nt = overrides::to_string(&out);
    (nt != d.text).then_some(nt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str, dur: i64) -> Dialogue {
        let mut x = Dialogue::default();
        x.start_time = 0;
        x.end_time = dur;
        x.style = "Default".into();
        x.text = text.into();
        x
    }
    fn errs(text: &str, dur: i64) -> Vec<String> {
        lint(&d(text, dur), &["Default", "Sign"]).into_iter().map(|f| match f { Finding::Error(m) | Finding::Note(m) => m }).collect()
    }

    #[test]
    fn clean_lines_have_no_findings() {
        for t in [
            "{\\an8\\pos(960,80)\\fad(200,200)\\c&H00FF00&\\3c&H000000&\\bord3\\blur1}Tabela",
            "{\\k20}ka{\\k30}ra{\\kf50}o{\\ko0}ke",
            "{\\t(0,500,\\fscx120\\fscy120)\\t(\\1c&HFFFFFF&)}Metin\\Nikinci",
            "{\\move(0,0,100,100,0,500)\\clip(0,0,100,100)}x",
            "{\\iclip(m 0 0 l 10 0 10 10)}x{\\clip(2,m 0 0 l 1 1)}y",
            "{\\fs}{\\bord}{\\r}{\\rSign}Sıfırla",
            "{\\fs+5\\alpha&H80&\\1a&HFF&\\fnArial Black}x",
            "{\\p1}m 0 0 l 100 0 100 100{\\p0}",
        ] {
            assert!(errs(t, 1000).is_empty(), "{t}: {:?}", errs(t, 1000));
        }
    }

    #[test]
    fn finds_common_mistakes() {
        let has = |t: &str, dur: i64, needle: &str| {
            let e = errs(t, dur);
            assert!(e.iter().any(|m| m.contains(needle)), "{t}: {e:?} (beklenen: {needle})");
        };
        has("{\\i1 eğik", 1000, "süslü");
        has("metin} fazla", 1000, "süslü");
        has("{\\fsc100}x", 1000, "sayı değil");
        has("{\\blurr3}x", 1000, "sayı değil");
        has("{\\bord3,5}x", 1000, "sayı değil");
        has("{\\c&HGG00FF&}x", 1000, "renk");
        has("{\\pos(10)}x", 1000, "iki sayı");
        has("{\\fad(600,600)}x", 1000, "uzun");
        has("{\\move(0,0,1,1,800,200)}x", 1000, "ters");
        has("{\\t(1500,2000,\\fs50)}x", 1000, "bittikten sonra");
        has("{\\t(0,100,\\pos(1,2))}x", 1000, "canlandırılamaz");
        has("{\\t(0,100,\\t(\\fs2))}x", 1000, "iç içe");
        has("{\\pos(1,2)}a{\\pos(3,4)}b", 1000, "Birden fazla");
        has("{\\k50}ka{\\k80}ra", 1000, "uzun");
        has("{\\k10}ka{\\k10}ra", 1000, "kısa");
        has("{\\rYok}x", 1000, "bilinmeyen stil");
        has("{\\an10}x", 1000, "1-9");
        has("{\\zz}x", 1000, "Bilinmeyen etiket");
        has("Merhaba {TLN: kelime oyunu} dünya", 1000, "not/yorum");
    }

    #[test]
    fn karaoke_fx_lines_skip_duration_check() {
        let mut x = d("{\\k500}ka", 1000);
        x.effect = "fx".into();
        assert!(lint(&x, &[]).is_empty());
    }

    #[test]
    fn repair_and_strip_notes() {
        assert_eq!(strip_notes("Merhaba {TLN: oyun} dünya"), "Merhaba dünya");
        assert_eq!(strip_notes("{\\i1}x{not}"), "{\\i1}x");
        assert_eq!(strip_notes("{\\an8}Değişmez  metin"), "{\\an8}Değişmez  metin");
        assert_eq!(repair(&d("{\\an8}{\\fad(800,800)}{}x{not}{\\i1}y", 1000)).unwrap(), "{\\an8\\fad(500,500)}x{not}{\\i1}y");
        assert_eq!(repair(&d("{\\an8}x", 1000)), None);
        // onarımdan sonra etiket hatası kalmamalı
        let fixed = repair(&d("{\\fad(900,300)}x", 1000)).unwrap();
        assert!(errs(&fixed, 1000).is_empty(), "{fixed}");
    }
}
