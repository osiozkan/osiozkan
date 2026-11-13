//! TTML ailesi: TTML1/DFXP, Netflix TTML (DFXP-full), EBU-TT (`tt:` ön ekli) ve Apple iTT.
//!
//! Okuyucu küçük bir XML belirteçleyiciyle çalışır: ad alanı ön ekleri yok sayılır,
//! `begin/end/dur` (saat, kare `hh:mm:ss:ff`, `ms/s/m/h/f/t` ofsetleri; `ttp:frameRate`,
//! `frameRateMultiplier`, `tickRate`), `<span>` ve `<style>` ile italik/kalın/altı çizili,
//! üst bölge (`displayAlign="before"` ya da üst yarıdaki `origin`) → `{\an8}` desteklenir.
use crate::ass::{Dialogue, Script};
use crate::{tags, time};
use std::collections::HashMap;

// ------------------------------------------------------------------ belirteçleyici

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    close: bool,
    self_close: bool,
}

impl Tag {
    fn get(&self, local: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == local).map(|(_, v)| v.as_str())
    }
}

enum Tok<'a> {
    Tag(Tag),
    Text(&'a str),
}

fn local(n: &str) -> String {
    n.rsplit(':').next().unwrap_or(n).to_string()
}

fn tokenize(src: &str) -> Vec<Tok<'_>> {
    let mut out = Vec::new();
    let mut rest = src;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push(Tok::Text(rest));
            break;
        };
        if lt > 0 {
            out.push(Tok::Text(&rest[..lt]));
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map(|i| &r[i + 3..]).unwrap_or("");
            continue;
        }
        if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let e = r.find("]]>").unwrap_or(r.len());
            out.push(Tok::Text(&r[..e]));
            rest = r.get(e + 3..).unwrap_or("");
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map(|i| &rest[i + 1..]).unwrap_or("");
            continue;
        }
        // Etiket sonu: tırnak içindeki '>' atlanır.
        let b = rest.as_bytes();
        let mut i = 1;
        let mut q = 0u8;
        while i < b.len() {
            match b[i] {
                b'"' | b'\'' if q == 0 => q = b[i],
                c if c == q => q = 0,
                b'>' if q == 0 => break,
                _ => {}
            }
            i += 1;
        }
        let inner = &rest[1..i.min(rest.len())];
        rest = rest.get(i + 1..).unwrap_or("");
        let close = inner.starts_with('/');
        let self_close = inner.ends_with('/');
        let body = inner.trim_start_matches('/').trim_end_matches('/');
        let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
        let mut attrs = Vec::new();
        let mut a = &body[name_end..];
        loop {
            a = a.trim_start();
            let Some(eq) = a.find('=') else { break };
            let key = local(a[..eq].trim());
            let v = a[eq + 1..].trim_start();
            let Some(qc) = v.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
            let Some(end) = v[1..].find(qc) else { break };
            attrs.push((key, unescape(&v[1..1 + end])));
            a = &v[end + 2..];
        }
        out.push(Tok::Tag(Tag { name: local(&body[..name_end]), attrs, close, self_close }));
    }
    out
}

pub(crate) fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let r = &rest[i..];
        let Some(semi) = r.find(';').filter(|&k| k < 12) else {
            out.push('&');
            rest = &r[1..];
            continue;
        };
        let ent = &r[1..semi];
        let c = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &r[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &r[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ------------------------------------------------------------------ zaman

#[derive(Clone, Copy, Debug)]
struct Rates {
    /// Saniyedeki nominal kare (`ttp:frameRate`).
    frame: f64,
    /// Etkin kare hızı (çarpan uygulanmış).
    eff: f64,
    tick: f64,
}

impl Default for Rates {
    fn default() -> Self {
        Rates { frame: 30.0, eff: 30.0, tick: 1.0 }
    }
}

fn parse_time(s: &str, r: Rates) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains(':') {
        let p: Vec<&str> = s.split(':').collect();
        let num = |x: &str| x.trim().parse::<f64>().ok();
        let (h, m, sec) = (num(p[0])?, num(*p.get(1)?)?, num(*p.get(2)?)?);
        let mut ms = (h * 3600.0 + m * 60.0 + sec) * 1000.0;
        if let Some(f) = p.get(3) {
            // hh:mm:ss:ff(.alt-kare)
            ms += num(f)? * 1000.0 / r.eff;
        }
        return Some(ms.round() as i64);
    }
    let unit_at = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let v: f64 = s[..unit_at].parse().ok()?;
    let ms = match &s[unit_at..] {
        "h" => v * 3_600_000.0,
        "m" => v * 60_000.0,
        "s" | "" => v * 1000.0,
        "ms" => v,
        "f" => v * 1000.0 / r.eff,
        "t" => v * 1000.0 / r.tick,
        _ => return None,
    };
    Some(ms.round() as i64)
}

// ------------------------------------------------------------------ okuma

#[derive(Clone, Copy, Default, PartialEq)]
struct Fmt {
    i: bool,
    b: bool,
    u: bool,
}

fn apply_style(tag: &Tag, styles: &HashMap<String, Fmt>, base: Fmt) -> Fmt {
    let mut f = base;
    if let Some(ids) = tag.get("style") {
        for id in ids.split_whitespace() {
            if let Some(s) = styles.get(id) {
                f.i |= s.i;
                f.b |= s.b;
                f.u |= s.u;
            }
        }
    }
    if let Some(v) = tag.get("fontStyle") {
        f.i = v == "italic" || v == "oblique";
    }
    if let Some(v) = tag.get("fontWeight") {
        f.b = v == "bold";
    }
    if let Some(v) = tag.get("textDecoration") {
        f.u = v.contains("underline") && !v.contains("noUnderline");
    }
    f
}

fn region_is_top(tag: &Tag) -> bool {
    if tag.get("displayAlign") == Some("before") {
        return true;
    }
    if let Some(o) = tag.get("origin") {
        if let Some(y) = o.split_whitespace().nth(1) {
            if let Some(p) = y.strip_suffix('%') {
                return p.parse::<f64>().map(|v| v < 40.0).unwrap_or(false);
            }
        }
    }
    false
}

fn fmt_tags(from: Fmt, to: Fmt) -> String {
    let mut s = String::new();
    if from.i != to.i {
        s.push_str(if to.i { "\\i1" } else { "\\i0" });
    }
    if from.b != to.b {
        s.push_str(if to.b { "\\b1" } else { "\\b0" });
    }
    if from.u != to.u {
        s.push_str(if to.u { "\\u1" } else { "\\u0" });
    }
    if s.is_empty() {
        s
    } else {
        format!("{{{}}}", s)
    }
}

/// TTML/DFXP/iTT/EBU-TT metnini satırlara çevirir.
pub fn parse(src: &str) -> Vec<Dialogue> {
    let toks = tokenize(src);
    let mut rates = Rates::default();
    let mut styles: HashMap<String, Fmt> = HashMap::new();
    let mut top_regions: HashMap<String, bool> = HashMap::new();
    // Önce head: stiller ve bölgeler (body'den önce tanımlanır ama sıraya güvenme).
    for t in &toks {
        if let Tok::Tag(tag) = t {
            if tag.close {
                continue;
            }
            match tag.name.as_str() {
                "tt" => {
                    if let Some(f) = tag.get("frameRate").and_then(|v| v.parse::<f64>().ok()) {
                        rates.frame = f;
                        rates.eff = f;
                    }
                    if let Some(m) = tag.get("frameRateMultiplier") {
                        let v: Vec<f64> = m.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                        if v.len() == 2 && v[1] != 0.0 {
                            rates.eff = rates.frame * v[0] / v[1];
                        }
                    }
                    if let Some(t) = tag.get("tickRate").and_then(|v| v.parse::<f64>().ok()) {
                        rates.tick = t;
                    } else if let Some(f) = tag.get("frameRate").and_then(|v| v.parse::<f64>().ok()) {
                        rates.tick = f;
                    }
                }
                "style" => {
                    if let Some(id) = tag.get("id") {
                        let f = apply_style(tag, &styles, Fmt::default());
                        styles.insert(id.to_string(), f);
                    }
                }
                "region" => {
                    if let Some(id) = tag.get("id") {
                        top_regions.insert(id.to_string(), region_is_top(tag));
                    }
                }
                _ => {}
            }
        }
    }

    let mut out = Vec::new();
    // body/div bağlamı: (bölge, biçim, başlangıç ofseti)
    let mut ctx: Vec<(Option<String>, Fmt, i64)> = vec![(None, Fmt::default(), 0)];
    let mut in_head = false;
    // Etkin <p>: (başlangıç, bitiş, üst?, metin, biçim yığını, yazılmış biçim)
    struct P {
        start: i64,
        end: i64,
        top: bool,
        text: String,
        stack: Vec<Fmt>,
        cur: Fmt,
    }
    let mut p: Option<P> = None;
    for t in &toks {
        match t {
            Tok::Tag(tag) => match (tag.name.as_str(), tag.close) {
                ("head", false) => in_head = !tag.self_close,
                ("head", true) => in_head = false,
                _ if in_head => {}
                ("body" | "div", false) if p.is_none() => {
                    let (reg, f, off) = ctx.last().cloned().unwrap_or((None, Fmt::default(), 0));
                    let off = off + tag.get("begin").and_then(|v| parse_time(v, rates)).unwrap_or(0);
                    let entry = (tag.get("region").map(str::to_string).or(reg), apply_style(tag, &styles, f), off);
                    if !tag.self_close {
                        ctx.push(entry);
                    }
                }
                ("body" | "div", true) if p.is_none() => {
                    if ctx.len() > 1 {
                        ctx.pop();
                    }
                }
                ("p", false) => {
                    let (reg, f, off) = ctx.last().cloned().unwrap_or((None, Fmt::default(), 0));
                    let begin = tag.get("begin").and_then(|v| parse_time(v, rates));
                    let end = tag.get("end").and_then(|v| parse_time(v, rates));
                    let dur = tag.get("dur").and_then(|v| parse_time(v, rates));
                    let (Some(b), Some(e)) = (begin, end.or(begin.zip(dur).map(|(b, d)| b + d))) else { continue };
                    let reg = tag.get("region").map(str::to_string).or(reg);
                    let top = reg.as_deref().and_then(|r| top_regions.get(r).copied()).unwrap_or(false);
                    let pf = apply_style(tag, &styles, f);
                    let mut np = P { start: off + b, end: off + e, top, text: String::new(), stack: vec![pf], cur: Fmt::default() };
                    if tag.self_close {
                        np.stack.clear();
                    }
                    p = Some(np);
                    if tag.self_close {
                        p = None;
                    }
                }
                ("p", true) => {
                    if let Some(mut q) = p.take() {
                        if q.cur != Fmt::default() {
                            let tg = fmt_tags(q.cur, Fmt::default());
                            q.text.push_str(&tg);
                        }
                        let text = tidy(&q.text);
                        let text = if q.top { format!("{{\\an8}}{}", text) } else { text };
                        out.push(Dialogue { start_time: q.start, end_time: q.end.max(q.start), text, ..Default::default() });
                    }
                }
                ("br", _) => {
                    if let Some(q) = p.as_mut() {
                        // Kapanmış span'ın biçim değişikliğini satır sonundan önce yaz.
                        let want = q.stack.last().copied().unwrap_or_default();
                        if want != q.cur {
                            let tg = fmt_tags(q.cur, want);
                            q.text.push_str(&tg);
                            q.cur = want;
                        }
                        q.text.push_str("\\N");
                    }
                }
                ("span", false) => {
                    if let Some(q) = p.as_mut() {
                        let base = q.stack.last().copied().unwrap_or_default();
                        let f = apply_style(tag, &styles, base);
                        if !tag.self_close {
                            q.stack.push(f);
                        }
                    }
                }
                ("span", true) => {
                    if let Some(q) = p.as_mut() {
                        if q.stack.len() > 1 {
                            q.stack.pop();
                        }
                    }
                }
                _ => {}
            },
            Tok::Text(s) => {
                if let Some(q) = p.as_mut() {
                    let txt = unescape(s);
                    if txt.trim().is_empty() && !txt.is_empty() {
                        q.text.push(' ');
                        continue;
                    }
                    let want = q.stack.last().copied().unwrap_or_default();
                    if want != q.cur {
                        let tg = fmt_tags(q.cur, want);
                        q.text.push_str(&tg);
                        q.cur = want;
                    }
                    q.text.push_str(&txt.replace('{', "(").replace('}', ")"));
                }
            }
        }
    }
    out
}

/// XML boşluk daraltma; `\N` çevresindeki boşluklar atılır.
fn tidy(s: &str) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace(" \\N", "\\N").replace("\\N ", "\\N").trim().to_string()
}

// ------------------------------------------------------------------ yazma

/// ASS metnini biçimli parçalara ayırır: (metin, italik, kalın, altı çizili); satır sonu `\n`.
/// Çizimler (`\p1`) atlanır. EBU STL yazıcısı da kullanır.
pub(crate) fn styled_runs(text: &str) -> Vec<(String, bool, bool, bool)> {
    let mut out: Vec<(String, bool, bool, bool)> = Vec::new();
    let (mut i, mut b, mut u, mut drawing) = (false, false, false, false);
    for part in tags::split(text) {
        match part {
            tags::Part::Block(blk) => {
                for tag in blk.split('\\').skip(1) {
                    let tag = tag.trim();
                    // Değersiz `\i` / `\b` stil varsayılanına döner: düz say.
                    let val = |n: &str| tag.strip_prefix(n).filter(|r| r.chars().all(|c| c.is_ascii_digit()));
                    if let Some(r) = val("i") {
                        i = r.parse::<u32>().map(|v| v > 0).unwrap_or(false);
                    } else if let Some(r) = val("b") {
                        b = matches!(r.parse::<u32>(), Ok(v) if v == 1 || v >= 600);
                    } else if let Some(r) = val("u") {
                        u = r.parse::<u32>().map(|v| v > 0).unwrap_or(false);
                    } else if let Some(r) = val("p") {
                        drawing = r.parse::<u32>().unwrap_or(0) > 0;
                    } else if tag.starts_with('r') && !tag.starts_with("rnd") {
                        i = false;
                        b = false;
                        u = false;
                    }
                }
            }
            tags::Part::Text(t) if !drawing => {
                let t = t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", "\u{a0}");
                match out.last_mut() {
                    Some(last) if (last.1, last.2, last.3) == (i, b, u) => last.0.push_str(&t),
                    _ => out.push((t, i, b, u)),
                }
            }
            tags::Part::Text(_) => {}
        }
    }
    out.retain(|r| !r.0.is_empty());
    out
}

fn is_top(d: &Dialogue) -> bool {
    tags::split(&d.text).iter().any(|p| matches!(p, tags::Part::Block(b) if ["\\an7", "\\an8", "\\an9", "\\a5", "\\a6", "\\a7"].iter().any(|t| b.contains(t))))
}

fn body_xml(d: &Dialogue) -> String {
    let mut s = String::new();
    for (t, i, b, u) in styled_runs(&d.text) {
        let esc = t.split('\n').map(escape).collect::<Vec<_>>().join("<br/>");
        if !(i || b || u) {
            s.push_str(&esc);
            continue;
        }
        s.push_str("<span");
        if i {
            s.push_str(" tts:fontStyle=\"italic\"");
        }
        if b {
            s.push_str(" tts:fontWeight=\"bold\"");
        }
        if u {
            s.push_str(" tts:textDecoration=\"underline\"");
        }
        s.push('>');
        s.push_str(&esc);
        s.push_str("</span>");
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    /// TTML1/DFXP, `hh:mm:ss.mmm`
    Ttml,
    /// Apple iTunes Timed Text: kare zaman kodu (`ttp:frameRate`), sabit bölgeler.
    Itt { fps: f64 },
}

/// Kare zaman kodu: tam saniyeler + kalan kareler (TTML media timeBase).
fn frame_tc(ms: i64, fps: f64) -> String {
    let nominal = fps.round().max(1.0);
    let ms = ms.max(0);
    let mut s = ms / 1000;
    let mut f = (((ms % 1000) as f64) * fps / 1000.0).round() as i64;
    if f >= nominal as i64 {
        f = 0;
        s += 1;
    }
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, f)
}

pub fn write(script: &Script, profile: Profile) -> String {
    let lang = script.info("Language").unwrap_or("tr");
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let (tc, head_attrs): (Box<dyn Fn(i64) -> String>, String) = match profile {
        Profile::Ttml => (Box::new(time::format_vtt), String::new()),
        Profile::Itt { fps } => {
            let nominal = fps.round();
            let mult = if (fps - nominal).abs() > 0.001 { " ttp:frameRateMultiplier=\"1000 1001\"" } else { "" };
            (Box::new(move |ms| frame_tc(ms, fps)), format!(" ttp:timeBase=\"media\" ttp:frameRate=\"{}\"{}", nominal, mult))
        }
    };
    out.push_str(&format!(
        "<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xmlns:ttm=\"http://www.w3.org/ns/ttml#metadata\" xmlns:ttp=\"http://www.w3.org/ns/ttml#parameter\"{} xml:lang=\"{}\">\n",
        head_attrs,
        escape(lang)
    ));
    out.push_str("  <head>\n    <styling>\n      <style xml:id=\"normal\" tts:fontFamily=\"sansSerif\" tts:fontSize=\"100%\" tts:color=\"white\" tts:textAlign=\"center\"/>\n    </styling>\n");
    out.push_str("    <layout>\n      <region xml:id=\"bottom\" tts:origin=\"0% 85%\" tts:extent=\"100% 15%\" tts:textAlign=\"center\" tts:displayAlign=\"after\"/>\n      <region xml:id=\"top\" tts:origin=\"0% 0%\" tts:extent=\"100% 15%\" tts:textAlign=\"center\" tts:displayAlign=\"before\"/>\n    </layout>\n  </head>\n");
    out.push_str("  <body style=\"normal\" region=\"bottom\">\n    <div>\n");
    for d in script.dialogues().filter(|d| !d.comment) {
        let body = body_xml(d);
        if body.trim().is_empty() {
            continue;
        }
        let reg = if is_top(d) { " region=\"top\"" } else { "" };
        out.push_str(&format!("      <p begin=\"{}\" end=\"{}\"{}>{}</p>\n", tc(d.start_time), tc(d.end_time), reg, body));
    }
    out.push_str("    </div>\n  </body>\n</tt>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netflix_style_dfxp() {
        let src = r#"<?xml version="1.0" encoding="utf-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tts="http://www.w3.org/ns/ttml#styling" xmlns:ttp="http://www.w3.org/ns/ttml#parameter" ttp:tickRate="10000000" ttp:frameRate="24" ttp:frameRateMultiplier="1000 1001">
<head><styling><style xml:id="s1" tts:fontStyle="italic"/></styling>
<layout><region xml:id="top" tts:origin="10% 5%" tts:extent="80% 20%"/><region xml:id="bot" tts:origin="10% 80%" tts:extent="80% 20%" tts:displayAlign="after"/></layout></head>
<body region="bot"><div>
<p begin="10010000t" end="30030000t">Merhaba<br/>
   <span style="s1">dünya</span> &amp; sen</p>
<p begin="00:00:05:12" dur="2s" region="top">Üst &#x15F;</p>
<!-- yorum <p begin="1s" end="2s">yok</p> -->
<p begin="1h" end="3600.5s"><span tts:fontStyle="italic">a</span>b</p>
</div></body></tt>"#;
        let d = parse(src);
        assert_eq!(d.len(), 3);
        assert_eq!((d[0].start_time, d[0].end_time), (1001, 3003));
        assert_eq!(d[0].text, "Merhaba\\N{\\i1}dünya{\\i0} & sen");
        // 12 kare / 23.976 = 500.5 ms
        assert_eq!(d[1].start_time, 5501);
        assert_eq!(d[1].end_time, 7501);
        assert_eq!(d[1].text, "{\\an8}Üst ş");
        assert_eq!(d[2].text, "{\\i1}a{\\i0}b");
        assert_eq!(d[2].end_time, 3_600_500);
        let br = parse(r#"<p begin="1s" end="2s"><span tts:fontStyle="italic">a</span><br/>b</p>"#);
        assert_eq!(br[0].text, "{\\i1}a{\\i0}\\Nb");
    }

    #[test]
    fn ebu_tt_prefixed() {
        let src = r#"<tt:tt xmlns:tt="http://www.w3.org/ns/ttml"><tt:body><tt:div><tt:p xml:id="s1" begin="00:00:01.000" end="00:00:02.500"><tt:span>Bir</tt:span><tt:br/><tt:span>İki</tt:span></tt:p></tt:div></tt:body></tt:tt>"#;
        let d = parse(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].text, "Bir\\Nİki");
        assert_eq!(d[0].end_time, 2500);
    }

    #[test]
    fn write_read_roundtrip_both_profiles() {
        let mut s = Script::from_dialogues(vec![
            Dialogue { start_time: 1000, end_time: 2500, text: "{\\i1}Eğik{\\i0} düz\\N<ikinci> & satır".into(), ..Default::default() },
            Dialogue { start_time: 3000, end_time: 4000, text: "{\\an8\\b1}Üst".into(), ..Default::default() },
            Dialogue { start_time: 5000, end_time: 6000, text: "{\\p1}m 0 0 l 10 10{\\p0}".into(), ..Default::default() },
        ]);
        s.set_info("Language", "tr");
        let t = write(&s, Profile::Ttml);
        let back = parse(&t);
        assert_eq!(back.len(), 2, "{t}");
        assert_eq!(back[0].text, "{\\i1}Eğik{\\i0} düz\\N<ikinci> & satır");
        assert_eq!(back[1].text, "{\\an8}{\\b1}Üst{\\b0}");
        let itt = write(&s, Profile::Itt { fps: 24000.0 / 1001.0 });
        assert!(itt.contains("ttp:frameRate=\"24\" ttp:frameRateMultiplier=\"1000 1001\""));
        assert!(itt.contains("begin=\"00:00:01:00\" end=\"00:00:02:12\""), "{itt}");
        let b2 = parse(&itt);
        assert_eq!(b2[0].start_time, 1000);
        assert!((b2[0].end_time - 2500).abs() <= 21);
    }

    #[test]
    fn frame_tc_rounds_up() {
        assert_eq!(frame_tc(1999, 24.0), "00:00:02:00");
        assert_eq!(frame_tc(3_661_041, 24.0), "01:01:01:01");
    }
}
