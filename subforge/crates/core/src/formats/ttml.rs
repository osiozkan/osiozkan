//! TTML / DFXP: `<p begin end>` with `<br/>` and styled `<span>`s; simple TTML writer.
use super::markup::{decode_entities, escape_entities, event_to_html, parse_html_color, HtmlOpts};
use super::{new_script, sorted_events};
use crate::ass::{Event, Script};
use crate::time::Ms;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

enum Tok {
    Start { name: String, attrs: HashMap<String, String>, empty: bool },
    End(String),
    Text(String),
}

fn attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"([\w:.-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap())
}

/// Local name: namespace prefix removed, lowercased.
fn local(n: &str) -> String {
    n.rsplit(':').next().unwrap_or(n).to_ascii_lowercase()
}

/// Minimal tolerant XML tokenizer (no DTDs, comments/PIs skipped, CDATA as text).
fn tokenize(xml: &str) -> Vec<Tok> {
    let mut out = vec![];
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(i) = rest.find('<') else {
            out.push(Tok::Text(rest.to_string()));
            break;
        };
        if i > 0 {
            out.push(Tok::Text(rest[..i].to_string()));
        }
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map(|j| &r[j + 3..]).unwrap_or("");
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let j = r.find("]]>").unwrap_or(r.len());
            out.push(Tok::Text(escape_entities(&r[..j])));
            rest = r.get(j + 3..).unwrap_or("");
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map(|j| &rest[j + 1..]).unwrap_or("");
        } else {
            // find the closing '>' outside quotes
            let (mut q, mut end) = (None::<char>, None);
            for (k, c) in rest.char_indices().skip(1) {
                match (q, c) {
                    (None, '"' | '\'') => q = Some(c),
                    (Some(x), c) if c == x => q = None,
                    (None, '>') => {
                        end = Some(k);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                out.push(Tok::Text(rest.to_string()));
                break;
            };
            let inner = &rest[1..end];
            rest = &rest[end + 1..];
            if let Some(n) = inner.strip_prefix('/') {
                out.push(Tok::End(local(n.trim())));
                continue;
            }
            let empty = inner.ends_with('/');
            let inner = inner.trim_end_matches('/');
            let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
            let mut attrs = HashMap::new();
            for c in attr_re().captures_iter(&inner[name_end..]) {
                let v = c.get(2).or(c.get(3)).map(|m| m.as_str()).unwrap_or("");
                attrs.insert(local(&c[1]), decode_entities(v));
            }
            out.push(Tok::Start { name: local(&inner[..name_end]), attrs, empty });
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Clock {
    frame_rate: f64,
    tick_rate: f64,
}

/// TTML time expression: clock time (`hh:mm:ss.fff`, `hh:mm:ss:ff`) or offset time (`12.5s`, `300ms`, `10f`, `t`).
fn parse_ttml_time(s: &str, c: Clock) -> Option<Ms> {
    let s = s.trim();
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let (h, m, rest) = match parts.len() {
            3 => (parts[0], parts[1], parts[2].to_string()),
            4 => (parts[0], parts[1], format!("{}:{}", parts[2], parts[3])),
            _ => return None,
        };
        let base = (h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0) * 1000.0;
        let secs = match rest.split_once(':') {
            Some((sec, fr)) => sec.parse::<f64>().ok()? * 1000.0 + fr.parse::<f64>().ok()? / c.frame_rate * 1000.0,
            None => rest.parse::<f64>().ok()? * 1000.0,
        };
        return Some((base + secs).round() as Ms);
    }
    let unit = s.trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '.');
    let n: f64 = s[..s.len() - unit.len()].parse().ok()?;
    Some(
        match unit {
            "h" => n * 3_600_000.0,
            "m" => n * 60_000.0,
            "s" => n * 1000.0,
            "ms" => n,
            "f" => n / c.frame_rate * 1000.0,
            "t" => n / c.tick_rate * 1000.0,
            _ => return None,
        }
        .round() as Ms,
    )
}

type Attrs = HashMap<String, String>;

#[derive(Clone, PartialEq, Default)]
struct Fmt {
    i: bool,
    b: bool,
    u: bool,
    s: bool,
    color: Option<String>,
}

impl Fmt {
    fn apply(&mut self, a: &Attrs) {
        if let Some(v) = a.get("fontstyle") {
            self.i = v == "italic" || v == "oblique";
        }
        if let Some(v) = a.get("fontweight") {
            self.b = v == "bold" || v.parse::<i32>().map(|n| n >= 600).unwrap_or(false);
        }
        if let Some(v) = a.get("textdecoration") {
            self.u = v.contains("underline");
            self.s = v.contains("lineThrough");
        }
        if let Some(c) = a.get("color").and_then(|c| parse_html_color(c.trim())) {
            self.color = Some(c.to_override());
        }
    }

    /// ASS tags that move the state from `from` to `self`.
    fn diff(&self, from: &Fmt) -> String {
        let mut o = String::new();
        for (a, b, t) in [(self.i, from.i, "i"), (self.b, from.b, "b"), (self.u, from.u, "u"), (self.s, from.s, "s")] {
            if a != b {
                o.push_str(&format!("{{\\{t}{}}}", a as u8));
            }
        }
        if self.color != from.color {
            o.push_str(&format!("{{\\c{}}}", self.color.clone().unwrap_or_default()));
        }
        o
    }
}

/// Effective attributes: referenced styles, then region, then the element's own.
fn resolve(a: &Attrs, styles: &HashMap<String, Attrs>, regions: &HashMap<String, Attrs>) -> Attrs {
    let mut out = Attrs::new();
    let mut add = |m: &Attrs| out.extend(m.iter().map(|(k, v)| (k.clone(), v.clone())));
    if let Some(r) = a.get("region").and_then(|r| regions.get(r)) {
        if let Some(refs) = r.get("style") {
            refs.split_whitespace().filter_map(|n| styles.get(n)).for_each(&mut add);
        }
        add(r);
    }
    if let Some(refs) = a.get("style") {
        refs.split_whitespace().filter_map(|n| styles.get(n)).for_each(&mut add);
    }
    add(a);
    out
}

fn align_tag(a: &Attrs) -> Option<i32> {
    let col = match a.get("textalign").map(String::as_str) {
        Some("left" | "start") => 1,
        Some("right" | "end") => 3,
        _ => 2,
    };
    let row = match a.get("displayalign").map(String::as_str) {
        Some("before") => 2,
        Some("center") => 1,
        _ => 0,
    };
    let an = col + 3 * row;
    (an != 2).then_some(an)
}

pub fn parse_ttml(xml: &str) -> Script {
    let mut script = new_script();
    let toks = tokenize(xml.trim_start_matches('\u{feff}'));
    let mut clock = Clock { frame_rate: 30.0, tick_rate: 10_000_000.0 };
    let (mut styles, mut regions) = (HashMap::<String, Attrs>::new(), HashMap::<String, Attrs>::new());
    // first pass: root attributes, styles, regions
    for t in &toks {
        if let Tok::Start { name, attrs, .. } = t {
            match name.as_str() {
                "tt" => {
                    if let Some(f) = attrs.get("framerate").and_then(|f| f.parse::<f64>().ok()) {
                        let mult = attrs.get("frameratemultiplier").and_then(|m| {
                            let (a, b) = m.split_once(' ')?;
                            Some(a.parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?)
                        });
                        clock.frame_rate = f * mult.unwrap_or(1.0);
                    }
                    if let Some(f) = attrs.get("tickrate").and_then(|f| f.parse::<f64>().ok()) {
                        clock.tick_rate = f;
                    }
                }
                "style" => {
                    if let Some(id) = attrs.get("id") {
                        styles.insert(id.clone(), attrs.clone());
                    }
                }
                "region" => {
                    if let Some(id) = attrs.get("id") {
                        regions.insert(id.clone(), attrs.clone());
                    }
                }
                _ => {}
            }
        }
    }
    let mut in_p: Option<(Ms, Ms, Attrs)> = None;
    let mut text = String::new();
    let mut fmt_stack: Vec<Fmt> = vec![];
    let mut cur = Fmt::default();
    let mut div_attrs: Vec<Attrs> = vec![];
    let mut in_head = false;
    for t in toks {
        match t {
            Tok::Start { name, attrs, empty } => match name.as_str() {
                "head" => in_head = true,
                "div" if !empty => div_attrs.push(attrs),
                "p" if !in_head => {
                    let parent = div_attrs.last().cloned().unwrap_or_default();
                    let mut a = parent;
                    a.extend(attrs.clone());
                    let (Some(s), end) = (attrs.get("begin").and_then(|b| parse_ttml_time(b, clock)), attrs.get("end").and_then(|e| parse_ttml_time(e, clock))) else {
                        continue;
                    };
                    let end = end.or_else(|| attrs.get("dur").and_then(|d| parse_ttml_time(d, clock)).map(|d| s + d)).unwrap_or(s + 3000);
                    let eff = resolve(&a, &styles, &regions);
                    cur = Fmt::default();
                    cur.apply(&eff);
                    text = cur.diff(&Fmt::default());
                    fmt_stack.clear();
                    if empty {
                        script.events.push(Event::new(s, end, "Default", ""));
                    } else {
                        in_p = Some((s, end, eff));
                    }
                }
                "br" if in_p.is_some() => text.push_str("\\N"),
                "span" if in_p.is_some() && !empty => {
                    let eff = resolve(&attrs, &styles, &regions);
                    let mut next = cur.clone();
                    next.apply(&eff);
                    text.push_str(&next.diff(&cur));
                    fmt_stack.push(std::mem::replace(&mut cur, next));
                }
                _ => {}
            },
            Tok::End(name) => match name.as_str() {
                "head" => in_head = false,
                "div" => {
                    div_attrs.pop();
                }
                "span" if in_p.is_some() => {
                    if let Some(prev) = fmt_stack.pop() {
                        text.push_str(&prev.diff(&cur));
                        cur = prev;
                    }
                }
                "p" => {
                    if let Some((s, e, eff)) = in_p.take() {
                        let ws = Regex::new(r"[ \t\r\n]+").unwrap();
                        let flat = ws.replace_all(&text, " ").replace(" \\N", "\\N").replace("\\N ", "\\N");
                        let mut flat = flat.trim().to_string();
                        if let Some(an) = align_tag(&eff) {
                            flat = format!("{{\\an{an}}}{flat}");
                        }
                        script.events.push(Event::new(s, e, "Default", &flat));
                    }
                }
                _ => {}
            },
            Tok::Text(t) if in_p.is_some() => {
                let t = decode_entities(&t).replace('{', "(").replace('}', ")");
                text.push_str(&t);
            }
            Tok::Text(_) => {}
        }
    }
    script
}

fn ttml_stamp(ms: Ms) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Replace the markup writer's tags with TTML spans.
fn html_to_ttml(html: &str) -> String {
    let mut s = html.replace("<i>", "<span tts:fontStyle=\"italic\">").replace("<b>", "<span tts:fontWeight=\"bold\">");
    s = s.replace("<u>", "<span tts:textDecoration=\"underline\">").replace("<s>", "<span tts:textDecoration=\"lineThrough\">");
    s = s.replace("<font color=", "<span tts:color=");
    for close in ["</i>", "</b>", "</u>", "</s>", "</font>"] {
        s = s.replace(close, "</span>");
    }
    s.replace('\n', "<br/>")
}

pub fn write_ttml(script: &Script) -> String {
    let lang = script.info_get("Language").filter(|l| !l.is_empty()).unwrap_or("und");
    let h = HtmlOpts { entities: true, an_tags: true, style_formatting: true };
    let mut body = String::new();
    let mut top = false;
    for e in sorted_events(script) {
        let mut t = event_to_html(script, e, &h);
        let mut attrs = String::new();
        if let Some(rest) = t.strip_prefix("{\\an") {
            if let Some((n, r)) = rest.split_once('}') {
                let n: i32 = n.parse().unwrap_or(2);
                if n >= 7 {
                    attrs.push_str(" region=\"top\"");
                    top = true;
                } else if n >= 4 {
                    attrs.push_str(" tts:displayAlign=\"center\"");
                }
                match (n - 1) % 3 {
                    0 => attrs.push_str(" tts:textAlign=\"left\""),
                    2 => attrs.push_str(" tts:textAlign=\"right\""),
                    _ => {}
                }
                t = r.to_string();
            }
        }
        let t = t.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        if t.is_empty() {
            continue;
        }
        body.push_str(&format!("      <p begin=\"{}\" end=\"{}\"{attrs}>{}</p>\n", ttml_stamp(e.start), ttml_stamp(e.end), html_to_ttml(&t)));
    }
    let layout = if top {
        "  <head>\n    <layout>\n      <region xml:id=\"top\" tts:displayAlign=\"before\" tts:textAlign=\"center\"/>\n    </layout>\n  </head>\n"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xml:lang=\"{}\">\n{layout}  <body>\n    <div>\n{body}    </div>\n  </body>\n</tt>\n",
        escape_entities(lang)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<!-- comment -->
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tts="http://www.w3.org/ns/ttml#styling" xmlns:ttp="http://www.w3.org/ns/ttml#parameter" xml:lang="tr" ttp:frameRate="25">
  <head>
    <styling>
      <style xml:id="it" tts:fontStyle="italic"/>
      <style xml:id="red" tts:color="#FF0000"/>
    </styling>
    <layout><region xml:id="top" tts:displayAlign="before"/></layout>
  </head>
  <body><div>
    <p begin="00:00:01.500" end="00:00:03.000">Merhaba <span style="it">eğik</span>
      dünya<br/>İkinci <span tts:fontWeight="bold" tts:color="#00ff00">satır &amp; ışık</span>!</p>
    <p begin="4s" dur="2.5s" region="top" style="red">Üst</p>
    <p begin="00:00:10:12" end="00:00:12:00"><span tts:textDecoration="underline">Kare</span></p>
    <p begin="5000ms" end="6000ms" tts:textAlign="right"/>
  </div></body>
</tt>"##;

    #[test]
    fn ttml_parse() {
        let s = parse_ttml(SRC);
        assert_eq!(s.events.len(), 4);
        assert_eq!((s.events[0].start, s.events[0].end), (1500, 3000));
        assert_eq!(s.events[0].text, "Merhaba {\\i1}eğik{\\i0} dünya\\Nİkinci {\\b1}{\\c&H00FF00&}satır & ışık{\\b0}{\\c}!");
        assert_eq!((s.events[1].start, s.events[1].end), (4000, 6500));
        assert_eq!(s.events[1].text, "{\\an8}{\\c&H0000FF&}Üst");
        // 10 s + 12 frames at 25 fps
        assert_eq!(s.events[2].start, 10_480);
        assert_eq!(s.events[2].text, "{\\u1}Kare{\\u0}");
        assert_eq!(s.events[3].text, "");
    }

    #[test]
    fn ttml_write_roundtrip() {
        let mut s = Script::new();
        s.events.push(Event::new(1000, 2500, "Default", "{\\i1}Işık{\\i0} & <ğ>\\N{\\c&HFF&}kırmızı"));
        s.events.push(Event::new(3000, 4000, "Default", "{\\an7}Sol üst"));
        let out = write_ttml(&s);
        assert!(out.contains("<p begin=\"00:00:01.000\" end=\"00:00:02.500\"><span tts:fontStyle=\"italic\">Işık</span> &amp; &lt;ğ&gt;<br/><span tts:color=\"#FF0000\">kırmızı</span></p>"));
        assert!(out.contains("region=\"top\" tts:textAlign=\"left\""));
        let b = parse_ttml(&out);
        assert_eq!(b.events.len(), 2);
        assert_eq!(b.events[0].text, "{\\i1}Işık{\\i0} & <ğ>\\N{\\c&H0000FF&}kırmızı{\\c}");
        assert_eq!(b.events[1].text, "{\\an7}Sol üst");
    }
}
