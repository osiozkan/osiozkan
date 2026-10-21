//! Subtitle Edit-style text tools: fix common errors, hearing-impaired removal, casing (with
//! Turkish dotted/dotless i rules), line rebalancing, merge/split and statistics.

use crate::ass::{Document, Entry, Event};
use crate::tags::{split_blocks, visible_len, Seg};
use regex::Regex;

/// Apply `f` to the plain-text parts only (override blocks untouched).
pub fn map_text(text: &str, f: &mut dyn FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    for s in split_blocks(text) {
        match s {
            Seg::Tags(t) => {
                out.push('{');
                out.push_str(t);
                out.push('}');
            }
            Seg::Text(t) => out.push_str(&f(t)),
        }
    }
    out
}

pub fn upper_tr(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'i' => vec!['İ'], 'ı' => vec!['I'], c => c.to_uppercase().collect() }).collect()
}
pub fn lower_tr(s: &str) -> String {
    s.chars().flat_map(|c| match c { 'I' => vec!['ı'], 'İ' => vec!['i'], c => c.to_lowercase().collect() }).collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Casing {
    Upper,
    Lower,
    Title,
    Sentence,
}

pub fn change_case(s: &str, mode: Casing, turkish: bool) -> String {
    let up = |x: &str| if turkish { upper_tr(x) } else { x.to_uppercase() };
    let lo = |x: &str| if turkish { lower_tr(x) } else { x.to_lowercase() };
    match mode {
        Casing::Upper => up(s),
        Casing::Lower => lo(s),
        Casing::Title => {
            let mut out = String::new();
            let mut start = true;
            for c in s.chars() {
                if c.is_alphabetic() {
                    let t = c.to_string();
                    out.push_str(&if start { up(&t) } else { lo(&t) });
                    start = false;
                } else {
                    start = c.is_whitespace() || c == '-' || c == '(' || c == '"' || c == '\\';
                    out.push(c);
                }
            }
            out
        }
        Casing::Sentence => {
            let mut out = String::new();
            let mut start = true;
            for c in s.chars() {
                if c.is_alphabetic() {
                    let t = c.to_string();
                    out.push_str(&if start { up(&t) } else { lo(&t) });
                    start = false;
                } else {
                    if matches!(c, '.' | '!' | '?' | '…') {
                        start = true;
                    }
                    out.push(c);
                }
            }
            out
        }
    }
}

pub fn change_case_doc(doc: &mut Document, mode: Casing, turkish: bool, only: Option<&[usize]>) {
    for (i, e) in doc.events_mut().enumerate() {
        if only.map(|o| o.contains(&i)).unwrap_or(true) {
            e.text = map_text(&e.text, &mut |t| {
                // keep \N / \h escapes intact
                let parts: Vec<&str> = t.split("\\N").collect();
                parts.iter().map(|p| change_case(p, mode, turkish)).collect::<Vec<_>>().join("\\N")
            });
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FixOptions {
    pub double_spaces: bool,
    pub space_before_punct: bool,
    pub space_after_punct: bool,
    pub ellipsis: bool,
    pub trim_around_breaks: bool,
    pub dash_spacing: bool,
}

impl FixOptions {
    pub fn all() -> FixOptions {
        FixOptions { double_spaces: true, space_before_punct: true, space_after_punct: true, ellipsis: true, trim_around_breaks: true, dash_spacing: true }
    }
}

/// Fix common typography errors; returns number of events changed.
pub fn fix_common(doc: &mut Document, o: &FixOptions) -> usize {
    let dbl = Regex::new(r" {2,}").unwrap();
    let sp_before = Regex::new(r" +([,.!?;:…])").unwrap();
    let sp_after = Regex::new(r#"([,!?;:…])([^\s\\\d,.!?;:…)"'»”’\]}])"#).unwrap();
    let dots = Regex::new(r"\.{3,}").unwrap();
    let brk = Regex::new(r" *\\N *").unwrap();
    let dash = Regex::new(r"(^|\\N)-(\S)").unwrap();
    let mut changed = 0;
    for e in doc.events_mut() {
        let orig = e.text.clone();
        let mut t = map_text(&e.text, &mut |s| {
            let mut s = s.to_string();
            if o.ellipsis {
                s = dots.replace_all(&s, "…").to_string();
            }
            if o.double_spaces {
                s = dbl.replace_all(&s, " ").to_string();
            }
            if o.space_before_punct {
                s = sp_before.replace_all(&s, "$1").to_string();
            }
            if o.space_after_punct {
                s = sp_after.replace_all(&s, "$1 $2").to_string();
            }
            s
        });
        if o.trim_around_breaks {
            t = brk.replace_all(&t, "\\N").to_string();
            t = t.trim().to_string();
        }
        if o.dash_spacing {
            t = dash.replace_all(&t, "$1- $2").to_string();
        }
        if t != orig {
            e.text = t;
            changed += 1;
        }
    }
    changed
}

/// Remove sound descriptions like [door slams], (laughs), ♪ lyrics ♪ and `NAME:` speaker labels.
/// Events that become empty are deleted. Returns (changed, deleted).
pub fn remove_hearing_impaired(doc: &mut Document, speaker_labels: bool, music: bool) -> (usize, usize) {
    let brackets = Regex::new(r"\[[^\]]*\]|\([^)]*\)|（[^）]*）|［[^］]*］").unwrap();
    let speaker = Regex::new(r"(?m)^\s*[A-ZÇĞİÖŞÜ][A-ZÇĞİÖŞÜ0-9 .'’-]{1,24}:\s*").unwrap();
    let note = Regex::new(r"[♪♫].*?[♪♫]|[♪♫].*$").unwrap();
    let (mut changed, mut deleted) = (0, 0);
    let mut kill = Vec::new();
    for (idx, e) in doc.entries.iter_mut().enumerate() {
        let Entry::Event(e) = e else { continue };
        if e.comment {
            continue;
        }
        let orig = e.text.clone();
        let mut t = map_text(&e.text, &mut |s| {
            let mut s = brackets.replace_all(s, "").to_string();
            if music {
                s = note.replace_all(&s, "").to_string();
            }
            if speaker_labels {
                s = s.split("\\N").map(|p| speaker.replace(p, "").to_string()).collect::<Vec<_>>().join("\\N");
            }
            s
        });
        // clean break leftovers
        let lead = Regex::new(r"^(\s|\\N)+|(\s|\\N)+$").unwrap();
        t = lead.replace_all(&t, "").to_string();
        t = t.replace("\\N\\N", "\\N");
        if t != orig {
            if crate::tags::strip_tags(&t).trim().is_empty() {
                kill.push(idx);
                deleted += 1;
            } else {
                e.text = t;
                changed += 1;
            }
        }
    }
    for i in kill.into_iter().rev() {
        doc.entries.remove(i);
    }
    (changed, deleted)
}

/// Re-break a text into at most two balanced lines when it is longer than `max_len` (visible chars).
/// Existing `\N` are removed first. Text with override tags is left untouched.
pub fn rebalance(text: &str, max_len: usize) -> String {
    // keep a leading run of override blocks ({\an8}{\pos(..)}) and balance the rest
    let mut split = 0;
    let mut rest = text;
    while rest.starts_with('{') {
        match rest.find('}') {
            Some(i) => {
                split += i + 1;
                rest = &rest[i + 1..];
            }
            None => break,
        }
    }
    if split > 0 {
        return format!("{}{}", &text[..split], rebalance(&text[split..], max_len));
    }
    if text.contains('{') || text.contains("\\p1") {
        return text.to_string();
    }
    let flat = text.replace("\\N", " ").replace("\\n", " ");
    let flat = Regex::new(r" {2,}").unwrap().replace_all(flat.trim(), " ").to_string();
    let chars: Vec<char> = flat.chars().collect();
    if chars.len() <= max_len {
        return flat;
    }
    let mid = chars.len() / 2;
    // best space nearest the middle, preferring a position after punctuation
    let mut best: Option<(usize, i64)> = None;
    for (i, c) in chars.iter().enumerate() {
        if *c == ' ' && i > 2 && i + 3 < chars.len() {
            let mut score = (i as i64 - mid as i64).abs() * 2;
            if matches!(chars[i - 1], ',' | '.' | '!' | '?' | ';' | ':' | '…') {
                score -= 6;
            }
            if best.map(|b| score < b.1).unwrap_or(true) {
                best = Some((i, score));
            }
        }
    }
    match best {
        Some((i, _)) => format!("{}\\N{}", chars[..i].iter().collect::<String>(), chars[i + 1..].iter().collect::<String>()),
        None => flat,
    }
}

pub fn rebalance_doc(doc: &mut Document, max_len: usize) -> usize {
    let mut n = 0;
    for e in doc.events_mut() {
        if e.comment {
            continue;
        }
        let r = rebalance(&e.text, max_len);
        if r != e.text {
            e.text = r;
            n += 1;
        }
    }
    n
}

/// Merge consecutive same-style/same-layer events when the gap is small and the result is short.
pub fn merge_short(doc: &mut Document, max_gap_ms: i64, max_chars: usize) -> usize {
    let mut merged = 0;
    let mut i = 0;
    let idxs: Vec<usize> = doc.entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Event(ev) if !ev.comment)).map(|(i, _)| i).collect();
    let mut drop: Vec<usize> = Vec::new();
    let mut cur: Option<usize> = None;
    while i < idxs.len() {
        let pos = idxs[i];
        if let Some(c) = cur {
            let (Entry::Event(a), Entry::Event(b)) = (&doc.entries[c], &doc.entries[pos]) else { unreachable!() };
            let combined = visible_len(&a.text) + visible_len(&b.text) + 1;
            if a.style == b.style && a.layer == b.layer && b.start - a.end <= max_gap_ms && b.start >= a.end && combined <= max_chars && !a.text.contains('{') && !b.text.contains('{') {
                let (bt, be) = (b.text.clone(), b.end);
                if let Entry::Event(a) = &mut doc.entries[c] {
                    a.text = format!("{} {}", a.text, bt);
                    a.end = be;
                }
                drop.push(pos);
                merged += 1;
                i += 1;
                continue;
            }
        }
        cur = Some(pos);
        i += 1;
    }
    for p in drop.into_iter().rev() {
        doc.entries.remove(p);
    }
    merged
}

/// Split events longer than `max_chars` at the best sentence/space boundary; time split proportional.
pub fn split_long(doc: &mut Document, max_chars: usize) -> usize {
    let mut out: Vec<Entry> = Vec::new();
    let mut n = 0;
    for e in doc.entries.drain(..) {
        match e {
            Entry::Event(ev) if !ev.comment && !ev.text.contains('{') && visible_len(&ev.text) > max_chars => {
                let flat = ev.text.replace("\\N", " ");
                let chars: Vec<char> = flat.chars().collect();
                let mid = chars.len() / 2;
                let split = (1..chars.len() - 1)
                    .filter(|&i| chars[i] == ' ')
                    .min_by_key(|&i| ((i as i64 - mid as i64).abs() as i64) - if matches!(chars[i - 1], '.' | '!' | '?' | ',' | ';') { 8 } else { 0 });
                if let Some(i) = split {
                    let a: String = chars[..i].iter().collect();
                    let b: String = chars[i + 1..].iter().collect();
                    let t = ev.start + ((ev.end - ev.start) as f64 * (a.chars().count() as f64 / (a.chars().count() + b.chars().count()) as f64)).round() as i64;
                    out.push(Entry::Event(Event { text: a, end: t, ..ev.clone() }));
                    out.push(Entry::Event(Event { text: b, start: t, ..ev }));
                    n += 1;
                    continue;
                }
                out.push(Entry::Event(ev));
            }
            other => out.push(other),
        }
    }
    doc.entries = out;
    n
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub lines: usize,
    pub comments: usize,
    pub chars: usize,
    pub words: usize,
    pub total_ms: i64,
    pub avg_cps: f64,
    pub max_cps: f64,
    pub avg_line_len: f64,
}

pub fn stats(doc: &Document) -> Stats {
    let mut s = Stats::default();
    let mut cps_sum = 0.0;
    let mut cps_n = 0.0;
    for e in doc.events() {
        if e.comment {
            s.comments += 1;
            continue;
        }
        s.lines += 1;
        let vt = crate::tags::visible_text(&e.text);
        let len = vt.chars().filter(|c| *c != '\n').count();
        s.chars += len;
        s.words += vt.split_whitespace().count();
        let d = e.end - e.start;
        s.total_ms += d.max(0);
        if d > 0 && len > 0 {
            let cps = len as f64 / (d as f64 / 1000.0);
            cps_sum += cps;
            cps_n += 1.0;
            s.max_cps = s.max_cps.max(cps);
        }
    }
    if cps_n > 0.0 {
        s.avg_cps = cps_sum / cps_n;
    }
    if s.lines > 0 {
        s.avg_line_len = s.chars as f64 / s.lines as f64;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(lines: &[(&str, i64, i64)]) -> Document {
        let mut d = Document::new_default();
        for (t, s, e) in lines {
            d.entries.push(Entry::Event(Event { text: t.to_string(), start: *s, end: *e, ..Event::default() }));
        }
        d
    }
    fn texts(d: &Document) -> Vec<String> {
        d.events().map(|e| e.text.clone()).collect()
    }

    #[test]
    fn turkish_casing() {
        assert_eq!(upper_tr("işık ılık"), "İŞIK ILIK");
        assert_eq!(lower_tr("İSTANBUL ISPARTA"), "istanbul ısparta");
        assert_eq!(change_case("merhaba dünya", Casing::Title, true), "Merhaba Dünya");
        assert_eq!(change_case("iyi misin? ışık var.", Casing::Sentence, true), "İyi misin? Işık var.");
    }

    #[test]
    fn fixes_typography() {
        let mut d = doc(&[("Hello  world ,how are you ...", 0, 1000), ("{\\i1}ok{\\i0}  yes\\N -Hi", 0, 1000)]);
        assert_eq!(fix_common(&mut d, &FixOptions::all()), 2);
        assert_eq!(texts(&d), ["Hello world, how are you…", "{\\i1}ok{\\i0} yes\\N- Hi"]);
    }

    #[test]
    fn hearing_impaired() {
        let mut d = doc(&[("[door slams]", 0, 1000), ("JOHN: Hello (laughs)", 0, 1000), ("♪ la la ♪", 0, 1000), ("Plain", 0, 1000)]);
        let (c, k) = remove_hearing_impaired(&mut d, true, true);
        assert_eq!((c, k), (1, 2));
        assert_eq!(texts(&d), ["Hello", "Plain"]);
    }

    #[test]
    fn rebalance_and_split_merge() {
        let r = rebalance("This is a fairly long subtitle line that needs breaking nicely", 40);
        assert!(r.contains("\\N"));
        let parts: Vec<&str> = r.split("\\N").collect();
        assert!((parts[0].len() as i64 - parts[1].len() as i64).abs() < 14, "{}", r);
        let mut d = doc(&[("Hello", 0, 1000), ("world", 1100, 2000), ("far away", 5000, 6000)]);
        assert_eq!(merge_short(&mut d, 200, 40), 1);
        assert_eq!(texts(&d), ["Hello world", "far away"]);
        assert_eq!(d.events().next().unwrap().end, 2000);
        let mut d = doc(&[("Short sentence here. Another sentence follows right after", 0, 4000)]);
        assert_eq!(split_long(&mut d, 30), 1);
        assert_eq!(d.events().count(), 2);
        let e: Vec<_> = d.events().collect();
        assert_eq!(e[0].end, e[1].start);
        assert!(e[0].text.ends_with("here."));
    }

    #[test]
    fn rebalance_keeps_leading_tags() {
        let r = rebalance("{\\an8}This is a fairly long subtitle line that needs breaking nicely", 40);
        assert!(r.starts_with("{\\an8}") && r.contains("\\N"), "{}", r);
        assert_eq!(rebalance("short", 40), "short");
    }

    #[test]
    fn stats_work() {
        let d = doc(&[("Hello world", 0, 1000), ("ab", 1000, 2000)]);
        let s = stats(&d);
        assert_eq!((s.lines, s.words, s.chars), (2, 3, 13));
        assert!((s.max_cps - 11.0).abs() < 1e-9);
    }
}
