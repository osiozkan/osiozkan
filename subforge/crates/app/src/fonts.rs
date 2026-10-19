//! Fonts used by a script and where they live on disk (fonts collector).
use fontdb::{Database, Family, Query, Stretch, Style as FStyle, Weight};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use subforge_core::ass::Script;
use subforge_core::tags::{self, Item, Segment};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontKey {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Debug, Clone)]
pub struct FontUse {
    pub key: FontKey,
    pub uses: usize,
    /// File found on this machine (None = missing).
    pub path: Option<PathBuf>,
}

/// Every (family, bold, italic) combination referenced by styles and `\fn`/`\b`/`\i` overrides.
pub fn used_fonts(script: &Script) -> Vec<FontKey> {
    let mut counts: BTreeMap<FontKey, usize> = BTreeMap::new();
    for st in &script.styles {
        *counts.entry(FontKey { family: st.fontname.trim_start_matches('@').to_string(), bold: st.bold, italic: st.italic }).or_default() += 0;
    }
    for e in &script.events {
        if !e.text.contains("\\fn") && !e.text.contains("\\b") && !e.text.contains("\\i") && !e.text.contains("\\r") {
            // only the style font
            if let Some(st) = script.style(&e.style) {
                *counts.entry(FontKey { family: st.fontname.trim_start_matches('@').to_string(), bold: st.bold, italic: st.italic }).or_default() += 1;
            }
            continue;
        }
        let base = script.style(&e.style);
        let (mut fam, mut bold, mut italic) = match base {
            Some(s) => (s.fontname.clone(), s.bold, s.italic),
            None => ("Arial".to_string(), false, false),
        };
        let (bfam, bbold, bital) = (fam.clone(), bold, italic);
        *counts.entry(FontKey { family: fam.trim_start_matches('@').to_string(), bold, italic }).or_default() += 1;
        for seg in tags::parse(&e.text) {
            if let Segment::Block(items) = seg {
                for it in items {
                    if let Item::Tag { name, value } = it {
                        let v = value.trim();
                        match name.as_str() {
                            "fn" => fam = v.to_string(),
                            "b" => bold = v != "0",
                            "i" => italic = v != "0",
                            "r" => {
                                fam = bfam.clone();
                                bold = bbold;
                                italic = bital;
                                if let Some(st) = script.style(v) {
                                    fam = st.fontname.clone();
                                    bold = st.bold;
                                    italic = st.italic;
                                }
                            }
                            _ => continue,
                        }
                        *counts.entry(FontKey { family: fam.trim_start_matches('@').to_string(), bold, italic }).or_default() += 0;
                    }
                }
            }
        }
    }
    counts.into_keys().collect()
}

/// Resolve each used font against installed fonts plus `extra_dirs`.
pub fn resolve(script: &Script, extra_dirs: &[PathBuf]) -> Vec<FontUse> {
    let mut db = Database::new();
    db.load_system_fonts();
    for d in extra_dirs {
        db.load_fonts_dir(d);
    }
    used_fonts(script)
        .into_iter()
        .map(|key| {
            let fams = [Family::Name(&key.family)];
            let id = db.query(&Query { families: &fams, weight: if key.bold { Weight::BOLD } else { Weight::NORMAL }, stretch: Stretch::Normal, style: if key.italic { FStyle::Italic } else { FStyle::Normal } });
            // fontdb falls back loosely; require the family name to match
            let path = id.and_then(|id| db.face(id)).filter(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(&key.family)) || f.post_script_name.eq_ignore_ascii_case(&key.family)).and_then(|f| match &f.source {
                fontdb::Source::File(p) => Some(p.clone()),
                fontdb::Source::SharedFile(p, _) => Some(p.clone()),
                _ => None,
            });
            FontUse { key, uses: 0, path }
        })
        .collect()
}

/// Copy the found font files into `dest`; returns (copied, missing family names).
pub fn collect_to(fonts: &[FontUse], dest: &Path) -> std::io::Result<(usize, Vec<String>)> {
    std::fs::create_dir_all(dest)?;
    let mut copied = 0;
    let mut seen = std::collections::HashSet::new();
    let mut missing = vec![];
    for f in fonts {
        match &f.path {
            Some(p) if seen.insert(p.clone()) => {
                if let Some(name) = p.file_name() {
                    std::fs::copy(p, dest.join(name))?;
                    copied += 1;
                }
            }
            Some(_) => {}
            None => missing.push(f.key.family.clone()),
        }
    }
    missing.sort();
    missing.dedup();
    Ok((copied, missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use subforge_core::ass::{Event, Style};

    #[test]
    fn finds_fonts_in_styles_and_tags() {
        let mut s = Script::new();
        s.styles.push(Style { name: "Sign".into(), fontname: "Impact".into(), ..Default::default() });
        s.events.push(Event::new(0, 1, "Default", "{\\fnComic Sans MS\\b1}x{\\r}y"));
        let u = used_fonts(&s);
        let names: Vec<&str> = u.iter().map(|k| k.family.as_str()).collect();
        assert!(names.contains(&"Arial") && names.contains(&"Impact") && names.contains(&"Comic Sans MS"));
        assert!(u.iter().any(|k| k.family == "Comic Sans MS" && k.bold));
    }
}
