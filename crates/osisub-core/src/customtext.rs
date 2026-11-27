//! Özel metin biçimiyle dışa aktarma (Subtitle Edit Export custom text format): başlık + satır başına
//! şablon + son kısım. Çeviri notları, kontrol çizelgeleri, dublaj metinleri için.
//!
//! Şablon belirteçleri: `{number}` `{start}` `{end}` `{duration}` `{text}` (etiketsiz) `{text_tags}`
//! `{actor}` `{style}` `{effect}` `{layer}` `{tab}` `{newline}`; başlık/sonda `{title}` `{count}`.
//! Zaman biçimi: `hh` `h` `mm` `ss` `zzz` (ms) `zz` (cs) `s` (toplam saniye, ondalıklı).
use crate::ass::{Entry, Script};

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub header: String,
    pub line: String,
    pub footer: String,
    pub time_format: String,
    /// Metindeki satır sonları (`\N`) bununla değiştirilir
    pub newline: String,
}

impl Default for Template {
    fn default() -> Self {
        Template { header: String::new(), line: "{number}{tab}{start}{tab}{end}{tab}{actor}{tab}{text}{newline}".into(), footer: String::new(), time_format: "hh:mm:ss.zzz".into(), newline: " | ".into() }
    }
}

pub fn format_time(ms: i64, fmt: &str) -> String {
    let ms = ms.max(0);
    let (h, m, s, z) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let mut out = String::new();
    let c: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    let run = |i: usize, ch: char| c[i..].iter().take_while(|&&x| x == ch).count();
    while i < c.len() {
        let n = run(i, c[i]);
        match (c[i], n) {
            ('h', 1) => out.push_str(&h.to_string()),
            ('h', _) => out.push_str(&format!("{:02}", h)),
            ('m', _) => out.push_str(&format!("{:02}", m)),
            ('s', 1) => out.push_str(&format!("{:.3}", ms as f64 / 1000.0)),
            ('s', _) => out.push_str(&format!("{:02}", s)),
            ('z', 2) => out.push_str(&format!("{:02}", z / 10)),
            ('z', _) => out.push_str(&format!("{:03}", z)),
            (ch, n) => (0..n).for_each(|_| out.push(ch)),
        }
        i += n;
    }
    out
}

fn fill(tpl: &str, vals: &[(&str, String)]) -> String {
    let mut s = tpl.to_string();
    for (k, v) in vals {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Yorum olmayan diyalog satırlarını şablonla yazar.
pub fn render(script: &Script, t: &Template, title: &str) -> String {
    let lines: Vec<_> = script.entries.iter().filter_map(|e| match e { Entry::Dialogue(d) if !d.comment => Some(d), _ => None }).collect();
    let common = |v: &mut Vec<(&str, String)>| {
        v.push(("tab", "\t".into()));
        v.push(("newline", "\n".into()));
        v.push(("title", title.into()));
        v.push(("count", lines.len().to_string()));
    };
    let mut hv = vec![];
    common(&mut hv);
    let mut out = fill(&t.header, &hv);
    for (n, d) in lines.iter().enumerate() {
        let nl = |x: &str| x.replace("\\N", &t.newline).replace("\\n", &t.newline);
        let mut v = vec![
            ("number", (n + 1).to_string()),
            ("start", format_time(d.start_time, &t.time_format)),
            ("end", format_time(d.end_time, &t.time_format)),
            ("duration", format_time(d.duration(), &t.time_format)),
            ("text_tags", nl(&d.text)),
            ("text", nl(&crate::tags::strip_tags(&d.text).replace("\\h", " "))),
            ("actor", d.actor.clone()),
            ("style", d.style.clone()),
            ("effect", d.effect.clone()),
            ("layer", d.layer.to_string()),
        ];
        common(&mut v);
        out.push_str(&fill(&t.line, &v));
    }
    out.push_str(&fill(&t.footer, &hv));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    #[test]
    fn times_and_template() {
        assert_eq!(format_time(3_723_456, "hh:mm:ss.zzz"), "01:02:03.456");
        assert_eq!(format_time(3_723_456, "h:mm:ss,zz"), "1:02:03,45");
        assert_eq!(format_time(1500, "s"), "1.500");
        let mut s = Script::default();
        for (t, a, c) in [("{\\i1}Merhaba\\Ndünya", "Saki", false), ("not", "", true), ("İkinci", "Ken", false)] {
            let mut d = Dialogue::default();
            d.text = t.into();
            d.actor = a.into();
            d.comment = c;
            d.start_time = 1000;
            d.end_time = 2500;
            s.entries.push(Entry::Dialogue(d));
        }
        let t = Template { header: "# {title} ({count}){newline}".into(), ..Default::default() };
        assert_eq!(render(&s, &t, "ep01"), "# ep01 (2)\n1\t00:00:01.000\t00:00:02.500\tSaki\tMerhaba | dünya\n2\t00:00:01.000\t00:00:02.500\tKen\tİkinci\n");
        let t2 = Template { line: "{text_tags}/{duration}{newline}".into(), time_format: "s".into(), newline: "<br>".into(), ..Default::default() };
        assert!(render(&s, &t2, "").starts_with("{\\i1}Merhaba<br>dünya/1.500\n"));
    }
}
