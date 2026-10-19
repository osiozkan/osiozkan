//! SRT and WebVTT import/export.
use crate::ass::{Event, Script};
use crate::formats::markup::{event_to_html, html_to_ass, HtmlOpts};
use crate::tags::{parse, Item, Segment};
use crate::time::{format_clock, parse_time, Ms};

#[derive(Debug, Clone, Copy)]
pub struct SrtOptions {
    /// Emit `{\an8}` (from the text or the style alignment) at the start of the cue.
    pub an_tags: bool,
    /// Start from the style's italic/bold/underline/strikeout flags.
    pub style_formatting: bool,
}

impl Default for SrtOptions {
    fn default() -> Self {
        SrtOptions { an_tags: false, style_formatting: true }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VttOptions {
    /// Number the cues (`1`, `2`, ...) before the timing line.
    pub cue_ids: bool,
    /// Write `align:`/`line:`/`position:` from `\an` and `\pos`.
    pub settings: bool,
    /// Write the event actor as `<v Actor>`.
    pub speakers: bool,
    pub style_formatting: bool,
}

impl Default for VttOptions {
    fn default() -> Self {
        VttOptions { cue_ids: false, settings: true, speakers: true, style_formatting: true }
    }
}

/// `start --> end rest`: tolerant of spaces, `.`/`,`, hour-less and 1-digit-hour stamps, `->` arrows.
fn parse_timing(line: &str) -> Option<(Ms, Ms, &str)> {
    let (a, b) = line.split_once("-->").or_else(|| line.split_once("->"))?;
    let b = b.trim_start();
    let end = b.split_whitespace().next().unwrap_or("");
    let rest = b[end.len()..].trim();
    Some((parse_time(a.trim())?, parse_time(end)?, rest))
}

fn is_index(l: &str) -> bool {
    let t = l.trim();
    !t.is_empty() && t.len() <= 9 && t.bytes().all(|b| b.is_ascii_digit())
}

fn is_special_block(l: &str) -> bool {
    ["NOTE", "STYLE", "REGION"].iter().any(|k| l.trim_end() == *k || l.starts_with(&format!("{k} ")) || l.starts_with(&format!("{k}\t")))
}

fn parse_blocks(text: &str, vtt: bool) -> Script {
    let mut script = Script::new();
    script.info_set("Title", "Imported");
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    if vtt {
        // header block (WEBVTT + metadata) runs to the first blank line
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i < lines.len() && lines[i].trim_start().starts_with("WEBVTT") {
            while i < lines.len() && !lines[i].trim().is_empty() && parse_timing(lines[i]).is_none() {
                i += 1;
            }
        } else {
            i = 0;
        }
    }
    let res = script.play_res();
    let mut cur: Option<(Event, Vec<&str>, String)> = None;
    let mut skipping = false;
    let flush = |cur: &mut Option<(Event, Vec<&str>, String)>, script: &mut Script| {
        if let Some((mut e, body, settings)) = cur.take() {
            let raw = body.join("\n");
            let (actor, raw) = if vtt { split_voice(&raw) } else { (String::new(), raw) };
            e.actor = actor;
            e.text = html_to_ass(&raw, vtt);
            if vtt {
                apply_vtt_settings(&mut e, &settings, res);
            }
            script.events.push(e);
        }
    };
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        if let Some((s, e, rest)) = parse_timing(l) {
            if let Some((_, body, _)) = cur.as_mut() {
                // a cue number glued to the previous cue because the blank line is missing
                if body.len() > 1 && is_index(body[body.len() - 1]) {
                    body.pop();
                }
            }
            flush(&mut cur, &mut script);
            cur = Some((Event { start: s, end: e, ..Default::default() }, vec![], rest.to_string()));
            skipping = false;
            continue;
        }
        if l.trim().is_empty() {
            flush(&mut cur, &mut script);
            skipping = false;
            continue;
        }
        if skipping {
            continue;
        }
        if let Some((_, body, _)) = cur.as_mut() {
            body.push(l.trim_end());
            continue;
        }
        // between cues
        if vtt && is_special_block(l) {
            skipping = true;
            continue;
        }
        if is_index(l) || i < lines.len() && parse_timing(lines[i]).is_some() {
            continue; // cue number / identifier
        }
        if !vtt {
            // stray text after a blank line: keep it with the previous cue
            if let Some(last) = script.events.last_mut() {
                last.text.push_str("\\N");
                last.text.push_str(&html_to_ass(l.trim_end(), false));
            }
        }
    }
    flush(&mut cur, &mut script);
    script
}

/// Pull the first `<v Speaker>` out of a cue (the tag itself is dropped later with the other markup).
fn split_voice(raw: &str) -> (String, String) {
    let Some(i) = raw.find("<v") else { return (String::new(), raw.to_string()) };
    let after = &raw[i + 2..];
    let Some(j) = after.find('>') else { return (String::new(), raw.to_string()) };
    let tag = &after[..j];
    if !(tag.starts_with(' ') || tag.starts_with('.') || tag.starts_with('\t')) {
        return (String::new(), raw.to_string());
    }
    let name = tag.split_once(char::is_whitespace).map(|(_, n)| n.trim()).unwrap_or("");
    let mut out = String::from(&raw[..i]);
    out.push_str(&after[j + 1..]);
    let out = out.replacen("</v>", "", 1);
    (crate::formats::markup::decode_entities(name), out)
}

/// Translate WebVTT cue settings to `\an`, `\pos` and margins (heuristics, PlayRes based).
fn apply_vtt_settings(ev: &mut Event, settings: &str, (w, h): (i32, i32)) {
    let (mut align, mut pos, mut line, mut size) = (None::<&str>, None::<f64>, None::<(f64, bool)>, None::<f64>);
    for tok in settings.split_whitespace() {
        let Some((k, v)) = tok.split_once(':') else { continue };
        let num = |s: &str| s.trim_end_matches('%').parse::<f64>().ok();
        match k {
            "align" => align = Some(v),
            "position" => {
                let (p, a) = v.split_once(',').unwrap_or((v, ""));
                pos = num(p);
                if !a.is_empty() && align.is_none() {
                    align = Some(a);
                }
            }
            "line" => {
                let l = v.split_once(',').map(|x| x.0).unwrap_or(v);
                line = num(l).map(|n| (n, l.ends_with('%')));
            }
            "size" => size = num(v),
            _ => {}
        }
    }
    let col = match align.unwrap_or("center") {
        a if a.contains("left") || a.contains("start") => 1,
        a if a.contains("right") || a.contains("end") => 3,
        _ => 2,
    };
    let row = match line {
        Some((n, true)) if n < 33.0 => 2,
        Some((n, true)) if n <= 66.0 => 1,
        Some((n, false)) if (0.0..5.0).contains(&n) => 2,
        Some((n, false)) if (5.0..10.0).contains(&n) => 1,
        _ => 0,
    };
    let an = col + 3 * row;
    let (wf, hf) = (w as f64, h as f64);
    let mut tags = String::new();
    match (pos, line) {
        (Some(p), Some((l, true))) => {
            tags = format!("\\an{an}\\pos({},{})", (p * wf / 100.0).round(), (l * hf / 100.0).round());
        }
        _ => {
            if an != 2 {
                tags = format!("\\an{an}");
            }
            if let Some(p) = pos {
                match col {
                    1 => ev.margin_l = (p * wf / 100.0).round() as i32,
                    3 => ev.margin_r = ((100.0 - p) * wf / 100.0).round() as i32,
                    _ if (p - 50.0).abs() > 0.5 => {
                        let m = (wf * (2.0 * (p - 50.0).abs() / 100.0)).round() as i32;
                        if p < 50.0 { ev.margin_r = m } else { ev.margin_l = m }
                    }
                    _ => {}
                }
            } else if let (Some(s), 2) = (size, col) {
                if s < 100.0 {
                    let m = ((100.0 - s) / 200.0 * wf).round() as i32;
                    ev.margin_l = m;
                    ev.margin_r = m;
                }
            }
        }
    }
    if !tags.is_empty() {
        ev.text = format!("{{{tags}}}{}", ev.text);
    }
}

pub fn parse_srt(text: &str) -> Script {
    parse_blocks(text, false)
}

pub fn parse_vtt(text: &str) -> Script {
    parse_blocks(text, true)
}

fn sorted_events(script: &Script) -> Vec<&Event> {
    let mut evs: Vec<&Event> = script.events.iter().filter(|e| !e.is_comment()).collect();
    evs.sort_by_key(|e| (e.start, e.end));
    evs
}

/// Blank lines would end a cue, so drop them.
fn cue_text(t: &str) -> String {
    t.split('\n').map(str::trim_end).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

pub fn write_srt(script: &Script) -> String {
    write_srt_with(script, &SrtOptions::default())
}

pub fn write_srt_with(script: &Script, opts: &SrtOptions) -> String {
    let h = HtmlOpts { entities: false, an_tags: opts.an_tags, style_formatting: opts.style_formatting };
    let mut o = String::new();
    let mut n = 1;
    for e in sorted_events(script) {
        let t = cue_text(&event_to_html(script, e, &h));
        if t.trim().is_empty() {
            continue;
        }
        o.push_str(&format!("{n}\r\n{} --> {}\r\n{}\r\n\r\n", format_clock(e.start, ','), format_clock(e.end, ','), t.replace('\n', "\r\n")));
        n += 1;
    }
    o
}

pub fn write_vtt(script: &Script) -> String {
    write_vtt_with(script, &VttOptions::default())
}

/// `\an` / `\pos` of an event (style alignment as fallback) as WebVTT cue settings.
fn vtt_settings(script: &Script, e: &Event) -> String {
    let (mut an, mut pos) = (None::<i32>, None::<(f64, f64)>);
    for seg in parse(&e.text) {
        let Segment::Block(items) = seg else { continue };
        for it in items {
            let Item::Tag { name, value } = it else { continue };
            match name.as_str() {
                "an" if an.is_none() => an = value.trim().parse().ok(),
                "pos" | "move" if pos.is_none() => {
                    let v: Vec<f64> = value.trim().trim_start_matches('(').trim_end_matches(')').split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if v.len() >= 2 {
                        pos = Some((v[0], v[1]));
                    }
                }
                _ => {}
            }
        }
    }
    let an = an.or_else(|| script.style(&e.style).map(|s| s.align)).filter(|a| (1..=9).contains(a)).unwrap_or(2);
    let (w, h) = script.play_res();
    let mut s = vec![];
    if let Some((x, y)) = pos {
        s.push(format!("position:{}%", fmt_pct(x / w as f64 * 100.0)));
        s.push(format!("line:{}%", fmt_pct(y / h as f64 * 100.0)));
    } else {
        match (an - 1) / 3 {
            2 => s.push("line:0".to_string()),
            1 => s.push("line:50%".to_string()),
            _ => {}
        }
    }
    match (an - 1) % 3 {
        0 => s.push("align:start".to_string()),
        2 => s.push("align:end".to_string()),
        _ => {}
    }
    s.join(" ")
}

fn fmt_pct(v: f64) -> String {
    crate::ass::fmt_num((v.clamp(0.0, 100.0) * 100.0).round() / 100.0)
}

pub fn write_vtt_with(script: &Script, opts: &VttOptions) -> String {
    let h = HtmlOpts { entities: true, an_tags: false, style_formatting: opts.style_formatting };
    let mut o = String::from("WEBVTT\n\n");
    let mut n = 1;
    for e in sorted_events(script) {
        let mut t = cue_text(&event_to_html(script, e, &h));
        if t.trim().is_empty() {
            continue;
        }
        if opts.speakers && !e.actor.trim().is_empty() {
            t = format!("<v {}>{t}", crate::formats::markup::escape_entities(e.actor.trim()).replace('>', "&gt;"));
        }
        if opts.cue_ids {
            o.push_str(&format!("{n}\n"));
        }
        let settings = if opts.settings { vtt_settings(script, e) } else { String::new() };
        o.push_str(&format!("{} --> {}", format_clock(e.start, '.'), format_clock(e.end, '.')));
        if !settings.is_empty() {
            o.push(' ');
            o.push_str(&settings);
        }
        o.push_str(&format!("\n{t}\n\n"));
        n += 1;
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_roundtrip() {
        let s = parse_srt("1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Selam</i>\r\ndünya\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nTamam\r\n");
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].text, "{\\i1}Selam{\\i0}\\Ndünya");
        let out = write_srt(&s);
        assert!(out.contains("<i>Selam</i>\r\ndünya"));
        assert!(out.contains("00:00:01,000 --> 00:00:02,500"));
    }

    #[test]
    fn srt_tolerant() {
        // no blank lines between cues, dot separator, spaces, 1-digit hour, unnumbered, zero length, coordinates
        let src = "1\n0:00:01.000  -->  0:00:02.000\nBir\n2\n00:00:03,000 --> 00:00:03,000 X1:10 X2:20\nİki\n00:00:04,000 --> 00:00:03,500\nÜç\n\n\n\n5\n00:00:05,0 --> 00:00:06,5\n{\\an8}Dört <font color=\"#ff0000\" face=\"Arial\" size=\"30\">kırmızı</font> ğ\n";
        let s = parse_srt(src);
        assert_eq!(s.events.len(), 4);
        assert_eq!(s.events[0].text, "Bir");
        assert_eq!((s.events[0].start, s.events[0].end), (1000, 2000));
        assert_eq!(s.events[1].text, "İki");
        assert_eq!(s.events[1].duration(), 0);
        assert_eq!(s.events[2].text, "Üç");
        assert_eq!(s.events[3].start, 5000);
        assert_eq!(s.events[3].end, 6500);
        assert_eq!(s.events[3].text, "{\\an8}Dört {\\c&H0000FF&\\fnArial\\fs30}kırmızı{\\c\\fn\\fs} ğ");
    }

    #[test]
    fn srt_digit_only_text_and_stray_lines() {
        let s = parse_srt("1\n00:00:01,000 --> 00:00:02,000\n42\n\n2\n00:00:03,000 --> 00:00:04,000\nline\n\norphan text\n");
        assert_eq!(s.events[0].text, "42");
        assert_eq!(s.events[1].text, "line\\Norphan text");
    }

    #[test]
    fn srt_write_options() {
        let mut s = Script::new();
        s.events.push(Event::new(0, 1500, "Default", "{\\an8}{\\i1}a{\\i0} b{\\c&HFF&}red{\\r}\\Nsatır"));
        let plain = write_srt(&s);
        assert!(plain.contains("<i>a</i> b<font color=\"#FF0000\">red</font>\r\nsatır"));
        assert!(!plain.contains("an8"));
        let with = write_srt_with(&s, &SrtOptions { an_tags: true, ..Default::default() });
        assert!(with.contains("\r\n{\\an8}<i>a</i>"));
    }

    #[test]
    fn vtt_basic_and_settings() {
        let src = "WEBVTT - title\nKind: captions\n\nSTYLE\n::cue { color: red }\n\nNOTE comment\nmore\n\nintro\n00:01.000 --> 00:02.000 align:start position:10%\n<v Ahmet>Merhaba <c.yellow>sarı</c> &amp; <i>eğik</i></v>\n\n2\n01:00:03.500 --> 01:00:04.000 line:0 align:end\nÜst\n\n00:05.000 --> 00:06.000 line:50%\nOrta\n\n00:07.000 --> 00:08.000 position:25% line:80%\nSerbest\n";
        let s = parse_vtt(src);
        assert_eq!(s.events.len(), 4);
        assert_eq!(s.events[0].start, 1000);
        assert_eq!(s.events[0].actor, "Ahmet");
        assert_eq!(s.events[0].text, "{\\an1}Merhaba {\\c&H00FFFF&}sarı{\\c} & {\\i1}eğik{\\i0}");
        assert_eq!(s.events[0].margin_l, 192);
        assert_eq!(s.events[1].start, 3_603_500);
        assert_eq!(s.events[1].text, "{\\an9}Üst");
        assert_eq!(s.events[2].text, "{\\an5}Orta");
        assert_eq!(s.events[3].text, "{\\an2\\pos(480,864)}Serbest");
    }

    #[test]
    fn vtt_without_header_and_write() {
        let s = parse_vtt("00:01.000 --> 00:02.000\nHi\n");
        assert_eq!(s.events[0].text, "Hi");
        let mut s = Script::new();
        let mut e = Event::new(1000, 2000, "Default", "{\\an9}Tom & <Jerry>");
        e.actor = "Tom".into();
        s.events.push(e);
        s.events.push(Event::new(3000, 4000, "Default", "{\\pos(960,540)}mid"));
        let out = write_vtt_with(&s, &VttOptions { cue_ids: true, ..Default::default() });
        assert!(out.starts_with("WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000 line:0 align:end\n<v Tom>Tom &amp; &lt;Jerry&gt;\n"));
        assert!(out.contains("2\n00:00:03.000 --> 00:00:04.000 position:50% line:50%\nmid"));
        // reading it back
        let b = parse_vtt(&out);
        assert_eq!(b.events[0].actor, "Tom");
        assert_eq!(b.events[0].text, "{\\an9}Tom & <Jerry>");
        assert_eq!(b.events[1].text, "{\\an5\\pos(960,540)}mid");
    }
}
