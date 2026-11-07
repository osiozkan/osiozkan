//! Find & replace over events.
use crate::ass::AssFile;
use regex::RegexBuilder;

#[derive(Clone, Debug)]
pub struct SearchOpts {
    pub pattern: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub skip_tags: bool,
    pub field: Field,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field { Text, Actor, Style, Effect }

fn build(o: &SearchOpts) -> Result<regex::Regex, regex::Error> {
    let mut p = if o.regex { o.pattern.clone() } else { regex::escape(&o.pattern) };
    if o.whole_word { p = format!(r"\b(?:{})\b", p); }
    RegexBuilder::new(&p).case_insensitive(!o.case_sensitive).build()
}

pub fn find_all(f: &AssFile, o: &SearchOpts) -> Result<Vec<usize>, regex::Error> {
    let re = build(o)?;
    Ok(f.events.iter().enumerate().filter(|(_, e)| {
        let s = match o.field { Field::Text => &e.text, Field::Actor => &e.actor, Field::Style => &e.style, Field::Effect => &e.effect };
        if o.field == Field::Text && o.skip_tags { re.is_match(&crate::tags::strip_tags(s)) } else { re.is_match(s) }
    }).map(|(i, _)| i).collect())
}

/// Returns number of events changed.
pub fn replace_all(f: &mut AssFile, o: &SearchOpts, with: &str) -> Result<usize, regex::Error> {
    let re = build(o)?;
    let rep = if o.regex { with.to_string() } else { with.replace('$', "$$") };
    let mut n = 0;
    for e in &mut f.events {
        let field = match o.field { Field::Text => &mut e.text, Field::Actor => &mut e.actor, Field::Style => &mut e.style, Field::Effect => &mut e.effect };
        let new = if o.field == Field::Text && o.skip_tags {
            crate::tags::map_plain(field, |s| re.replace_all(s, rep.as_str()).into_owned())
        } else { re.replace_all(field, rep.as_str()).into_owned() };
        if new != *field { *field = new; n += 1; }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;
    #[test]
    fn replace() {
        let mut f = AssFile::new_default();
        f.events.push(Event { text: "{\\i1}foo{\\i0} Foo".into(), ..Default::default() });
        let o = SearchOpts { pattern: "foo".into(), regex: false, case_sensitive: false, whole_word: false, skip_tags: true, field: Field::Text };
        assert_eq!(find_all(&f, &o).unwrap(), vec![0]);
        assert_eq!(replace_all(&mut f, &o, "bar").unwrap(), 1);
        assert_eq!(f.events[0].text, "{\\i1}bar{\\i0} bar");
    }
}
