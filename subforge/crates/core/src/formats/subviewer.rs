//! SubViewer 2.0 `.sub`: `[INFORMATION]` header, then `hh:mm:ss.cc,hh:mm:ss.cc` + text with `[br]` breaks.
use super::sbv::parse_range;
use super::{new_script, plain_lines, sorted_events};
use crate::ass::{Event, Script};
use crate::time::Ms;

pub fn parse_subviewer(text: &str) -> Script {
    let mut script = new_script();
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut cur: Option<(Ms, Ms, Vec<String>)> = None;
    let mut in_info = false;
    let flush = |cur: &mut Option<(Ms, Ms, Vec<String>)>, script: &mut Script| {
        if let Some((s, e, body)) = cur.take() {
            let t = body.join("[br]").replace("[br]", "\\N").replace('{', "(").replace('}', ")");
            script.events.push(Event::new(s, e, "Default", &t));
        }
    };
    for l in text.split('\n') {
        let t = l.trim();
        if t.eq_ignore_ascii_case("[INFORMATION]") {
            in_info = true;
            continue;
        }
        if t.eq_ignore_ascii_case("[END INFORMATION]") {
            in_info = false;
            continue;
        }
        if in_info {
            if let Some(v) = t.strip_prefix("[TITLE]").filter(|v| !v.trim().is_empty()) {
                script.info_set("Title", v.trim());
            }
            continue;
        }
        if let Some((s, e)) = parse_range(t) {
            flush(&mut cur, &mut script);
            cur = Some((s, e, vec![]));
        } else if t.is_empty() {
            flush(&mut cur, &mut script);
        } else if t.starts_with("[SUBTITLE]") || t.starts_with("[COLF]") {
            continue;
        } else if let Some((_, _, body)) = cur.as_mut() {
            body.push(t.to_string());
        }
    }
    flush(&mut cur, &mut script);
    script
}

fn stamp(ms: Ms) -> String {
    let cs = (ms.max(0) + 5) / 10;
    format!("{:02}:{:02}:{:02}.{:02}", cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100)
}

pub fn write_subviewer(script: &Script) -> String {
    let title = script.info_get("Title").unwrap_or("");
    let mut o = format!(
        "[INFORMATION]\r\n[TITLE]{title}\r\n[AUTHOR]\r\n[SOURCE]\r\n[PRG]\r\n[FILEPATH]\r\n[DELAY]0\r\n[CD TRACK]0\r\n[COMMENT]\r\n[END INFORMATION]\r\n[SUBTITLE]\r\n[COLF]&HFFFFFF,[STYLE]no,[SIZE]18,[FONT]Arial\r\n"
    );
    for e in sorted_events(script) {
        let lines = plain_lines(e);
        if lines.is_empty() {
            continue;
        }
        o.push_str(&format!("{},{}\r\n{}\r\n\r\n", stamp(e.start), stamp(e.end), lines.join("[br]")));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subviewer_roundtrip() {
        let src = "[INFORMATION]\r\n[TITLE]Deneme\r\n[AUTHOR]x\r\n[END INFORMATION]\r\n[SUBTITLE]\r\n[COLF]&HFFFFFF,[STYLE]no,[SIZE]18,[FONT]Arial\r\n00:00:01.50,00:00:03.00\r\nMerhaba[br]İkinci ışık\r\n\r\n01:00:00.05,01:00:02.00\r\nğüşöç\r\n";
        let s = parse_subviewer(src);
        assert_eq!(s.info_get("Title"), Some("Deneme"));
        assert_eq!(s.events.len(), 2);
        assert_eq!((s.events[0].start, s.events[0].end), (1500, 3000));
        assert_eq!(s.events[0].text, "Merhaba\\Nİkinci ışık");
        assert_eq!(s.events[1].start, 3_600_050);
        let out = write_subviewer(&s);
        assert!(out.contains("00:00:01.50,00:00:03.00\r\nMerhaba[br]İkinci ışık"));
        assert_eq!(parse_subviewer(&out).events, s.events);
    }
}
