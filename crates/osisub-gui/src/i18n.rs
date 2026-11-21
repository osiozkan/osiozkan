//! Arayüz dili. Anahtar Türkçe metnin kendisidir: çevirisi olmayan metin Türkçe kalır,
//! böylece yeni bir düğme eklemek çeviriyi bozmaz (`i18n/en.jsonl` sonradan güncellenir).
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Tr = 0,
    En = 1,
}

static LANG: AtomicU8 = AtomicU8::new(0);

static EN: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| parse(include_str!("../i18n/en.jsonl")));

fn parse(src: &'static str) -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    for line in src.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok([k, v]) = serde_json::from_str::<[String; 2]>(line) {
            if !v.is_empty() {
                m.insert(&*Box::leak(k.into_boxed_str()), &*Box::leak(v.into_boxed_str()));
            }
        }
    }
    m
}

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::Tr }
}

/// Lang kodundan ("tr", "en"); bilinmeyen → Türkçe.
pub fn from_code(c: &str) -> Lang {
    if c.eq_ignore_ascii_case("en") { Lang::En } else { Lang::Tr }
}

pub fn code(l: Lang) -> &'static str {
    match l {
        Lang::Tr => "tr",
        Lang::En => "en",
    }
}

/// Arayüz metnini geçerli dile çevirir (çevirisi yoksa olduğu gibi).
pub fn tr(s: &'static str) -> &'static str {
    match lang() {
        Lang::Tr => s,
        Lang::En => EN.get(s).copied().unwrap_or(s),
    }
}

/// `format!` yerine: Türkçede `format!` ile aynı; İngilizcede şablonun çevirisi kullanılır.
/// Değerler, Türkçe şablonun düz parçaları biçimlenmiş metinde aranarak geri çıkarılır
/// (bağımsız değişkenler iki kez değerlendirilmez, `{ad}` yakalamaları çalışır). Çeviride
/// `{}` sırayla, `{0}`/`{1}` sırası değiştirilmiş yer tutuculardır.
macro_rules! trf {
    ($lit:literal $($rest:tt)*) => {
        $crate::i18n::fmt_tr($lit, format!($lit $($rest)*))
    };
}
pub(crate) use trf;

/// Şablonu düz parçalara ayırır (`{{`/`}}` kaçışları çözülür); parça sayısı = yer tutucu + 1.
fn split_template(t: &str) -> Option<Vec<String>> {
    let mut parts = vec![String::new()];
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => { it.next(); parts.last_mut()?.push('{'); }
            '}' if it.peek() == Some(&'}') => { it.next(); parts.last_mut()?.push('}'); }
            '{' => {
                loop { match it.next()? { '}' => break, _ => {} } }
                parts.push(String::new());
            }
            _ => parts.last_mut()?.push(c),
        }
    }
    Some(parts)
}

/// Biçimlenmiş metinden yer tutucu değerlerini çıkarır.
fn extract<'a>(parts: &[String], s: &'a str) -> Option<Vec<&'a str>> {
    let mut rest = s.strip_prefix(parts[0].as_str())?;
    let mut vals = Vec::new();
    let n = parts.len() - 1;
    for (i, lit) in parts.iter().enumerate().skip(1) {
        if i == n {
            vals.push(rest.strip_suffix(lit.as_str())?);
            rest = "";
        } else {
            if lit.is_empty() { return None; }
            let k = rest.find(lit.as_str())?;
            vals.push(&rest[..k]);
            rest = &rest[k + lit.len()..];
        }
    }
    rest.is_empty().then_some(vals)
}

fn fill(en: &str, vals: &[&str]) -> Option<String> {
    let mut out = String::new();
    let mut next = 0;
    let mut it = en.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => { it.next(); out.push('{'); }
            '}' if it.peek() == Some(&'}') => { it.next(); out.push('}'); }
            '{' => {
                let mut spec = String::new();
                loop { match it.next()? { '}' => break, ch => spec.push(ch) } }
                let i = match spec.parse::<usize>() { Ok(i) => i, Err(_) => { next += 1; next - 1 } };
                out.push_str(vals.get(i)?);
            }
            _ => out.push(c),
        }
    }
    Some(out)
}

pub fn fmt_tr(tmpl: &'static str, rendered: String) -> String {
    if lang() == Lang::Tr { return rendered; }
    let Some(en) = EN.get(tmpl) else { return rendered };
    split_template(tmpl)
        .and_then(|parts| extract(&parts, &rendered))
        .and_then(|vals| fill(en, &vals))
        .unwrap_or(rendered)
}

/// Şablonlu çeviri listesi: (Türkçe parçalar, İngilizce şablon). Düz metni 3 karakterden
/// kısa şablonlar (ör. "{} {}") her şeyle eşleşeceği için alınmaz.
static TEMPLATES: LazyLock<Vec<(Vec<String>, &'static str)>> = LazyLock::new(|| {
    let mut v: Vec<(Vec<String>, &'static str)> = EN.iter()
        .filter(|(k, _)| k.contains('{'))
        .filter_map(|(k, e)| split_template(k).filter(|p| p.len() > 1 && p.iter().map(|x| x.chars().count()).sum::<usize>() >= 3).map(|p| (p, *e)))
        .collect();
    // uzun düz metinli (daha belirgin) şablon önce
    v.sort_by_key(|(p, _)| std::cmp::Reverse(p.iter().map(|x| x.len()).sum::<usize>()));
    v
});

thread_local! {
    static MSG_CACHE: std::cell::RefCell<HashMap<String, String>> = Default::default();
}

/// Çalışma anında üretilmiş (ör. çekirdekten gelen QC) iletiyi çevirir: önce tam eşleşme,
/// sonra şablonlar. Sonuç önbelleklenir (her karede çizilen listeler için).
pub fn tr_msg(msg: &str) -> String {
    if lang() == Lang::Tr { return msg.to_string(); }
    if let Some(e) = EN.get(msg) { return e.to_string(); }
    MSG_CACHE.with(|c| {
        if let Some(r) = c.borrow().get(msg) { return r.clone(); }
        let r = TEMPLATES.iter()
            .find_map(|(parts, en)| extract(parts, msg).and_then(|v| fill(en, &v)))
            .unwrap_or_else(|| msg.to_string());
        let mut c = c.borrow_mut();
        if c.len() > 20_000 { c.clear(); }
        c.insert(msg.to_string(), r.clone());
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tablo_gecerli() {
        let src = include_str!("../i18n/en.jsonl");
        let n = src.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(EN.len(), n, "en.jsonl'de bozuk ya da yinelenen satır var");
        for (k, v) in EN.iter() {
            // ASS kaçışları ve kısayollar çeviride korunmalı
            for tok in ["\\N", "Ctrl+"] {
                assert_eq!(k.matches(tok).count(), v.matches(tok).count(), "{tok} kayboldu: {k} → {v}");
            }
        }
    }

    /// Kaynakta `tr("…")` ile sarılmış her metnin İngilizcesi olmalı (yeni metin eklerken
    /// `i18n/en.jsonl`'ye satır ekleyin).
    #[test]
    fn kaynaktaki_metinlerin_cevirisi_var() {
        fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() { walk(&p, out) } else if p.extension().map_or(false, |x| x == "rs") { out.push(p) }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut missing = Vec::new();
        for f in files.iter().filter(|f| !f.ends_with("i18n.rs")) {
            let src = std::fs::read_to_string(f).unwrap();
            let starts = src.match_indices("tr(\"").map(|(i, _)| (i, i + 4))
                .filter(|&(i, _)| !(i > 0 && (src.as_bytes()[i - 1].is_ascii_alphanumeric() || src.as_bytes()[i - 1] == b'_')))
                .chain(src.match_indices("trf!(\"").map(|(i, _)| (i, i + 6)));
            for (_, b) in starts {
                let mut v = String::new();
                let mut it = src[b..].chars();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' => match it.next() { Some('n') => v.push('\n'), Some('t') => v.push('\t'), Some(o) => v.push(o), None => {} },
                        _ => v.push(c),
                    }
                }
                match EN.get(v.as_str()) {
                    None => missing.push(format!("{}: {v}", f.file_name().unwrap().to_string_lossy())),
                    Some(en) => {
                        // yer tutucu sayısı korunmalı (sıra numaralı çeviri hariç)
                        let n = split_template(&v).map_or(0, |p| p.len());
                        let m = split_template(en).map_or(0, |p| p.len());
                        if n != m && !en.contains("{0}") { missing.push(format!("yer tutucu sayısı farklı: {v} → {en}")) }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "çevirisi olmayan {} metin:\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn sablon_cevirisi() {
        let parts = split_template("{} satır çevrildi ({e}) {{x}}").unwrap();
        assert_eq!(parts, vec!["", " satır çevrildi (", ") {x}"]);
        let v = extract(&parts, "12 satır çevrildi (zaman aşımı) {x}").unwrap();
        assert_eq!(v, vec!["12", "zaman aşımı"]);
        assert_eq!(fill("{} lines translated ({})", &v).unwrap(), "12 lines translated (zaman aşımı)");
        assert_eq!(fill("{1}: {0}", &v).unwrap(), "zaman aşımı: 12");
        assert_eq!(fill("{:.1} x", &["3.5"]).unwrap(), "3.5 x");
        // yan yana yer tutucu belirsiz → Türkçe kalır
        assert!(extract(&split_template("{}{} a").unwrap(), "12 a").is_none());
        let p2 = split_template("Satır {}").unwrap();
        assert_eq!(extract(&p2, "Satır 5").unwrap(), vec!["5"]);
        assert!(extract(&p2, "Başka 5").is_none());
    }

    #[test]
    fn eksik_ceviri_turkce_kalir() {
        set_lang(Lang::En);
        assert_eq!(tr("bu metnin çevirisi yok"), "bu metnin çevirisi yok");
        set_lang(Lang::Tr);
        assert_eq!(tr("Dosya"), "Dosya");
    }
}
