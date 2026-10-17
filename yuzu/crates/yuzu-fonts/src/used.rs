//! Find the fonts a script actually renders with.

use asscore::tags::{split_block_tags, split_parts, Part};
use asscore::Script;
use std::collections::{BTreeMap, BTreeSet};

/// One distinct (family, bold, italic) combination used by a script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontUse {
    /// Family as written in the script (first spelling seen), without a leading `@`.
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// Every character rendered with this font (drawings excluded).
    pub chars: BTreeSet<char>,
    /// Number of dialogue lines that render at least one character with it.
    pub lines: usize,
}

#[derive(Clone)]
struct State {
    family: String,
    bold: bool,
    italic: bool,
    drawing: bool,
}

fn norm_family(s: &str) -> String {
    s.trim().trim_start_matches('@').trim().to_string()
}

fn style_state(script: &Script, name: &str) -> State {
    let st = script.style_or_default(name);
    State { family: norm_family(&st.fontname), bold: st.bold, italic: st.italic, drawing: false }
}

fn digits(s: &str) -> Option<i64> {
    let s = s.trim();
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        s.parse().ok()
    } else {
        None
    }
}

fn apply_tag(st: &mut State, tag: &str, script: &Script, base: &State) {
    let Some(t) = tag.strip_prefix('\\') else { return };
    if let Some(name) = t.strip_prefix("fn") {
        let n = norm_family(name);
        st.family = if n.is_empty() { base.family.clone() } else { n };
    } else if let Some(name) = t.strip_prefix('r') {
        let name = name.trim();
        let drawing = st.drawing;
        *st = if name.is_empty() || script.style(name).is_none() { base.clone() } else { style_state(script, name) };
        st.drawing = drawing;
    } else if let Some(arg) = t.strip_prefix('b') {
        if arg.trim().is_empty() {
            st.bold = base.bold;
        } else if let Some(n) = digits(arg) {
            st.bold = n == 1 || n >= 600;
        }
    } else if let Some(arg) = t.strip_prefix('i') {
        if arg.trim().is_empty() {
            st.italic = base.italic;
        } else if let Some(n) = digits(arg) {
            st.italic = n != 0;
        }
    } else if let Some(arg) = t.strip_prefix('p') {
        // `\pbo` and friends do not parse as a number and are ignored.
        if let Some(n) = digits(arg) {
            st.drawing = n > 0;
        }
    }
}

/// Visible characters of a text run: `\N`/`\n` are dropped, `\h` becomes U+00A0.
fn visible(text: &str) -> impl Iterator<Item = char> + '_ {
    let mut it = text.chars().peekable();
    std::iter::from_fn(move || loop {
        let c = it.next()?;
        if c == '\\' {
            match it.peek() {
                Some('N') | Some('n') => {
                    it.next();
                    continue;
                }
                Some('h') => {
                    it.next();
                    return Some('\u{a0}');
                }
                _ => {}
            }
        }
        if c == '\r' || c == '\n' {
            continue;
        }
        return Some(c);
    })
}

/// All fonts rendered by non-comment events, sorted by family (case-insensitive), then regular
/// before bold and upright before italic.
///
/// Styles never referenced by a rendered character do not appear. Text inside `\p` drawings does
/// not count. A leading `@` (vertical writing) is stripped from the family.
pub fn used_fonts(script: &Script) -> Vec<FontUse> {
    let mut map: BTreeMap<(String, bool, bool), FontUse> = BTreeMap::new();
    for ev in script.events.iter().filter(|e| !e.comment) {
        let base = style_state(script, &ev.style);
        let mut st = base.clone();
        let mut touched: BTreeSet<(String, bool, bool)> = BTreeSet::new();
        for part in split_parts(&ev.text) {
            match part {
                Part::Block(b) => {
                    for tag in split_block_tags(&b) {
                        apply_tag(&mut st, &tag, script, &base);
                    }
                }
                Part::Text(t) => {
                    if st.drawing {
                        continue;
                    }
                    let key = (st.family.to_lowercase(), st.bold, st.italic);
                    for c in visible(&t) {
                        let e = map.entry(key.clone()).or_insert_with(|| FontUse {
                            family: st.family.clone(),
                            bold: st.bold,
                            italic: st.italic,
                            chars: BTreeSet::new(),
                            lines: 0,
                        });
                        e.chars.insert(c);
                        touched.insert(key.clone());
                    }
                }
            }
        }
        for k in touched {
            if let Some(e) = map.get_mut(&k) {
                e.lines += 1;
            }
        }
    }
    map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use asscore::{Event, Style};

    fn script(styles: Vec<Style>, events: Vec<(&str, &str)>) -> Script {
        let mut s = Script::new();
        s.styles = styles;
        s.events = events.into_iter().map(|(st, t)| Event::new(0, 1000, st, t)).collect();
        s
    }
    fn style(name: &str, font: &str, bold: bool, italic: bool) -> Style {
        Style { name: name.into(), fontname: font.into(), bold, italic, ..Style::default() }
    }
    fn find<'a>(v: &'a [FontUse], fam: &str, b: bool, i: bool) -> Option<&'a FontUse> {
        v.iter().find(|f| f.family == fam && f.bold == b && f.italic == i)
    }

    #[test]
    fn style_fonts_only_when_used() {
        let s = script(vec![style("Default", "Arial", true, false), style("Unused", "Comic", false, false)], vec![("Default", "Hi")]);
        let u = used_fonts(&s);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].family, "Arial");
        assert!(u[0].bold && !u[0].italic);
        assert_eq!(u[0].chars.iter().collect::<String>(), "Hi");
        assert_eq!(u[0].lines, 1);
    }

    #[test]
    fn fn_override_and_revert() {
        let s = script(vec![style("Default", "Arial", false, false)], vec![("Default", "a{\\fnTimes New Roman}b{\\fn}c")]);
        let u = used_fonts(&s);
        assert_eq!(u.len(), 2);
        assert_eq!(find(&u, "Arial", false, false).unwrap().chars.iter().collect::<String>(), "ac");
        assert_eq!(find(&u, "Times New Roman", false, false).unwrap().chars.iter().collect::<String>(), "b");
    }

    #[test]
    fn fn_starting_with_c_is_not_fnc() {
        let s = script(vec![style("Default", "Arial", false, false)], vec![("Default", "{\\fnComic Sans MS}x")]);
        assert_eq!(used_fonts(&s)[0].family, "Comic Sans MS");
    }

    #[test]
    fn bold_italic_tags() {
        let s = script(
            vec![style("Default", "Arial", false, false)],
            vec![("Default", "{\\b1}a{\\b0\\i1}b{\\b700}c{\\i0}d{\\b}e{\\blur2\\bord3}f")],
        );
        let u = used_fonts(&s);
        let chars = |b, i| find(&u, "Arial", b, i).map(|f| f.chars.iter().collect::<String>());
        assert_eq!(chars(true, false).as_deref(), Some("ad"));
        assert_eq!(chars(true, true).as_deref(), Some("c"));
        assert_eq!(chars(false, true).as_deref(), Some("b"));
        assert_eq!(chars(false, false).as_deref(), Some("ef"));
    }

    #[test]
    fn reset_to_style() {
        let s = script(
            vec![style("Default", "Arial", false, false), style("Alt", "Georgia", false, true)],
            vec![("Default", "{\\b1\\fnX}a{\\r}b{\\rAlt}c{\\rNope}d")],
        );
        let u = used_fonts(&s);
        assert_eq!(find(&u, "X", true, false).unwrap().chars.len(), 1);
        assert_eq!(find(&u, "Arial", false, false).unwrap().chars.iter().collect::<String>(), "bd");
        assert_eq!(find(&u, "Georgia", false, true).unwrap().chars.iter().collect::<String>(), "c");
    }

    #[test]
    fn vertical_and_drawings_and_comments() {
        let s = script(vec![style("Default", "@MS Gothic", false, false)], vec![("Default", "{\\p1}m 0 0 l 10 10{\\p0}ab\\Ncd\\h")]);
        let mut s = s;
        s.events.push(Event { comment: true, ..Event::new(0, 1, "Default", "zzz{\\fnGhost}") });
        let u = used_fonts(&s);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].family, "MS Gothic");
        assert_eq!(u[0].chars.iter().collect::<String>(), "abcd\u{a0}");
    }

    #[test]
    fn drawing_only_line_uses_no_font_and_lines_counted() {
        let mut s = script(vec![style("Default", "Arial", false, false)], vec![("Default", "{\\p1}m 0 0 l 5 5"), ("Default", "x"), ("Default", "y{\\i1}z")]);
        s.events.push(Event::new(0, 1, "Default", "w"));
        let u = used_fonts(&s);
        assert_eq!(find(&u, "Arial", false, false).unwrap().lines, 3);
        assert_eq!(find(&u, "Arial", false, true).unwrap().lines, 1);
    }

    #[test]
    fn case_insensitive_merge_and_missing_style_falls_back() {
        let s = script(vec![style("Default", "Arial", false, false)], vec![("Nope", "a{\\fnARIAL}b")]);
        let u = used_fonts(&s);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].chars.len(), 2);
    }
}
