//! LRC lyrics: `[mm:ss.xx]text`, metadata tags, enhanced word timing `<mm:ss.xx>` mapped to `\k`.
use super::{new_script, plain_lines, sorted_events};
use crate::ass::{Event, Script};
use crate::tags::syllables;
use crate::time::Ms;
use regex::Regex;
use std::sync::OnceLock;

const LAST_LINE_MS: Ms = 3000;

fn stamp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[(\d+):(\d{1,2})(?:[.:](\d{1,3}))?\]").unwrap())
}

fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<(\d+):(\d{1,2})(?:[.:](\d{1,3}))?>").unwrap())
}

fn meta_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[([A-Za-z#]+):([^\]]*)\]\s*$").unwrap())
}

fn to_ms(m: &str, s: &str, frac: Option<&str>) -> Ms {
    let f = frac.map(|f| format!("{f:0<3}").parse::<Ms>().unwrap_or(0)).unwrap_or(0);
    (m.parse::<Ms>().unwrap_or(0) * 60 + s.parse::<Ms>().unwrap_or(0)) * 1000 + f
}

struct Line {
    time: Ms,
    text: String,
}

pub fn parse_lrc(text: &str) -> Script {
    let mut script = new_script();
    let mut offset: Ms = 0;
    let mut lines: Vec<Line> = vec![];
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let mut rest = raw.trim();
        if let Some(c) = meta_re().captures(rest) {
            let (k, v) = (c[1].to_ascii_lowercase(), c[2].trim());
            match k.as_str() {
                "offset" => offset = v.trim_start_matches('+').parse().unwrap_or(0),
                "ti" => script.info_set("Title", v),
                _ => script.info_set(&format!("LRC {k}"), v),
            }
            continue;
        }
        let mut times = vec![];
        while let Some(c) = stamp_re().captures(rest) {
            times.push(to_ms(&c[1], &c[2], c.get(3).map(|m| m.as_str())));
            rest = rest[c[0].len()..].trim_start();
        }
        for t in times {
            lines.push(Line { time: t - offset, text: rest.to_string() });
        }
    }
    lines.sort_by_key(|l| l.time);
    for (i, l) in lines.iter().enumerate() {
        let plain = word_re().replace_all(&l.text, "");
        if plain.trim().is_empty() {
            continue;
        }
        let start = l.time.max(0);
        let end = lines.get(i + 1).map(|n| n.time).unwrap_or(start + LAST_LINE_MS).max(start);
        script.events.push(Event::new(start, end, "Default", &enhanced_to_k(&l.text, start, end, offset)));
    }
    script
}

/// `<mm:ss.xx>word` stamps to `{\kN}word` (durations in centiseconds); plain lines pass through.
fn enhanced_to_k(text: &str, start: Ms, end: Ms, offset: Ms) -> String {
    let esc = |s: &str| s.replace('{', "(").replace('}', ")");
    let marks: Vec<_> = word_re().captures_iter(text).collect();
    if marks.is_empty() {
        return esc(text.trim());
    }
    let mut out = String::new();
    let lead = &text[..marks[0].get(0).unwrap().start()];
    let first = to_ms(&marks[0][1], &marks[0][2], marks[0].get(3).map(|m| m.as_str())) - offset;
    if first > start {
        out.push_str(&format!("{{\\k{}}}", (first - start + 5) / 10));
    }
    out.push_str(&esc(lead));
    for (i, m) in marks.iter().enumerate() {
        let t = to_ms(&m[1], &m[2], m.get(3).map(|x| x.as_str())) - offset;
        let next = marks.get(i + 1).map(|n| to_ms(&n[1], &n[2], n.get(3).map(|x| x.as_str())) - offset).unwrap_or(end);
        let seg_end = marks.get(i + 1).map(|n| n.get(0).unwrap().start()).unwrap_or(text.len());
        let word = &text[m.get(0).unwrap().end()..seg_end];
        out.push_str(&format!("{{\\k{}}}{}", ((next - t).max(0) + 5) / 10, esc(word)));
    }
    out.trim_end().to_string()
}

fn stamp(ms: Ms, bracket: (char, char)) -> String {
    let cs = (ms.max(0) + 5) / 10;
    format!("{}{:02}:{:02}.{:02}{}", bracket.0, cs / 6000, cs / 100 % 60, cs % 100, bracket.1)
}

/// Write LRC; with `enhanced`, karaoke syllables become `<mm:ss.xx>` word stamps.
pub fn write_lrc(script: &Script, enhanced: bool) -> String {
    let mut o = String::new();
    if let Some(t) = script.info_get("Title").filter(|t| *t != "Imported" && *t != "Untitled") {
        o.push_str(&format!("[ti:{t}]\n"));
    }
    for (k, v) in &script.info {
        if let Some(tag) = k.strip_prefix("LRC ") {
            o.push_str(&format!("[{tag}:{v}]\n"));
        }
    }
    let evs = sorted_events(script);
    for (i, e) in evs.iter().enumerate() {
        let lines = plain_lines(e);
        if lines.is_empty() {
            continue;
        }
        o.push_str(&stamp(e.start, ('[', ']')));
        let syl = syllables(&e.text);
        if enhanced && syl.iter().any(|s| s.duration_cs > 0) {
            let mut t = e.start;
            for s in syl {
                o.push_str(&stamp(t, ('<', '>')));
                o.push_str(&s.text.replace("\\N", " ").replace("\\n", " ").replace("\\h", " "));
                t += s.duration_cs * 10;
            }
        } else {
            o.push_str(&lines.join(" "));
        }
        o.push('\n');
        // an empty stamp clears the line when there is a gap before the next one
        if evs.get(i + 1).map(|n| n.start > e.end).unwrap_or(true) {
            o.push_str(&stamp(e.end, ('[', ']')));
            o.push('\n');
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "[ti:Şarkı]\n[ar:Sanatçı]\n[offset:+500]\n[00:01.50]İlk satır\n[00:04.00][00:20.00]Nakarat ışık\n[00:07.00]\n[00:09.00]<00:09.00>Sev<00:09.50>gi <00:10.00>ğüş\n[00:12.00]\n";

    #[test]
    fn lrc_parse() {
        let s = parse_lrc(SRC);
        assert_eq!(s.info_get("Title"), Some("Şarkı"));
        assert_eq!(s.info_get("LRC ar"), Some("Sanatçı"));
        assert_eq!(s.events.len(), 4);
        // offset +500: lyrics appear 0.5 s earlier
        assert_eq!((s.events[0].start, s.events[0].end), (1000, 3500));
        assert_eq!(s.events[0].text, "İlk satır");
        // the empty stamp ends the previous line
        assert_eq!(s.events[1].end, 6500);
        assert_eq!(s.events[2].text, "{\\k50}Sev{\\k50}gi {\\k200}ğüş");
        assert_eq!((s.events[2].start, s.events[2].end), (8500, 11500));
        assert_eq!(s.events[3].start, 19_500);
        assert_eq!(s.events[3].end, 22_500);
    }

    #[test]
    fn lrc_enhanced_roundtrip() {
        let mut s = Script::new();
        s.info_set("Title", "Test");
        s.events.push(Event::new(1000, 3000, "Default", "{\\k50}Mer{\\k150}haba"));
        s.events.push(Event::new(3000, 4000, "Default", "İkinci"));
        let plain = write_lrc(&s, false);
        assert_eq!(plain, "[ti:Test]\n[00:01.00]Merhaba\n[00:03.00]İkinci\n[00:04.00]\n");
        let enh = write_lrc(&s, true);
        assert!(enh.contains("[00:01.00]<00:01.00>Mer<00:01.50>haba\n"));
        let back = parse_lrc(&enh);
        assert_eq!(back.events[0].text, "{\\k50}Mer{\\k150}haba");
        assert_eq!((back.events[0].start, back.events[0].end), (1000, 3000));
    }
}
