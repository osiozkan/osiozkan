//! Override-tag helpers: block splitting, stripping, karaoke syllables.

#[derive(Debug, Clone, PartialEq)]
pub enum Seg<'a> {
    Tags(&'a str),
    Text(&'a str),
}

/// Split dialogue text into `{override}` blocks and plain text runs.
pub fn split_blocks(text: &str) -> Vec<Seg<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if let Some(close_rel) = rest[open..].find('}') {
                if open > 0 {
                    out.push(Seg::Text(&rest[..open]));
                }
                out.push(Seg::Tags(&rest[open + 1..open + close_rel]));
                rest = &rest[open + close_rel + 1..];
                continue;
            }
        }
        out.push(Seg::Text(rest));
        break;
    }
    out
}

/// Remove all override blocks (keeps `\N` etc.).
pub fn strip_tags(text: &str) -> String {
    split_blocks(text)
        .into_iter()
        .filter_map(|s| if let Seg::Text(t) = s { Some(t) } else { None })
        .collect()
}

/// Visible text for reading-speed / length: tags removed, `\N`/`\n` -> newline marker, `\h` -> space.
pub fn visible_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

pub fn visible_len(text: &str) -> usize {
    visible_text(text).chars().filter(|c| *c != '\n').count()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syllable {
    /// duration in centiseconds
    pub dur_cs: i64,
    /// "k", "kf", "ko" or "K"
    pub kind: String,
    pub text: String,
    /// offset from line start in ms
    pub start_ms: i64,
    pub tags: String,
    /// text including non-karaoke override blocks (karaoke tag removed)
    pub raw: String,
}

/// Parse karaoke syllables. Text before the first \k is syllable 0 with dur 0 (like karaskel).
pub fn syllables(text: &str) -> Vec<Syllable> {
    let re = regex::Regex::new(r"\\(kf|ko|k|K)(-?\d+(?:\.\d+)?)").unwrap();
    let mut out: Vec<Syllable> = Vec::new();
    let mut cur = Syllable { dur_cs: 0, kind: "k".into(), text: String::new(), start_ms: 0, tags: String::new(), raw: String::new() };
    let mut elapsed_ms = 0i64;
    let mut started = false;
    for seg in split_blocks(text) {
        match seg {
            Seg::Tags(t) => {
                if let Some(m) = re.captures(t) {
                    if started || !cur.text.is_empty() {
                        elapsed_ms += cur.dur_cs * 10;
                        out.push(cur.clone());
                    }
                    let kind = &m[1];
                    let kind = if kind == "K" { "kf" } else { kind };
                    cur = Syllable {
                        dur_cs: m[2].parse::<f64>().unwrap_or(0.0).round() as i64,
                        kind: kind.into(),
                        text: String::new(),
                        start_ms: elapsed_ms,
                        tags: t.to_string(),
                        raw: String::new(),
                    };
                    started = true;
                } else {
                    cur.tags.push_str(t);
                }
            }
            Seg::Text(t) => cur.text.push_str(t),
        }
    }
    if started || !cur.text.is_empty() {
        out.push(cur);
    }
    out
}

/// Aegisub `parse_karaoke_data` semantics: index 0 is always the text before the first `\k` tag
/// (possibly empty, duration 0); then one entry per karaoke tag. `raw` keeps non-karaoke override
/// blocks, with the karaoke tag itself removed.
pub fn karaoke_data(text: &str) -> Vec<Syllable> {
    let re = regex::Regex::new(r"\\(kf|ko|k|K)(-?\d+(?:\.\d+)?)").unwrap();
    let mk = |dur: i64, kind: &str, start: i64| Syllable { dur_cs: dur, kind: kind.into(), text: String::new(), start_ms: start, tags: String::new(), raw: String::new() };
    let mut out = vec![mk(0, "k", 0)];
    let mut elapsed = 0i64;
    for seg in split_blocks(text) {
        match seg {
            Seg::Tags(t) => {
                if let Some(m) = re.captures(t) {
                    let last = out.last().unwrap();
                    elapsed += last.dur_cs * 10;
                    let kind = if &m[1] == "K" { "kf" } else { &m[1] };
                    let mut syl = mk(m[2].parse::<f64>().unwrap_or(0.0).round() as i64, kind, elapsed);
                    syl.tags = t.to_string();
                    let rest = re.replace_all(t, "").to_string();
                    if !rest.is_empty() {
                        syl.raw.push_str(&format!("{{{}}}", rest));
                    }
                    out.push(syl);
                } else {
                    let cur = out.last_mut().unwrap();
                    cur.tags.push_str(t);
                    cur.raw.push_str(&format!("{{{}}}", t));
                }
            }
            Seg::Text(t) => {
                let cur = out.last_mut().unwrap();
                cur.text.push_str(t);
                cur.raw.push_str(t);
            }
        }
    }
    out
}

/// Total karaoke duration in ms.
pub fn karaoke_total_ms(text: &str) -> i64 {
    syllables(text).iter().map(|s| s.dur_cs * 10).sum()
}

/// Replace a tag everywhere, e.g. remove all `\blur` values. Returns text with tag removed.
pub fn remove_tag(text: &str, name: &str) -> String {
    let re = regex::Regex::new(&format!(r"\\{}(?:\([^)]*\)|[^\\}}]*)", regex::escape(name))).unwrap();
    split_blocks(text)
        .into_iter()
        .map(|s| match s {
            Seg::Tags(t) => {
                let r = re.replace_all(t, "").to_string();
                if r.is_empty() { String::new() } else { format!("{{{}}}", r) }
            }
            Seg::Text(t) => t.to_string(),
        })
        .collect()
}

/// Merge adjacent override blocks `{\a}{\b}` -> `{\a\b}` and drop empty ones.
pub fn merge_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut pending = String::new();
    for s in split_blocks(text) {
        match s {
            Seg::Tags(t) => pending.push_str(t),
            Seg::Text(t) => {
                if !pending.is_empty() {
                    out.push_str(&format!("{{{}}}", pending));
                    pending.clear();
                }
                out.push_str(t);
            }
        }
    }
    if !pending.is_empty() {
        out.push_str(&format!("{{{}}}", pending));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strip() {
        assert_eq!(strip_tags("{\\i1}Hi{\\i0} there\\Nyou"), "Hi there\\Nyou");
        assert_eq!(visible_len("{\\b1}ab{\\b0}\\Ncd"), 4);
    }
    #[test]
    fn kara() {
        let s = syllables("{\\k20}Ka{\\kf30}ra{\\k10}");
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].text, "Ka");
        assert_eq!(s[1].start_ms, 200);
        assert_eq!(s[1].kind, "kf");
        assert_eq!(s[2].start_ms, 500);
        assert_eq!(karaoke_total_ms("{\\k20}Ka{\\kf30}ra{\\k10}"), 600);
        let lead = syllables("Intro{\\k10}x");
        assert_eq!(lead.len(), 2);
        assert_eq!(lead[0].dur_cs, 0);
    }
    #[test]
    fn kdata() {
        let k = karaoke_data("{\\k20\\fad(1,2)}Ka{\\kf30}ra");
        assert_eq!(k.len(), 3);
        assert_eq!(k[0].text, "");
        assert_eq!(k[1].raw, "{\\fad(1,2)}Ka");
        assert_eq!(k[2].start_ms, 200);
        let k = karaoke_data("lead{\\k5}x");
        assert_eq!(k[0].text, "lead");
        assert_eq!(k[1].dur_cs, 5);
    }
    #[test]
    fn rm_merge() {
        assert_eq!(remove_tag("{\\blur3\\i1}a", "blur"), "{\\i1}a");
        assert_eq!(merge_blocks("{\\a}{\\b}x"), "{\\a\\b}x");
    }
}
