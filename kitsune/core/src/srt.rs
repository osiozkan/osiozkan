//! SRT okuma/yazma. `<i>`, `<b>`, `<u>` ASS etiketlerine çevrilir ve geri döner.
use crate::model::{Document, Event};
use crate::tags::{split_blocks, Block};
use crate::time;

fn html_to_ass(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            rest = &rest[i..];
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
            t if t.starts_with("font") || t == "/font" => {}
            _ => out.push_str(&rest[i..i + j + 1]),
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out.replace("\r\n", "\\N").replace('\n', "\\N")
}

pub fn parse(input: &str) -> Document {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).replace("\r\n", "\n");
    let mut doc = Document::new();
    for block in input.split("\n\n") {
        let mut lines = block.trim_matches('\n').lines();
        let mut first = lines.next();
        if let Some(l) = first {
            if !l.contains("-->") {
                first = lines.next();
            }
        }
        let Some(tl) = first else { continue };
        let Some((a, b)) = tl.split_once("-->") else { continue };
        // Konum bilgisi gibi ek alanları at.
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (time::parse_srt(a), time::parse_srt(b)) else { continue };
        let text: Vec<&str> = lines.collect();
        doc.events.push(Event { start, end, text: html_to_ass(&text.join("\n")), ..Event::default() });
    }
    doc
}

pub fn write(doc: &Document) -> String {
    let mut o = String::new();
    let mut n = 0;
    for e in doc.events.iter().filter(|e| !e.comment) {
        n += 1;
        o.push_str(&format!("{}\n{} --> {}\n", n, time::fmt_srt(e.start), time::fmt_srt(e.end)));
        let mut t = String::new();
        for b in split_blocks(&e.text) {
            match b {
                Block::Text(s) => t.push_str(s),
                Block::Tags(tg) => {
                    for part in tg.split('\\') {
                        match part {
                            "i1" => t.push_str("<i>"),
                            "i0" => t.push_str("</i>"),
                            "b1" => t.push_str("<b>"),
                            "b0" => t.push_str("</b>"),
                            "u1" => t.push_str("<u>"),
                            "u0" => t.push_str("</u>"),
                            _ => {}
                        }
                    }
                }
            }
        }
        o.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " "));
        o.push_str("\n\n");
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let d = parse("1\n00:00:01,000 --> 00:00:02,500\n<i>Selam</i>\nDünya\n\n2\n00:00:03,000 --> 00:00:04,000\nIki\n");
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].text, "{\\i1}Selam{\\i0}\\NDünya");
        let w = write(&d);
        assert!(w.contains("<i>Selam</i>\nDünya"));
        assert!(w.contains("00:00:01,000 --> 00:00:02,500"));
    }
}
