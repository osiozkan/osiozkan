//! Inline HTML-ish markup (`<i>`, `<b>`, `<font color>`) to and from ASS override tags.
use crate::ass::{Event, Script, Style};
use crate::color::Color;
use crate::tags::{parse, Item, Segment};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct HtmlOpts {
    /// Decode/encode `&amp; &lt; &gt; &nbsp;` (VTT, SAMI, TTML) instead of passing text through (SRT).
    pub entities: bool,
    /// Emit `{\anN}` at the start of the text (from the text or, failing that, the style).
    pub an_tags: bool,
    /// Treat the style's italic/bold/underline/strikeout flags as the initial state.
    pub style_formatting: bool,
}

impl Default for HtmlOpts {
    fn default() -> Self {
        HtmlOpts { entities: false, an_tags: false, style_formatting: true }
    }
}

fn attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)([a-z][a-z-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>"']+))"#).unwrap())
}

const NAMED_COLORS: &[(&str, u32)] = &[
    ("white", 0xFFFFFF),
    ("black", 0x000000),
    ("red", 0xFF0000),
    ("lime", 0x00FF00),
    ("green", 0x008000),
    ("blue", 0x0000FF),
    ("yellow", 0xFFFF00),
    ("cyan", 0x00FFFF),
    ("aqua", 0x00FFFF),
    ("magenta", 0xFF00FF),
    ("fuchsia", 0xFF00FF),
    ("gray", 0x808080),
    ("grey", 0x808080),
    ("silver", 0xC0C0C0),
    ("orange", 0xFFA500),
    ("pink", 0xFFC0CB),
    ("purple", 0x800080),
    ("maroon", 0x800000),
    ("navy", 0x000080),
    ("teal", 0x008080),
    ("olive", 0x808000),
];

/// `#rrggbb`, `#rgb`, bare `rrggbb` or a CSS colour name.
pub fn parse_html_color(s: &str) -> Option<Color> {
    let s = s.trim().to_ascii_lowercase();
    if let Some((_, v)) = NAMED_COLORS.iter().find(|(n, _)| *n == s) {
        return Some(Color::rgb((v >> 16) as u8, (v >> 8) as u8, *v as u8));
    }
    let h = s.strip_prefix('#').unwrap_or(&s);
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = match h.len() {
        6 => u32::from_str_radix(h, 16).ok()?,
        3 => {
            let v = u32::from_str_radix(h, 16).ok()?;
            ((v >> 8 & 0xF) * 0x11) << 16 | ((v >> 4 & 0xF) * 0x11) << 8 | (v & 0xF) * 0x11
        }
        8 => u32::from_str_radix(&h[..6], 16).ok()?,
        _ => return None,
    };
    Some(Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest.find(';').filter(|j| *j <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let ch = match ent {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".into()),
            "gt" => Some(">".into()),
            "quot" => Some("\"".into()),
            "apos" => Some("'".into()),
            "nbsp" => Some("\\h".into()),
            "lrm" | "rlm" => Some(String::new()),
            _ => ent
                .strip_prefix('#')
                .and_then(|n| match n.strip_prefix(['x', 'X']) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => n.parse().ok(),
                })
                .and_then(char::from_u32)
                .map(|c| c.to_string()),
        };
        match ch {
            Some(c) => {
                out.push_str(&c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn escape_entities(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[derive(Default, Clone)]
struct FontFrame {
    color: Option<String>,
    face: Option<String>,
    size: Option<String>,
}

fn is_timestamp(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b':' || b == b'.' || b == b',') && s.contains(':')
}

/// Convert `<i>`, `<b>`, `<u>`, `<s>`, `<font color face size>`, `<br>` to ASS text. Other known tags are dropped,
/// unknown `<...>` stays literal. Line breaks in the input become `\N`.
pub fn html_to_ass(s: &str, entities: bool) -> String {
    let mut out = String::new();
    let mut fonts: Vec<FontFrame> = vec![];
    let mut rest = s;
    let push_text = |out: &mut String, t: &str| {
        let t = t.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\\N");
        if entities {
            out.push_str(&decode_entities(&t));
        } else {
            out.push_str(&t);
        }
    };
    while let Some(i) = rest.find('<') {
        push_text(&mut out, &rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            push_text(&mut out, &rest[i..]);
            rest = "";
            break;
        };
        let inner = &rest[i + 1..i + j];
        let closing = inner.starts_with('/');
        let body = inner.trim_start_matches('/').trim_end();
        if body.starts_with(char::is_whitespace) {
            push_text(&mut out, "<");
            rest = &rest[i + 1..];
            continue;
        }
        let name: String = body.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        let known = matches!(
            name.as_str(),
            "i" | "b" | "u" | "s" | "em" | "strong" | "font" | "br" | "c" | "v" | "ruby" | "rt" | "rp" | "lang" | "span" | "q" | "strike" | "big" | "small" | "center" | "p" | "div"
        );
        if !known && !(is_timestamp(body) && !closing) {
            push_text(&mut out, "<");
            rest = &rest[i + 1..];
            continue;
        }
        match (name.as_str(), closing) {
            ("i" | "em", false) => out.push_str("{\\i1}"),
            ("i" | "em", true) => out.push_str("{\\i0}"),
            ("b" | "strong", false) => out.push_str("{\\b1}"),
            ("b" | "strong", true) => out.push_str("{\\b0}"),
            ("u", false) => out.push_str("{\\u1}"),
            ("u", true) => out.push_str("{\\u0}"),
            ("s" | "strike", false) => out.push_str("{\\s1}"),
            ("s" | "strike", true) => out.push_str("{\\s0}"),
            ("br", _) => out.push_str("\\N"),
            ("font", false) => {
                let mut f = FontFrame::default();
                for c in attr_re().captures_iter(body) {
                    let val = c.get(2).or(c.get(3)).or(c.get(4)).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                    match c[1].to_ascii_lowercase().as_str() {
                        "color" => f.color = parse_html_color(&val).map(|c| c.to_override()),
                        "face" => f.face = Some(val).filter(|v| !v.is_empty()),
                        "size" => f.size = val.trim_start_matches('+').parse::<f64>().ok().map(|n| crate::ass::fmt_num(n)),
                        _ => {}
                    }
                }
                let mut tags = String::new();
                if let Some(c) = &f.color {
                    tags.push_str(&format!("\\c{c}"));
                }
                if let Some(n) = &f.face {
                    tags.push_str(&format!("\\fn{n}"));
                }
                if let Some(n) = &f.size {
                    tags.push_str(&format!("\\fs{n}"));
                }
                if !tags.is_empty() {
                    out.push_str(&format!("{{{tags}}}"));
                }
                fonts.push(f);
            }
            ("c", false) => {
                // WebVTT class span: colour classes map to \c, anything else only keeps the nesting balanced
                let color = body.split('.').skip(1).find_map(|c| parse_html_color(c.split_whitespace().next().unwrap_or(""))).map(|c| c.to_override());
                if let Some(c) = &color {
                    out.push_str(&format!("{{\\c{c}}}"));
                }
                fonts.push(FontFrame { color, ..Default::default() });
            }
            ("font" | "c", true) => {
                if let Some(f) = fonts.pop() {
                    // restore the enclosing font's values, or reset to the style
                    let outer = |pick: fn(&FontFrame) -> &Option<String>| fonts.iter().rev().find_map(|x| pick(x).clone());
                    let mut tags = String::new();
                    if f.color.is_some() {
                        tags.push_str(&format!("\\c{}", outer(|x| &x.color).unwrap_or_default()));
                    }
                    if f.face.is_some() {
                        tags.push_str(&format!("\\fn{}", outer(|x| &x.face).unwrap_or_default()));
                    }
                    if f.size.is_some() {
                        tags.push_str(&format!("\\fs{}", outer(|x| &x.size).unwrap_or_default()));
                    }
                    if !tags.is_empty() {
                        out.push_str(&format!("{{{tags}}}"));
                    }
                }
            }
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    push_text(&mut out, rest);
    out
}

/// Formatting state the HTML side can express.
#[derive(Clone, PartialEq, Default, Debug)]
struct State {
    i: bool,
    b: bool,
    u: bool,
    s: bool,
    color: Option<Color>,
}

impl State {
    fn from_style(st: Option<&Style>, on: bool) -> State {
        match (st, on) {
            (Some(s), true) => State { i: s.italic, b: s.bold, u: s.underline, s: s.strikeout, color: None },
            _ => State::default(),
        }
    }
}

fn tag_flag(v: &str) -> Option<bool> {
    let v = v.trim();
    if v.is_empty() {
        return None;
    }
    Some(v.parse::<i32>().map(|n| n != 0).unwrap_or(false))
}

struct Writer {
    out: String,
    /// currently open tags, innermost last: 'i' 'b' 'u' 's' 'f'
    open: Vec<char>,
    open_color: Option<Color>,
}

impl Writer {
    fn close_tag(&mut self, t: char) {
        self.out.push_str(match t {
            'i' => "</i>",
            'b' => "</b>",
            'u' => "</u>",
            's' => "</s>",
            _ => "</font>",
        });
    }

    fn open_tag(&mut self, t: char, st: &State) {
        match t {
            'i' => self.out.push_str("<i>"),
            'b' => self.out.push_str("<b>"),
            'u' => self.out.push_str("<u>"),
            's' => self.out.push_str("<s>"),
            _ => {
                let c = st.color.unwrap();
                self.out.push_str(&format!("<font color=\"{}\">", c.to_hex_rgb()));
                self.open_color = Some(c);
            }
        }
        self.open.push(t);
    }

    /// Bring the open tags in line with `st` before emitting text.
    fn sync(&mut self, st: &State) {
        let oc = self.open_color;
        let want = |t: char| match t {
            'i' => st.i,
            'b' => st.b,
            'u' => st.u,
            's' => st.s,
            _ => st.color.is_some() && st.color == oc,
        };
        // close down to the lowest tag that is no longer wanted
        if let Some(pos) = self.open.iter().position(|t| !want(*t)) {
            let closed: Vec<char> = self.open.drain(pos..).collect();
            for t in closed.iter().rev() {
                self.close_tag(*t);
            }
            for t in closed {
                if want(t) {
                    self.open_tag(t, st);
                }
            }
        }
        for t in ['i', 'b', 'u', 's'] {
            if want(t) && !self.open.contains(&t) {
                self.open_tag(t, st);
            }
        }
        if st.color.is_some() && !self.open.contains(&'f') {
            self.open_tag('f', st);
        }
    }

    fn finish(&mut self) {
        while let Some(t) = self.open.pop() {
            self.close_tag(t);
        }
    }
}

/// ASS event text to HTML-ish markup (newlines become `\n`).
pub fn event_to_html(script: &Script, e: &Event, opts: &HtmlOpts) -> String {
    let style_of = |name: &str| script.styles.iter().find(|s| s.name == name);
    let base_style = style_of(&e.style);
    let mut base = State::from_style(base_style, opts.style_formatting);
    let mut st = base.clone();
    let mut w = Writer { out: String::new(), open: vec![], open_color: None };
    let mut drawing = false;
    let segs = parse(&e.text);
    if opts.an_tags {
        let an = segs.iter().find_map(|s| match s {
            Segment::Block(items) => items.iter().find_map(|it| match it {
                Item::Tag { name, value } if name == "an" => value.trim().parse::<i32>().ok(),
                _ => None,
            }),
            _ => None,
        });
        let an = an.or_else(|| base_style.map(|s| s.align).filter(|a| (1..=9).contains(a) && *a != 2));
        if let Some(n) = an.filter(|n| *n != 2) {
            w.out.push_str(&format!("{{\\an{n}}}"));
        }
    }
    for seg in segs {
        match seg {
            Segment::Text(t) => {
                if drawing {
                    continue;
                }
                let t = t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ");
                let t = if opts.entities { escape_entities(&t) } else { t };
                if t.is_empty() {
                    continue;
                }
                // whitespace-only runs do not open tags
                if t.trim().is_empty() {
                    w.out.push_str(&t);
                } else {
                    w.sync(&st);
                    w.out.push_str(&t);
                }
            }
            Segment::Block(items) => {
                for it in items {
                    let Item::Tag { name, value } = it else { continue };
                    match name.as_str() {
                        "i" => st.i = tag_flag(&value).unwrap_or(base.i),
                        "b" => st.b = tag_flag(&value).unwrap_or(base.b),
                        "u" => st.u = tag_flag(&value).unwrap_or(base.u),
                        "s" => st.s = tag_flag(&value).unwrap_or(base.s),
                        "c" | "1c" => st.color = Color::parse(value.trim()).map(|c| Color::rgb(c.r, c.g, c.b)).filter(|_| !value.trim().is_empty()),
                        "r" => {
                            let rs = value.trim();
                            base = State::from_style(if rs.is_empty() { base_style } else { style_of(rs).or(base_style) }, opts.style_formatting);
                            st = base.clone();
                        }
                        "p" => drawing = value.trim().parse::<i32>().map(|n| n > 0).unwrap_or(false),
                        _ => {}
                    }
                }
            }
        }
    }
    w.finish();
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(text: &str) -> String {
        let s = Script::new();
        event_to_html(&s, &Event::new(0, 1, "Default", text), &HtmlOpts::default())
    }

    #[test]
    fn html_in() {
        assert_eq!(html_to_ass("<i>a</i> <b>b</b><u>c</u>", false), "{\\i1}a{\\i0} {\\b1}b{\\b0}{\\u1}c{\\u0}");
        assert_eq!(html_to_ass("<font color=\"#ff8000\" face='Tahoma' size=\"20\">x</font>y", false), "{\\c&H0080FF&\\fnTahoma\\fs20}x{\\c\\fn\\fs}y");
        assert_eq!(html_to_ass("<font color=red>a<font color=#0000ff>b</font>c</font>", false), "{\\c&H0000FF&}a{\\c&HFF0000&}b{\\c&H0000FF&}c{\\c}");
        assert_eq!(html_to_ass("a < b and 3 > 2 <3 <Ahmet>", false), "a < b and 3 > 2 <3 <Ahmet>");
        assert_eq!(html_to_ass("x<br/>y\nz", false), "x\\Ny\\Nz");
        assert_eq!(html_to_ass("{\\an8}Tom &amp; Jerry&nbsp;!", true), "{\\an8}Tom & Jerry\\h!");
        assert_eq!(html_to_ass("<c.yellow.bg_blue>x</c><00:01.000>y<c.foo>z</c>", true), "{\\c&H00FFFF&}x{\\c}yz");
    }

    #[test]
    fn html_out_resets() {
        assert_eq!(conv("{\\i1}a{\\i0}b"), "<i>a</i>b");
        assert_eq!(conv("{\\i1\\b1}a{\\i0}b{\\r}c"), "<i><b>a</b></i><b>b</b>c");
        assert_eq!(conv("{\\c&H0000FF&}red{\\c}no"), "<font color=\"#FF0000\">red</font>no");
        assert_eq!(conv("{\\1c&HFF0000&}a\\Nb{\\r}c"), "<font color=\"#0000FF\">a\nb</font>c");
        assert_eq!(conv("{\\i1}a {\\i0}{\\i1}b{\\i0}"), "<i>a b</i>");
        assert_eq!(conv("{\\p1}m 0 0 l 10 10{\\p0}t"), "t");
        assert_eq!(conv("a{\\fs30\\t(\\i1)}b"), "ab");
    }

    #[test]
    fn style_state_and_an() {
        let mut s = Script::new();
        s.styles[0].italic = true;
        s.styles[0].align = 8;
        let e = Event::new(0, 1, "Default", "x{\\i0}y{\\r}z");
        let o = HtmlOpts { an_tags: true, ..Default::default() };
        assert_eq!(event_to_html(&s, &e, &o), "{\\an8}<i>x</i>y<i>z</i>");
        let e = Event::new(0, 1, "Default", "{\\an2}x");
        assert_eq!(event_to_html(&s, &e, &HtmlOpts::default()), "<i>x</i>");
    }
}
