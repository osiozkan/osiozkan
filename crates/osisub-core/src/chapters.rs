//! Bölümler (Matroska chapters): Efekt alanı "chapter" olan satırlar (yorum da olabilir) bölüm
//! başlangıcıdır, metni bölüm adıdır. Matroska XML, OGM (CHAPTER01=…) ve ffmpeg FFMETADATA
//! (MKV'ye gömmek için) yazılır; XML/OGM okunup satırlara geri çevrilebilir.
use crate::ass::{Dialogue, Entry, Script};

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub start_ms: i64,
    pub title: String,
}

pub const EFFECT: &str = "chapter";

pub fn is_chapter(d: &Dialogue) -> bool {
    d.effect.trim().eq_ignore_ascii_case(EFFECT) || d.effect.trim().eq_ignore_ascii_case("chapters")
}

/// Belgedeki bölümler (başlangıca göre sıralı; aynı anda iki bölüm varsa ilki).
pub fn from_script(s: &Script) -> Vec<Chapter> {
    let mut v: Vec<Chapter> = s
        .dialogues()
        .filter(|d| is_chapter(d))
        .map(|d| Chapter { start_ms: d.start_time.max(0), title: crate::tags::strip_tags(&d.text).replace("\\N", " ").trim().to_string() })
        .collect();
    v.sort_by_key(|c| c.start_ms);
    v.dedup_by_key(|c| c.start_ms);
    // adsız bölümler zaman sırasındaki numarasını alır
    for (i, c) in v.iter_mut().enumerate() {
        if c.title.is_empty() {
            c.title = format!("Bölüm {}", i + 1);
        }
    }
    v
}

fn hms(ms: i64, sep_ms: char) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, sep_ms, ms % 1000)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Matroska bölüm XML'i (mkvmerge `--chapters`, MKVToolNix bölüm düzenleyicisi).
pub fn to_matroska_xml(ch: &[Chapter], lang: &str) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE Chapters SYSTEM \"matroskachapters.dtd\">\n<Chapters>\n  <EditionEntry>\n");
    for c in ch {
        s.push_str(&format!(
            "    <ChapterAtom>\n      <ChapterTimeStart>{}000000</ChapterTimeStart>\n      <ChapterDisplay>\n        <ChapterString>{}</ChapterString>\n        <ChapterLanguage>{}</ChapterLanguage>\n      </ChapterDisplay>\n    </ChapterAtom>\n",
            hms(c.start_ms, '.'),
            xml_escape(&c.title),
            if lang.is_empty() { "und" } else { lang }
        ));
    }
    s.push_str("  </EditionEntry>\n</Chapters>\n");
    s
}

/// OGM basit bölüm biçimi.
pub fn to_ogm(ch: &[Chapter]) -> String {
    let mut s = String::new();
    for (i, c) in ch.iter().enumerate() {
        s.push_str(&format!("CHAPTER{:02}={}\nCHAPTER{:02}NAME={}\n", i + 1, hms(c.start_ms, '.'), i + 1, c.title));
    }
    s
}

/// ffmpeg FFMETADATA (`-i bolumler.txt -map_chapters 1`). Bitiş: sonraki bölüm, sonuncusu `total_ms`.
pub fn to_ffmetadata(ch: &[Chapter], total_ms: i64) -> String {
    let esc = |t: &str| t.replace('\\', "\\\\").replace('=', "\\=").replace(';', "\\;").replace('#', "\\#").replace('\n', "\\\n");
    let mut s = String::from(";FFMETADATA1\n");
    for (i, c) in ch.iter().enumerate() {
        let end = ch.get(i + 1).map(|n| n.start_ms).unwrap_or(total_ms).max(c.start_ms + 1);
        s.push_str(&format!("[CHAPTER]\nTIMEBASE=1/1000\nSTART={}\nEND={}\ntitle={}\n", c.start_ms, end, esc(&c.title)));
    }
    s
}

fn parse_hms(t: &str) -> Option<i64> {
    let t = t.trim();
    let (hms, frac) = t.split_once(['.', ',']).unwrap_or((t, "0"));
    let p: Vec<i64> = hms.split(':').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    if p.len() > 3 || p.iter().any(|&x| !(0..=1_000_000).contains(&x)) {
        return None;
    }
    let secs = p.iter().fold(0i64, |a, x| a * 60 + x);
    // kesir: 3 haneye kırp/tamamla (XML'de nanosaniye)
    let f: String = frac.chars().take_while(|c| c.is_ascii_digit()).chain(std::iter::repeat('0')).take(3).collect();
    Some(secs * 1000 + f.parse::<i64>().ok()?)
}

/// Matroska XML ya da OGM bölüm dosyasını okur.
pub fn parse(text: &str) -> Vec<Chapter> {
    let mut v = Vec::new();
    if text.contains("<ChapterAtom") {
        for atom in text.split("<ChapterAtom").skip(1) {
            let tag = |name: &str| atom.split(&format!("<{name}>")).nth(1).and_then(|r| r.split(&format!("</{name}>")).next()).map(|x| x.trim().to_string());
            if let Some(st) = tag("ChapterTimeStart").and_then(|t| parse_hms(&t)) {
                let title = tag("ChapterString").unwrap_or_default().replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&");
                v.push(Chapter { start_ms: st, title });
            }
        }
    } else {
        let mut times: std::collections::BTreeMap<String, i64> = Default::default();
        let mut names: std::collections::BTreeMap<String, String> = Default::default();
        for l in text.lines() {
            let Some((k, val)) = l.split_once('=') else { continue };
            let k = k.trim().to_ascii_uppercase();
            if let Some(n) = k.strip_prefix("CHAPTER") {
                if let Some(n) = n.strip_suffix("NAME") {
                    names.insert(n.to_string(), val.trim().to_string());
                } else if let Some(t) = parse_hms(val) {
                    times.insert(n.to_string(), t);
                }
            }
        }
        for (n, t) in times {
            v.push(Chapter { start_ms: t, title: names.get(&n).cloned().unwrap_or_default() });
        }
    }
    v.sort_by_key(|c| c.start_ms);
    v
}

/// Bölümleri belgeye yorum satırı olarak ekler (Efekt "chapter"; mevcut bölüm satırları silinir).
pub fn apply_to_script(s: &mut Script, ch: &[Chapter]) {
    s.entries.retain(|e| !matches!(e, Entry::Dialogue(d) if is_chapter(d)));
    let mut ds: Vec<Dialogue> = ch.iter().map(|c| Dialogue { comment: true, start_time: c.start_ms, end_time: c.start_ms, effect: EFFECT.into(), text: c.title.clone(), ..Default::default() }).collect();
    // olayların başına, zaman sırasıyla
    let at = s.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap_or(s.entries.len());
    for (k, d) in ds.drain(..).enumerate() {
        s.entries.insert(at + k, Entry::Dialogue(d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> Script {
        let mk = |s, effect: &str, t: &str, comment| Dialogue { start_time: s, end_time: s + 1000, effect: effect.into(), text: t.into(), comment, ..Default::default() };
        Script::from_dialogues(vec![mk(90_500, "chapter", "Açılış", true), mk(0, "Chapter", "{\\i1}Ön bölüm", true), mk(5000, "", "diyalog", false), mk(1_330_250, "chapter", "", false)])
    }

    #[test]
    fn collects_and_writes_formats() {
        let ch = from_script(&script());
        assert_eq!(ch, vec![Chapter { start_ms: 0, title: "Ön bölüm".into() }, Chapter { start_ms: 90_500, title: "Açılış".into() }, Chapter { start_ms: 1_330_250, title: "Bölüm 3".into() }]);
        let ogm = to_ogm(&ch);
        assert!(ogm.starts_with("CHAPTER01=00:00:00.000\nCHAPTER01NAME=Ön bölüm\nCHAPTER02=00:01:30.500\n"), "{ogm}");
        let xml = to_matroska_xml(&ch, "tur");
        assert!(xml.contains("<ChapterTimeStart>00:22:10.250000000</ChapterTimeStart>") && xml.contains("<ChapterLanguage>tur</ChapterLanguage>"));
        let ff = to_ffmetadata(&ch, 1_420_000);
        assert!(ff.contains("START=90500\nEND=1330250\ntitle=Açılış") && ff.contains("START=1330250\nEND=1420000"), "{ff}");
        // geri okuma
        assert_eq!(parse(&xml), ch);
        assert_eq!(parse(&ogm), ch);
    }

    #[test]
    fn garbage_does_not_panic() {
        for t in ["CHAPTER01=99999999999999:99:99.999", "CHAPTER01=:::", "CHAPTER1NAME=x", "<ChapterAtom><ChapterTimeStart>1:2:3:4:5.9</ChapterTimeStart>", "<ChapterAtom><ChapterTimeStart>00:00:01.5</ChapterTimeStart><ChapterString>a", "CHAPTER01=00:00:01.ü"] {
            let _ = parse(t);
        }
        assert_eq!(parse("CHAPTER01=00:00:01.5\nCHAPTER01NAME=x\n"), vec![Chapter { start_ms: 1500, title: "x".into() }]);
    }

    #[test]
    fn apply_replaces_chapter_lines() {
        let mut s = script();
        apply_to_script(&mut s, &[Chapter { start_ms: 1000, title: "A & <B>".into() }]);
        assert_eq!(from_script(&s), vec![Chapter { start_ms: 1000, title: "A & <B>".into() }]);
        assert_eq!(s.dialogues().filter(|d| !is_chapter(d)).count(), 1);
        let x = to_matroska_xml(&from_script(&s), "");
        assert!(x.contains("A &amp; &lt;B&gt;") && x.contains("und"));
        assert_eq!(parse(&x)[0].title, "A & <B>");
    }
}
