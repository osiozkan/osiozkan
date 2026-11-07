//! Batch fixes inspired by Subtitle Edit's "fix common errors" and Aegisub's timing helpers.
use crate::ass::AssFile;
use crate::tags;

/// Byte positions of ASCII spaces that are outside `{...}` override blocks.
fn plain_spaces(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut depth = false;
    for (i, c) in text.char_indices() {
        match c { '{' => depth = true, '}' => depth = false, ' ' if !depth => out.push(i), _ => {} }
    }
    out
}

/// Insert `\N` at the space that best balances the two lines. No-op if the text already has a break
/// or fits within `max_len`. `bottom_heavy` prefers a longer second line (looks better under video).
pub fn auto_break(text: &str, max_len: usize, bottom_heavy: bool) -> String {
    if text.contains("\\N") || text.contains("\\n") { return text.to_string(); }
    let total = tags::visible_len(text);
    if total <= max_len { return text.to_string(); }
    let spaces = plain_spaces(text);
    if spaces.is_empty() { return text.to_string(); }
    let target = if bottom_heavy { (total as f64 * 0.45).round() as usize } else { total / 2 };
    let mut best: Option<(usize, usize)> = None; // (score, pos)
    for &p in &spaces {
        let left = tags::visible_len(&text[..p]);
        let score = left.abs_diff(target);
        if best.map_or(true, |(s, _)| score < s) { best = Some((score, p)); }
    }
    let p = best.unwrap().1;
    format!("{}\\N{}", text[..p].trim_end(), text[p + 1..].trim_start())
}

/// Replace `\N` with a space.
pub fn unbreak(text: &str) -> String {
    text.replace(" \\N", " ").replace("\\N ", " ").replace("\\N", " ")
}

pub fn auto_break_all(f: &mut AssFile, max_len: usize, bottom_heavy: bool) -> usize {
    let mut n = 0;
    for e in f.events.iter_mut().filter(|e| !e.comment) {
        let t = auto_break(&e.text, max_len, bottom_heavy);
        if t != e.text { e.text = t; n += 1; }
    }
    n
}

/// Remove hearing-impaired annotations: `[ ... ]`, `( ... )`, and leading speaker labels like `AYŞE:`.
pub fn remove_hi(f: &mut AssFile) -> usize {
    let re_br = regex::Regex::new(r"\s*(\[[^\]]*\]|\([^)]*\))\s*").unwrap();
    let re_label = regex::Regex::new(r"^(\\N)?\s*[A-ZÇĞİÖŞÜ][A-ZÇĞİÖŞÜ0-9 .'-]{1,24}:\s*").unwrap();
    let mut changed = 0;
    for e in &mut f.events {
        if e.comment { continue; }
        let before = e.text.clone();
        let t = tags::map_plain(&e.text, |s| re_label.replace_all(&re_br.replace_all(s, " "), "$1").trim().to_string());
        e.text = t.replace("\\N\\N", "\\N").trim_matches(|c: char| c == ' ').trim_start_matches("\\N").trim_end_matches("\\N").to_string();
        if e.text != before { changed += 1; }
    }
    f.events.retain(|e| e.comment || !tags::visible_text(&e.text).trim().is_empty() || before_non_text(e));
    changed
}
fn before_non_text(e: &crate::ass::Event) -> bool { !e.effect.is_empty() } // keep effect-bearing empties (typesetting)

/// Trim earlier lines so same-style neighbours never overlap. Returns number of lines changed.
pub fn fix_overlaps(f: &mut AssFile) -> usize {
    let mut order: Vec<usize> = (0..f.events.len()).filter(|&i| !f.events[i].comment && f.events[i].effect.is_empty()).collect();
    order.sort_by_key(|&i| (f.events[i].start, f.events[i].end));
    let mut n = 0;
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        if f.events[b].start < f.events[a].end && f.events[a].style == f.events[b].style && f.events[a].layer == f.events[b].layer {
            f.events[a].end = f.events[b].start.max(f.events[a].start + 1); n += 1;
        }
    }
    n
}

/// Extend line ends (never past the next line's start minus `min_gap_ms`, never beyond `max_dur_ms`)
/// until reading speed is at most `max_cps`. Returns number of lines extended.
pub fn extend_for_cps(f: &mut AssFile, max_cps: f64, min_gap_ms: i64, max_dur_ms: i64) -> usize {
    let mut order: Vec<usize> = (0..f.events.len()).filter(|&i| !f.events[i].comment).collect();
    order.sort_by_key(|&i| f.events[i].start);
    let mut n = 0;
    for (k, &i) in order.iter().enumerate() {
        let len = tags::visible_len(&f.events[i].text) as f64;
        let need = (len * 1000.0 / max_cps).ceil() as i64;
        let dur = f.events[i].end - f.events[i].start;
        if dur >= need { continue; }
        let limit = order.get(k + 1).map(|&j| f.events[j].start - min_gap_ms).unwrap_or(i64::MAX);
        let new_end = (f.events[i].start + need).min(limit).min(f.events[i].start + max_dur_ms);
        if new_end > f.events[i].end { f.events[i].end = new_end; n += 1; }
    }
    n
}

/// Guarantee a minimum duration where room is available.
pub fn enforce_min_duration(f: &mut AssFile, min_ms: i64, min_gap_ms: i64) -> usize {
    let mut order: Vec<usize> = (0..f.events.len()).filter(|&i| !f.events[i].comment).collect();
    order.sort_by_key(|&i| f.events[i].start);
    let mut n = 0;
    for (k, &i) in order.iter().enumerate() {
        let dur = f.events[i].end - f.events[i].start;
        if dur >= min_ms { continue; }
        let limit = order.get(k + 1).map(|&j| f.events[j].start - min_gap_ms).unwrap_or(i64::MAX);
        let new_end = (f.events[i].start + min_ms).min(limit);
        if new_end > f.events[i].end { f.events[i].end = new_end; n += 1; }
    }
    n
}

/// Collapse spaces, fix punctuation spacing, `...` -> `…`, trim each line. Tags untouched.
pub fn clean_text(f: &mut AssFile) -> usize {
    let mut n = 0;
    for e in f.events.iter_mut().filter(|e| !e.comment) {
        let t = tags::map_plain(&e.text, |s| {
            let mut s = s.replace("...", "…");
            while s.contains("  ") { s = s.replace("  ", " "); }
            for p in [" ,", " .", " !", " ?", " ;", " :", " …"] { s = s.replace(p, &p[1..]); }
            s.replace(" \\N", "\\N").replace("\\N ", "\\N")
        });
        let t = t.trim().to_string();
        if t != e.text { e.text = t; n += 1; }
    }
    n
}

/// Dialogue dash normalisation: lines beginning with "-" get "- " (hyphen + space).
pub fn normalize_dashes(f: &mut AssFile) -> usize {
    let re = regex::Regex::new(r"(^|\\N)[-–—]\s*").unwrap();
    let mut n = 0;
    for e in f.events.iter_mut().filter(|e| !e.comment) {
        let t = re.replace_all(&e.text, "$1- ").into_owned();
        if t != e.text { e.text = t; n += 1; }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;
    fn ev(a: i64, b: i64, t: &str) -> Event { Event { start: a, end: b, text: t.into(), ..Default::default() } }
    #[test]
    fn breaks() {
        let t = "Bu çok uzun bir cümle ve ikiye bölünmesi gerekiyor galiba";
        let b = auto_break(t, 30, false);
        assert!(b.contains("\\N"));
        assert_eq!(unbreak(&b), t);
        assert_eq!(auto_break("kısa", 30, false), "kısa");
        let tagged = auto_break("{\\i1}Bu çok uzun bir cümle{\\i0} ve bölünmeli sanırım evet", 30, false);
        assert!(tagged.contains("\\N") && tagged.starts_with("{\\i1}"));
    }
    #[test]
    fn hi_removal() {
        let mut f = AssFile::new_default();
        f.events = vec![ev(0, 1000, "[müzik çalıyor]"), ev(1000, 2000, "AYŞE: Merhaba (gülüyor)"), ev(2000, 3000, "normal")];
        remove_hi(&mut f);
        assert_eq!(f.events.len(), 2);
        assert_eq!(f.events[0].text, "Merhaba");
    }
    #[test]
    fn timing_fixes() {
        let mut f = AssFile::new_default();
        f.events = vec![ev(0, 1500, "a"), ev(1000, 2000, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"), ev(5000, 5100, "x")];
        assert_eq!(fix_overlaps(&mut f), 1);
        assert_eq!(f.events[0].end, 1000);
        assert_eq!(extend_for_cps(&mut f, 17.0, 84, 7000), 1);
        assert!(f.events[1].end > 2000 && f.events[1].end <= 5000 - 84);
        assert_eq!(enforce_min_duration(&mut f, 833, 84), 1);
        assert_eq!(f.events[2].end, 5833);
    }
    #[test]
    fn cleaning() {
        let mut f = AssFile::new_default();
        f.events = vec![ev(0, 1000, "a  b ...\\N- c")];
        clean_text(&mut f);
        assert_eq!(f.events[0].text, "a b…\\N- c");
        normalize_dashes(&mut f);
        assert_eq!(f.events[0].text, "a b…\\N- c");
    }
}
