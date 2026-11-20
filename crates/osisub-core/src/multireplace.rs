//! Çoklu değiştir (Subtitle Edit "Multiple replace"): sırayla uygulanan bul → değiştir kuralları.
//! Etiket blokları korunur (yalnızca görünür metin), yorumlar atlanır. Kurallar TSV olarak
//! saklanır: `açık<TAB>regex<TAB>harf<TAB>sözcük<TAB>bul<TAB>değiştir` (sekme/satır sonu `\t`/`\n`).
use crate::ass::Script;
use crate::search::{self, Field, Options};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub on: bool,
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
}

impl Default for Rule {
    fn default() -> Self {
        Rule { on: true, find: String::new(), replace: String::new(), regex: false, match_case: true, whole_word: false }
    }
}

fn opts(r: &Rule, only: Option<&[usize]>) -> Options {
    Options {
        pattern: r.find.clone(),
        replacement: r.replace.clone(),
        regex: r.regex,
        match_case: r.match_case,
        whole_word: r.whole_word,
        fields: vec![Field::Text],
        skip_comments: true,
        only: only.map(|o| o.to_vec()),
        ignore_tags: true,
    }
}

/// Kuralları sırayla uygular; her kural için (değişen satır sayısı ya da hata).
pub fn apply(script: &mut Script, rules: &[Rule], only: Option<&[usize]>) -> Vec<Result<usize, String>> {
    rules.iter().map(|r| if !r.on || r.find.is_empty() { Ok(0) } else { search::replace_all(script, &opts(r, only)) }).collect()
}

/// Uygulamadan önizleme: her kuralın (sırayla uygulanmış gibi) değiştireceği satır sayısı.
pub fn preview(script: &Script, rules: &[Rule], only: Option<&[usize]>) -> Vec<Result<usize, String>> {
    let mut s = script.clone();
    apply(&mut s, rules, only)
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(o) => { out.push('\\'); out.push(o); }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn to_tsv(rules: &[Rule]) -> String {
    let b = |v: bool| if v { "1" } else { "0" };
    rules.iter().map(|r| format!("{}\t{}\t{}\t{}\t{}\t{}\n", b(r.on), b(r.regex), b(r.match_case), b(r.whole_word), esc(&r.find), esc(&r.replace))).collect()
}

pub fn from_tsv(t: &str) -> Vec<Rule> {
    // Not Defteri'nin UTF-8 BOM'u ilk kuralın "açık" alanını bozmasın
    t.trim_start_matches('\u{feff}').lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(6, '\t').collect();
            if p.len() < 6 { return None; }
            let f = |s: &str| s.trim() == "1";
            Some(Rule { on: f(p[0]), regex: f(p[1]), match_case: f(p[2]), whole_word: f(p[3]), find: unesc(p[4]), replace: unesc(p[5]) })
        })
        .collect()
}

/// Yeni kullanıcı için örnek kurallar (kapalı): Türkçe tipografi.
pub fn examples() -> Vec<Rule> {
    vec![
        Rule { on: false, find: "...".into(), replace: "…".into(), ..Default::default() },
        Rule { on: false, find: r"\s+([,.!?…])".into(), replace: "$1".into(), regex: true, ..Default::default() },
        Rule { on: false, find: "''".into(), replace: "\"".into(), ..Default::default() },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::{Dialogue, Entry};

    #[test]
    fn kurallar() {
        let mut s = Script::from_dialogues(vec![
            Dialogue { text: "{\\i1}Bekle... Naruto-kun !{\\i0}".into(), ..Default::default() },
            Dialogue { text: "naruto geldi".into(), comment: true, ..Default::default() },
            Dialogue { text: "Naruto ve Narutomaki".into(), ..Default::default() },
        ]);
        let rules = vec![
            Rule { find: "...".into(), replace: "…".into(), ..Default::default() },
            Rule { find: r"\s+([!?])".into(), replace: "$1".into(), regex: true, ..Default::default() },
            Rule { find: "Naruto".into(), replace: "Naruto".into(), on: false, ..Default::default() },
            Rule { find: "naruto".into(), replace: "Naruto".into(), match_case: false, whole_word: true, ..Default::default() },
            Rule { find: "(".into(), replace: "x".into(), regex: true, ..Default::default() },
        ];
        let pv = preview(&s, &rules, None);
        assert_eq!(pv[0], Ok(1));
        assert!(pv[4].is_err(), "bozuk regex hata vermeli");
        let r = apply(&mut s, &rules, None);
        assert_eq!(r[0..4], [Ok(1), Ok(1), Ok(0), Ok(0)], "yorum atlanır, aynı kalan değişiklik sayılmaz");
        let t: Vec<String> = s.entries.iter().filter_map(|e| if let Entry::Dialogue(d) = e { Some(d.text.clone()) } else { None }).collect();
        assert_eq!(t, ["{\\i1}Bekle… Naruto-kun!{\\i0}", "naruto geldi", "Naruto ve Narutomaki"]);
    }

    #[test]
    fn tsv_gidis_donus() {
        let mut rules = examples();
        rules.push(Rule { find: "a\tb\\c\nd".into(), replace: "x".into(), regex: true, whole_word: true, ..Default::default() });
        assert_eq!(from_tsv(&to_tsv(&rules)), rules);
        assert!(from_tsv(&format!("\u{feff}{}", to_tsv(&[Rule { find: "a".into(), ..Default::default() }])))[0].on);
    }
}
