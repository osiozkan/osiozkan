//! Timing and bulk-edit operations (shift, stretch, framerate conversion, find/replace, merge).

use crate::ass::Script;
use regex::Regex;

pub fn shift(script: &mut Script, delta_ms: i64, range: Option<(usize, usize)>) {
    let (a, b) = range.unwrap_or((0, script.events.len()));
    for e in script.events.iter_mut().skip(a).take(b.saturating_sub(a)) {
        e.start = (e.start + delta_ms).max(0);
        e.end = (e.end + delta_ms).max(0);
    }
}

/// Linear two-point sync: map time `a_old→a_new` and `b_old→b_new`.
pub fn sync_two_points(script: &mut Script, a_old: i64, a_new: i64, b_old: i64, b_new: i64) -> Result<(), String> {
    if a_old == b_old {
        return Err("sync points must differ".into());
    }
    let k = (b_new - a_new) as f64 / (b_old - a_old) as f64;
    let f = |t: i64| ((t - a_old) as f64 * k + a_new as f64).round().max(0.0) as i64;
    for e in &mut script.events {
        e.start = f(e.start);
        e.end = f(e.end);
    }
    Ok(())
}

/// Reinterpret timings made for `from_fps` as `to_fps` (e.g. 23.976 → 25).
pub fn change_framerate(script: &mut Script, from_fps: f64, to_fps: f64) {
    let k = from_fps / to_fps;
    for e in &mut script.events {
        e.start = (e.start as f64 * k).round() as i64;
        e.end = (e.end as f64 * k).round() as i64;
    }
}

#[derive(Clone, Debug, Default)]
pub struct Replace {
    pub pattern: String,
    pub with: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Only replace in this style, if set.
    pub style: Option<String>,
    /// Only touch visible text (leave `{...}` blocks alone).
    pub skip_tags: bool,
}

/// Returns number of events changed.
pub fn find_replace(script: &mut Script, r: &Replace) -> Result<usize, String> {
    let mut pat = if r.regex { r.pattern.clone() } else { regex::escape(&r.pattern) };
    if r.whole_word {
        pat = format!(r"\b(?:{})\b", pat);
    }
    if !r.case_sensitive {
        pat = format!("(?i){}", pat);
    }
    let re = Regex::new(&pat).map_err(|e| e.to_string())?;
    let rep = if r.regex { r.with.clone() } else { r.with.replace('$', "$$") };
    let mut n = 0;
    for e in &mut script.events {
        if r.style.as_ref().is_some_and(|s| *s != e.style) {
            continue;
        }
        let new = if r.skip_tags {
            use crate::tags::Segment;
            let mut segs = crate::tags::parse(&e.text);
            for s in &mut segs {
                if let Segment::Text(t) = s {
                    *t = re.replace_all(t, rep.as_str()).into_owned();
                }
            }
            crate::tags::serialize(&segs)
        } else {
            re.replace_all(&e.text, rep.as_str()).into_owned()
        };
        if new != e.text {
            e.text = new;
            n += 1;
        }
    }
    Ok(n)
}

/// Join events `i` and `i+1` into one (text joined with `\N`).
pub fn merge_with_next(script: &mut Script, i: usize) -> bool {
    if i + 1 >= script.events.len() {
        return false;
    }
    let next = script.events.remove(i + 1);
    let e = &mut script.events[i];
    e.end = e.end.max(next.end);
    e.text = format!("{}\\N{}", e.text, next.text);
    true
}

/// Split event at `at_ms` into two events, text split at char index `at_char` of the raw text.
pub fn split_event(script: &mut Script, i: usize, at_ms: i64, at_char: usize) -> bool {
    let Some(e) = script.events.get(i).cloned() else { return false };
    if at_ms <= e.start || at_ms >= e.end {
        return false;
    }
    let chars: Vec<char> = e.text.chars().collect();
    let at = at_char.min(chars.len());
    let (l, r): (String, String) = (chars[..at].iter().collect(), chars[at..].iter().collect());
    let mut a = e.clone();
    a.end = at_ms;
    a.text = l.trim_end().to_string();
    let mut b = e;
    b.start = at_ms;
    b.text = r.trim_start().to_string();
    script.events[i] = a;
    script.events.insert(i + 1, b);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;
    fn ev(s: i64, e: i64, t: &str) -> Event {
        Event { start: s, end: e, text: t.into(), ..Event::default() }
    }
    #[test]
    fn shifting_and_sync() {
        let mut s = Script::new();
        s.events = vec![ev(1000, 2000, "a"), ev(5000, 6000, "b")];
        shift(&mut s, 500, None);
        assert_eq!(s.events[0].start, 1500);
        sync_two_points(&mut s, 1500, 1000, 5500, 9000).unwrap();
        assert_eq!(s.events[0].start, 1000);
        assert_eq!(s.events[1].start, 9000);
    }
    #[test]
    fn replace_skip_tags() {
        let mut s = Script::new();
        s.events = vec![ev(0, 1, "{\\i1}i am{\\i0} i")];
        let n = find_replace(&mut s, &Replace { pattern: "i".into(), with: "X".into(), skip_tags: true, whole_word: true, ..Default::default() }).unwrap();
        assert_eq!(n, 1);
        assert_eq!(s.events[0].text, "{\\i1}X am{\\i0} X");
    }
    #[test]
    fn merge_split() {
        let mut s = Script::new();
        s.events = vec![ev(0, 1000, "a"), ev(1000, 2000, "b")];
        assert!(merge_with_next(&mut s, 0));
        assert_eq!(s.events[0].text, "a\\Nb");
        assert!(split_event(&mut s, 0, 1000, 1));
        assert_eq!(s.events.len(), 2);
    }
}
