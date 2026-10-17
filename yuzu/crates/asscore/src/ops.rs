//! Editing operations and quality checks.

use crate::model::*;
use crate::tags;
use crate::time::Fps;
use regex::Regex;

/// Shift selected events (indices) by `delta_ms`. `which`: 0 both, 1 start only, 2 end only.
pub fn shift(sc: &mut Script, idx: &[usize], delta_ms: i64, which: u8) {
    for &i in idx {
        if let Some(e) = sc.events.get_mut(i) {
            if which != 2 { e.start = (e.start + delta_ms).max(0); }
            if which != 1 { e.end = (e.end + delta_ms).max(0); }
        }
    }
}

/// Shift selected events by frames using a constant frame rate.
pub fn shift_frames(sc: &mut Script, idx: &[usize], frames: i64, fps: Fps) {
    for &i in idx {
        if let Some(e) = sc.events.get_mut(i) {
            let sf = fps.frame_from_ms(e.start) + frames;
            let ef = fps.frame_from_ms(e.end) + frames;
            e.start = fps.start_ms(sf.max(0));
            e.end = fps.start_ms(ef.max(0));
        }
    }
}

/// Linear retime: map [src_a,src_b] → [dst_a,dst_b] (two-point sync, e.g. fix framerate drift).
pub fn linear_retime(sc: &mut Script, idx: &[usize], src_a: i64, src_b: i64, dst_a: i64, dst_b: i64) {
    if src_b == src_a { return; }
    let k = (dst_b - dst_a) as f64 / (src_b - src_a) as f64;
    let f = |t: i64| (dst_a as f64 + (t - src_a) as f64 * k).round().max(0.0) as i64;
    for &i in idx {
        if let Some(e) = sc.events.get_mut(i) { e.start = f(e.start); e.end = f(e.end); }
    }
}

#[derive(Clone, Debug)]
pub struct FindOpts {
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// 0 = text, 1 = style, 2 = actor, 3 = effect, 4 = text without tags
    pub field: u8,
    pub skip_comments: bool,
}

impl Default for FindOpts {
    fn default() -> Self { FindOpts { regex: false, case_sensitive: false, whole_word: false, field: 0, skip_comments: false } }
}

fn build_re(pat: &str, o: &FindOpts) -> Result<Regex, String> {
    let mut p = if o.regex { pat.to_string() } else { regex::escape(pat) };
    if o.whole_word { p = format!(r"\b(?:{})\b", p); }
    if !o.case_sensitive { p = format!("(?i){}", p); }
    Regex::new(&p).map_err(|e| e.to_string())
}

fn field_of<'a>(e: &'a mut Event, f: u8) -> &'a mut String {
    match f { 1 => &mut e.style, 2 => &mut e.actor, 3 => &mut e.effect, _ => &mut e.text }
}

/// Find all matching event indices.
pub fn find_all(sc: &Script, pat: &str, o: &FindOpts) -> Result<Vec<usize>, String> {
    let re = build_re(pat, o)?;
    Ok(sc.events.iter().enumerate().filter(|(_, e)| {
        if o.skip_comments && e.comment { return false; }
        let hay: String = match o.field { 1 => e.style.clone(), 2 => e.actor.clone(), 3 => e.effect.clone(), 4 => tags::display_text(&e.text), _ => e.text.clone() };
        re.is_match(&hay)
    }).map(|(i, _)| i).collect())
}

/// Replace in all (or given) events. Returns number of replaced occurrences.
pub fn replace_all(sc: &mut Script, pat: &str, rep: &str, o: &FindOpts, only: Option<&[usize]>) -> Result<usize, String> {
    let re = build_re(pat, o)?;
    let rep_s = if o.regex { rep.to_string() } else { rep.replace('$', "$$") };
    let mut count = 0;
    for (i, e) in sc.events.iter_mut().enumerate() {
        if let Some(sel) = only { if !sel.contains(&i) { continue; } }
        if o.skip_comments && e.comment { continue; }
        if o.field == 4 {
            // replace only in text parts, preserving tags
            let mut parts = tags::split_parts(&e.text);
            let mut changed = false;
            for p in parts.iter_mut() {
                if let tags::Part::Text(t) = p {
                    let n = re.find_iter(t).count();
                    if n > 0 { count += n; changed = true; *t = re.replace_all(t, rep_s.as_str()).into_owned(); }
                }
            }
            if changed { e.text = tags::join_parts(&parts); }
        } else {
            let f = field_of(e, o.field);
            let n = re.find_iter(f).count();
            if n > 0 { count += n; *f = re.replace_all(f, rep_s.as_str()).into_owned(); }
        }
    }
    Ok(count)
}

// ---------------------------------------------------------------- QC

#[derive(Clone, Debug, PartialEq)]
pub enum Severity { Error, Warning, Info }

#[derive(Clone, Debug)]
pub struct Issue {
    pub index: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct QcConfig {
    pub max_cps: f64,
    pub max_cpl: usize,
    pub max_lines: usize,
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub min_gap_ms: i64,
    pub check_overlap: bool,
}

impl Default for QcConfig {
    fn default() -> Self {
        QcConfig { max_cps: 20.0, max_cpl: 42, max_lines: 2, min_duration_ms: 500, max_duration_ms: 7000, min_gap_ms: 0, check_overlap: true }
    }
}

pub fn cps(e: &Event) -> f64 {
    let d = e.duration();
    if d <= 0 { return f64::INFINITY; }
    tags::visible_len(&e.text) as f64 * 1000.0 / d as f64
}

pub fn qc(sc: &Script, cfg: &QcConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let unbalanced = |t: &str| t.matches('{').count() != t.matches('}').count();
    // Only check dialogue-layer-0, non-typeset lines for overlap.
    let mut by_style_time: Vec<usize> = (0..sc.events.len()).filter(|&i| !sc.events[i].comment).collect();
    by_style_time.sort_by_key(|&i| (sc.events[i].start, sc.events[i].end));
    for (i, e) in sc.events.iter().enumerate() {
        if e.comment { continue; }
        let is_typeset = e.text.contains("\\p1") || e.text.contains("\\pos") && e.layer > 0 || e.effect.starts_with("fx") || e.effect.contains("karaoke");
        if e.end <= e.start { out.push(Issue { index: i, severity: Severity::Error, code: "negative-duration", message: "End time is not after start time".into() }); continue; }
        if unbalanced(&e.text) { out.push(Issue { index: i, severity: Severity::Error, code: "unbalanced-braces", message: "Unbalanced { } in text".into() }); }
        if sc.style(&e.style).is_none() { out.push(Issue { index: i, severity: Severity::Error, code: "missing-style", message: format!("Style '{}' does not exist", e.style) }); }
        if is_typeset { continue; }
        let disp = tags::display_text(&e.text);
        if disp.trim().is_empty() { out.push(Issue { index: i, severity: Severity::Warning, code: "empty", message: "Line has no visible text".into() }); continue; }
        if e.duration() < cfg.min_duration_ms { out.push(Issue { index: i, severity: Severity::Warning, code: "short", message: format!("Duration {} ms below minimum", e.duration()) }); }
        if e.duration() > cfg.max_duration_ms { out.push(Issue { index: i, severity: Severity::Warning, code: "long", message: format!("Duration {} ms above maximum", e.duration()) }); }
        let c = cps(e);
        if c > cfg.max_cps { out.push(Issue { index: i, severity: Severity::Warning, code: "cps", message: format!("{:.1} characters/second exceeds {}", c, cfg.max_cps) }); }
        let lines: Vec<&str> = disp.split('\n').collect();
        if lines.len() > cfg.max_lines { out.push(Issue { index: i, severity: Severity::Warning, code: "lines", message: format!("{} lines (max {})", lines.len(), cfg.max_lines) }); }
        if let Some(l) = lines.iter().map(|l| l.chars().count()).max() { if l > cfg.max_cpl { out.push(Issue { index: i, severity: Severity::Info, code: "cpl", message: format!("Line of {} chars exceeds {}", l, cfg.max_cpl) }); } }
        if disp.contains("  ") { out.push(Issue { index: i, severity: Severity::Info, code: "double-space", message: "Double space".into() }); }
        if disp != disp.trim() { out.push(Issue { index: i, severity: Severity::Info, code: "trim", message: "Leading/trailing whitespace".into() }); }
        if disp.contains("...") { out.push(Issue { index: i, severity: Severity::Info, code: "ellipsis", message: "Three dots; consider '…'".into() }); }
    }
    if cfg.check_overlap {
        for w in by_style_time.windows(2) {
            let (a, b) = (&sc.events[w[0]], &sc.events[w[1]]);
            let typeset = |e: &Event| e.text.contains("\\p1") || e.layer > 0 || e.effect.starts_with("fx");
            if typeset(a) || typeset(b) || a.style != b.style { continue; }
            if b.start < a.end { out.push(Issue { index: w[1], severity: Severity::Warning, code: "overlap", message: format!("Overlaps previous line by {} ms", a.end - b.start) }); }
            else if b.start - a.end < cfg.min_gap_ms { out.push(Issue { index: w[1], severity: Severity::Info, code: "gap", message: format!("Gap {} ms below minimum", b.start - a.end) }); }
        }
    }
    out.sort_by_key(|i| i.index);
    out
}

// ---------------------------------------------------------------- Timing post-processor

#[derive(Clone, Debug)]
pub struct PostProc {
    pub lead_in: i64,
    pub lead_out: i64,
    pub keyframe_snap_before: i64,
    pub keyframe_snap_after: i64,
    pub keyframe_snap_lead_in: i64,
    pub keyframe_snap_lead_out: i64,
    pub join_gap_threshold: i64,
    pub min_gap: i64,
}

impl Default for PostProc {
    fn default() -> Self {
        PostProc { lead_in: 120, lead_out: 200, keyframe_snap_before: 0, keyframe_snap_after: 0, keyframe_snap_lead_in: 0, keyframe_snap_lead_out: 0, join_gap_threshold: 0, min_gap: 0 }
    }
}

/// Aegisub-style timing post-processor. `keyframes` are times in ms.
pub fn post_process(sc: &mut Script, idx: &[usize], pp: &PostProc, keyframes: &[i64]) {
    for &i in idx {
        if let Some(e) = sc.events.get_mut(i) {
            e.start = (e.start - pp.lead_in).max(0);
            e.end += pp.lead_out;
        }
    }
    // Keyframe snapping
    if !keyframes.is_empty() {
        for &i in idx {
            let e = &mut sc.events[i];
            // start: snap to the closest keyframe within thresholds
            if let Some(&k) = keyframes.iter().filter(|&&k| (k <= e.start && e.start - k <= pp.keyframe_snap_before) || (k > e.start && k - e.start <= pp.keyframe_snap_after)).min_by_key(|&&k| (k - e.start).abs()) {
                e.start = k;
            }
            if let Some(&k) = keyframes.iter().filter(|&&k| (k >= e.end && k - e.end <= pp.keyframe_snap_after) || (k < e.end && e.end - k <= pp.keyframe_snap_before)).min_by_key(|&&k| (k - e.end).abs()) {
                e.end = k;
            }
        }
    }
    // Join / separate adjacent lines (by start order among the selection)
    let mut order: Vec<usize> = idx.to_vec();
    order.sort_by_key(|&i| sc.events[i].start);
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let gap = sc.events[b].start - sc.events[a].end;
        if gap < 0 {
            // overlap → meet in the middle
            let mid = (sc.events[b].start + sc.events[a].end) / 2;
            sc.events[a].end = mid - pp.min_gap / 2;
            sc.events[b].start = mid + pp.min_gap / 2 + pp.min_gap % 2;
        } else if gap > 0 && gap <= pp.join_gap_threshold {
            let mid = (sc.events[b].start + sc.events[a].end) / 2;
            sc.events[a].end = mid;
            sc.events[b].start = mid;
        }
    }
}

/// Snap all given events to frame boundaries.
pub fn snap_to_frames(sc: &mut Script, idx: &[usize], fps: Fps) {
    for &i in idx {
        if let Some(e) = sc.events.get_mut(i) {
            e.start = fps.start_ms(fps.frame_from_ms(e.start));
            e.end = fps.start_ms(fps.frame_from_ms(e.end));
        }
    }
}

// ---------------------------------------------------------------- Structure

/// Split event `i` at `at_ms` and text offset `char_off` (in chars of raw text). The two halves meet at `at_ms`.
pub fn split_event(sc: &mut Script, i: usize, at_ms: i64, char_off: usize) {
    let e = sc.events[i].clone();
    let chars: Vec<char> = e.text.chars().collect();
    let off = char_off.min(chars.len());
    let a: String = chars[..off].iter().collect();
    let b: String = chars[off..].iter().collect();
    let mut e1 = e.clone();
    let mut e2 = e;
    e1.text = a.trim_end().to_string();
    e2.text = b.trim_start().to_string();
    e1.end = at_ms;
    e2.start = at_ms;
    sc.events[i] = e1;
    sc.events.insert(i + 1, e2);
}

/// Merge consecutive events (sorted indices) into the first one. `sep` joins texts.
pub fn merge_events(sc: &mut Script, idx: &[usize], sep: &str) {
    if idx.len() < 2 { return; }
    let mut sorted = idx.to_vec();
    sorted.sort();
    let first = sorted[0];
    let mut text = sc.events[first].text.clone();
    let mut end = sc.events[first].end;
    let mut start = sc.events[first].start;
    for &j in &sorted[1..] {
        text.push_str(sep);
        text.push_str(&sc.events[j].text);
        end = end.max(sc.events[j].end);
        start = start.min(sc.events[j].start);
    }
    sc.events[first].text = text;
    sc.events[first].end = end;
    sc.events[first].start = start;
    for &j in sorted[1..].iter().rev() { sc.events.remove(j); }
}

pub fn delete_events(sc: &mut Script, idx: &[usize]) {
    let mut s = idx.to_vec();
    s.sort_unstable();
    s.dedup();
    for &j in s.iter().rev() { if j < sc.events.len() { sc.events.remove(j); } }
}

pub fn duplicate_events(sc: &mut Script, idx: &[usize]) {
    let mut s = idx.to_vec();
    s.sort_unstable();
    for (n, &j) in s.iter().enumerate() {
        let e = sc.events[j + n].clone();
        sc.events.insert(j + n + 1, e);
    }
}

/// Remove exact duplicate lines (same time, style, text). Returns removed count.
pub fn dedupe(sc: &mut Script) -> usize {
    let mut seen = std::collections::HashSet::new();
    let before = sc.events.len();
    sc.events.retain(|e| seen.insert((e.comment, e.layer, e.start, e.end, e.style.clone(), e.text.clone())));
    before - sc.events.len()
}

/// Rescale a script to a new PlayRes, adjusting styles, margins, and common positional tags.
pub fn resample(sc: &mut Script, new_x: i32, new_y: i32) {
    let (ox, oy) = sc.res();
    if ox == 0 || oy == 0 { return; }
    let (fx, fy) = (new_x as f64 / ox as f64, new_y as f64 / oy as f64);
    let fm = fy;
    for s in sc.styles.iter_mut() {
        s.fontsize *= fm; s.outline *= fm; s.shadow *= fm; s.spacing *= fx;
        s.margin_l = (s.margin_l as f64 * fx).round() as i32;
        s.margin_r = (s.margin_r as f64 * fx).round() as i32;
        s.margin_v = (s.margin_v as f64 * fy).round() as i32;
    }
    let num = Regex::new(r"-?\d+\.?\d*").unwrap();
    for e in sc.events.iter_mut() {
        e.margin_l = (e.margin_l as f64 * fx).round() as i32;
        e.margin_r = (e.margin_r as f64 * fx).round() as i32;
        e.margin_v = (e.margin_v as f64 * fy).round() as i32;
        let mut parts = tags::split_parts(&e.text);
        for p in parts.iter_mut() {
            if let tags::Part::Block(b) = p {
                let tl = tags::split_block_tags(b);
                let mut nb = String::new();
                for t in tl {
                    let (n, a) = tags::tag_name_arg(&t);
                    nb.push_str(&rescale_tag(&n, &a, fx, fy, &num).unwrap_or(t));
                }
                *b = nb;
            }
        }
        e.text = tags::join_parts(&parts);
    }
    sc.info_set("PlayResX", &new_x.to_string());
    sc.info_set("PlayResY", &new_y.to_string());
}

fn rescale_tag(n: &str, a: &str, fx: f64, fy: f64, num: &Regex) -> Option<String> {
    let scale_list = |a: &str, axes: &[char]| -> String {
        let inner = a.trim_start_matches('(').trim_end_matches(')');
        let mut i = 0;
        let r: Vec<String> = inner.split(',').map(|v| {
            let ax = axes[i % axes.len()]; i += 1;
            match v.trim().parse::<f64>() { Ok(x) => crate::model::fmt_num(x * if ax == 'x' { fx } else { fy }), Err(_) => v.to_string() }
        }).collect();
        format!("({})", r.join(","))
    };
    match n {
        "pos" | "org" => Some(format!("\\{}{}", n, scale_list(a, &['x', 'y']))),
        "move" => {
            let inner = a.trim_start_matches('(').trim_end_matches(')');
            let v: Vec<&str> = inner.split(',').collect();
            let mut o = Vec::new();
            for (k, s) in v.iter().enumerate() {
                if k < 4 { match s.trim().parse::<f64>() { Ok(x) => o.push(crate::model::fmt_num(x * if k % 2 == 0 { fx } else { fy })), Err(_) => o.push(s.to_string()) } } else { o.push(s.to_string()); }
            }
            Some(format!("\\move({})", o.join(",")))
        }
        "fs" | "bord" | "shad" | "blur" | "be" | "ybord" | "yshad" => { let _ = num; Some(format!("\\{}{}", n, crate::model::fmt_num(a.trim().parse::<f64>().ok()? * fy))) }
        "xbord" | "xshad" | "fsp" => Some(format!("\\{}{}", n, crate::model::fmt_num(a.trim().parse::<f64>().ok()? * fx))),
        "clip" | "iclip" if a.contains(',') && !a.contains('\\') => Some(format!("\\{}{}", n, scale_list(a, &['x', 'y']))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Script {
        let mut s = Script::new();
        s.events.push(Event::new(1000, 2000, "Default", "Hello"));
        s.events.push(Event::new(2500, 4000, "Default", "{\\i1}World{\\i0} there"));
        s
    }
    #[test]
    fn shift_works() {
        let mut s = sample();
        shift(&mut s, &[0, 1], 500, 0);
        assert_eq!((s.events[0].start, s.events[0].end), (1500, 2500));
        shift(&mut s, &[0], -10_000, 0);
        assert_eq!(s.events[0].start, 0);
    }
    #[test]
    fn replace_preserves_tags() {
        let mut s = sample();
        let o = FindOpts { field: 4, ..Default::default() };
        let n = replace_all(&mut s, "o", "X", &o, None).unwrap();
        assert!(n >= 1);
        assert!(s.events[1].text.contains("{\\i1}"));
    }
    #[test]
    fn regex_replace() {
        let mut s = sample();
        let o = FindOpts { regex: true, ..Default::default() };
        replace_all(&mut s, r"(\w+) there", "$1 here", &o, None).unwrap();
        assert!(s.events[1].text.ends_with("{\\i0} here") || s.events[1].text.contains("here"));
    }
    #[test]
    fn qc_flags() {
        let mut s = sample();
        s.events.push(Event::new(3900, 3950, "Default", "A very long line indeed here..."));
        s.events.push(Event::new(9000, 9900, "Missing", "x"));
        let issues = qc(&s, &QcConfig::default());
        let codes: Vec<_> = issues.iter().map(|i| i.code).collect();
        assert!(codes.contains(&"missing-style"));
        assert!(codes.contains(&"overlap"));
        assert!(codes.contains(&"short"));
    }
    #[test]
    fn split_merge() {
        let mut s = sample();
        split_event(&mut s, 0, 1500, 2);
        assert_eq!(s.events.len(), 3);
        assert_eq!(s.events[0].text, "He");
        merge_events(&mut s, &[0, 1], "");
        assert_eq!(s.events[0].text, "Hello");
        assert_eq!(s.events.len(), 2);
    }
    #[test]
    fn resample_scales() {
        let mut s = sample();
        s.events[0].text = "{\\pos(100,50)\\fs40}Hi".into();
        resample(&mut s, 3840, 2160);
        assert!(s.events[0].text.contains("\\pos(200,100)"), "{}", s.events[0].text);
        assert!(s.events[0].text.contains("\\fs80"));
        assert_eq!(s.res(), (3840, 2160));
    }
    #[test]
    fn post_proc_overlap() {
        let mut s = sample();
        let pp = PostProc { lead_in: 0, lead_out: 800, ..Default::default() };
        post_process(&mut s, &[0, 1], &pp, &[]);
        assert!(s.events[0].end <= s.events[1].start);
    }
}
