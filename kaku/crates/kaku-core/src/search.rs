//! Find & replace over lines (plain / regex, case, field selection).

use crate::ass::Script;
use regex::{Regex, RegexBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Text,
    TextNoTags,
    Style,
    Actor,
    Effect,
}

pub struct Query {
    re: Regex,
    pub field: Field,
}

impl Query {
    pub fn new(pattern: &str, regex: bool, case_sensitive: bool, whole_word: bool, field: Field) -> Result<Query, regex::Error> {
        let mut p = if regex { pattern.to_string() } else { regex::escape(pattern) };
        if whole_word {
            p = format!(r"\b(?:{p})\b");
        }
        Ok(Query { re: RegexBuilder::new(&p).case_insensitive(!case_sensitive).build()?, field })
    }
}

/// Returns the indices (among `lines()`) of matching lines.
pub fn find(script: &Script, q: &Query) -> Vec<usize> {
    script
        .lines()
        .enumerate()
        .filter(|(_, l)| match q.field {
            Field::Text => q.re.is_match(&l.text),
            Field::TextNoTags => q.re.is_match(&crate::tags::visible_text(&l.text)),
            Field::Style => q.re.is_match(&l.style),
            Field::Actor => q.re.is_match(&l.actor),
            Field::Effect => q.re.is_match(&l.effect),
        })
        .map(|(i, _)| i)
        .collect()
}

/// Replace; returns number of lines changed. In `TextNoTags` mode only plain text segments are touched.
pub fn replace(script: &mut Script, q: &Query, with: &str) -> usize {
    let mut n = 0;
    for l in script.lines_mut() {
        let new = match q.field {
            Field::Text => q.re.replace_all(&l.text, with).into_owned(),
            Field::TextNoTags => {
                let segs: Vec<crate::tags::Seg> = crate::tags::parse(&l.text)
                    .into_iter()
                    .map(|s| match s {
                        crate::tags::Seg::Text(t) => crate::tags::Seg::Text(q.re.replace_all(&t, with).into_owned()),
                        b => b,
                    })
                    .collect();
                crate::tags::render(&segs)
            }
            Field::Style => q.re.replace_all(&l.style, with).into_owned(),
            Field::Actor => q.re.replace_all(&l.actor, with).into_owned(),
            Field::Effect => q.re.replace_all(&l.effect, with).into_owned(),
        };
        let slot = match q.field {
            Field::Text | Field::TextNoTags => &mut l.text,
            Field::Style => &mut l.style,
            Field::Actor => &mut l.actor,
            Field::Effect => &mut l.effect,
        };
        if *slot != new {
            *slot = new;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;
    #[test]
    fn replace_text_only() {
        let mut s = Script::new_default();
        s.push_line(Line::new(0, 1, "{\\i1}i am{\\i0} i"));
        let q = Query::new("i", false, true, true, Field::TextNoTags).unwrap();
        assert_eq!(replace(&mut s, &q, "I"), 1);
        assert_eq!(s.lines().next().unwrap().text, "{\\i1}I am{\\i0} I");
        let q = Query::new("AM", false, false, false, Field::Text).unwrap();
        assert_eq!(find(&s, &q), vec![0]);
    }
}
