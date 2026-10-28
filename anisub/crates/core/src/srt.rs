//! SubRip import/export, with HTML-ish tags mapped to ASS overrides.

use crate::ass::{AssFile, Event, Line};
use crate::tags::{parse_tags, split_blocks, Block};
use crate::time;

pub fn parse(text: &str) -> AssFile {
    let mut file = AssFile::new_default();
    let norm = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    for chunk in norm.split("\n\n") {
        let mut lines = chunk.lines().filter(|l| !l.trim().is_empty()).peekable();
        // optional index line
        if let Some(first) = lines.peek() {
            if first.trim().parse::<u32>().is_ok() {
                lines.next();
            }
        }
        let Some(tl) = lines.next() else { continue };
        let Some((a, b)) = tl.split_once("-->") else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (time::parse(a), time::parse(b)) else { continue };
        let body: Vec<String> = lines.map(|l| html_to_ass(l.trim_end())).collect();
        file.lines.push(Line::Dialogue(Event {
            start,
            end,
            text: body.join("\\N"),
            ..Event::default()
        }));
    }
    file
}

fn html_to_ass(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
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
            t if t.starts_with("font") => {
                if let Some(c) = t.split("color=").nth(1) {
                    let c = c.trim_matches(|ch| ch == '"' || ch == '\'' || ch == ' ' || ch == '#');
                    if c.len() == 6 && u32::from_str_radix(c, 16).is_ok() {
                        // HTML RRGGBB -> ASS BBGGRR
                        out.push_str(&format!(
                            "{{\\c&H{}{}{}&}}",
                            &c[4..6].to_uppercase(),
                            &c[2..4].to_uppercase(),
                            &c[0..2].to_uppercase()
                        ));
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

fn ass_to_html(text: &str) -> String {
    let mut out = String::new();
    let (mut i, mut b, mut u) = (false, false, false);
    for blk in split_blocks(text) {
        match blk {
            Block::Text(t) => out.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")),
            Block::Tags(t) => {
                for (n, a) in parse_tags(&t) {
                    let on = a.trim() == "1";
                    let off = a.trim() == "0";
                    for (name, flag, o, c) in
                        [("i", &mut i, "<i>", "</i>"), ("b", &mut b, "<b>", "</b>"), ("u", &mut u, "<u>", "</u>")]
                    {
                        if n == name {
                            if on && !*flag {
                                out.push_str(o);
                                *flag = true;
                            } else if off && *flag {
                                out.push_str(c);
                                *flag = false;
                            }
                        }
                    }
                }
            }
        }
    }
    if i { out.push_str("</i>"); }
    if b { out.push_str("</b>"); }
    if u { out.push_str("</u>"); }
    out
}

pub fn write(file: &AssFile) -> String {
    let mut out = String::new();
    for (n, e) in file.events().filter(|e| !e.comment).enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            n + 1,
            time::format_clock(e.start, ','),
            time::format_clock(e.end, ','),
            ass_to_html(&e.text)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let f = parse("1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Selam</i>\r\n<font color=\"#ff0000\">dünya</font>\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nx\r\n");
        let ev: Vec<_> = f.events().collect();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].text, "{\\i1}Selam{\\i0}\\N{\\c&H0000FF&}dünya");
        assert_eq!(ev[0].end, 2500);
        let s = write(&f);
        assert!(s.starts_with("1\n00:00:01,000 --> 00:00:02,500\n<i>Selam</i>\ndünya\n"));
    }
}
