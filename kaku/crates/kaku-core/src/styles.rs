//! Style management: rename (with line updates), duplicate, delete, import, unused styles.

use crate::ass::{Entry, Script, Style};
use std::collections::HashSet;

/// Rename a style and update every line that references it. Returns false if `to` already exists or `from` is missing.
pub fn rename(script: &mut Script, from: &str, to: &str) -> bool {
    if from == to || script.styles().any(|s| s.name == to) || !script.styles().any(|s| s.name == from) {
        return false;
    }
    for e in script.entries.iter_mut() {
        match e {
            Entry::Style(s) if s.name == from => s.name = to.to_string(),
            Entry::Line(l) if l.style == from => l.style = to.to_string(),
            _ => {}
        }
    }
    true
}

pub fn duplicate(script: &mut Script, name: &str) -> Option<String> {
    let pos = script.entries.iter().position(|e| matches!(e, Entry::Style(s) if s.name == name))?;
    let Entry::Style(mut s) = script.entries[pos].clone() else { return None };
    let mut new = format!("{name} copy");
    let mut k = 2;
    while script.styles().any(|x| x.name == new) {
        new = format!("{name} copy {k}");
        k += 1;
    }
    s.name = new.clone();
    script.entries.insert(pos + 1, Entry::Style(s));
    Some(new)
}

/// Delete a style; lines using it are re-pointed to `replacement` (if given).
pub fn delete(script: &mut Script, name: &str, replacement: Option<&str>) {
    script.entries.retain(|e| !matches!(e, Entry::Style(s) if s.name == name));
    if let Some(r) = replacement {
        for l in script.lines_mut() {
            if l.style == name {
                l.style = r.to_string();
            }
        }
    }
}

/// Copy styles from `src` (all, or those named in `only`). Existing names are replaced when `overwrite`.
pub fn import(script: &mut Script, src: &Script, only: Option<&[String]>, overwrite: bool) -> usize {
    let mut n = 0;
    for s in src.styles() {
        if only.map_or(false, |o| !o.contains(&s.name)) {
            continue;
        }
        match script.entries.iter().position(|e| matches!(e, Entry::Style(x) if x.name == s.name)) {
            Some(i) if overwrite => {
                script.entries[i] = Entry::Style(s.clone());
                n += 1;
            }
            Some(_) => {}
            None => {
                let pos = script.entries.iter().rposition(|e| matches!(e, Entry::Style(_))).map(|p| p + 1).unwrap_or_else(|| {
                    script.entries.iter().rposition(|e| matches!(e, Entry::Info { .. })).map(|p| p + 1).unwrap_or(0)
                });
                script.entries.insert(pos, Entry::Style(s.clone()));
                n += 1;
            }
        }
    }
    n
}

pub fn unused(script: &Script) -> Vec<String> {
    let used: HashSet<&str> = script.lines().map(|l| l.style.as_str()).collect();
    script.styles().filter(|s| !used.contains(s.name.as_str())).map(|s| s.name.clone()).collect()
}

pub fn missing(script: &Script) -> Vec<String> {
    let defined: HashSet<&str> = script.styles().map(|s| s.name.as_str()).collect();
    let mut m: Vec<String> = script.lines().map(|l| l.style.clone()).filter(|n| !defined.contains(n.as_str())).collect::<HashSet<_>>().into_iter().collect();
    m.sort();
    m
}

/// Add default-looking definitions for every style referenced by lines but not defined.
pub fn create_missing(script: &mut Script) -> usize {
    let miss = missing(script);
    let n = miss.len();
    for name in miss {
        script.entries.insert(
            script.entries.iter().rposition(|e| matches!(e, Entry::Style(_))).map(|p| p + 1).unwrap_or(0),
            Entry::Style(Style { name, ..Default::default() }),
        );
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;

    fn sc() -> Script {
        let mut s = Script::new_default();
        let mut l = Line::new(0, 1000, "x");
        l.style = "Default".into();
        s.push_line(l);
        let mut l2 = Line::new(0, 1000, "y");
        l2.style = "Ghost".into();
        s.push_line(l2);
        s
    }

    #[test]
    fn rename_updates_lines() {
        let mut s = sc();
        assert!(rename(&mut s, "Default", "Main"));
        assert_eq!(s.lines().next().unwrap().style, "Main");
        assert!(!rename(&mut s, "Main", "Main"));
        assert!(!rename(&mut s, "Nope", "X"));
    }

    #[test]
    fn dup_missing_unused() {
        let mut s = sc();
        assert_eq!(duplicate(&mut s, "Default").unwrap(), "Default copy");
        assert_eq!(unused(&s), vec!["Default copy"]);
        assert_eq!(missing(&s), vec!["Ghost"]);
        assert_eq!(create_missing(&mut s), 1);
        assert!(missing(&s).is_empty());
        delete(&mut s, "Ghost", Some("Default"));
        assert_eq!(s.lines().nth(1).unwrap().style, "Default");
    }

    #[test]
    fn import_styles() {
        let mut a = Script::new_default();
        let mut b = Script::new_default();
        b.entries.push(Entry::Style(Style { name: "Sign".into(), fontsize: 30.0, ..Default::default() }));
        assert_eq!(import(&mut a, &b, None, false), 1);
        assert_eq!(a.styles().count(), 2);
        assert_eq!(import(&mut a, &b, None, false), 0);
    }
}
