//! Aegisub "Satırları seç" (Select Lines): alana göre eşleşen satırları seçimle birleştirir.
use crate::ass::{Entry, Script};
use crate::search::{self, Field};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Alan metni tam olarak desen (büyük/küçük harf ayarına göre)
    Exact,
    Contains,
    Regex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Set,
    Add,
    Subtract,
    Intersect,
}

#[derive(Clone, Debug)]
pub struct Criteria {
    pub pattern: String,
    pub field: Field,
    pub mode: Mode,
    pub match_case: bool,
    /// Eşleşmeyenleri seç
    pub invert: bool,
    pub dialogues: bool,
    pub comments: bool,
    pub action: Action,
}

impl Default for Criteria {
    fn default() -> Self {
        Criteria { pattern: String::new(), field: Field::Text, mode: Mode::Contains, match_case: false, invert: false, dialogues: true, comments: false, action: Action::Set }
    }
}

/// Yeni seçimi (artan sıralı entry indeksleri) döndürür. Boş desen "Tam" kipinde boş alanı eşler.
pub fn select(script: &Script, c: &Criteria, current: &[usize]) -> Result<Vec<usize>, String> {
    let re = match c.mode {
        Mode::Exact if c.pattern.is_empty() => None,
        _ => {
            if c.pattern.is_empty() {
                return Err("Boş desen".into());
            }
            // Düz kiplerde search::build Türkçe i/İ ı/I eşlemesini uygular; Tam kipte eşleşme tüm alanı kaplamalı.
            let o = search::Options { pattern: c.pattern.clone(), regex: c.mode == Mode::Regex, match_case: c.match_case, ..Default::default() };
            Some(search::build(&o)?)
        }
    };
    let mut matched = vec![];
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if (d.comment && !c.comments) || (!d.comment && !c.dialogues) {
            continue;
        }
        let v = match c.field {
            Field::Text => &d.text,
            Field::Actor => &d.actor,
            Field::Effect => &d.effect,
            Field::Style => &d.style,
        };
        let hit = match &re {
            None => v.is_empty(),
            Some(r) if c.mode == Mode::Exact => matches!(r.find(v), Ok(Some(m)) if m.start() == 0 && m.end() == v.len()),
            Some(r) => r.is_match(v).unwrap_or(false),
        };
        if hit != c.invert {
            matched.push(i);
        }
    }
    let mut cur: Vec<usize> = current.to_vec();
    cur.sort_unstable();
    cur.dedup();
    let out = match c.action {
        Action::Set => matched,
        Action::Add => {
            let mut v = cur;
            v.extend(matched);
            v.sort_unstable();
            v.dedup();
            v
        }
        Action::Subtract => cur.into_iter().filter(|i| matched.binary_search(i).is_err()).collect(),
        Action::Intersect => cur.into_iter().filter(|i| matched.binary_search(i).is_ok()).collect(),
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn sc() -> Script {
        let mk = |style: &str, actor: &str, text: &str, comment: bool| Dialogue { style: style.into(), actor: actor.into(), text: text.into(), comment, ..Default::default() };
        Script::from_dialogues(vec![mk("Default", "Sakura", "Merhaba", false), mk("Sign", "", "{\\pos(1,2)}OKUL", false), mk("Default", "Kenji", "İyi geceler", false), mk("Sign", "", "not", true)])
    }

    fn idx(s: &Script, v: &[usize]) -> Vec<usize> {
        // entry indekslerini 0 tabanlı diyalog sırasına çevir
        let ds: Vec<usize> = s.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Dialogue(_))).map(|(i, _)| i).collect();
        v.iter().map(|i| ds.iter().position(|d| d == i).unwrap()).collect()
    }

    #[test]
    fn modes_and_actions() {
        let s = sc();
        let c = Criteria { pattern: "Sign".into(), field: Field::Style, mode: Mode::Exact, ..Default::default() };
        let r = select(&s, &c, &[]).unwrap();
        assert_eq!(idx(&s, &r), vec![1]);
        let c2 = Criteria { comments: true, ..c.clone() };
        assert_eq!(idx(&s, &select(&s, &c2, &[]).unwrap()), vec![1, 3]);
        // Türkçe büyük/küçük harf: "iyi" → "İyi"
        let c3 = Criteria { pattern: "iyi".into(), ..Default::default() };
        let r3 = select(&s, &c3, &[]).unwrap();
        assert_eq!(idx(&s, &r3), vec![2]);
        // ekle / çıkar / kesişim
        let add = select(&s, &Criteria { action: Action::Add, ..c3.clone() }, &r).unwrap();
        assert_eq!(idx(&s, &add), vec![1, 2]);
        let sub = select(&s, &Criteria { action: Action::Subtract, ..c3.clone() }, &add).unwrap();
        assert_eq!(idx(&s, &sub), vec![1]);
        let int = select(&s, &Criteria { action: Action::Intersect, ..c3.clone() }, &add).unwrap();
        assert_eq!(idx(&s, &int), vec![2]);
        // regex + ters
        let rx = Criteria { pattern: r"^\{\\pos".into(), mode: Mode::Regex, invert: true, ..Default::default() };
        assert_eq!(idx(&s, &select(&s, &rx, &[]).unwrap()), vec![0, 2]);
        // boş tam = boş alan (konuşanı olmayanlar)
        let empty = Criteria { field: Field::Actor, mode: Mode::Exact, ..Default::default() };
        assert_eq!(idx(&s, &select(&s, &empty, &[]).unwrap()), vec![1]);
        assert!(select(&s, &Criteria { pattern: "(".into(), mode: Mode::Regex, ..Default::default() }, &[]).is_err());
    }
}
