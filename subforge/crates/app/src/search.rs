//! Find & replace across events (Turkish-aware case folding).
use fancy_regex::Regex;
use subforge_core::ass::{Event, Script};
use subforge_core::turkish;

#[derive(Debug, Clone)]
pub struct SearchOpts {
    pub query: String,
    pub replace: String,
    pub regex: bool,
    pub match_case: bool,
    pub in_text: bool,
    pub in_actor: bool,
    pub in_style: bool,
    pub in_effect: bool,
    /// ignore ASS tags (`{...}`) when matching text (only for plain search)
    pub skip_comments: bool,
    pub only_selected: bool,
}

impl Default for SearchOpts {
    fn default() -> Self {
        SearchOpts {
            query: String::new(),
            replace: String::new(),
            regex: false,
            match_case: false,
            in_text: true,
            in_actor: false,
            in_style: false,
            in_effect: false,
            skip_comments: false,
            only_selected: false,
        }
    }
}

pub struct Matcher {
    re: Regex,
}

impl Matcher {
    pub fn new(o: &SearchOpts) -> Result<Matcher, String> {
        if o.query.is_empty() {
            return Err("empty query".into());
        }
        let mut pat = if o.regex { o.query.clone() } else { fancy_regex::escape(&o.query).to_string() };
        if !o.match_case {
            // Turkish: make i/İ and ı/I interchangeable pairs explicitly
            pat = fold_turkish_classes(&pat, o.regex);
            pat = format!("(?i){pat}");
        }
        Ok(Matcher { re: Regex::new(&pat).map_err(|e| e.to_string())? })
    }

    pub fn is_match(&self, s: &str) -> bool {
        self.re.is_match(s).unwrap_or(false)
    }

    pub fn replace_all(&self, s: &str, with: &str) -> String {
        self.re.replace_all(s, with).into_owned()
    }
}

/// In case-insensitive plain searches treat i≡İ, ı≡I so "istanbul" finds "İSTANBUL".
fn fold_turkish_classes(pat: &str, is_regex: bool) -> String {
    if is_regex {
        return pat.to_string();
    }
    let mut out = String::new();
    for c in pat.chars() {
        match c {
            'i' | 'İ' => out.push_str("[iİ]"),
            'ı' | 'I' => out.push_str("[ıI]"),
            _ => out.push(c),
        }
    }
    out
}

fn fields<'a>(e: &'a Event, o: &SearchOpts) -> Vec<&'a str> {
    let mut v = vec![];
    if o.in_text {
        v.push(e.text.as_str());
    }
    if o.in_actor {
        v.push(e.actor.as_str());
    }
    if o.in_style {
        v.push(e.style.as_str());
    }
    if o.in_effect {
        v.push(e.effect.as_str());
    }
    v
}

/// Indices of events matching, in order.
pub fn find_all(script: &Script, o: &SearchOpts, selection: &[usize]) -> Result<Vec<usize>, String> {
    let m = Matcher::new(o)?;
    Ok(script
        .events
        .iter()
        .enumerate()
        .filter(|(i, _)| !o.only_selected || selection.contains(i))
        .filter(|(_, e)| fields(e, o).iter().any(|f| m.is_match(f)))
        .map(|(i, _)| i)
        .collect())
}

/// Next match strictly after `from` (wraps around).
pub fn find_next(script: &Script, o: &SearchOpts, from: Option<usize>, selection: &[usize]) -> Result<Option<usize>, String> {
    let all = find_all(script, o, selection)?;
    let from = from.map(|f| f as isize).unwrap_or(-1);
    Ok(all.iter().copied().find(|&i| i as isize > from).or_else(|| all.first().copied()))
}

/// Replace in all matching events, returns number of events changed.
pub fn replace_all(script: &mut Script, o: &SearchOpts, selection: &[usize]) -> Result<usize, String> {
    let m = Matcher::new(o)?;
    let mut n = 0;
    for (i, e) in script.events.iter_mut().enumerate() {
        if o.only_selected && !selection.contains(&i) {
            continue;
        }
        let mut changed = false;
        let mut apply = |f: &mut String| {
            if m.is_match(f) {
                let r = m.replace_all(f, &o.replace);
                if r != *f {
                    *f = r;
                    changed = true;
                }
            }
        };
        if o.in_text {
            apply(&mut e.text);
        }
        if o.in_actor {
            apply(&mut e.actor);
        }
        if o.in_style {
            apply(&mut e.style);
        }
        if o.in_effect {
            apply(&mut e.effect);
        }
        if changed {
            n += 1;
        }
    }
    Ok(n)
}

#[allow(dead_code)]
fn _t() -> String {
    turkish::upper("x")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn script() -> Script {
        let mut s = Script::new();
        s.events = vec![Event::new(0, 1, "Default", "İstanbul'a gidiyorum"), Event::new(0, 1, "Default", "ışık yok"), Event::new(0, 1, "Default", "hello")];
        s
    }
    #[test]
    fn turkish_insensitive() {
        let s = script();
        let o = SearchOpts { query: "istanbul".into(), ..Default::default() };
        assert_eq!(find_all(&s, &o, &[]).unwrap(), vec![0]);
        let o = SearchOpts { query: "ISIK".into(), ..Default::default() };
        assert_eq!(find_all(&s, &o, &[]).unwrap(), Vec::<usize>::new());
        let o = SearchOpts { query: "Işık".into(), ..Default::default() };
        assert_eq!(find_all(&s, &o, &[]).unwrap(), vec![1]);
    }
    #[test]
    fn replace() {
        let mut s = script();
        let o = SearchOpts { query: "(\\w+)o".into(), replace: "${1}0".into(), regex: true, ..Default::default() };
        let n = replace_all(&mut s, &o, &[]).unwrap();
        assert!(n >= 1);
        assert_eq!(s.events[2].text, "hell0");
    }
}

// ---------------------------------------------------------------- select lines

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Subtract,
    Intersect,
}

#[derive(Debug, Clone)]
pub struct SelectOpts {
    pub query: String,
    pub regex: bool,
    pub match_case: bool,
    pub in_text: bool,
    pub in_actor: bool,
    pub in_style: bool,
    pub in_effect: bool,
    pub only_dialogue: bool,
    pub only_comments: bool,
    pub invert: bool,
    pub mode: SelectMode,
}

impl Default for SelectOpts {
    fn default() -> Self {
        SelectOpts { query: String::new(), regex: false, match_case: false, in_text: true, in_actor: false, in_style: false, in_effect: false, only_dialogue: false, only_comments: false, invert: false, mode: SelectMode::Replace }
    }
}

/// Compute the new selection (event indices) according to `o` applied to `current`.
pub fn select_lines(script: &Script, o: &SelectOpts, current: &[usize]) -> Result<Vec<usize>, String> {
    let so = SearchOpts { query: o.query.clone(), regex: o.regex, match_case: o.match_case, in_text: o.in_text, in_actor: o.in_actor, in_style: o.in_style, in_effect: o.in_effect, ..Default::default() };
    let matcher = if o.query.is_empty() { None } else { Some(Matcher::new(&so)?) };
    let hits: Vec<usize> = script
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| !(o.only_dialogue && e.is_comment()) && !(o.only_comments && !e.is_comment()))
        .filter(|(_, e)| match &matcher {
            None => true,
            Some(m) => fields(e, &so).iter().any(|f| m.is_match(f)) != o.invert,
        })
        .map(|(i, _)| i)
        .collect();
    let cur: std::collections::BTreeSet<usize> = current.iter().copied().collect();
    let hit_set: std::collections::BTreeSet<usize> = hits.into_iter().collect();
    let out: std::collections::BTreeSet<usize> = match o.mode {
        SelectMode::Replace => hit_set,
        SelectMode::Add => cur.union(&hit_set).copied().collect(),
        SelectMode::Subtract => cur.difference(&hit_set).copied().collect(),
        SelectMode::Intersect => cur.intersection(&hit_set).copied().collect(),
    };
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod select_tests {
    use super::*;
    #[test]
    fn select_modes() {
        let mut s = Script::new();
        s.events = vec![Event::new(0, 1, "A", "foo"), Event::new(0, 1, "B", "foo bar"), Event::new(0, 1, "A", "baz")];
        let o = SelectOpts { query: "foo".into(), ..Default::default() };
        assert_eq!(select_lines(&s, &o, &[]).unwrap(), vec![0, 1]);
        let o2 = SelectOpts { query: "A".into(), in_text: false, in_style: true, match_case: true, mode: SelectMode::Intersect, ..Default::default() };
        assert_eq!(select_lines(&s, &o2, &[0, 1]).unwrap(), vec![0]);
        let o3 = SelectOpts { query: "foo".into(), invert: true, ..Default::default() };
        assert_eq!(select_lines(&s, &o3, &[]).unwrap(), vec![2]);
    }
}
