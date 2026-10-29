//! SubRip (.srt) okuma/yazma ve ASS'ye dönüştürme.
use crate::ass::{Dialogue, Script};
use crate::time;

/// SRT metnini diyalog listesine çevirir. `<i>`, `<b>`, `<u>`, `<font color>` etiketleri ASS'ye çevrilir.
pub fn parse(text: &str) -> Vec<Dialogue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut lines = block.trim_matches('\n').lines().peekable();
        // Sıra numarası (olmayabilir).
        let mut tl = match lines.next() {
            Some(l) => l,
            None => continue,
        };
        if !tl.contains("-->") {
            tl = match lines.next() {
                Some(l) => l,
                None => continue,
            };
        }
        let Some((a, b)) = tl.split_once("-->") else { continue };
        // Bitişten sonraki koordinat/ayarları at.
        let b = b.trim().split_whitespace().next().unwrap_or("");
        let (Some(st), Some(en)) = (time::parse_generic(a), time::parse_generic(b)) else { continue };
        let body: Vec<&str> = lines.collect();
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

/// ASS metnini SRT biçimine indirger (etiketler `<i>` vb. olarak korunur).
pub fn ass_text_to_srt(s: &str) -> String {
    let mut out = String::new();
    let mut italic = false;
    let mut bold = false;
    let mut underline = false;
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
                    let on = |t: &str, k: &str| t.strip_prefix(k).map(|v| v.trim() != "0");
                    if let Some(v) = on(t, "i").filter(|_| !t.starts_with("iclip")) {
                        if v != italic { out.push_str(if v { "<i>" } else { "</i>" }); italic = v; }
                    } else if let Some(v) = on(t, "b").filter(|_| !t.starts_with("blur") && !t.starts_with("bord")) {
                        if v != bold { out.push_str(if v { "<b>" } else { "</b>" }); bold = v; }
                    } else if let Some(v) = on(t, "u") {
                        if v != underline { out.push_str(if v { "<u>" } else { "</u>" }); underline = v; }
                    }
                }
                while let Some(&(j, _)) = chars.peek() {
                    if j <= end { chars.next(); } else { break; }
                }
            }
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
    out
}

pub fn write(script: &Script) -> String {
    let mut out = String::new();
    let mut n = 0;
    for d in script.dialogues().filter(|d| !d.comment) {
        n += 1;
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", n, time::format_srt(d.start_time), time::format_srt(d.end_time), ass_text_to_srt(&d.text)));
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
}
