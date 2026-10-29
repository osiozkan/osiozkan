//! Betikte kullanılan yazı tiplerini (stil + \fn + \b/\i) çıkarır.
use crate::ass::{Entry, Script};
use crate::overrides::{self, Seg};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontUse {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// Bu yazı tipini kullanan satır sayısı.
    pub lines: usize,
}

fn norm(f: &str) -> String {
    f.trim().trim_start_matches('@').to_string()
}

fn bold_from(arg: &str, cur: bool) -> bool {
    match arg.trim() {
        "" => cur,
        "0" => false,
        "1" => true,
        n => n.parse::<i32>().map(|w| w >= 600).unwrap_or(cur),
    }
}

pub fn used_fonts(script: &Script) -> Vec<FontUse> {
    let styles: BTreeMap<&str, &crate::ass::Style> = script.styles().map(|s| (s.name.as_str(), s)).collect();
    let mut counts: BTreeMap<(String, bool, bool), usize> = BTreeMap::new();
    for e in &script.entries {
        let Entry::Dialogue(d) = e else { continue };
        if d.comment {
            continue;
        }
        let Some(st) = styles.get(d.style.as_str()).copied().or_else(|| styles.get("Default").copied()) else { continue };
        let base = (norm(&st.fontname), st.bold, st.italic);
        let (mut fam, mut b, mut i) = base.clone();
        let mut seen: Vec<(String, bool, bool)> = vec![];
        let has_text = |t: &str| !t.is_empty();
        let mut pending_visible = false;
        let mut push = |f: &(String, bool, bool), seen: &mut Vec<(String, bool, bool)>| {
            if !seen.contains(f) {
                seen.push(f.clone());
            }
        };
        for seg in overrides::parse(&d.text) {
            match seg {
                Seg::Block(tags) => {
                    for t in tags {
                        match t.name.as_str() {
                            "fn" => fam = norm(&t.arg),
                            "b" => b = bold_from(&t.arg, b),
                            "i" => i = t.arg.trim() == "1",
                            "r" => {
                                let name = t.arg.trim();
                                let s2 = styles.get(name).copied().unwrap_or(st);
                                (fam, b, i) = (norm(&s2.fontname), s2.bold, s2.italic);
                            }
                            _ => {}
                        }
                    }
                }
                Seg::Text(t) => {
                    if has_text(&t) && !t.trim().is_empty() {
                        pending_visible = true;
                        push(&(fam.clone(), b, i), &mut seen);
                    }
                }
            }
        }
        if !pending_visible {
            push(&base, &mut seen);
        }
        for f in seen {
            *counts.entry(f).or_default() += 1;
        }
    }
    counts.into_iter().map(|((family, bold, italic), lines)| FontUse { family, bold, italic, lines }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_fonts() {
        let s = Script::parse("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\nStyle: Alt,@Meiryo,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,a{\\fnImpact\\b1}b{\\r}c\nDialogue: 0,0:00:01.00,0:00:02.00,Alt,,0,0,0,,x\nComment: 0,0:00:01.00,0:00:02.00,Alt,,0,0,0,,{\\fnNope}y\n");
        let u = used_fonts(&s);
        let names: Vec<(&str, bool, usize)> = u.iter().map(|f| (f.family.as_str(), f.bold, f.lines)).collect();
        assert_eq!(names, vec![("Arial", false, 1), ("Impact", true, 1), ("Meiryo", true, 1)]);
    }
}
