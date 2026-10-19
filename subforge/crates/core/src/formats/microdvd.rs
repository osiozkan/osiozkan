//! MicroDVD `.sub`: `{start frame}{end frame}text`, `|` breaks lines, `{y:i}`-style control codes.
use super::markup::{event_to_html, HtmlOpts};
use super::{new_script, sorted_events};
use crate::ass::{Event, Script};
use crate::time::{Fps, Ms};
use regex::Regex;
use std::sync::OnceLock;

pub const DEFAULT_FPS: f64 = 23.976;

fn line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\{(\d+)\}\{(\d*)\}(.*)$").unwrap())
}

fn code_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{([A-Za-z]):([^}]*)\}").unwrap())
}

/// The frame rate from a `{1}{1}23.976` header line, if the file has one.
pub fn microdvd_header_fps(text: &str) -> Option<f64> {
    let first = text.trim_start_matches('\u{feff}').lines().find(|l| !l.trim().is_empty())?;
    let c = line_re().captures(first.trim())?;
    if &c[1] != "1" || &c[2] != "1" {
        return None;
    }
    c[3].trim().parse::<f64>().ok().filter(|f| *f > 1.0 && *f < 1000.0)
}

fn convert_codes(t: &str) -> String {
    let mut out = String::new();
    let mut dirty = false;
    // split on '|' while handling codes inside each line
    let mut lines = t.split('|').peekable();
    while let Some(line) = lines.next() {
        let mut last = 0;
        let mut text = String::new();
        for c in code_re().captures_iter(line) {
            let m = c.get(0).unwrap();
            text.push_str(&line[last..m.start()].replace('{', "(").replace('}', ")"));
            last = m.end();
            let v = c[2].trim();
            match c[1].to_ascii_lowercase().as_str() {
                "y" => {
                    for f in v.chars() {
                        match f.to_ascii_lowercase() {
                            'i' => text.push_str("{\\i1}"),
                            'b' => text.push_str("{\\b1}"),
                            'u' => text.push_str("{\\u1}"),
                            's' => text.push_str("{\\s1}"),
                            _ => continue,
                        }
                        dirty = true;
                    }
                }
                "c" => {
                    if let Some(h) = v.strip_prefix('$').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit())) {
                        text.push_str(&format!("{{\\c&H{}&}}", h.to_ascii_uppercase()));
                        dirty = true;
                    }
                }
                "f" if !v.is_empty() => {
                    text.push_str(&format!("{{\\fn{v}}}"));
                    dirty = true;
                }
                "s" => {
                    if let Ok(n) = v.parse::<u32>() {
                        text.push_str(&format!("{{\\fs{n}}}"));
                        dirty = true;
                    }
                }
                _ => {}
            }
        }
        text.push_str(&line[last..].replace('{', "(").replace('}', ")"));
        out.push_str(&text);
        if lines.peek().is_some() {
            if dirty {
                out.push_str("{\\r}");
                dirty = false;
            }
            out.push_str("\\N");
        }
    }
    out
}

/// Parse MicroDVD text; a `{1}{1}fps` header overrides `fps`.
pub fn parse_microdvd(text: &str, fps: f64) -> Script {
    let fps = microdvd_header_fps(text).unwrap_or(if fps > 0.0 { fps } else { DEFAULT_FPS });
    let rate = Fps::from_f64(fps);
    let mut script = new_script();
    let mut first = true;
    for l in text.trim_start_matches('\u{feff}').lines() {
        let Some(c) = line_re().captures(l.trim()) else { continue };
        let start: i64 = c[1].parse().unwrap_or(0);
        if first {
            first = false;
            if start == 1 && &c[2] == "1" && c[3].trim().parse::<f64>().is_ok() {
                continue;
            }
        }
        let end: i64 = c[2].parse().unwrap_or_else(|_| start + (fps * 2.0) as i64);
        script.events.push(Event::new(rate.start_ms(start), rate.start_ms(end), "Default", &convert_codes(&c[3])));
    }
    script
}

fn frame(ms: Ms, fps: f64) -> i64 {
    (ms.max(0) as f64 * fps / 1000.0).round() as i64
}

/// Drop every `<...>` tag from a markup string (control codes cover line-level style only).
fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn write_microdvd(script: &Script, fps: f64) -> String {
    let fps = if fps > 0.0 { fps } else { DEFAULT_FPS };
    let mut o = format!("{{1}}{{1}}{}\n", crate::ass::fmt_num(fps));
    for e in sorted_events(script) {
        let html = event_to_html(script, e, &HtmlOpts::default());
        let mut lines = vec![];
        for l in html.split('\n').map(str::trim).filter(|l| !l.is_empty()) {
            let mut rest = l;
            let mut code = String::new();
            let mut flags = String::new();
            loop {
                let before = rest;
                for (open, close, ch) in [("<i>", "</i>", 'i'), ("<b>", "</b>", 'b'), ("<u>", "</u>", 'u'), ("<s>", "</s>", 's')] {
                    if rest.starts_with(open) && (rest.ends_with(close) || !rest.contains(close)) {
                        rest = &rest[open.len()..];
                        rest = rest.strip_suffix(close).unwrap_or(rest);
                        flags.push(ch);
                    }
                }
                if let Some(r) = rest.strip_prefix("<font color=\"#") {
                    if let Some((hex, r2)) = r.split_once("\">") {
                        if hex.len() == 6 {
                            code.push_str(&format!("{{c:${}{}{}}}", &hex[4..6], &hex[2..4], &hex[0..2]));
                            rest = r2.strip_suffix("</font>").unwrap_or(r2);
                        }
                    }
                }
                if rest == before {
                    break;
                }
            }
            if !flags.is_empty() {
                code.insert_str(0, &format!("{{y:{flags}}}"));
            }
            let t = strip_html(rest).replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">");
            lines.push(format!("{code}{t}"));
        }
        if lines.is_empty() {
            continue;
        }
        o.push_str(&format!("{{{}}}{{{}}}{}\n", frame(e.start, fps), frame(e.end, fps), lines.join("|")));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microdvd_roundtrip() {
        let src = "{1}{1}25\n{25}{75}Merhaba dünya|{y:i}İkinci satır\n{100}{150}{y:b}{c:$0000FF}Kırmızı ışık\n{200}{}Bitiş\n";
        let s = parse_microdvd(src, 0.0);
        assert_eq!(s.events.len(), 3);
        assert_eq!((s.events[0].start, s.events[0].end), (1000, 3000));
        assert_eq!(s.events[0].text, "Merhaba dünya\\N{\\i1}İkinci satır");
        assert_eq!(s.events[1].text, "{\\b1}{\\c&H0000FF&}Kırmızı ışık");
        assert_eq!(s.events[2].end, 10_000);
        let out = write_microdvd(&s, 25.0);
        assert!(out.starts_with("{1}{1}25\n{25}{75}Merhaba dünya|{y:i}İkinci satır\n"));
        assert!(out.contains("{100}{150}{y:b}{c:$0000FF}Kırmızı ışık"));
        assert_eq!(parse_microdvd(&out, 25.0).events, s.events);
    }

    #[test]
    fn fps_argument_and_header() {
        let s = parse_microdvd("{24}{48}x\n", 24.0);
        assert_eq!((s.events[0].start, s.events[0].end), (1000, 2000));
        assert_eq!(microdvd_header_fps("{1}{1}23.976\n{1}{2}x"), Some(23.976));
        assert_eq!(microdvd_header_fps("{1}{20}x"), None);
        // header wins over the argument and is not an event
        let s = parse_microdvd("{1}{1}25\n{25}{50}a\n", 30.0);
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.events[0].start, 1000);
    }
}
