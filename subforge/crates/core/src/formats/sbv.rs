//! YouTube `.sbv`: `h:mm:ss.mmm,h:mm:ss.mmm` followed by the text, blocks separated by blank lines.
use super::{new_script, plain_lines, sorted_events};
use crate::ass::{Event, Script};
use crate::time::{parse_time, Ms};

pub(crate) fn parse_range(l: &str) -> Option<(Ms, Ms)> {
    let (a, b) = l.trim().split_once(',')?;
    Some((parse_time(a)?, parse_time(b)?))
}

pub fn parse_sbv(text: &str) -> Script {
    let mut script = new_script();
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut cur: Option<(Ms, Ms, Vec<&str>)> = None;
    let done = |cur: &mut Option<(Ms, Ms, Vec<&str>)>, script: &mut Script| {
        if let Some((s, e, body)) = cur.take() {
            script.events.push(Event::new(s, e, "Default", &body.join("\\N").replace('{', "(").replace('}', ")")));
        }
    };
    for l in text.split('\n') {
        if let Some((s, e)) = parse_range(l) {
            done(&mut cur, &mut script);
            cur = Some((s, e, vec![]));
        } else if l.trim().is_empty() {
            done(&mut cur, &mut script);
        } else if let Some((_, _, body)) = cur.as_mut() {
            body.push(l.trim_end());
        }
    }
    done(&mut cur, &mut script);
    script
}

fn stamp(ms: Ms) -> String {
    let ms = ms.max(0);
    format!("{}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

pub fn write_sbv(script: &Script) -> String {
    let mut o = String::new();
    for e in sorted_events(script) {
        let lines = plain_lines(e);
        if lines.is_empty() {
            continue;
        }
        o.push_str(&format!("{},{}\n{}\n\n", stamp(e.start), stamp(e.end), lines.join("\n")));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sbv_roundtrip() {
        let src = "0:00:01.599,0:00:04.160\n>> Merhaba dünya\nİkinci satır\n\n0:01:05.000,0:01:06.250\nÇok güzel ışık\n";
        let s = parse_sbv(src);
        assert_eq!(s.events.len(), 2);
        assert_eq!((s.events[0].start, s.events[0].end), (1599, 4160));
        assert_eq!(s.events[0].text, ">> Merhaba dünya\\Nİkinci satır");
        let out = write_sbv(&s);
        assert_eq!(out, format!("{src}\n"));
        assert_eq!(parse_sbv(&out).events, s.events);
    }
}
