//! Ek altyazı biçimleri: LRC (gelişmiş sözcük zamanlı dahil), MicroDVD, SBV, TTML, TXT, CSV.
use crate::ass::{Dialogue, Script};
use crate::{karaoke, srt, tags, time};

fn ass_text_from_plain(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\n', "\\N")
}

// ----------------------------------------------------------------------- LRC

fn parse_lrc_stamp(s: &str) -> Option<i64> {
    // mm:ss.xx veya mm:ss:xx veya mm:ss
    let s = s.trim();
    let (m, rest) = s.split_once(':')?;
    let m: i64 = m.trim().parse().ok()?;
    let rest = rest.replace(':', ".");
    let (sec, frac) = match rest.split_once('.') {
        Some((a, b)) => (a, b),
        None => (rest.as_str(), ""),
    };
    let sec: i64 = sec.parse().ok()?;
    let ms = if frac.is_empty() { 0 } else { format!("{:0<3}", &frac[..frac.len().min(3)]).parse::<i64>().ok()? };
    Some((m * 60 + sec) * 1000 + ms)
}

/// LRC oku. Gelişmiş `<mm:ss.xx>` sözcük zamanları `\kf` etiketlerine çevrilir.
pub fn parse_lrc(text: &str) -> Vec<Dialogue> {
    let mut items: Vec<(i64, String)> = vec![];
    for line in text.lines() {
        let mut rest = line.trim();
        let mut stamps = vec![];
        while rest.starts_with('[') {
            let Some(e) = rest.find(']') else { break };
            match parse_lrc_stamp(&rest[1..e]) {
                Some(t) => stamps.push(t),
                None => break, // [ar:..] gibi üstveri
            }
            rest = rest[e + 1..].trim_start();
        }
        if stamps.is_empty() || rest.is_empty() && stamps.is_empty() {
            continue;
        }
        for t in stamps {
            items.push((t, rest.to_string()));
        }
    }
    items.sort_by_key(|x| x.0);
    let mut out = vec![];
    for (i, (start, raw)) in items.iter().enumerate() {
        let end = items.get(i + 1).map(|n| n.0).unwrap_or(start + 3000);
        if raw.is_empty() {
            continue;
        }
        out.push(Dialogue { start_time: *start, end_time: end.max(start + 1), text: lrc_words_to_ass(raw, *start, end), ..Default::default() });
    }
    out
}

fn lrc_words_to_ass(raw: &str, start: i64, end: i64) -> String {
    if !raw.contains('<') {
        return ass_text_from_plain(raw);
    }
    // Parçala: <stamp>kelime<stamp>kelime ...
    let mut marks: Vec<(i64, String)> = vec![];
    let mut rest = raw;
    let mut cur_t = start;
    let mut lead = String::new();
    while let Some(i) = rest.find('<') {
        let Some(j) = rest[i..].find('>') else { break };
        let seg = &rest[..i];
        if marks.is_empty() && cur_t == start && lead.is_empty() {
            lead = seg.to_string();
        } else if let Some(last) = marks.last_mut() {
            last.1.push_str(seg);
        }
        if let Some(t) = parse_lrc_stamp(&rest[i + 1..i + j]) {
            marks.push((t, String::new()));
            cur_t = t;
        }
        rest = &rest[i + j + 1..];
    }
    if let Some(last) = marks.last_mut() {
        last.1.push_str(rest);
    }
    let mut syls = vec![];
    if !lead.trim().is_empty() {
        syls.push(karaoke::Syl { dur: 0, kind: "kf".into(), text: lead });
    }
    for (i, (t, w)) in marks.iter().enumerate() {
        let next = marks.get(i + 1).map(|m| m.0).unwrap_or(end);
        syls.push(karaoke::Syl { dur: ((next - t).max(0) as f64 / 10.0).round() as i64, kind: "kf".into(), text: w.clone() });
    }
    karaoke::build(&syls)
}

pub fn write_lrc(script: &Script) -> String {
    let stamp = |ms: i64| format!("{:02}:{:02}.{:02}", ms / 60000, ms / 1000 % 60, ms / 10 % 100);
    let mut out = String::new();
    for d in script.dialogues().filter(|d| !d.comment) {
        let syls = karaoke::parse(&d.text);
        if syls.iter().any(|s| s.dur > 0) {
            let mut t = d.start_time;
            let mut line = format!("[{}]", stamp(d.start_time));
            for s in &syls {
                line.push_str(&format!("<{}>{}", stamp(t), tags::visible_text(&s.text).replace('\n', " ")));
                t += s.dur * 10;
            }
            out.push_str(&line);
        } else {
            out.push_str(&format!("[{}]{}", stamp(d.start_time), tags::visible_text(&d.text).replace('\n', " ")));
        }
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------------ MicroDVD

pub fn parse_microdvd(text: &str, default_fps: f64) -> Vec<Dialogue> {
    let mut fps = default_fps;
    let mut out = vec![];
    for line in text.lines() {
        let line = line.trim();
        let Some(r) = line.strip_prefix('{') else { continue };
        let Some((a, r)) = r.split_once('}') else { continue };
        let Some(r) = r.strip_prefix('{') else { continue };
        let Some((b, body)) = r.split_once('}') else { continue };
        let (Ok(a), Ok(b)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>()) else { continue };
        if a == 1 && b == 1 {
            if let Ok(f) = body.trim().parse::<f64>() {
                fps = f;
                continue;
            }
        }
        let to_ms = |f: i64| (f as f64 * 1000.0 / fps).round() as i64;
        let mut t = body.replace('|', "\\N");
        // {y:i} → italik
        if let Some(rest) = t.strip_prefix("{y:i}") {
            t = format!("{{\\i1}}{}", rest);
        }
        out.push(Dialogue { start_time: to_ms(a), end_time: to_ms(b), text: t, ..Default::default() });
    }
    out
}

pub fn write_microdvd(script: &Script, fps: f64) -> String {
    let mut out = format!("{{1}}{{1}}{}\n", fps);
    for d in script.dialogues().filter(|d| !d.comment) {
        let f = |ms: i64| (ms as f64 * fps / 1000.0).round() as i64;
        out.push_str(&format!("{{{}}}{{{}}}{}\n", f(d.start_time), f(d.end_time), tags::visible_text(&d.text).replace('\n', "|")));
    }
    out
}

// ----------------------------------------------------------------------- SBV

pub fn parse_sbv(text: &str) -> Vec<Dialogue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let mut out = vec![];
    for block in text.split("\n\n") {
        let mut lines = block.trim().lines();
        let Some(tl) = lines.next() else { continue };
        let Some((a, b)) = tl.split_once(',') else { continue };
        let (Some(s), Some(e)) = (time::parse_generic(a), time::parse_generic(b)) else { continue };
        let body: Vec<&str> = lines.collect();
        out.push(Dialogue { start_time: s, end_time: e, text: ass_text_from_plain(&body.join("\n")), ..Default::default() });
    }
    out
}

pub fn write_sbv(script: &Script) -> String {
    let f = |ms: i64| format!("{}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let mut out = String::new();
    for d in script.dialogues().filter(|d| !d.comment) {
        out.push_str(&format!("{},{}\n{}\n\n", f(d.start_time), f(d.end_time), srt::take_an_prefix(&srt::ass_text_to_srt(&d.text)).1.replace(['<', '>'], "")));
    }
    out
}

// ---------------------------------------------------------------------- TTML

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// TTML/DFXP/iTT/EBU-TT okuyucu (bkz. `crate::ttml`).
pub fn parse_ttml(text: &str) -> Vec<Dialogue> {
    crate::ttml::parse(text)
}

pub fn write_ttml(script: &Script) -> String {
    crate::ttml::write(script, crate::ttml::Profile::Ttml)
}

// ------------------------------------------------------------------ TXT / CSV

pub fn write_txt(script: &Script) -> String {
    script.dialogues().filter(|d| !d.comment).map(|d| tags::visible_text(&d.text).replace('\n', " ")).collect::<Vec<_>>().join("\n") + "\n"
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

pub fn write_csv(script: &Script) -> String {
    let mut out = String::from("Başlangıç,Bitiş,Stil,Konuşan,Metin\n");
    for d in script.dialogues().filter(|d| !d.comment) {
        out.push_str(&format!("{},{},{},{},{}\n", time::format_ass(d.start_time), time::format_ass(d.end_time), csv_field(&d.style), csv_field(&d.actor), csv_field(&tags::visible_text(&d.text).replace('\n', " / "))));
    }
    out
}

// ----------------------------------------------------------------- SubViewer 2

/// SubViewer 2.0 (`.sub`, `[INFORMATION]` başlıklı): `00:00:01.00,00:00:03.00` + metin (`[br]` satır sonu).
pub fn parse_subviewer(text: &str) -> Vec<Dialogue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let mut out = vec![];
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        let l = l.trim();
        let Some((a, b)) = l.split_once(',') else { continue };
        if !a.starts_with(|c: char| c.is_ascii_digit()) || a.matches(':').count() != 2 {
            continue;
        }
        let (Some(s), Some(e)) = (time::parse_generic(a), time::parse_generic(b)) else { continue };
        let mut body = vec![];
        while let Some(n) = lines.peek() {
            if n.trim().is_empty() {
                break;
            }
            body.push(lines.next().unwrap().trim().to_string());
        }
        let t = body.join("[br]").replace("[br]", "\\N").replace("[BR]", "\\N");
        out.push(Dialogue { start_time: s, end_time: e, text: t, ..Default::default() });
    }
    out
}

/// SubViewer 2.0 yazar (metin düz; etiketler atılır).
pub fn write_subviewer(script: &Script) -> String {
    let f = |ms: i64| format!("{:02}:{:02}:{:02}.{:02}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000 / 10);
    let mut out = String::from("[INFORMATION]\n[TITLE]\n[AUTHOR]\n[SOURCE]\n[PRG]\n[FILEPATH]\n[DELAY]0\n[CD TRACK]0\n[COMMENT]\n[END INFORMATION]\n[SUBTITLE]\n[COLF]&HFFFFFF,[STYLE]bd,[SIZE]18,[FONT]Arial\n");
    for d in script.dialogues().filter(|d| !d.comment) {
        out.push_str(&format!("{},{}\n{}\n\n", f(d.start_time), f(d.end_time), tags::visible_text(&d.text).replace('\n', "[br]")));
    }
    out
}

// ------------------------------------------------------------------------ SAMI

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// SAMI (`.smi`/`.sami`): `<SYNC Start=ms>` blokları; boş (`&nbsp;`) blok önceki satırı bitirir.
/// Birden çok dil sınıfı varsa ilk görülen sınıf alınır.
pub fn parse_sami(text: &str) -> Vec<Dialogue> {
    let lower = text.to_ascii_lowercase();
    let mut syncs: Vec<(usize, i64)> = vec![];
    let mut from = 0;
    while let Some(i) = lower[from..].find("<sync") {
        let at = from + i;
        let end = lower[at..].find('>').map(|e| at + e + 1).unwrap_or(lower.len());
        let tag = &lower[at..end];
        let start = tag.find("start").and_then(|k| {
            let r = tag[k + 5..].trim_start().strip_prefix('=')?.trim_start().trim_start_matches(['"', '\'']);
            let digits: String = r.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<i64>().ok()
        });
        if let Some(st) = start {
            syncs.push((end, st));
        }
        from = end;
    }
    let mut class: Option<String> = None;
    let mut out: Vec<Dialogue> = vec![];
    for (k, &(body_start, st)) in syncs.iter().enumerate() {
        let body_end = syncs.get(k + 1).map(|n| lower[..n.0].rfind("<sync").unwrap_or(n.0)).unwrap_or_else(|| lower.find("</body").unwrap_or(text.len()));
        let raw = &text[body_start..body_end.max(body_start)];
        let rl = raw.to_ascii_lowercase();
        // sınıf: <p class=XX>
        if let Some(ci) = rl.find("class") {
            let c: String = rl[ci + 5..].trim_start().trim_start_matches('=').trim_start().trim_start_matches(['"', '\'']).chars().take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
            match &class {
                None => class = Some(c),
                Some(x) if *x != c => continue,
                _ => {}
            }
        }
        let with_breaks = {
            let mut t = String::new();
            let mut i = 0;
            while i < raw.len() {
                if rl[i..].starts_with("<br") {
                    t.push('\n');
                    i += rl[i..].find('>').map(|e| e + 1).unwrap_or(rl.len() - i);
                } else {
                    let ch = raw[i..].chars().next().unwrap();
                    t.push(ch);
                    i += ch.len_utf8();
                }
            }
            t
        };
        let plain = xml_unescape(&strip_html(&with_breaks).replace("&nbsp;", " ").replace("&NBSP;", " "));
        let lines: Vec<&str> = plain.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        if let Some(prev) = out.last_mut() {
            if prev.end_time < 0 {
                prev.end_time = st;
            }
        }
        if !lines.is_empty() {
            out.push(Dialogue { start_time: st, end_time: -1, text: lines.join("\\N"), ..Default::default() });
        }
    }
    if let Some(last) = out.last_mut() {
        if last.end_time < 0 {
            last.end_time = last.start_time + 3000;
        }
    }
    out
}

/// SAMI yazar: her satır bir SYNC, ardından boşluk varsa `&nbsp;` bitiş SYNC'i.
pub fn write_sami(script: &Script) -> String {
    let mut out = String::from("<SAMI>\n<HEAD>\n<TITLE></TITLE>\n<STYLE TYPE=\"text/css\">\n<!--\nP { margin-left: 8pt; margin-right: 8pt; text-align: center; font-family: Arial; }\n.TRCC { Name: Turkish; lang: tr-TR; SAMIType: CC; }\n-->\n</STYLE>\n</HEAD>\n<BODY>\n");
    let ds: Vec<&Dialogue> = script.dialogues().filter(|d| !d.comment).collect();
    for (i, d) in ds.iter().enumerate() {
        let txt = xml_escape(&tags::visible_text(&d.text)).replace('\n', "<br>");
        out.push_str(&format!("<SYNC Start={}><P Class=TRCC>{}\n", d.start_time, txt));
        let next = ds.get(i + 1).map(|n| n.start_time);
        if next.map_or(true, |n| n > d.end_time) {
            out.push_str(&format!("<SYNC Start={}><P Class=TRCC>&nbsp;\n", d.end_time));
        }
    }
    out.push_str("</BODY>\n</SAMI>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_plain_and_enhanced() {
        let d = parse_lrc("[ar:Sanatçı]\n[00:01.00]Merhaba dünya\n[00:04.50][00:09.00]Tekrar\n[00:12.00]<00:12.00>ka<00:12.50>ra<00:13.00>oke\n");
        assert_eq!(d.len(), 4);
        assert_eq!((d[0].start_time, d[0].end_time), (1000, 4500));
        assert_eq!(d[2].start_time, 9000);
        let k = karaoke::parse(&d[3].text);
        assert_eq!(k.iter().map(|s| (s.dur, s.text.as_str())).collect::<Vec<_>>(), vec![(50, "ka"), (50, "ra"), (200, "oke")]);
        let s = Script::from_dialogues(d);
        let w = write_lrc(&s);
        assert!(w.contains("[00:12.00]<00:12.00>ka<00:12.50>ra<00:13.00>oke"), "{}", w);
    }

    #[test]
    fn microdvd_roundtrip() {
        let d = parse_microdvd("{1}{1}25\n{25}{75}Selam|Dünya\n", 23.976);
        assert_eq!((d[0].start_time, d[0].end_time, d[0].text.as_str()), (1000, 3000, "Selam\\NDünya"));
        let w = write_microdvd(&Script::from_dialogues(d), 25.0);
        assert!(w.contains("{25}{75}Selam|Dünya"));
    }

    #[test]
    fn sbv_ttml() {
        let d = parse_sbv("0:00:01.000,0:00:02.500\nMerhaba\nDünya\n\n0:00:03.000,0:00:04.000\nİkinci\n");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].text, "Merhaba\\NDünya");
        let t = write_ttml(&Script::from_dialogues(d));
        let back = parse_ttml(&t);
        assert_eq!(back.len(), 2);
        assert_eq!((back[0].start_time, back[0].end_time, back[0].text.as_str()), (1000, 2500, "Merhaba\\NDünya"));
        let t2 = parse_ttml("<p begin=\"1.5s\" end=\"3s\">a &amp; b</p>");
        assert_eq!((t2[0].start_time, t2[0].text.as_str()), (1500, "a & b"));
    }

    #[test]
    fn csv_txt() {
        let s = Script::from_dialogues(vec![Dialogue { text: "a, \"b\"".into(), ..Default::default() }]);
        assert!(write_csv(&s).contains("\"a, \"\"b\"\"\""));
        assert_eq!(write_txt(&s), "a, \"b\"\n");
    }

    #[test]
    fn subviewer_roundtrip() {
        let src = "[INFORMATION]\n[TITLE]x\n[END INFORMATION]\n[SUBTITLE]\n00:00:01.00,00:00:03.50\nMerhaba[br]dünya\n\n00:00:04.00,00:00:05.00\nİkinci\n";
        let d = parse_subviewer(src);
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].start_time, d[0].end_time, d[0].text.as_str()), (1000, 3500, "Merhaba\\Ndünya"));
        let back = parse_subviewer(&write_subviewer(&Script::from_dialogues(d.clone())));
        assert_eq!(back.iter().map(|x| (x.start_time, x.end_time, x.text.clone())).collect::<Vec<_>>(), d.iter().map(|x| (x.start_time, x.end_time, x.text.clone())).collect::<Vec<_>>());
    }

    #[test]
    fn sami_roundtrip() {
        let src = "<SAMI><BODY>\n<SYNC Start=1000><P Class=TRCC>Merhaba<br>dünya &amp; <i>sen</i>\n<SYNC Start=3000><P Class=TRCC>&nbsp;\n<SYNC Start=4000><P Class=ENCC>Hello\n<SYNC Start=4000><P Class=TRCC>İkinci\n<SYNC Start=6000><P Class=TRCC>Üçüncü\n</BODY></SAMI>";
        let d = parse_sami(src);
        assert_eq!(d.iter().map(|x| (x.start_time, x.end_time, x.text.as_str())).collect::<Vec<_>>(), vec![(1000, 3000, "Merhaba\\Ndünya & sen"), (4000, 6000, "İkinci"), (6000, 9000, "Üçüncü")]);
        let back = parse_sami(&write_sami(&Script::from_dialogues(d.clone())));
        assert_eq!(back.iter().map(|x| (x.start_time, x.end_time, x.text.clone())).collect::<Vec<_>>(), d.iter().map(|x| (x.start_time, x.end_time, x.text.clone())).collect::<Vec<_>>());
    }
}
