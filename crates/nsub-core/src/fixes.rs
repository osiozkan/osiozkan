//! Subtitle Edit–style bulk fixes: casing, hearing-impaired removal, line balancing,
//! splitting/merging by length, gap/CPS repair and frame snapping.

use crate::ass::{Event, Script};
use crate::tags::{self, Segment};
use crate::text;
use crate::time::FrameRate;
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Sentence,
    Title,
    Invert,
}

fn map_text(s: &str, f: impl Fn(&str) -> String) -> String {
    let mut segs = tags::parse(s);
    for seg in &mut segs {
        if let Segment::Text(t) = seg {
            // keep \N \n \h escapes intact
            let parts: Vec<String> = regex_split_keep(t).into_iter().map(|(is_esc, p)| if is_esc { p } else { f(&p) }).collect();
            *t = parts.concat();
        }
    }
    tags::serialize(&segs)
}

fn regex_split_keep(t: &str) -> Vec<(bool, String)> {
    let re = Regex::new(r"\\[Nnh]").unwrap();
    let mut out = vec![];
    let mut last = 0;
    for m in re.find_iter(t) {
        if m.start() > last {
            out.push((false, t[last..m.start()].to_string()));
        }
        out.push((true, m.as_str().to_string()));
        last = m.end();
    }
    if last < t.len() {
        out.push((false, t[last..].to_string()));
    }
    out
}

/// Change case of the visible text only (tags untouched), Turkish aware (i/İ, ı/I).
pub fn change_case(script: &mut Script, mode: Case, only: Option<&[usize]>) -> usize {
    let mut n = 0;
    for (i, e) in script.events.iter_mut().enumerate() {
        if only.map_or(false, |o| !o.contains(&i)) {
            continue;
        }
        let new = match mode {
            Case::Upper => map_text(&e.text, text::upper_tr),
            Case::Lower => map_text(&e.text, text::lower_tr),
            Case::Sentence => text::sentence_case_tr(&e.text),
            Case::Title => map_text(&e.text, |t| {
                let mut out = String::new();
                let mut start = true;
                for c in t.chars() {
                    if c.is_alphabetic() {
                        out.push_str(&if start { text::upper_tr(&c.to_string()) } else { text::lower_tr(&c.to_string()) });
                        start = false;
                    } else {
                        start = !(c == '\'' || c == '’');
                        out.push(c);
                    }
                }
                out
            }),
            Case::Invert => map_text(&e.text, |t| {
                t.chars()
                    .map(|c| {
                        if c.is_uppercase() {
                            text::lower_tr(&c.to_string())
                        } else {
                            text::upper_tr(&c.to_string())
                        }
                    })
                    .collect()
            }),
        };
        if new != e.text {
            e.text = new;
            n += 1;
        }
    }
    n
}

/// Remove `[sound]`, `(sound)`, `♪ music ♪` and `NAME:` speaker labels. Lines left empty are deleted.
pub fn remove_hearing_impaired(script: &mut Script) -> usize {
    let brackets = Regex::new(r"\[[^\]]*\]|\([^)]*\)|[♪♫][^♪♫]*[♪♫]?").unwrap();
    let speaker = Regex::new(r"(?m)^(?:\\N)?\s*[\p{Lu}][\p{Lu} .'-]{1,20}:\s*").unwrap();
    let mut changed = 0;
    for e in &mut script.events {
        let orig = e.text.clone();
        let mut t = map_text(&e.text, |s| brackets.replace_all(s, "").into_owned());
        t = map_text(&t, |s| speaker.replace_all(s, "").into_owned());
        let t = t.replace("\\N\\N", "\\N");
        let t = t.trim().trim_start_matches("\\N").trim_end_matches("\\N").trim().to_string();
        if t != orig {
            e.text = t;
            changed += 1;
        }
    }
    script.events.retain(|e| e.comment || !tags::visible_text(&e.text).trim().is_empty() || !e.effect.is_empty() || orig_is_drawing(&e.text));
    changed
}

fn orig_is_drawing(t: &str) -> bool {
    t.contains("\\p1")
}

/// Visible-character index → byte offset in a text segment sequence; returns candidate break points
/// (byte offsets into the whole string, restricted to plain-text spaces) with the visible length before them.
fn break_candidates(s: &str) -> (Vec<(usize, usize)>, usize) {
    let mut cands = vec![];
    let mut byte = 0;
    let mut vis = 0;
    for seg in tags::parse(s) {
        match seg {
            Segment::Block(tags) => byte += tags::serialize(&[Segment::Block(tags)]).len(),
            Segment::Text(t) => {
                let mut it = t.char_indices().peekable();
                while let Some((i, c)) = it.next() {
                    if c == '\\' && matches!(it.peek(), Some((_, 'N')) | Some((_, 'n'))) {
                        it.next();
                        vis += 1;
                        continue;
                    }
                    if c == ' ' {
                        cands.push((byte + i, vis));
                    }
                    vis += 1;
                }
                byte += t.len();
            }
        }
    }
    (cands, vis)
}

/// Re-break a text into at most two balanced lines when it exceeds `max_len` characters per line
/// (or when `force` is set). Existing `\N` breaks are removed first. Returns the new text.
pub fn balance_lines(s: &str, max_len: usize, force: bool) -> String {
    let flat = s.replace("\\N", " ").replace("\\n", " ");
    let flat = Regex::new(r" {2,}").unwrap().replace_all(&flat, " ").trim().to_string();
    let (cands, total) = break_candidates(&flat);
    if (total <= max_len && !force) || cands.is_empty() {
        return flat;
    }
    let best = cands.iter().min_by_key(|(_, v)| (*v as i64 - (total as i64 - *v as i64)).abs()).unwrap();
    let mut out = flat.clone();
    out.replace_range(best.0..best.0 + 1, "\\N");
    out
}

pub fn balance_all(script: &mut Script, max_len: usize) -> usize {
    let mut n = 0;
    for e in script.events.iter_mut().filter(|e| !e.comment) {
        let t = balance_lines(&e.text, max_len, false);
        if t != e.text {
            e.text = t;
            n += 1;
        }
    }
    n
}

/// Replace all `\N` by a space.
pub fn unbreak(script: &mut Script) -> usize {
    let mut n = 0;
    for e in &mut script.events {
        if e.text.contains("\\N") {
            e.text = e.text.replace("\\N", " ");
            n += 1;
        }
    }
    n
}

/// Split events whose single line is longer than `max_chars` visible chars into several events,
/// dividing the time proportionally to text length. Returns number of events split.
pub fn split_long(script: &mut Script, max_chars: usize) -> usize {
    let mut out: Vec<Event> = Vec::with_capacity(script.events.len());
    let mut n = 0;
    for e in script.events.drain(..) {
        let vis = tags::visible_text(&e.text);
        if e.comment || vis.chars().count() <= max_chars || e.text.contains('{') {
            out.push(e);
            continue;
        }
        // split at sentence ends first, then at spaces so each part is <= max_chars
        let words: Vec<&str> = e.text.split(' ').collect();
        let mut parts: Vec<String> = vec![];
        let mut cur = String::new();
        for w in words {
            let ends_sentence = cur.ends_with(['.', '!', '?', '…']);
            if !cur.is_empty() && (cur.chars().count() + 1 + w.chars().count() > max_chars || (ends_sentence && cur.chars().count() > max_chars / 3)) {
                parts.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        if !cur.is_empty() {
            parts.push(cur);
        }
        if parts.len() < 2 {
            out.push(e);
            continue;
        }
        n += 1;
        let total: usize = parts.iter().map(|p| p.chars().count().max(1)).sum();
        let dur = e.end - e.start;
        let mut t = e.start;
        for (i, p) in parts.iter().enumerate() {
            let share = (dur as f64 * p.chars().count().max(1) as f64 / total as f64).round() as i64;
            let end = if i + 1 == parts.len() { e.end } else { t + share };
            out.push(Event { start: t, end, text: p.clone(), ..e.clone() });
            t = end;
        }
    }
    script.events = out;
    n
}

/// Merge consecutive events with identical text when the gap is at most `max_gap_ms`.
pub fn merge_duplicates(script: &mut Script, max_gap_ms: i64) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i + 1 < script.events.len() {
        let (a, b) = (&script.events[i], &script.events[i + 1]);
        if !a.comment && !b.comment && a.text == b.text && a.style == b.style && b.start - a.end <= max_gap_ms && b.start >= a.start {
            let end = b.end.max(a.end);
            script.events.remove(i + 1);
            script.events[i].end = end;
            n += 1;
        } else {
            i += 1;
        }
    }
    n
}

/// Merge short consecutive lines (both visible texts ≤ `max_chars`, gap ≤ `max_gap_ms`) into one two-line event.
pub fn merge_short(script: &mut Script, max_chars: usize, max_gap_ms: i64) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i + 1 < script.events.len() {
        let (a, b) = (&script.events[i], &script.events[i + 1]);
        let (la, lb) = (tags::visible_text(&a.text), tags::visible_text(&b.text));
        if !a.comment && !b.comment && a.style == b.style && !la.contains('\n') && !lb.contains('\n') && la.chars().count() <= max_chars && lb.chars().count() <= max_chars && b.start - a.end <= max_gap_ms && b.start >= a.end {
            crate::ops::merge_with_next(script, i);
            n += 1;
        } else {
            i += 1;
        }
    }
    n
}

/// Enforce a minimum gap between consecutive events (same style/layer) by trimming the earlier end.
pub fn apply_min_gap(script: &mut Script, gap_ms: i64) -> usize {
    crate::qc::fix_overlaps(script, gap_ms)
}

/// Extend the end of events whose CPS is above `max_cps`, as far as the next event allows (keeping `gap_ms`).
pub fn fix_cps(script: &mut Script, max_cps: f64, gap_ms: i64, max_duration_ms: i64) -> usize {
    let mut idx: Vec<usize> = (0..script.events.len()).filter(|&i| !script.events[i].comment).collect();
    idx.sort_by_key(|&i| (script.events[i].start, i));
    let mut n = 0;
    for (k, &i) in idx.iter().enumerate() {
        let (chars, start, end, style) = {
            let e = &script.events[i];
            (text::reading_chars(&e.text), e.start, e.end, e.style.clone())
        };
        let need = ((chars as f64 / max_cps) * 1000.0).ceil() as i64;
        if end - start >= need {
            continue;
        }
        let limit = idx[k + 1..].iter().map(|&j| &script.events[j]).find(|o| o.style == style).map(|o| o.start - gap_ms).unwrap_or(i64::MAX);
        let new_end = (start + need).min(limit).min(start + max_duration_ms);
        if new_end > end {
            script.events[i].end = new_end;
            n += 1;
        }
    }
    n
}

/// Snap start/end times to frame boundaries.
pub fn snap_to_frames(script: &mut Script, fps: f64) {
    let fr = FrameRate::new(fps);
    for e in &mut script.events {
        e.start = fr.snap_start(e.start);
        e.end = fr.snap_start(e.end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: i64, e: i64, t: &str) -> Event {
        Event { start: s, end: e, text: t.into(), ..Event::default() }
    }

    #[test]
    fn casing_keeps_tags() {
        let mut s = Script::new();
        s.events = vec![ev(0, 1000, "{\\i1}işık{\\i0} ılık\\Nİstanbul")];
        change_case(&mut s, Case::Upper, None);
        assert_eq!(s.events[0].text, "{\\i1}İŞIK{\\i0} ILIK\\NİSTANBUL");
        change_case(&mut s, Case::Lower, None);
        assert_eq!(s.events[0].text, "{\\i1}işık{\\i0} ılık\\Nistanbul");
        change_case(&mut s, Case::Title, None);
        assert_eq!(s.events[0].text, "{\\i1}İşık{\\i0} Ilık\\Nİstanbul");
    }

    #[test]
    fn hi_removal() {
        let mut s = Script::new();
        s.events = vec![ev(0, 1, "[door slams]"), ev(1, 2, "JOHN: Hello (laughs)"), ev(2, 3, "♪ la la ♪"), ev(3, 4, "Keep me")];
        remove_hearing_impaired(&mut s);
        let t: Vec<&str> = s.events.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(t, vec!["Hello", "Keep me"]);
    }

    #[test]
    fn balancing() {
        let t = balance_lines("This is a rather long subtitle line that should be split nicely", 30, false);
        assert!(t.contains("\\N"));
        let parts: Vec<&str> = t.split("\\N").collect();
        assert_eq!(parts.len(), 2);
        assert!((parts[0].len() as i64 - parts[1].len() as i64).abs() < 12, "{t}");
        assert_eq!(balance_lines("short", 30, false), "short");
        // tags are never split
        let t2 = balance_lines("{\\fs20}aaaa bbbb cccc dddd eeee ffff", 10, false);
        assert!(t2.starts_with("{\\fs20}"));
    }

    #[test]
    fn split_and_merge() {
        let mut s = Script::new();
        s.events = vec![ev(0, 8000, "One sentence here. Another sentence follows quickly. And a third one ends it.")];
        assert_eq!(split_long(&mut s, 30), 1);
        assert!(s.events.len() >= 3);
        assert_eq!(s.events.first().unwrap().start, 0);
        assert_eq!(s.events.last().unwrap().end, 8000);
        for w in s.events.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
        let mut d = Script::new();
        d.events = vec![ev(0, 1000, "same"), ev(1100, 2000, "same"), ev(3000, 4000, "other")];
        assert_eq!(merge_duplicates(&mut d, 200), 1);
        assert_eq!(d.events[0].end, 2000);
        let mut m = Script::new();
        m.events = vec![ev(0, 1000, "a"), ev(1000, 2000, "b")];
        assert_eq!(merge_short(&mut m, 10, 100), 1);
        assert_eq!(m.events[0].text, "a\\Nb");
    }

    #[test]
    fn cps_and_frames() {
        let mut s = Script::new();
        s.events = vec![ev(0, 500, "twenty chars of text"), ev(5000, 6000, "next")];
        assert_eq!(fix_cps(&mut s, 20.0, 100, 7000), 1);
        assert_eq!(s.events[0].end, 850); // 17 non-space chars at 20 CPS
    }
}
