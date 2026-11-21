//! SubRip (.srt) okuma/yazma ve ASS'ye dönüştürme.
use crate::ass::{Dialogue, Script};
use crate::time;

/// SRT metnini diyalog listesine çevirir. `<i>`, `<b>`, `<u>`, `<font color>` etiketleri ASS'ye çevrilir.
pub fn parse(text: &str) -> Vec<Dialogue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    // Satır satır: her zaman satırı ("a --> b") yeni bir altyazı başlatır; hemen önündeki yalnız
    // rakamdan oluşan satır sıra numarasıdır. Böylece altyazı içindeki boş satırlar ve yalnızca
    // boşluk içeren ayraç satırları metni bölmez/birleştirmez (Aegisub gibi).
    let timing = |l: &str| -> Option<(i64, i64)> {
        let (a, b) = l.split_once("-->")?;
        let b = b.trim().split_whitespace().next().unwrap_or("");
        Some((time::parse_generic(a.trim())?, time::parse_generic(b)?))
    };
    let starts: Vec<(usize, i64, i64)> = lines.iter().enumerate().filter_map(|(i, l)| timing(l).map(|(a, b)| (i, a, b))).collect();
    let mut out = Vec::new();
    for (k, &(i, st, en)) in starts.iter().enumerate() {
        let mut stop = starts.get(k + 1).map_or(lines.len(), |n| n.0);
        // sonraki altyazının sıra numarası bu altyazıya ait değil
        if k + 1 < starts.len() && stop > i + 1 {
            let prev = lines[stop - 1].trim();
            // sıra numarası (SRT) ya da boş satırdan sonra gelen ipucu kimliği (WebVTT "cue1")
            let after_blank = stop >= i + 3 && lines[stop - 2].trim().is_empty();
            if !prev.is_empty() && (prev.chars().all(|c| c.is_ascii_digit()) || after_blank) {
                stop -= 1;
            }
        }
        let mut body: Vec<&str> = lines[i + 1..stop].iter().map(|l| if l.trim().is_empty() { "" } else { *l }).collect();
        while body.last().map_or(false, |l| l.is_empty()) { body.pop(); }
        while body.first().map_or(false, |l| l.is_empty()) { body.remove(0); }
        let text = srt_markup_to_ass(&body.join("\n"));
        out.push(Dialogue { start_time: st, end_time: en, text, ..Default::default() });
    }
    out
}

pub fn to_script(text: &str) -> Script {
    Script::from_dialogues(parse(text))
}

fn srt_markup_to_ass(s: &str) -> String {
    let s = s.replace('\n', "\\N");
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let tag = rest[i + 1..i + j].trim().to_ascii_lowercase();
        let known = matches!(tag.as_str(), "i" | "/i" | "b" | "/b" | "u" | "/u" | "s" | "/s" | "/font" | "font")
            || tag.starts_with("font ");
        if !known {
            // etiket değil ("<3", "a < b"): olduğu gibi bırak
            out.push('<');
            rest = &rest[i + 1..];
            continue;
        }
        match tag.as_str() {
            "i" => out.push_str("{\\i1}"),
            "/i" => out.push_str("{\\i0}"),
            "b" => out.push_str("{\\b1}"),
            "/b" => out.push_str("{\\b0}"),
            "u" => out.push_str("{\\u1}"),
            "/u" => out.push_str("{\\u0}"),
            "s" => out.push_str("{\\s1}"),
            "/s" => out.push_str("{\\s0}"),
            "/font" => out.push_str("{\\c}"),
            t if t.starts_with("font") => {
                if let Some(k) = t.find("color=") {
                    let v = t[k + 6..].trim_matches(|c| c == '"' || c == '\'' || c == ' ');
                    let v = v.split(|c: char| c == '"' || c == '\'' || c == ' ').next().unwrap_or("");
                    if let Some(hex) = v.strip_prefix('#') {
                        if hex.len() == 6 {
                            out.push_str(&format!("{{\\c&H{}{}{}&}}", &hex[4..6].to_uppercase(), &hex[2..4].to_uppercase(), &hex[0..2].to_uppercase()));
                        }
                    }
                }
            }
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// `ass_text_to_srt` çıktısının başındaki `{\\anN}` ön ekini ayırır (VTT/SBV için).
pub fn take_an_prefix(s: &str) -> (Option<u8>, &str) {
    if let Some(r) = s.strip_prefix("{\\an") {
        if let Some(d) = r.chars().next().and_then(|c| c.to_digit(10)) {
            if r[1..].starts_with('}') {
                return (Some(d as u8), &r[2..]);
            }
        }
    }
    (None, s)
}

/// ASS metnini SRT biçimine indirger (etiketler `<i>` vb. olarak korunur).
pub fn ass_text_to_srt(s: &str) -> String {
    let mut out = String::new();
    let mut italic = false;
    let mut bold = false;
    let mut underline = false;
    let mut an: Option<u8> = None;
    // `\p1` çizim kipi: vektör komutları metin değildir (tabela şekli SRT'de anlamsız harf olurdu)
    let mut drawing = false;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' => {
                // Kapanmamış blok: süslü parantezi düz metin say.
                let Some(rel) = s[i..].find('}') else {
                    out.push(c);
                    continue;
                };
                let end = i + rel;
                let block = &s[i + 1..end];
                for t in block.split('\\').skip(1) {
                    // Yalnızca sayısal değerli `\i1`, `\b700`… (`\be`, `\blur`, `\iclip` değil)
                    let on = |t: &str, k: &str| {
                        t.strip_prefix(k).map(str::trim).filter(|v| v.chars().all(|c| c.is_ascii_digit())).map(|v| !v.is_empty() && v != "0" && v != "400")
                    };
                    if let Some(p) = t.strip_prefix('p').map(str::trim).filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit())) {
                        drawing = p != "0";
                    } else if let Some(v) = on(t, "i") {
                        if v != italic { out.push_str(if v { "<i>" } else { "</i>" }); italic = v; }
                    } else if let Some(v) = on(t, "b") {
                        if v != bold { out.push_str(if v { "<b>" } else { "</b>" }); bold = v; }
                    } else if let Some(v) = on(t, "u") {
                        if v != underline { out.push_str(if v { "<u>" } else { "</u>" }); underline = v; }
                    } else if an.is_none() {
                        // Konum: `{\an8}` SRT'de yaygın kuraldır (Subtitle Edit, oynatıcılar okur).
                        an = t.strip_prefix("an").and_then(|v| v.trim().parse::<u8>().ok()).filter(|v| (1..=9).contains(v) && *v != 2);
                    }
                }
                while let Some(&(j, _)) = chars.peek() {
                    if j <= end { chars.next(); } else { break; }
                }
            }
            _ if drawing => {}
            '\\' => match chars.peek().map(|&(_, c)| c) {
                Some('N') | Some('n') => { chars.next(); out.push('\n'); }
                Some('h') => { chars.next(); out.push(' '); }
                _ => out.push('\\'),
            },
            c => out.push(c),
        }
    }
    if italic { out.push_str("</i>"); }
    if bold { out.push_str("</b>"); }
    if underline { out.push_str("</u>"); }
    match an {
        Some(a) => format!("{{\\an{}}}{}", a, out),
        None => out,
    }
}

/// `ass_text_to_srt` çıktısında görünür metin yok mu (yalnızca biçim etiketleri / konum)?
pub fn srt_is_blank(t: &str) -> bool {
    let t = take_an_prefix(t).1;
    ["<i>", "</i>", "<b>", "</b>", "<u>", "</u>"].iter().fold(t.to_string(), |a, tag| a.replace(tag, "")).trim().is_empty()
}

pub fn write(script: &Script) -> String {
    let mut out = String::new();
    let mut n = 0;
    for d in script.dialogues().filter(|d| !d.comment) {
        let t = ass_text_to_srt(&d.text);
        if !d.text.trim().is_empty() && srt_is_blank(&t) {
            continue; // yalnızca çizimden (tabela şekli) oluşan satır
        }
        n += 1;
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n, time::format_srt(d.start_time), time::format_srt(d.end_time), t));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_roundtrip() {
        let d = parse("1\n00:00:01,000 --> 00:00:02,500\n<i>Selam</i>\nDünya\n\n2\n00:00:03,000 --> 00:00:04,000\nx\n");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].text, "{\\i1}Selam{\\i0}\\NDünya");
        let s = Script::from_dialogues(d);
        let o = write(&s);
        assert!(o.contains("<i>Selam</i>\nDünya"));
    }

    #[test]
    fn position_kept_and_be_is_not_bold() {
        assert_eq!(ass_text_to_srt("{\\an8\\be1\\blur2}Üst {\\b1}k{\\b0}"), "{\\an8}Üst <b>k</b>");
        assert_eq!(ass_text_to_srt("{\\an2\\iclip(0,0,1,1)}a"), "a");
        let d = parse("1\n00:00:01,000 --> 00:00:02,000\n{\\an8}Üst\n");
        assert_eq!(d[0].text, "{\\an8}Üst");
        assert!(write(&Script::from_dialogues(d)).contains("{\\an8}Üst"));
    }

    #[test]
    fn inceleme_duzeltmeleri() {
        // yalnızca boşluk içeren ayraç satırı altyazıları birleştirmez
        let d = parse("1\n00:00:01,000 --> 00:00:02,000\nA\n \n2\n00:00:03,000 --> 00:00:04,000\nB\n");
        assert_eq!(d.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        // altyazı içindeki boş satır metni kesmez (yazıcımız a\N\Nb'yi böyle yazar)
        let d = parse("1\n00:00:01,000 --> 00:00:02,000\nA\n\nA2\n\n2\n00:00:03,000 --> 00:00:04,000\nB\n");
        assert_eq!(d[0].text, "A\\N\\NA2");
        assert_eq!(d[1].text, "B");
        let s = Script::from_dialogues(vec![Dialogue { start_time: 0, end_time: 1000, text: "a\\N\\Nb".into(), ..Default::default() }]);
        assert_eq!(parse(&write(&s))[0].text, "a\\N\\Nb");
        // bilinmeyen "etiket" düz metindir
        assert_eq!(parse("1\n00:00:01,000 --> 00:00:02,000\nI <3 you <i>so</i> much\n")[0].text, "I <3 you {\\i1}so{\\i0} much");
        assert_eq!(parse("1\n00:00:01,000 --> 00:00:02,000\nif a < b and c > d\n")[0].text, "if a < b and c > d");
        // çizim satırı SRT'ye yazılmaz, çizimden sonraki metin kalır
        let s = Script::from_dialogues(vec![
            Dialogue { start_time: 0, end_time: 1000, text: "{\\an7\\pos(0,0)\\p1}m 0 0 l 100 0 100 100{\\p0}".into(), ..Default::default() },
            Dialogue { start_time: 0, end_time: 1000, text: "{\\p1}m 0 0 l 1 1{\\p0}Yazı".into(), ..Default::default() },
        ]);
        let w = write(&s);
        assert!(!w.contains("m 0 0"), "{w}");
        assert_eq!(parse(&w).len(), 1);
        assert_eq!(parse(&w)[0].text, "Yazı");
    }
}