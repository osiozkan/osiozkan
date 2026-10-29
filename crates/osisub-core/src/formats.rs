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
        out.push_str(&format!("{},{}\n{}\n\n", f(d.start_time), f(d.end_time), srt::ass_text_to_srt(&d.text).replace(['<', '>'], "")));
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

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let i = tag.find(&format!("{}=\"", name))?;
    let r = &tag[i + name.len() + 2..];
    Some(&r[..r.find('"')?])
}

fn ttml_time(s: &str) -> Option<i64> {
    if let Some(v) = s.strip_suffix("ms") {
        return v.trim().parse::<f64>().ok().map(|x| x.round() as i64);
    }
    if let Some(v) = s.strip_suffix('s') {
        return v.trim().parse::<f64>().ok().map(|x| (x * 1000.0).round() as i64);
    }
    time::parse_generic(s)
}

/// Basit TTML/DFXP okuyucu: `<p begin end>` içeriği, `<br/>` satır sonu, `<span>` düz metne indirgenir.
pub fn parse_ttml(text: &str) -> Vec<Dialogue> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find("<p ") {
        let r = &rest[i..];
        let Some(tag_end) = r.find('>') else { break };
        let tag = &r[..tag_end];
        let Some(close) = r.find("</p>") else { break };
        let inner = &r[tag_end + 1..close];
        if let (Some(b), Some(e)) = (attr(tag, "begin").and_then(ttml_time), attr(tag, "end").and_then(ttml_time)) {
            let mut t = String::new();
            let mut s = inner;
            while let Some(k) = s.find('<') {
                t.push_str(&s[..k]);
                let Some(g) = s[k..].find('>') else { break };
                let tg = &s[k..k + g + 1];
                if tg.starts_with("<br") {
                    t.push_str("\\N");
                }
                s = &s[k + g + 1..];
            }
            t.push_str(s);
            out.push(Dialogue { start_time: b, end_time: e, text: xml_unescape(&t.split_whitespace().collect::<Vec<_>>().join(" ").replace("\\N ", "\\N")), ..Default::default() });
        }
        rest = &r[close + 4..];
    }
    out
}

pub fn write_ttml(script: &Script) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\" xml:lang=\"tr\">\n  <body>\n    <div>\n");
    for d in script.dialogues().filter(|d| !d.comment) {
        let body = xml_escape(&tags::visible_text(&d.text)).replace('\n', "<br/>");
        out.push_str(&format!("      <p begin=\"{}\" end=\"{}\">{}</p>\n", time::format_vtt(d.start_time), time::format_vtt(d.end_time), body));
    }
    out.push_str("    </div>\n  </body>\n</tt>\n");
    out
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
}
