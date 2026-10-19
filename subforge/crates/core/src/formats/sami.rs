//! SAMI `.smi`: `<SYNC Start=ms><P Class=XX>text`, an empty (`&nbsp;`) sync clears the previous line.
use super::markup::{escape_entities, event_to_html, html_to_ass, HtmlOpts};
use super::{ensure_style, new_script, sorted_events};
use crate::ass::{Event, Script};
use crate::time::Ms;
use regex::Regex;
use std::sync::OnceLock;

const LAST_LINE_MS: Ms = 3000;

fn sync_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<sync\s[^>]*?start\s*=\s*["']?(\d+)["']?[^>]*>"#).unwrap())
}

fn class_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<p\s[^>]*?class\s*=\s*["']?([A-Za-z0-9_-]+)["']?[^>]*>"#).unwrap())
}

fn p_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)</?p(\s[^>]*)?>").unwrap())
}

/// Parse SAMI. The first `Class` becomes style `Default`; further classes (other languages) get their own style.
pub fn parse_sami(text: &str) -> Script {
    let mut script = new_script();
    let text = text.trim_start_matches('\u{feff}');
    if let Some(t) = Regex::new(r"(?is)<title>(.*?)</title>").unwrap().captures(text) {
        let t = t[1].trim();
        if !t.is_empty() {
            script.info_set("Title", t);
        }
    }
    let body_start = Regex::new(r"(?i)<body[^>]*>").unwrap().find(text).map(|m| m.end()).unwrap_or(0);
    let body = &text[body_start..];
    let body = body.split("</BODY>").next().unwrap_or(body);
    let syncs: Vec<_> = sync_re().captures_iter(body).collect();
    let mut classes: Vec<String> = vec![];
    let mut open: Vec<(String, usize)> = vec![]; // class -> event index
    for (i, c) in syncs.iter().enumerate() {
        let t: Ms = c[1].parse().unwrap_or(0);
        let m = c.get(0).unwrap();
        let end = syncs.get(i + 1).map(|n| n.get(0).unwrap().start()).unwrap_or(body.len());
        let chunk = &body[m.end()..end];
        let class = class_re().captures(chunk).map(|c| c[1].to_string()).unwrap_or_default();
        if !classes.contains(&class) {
            classes.push(class.clone());
        }
        if let Some(pos) = open.iter().position(|(c, _)| *c == class) {
            let (_, idx) = open.remove(pos);
            script.events[idx].end = t.max(script.events[idx].start);
        }
        let inner = p_re().replace_all(chunk, "");
        let flat = Regex::new(r"(?i)<br\s*/?>").unwrap().replace_all(&inner, "\u{1}").replace(['\r', '\n'], " ");
        let flat = Regex::new(r"\s+").unwrap().replace_all(&flat, " ").replace('\u{1}', "<br>");
        let flat = flat.trim();
        let visible = flat.replace("&nbsp;", "").replace("<br>", "");
        if visible.trim().is_empty() {
            continue;
        }
        let style = if classes.first() == Some(&class) { "Default".to_string() } else { class.clone() };
        let mut ev = Event::new(t, t + LAST_LINE_MS, &style, &html_to_ass(flat, true).trim_end_matches("\\N").to_string());
        ev.text = ev.text.trim_start_matches("\\N").to_string();
        script.events.push(ev);
        open.push((class, script.events.len() - 1));
    }
    for c in classes.iter().skip(1) {
        ensure_style(&mut script, c);
    }
    script.events.sort_by_key(|e| e.start);
    script
}

pub fn write_sami(script: &Script) -> String {
    let title = escape_entities(script.info_get("Title").unwrap_or(""));
    let mut o = format!(
        "<SAMI>\r\n<HEAD>\r\n<TITLE>{title}</TITLE>\r\n<STYLE TYPE=\"text/css\">\r\n<!--\r\nP {{ margin-left:8pt; margin-right:8pt; margin-bottom:2pt; margin-top:2pt; font-size:20pt; text-align:center; font-weight:normal; color:white; }}\r\n.ENCC {{ Name:English; lang:en-US; SAMIType:CC; }}\r\n-->\r\n</STYLE>\r\n</HEAD>\r\n<BODY>\r\n"
    );
    let h = HtmlOpts { entities: true, an_tags: false, style_formatting: true };
    let evs = sorted_events(script);
    let mut cleared = true;
    let mut prev_end: Ms = 0;
    for e in evs {
        let t = event_to_html(script, e, &h);
        let t = t.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("<br>");
        if t.is_empty() {
            continue;
        }
        if !cleared && e.start > prev_end {
            o.push_str(&format!("<SYNC Start={}><P Class=ENCC>&nbsp;\r\n", prev_end));
        }
        o.push_str(&format!("<SYNC Start={}><P Class=ENCC>{t}\r\n", e.start));
        cleared = false;
        prev_end = e.end;
    }
    if !cleared {
        o.push_str(&format!("<SYNC Start={}><P Class=ENCC>&nbsp;\r\n", prev_end));
    }
    o.push_str("</BODY>\r\n</SAMI>\r\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "<SAMI>\n<HEAD><TITLE>Deneme</TITLE>\n<STYLE TYPE=\"text/css\"><!--\nP { font-size:20pt; }\n.KRCC { Name:Korean; lang:ko-KR; }\n.ENCC { Name:English; lang:en-US; }\n--></STYLE></HEAD>\n<BODY>\n<SYNC Start=1000><P Class=ENCC>Merhaba <i>dünya</i><br>İkinci satır &amp; ışık\n<SYNC Start=3000><P Class=ENCC>&nbsp;\n<SYNC Start=4000><P Class=ENCC>Ğüşöç\n<SYNC Start=4000><P Class=KRCC>Other\n<SYNC Start=6000><P Class=ENCC>&nbsp;\n</BODY>\n</SAMI>\n";

    #[test]
    fn sami_parse() {
        let s = parse_sami(SRC);
        assert_eq!(s.info_get("Title"), Some("Deneme"));
        assert_eq!(s.events.len(), 3);
        assert_eq!((s.events[0].start, s.events[0].end), (1000, 3000));
        assert_eq!(s.events[0].text, "Merhaba {\\i1}dünya{\\i0}\\Nİkinci satır & ışık");
        assert_eq!((s.events[1].start, s.events[1].end, s.events[1].text.as_str()), (4000, 6000, "Ğüşöç"));
        assert_eq!(s.events[2].style, "KRCC");
        assert_eq!(s.events[2].end, 7000);
        assert!(s.style("KRCC").is_some());
    }

    #[test]
    fn sami_write_roundtrip() {
        let mut s = Script::new();
        s.events.push(Event::new(1000, 2000, "Default", "{\\i1}Işık{\\i0} & <ğ>\\Nikinci"));
        s.events.push(Event::new(2000, 3000, "Default", "Bitişik"));
        let out = write_sami(&s);
        assert!(out.contains("<SYNC Start=1000><P Class=ENCC><i>Işık</i> &amp; &lt;ğ&gt;<br>ikinci\r\n<SYNC Start=2000>"));
        assert!(out.contains("<SYNC Start=3000><P Class=ENCC>&nbsp;"));
        let b = parse_sami(&out);
        assert_eq!(b.events.len(), 2);
        assert_eq!(b.events[0].text, "{\\i1}Işık{\\i0} & <ğ>\\Nikinci");
        assert_eq!((b.events[0].end, b.events[1].end), (2000, 3000));
    }
}
