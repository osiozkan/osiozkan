//! Multiple replace (Subtitle Edit): saved groups of find/replace rules run in order.
//!
//! File (UTF-8, tab separated; `#` lines are comments):
//! ```text
//! @group<TAB>Tipografi<TAB>1
//! 1<TAB>regex<TAB> +([,.!?])<TAB>$1<TAB>noktalama öncesi boşluk
//! 0<TAB>plain<TAB>...<TAB>…<TAB>
//! ```
//! Kinds: `plain` (exact text), `nocase` (ignores case, also Turkish i/İ/ı/I), `regex` (Rust regex
//! syntax, `$1` in the replacement). Tabs and line breaks cannot appear inside a field.

use crate::model::Script;
use crate::tags::{self, Part};
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Plain, NoCase, Regex }

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Plain, Kind::NoCase, Kind::Regex];
    pub fn code(self) -> &'static str { match self { Kind::Plain => "plain", Kind::NoCase => "nocase", Kind::Regex => "regex" } }
    pub fn label(self) -> &'static str { match self { Kind::Plain => "Normal", Kind::NoCase => "Case-insensitive", Kind::Regex => "Regex" } }
    fn parse(s: &str) -> Kind { match s.trim() { "regex" => Kind::Regex, "nocase" => Kind::NoCase, _ => Kind::Plain } }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub enabled: bool,
    pub kind: Kind,
    pub find: String,
    pub replace: String,
    pub note: String,
}

impl Rule {
    pub fn new(kind: Kind, find: &str, replace: &str, note: &str) -> Rule {
        Rule { enabled: true, kind, find: find.into(), replace: replace.into(), note: note.into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub name: String,
    pub enabled: bool,
    pub rules: Vec<Rule>,
}

fn clean(s: &str) -> String { s.replace(['\t', '\n', '\r'], " ") }

pub fn parse(text: &str) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols[0] == "@group" {
            out.push(Group { name: cols.get(1).unwrap_or(&"").to_string(), enabled: cols.get(2).map(|v| v.trim() != "0").unwrap_or(true), rules: vec![] });
            continue;
        }
        if cols.len() < 4 { continue; }
        if out.is_empty() { out.push(Group { name: "Kurallar".into(), enabled: true, rules: vec![] }); }
        let g = out.last_mut().unwrap();
        g.rules.push(Rule { enabled: cols[0].trim() != "0", kind: Kind::parse(cols[1]), find: cols[2].to_string(), replace: cols[3].to_string(), note: cols.get(4).unwrap_or(&"").to_string() });
    }
    out
}

pub fn write(groups: &[Group]) -> String {
    let mut o = String::from("# Yuzu multiple replace\n");
    for g in groups {
        o.push_str(&format!("@group\t{}\t{}\n", clean(&g.name), g.enabled as u8));
        for r in &g.rules {
            o.push_str(&format!("{}\t{}\t{}\t{}\t{}\n", r.enabled as u8, r.kind.code(), clean(&r.find), clean(&r.replace), clean(&r.note)));
        }
    }
    o
}

/// A small example set (off until switched on).
pub fn defaults() -> Vec<Group> {
    vec![Group {
        name: "Tipografi (örnek)".into(),
        enabled: false,
        rules: vec![
            Rule::new(Kind::Regex, " +([,.!?;:…])", "$1", "noktalama öncesi boşluk"),
            Rule::new(Kind::Regex, " {2,}", " ", "çift boşluk"),
            Rule { enabled: false, ..Rule::new(Kind::Plain, "...", "…", "üç nokta → tek karakter") },
        ],
    }]
}

pub struct Compiled {
    re: Regex,
    rep: String,
}

/// Enabled rules of enabled groups, in order. Errors name the group and the rule number.
pub fn compile(groups: &[Group]) -> Result<Vec<Compiled>, String> {
    let mut out = Vec::new();
    for g in groups.iter().filter(|g| g.enabled) {
        for (k, r) in g.rules.iter().enumerate().filter(|(_, r)| r.enabled && !r.find.is_empty()) {
            let (pat, rep) = match r.kind {
                Kind::Regex => (r.find.clone(), r.replace.clone()),
                Kind::Plain => (regex::escape(&r.find), r.replace.replace('$', "$$")),
                Kind::NoCase => (crate::lines::plain_pattern(&r.find, false), r.replace.replace('$', "$$")),
            };
            let re = Regex::new(&pat).map_err(|e| format!("{} / {}: {}", g.name, k + 1, e))?;
            out.push(Compiled { re, rep });
        }
    }
    Ok(out)
}

/// Run the rules over a line. With `text_only`, override blocks are left untouched.
pub fn apply(text: &str, rules: &[Compiled], text_only: bool) -> String {
    let run = |s: &str| -> String {
        let mut cur = s.to_string();
        for r in rules {
            if r.re.is_match(&cur) { cur = r.re.replace_all(&cur, r.rep.as_str()).into_owned(); }
        }
        cur
    };
    if !text_only { return run(text); }
    let parts: Vec<Part> = tags::split_parts(text).into_iter().map(|p| match p { Part::Text(t) => Part::Text(run(&t)), b => b }).collect();
    tags::join_parts(&parts)
}

/// Lines that would change: (row, new text).
pub fn preview(sc: &Script, rules: &[Compiled], rows: Option<&[usize]>, text_only: bool, skip_comments: bool) -> Vec<(usize, String)> {
    let all: Vec<usize>;
    let rows = match rows { Some(r) => r, None => { all = (0..sc.events.len()).collect(); &all } };
    rows.iter().filter_map(|&i| {
        let e = sc.events.get(i)?;
        if skip_comments && e.comment { return None; }
        let new = apply(&e.text, rules, text_only);
        (new != e.text).then_some((i, new))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Event;

    #[test]
    fn file_round_trip() {
        let mut g = defaults();
        g.push(Group { name: "İsimler".into(), enabled: true, rules: vec![Rule::new(Kind::NoCase, "naruto", "Naruto", ""), Rule::new(Kind::Plain, "a\tb", "c", "")] });
        let back = parse(&write(&g));
        assert_eq!(back[0], g[0]);
        assert_eq!(back[1].rules[0], g[1].rules[0]);
        assert_eq!(back[1].rules[1].find, "a b");
        // rules before any group header land in a default group
        let loose = parse("1\tplain\tx\ty\n");
        assert_eq!(loose[0].rules.len(), 1);
    }

    #[test]
    fn rules_run_in_order_outside_tags() {
        let groups = vec![
            Group { name: "A".into(), enabled: true, rules: vec![Rule::new(Kind::Regex, " +([,.!?])", "$1", ""), Rule::new(Kind::Plain, "$", "₺", ""), Rule::new(Kind::NoCase, "ıstanbul", "İstanbul", "")] },
            Group { name: "B".into(), enabled: false, rules: vec![Rule::new(Kind::Plain, "a", "b", "")] },
            Group { name: "C".into(), enabled: true, rules: vec![Rule { enabled: false, ..Rule::new(Kind::Plain, "i", "X", "") }, Rule::new(Kind::Plain, "₺", "TL", "")] },
        ];
        let c = compile(&groups).unwrap();
        assert_eq!(c.len(), 4);
        assert_eq!(apply("{\\an8}ISTANBUL , 5$ !", &c, true), "{\\an8}İstanbul, 5TL!");
        // with text_only off the rules also see the tags
        let t = compile(&[Group { name: "T".into(), enabled: true, rules: vec![Rule::new(Kind::Plain, "an8", "an2", "")] }]).unwrap();
        assert_eq!(apply("{\\an8}an8", &t, true), "{\\an8}an2");
        assert_eq!(apply("{\\an8}an8", &t, false), "{\\an2}an2");
        let mut sc = Script::new();
        sc.events = vec![Event::new(0, 1, "Default", "Merhaba !"), Event::new(1, 2, "Default", "tamam"), Event { comment: true, ..Event::new(2, 3, "Default", "yorum !") }];
        assert_eq!(preview(&sc, &c, None, true, true), vec![(0, "Merhaba!".to_string())]);
        assert_eq!(preview(&sc, &c, None, true, false).len(), 2);
        assert!(preview(&sc, &c, Some(&[1]), true, false).is_empty());
        let bad = vec![Group { name: "Bozuk".into(), enabled: true, rules: vec![Rule::new(Kind::Regex, "(", "", "")] }];
        assert!(compile(&bad).err().unwrap().starts_with("Bozuk / 1:"));
    }
}
