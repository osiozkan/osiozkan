//! One-click subtitle fixes modelled on Subtitle Edit's "Fix common errors" and related tools.
//!
//! Conventions shared by every function in this module:
//!
//! * Functions take `&mut Script` and a selection `sel: &[usize]` of event indices. An **empty
//!   selection means "all events"**; out-of-range indices are ignored.
//! * Every function returns `Vec<Change>` (or a report struct holding one) describing what was
//!   modified, so a UI can show a preview/undo list. `Change::index` is the event index in the script
//!   *as it was when the function was called* (relevant for functions that delete or split events).
//! * Text edits only touch text outside `{}` override blocks (the ASS text is converted to a
//!   "visible" string with one private-use placeholder per block, edited, and restored); an edit that
//!   would drop or reorder a block is discarded. `\N`/`\n` are seen as line breaks, `\h` as NBSP.
//! * Comments, drawings (`\p1`), typesetting (`\pos` on layer > 0, effects starting with `fx`,
//!   `karaoke`) and lines with karaoke tags are never touched by text fixes.

#![allow(clippy::regex_creation_in_loops)]

use crate::model::*;
use crate::ops::{Issue, Severity};
use crate::tags::{self, Part};
use crate::time;
use regex::{Captures, Regex};
use std::collections::HashMap;

macro_rules! re {
    ($p:expr) => {{
        static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        R.get_or_init(|| Regex::new($p).unwrap())
    }};
}

/// A single modification (or proposed modification) of one event.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub index: usize,
    pub description: String,
    pub before: String,
    pub after: String,
}

// ------------------------------------------------------------------------------------------
// Language / casing helpers
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    /// English-style rules (pronoun "I", `i`/`I` pairing).
    #[default]
    English,
    /// Turkish: `I`<->`ı`, `İ`<->`i`.
    Turkish,
    /// Unicode default casing, no language specific rules.
    Other,
}

impl Lang {
    pub fn from_code(code: &str) -> Lang {
        match code.trim().to_ascii_lowercase().as_str() {
            "tr" | "tur" | "turkish" | "tr-tr" | "türkçe" => Lang::Turkish,
            "en" | "eng" | "english" | "en-us" | "en-gb" => Lang::English,
            _ => Lang::Other,
        }
    }
}

fn lower_char(c: char, lang: Lang) -> String {
    match (c, lang) {
        ('I', Lang::Turkish) => "ı".into(),
        ('İ', _) => "i".into(),
        _ => c.to_lowercase().collect(),
    }
}

fn upper_char(c: char, lang: Lang) -> String {
    match (c, lang) {
        ('i', Lang::Turkish) => "İ".into(),
        ('ı', _) => "I".into(),
        _ => c.to_uppercase().collect(),
    }
}

/// Lower-case a string. Turkish: `I`->`ı`, `İ`->`i`. Other languages map `İ` to plain `i`.
pub fn to_lower(s: &str, lang: Lang) -> String {
    s.chars().map(|c| lower_char(c, lang)).collect()
}

/// Upper-case a string. Turkish: `i`->`İ`, `ı`->`I`.
pub fn to_upper(s: &str, lang: Lang) -> String {
    s.chars().map(|c| upper_char(c, lang)).collect()
}

/// Turkish lower-casing (`to_lower(s, Lang::Turkish)`).
pub fn tr_lower(s: &str) -> String {
    to_lower(s, Lang::Turkish)
}

/// Turkish upper-casing (`to_upper(s, Lang::Turkish)`).
pub fn tr_upper(s: &str) -> String {
    to_upper(s, Lang::Turkish)
}

fn is_ph(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
}

/// True when the text has at least `min_letters` letters and none of them is lower-case.
pub fn is_all_caps(s: &str, min_letters: usize) -> bool {
    let mut n = 0;
    for c in s.chars() {
        if c.is_lowercase() {
            return false;
        }
        if c.is_uppercase() {
            n += 1;
        }
    }
    n >= min_letters && n > 0
}

fn is_dash(c: char) -> bool {
    matches!(c, '-' | '–' | '—')
}

/// Sentence case: everything lower-case except the first letter of each sentence/line-leading dash
/// and the entries of `names` (kept in the given spelling). English also capitalises the pronoun "I".
/// Placeholders (private-use chars) and line breaks pass through, so it can be used on "visible" text.
pub fn sentence_case(s: &str, lang: Lang, names: &[String]) -> String {
    let mut out = String::with_capacity(s.len());
    let (mut cap, mut term, mut line_start) = (true, false, true);
    for c in s.chars() {
        if is_ph(c) {
            out.push(c);
            continue;
        }
        if c.is_alphabetic() {
            if cap {
                out.push_str(&upper_char(c, lang));
            } else {
                out.push_str(&lower_char(c, lang));
            }
            cap = false;
            term = false;
            line_start = false;
        } else if c.is_whitespace() {
            if term {
                cap = true;
                term = false;
            }
            if c == '\n' {
                line_start = true;
            }
            out.push(c);
        } else if c.is_numeric() {
            out.push(c);
            cap = false;
            term = false;
            line_start = false;
        } else {
            if matches!(c, '.' | '!' | '?' | '…') {
                term = true;
            } else if is_dash(c) && line_start {
                cap = true;
            } else if !matches!(c, '"' | '\'' | ')' | ']' | '»' | '”' | '’' | '*') {
                term = false;
            }
            if !is_dash(c) {
                line_start = false;
            }
            out.push(c);
        }
    }
    if lang == Lang::English {
        out = re!(r"\bi\b").replace_all(&out, "I").into_owned();
    }
    for n in names {
        let n = n.trim();
        if n.is_empty() {
            continue;
        }
        if let Ok(r) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(n))) {
            out = r.replace_all(&out, regex::NoExpand(n)).into_owned();
        }
    }
    out
}

/// Title case: first letter of every word upper-case, the rest lower-case (apostrophes inside a word
/// do not start a new word).
pub fn title_case(s: &str, lang: Lang) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut new_word = true;
    for (i, &c) in chars.iter().enumerate() {
        if is_ph(c) {
            out.push(c);
        } else if c.is_alphabetic() {
            out.push_str(&if new_word { upper_char(c, lang) } else { lower_char(c, lang) });
            new_word = false;
        } else if c.is_numeric() {
            new_word = false;
            out.push(c);
        } else {
            if matches!(c, '\'' | '’') {
                let prev_l = i > 0 && chars[i - 1].is_alphabetic();
                let next_l = chars.get(i + 1).map(|x| x.is_alphabetic()).unwrap_or(false);
                new_word = !(prev_l && next_l);
            } else {
                new_word = true;
            }
            out.push(c);
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseMode {
    Upper,
    Lower,
    Title,
    Sentence,
}

// ------------------------------------------------------------------------------------------
// Selection / protection / visible-text plumbing
// ------------------------------------------------------------------------------------------

fn targets(sc: &Script, sel: &[usize]) -> Vec<usize> {
    if sel.is_empty() {
        (0..sc.events.len()).collect()
    } else {
        let mut v: Vec<usize> = sel.iter().copied().filter(|&i| i < sc.events.len()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Typesetting / drawing line (`\p1`, `\pos` on layer > 0, effect `fx...` or `karaoke...`).
pub fn is_typeset(e: &Event) -> bool {
    let eff = e.effect.to_ascii_lowercase();
    re!(r"\\p[1-9]").is_match(&e.text)
        || (e.text.contains("\\pos") && e.layer > 0)
        || eff.starts_with("fx")
        || eff.contains("karaoke")
}

pub fn has_karaoke_tags(e: &Event) -> bool {
    re!(r"\\(?:kf|ko|kt|[kK])\d").is_match(&e.text)
}

/// Events that text fixes leave alone: comments, typesetting and karaoke lines.
pub fn is_protected(e: &Event) -> bool {
    e.comment || is_typeset(e) || has_karaoke_tags(e)
}

/// Convert ASS text into a plain string with placeholders for override blocks.
fn to_vis(text: &str) -> Option<(String, Vec<String>)> {
    if text.chars().any(is_ph) {
        return None;
    }
    let mut s = String::new();
    let mut blocks = Vec::new();
    for p in tags::split_parts(text) {
        match p {
            Part::Text(t) => s.push_str(&t.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", "\u{a0}")),
            Part::Block(b) => {
                if blocks.len() >= 6000 {
                    return None;
                }
                s.push(char::from_u32(0xE000 + blocks.len() as u32)?);
                blocks.push(b);
            }
        }
    }
    Some((s, blocks))
}

fn render(s: &str, blocks: &[String], strict: bool) -> Option<String> {
    let mut out = String::new();
    let mut next = 0usize;
    for c in s.chars() {
        if is_ph(c) {
            let idx = (c as u32 - 0xE000) as usize;
            if strict && idx != next {
                return None;
            }
            out.push('{');
            out.push_str(blocks.get(idx)?);
            out.push('}');
            next += 1;
        } else if c == '\n' {
            out.push_str("\\N");
        } else if c == '\u{a0}' {
            out.push_str("\\h");
        } else {
            out.push(c);
        }
    }
    if strict && next != blocks.len() {
        return None;
    }
    Some(out)
}

/// Apply `f` to the visible form of `text`; `None` when nothing changed or the edit was unsafe.
fn edit_text(text: &str, f: &dyn Fn(&str) -> String) -> Option<String> {
    let (v, blocks) = to_vis(text)?;
    let n = f(&v);
    if n == v {
        return None;
    }
    let r = render(&n, &blocks, true)?;
    if r == text {
        None
    } else {
        Some(r)
    }
}

fn apply_text(sc: &mut Script, sel: &[usize], desc: &str, f: &dyn Fn(&str) -> String) -> Vec<Change> {
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        if is_protected(&sc.events[i]) {
            continue;
        }
        if let Some(new) = edit_text(&sc.events[i].text, f) {
            out.push(Change { index: i, description: desc.into(), before: sc.events[i].text.clone(), after: new.clone() });
            sc.events[i].text = new;
        }
    }
    out
}

fn vlen(s: &str) -> usize {
    s.chars().filter(|c| !is_ph(*c) && *c != '\n').count()
}

fn keep_ph(c: &Captures) -> String {
    c[0].chars().filter(|x| is_ph(*x)).collect()
}

fn fmt_range(s: i64, e: i64) -> String {
    format!("{} --> {}", time::format_srt(s), time::format_srt(e))
}

fn visible_empty(text: &str) -> bool {
    tags::display_text(text).trim().is_empty()
}

fn visible_chars(e: &Event) -> usize {
    tags::visible_len(&e.text)
}

/// Key identifying events that share a screen position: overlap/gap rules apply within a track.
fn track_key(e: &Event) -> (String, i32, String, bool) {
    let an = re!(r"\\an?(\d)").captures(&e.text).map(|c| c[0].to_string()).unwrap_or_default();
    let pos = e.text.contains("\\pos") || e.text.contains("\\move");
    (e.style.clone(), e.layer, an, pos)
}

/// For each event: the next event (by start time) in the same track, if any.
fn next_in_track(sc: &Script) -> Vec<Option<usize>> {
    let mut groups: HashMap<(String, i32, String, bool), Vec<usize>> = HashMap::new();
    for (i, e) in sc.events.iter().enumerate() {
        if is_protected(e) || visible_empty(&e.text) {
            continue;
        }
        groups.entry(track_key(e)).or_default().push(i);
    }
    let mut next = vec![None; sc.events.len()];
    for g in groups.values_mut() {
        g.sort_by_key(|&i| (sc.events[i].start, i));
        for w in g.windows(2) {
            next[w[0]] = Some(w[1]);
        }
    }
    next
}

// ------------------------------------------------------------------------------------------
// Timing fixes
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DurationOpts {
    pub min_ms: i64,
    pub max_ms: i64,
    /// Minimum gap kept before the next line when extending.
    pub min_gap_ms: i64,
    /// Reading speed used to compute an "optimal" duration.
    pub target_cps: f64,
}

impl Default for DurationOpts {
    fn default() -> Self {
        DurationOpts { min_ms: 1000, max_ms: 7000, min_gap_ms: 83, target_cps: 15.0 }
    }
}

fn optimal_ms(chars: usize, cps: f64) -> i64 {
    if cps <= 0.0 {
        return 0;
    }
    (chars as f64 * 1000.0 / cps).ceil() as i64
}

/// Shorten the end of lines that overlap (or come closer than `min_gap_ms` to) the next line of the
/// same track. Lines starting at the same time are left alone (parallel speakers).
pub fn fix_overlaps(sc: &mut Script, sel: &[usize], min_gap_ms: i64) -> Vec<Change> {
    let next = next_in_track(sc);
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        let Some(n) = next[i] else { continue };
        let (e, nx) = (&sc.events[i], &sc.events[n]);
        if nx.start <= e.start {
            continue;
        }
        if nx.start - e.end < min_gap_ms.max(0) || nx.start < e.end {
            let new_end = (nx.start - min_gap_ms.max(0)).max(e.start + 1);
            if new_end != e.end {
                out.push(Change {
                    index: i,
                    description: if nx.start < e.end { "fix overlapping display time" } else { "fix short gap" }.into(),
                    before: fmt_range(e.start, e.end),
                    after: fmt_range(e.start, new_end),
                });
                sc.events[i].end = new_end;
            }
        }
    }
    out
}

/// Extend lines shorter than `min_ms` up to `min_ms`, but not closer than `min_gap_ms` to the next line.
pub fn fix_short_durations(sc: &mut Script, sel: &[usize], o: &DurationOpts) -> Vec<Change> {
    let next = next_in_track(sc);
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) || visible_empty(&e.text) || e.duration() >= o.min_ms {
            continue;
        }
        let mut new_end = e.start + o.min_ms;
        if let Some(n) = next[i] {
            if sc.events[n].start > e.start {
                new_end = new_end.min(sc.events[n].start - o.min_gap_ms);
            }
        }
        if new_end > e.end {
            out.push(Change {
                index: i,
                description: "fix short display time".into(),
                before: fmt_range(e.start, e.end),
                after: fmt_range(e.start, new_end),
            });
            sc.events[i].end = new_end;
        }
    }
    out
}

/// Shorten lines longer than `max_ms` to a duration derived from their length (`target_cps`),
/// clamped to `[min_ms, max_ms]`.
pub fn fix_long_durations(sc: &mut Script, sel: &[usize], o: &DurationOpts) -> Vec<Change> {
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) || visible_empty(&e.text) || e.duration() <= o.max_ms {
            continue;
        }
        let ideal = optimal_ms(visible_chars(e), o.target_cps).clamp(o.min_ms.min(o.max_ms), o.max_ms);
        let new_end = e.start + ideal;
        if new_end < e.end {
            out.push(Change {
                index: i,
                description: "fix long display time".into(),
                before: fmt_range(e.start, e.end),
                after: fmt_range(e.start, new_end),
            });
            sc.events[i].end = new_end;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Adjust {
    /// Multiply every duration (100.0 = unchanged).
    Percent(f64),
    /// Add milliseconds (may be negative).
    Add(i64),
    /// Set every duration.
    Fixed(i64),
    /// Duration from text length at the given chars/second.
    ByCps(f64),
    /// Extend the end to the next line (minus the gap), at most `max_ms`.
    ExtendToNext,
}

/// Recalculate durations. Results are clamped to `[min_ms, max_ms]`; growing never runs into the
/// next line (`min_gap_ms` is kept), and the start never moves.
pub fn adjust_durations(sc: &mut Script, sel: &[usize], adj: Adjust, o: &DurationOpts) -> Vec<Change> {
    let next = next_in_track(sc);
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) || visible_empty(&e.text) {
            continue;
        }
        let dur = e.duration();
        let want = match adj {
            Adjust::Percent(p) => (dur as f64 * p / 100.0).round() as i64,
            Adjust::Add(d) => dur + d,
            Adjust::Fixed(d) => d,
            Adjust::ByCps(c) => optimal_ms(visible_chars(e), c),
            Adjust::ExtendToNext => match next[i] {
                Some(n) if sc.events[n].start > e.start => sc.events[n].start - o.min_gap_ms - e.start,
                _ => dur,
            },
        };
        let mut new_dur = want.clamp(o.min_ms.min(o.max_ms), o.max_ms);
        if matches!(adj, Adjust::ExtendToNext) {
            new_dur = new_dur.max(dur.min(o.max_ms));
        }
        if new_dur > dur {
            if let Some(n) = next[i] {
                if sc.events[n].start > e.start {
                    new_dur = new_dur.min((sc.events[n].start - o.min_gap_ms - e.start).max(dur));
                }
            }
        }
        if new_dur != dur {
            out.push(Change {
                index: i,
                description: "adjust duration".into(),
                before: fmt_range(e.start, e.end),
                after: fmt_range(e.start, e.start + new_dur),
            });
            sc.events[i].end = sc.events[i].start + new_dur;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpsStrategy {
    /// Extend the end time (up to the next line minus gap and `max_ms`).
    ExtendEnd,
    /// Change nothing, only report.
    SuggestOnly,
}

/// A line that reads too fast and what would fix it.
#[derive(Clone, Debug, PartialEq)]
pub struct CpsSuggestion {
    pub index: usize,
    pub cps: f64,
    /// Extra display time that would be required (after any extension already made).
    pub needed_ms: i64,
    /// Maximum number of visible characters at the current duration.
    pub max_chars: usize,
    /// How many characters would have to go to reach `max_cps`.
    pub chars_to_remove: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CpsReport {
    pub changes: Vec<Change>,
    /// Lines still above the limit afterwards (shorten the text or retime).
    pub suggestions: Vec<CpsSuggestion>,
}

pub fn fix_cps(sc: &mut Script, sel: &[usize], max_cps: f64, strategy: CpsStrategy, o: &DurationOpts) -> CpsReport {
    let next = next_in_track(sc);
    let mut rep = CpsReport::default();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) || visible_empty(&e.text) {
            continue;
        }
        let chars = visible_chars(e);
        let need = optimal_ms(chars, max_cps);
        if e.duration() >= need {
            continue;
        }
        let (s, old_end) = (e.start, e.end);
        let mut end = old_end;
        if strategy == CpsStrategy::ExtendEnd {
            let mut new_end = s + need;
            if o.max_ms > 0 {
                new_end = new_end.min(s + o.max_ms.max(e.duration()));
            }
            if let Some(n) = next[i] {
                if sc.events[n].start > s {
                    new_end = new_end.min(sc.events[n].start - o.min_gap_ms);
                }
            }
            if new_end > old_end {
                rep.changes.push(Change {
                    index: i,
                    description: "extend end to reach reading speed".into(),
                    before: fmt_range(s, old_end),
                    after: fmt_range(s, new_end),
                });
                sc.events[i].end = new_end;
                end = new_end;
            }
        }
        let dur = end - s;
        if dur < need {
            let cps = if dur > 0 { chars as f64 * 1000.0 / dur as f64 } else { f64::INFINITY };
            let max_chars = ((max_cps * dur.max(0) as f64) / 1000.0).floor() as usize;
            rep.suggestions.push(CpsSuggestion {
                index: i,
                cps,
                needed_ms: need - dur,
                max_chars,
                chars_to_remove: chars.saturating_sub(max_chars),
            });
        }
    }
    rep
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReadingStats {
    pub lines: usize,
    pub chars: usize,
    pub words: usize,
    pub duration_ms: i64,
    pub avg_cps: f64,
    pub median_cps: f64,
    pub max_cps: f64,
    pub max_cps_index: Option<usize>,
    /// Lines with `cps > threshold`.
    pub over_threshold: usize,
    /// Longest display line (chars).
    pub max_line_len: usize,
    pub avg_line_len: f64,
    pub words_per_minute: f64,
    /// Lines with zero or negative duration (not counted in the CPS statistics).
    pub invalid_duration: usize,
}

/// Reading-speed statistics over the dialogue lines (comments/typesetting excluded).
pub fn reading_stats(sc: &Script, sel: &[usize], threshold_cps: f64) -> ReadingStats {
    let mut st = ReadingStats::default();
    let mut cps_list = Vec::new();
    let (mut line_total, mut line_count) = (0usize, 0usize);
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) {
            continue;
        }
        let disp = tags::display_text(&e.text);
        if disp.trim().is_empty() {
            continue;
        }
        st.lines += 1;
        let chars = disp.chars().filter(|c| *c != '\n').count();
        st.chars += chars;
        st.words += disp.split_whitespace().count();
        for l in disp.split('\n') {
            let n = l.chars().count();
            st.max_line_len = st.max_line_len.max(n);
            line_total += n;
            line_count += 1;
        }
        if e.duration() <= 0 {
            st.invalid_duration += 1;
            continue;
        }
        st.duration_ms += e.duration();
        let c = chars as f64 * 1000.0 / e.duration() as f64;
        if c > threshold_cps {
            st.over_threshold += 1;
        }
        if c > st.max_cps {
            st.max_cps = c;
            st.max_cps_index = Some(i);
        }
        cps_list.push(c);
    }
    if st.duration_ms > 0 {
        let chars_timed: usize = targets(sc, sel)
            .iter()
            .map(|&i| &sc.events[i])
            .filter(|e| !is_protected(e) && e.duration() > 0 && !visible_empty(&e.text))
            .map(visible_chars)
            .sum();
        st.avg_cps = chars_timed as f64 * 1000.0 / st.duration_ms as f64;
        st.words_per_minute = st.words as f64 * 60_000.0 / st.duration_ms as f64;
    }
    if !cps_list.is_empty() {
        cps_list.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let m = cps_list.len() / 2;
        st.median_cps = if cps_list.len() % 2 == 1 { cps_list[m] } else { (cps_list[m - 1] + cps_list[m]) / 2.0 };
    }
    if line_count > 0 {
        st.avg_line_len = line_total as f64 / line_count as f64;
    }
    st
}

// ------------------------------------------------------------------------------------------
// Spacing
// ------------------------------------------------------------------------------------------

fn tidy_lines(s: &str) -> String {
    let collapsed = re!(r"[ \t]{2,}").replace_all(s, " ");
    collapsed
        .split('\n')
        .map(|l| {
            let l = re!(r"^([\x{E000}-\x{F8FF}]*)[ \t]+").replace(l, "$1");
            re!(r"[ \t]+([\x{E000}-\x{F8FF}]*)$").replace(&l, "$1").into_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collapse runs of spaces and trim every line.
pub fn fix_double_spaces(sc: &mut Script, sel: &[usize]) -> Vec<Change> {
    apply_text(sc, sel, "fix double spaces", &tidy_lines)
}

/// Insert missing spaces after `, ; : ! ?` and sentence-ending dots that are glued to the next word.
pub fn fix_missing_spaces(sc: &mut Script, sel: &[usize]) -> Vec<Change> {
    apply_text(sc, sel, "fix missing spaces", &|s: &str| {
        let mut t = re!(r"(\p{L}),(\p{L})").replace_all(s, "$1, $2").into_owned();
        t = re!(r"(\p{L});(\p{L})").replace_all(&t, "$1; $2").into_owned();
        t = re!(r"(\p{L}{2}):(\p{L})").replace_all(&t, "$1: $2").into_owned();
        t = re!(r"([?!])(\p{L})").replace_all(&t, "$1 $2").into_owned();
        t = re!(r"(\p{Ll}{2})([.!?])(\p{Lu})").replace_all(&t, "$1$2 $3").into_owned();
        t
    })
}

/// Remove spaces before `, . ; : ! ? …`, after `( [ ¿ ¡` and before `) ]`, and at line edges.
pub fn fix_unneeded_spaces(sc: &mut Script, sel: &[usize]) -> Vec<Change> {
    apply_text(sc, sel, "fix unneeded spaces", &|s: &str| {
        let mut t = re!(r"[ \t]+([,;:!?…])").replace_all(s, "$1").into_owned();
        t = re!(r"[ \t]+(\.)([^\d]|$)").replace_all(&t, "$1$2").into_owned();
        t = re!(r"([(\[¿¡])[ \t]+").replace_all(&t, "$1").into_owned();
        t = re!(r"[ \t]+([)\]])").replace_all(&t, "$1").into_owned();
        tidy_lines(&t)
    })
}

// ------------------------------------------------------------------------------------------
// Casing
// ------------------------------------------------------------------------------------------

/// Convert ALL-CAPS lines to sentence case (Turkish aware). `names` keep their spelling.
pub fn fix_all_caps(sc: &mut Script, sel: &[usize], lang: Lang, names: &[String]) -> Vec<Change> {
    let names = names.to_vec();
    apply_text(sc, sel, "fix all caps", &move |s: &str| {
        if is_all_caps(s, 2) {
            sentence_case(s, lang, &names)
        } else {
            s.to_string()
        }
    })
}

/// Apply a casing mode to the (selected) lines.
pub fn change_casing(sc: &mut Script, sel: &[usize], mode: CaseMode, lang: Lang, names: &[String]) -> Vec<Change> {
    let names = names.to_vec();
    let desc = match mode {
        CaseMode::Upper => "change to upper case",
        CaseMode::Lower => "change to lower case",
        CaseMode::Title => "change to title case",
        CaseMode::Sentence => "change to sentence case",
    };
    apply_text(sc, sel, desc, &move |s: &str| match mode {
        CaseMode::Upper => to_upper(s, lang),
        CaseMode::Lower => to_lower(s, lang),
        CaseMode::Title => title_case(s, lang),
        CaseMode::Sentence => sentence_case(s, lang, &names),
    })
}

const L_WORDS: &[&str] = &[
    "like", "look", "looks", "looking", "love", "long", "left", "let", "lot", "little", "life", "last", "late", "live",
    "light", "line", "leave", "learn", "lost", "lose", "listen", "laugh", "lie", "lips", "lucky", "lady", "lunch",
    "large", "later", "less", "lead", "least", "lunch", "luck", "low", "loud", "lock", "line", "lives", "lawyer",
    "lake", "land", "kill", "tell", "well", "will", "all", "call", "feel", "fall", "still", "really", "little",
];

fn at_sentence_start(prefix: &str) -> bool {
    for c in prefix.chars().rev() {
        if c.is_whitespace() || is_ph(c) || matches!(c, '"' | '\'' | '“' | '‘' | '(' | '[') || is_dash(c) {
            continue;
        }
        return matches!(c, '.' | '!' | '?' | '…');
    }
    true
}

fn fix_i_english(s: &str, names: &[String]) -> String {
    let mut out = String::new();
    let mut last = 0;
    for m in re!(r"[\p{L}'’]+").find_iter(s) {
        out.push_str(&s[last..m.start()]);
        last = m.end();
        let w = m.as_str();
        if names.iter().any(|n| n.eq_ignore_ascii_case(w)) {
            out.push_str(w);
            continue;
        }
        let chars: Vec<char> = w.chars().collect();
        let mut r = chars.clone();
        // lone "l" or "l'm" => "I"
        let lower_w = w.to_lowercase();
        if lower_w == "l" {
            let prev = s[..m.start()].trim_end().chars().last();
            if !prev.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                out.push('I');
                continue;
            }
        }
        if let Some(rest) = w.strip_prefix('l') {
            if matches!(rest, "'m" | "’m" | "'ll" | "’ll" | "'ve" | "’ve" | "'d" | "’d") {
                out.push('I');
                out.push_str(rest);
                continue;
            }
        }
        // capital I inside a word after a lowercase letter => l
        if !w.starts_with("Mc") && !w.starts_with("Mac") {
            for k in 1..r.len() {
                if r[k] == 'I' && r[k - 1].is_lowercase() {
                    r[k] = 'l';
                }
            }
        }
        // word-initial I that is really an l ("Iook")
        if r[0] == 'I' && r.len() >= 3 && r[1..].iter().all(|c| c.is_lowercase()) {
            let rest: String = r[1..].iter().collect();
            if L_WORDS.contains(&format!("l{}", rest).as_str()) {
                r[0] = if at_sentence_start(&s[..m.start()]) { 'L' } else { 'l' };
            }
        }
        out.extend(r);
    }
    out.push_str(&s[last..]);
    out
}

fn fix_i_turkish(s: &str) -> String {
    re!(r"\p{L}+")
        .replace_all(s, |c: &Captures| {
            let chars: Vec<char> = c[0].chars().collect();
            let mut r = chars.clone();
            for k in 1..r.len() {
                if r[k - 1].is_lowercase() {
                    if r[k] == 'I' {
                        r[k] = 'ı';
                    } else if r[k] == 'İ' {
                        r[k] = 'i';
                    }
                }
            }
            r.into_iter().collect::<String>()
        })
        .into_owned()
}

/// Fix `I`/`l` and Turkish `I`/`ı`/`İ`/`i` OCR-style confusion inside words.
///
/// English: capital `I` after a lowercase letter becomes `l` ("heIlo" -> "hello"), word-initial `I`
/// in common `l` words ("Iook" -> "look"/"Look"), lone `l` and `l'm`-style contractions become `I`.
/// Turkish: `I` after a lowercase letter becomes `ı`, `İ` becomes `i` ("kIz" -> "kız").
/// `names` are left alone. `Lang::Other` does nothing.
pub fn fix_i_confusion(sc: &mut Script, sel: &[usize], lang: Lang, names: &[String]) -> Vec<Change> {
    let names = names.to_vec();
    apply_text(sc, sel, "fix I/l/ı/İ confusion", &move |s: &str| match lang {
        Lang::English => fix_i_english(s, &names),
        Lang::Turkish => fix_i_turkish(s),
        Lang::Other => s.to_string(),
    })
}

// ------------------------------------------------------------------------------------------
// Dashes, ellipsis, quotes
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DashOpts {
    /// Text put in front of each speaker line.
    pub dash: String,
    /// Remove the dash from single-line cues.
    pub remove_single: bool,
}

impl Default for DashOpts {
    fn default() -> Self {
        DashOpts { dash: "- ".into(), remove_single: false }
    }
}

/// Split a line into (leading placeholders, rest).
fn split_lead_ph(line: &str) -> (&str, &str) {
    let idx = line.char_indices().find(|(_, c)| !is_ph(*c)).map(|(i, _)| i).unwrap_or(line.len());
    line.split_at(idx)
}

fn dash_line(line: &str) -> Option<(String, String)> {
    let (lead, rest) = split_lead_ph(line);
    let rest_t = rest.trim_start();
    let mut it = rest_t.chars();
    let c = it.next()?;
    if !is_dash(c) {
        return None;
    }
    if it.clone().next().map(is_dash).unwrap_or(false) {
        return None; // "--" interruption, not a speaker dash
    }
    let body = rest_t[c.len_utf8()..].trim_start();
    Some((lead.to_string(), body.to_string()))
}

fn fix_dashes_str(v: &str, o: &DashOpts) -> String {
    let lines: Vec<&str> = v.split('\n').collect();
    let mut parsed: Vec<Option<(String, String)>> = lines.iter().map(|l| dash_line(l)).collect();
    let mk = |lead: &str, body: &str| format!("{}{}{}", lead, o.dash, body);
    let ends_sentence = |l: &str| {
        let t: &str = l.trim_end_matches(|c: char| is_ph(c) || c.is_whitespace() || matches!(c, '"' | '\'' | '”' | '’' | ')'));
        t.ends_with(['.', '!', '?', '…'])
    };
    let mut new: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    if lines.len() == 1 {
        if let Some((lead, body)) = &parsed[0] {
            new[0] = if o.remove_single { format!("{}{}", lead, body) } else { mk(lead, body) };
        }
    } else {
        let n_dash = parsed.iter().filter(|p| p.is_some()).count();
        if lines.len() == 2 && n_dash == 1 {
            // two-speaker dialogue: both lines need a dash
            if parsed[0].is_some() && ends_sentence(lines[0]) {
                let (lead, rest) = split_lead_ph(lines[1]);
                parsed[1] = Some((lead.to_string(), rest.trim_start().to_string()));
            } else if parsed[1].is_some() && !parsed[0].is_some() {
                let (lead, rest) = split_lead_ph(lines[0]);
                parsed[0] = Some((lead.to_string(), rest.trim_start().to_string()));
            }
        }
        for (i, p) in parsed.iter().enumerate() {
            if let Some((lead, body)) = p {
                new[i] = mk(lead, body);
            }
        }
    }
    new.join("\n")
}

/// Normalise speaker dashes: `-Hello`/`–Hello` become `- Hello`, and when one line of a two-line
/// cue starts a dialogue both lines start with a dash.
pub fn fix_dashes(sc: &mut Script, sel: &[usize], o: &DashOpts) -> Vec<Change> {
    let o = o.clone();
    apply_text(sc, sel, "fix dialogue dashes", &move |s: &str| fix_dashes_str(s, &o))
}

fn fix_ellipsis_str(s: &str, to_char: bool) -> String {
    let mut t = re!(r"\.{3,}").replace_all(s, "…").into_owned();
    t = re!(r"(?:\. ){2,}\.").replace_all(&t, "…").into_owned();
    t = re!(r"…{2,}").replace_all(&t, "…").into_owned();
    t = re!(r"…\.+").replace_all(&t, "…").into_owned();
    // space before the ellipsis after a word is not wanted
    t = re!(r"(\p{L})[ \t]+…").replace_all(&t, "$1…").into_owned();
    // a word glued after an ellipsis that follows a word needs a space
    t = re!(r"(\p{L}…)(\p{L})").replace_all(&t, "$1 $2").into_owned();
    // no space after a line-leading ellipsis
    t = re!(r"(?m)^([\x{E000}-\x{F8FF}]*(?:[-–—][ \t]*)?)…[ \t]+").replace_all(&t, "$1…").into_owned();
    if !to_char {
        t = t.replace('…', "...");
    }
    t
}

/// Normalise ellipses: runs of dots / `. . .` / `…` become one `…` (`to_char`) or `...`, with
/// sensible spacing ("Wait ...what" -> "Wait... what").
pub fn fix_ellipsis(sc: &mut Script, sel: &[usize], to_char: bool) -> Vec<Change> {
    apply_text(sc, sel, "fix ellipsis", &move |s: &str| fix_ellipsis_str(s, to_char))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteStyle {
    Straight,
    Curly,
}

fn straighten_quotes(s: &str) -> String {
    let mut t: String = s
        .chars()
        .map(|c| match c {
            '“' | '”' | '„' | '‟' => '"',
            '‘' | '’' | '‚' | '‛' | '´' | '`' | '′' => '\'',
            c => c,
        })
        .collect();
    t = t.replace("''", "\"");
    t
}

fn curl_quotes(s: &str) -> String {
    let chars: Vec<char> = straighten_quotes(s).chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != '"' && c != '\'' {
            out.push(c);
            continue;
        }
        let prev = chars[..i].iter().rev().find(|x| !is_ph(**x)).copied();
        let next = chars[i + 1..].iter().find(|x| !is_ph(**x)).copied();
        let prev_open = prev.map(|p| p.is_whitespace() || matches!(p, '(' | '[' | '{' | '“' | '‘') || is_dash(p)).unwrap_or(true);
        let next_ok = next.map(|n| !n.is_whitespace()).unwrap_or(false);
        if c == '"' {
            out.push(if prev_open && next_ok { '“' } else { '”' });
        } else {
            let alnum_both = prev.map(|p| p.is_alphanumeric()).unwrap_or(false) && next.map(|n| n.is_alphanumeric()).unwrap_or(false);
            let rest: String = chars[i + 1..].iter().take(6).collect::<String>().to_lowercase();
            let elision = next.map(|n| n.is_ascii_digit()).unwrap_or(false)
                || ["tis", "twas", "em ", "cause", "cos ", "til ", "bout", "round"].iter().any(|e| rest.starts_with(e));
            out.push(if alnum_both || (prev_open && next_ok && elision) {
                '’'
            } else if prev_open && next_ok {
                '‘'
            } else {
                '’'
            });
        }
    }
    out
}

/// Normalise quotes and apostrophes to straight or typographic ("curly") forms.
pub fn fix_quotes(sc: &mut Script, sel: &[usize], style: QuoteStyle) -> Vec<Change> {
    apply_text(sc, sel, "fix quotes", &move |s: &str| match style {
        QuoteStyle::Straight => straighten_quotes(s),
        QuoteStyle::Curly => curl_quotes(s),
    })
}

// ------------------------------------------------------------------------------------------
// Hearing impaired / empty / duplicates
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HiOpts {
    /// Remove `[...]` and `(...)` passages.
    pub brackets: bool,
    /// Remove lines containing `♪ ♫ ♬`.
    pub music: bool,
    /// Remove uppercase speaker labels such as `JOHN:`.
    pub labels: bool,
    /// Delete cues that end up empty.
    pub delete_empty: bool,
    /// When a two-speaker cue is left with one dashed line, drop the dash.
    pub remove_orphan_dash: bool,
}

impl Default for HiOpts {
    fn default() -> Self {
        HiOpts { brackets: true, music: true, labels: true, delete_empty: true, remove_orphan_dash: true }
    }
}

fn hi_clean(v: &str, o: &HiOpts) -> String {
    let mut s = v.to_string();
    if o.brackets {
        s = re!(r"\[[^\]]*\]").replace_all(&s, keep_ph).into_owned();
        s = re!(r"\([^)]*\)").replace_all(&s, keep_ph).into_owned();
    }
    let orig_lines = s.split('\n').count();
    let mut kept: Vec<String> = Vec::new();
    let mut pending = String::new();
    for line in s.split('\n') {
        let mut l = line.to_string();
        if o.music && re!(r"[♪♫♬]").is_match(&l) {
            l = l.chars().filter(|c| is_ph(*c)).collect();
        }
        if o.labels {
            l = re!(r"^([\x{E000}-\x{F8FF}\s]*(?:[-–—]\s*)?)\p{Lu}[\p{Lu}\p{N} ._'’\-]+:[ \t]*").replace(&l, "$1").into_owned();
        }
        let l = tidy_lines(&l);
        let bare: String = l.chars().filter(|c| !is_ph(*c) && !c.is_whitespace() && !is_dash(*c)).collect();
        if bare.is_empty() {
            pending.extend(l.chars().filter(|c| is_ph(*c)));
        } else {
            kept.push(format!("{}{}", pending, l));
            pending.clear();
        }
    }
    if !pending.is_empty() {
        match kept.last_mut() {
            Some(last) => last.push_str(&pending),
            None => kept.push(pending),
        }
    }
    if o.remove_orphan_dash && orig_lines >= 2 && kept.len() == 1 {
        if let Some((lead, body)) = dash_line(&kept[0]) {
            kept[0] = format!("{}{}", lead, body);
        }
    }
    kept.join("\n")
}

/// Remove text for the hearing impaired: `[music]`, `(sighs)`, `♪` lines and `NAME:` labels.
/// Cues left empty are deleted when `delete_empty` is set (their `Change::after` is empty).
pub fn remove_hearing_impaired(sc: &mut Script, sel: &[usize], o: &HiOpts) -> Vec<Change> {
    let oc = o.clone();
    let mut out = apply_text(sc, sel, "remove text for hearing impaired", &move |s: &str| hi_clean(s, &oc));
    if o.delete_empty {
        let mut del = Vec::new();
        for c in out.iter_mut() {
            if visible_empty(&sc.events[c.index].text) {
                c.description = "removed (only hearing impaired text)".into();
                c.after = String::new();
                del.push(c.index);
            }
        }
        remove_indices(sc, &del);
    }
    out
}

fn remove_indices(sc: &mut Script, idx: &[usize]) {
    let set: std::collections::HashSet<usize> = idx.iter().copied().collect();
    let mut i = 0;
    sc.events.retain(|_| {
        let keep = !set.contains(&i);
        i += 1;
        keep
    });
}

/// Delete cues without visible text (typesetting/karaoke/comments are kept).
pub fn remove_empty_lines(sc: &mut Script, sel: &[usize]) -> Vec<Change> {
    let mut out = Vec::new();
    let mut del = Vec::new();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) || !visible_empty(&e.text) {
            continue;
        }
        out.push(Change { index: i, description: "remove empty line".into(), before: e.text.clone(), after: String::new() });
        del.push(i);
    }
    remove_indices(sc, &del);
    out
}

/// Merge neighbouring cues (same style, same text) whose gap is at most `max_gap_ms`.
pub fn merge_duplicate_lines(sc: &mut Script, sel: &[usize], max_gap_ms: i64) -> Vec<Change> {
    let tg = targets(sc, sel);
    let in_sel: std::collections::HashSet<usize> = tg.iter().copied().collect();
    let mut out = Vec::new();
    let mut del = Vec::new();
    let mut cur: Option<usize> = None;
    for &i in &tg {
        let e = &sc.events[i];
        if is_protected(e) || visible_empty(&e.text) {
            cur = None;
            continue;
        }
        if let Some(c) = cur {
            let p = &sc.events[c];
            let consecutive = i == c + 1 + del.iter().filter(|&&d| d > c && d < i).count() || in_sel.contains(&i);
            if consecutive && p.text == e.text && p.style == e.style && e.start - p.end <= max_gap_ms && e.start >= p.start {
                let new_end = p.end.max(e.end);
                out.push(Change {
                    index: c,
                    description: format!("merged duplicate of line {}", i),
                    before: fmt_range(p.start, p.end),
                    after: fmt_range(p.start, new_end),
                });
                sc.events[c].end = new_end;
                del.push(i);
                continue;
            }
        }
        cur = Some(i);
    }
    remove_indices(sc, &del);
    out
}

// ------------------------------------------------------------------------------------------
// Line breaking / splitting / merging
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BreakOpts {
    pub max_cpl: usize,
    pub max_lines: usize,
    /// Join two-line cues into one line when the whole text is at most this long (0 = off).
    pub unbreak_shorter_than: usize,
    /// Re-balance existing line breaks of cues that already fit.
    pub rebalance: bool,
}

impl Default for BreakOpts {
    fn default() -> Self {
        BreakOpts { max_cpl: 42, max_lines: 2, unbreak_shorter_than: 0, rebalance: false }
    }
}

fn word_len(w: &str) -> usize {
    vlen(w)
}

fn break_penalty(prev: &str, next: &str) -> f64 {
    let last = prev.chars().rev().find(|c| !is_ph(*c) && !matches!(c, '"' | '\'' | '”' | '’' | ')'));
    let first = next.chars().find(|c| !is_ph(*c));
    if first.map(is_dash).unwrap_or(false) && next.chars().filter(|c| !is_ph(*c)).nth(1).map(|c| c == ' ' || c.is_alphabetic()).unwrap_or(true) {
        return -40.0;
    }
    match last {
        Some('.' | '!' | '?' | '…') => -25.0,
        Some(',' | ';' | ':') => -8.0,
        _ => 0.0,
    }
}

/// Wrap `words` into exactly `n` lines of at most `max` visible chars, minimising raggedness.
fn wrap_exact(words: &[&str], n: usize, max: usize) -> Option<Vec<String>> {
    let w = words.len();
    if n == 0 || w < n {
        return None;
    }
    let lens: Vec<usize> = words.iter().map(|x| word_len(x)).collect();
    let total: usize = lens.iter().sum::<usize>() + w - 1;
    let avg = total as f64 / n as f64;
    let seg_len = |a: usize, b: usize| -> usize { lens[a..b].iter().sum::<usize>() + (b - a - 1) };
    const INF: f64 = f64::INFINITY;
    let mut cost = vec![vec![INF; w + 1]; n + 1];
    let mut from = vec![vec![0usize; w + 1]; n + 1];
    cost[0][0] = 0.0;
    for j in 1..=n {
        for i in j..=w {
            for k in (j - 1)..i {
                if cost[j - 1][k] == INF {
                    continue;
                }
                let l = seg_len(k, i);
                if l > max {
                    continue;
                }
                let d = l as f64 - avg;
                let mut c = cost[j - 1][k] + d * d;
                if j > 1 {
                    c += break_penalty(words[k - 1], words[k]);
                }
                if j == n && n == 2 && i == w {
                    // bottom-heavy tie breaker
                    let top = seg_len(0, k);
                    if top > l {
                        c += 1.0;
                    }
                }
                if c < cost[j][i] {
                    cost[j][i] = c;
                    from[j][i] = k;
                }
            }
        }
    }
    if cost[n][w] == INF {
        return None;
    }
    let mut cuts = vec![w];
    let (mut i, mut j) = (w, n);
    while j > 0 {
        i = from[j][i];
        cuts.push(i);
        j -= 1;
    }
    cuts.reverse();
    Some((0..n).map(|x| words[cuts[x]..cuts[x + 1]].join(" ")).collect())
}

fn greedy_lines(words: &[&str], max: usize) -> usize {
    let mut lines = 1;
    let mut cur = 0usize;
    for w in words {
        let l = word_len(w);
        if cur == 0 {
            cur = l;
        } else if cur + 1 + l <= max {
            cur += 1 + l;
        } else {
            lines += 1;
            cur = l;
        }
    }
    lines
}

fn flatten(v: &str) -> String {
    v.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

fn wrap_flat(flat: &str, o: &BreakOpts) -> Option<String> {
    let words: Vec<&str> = flat.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return Some(String::new());
    }
    let total = vlen(flat);
    if total <= o.max_cpl {
        return Some(flat.to_string());
    }
    let n = greedy_lines(&words, o.max_cpl).max(2);
    wrap_exact(&words, n, o.max_cpl).map(|l| l.join("\n"))
}

fn break_vis(v: &str, o: &BreakOpts) -> String {
    let lines: Vec<&str> = v.split('\n').collect();
    let dash_lines = lines.iter().filter(|l| dash_line(l).is_some()).count();
    let fits = lines.len() <= o.max_lines && lines.iter().all(|l| vlen(l) <= o.max_cpl);
    if dash_lines >= 2 {
        // keep speaker structure, wrap each speaker block on its own
        let mut blocks: Vec<Vec<&str>> = Vec::new();
        for l in &lines {
            if dash_line(l).is_some() || blocks.is_empty() {
                blocks.push(vec![l]);
            } else {
                blocks.last_mut().unwrap().push(l);
            }
        }
        let mut res = Vec::new();
        for b in blocks {
            let joined = b.join("\n");
            if b.iter().all(|l| vlen(l) <= o.max_cpl) && !o.rebalance {
                res.push(joined);
            } else {
                res.push(wrap_flat(&flatten(&joined), o).unwrap_or(joined));
            }
        }
        return res.join("\n");
    }
    if fits {
        let flat = flatten(v);
        if lines.len() > 1 && o.unbreak_shorter_than > 0 && vlen(&flat) <= o.unbreak_shorter_than && vlen(&flat) <= o.max_cpl {
            return flat;
        }
        if o.rebalance && lines.len() > 1 {
            return wrap_flat(&flat, o).unwrap_or_else(|| v.to_string());
        }
        return v.to_string();
    }
    wrap_flat(&flatten(v), o).unwrap_or_else(|| v.to_string())
}

/// Insert balanced `\N` line breaks where a line is too long or there are too many lines.
/// Cues that cannot fit `max_lines` lines are wrapped over more lines (see [`split_long_events`]).
pub fn auto_break(sc: &mut Script, sel: &[usize], o: &BreakOpts) -> Vec<Change> {
    let o = o.clone();
    apply_text(sc, sel, "balance line breaks", &move |s: &str| break_vis(s, &o))
}

#[derive(Clone, Debug, Default)]
pub struct SplitOpts {
    pub brk: BreakOpts,
    /// Gap left between the pieces.
    pub gap_ms: i64,
}

fn split_class(prefix: &str, rest: &str) -> u8 {
    let last = prefix.chars().rev().find(|c| !is_ph(*c) && !matches!(c, '"' | '\'' | '”' | '’' | ')'));
    match last {
        Some('.' | '!' | '?' | '…') => 0,
        _ if rest.trim_start().starts_with("- ") => 0,
        Some(',' | ';' | ':') => 1,
        _ => 2,
    }
}

/// Split `flat` into `k` pieces at spaces near the ideal positions. A split is *feasible* when the
/// left piece fits `max_lines` and the rest can still fit in `k - 1` pieces; among feasible splits the
/// best boundary (sentence end, then comma, then any space) wins, ties go to the most balanced one.
fn split_k(flat: &str, k: usize, o: &BreakOpts) -> Vec<String> {
    if k <= 1 {
        return vec![flat.to_string()];
    }
    let n = vlen(flat);
    let ideal = n as f64 / k as f64;
    let cap = o.max_cpl * o.max_lines;
    let mut best: Option<(usize, (u8, i64, i64))> = None;
    for (i, c) in flat.char_indices() {
        if c != ' ' {
            continue;
        }
        let (a, b) = (&flat[..i], &flat[i + 1..]);
        if vlen(a) == 0 || vlen(b) == 0 {
            continue;
        }
        let feasible = lines_needed(a, o.max_cpl) <= o.max_lines && vlen(b) <= cap * (k - 1);
        // keep pieces from becoming tiny: left must hold at least a third of the ideal share
        let sane = vlen(a) as f64 >= ideal / 3.0;
        let dist = ((vlen(a) as f64 - ideal).abs() * 100.0) as i64;
        let key = (if feasible && sane { split_class(a, b) } else { 3 + split_class(a, b) }, 0, dist);
        // among feasible splits the class dominates; otherwise only balance matters
        let key = if key.0 >= 3 { (3, 0, dist) } else { key };
        if best.map(|x| key < x.1).unwrap_or(true) {
            best = Some((i, key));
        }
    }
    match best {
        Some((i, _)) => {
            let mut v = vec![flat[..i].trim().to_string()];
            v.extend(split_k(flat[i + 1..].trim(), k - 1, o));
            v
        }
        None => vec![flat.to_string()],
    }
}

fn lines_needed(flat: &str, max_cpl: usize) -> usize {
    let words: Vec<&str> = flat.split(' ').filter(|w| !w.is_empty()).collect();
    greedy_lines(&words, max_cpl)
}

/// Tags that must be repeated on a following piece when a cue is split.
fn carry_tags(blocks: &[&str]) -> String {
    let mut state: Vec<(String, String)> = Vec::new(); // (name, tag)
    for b in blocks {
        for tag in tags::split_block_tags(b) {
            let (name, arg) = tags::tag_name_arg(&tag);
            if name.is_empty() || name == "t" {
                continue;
            }
            if name == "r" {
                state.retain(|(n, _)| matches!(n.as_str(), "an" | "a" | "pos" | "org" | "clip" | "iclip" | "q"));
                continue;
            }
            if matches!(name.as_str(), "move" | "fad" | "fade") {
                continue;
            }
            let off = matches!(name.as_str(), "i" | "b" | "u" | "s") && arg.trim() == "0";
            state.retain(|(n, _)| *n != name);
            if !off {
                state.push((name, tag));
            }
        }
    }
    state.into_iter().map(|x| x.1).collect()
}

/// Split cues that cannot fit `max_lines x max_cpl` into consecutive cues, dividing the time by
/// text length; tags in effect are repeated on the later pieces. Each piece is line-balanced.
/// `Change::index` is the original index; `after` joins the new pieces with `" || "`.
pub fn split_long_events(sc: &mut Script, sel: &[usize], o: &SplitOpts) -> Vec<Change> {
    let tg: std::collections::HashSet<usize> = targets(sc, sel).into_iter().collect();
    let old = std::mem::take(&mut sc.events);
    let mut out = Vec::new();
    let mut events = Vec::with_capacity(old.len());
    for (i, e) in old.into_iter().enumerate() {
        let pieces = if tg.contains(&i) && !is_protected(&e) { split_event_text(&e, &o.brk) } else { None };
        let Some((pieces, all_blocks)) = pieces else {
            events.push(e);
            continue;
        };
        let weights: Vec<usize> = pieces.iter().map(|p| vlen(&p.0).max(1)).collect();
        let total: usize = weights.iter().sum();
        let mut cum = 0usize;
        let mut prev_t = e.start;
        let mut texts = Vec::new();
        let n = pieces.len();
        for (k, (vis, blocks_before)) in pieces.iter().enumerate() {
            cum += weights[k];
            let t = if k + 1 == n { e.end } else { e.start + ((e.duration() as f64) * cum as f64 / total as f64).round() as i64 };
            let mut ne = e.clone();
            ne.start = prev_t;
            ne.end = if k + 1 < n && t - prev_t > o.gap_ms { t - o.gap_ms } else { t };
            let carry = carry_tags(&blocks_before.iter().map(String::as_str).collect::<Vec<_>>());
            let brk = break_vis(vis, &o.brk);
            let body = render(&brk, &all_blocks, false).unwrap_or_default();
            ne.text = if carry.is_empty() || k == 0 { body } else { format!("{{{}}}{}", carry, body) };
            texts.push(ne.text.clone());
            prev_t = t;
            events.push(ne);
        }
        out.push(Change { index: i, description: format!("split into {} lines", n), before: e.text.clone(), after: texts.join(" || ") });
    }
    sc.events = events;
    out
}

type Pieces = (Vec<(String, Vec<String>)>, Vec<String>);

/// Returns (pieces, all blocks); a piece is (visible text, blocks that appeared before it). `None`
/// when no split is needed.
fn split_event_text(e: &Event, o: &BreakOpts) -> Option<Pieces> {
    let (vis, blocks) = to_vis(&e.text)?;
    let brk = break_vis(&vis, o);
    if brk.split('\n').count() <= o.max_lines {
        return None;
    }
    let flat = flatten(&vis);
    let total = vlen(&flat);
    let mut k = total.div_ceil((o.max_cpl * o.max_lines).max(1)).max(2);
    let mut pieces = Vec::new();
    while k <= 24 {
        let p = split_k(&flat, k, o);
        if p.len() >= 2 && p.iter().all(|x| lines_needed(x, o.max_cpl) <= o.max_lines) {
            pieces = p;
            break;
        }
        if p.len() < k {
            pieces = p;
            break;
        }
        k += 1;
    }
    if pieces.len() < 2 {
        return None;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut res = Vec::new();
    for p in pieces {
        let before = seen.clone();
        for c in p.chars().filter(|c| is_ph(*c)) {
            seen.push(blocks[(c as u32 - 0xE000) as usize].clone());
        }
        res.push((p, before));
    }
    Some((res, blocks))
}

#[derive(Clone, Debug)]
pub struct MergeShortOpts {
    /// A cue is "short" when its duration is below this.
    pub short_ms: i64,
    /// Maximum gap between the two cues.
    pub max_gap_ms: i64,
    pub max_cpl: usize,
    pub max_lines: usize,
    /// Do not merge into a cue that ends a sentence.
    pub stop_at_sentence_end: bool,
}

impl Default for MergeShortOpts {
    fn default() -> Self {
        MergeShortOpts { short_ms: 1000, max_gap_ms: 250, max_cpl: 42, max_lines: 2, stop_at_sentence_end: true }
    }
}

/// Merge a short cue with the following one when they are close, same style, and the combined
/// text fits (joined by a space when it fits on one line, else a line break).
pub fn merge_short_lines(sc: &mut Script, sel: &[usize], o: &MergeShortOpts) -> Vec<Change> {
    let tg: std::collections::HashSet<usize> = targets(sc, sel).into_iter().collect();
    let mut out = Vec::new();
    let mut del = Vec::new();
    let n = sc.events.len();
    let mut i = 0;
    while i + 1 < n {
        let j = i + 1;
        let (a, b) = (&sc.events[i], &sc.events[j]);
        let ok = tg.contains(&i)
            && tg.contains(&j)
            && !is_protected(a)
            && !is_protected(b)
            && a.style == b.style
            && a.layer == b.layer
            && !visible_empty(&a.text)
            && !visible_empty(&b.text)
            && a.duration() < o.short_ms
            && b.start >= a.start
            && b.start - a.end <= o.max_gap_ms;
        if ok {
            let (da, db) = (tags::display_text(&a.text), tags::display_text(&b.text));
            let sentence_end = da.trim_end().ends_with(['.', '!', '?', '…']);
            let single = !da.contains('\n') && !db.contains('\n');
            let joined_len = da.chars().filter(|c| *c != '\n').count() + 1 + db.chars().filter(|c| *c != '\n').count();
            let (sep, lines) = if single && joined_len <= o.max_cpl {
                (" ", 1)
            } else {
                ("\\N", da.split('\n').count() + db.split('\n').count())
            };
            let line_ok = da.split('\n').chain(db.split('\n')).all(|l| l.chars().count() <= o.max_cpl);
            if !(o.stop_at_sentence_end && sentence_end) && lines <= o.max_lines && line_ok {
                let text = format!("{}{}{}", a.text, sep, b.text);
                let new_end = a.end.max(b.end);
                out.push(Change {
                    index: i,
                    description: format!("merged short line with line {}", j),
                    before: format!("{} | {}", a.text, b.text),
                    after: text.clone(),
                });
                let (s, t) = (a.start, text);
                sc.events[i].text = t;
                sc.events[i].end = new_end;
                sc.events[i].start = s;
                del.push(j);
                // allow chain merges: keep i, skip over j
                i = j;
                // re-evaluate the merged cue against the following one
                if i + 1 < n {
                    sc.events[j] = sc.events[i - 1].clone();
                }
                continue;
            }
        }
        i += 1;
    }
    remove_indices(sc, &del);
    out
}

// ------------------------------------------------------------------------------------------
// Formatting removal
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct StripOpts {
    pub italic: bool,
    pub bold: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub colors: bool,
    pub fonts: bool,
    pub positioning: bool,
    pub alignment: bool,
    pub animation: bool,
    pub karaoke: bool,
    pub borders: bool,
    /// Remove every override block (and comments in `{}`).
    pub all: bool,
}

impl StripOpts {
    /// `\i \b \u \s`
    pub fn style_tags() -> StripOpts {
        StripOpts { italic: true, bold: true, underline: true, strikeout: true, ..Default::default() }
    }
    pub fn everything() -> StripOpts {
        StripOpts { all: true, ..Default::default() }
    }
    fn names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        let mut add = |on: bool, n: &[&'static str]| {
            if on {
                v.extend_from_slice(n)
            }
        };
        add(self.italic, &["i"]);
        add(self.bold, &["b"]);
        add(self.underline, &["u"]);
        add(self.strikeout, &["s"]);
        add(self.colors, &["c", "1c", "2c", "3c", "4c", "1a", "2a", "3a", "4a", "alpha"]);
        add(self.fonts, &["fn", "fs", "fscx", "fscy", "fsp", "fe"]);
        add(self.positioning, &["pos", "move", "org", "clip", "iclip"]);
        add(self.alignment, &["an", "a"]);
        add(self.animation, &["t", "fad", "fade"]);
        add(self.karaoke, &["k", "kf", "ko", "K", "kt"]);
        add(self.borders, &["bord", "xbord", "ybord", "shad", "xshad", "yshad", "blur", "be"]);
        v
    }
}

/// Strip selected formatting tags from override blocks (empty blocks vanish).
pub fn strip_formatting(sc: &mut Script, sel: &[usize], o: &StripOpts) -> Vec<Change> {
    let names = o.names();
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if e.comment || is_typeset(e) || (!o.karaoke && !o.all && has_karaoke_tags(e)) {
            continue;
        }
        let new = if o.all {
            tags::strip_tags(&e.text)
        } else if names.is_empty() {
            continue;
        } else {
            tags::strip_tag_names(&e.text, &names)
        };
        if new != e.text {
            out.push(Change { index: i, description: "strip formatting".into(), before: e.text.clone(), after: new.clone() });
            sc.events[i].text = new;
        }
    }
    out
}

/// Shorthand: remove italic/bold/underline/strikeout tags.
pub fn remove_style_tags(sc: &mut Script, sel: &[usize]) -> Vec<Change> {
    strip_formatting(sc, sel, &StripOpts::style_tags())
}

// ------------------------------------------------------------------------------------------
// Sorting / numbering
// ------------------------------------------------------------------------------------------

/// Stable chronological sort by (start, end). Returns the old index of each event in its new
/// position (so a UI can remap its selection).
pub fn sort_by_time(sc: &mut Script) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sc.events.len()).collect();
    order.sort_by_key(|&i| (sc.events[i].start, sc.events[i].end));
    let old = std::mem::take(&mut sc.events);
    sc.events = order.iter().map(|&i| old[i].clone()).collect();
    order
}

/// 1-based chronological number (by start, then end, then position) of every event, in current
/// order. Handy for numbering SRT-style output of an unsorted script.
pub fn renumber(sc: &Script) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sc.events.len()).collect();
    order.sort_by_key(|&i| (sc.events[i].start, sc.events[i].end, i));
    let mut num = vec![0; sc.events.len()];
    for (n, i) in order.into_iter().enumerate() {
        num[i] = n + 1;
    }
    num
}

// ------------------------------------------------------------------------------------------
// Bilingual helpers
// ------------------------------------------------------------------------------------------

fn split_text_lines(text: &str) -> Vec<String> {
    text.replace("\\n", "\\N").split("\\N").map(|s| s.to_string()).collect()
}

/// Split bilingual cues (two or more display lines) into two scripts: the first half of the lines
/// in `.0`, the second half in `.1`. Single-line cues go to `.0` only. Header data and styles are cloned.
pub fn split_bilingual(sc: &Script, sel: &[usize]) -> (Script, Script) {
    let mut a = sc.clone();
    let mut b = sc.clone();
    a.events.clear();
    b.events.clear();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if e.comment {
            continue;
        }
        let lines = split_text_lines(&e.text);
        if lines.len() < 2 {
            a.events.push(e.clone());
            continue;
        }
        let half = lines.len() / 2;
        let (l1, l2) = lines.split_at(half);
        let mut ea = e.clone();
        ea.text = l1.join("\\N");
        let mut eb = e.clone();
        eb.text = l2.join("\\N");
        a.events.push(ea);
        b.events.push(eb);
    }
    (a, b)
}

/// Swap the two lines of two-line cues (e.g. put the translation on top).
pub fn swap_bilingual_lines(sc: &mut Script, sel: &[usize]) -> Vec<Change> {
    let mut out = Vec::new();
    for i in targets(sc, sel) {
        let e = &sc.events[i];
        if is_protected(e) {
            continue;
        }
        let lines = split_text_lines(&e.text);
        if lines.len() != 2 {
            continue;
        }
        let new = format!("{}\\N{}", lines[1], lines[0]);
        out.push(Change { index: i, description: "swap lines".into(), before: e.text.clone(), after: new.clone() });
        sc.events[i].text = new;
    }
    out
}

/// Combine two scripts into one bilingual script: every cue of `a` gets the text of the cue of `b`
/// with the largest time overlap appended on a second line (`a` first).
pub fn merge_bilingual(a: &Script, b: &Script) -> Script {
    let mut out = a.clone();
    for e in out.events.iter_mut() {
        if e.comment {
            continue;
        }
        let best = b
            .events
            .iter()
            .filter(|x| !x.comment)
            .map(|x| (x, (e.end.min(x.end) - e.start.max(x.start))))
            .filter(|(_, ov)| *ov > 0)
            .max_by_key(|(_, ov)| *ov);
        if let Some((x, _)) = best {
            e.text = format!("{}\\N{}", e.text, x.text);
        }
    }
    out
}

// ------------------------------------------------------------------------------------------
// Netflix-style quality checks
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct NetflixConfig {
    pub max_cpl: usize,
    pub max_lines: usize,
    /// Reading speed limit for adult programs (Netflix: 20, many languages 17).
    pub max_cps_adult: f64,
    /// Reading speed limit for children's programs (Netflix: 17).
    pub max_cps_children: f64,
    pub children: bool,
    /// Minimum duration (Netflix: 5/6 s).
    pub min_duration_ms: i64,
    pub max_duration_ms: i64,
    pub fps: f64,
    /// Minimum gap in frames (Netflix: 2).
    pub min_gap_frames: i64,
    /// Gaps shorter than this many frames should be closed to the minimum (Netflix: 12).
    pub chain_gap_frames: i64,
    /// Info issue when the top line is longer than the bottom line.
    pub check_pyramid: bool,
}

impl Default for NetflixConfig {
    fn default() -> Self {
        NetflixConfig {
            max_cpl: 42,
            max_lines: 2,
            max_cps_adult: 20.0,
            max_cps_children: 17.0,
            children: false,
            min_duration_ms: 833,
            max_duration_ms: 7000,
            fps: 24.0,
            min_gap_frames: 2,
            chain_gap_frames: 12,
            check_pyramid: false,
        }
    }
}

impl NetflixConfig {
    pub fn max_cps(&self) -> f64 {
        if self.children {
            self.max_cps_children
        } else {
            self.max_cps_adult
        }
    }
    fn frames_ms(&self, f: i64) -> i64 {
        (f as f64 * 1000.0 / self.fps).round() as i64
    }
}

/// Netflix Timed Text Style Guide style checks. Issue codes start with `nf-`.
pub fn qc_netflix(sc: &Script, c: &NetflixConfig) -> Vec<Issue> {
    let mut out = Vec::new();
    let next = next_in_track(sc);
    let min_gap = c.frames_ms(c.min_gap_frames);
    let chain_gap = c.frames_ms(c.chain_gap_frames);
    let mut push = |i: usize, sev: Severity, code: &'static str, m: String| out.push(Issue { index: i, severity: sev, code, message: m });
    for (i, e) in sc.events.iter().enumerate() {
        if is_protected(e) {
            continue;
        }
        let disp = tags::display_text(&e.text);
        if disp.trim().is_empty() {
            continue;
        }
        let lines: Vec<&str> = disp.split('\n').collect();
        if lines.len() > c.max_lines {
            push(i, Severity::Error, "nf-lines", format!("{} lines (max {})", lines.len(), c.max_lines));
        }
        if let Some(l) = lines.iter().map(|l| l.trim().chars().count()).max() {
            if l > c.max_cpl {
                push(i, Severity::Error, "nf-cpl", format!("Line of {} chars exceeds {}", l, c.max_cpl));
            }
        }
        let d = e.duration();
        if d < c.min_duration_ms {
            push(i, Severity::Warning, "nf-min-duration", format!("Duration {} ms below {} ms", d, c.min_duration_ms));
        }
        if d > c.max_duration_ms {
            push(i, Severity::Warning, "nf-max-duration", format!("Duration {} ms above {} ms", d, c.max_duration_ms));
        }
        if d > 0 {
            let chars = disp.chars().filter(|x| *x != '\n').count();
            let cps = chars as f64 * 1000.0 / d as f64;
            if cps > c.max_cps() {
                push(i, Severity::Warning, "nf-cps", format!("{:.1} characters/second exceeds {}", cps, c.max_cps()));
            }
        }
        if disp.contains("...") {
            push(i, Severity::Info, "nf-ellipsis", "Use the single ellipsis character".into());
        }
        let dashed = lines.iter().filter(|l| l.trim_start().starts_with(['-', '–', '—'])).count();
        if lines.len() == 2 && dashed == 1 {
            push(i, Severity::Warning, "nf-dialogue-dash", "Two-speaker cue: both lines need a dash".into());
        }
        if lines.len() == 2 && lines.iter().any(|l| {
            let t = l.trim_start();
            t.starts_with(['-', '–', '—']) && !t.starts_with("- ")
        }) {
            push(i, Severity::Info, "nf-dialogue-dash", "Dialogue dash should be followed by a space".into());
        }
        if c.check_pyramid && lines.len() == 2 && lines[0].trim().chars().count() > lines[1].trim().chars().count() {
            push(i, Severity::Info, "nf-pyramid", "Top line longer than bottom line".into());
        }
        if let Some(n) = next[i] {
            let nx = &sc.events[n];
            let gap = nx.start - e.end;
            if nx.start > e.start {
                if gap < 0 {
                    push(n, Severity::Error, "nf-overlap", format!("Overlaps previous line by {} ms", -gap));
                } else if gap < min_gap {
                    push(n, Severity::Warning, "nf-gap", format!("Gap {} ms below {} frames ({} ms)", gap, c.min_gap_frames, min_gap));
                } else if gap > min_gap && gap < chain_gap {
                    push(n, Severity::Info, "nf-gap-chain", format!("Gap {} ms: close to {} frames or chain lines", gap, c.min_gap_frames));
                }
            }
        }
    }
    out.sort_by_key(|i| i.index);
    out
}

// ------------------------------------------------------------------------------------------
// "Fix common errors" runner
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CommonFixes {
    pub lang: Lang,
    pub names: Vec<String>,
    pub overlaps: bool,
    pub short_durations: bool,
    pub long_durations: bool,
    pub double_spaces: bool,
    pub missing_spaces: bool,
    pub unneeded_spaces: bool,
    pub all_caps: bool,
    pub i_confusion: bool,
    pub dashes: bool,
    pub ellipsis: bool,
    pub quotes: Option<QuoteStyle>,
    pub hearing_impaired: bool,
    pub auto_break: bool,
    pub merge_duplicates: bool,
    pub empty_lines: bool,
    pub duration: DurationOpts,
    pub brk: BreakOpts,
}

impl Default for CommonFixes {
    fn default() -> Self {
        CommonFixes {
            lang: Lang::English,
            names: vec![],
            overlaps: true,
            short_durations: true,
            long_durations: false,
            double_spaces: true,
            missing_spaces: true,
            unneeded_spaces: true,
            all_caps: false,
            i_confusion: true,
            dashes: true,
            ellipsis: true,
            quotes: None,
            hearing_impaired: false,
            auto_break: false,
            merge_duplicates: false,
            empty_lines: true,
            duration: DurationOpts::default(),
            brk: BreakOpts::default(),
        }
    }
}

/// Run the enabled fixes in a sensible order. Each entry holds the step name and its changes;
/// indices are relative to the script at the start of that step (deleting steps come last).
pub fn fix_common_errors(sc: &mut Script, sel: &[usize], f: &CommonFixes) -> Vec<(&'static str, Vec<Change>)> {
    let mut res: Vec<(&'static str, Vec<Change>)> = Vec::new();
    let mut add = |name: &'static str, c: Vec<Change>| {
        if !c.is_empty() {
            res.push((name, c));
        }
    };
    if f.i_confusion {
        add("i-confusion", fix_i_confusion(sc, sel, f.lang, &f.names));
    }
    if f.all_caps {
        add("all-caps", fix_all_caps(sc, sel, f.lang, &f.names));
    }
    if f.ellipsis {
        add("ellipsis", fix_ellipsis(sc, sel, true));
    }
    if let Some(q) = f.quotes {
        add("quotes", fix_quotes(sc, sel, q));
    }
    if f.double_spaces {
        add("double-spaces", fix_double_spaces(sc, sel));
    }
    if f.unneeded_spaces {
        add("unneeded-spaces", fix_unneeded_spaces(sc, sel));
    }
    if f.missing_spaces {
        add("missing-spaces", fix_missing_spaces(sc, sel));
    }
    if f.dashes {
        add("dashes", fix_dashes(sc, sel, &DashOpts::default()));
    }
    if f.auto_break {
        add("auto-break", auto_break(sc, sel, &f.brk));
    }
    if f.long_durations {
        add("long-durations", fix_long_durations(sc, sel, &f.duration));
    }
    if f.short_durations {
        add("short-durations", fix_short_durations(sc, sel, &f.duration));
    }
    if f.overlaps {
        add("overlaps", fix_overlaps(sc, sel, f.duration.min_gap_ms));
    }
    // deleting steps last
    if f.hearing_impaired {
        add("hearing-impaired", remove_hearing_impaired(sc, sel, &HiOpts::default()));
    }
    if f.merge_duplicates {
        add("merge-duplicates", merge_duplicate_lines(sc, sel, f.duration.min_gap_ms));
    }
    if f.empty_lines {
        add("empty-lines", remove_empty_lines(sc, sel));
    }
    res
}

// ------------------------------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn script(items: &[(i64, i64, &str)]) -> Script {
        let mut sc = Script::new();
        for (s, e, t) in items {
            sc.events.push(Event::new(*s, *e, "Default", t));
        }
        sc
    }
    fn texts(sc: &Script) -> Vec<String> {
        sc.events.iter().map(|e| e.text.clone()).collect()
    }
    fn one(t: &str) -> Script {
        script(&[(0, 3000, t)])
    }

    // ---- casing
    #[test]
    fn turkish_case_mapping() {
        assert_eq!(tr_lower("İ"), "i");
        assert_eq!(tr_lower("I"), "ı");
        assert_eq!(tr_upper("i"), "İ");
        assert_eq!(tr_upper("ı"), "I");
        assert_eq!(tr_lower("İSTANBUL ISPARTA"), "istanbul ısparta");
        assert_eq!(tr_upper("istanbul ışık"), "İSTANBUL IŞIK");
        assert_eq!(to_lower("İ", Lang::English), "i");
        assert_eq!(to_lower("I", Lang::English), "i");
        assert_eq!(Lang::from_code("tr"), Lang::Turkish);
    }

    #[test]
    fn turkish_sentence_case() {
        assert_eq!(sentence_case("IŞIK", Lang::Turkish, &[]), "Işık");
        assert_eq!(sentence_case("İSTANBUL", Lang::Turkish, &[]), "İstanbul");
        assert_eq!(sentence_case("ISPARTA", Lang::Turkish, &[]), "Isparta");
        assert_eq!(sentence_case("BUGÜN HAVA ÇOK GÜZEL. İNANAMIYORUM!", Lang::Turkish, &[]), "Bugün hava çok güzel. İnanamıyorum!");
        let mut sc = one("IŞIK İÇERİ GİRDİ.");
        let ch = fix_all_caps(&mut sc, &[], Lang::Turkish, &[]);
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events[0].text, "Işık içeri girdi.");
        assert_eq!(ch[0].before, "IŞIK İÇERİ GİRDİ.");
    }

    #[test]
    fn english_all_caps_keeps_tags_and_names() {
        let mut sc = one("{\\i1}I THINK JOHN IS HERE.{\\i0}\\NI'M SURE. I KNOW");
        fix_all_caps(&mut sc, &[], Lang::English, &["John".to_string()]);
        assert_eq!(sc.events[0].text, "{\\i1}I think John is here.{\\i0}\\NI'm sure. I know");
        let mut sc = one("Already Mixed CASE");
        assert!(fix_all_caps(&mut sc, &[], Lang::English, &[]).is_empty());
    }

    #[test]
    fn sentence_case_dash_dialogue_and_ellipsis() {
        assert_eq!(sentence_case("- WHAT?\n- NOTHING... REALLY", Lang::English, &[]), "- What?\n- Nothing... Really");
        assert_eq!(sentence_case("3.5 KG OF IT", Lang::English, &[]), "3.5 kg of it");
    }

    #[test]
    fn change_casing_modes() {
        let mut sc = one("hello wORLD don't {\\b1}stop{\\b0}");
        change_casing(&mut sc, &[], CaseMode::Title, Lang::English, &[]);
        assert_eq!(sc.events[0].text, "Hello World Don't {\\b1}Stop{\\b0}");
        change_casing(&mut sc, &[], CaseMode::Upper, Lang::English, &[]);
        assert_eq!(sc.events[0].text, "HELLO WORLD DON'T {\\b1}STOP{\\b0}");
        change_casing(&mut sc, &[], CaseMode::Lower, Lang::English, &[]);
        assert_eq!(sc.events[0].text, "hello world don't {\\b1}stop{\\b0}");
        change_casing(&mut sc, &[], CaseMode::Sentence, Lang::English, &[]);
        assert_eq!(sc.events[0].text, "Hello world don't {\\b1}stop{\\b0}");
        let mut tr = one("ışık iyi");
        change_casing(&mut tr, &[], CaseMode::Upper, Lang::Turkish, &[]);
        assert_eq!(tr.events[0].text, "IŞIK İYİ");
        change_casing(&mut tr, &[], CaseMode::Title, Lang::Turkish, &[]);
        assert_eq!(tr.events[0].text, "Işık İyi");
    }

    #[test]
    fn i_confusion_english() {
        let mut sc = script(&[
            (0, 1000, "heIlo wor1d"),
            (1000, 2000, "Iook at this. Iike it"),
            (2000, 3000, "l am here and l'm fine"),
            (3000, 4000, "I'm Ian from McIntosh, IT department"),
        ]);
        let ch = fix_i_confusion(&mut sc, &[], Lang::English, &[]);
        assert_eq!(ch.len(), 3);
        let t = texts(&sc);
        assert_eq!(t[0], "hello wor1d");
        assert_eq!(t[1], "Look at this. Like it");
        assert_eq!(t[2], "I am here and I'm fine");
        assert_eq!(t[3], "I'm Ian from McIntosh, IT department");
    }

    #[test]
    fn i_confusion_turkish() {
        let mut sc = script(&[(0, 1000, "kIz ve kİtap"), (1000, 2000, "Işık İstanbul KIZ")]);
        let ch = fix_i_confusion(&mut sc, &[], Lang::Turkish, &[]);
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events[0].text, "kız ve kitap");
        assert_eq!(sc.events[1].text, "Işık İstanbul KIZ");
    }

    // ---- spacing
    #[test]
    fn spaces() {
        let mut sc = script(&[
            (0, 1000, "Hello   world  \\N  second  line "),
            (1000, 2000, "Hello,there.How are you?Fine;ok"),
            (2000, 3000, "Hello , world ! ( really ) [ yes ] ..."),
            (3000, 4000, "3.5 and 10:30 and a.b.c and www.site.com"),
        ]);
        fix_double_spaces(&mut sc, &[]);
        assert_eq!(sc.events[0].text, "Hello world\\Nsecond line");
        fix_missing_spaces(&mut sc, &[]);
        assert_eq!(sc.events[1].text, "Hello, there. How are you? Fine; ok");
        assert_eq!(sc.events[3].text, "3.5 and 10:30 and a.b.c and www.site.com");
        fix_unneeded_spaces(&mut sc, &[]);
        assert_eq!(sc.events[2].text, "Hello, world! (really) [yes]...");
    }

    #[test]
    fn spaces_keep_tags() {
        let mut sc = one("{\\i1}Hello  {\\b1}world{\\b0} {\\i0}!");
        fix_double_spaces(&mut sc, &[]);
        assert_eq!(sc.events[0].text, "{\\i1}Hello {\\b1}world{\\b0} {\\i0}!");
    }

    // ---- ellipsis / quotes / dashes
    #[test]
    fn ellipsis() {
        let mut sc = script(&[
            (0, 1, "Wait ...what.... no . . . ok"),
            (1, 2, "... and then"),
            (2, 3, "- … so"),
        ]);
        fix_ellipsis(&mut sc, &[], true);
        assert_eq!(texts(&sc), vec!["Wait… what… no… ok", "…and then", "- …so"]);
        fix_ellipsis(&mut sc, &[], false);
        assert_eq!(sc.events[0].text, "Wait... what... no... ok");
        assert_eq!(sc.events[1].text, "...and then");
    }

    #[test]
    fn quotes() {
        let mut sc = one("He said “hi” and ‘it’s’ \"fine\" don't");
        fix_quotes(&mut sc, &[], QuoteStyle::Straight);
        assert_eq!(sc.events[0].text, "He said \"hi\" and 'it's' \"fine\" don't");
        fix_quotes(&mut sc, &[], QuoteStyle::Curly);
        assert_eq!(sc.events[0].text, "He said “hi” and ‘it’s’ “fine” don’t");
        let mut sc = one("{\\i1}\"Go\"{\\i0} 'tis the '90s");
        fix_quotes(&mut sc, &[], QuoteStyle::Curly);
        assert_eq!(sc.events[0].text, "{\\i1}“Go”{\\i0} ’tis the ’90s");
    }

    #[test]
    fn dashes() {
        let o = DashOpts::default();
        let mut sc = script(&[
            (0, 1, "-Hello there.\\NHi!"),
            (1, 2, "Hello there.\\N-Hi!"),
            (2, 3, "– One\\N– Two"),
            (3, 4, "-Single"),
            (4, 5, "-Hello there\\Nstill same speaker"),
            (5, 6, "{\\i1}-Italic.{\\i0}\\N-Other"),
            (6, 7, "--Wait"),
        ]);
        fix_dashes(&mut sc, &[], &o);
        let t = texts(&sc);
        assert_eq!(t[0], "- Hello there.\\N- Hi!");
        assert_eq!(t[1], "- Hello there.\\N- Hi!");
        assert_eq!(t[2], "- One\\N- Two");
        assert_eq!(t[3], "- Single");
        assert_eq!(t[4], "- Hello there\\Nstill same speaker");
        assert_eq!(t[5], "{\\i1}- Italic.{\\i0}\\N- Other");
        assert_eq!(t[6], "--Wait");
        let o2 = DashOpts { remove_single: true, ..o };
        fix_dashes(&mut sc, &[], &o2);
        assert_eq!(sc.events[3].text, "Single");
    }

    // ---- hearing impaired
    #[test]
    fn hearing_impaired() {
        let mut sc = script(&[
            (0, 1000, "[music playing]"),
            (1000, 2000, "(sighs) I can't."),
            (2000, 3000, "JOHN: Hello!\\NMARY: Hi."),
            (3000, 4000, "- (laughs)\\N- Really?"),
            (4000, 5000, "♪ la la la ♪"),
            (5000, 6000, "{\\i1}[door slams]{\\i0} Who is it?"),
            (6000, 7000, "Meet at 10:30 sharp"),
            (7000, 8000, "{\\i1}[Music]{\\i0}"),
        ]);
        let ch = remove_hearing_impaired(&mut sc, &[], &HiOpts::default());
        let t = texts(&sc);
        assert_eq!(t, vec!["I can't.", "Hello!\\NHi.", "Really?", "{\\i1}{\\i0}Who is it?", "Meet at 10:30 sharp"]);
        assert_eq!(ch.iter().filter(|c| c.after.is_empty()).count(), 3);
        assert_eq!(ch[0].index, 0);
    }

    #[test]
    fn hearing_impaired_keep_empty_when_asked() {
        let mut sc = one("[music]");
        let o = HiOpts { delete_empty: false, ..Default::default() };
        let ch = remove_hearing_impaired(&mut sc, &[], &o);
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events.len(), 1);
        assert_eq!(sc.events[0].text, "");
        let rm = remove_empty_lines(&mut sc, &[]);
        assert_eq!(rm.len(), 1);
        assert!(sc.events.is_empty());
    }

    #[test]
    fn empty_lines_keep_typesetting() {
        let mut sc = script(&[(0, 1, ""), (1, 2, "{\\pos(1,1)}"), (2, 3, "text")]);
        sc.events[1].layer = 2;
        sc.events.push(Event { effect: "fx".into(), ..Event::new(3, 4, "Default", "") });
        let ch = remove_empty_lines(&mut sc, &[]);
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events.len(), 3);
    }

    #[test]
    fn merge_duplicates() {
        let mut sc = script(&[(0, 1000, "Same"), (1050, 2000, "Same"), (2100, 3000, "Same"), (5000, 6000, "Same"), (6000, 7000, "Other")]);
        let ch = merge_duplicate_lines(&mut sc, &[], 100);
        assert_eq!(ch.len(), 2);
        assert_eq!(sc.events.len(), 3);
        assert_eq!((sc.events[0].start, sc.events[0].end), (0, 3000));
        assert_eq!(sc.events[1].start, 5000);
    }

    // ---- timing
    #[test]
    fn overlaps_and_gaps() {
        let mut sc = script(&[(0, 2500, "a"), (2000, 4000, "b"), (4010, 5000, "c")]);
        let ch = fix_overlaps(&mut sc, &[], 50);
        assert_eq!(ch.len(), 2);
        assert_eq!(sc.events[0].end, 1950);
        assert_eq!(sc.events[1].end, 3960);
        // simultaneous lines untouched
        let mut sc = script(&[(0, 2000, "a"), (0, 2500, "b")]);
        assert!(fix_overlaps(&mut sc, &[], 50).is_empty());
        // different styles do not collide
        let mut sc = script(&[(0, 2500, "a"), (2000, 4000, "b")]);
        sc.events[1].style = "Top".into();
        assert!(fix_overlaps(&mut sc, &[], 50).is_empty());
    }

    #[test]
    fn short_and_long_durations() {
        let o = DurationOpts { min_ms: 1000, max_ms: 5000, min_gap_ms: 100, target_cps: 15.0 };
        let mut sc = script(&[(0, 300, "Hi"), (400, 600, "Yo"), (2000, 2400, "Last")]);
        let ch = fix_short_durations(&mut sc, &[], &o);
        assert_eq!(ch.len(), 2);
        assert_eq!(sc.events[0].end, 300);
        assert_eq!(sc.events[1].end, 1400);
        assert_eq!(sc.events[2].end, 3000);
        let mut sc = script(&[(0, 9000, "Short text here"), (10_000, 20_000, &"x".repeat(150))]);
        let ch = fix_long_durations(&mut sc, &[], &o);
        assert_eq!(ch.len(), 2);
        assert_eq!(sc.events[0].end, 1000); // 15 chars @15cps = 1000ms
        assert_eq!(sc.events[1].end, 15_000); // 150 chars => 10s, clamped to max 5s
    }

    #[test]
    fn adjust_durations_modes() {
        let o = DurationOpts { min_ms: 500, max_ms: 8000, min_gap_ms: 100, target_cps: 15.0 };
        let mut sc = script(&[(0, 1000, "abcdefghij"), (3000, 4000, "klm")]);
        adjust_durations(&mut sc, &[], Adjust::ExtendToNext, &o);
        assert_eq!(sc.events[0].end, 2900);
        assert_eq!(sc.events[1].end, 4000);
        let mut sc = script(&[(0, 1000, "abcdefghij"), (1500, 3000, "klm")]);
        adjust_durations(&mut sc, &[], Adjust::Percent(200.0), &o);
        assert_eq!(sc.events[0].end, 1400); // limited by next start - gap
        assert_eq!(sc.events[1].end, 4500);
        adjust_durations(&mut sc, &[], Adjust::Fixed(2000), &o);
        assert_eq!(sc.events[1].end, 3500);
        adjust_durations(&mut sc, &[0], Adjust::Add(-1000), &o);
        assert_eq!(sc.events[0].end, 500);
        adjust_durations(&mut sc, &[1], Adjust::ByCps(10.0), &o);
        assert_eq!(sc.events[1].end, 1500 + 500); // 3 chars => 300ms, min 500
    }

    #[test]
    fn cps_extend_and_suggest() {
        let o = DurationOpts { min_ms: 500, max_ms: 8000, min_gap_ms: 100, target_cps: 15.0 };
        let mut sc = script(&[(0, 1000, &"a".repeat(30)), (1500, 2500, &"b".repeat(30)), (5000, 5500, "ok")]);
        let rep = fix_cps(&mut sc, &[], 20.0, CpsStrategy::ExtendEnd, &o);
        assert_eq!(sc.events[0].end, 1400); // limited by next
        assert_eq!(rep.changes.len(), 2);
        assert_eq!(rep.suggestions.len(), 1);
        assert_eq!(rep.suggestions[0].index, 0);
        assert_eq!(rep.suggestions[0].max_chars, 28);
        assert_eq!(rep.suggestions[0].chars_to_remove, 2);
        assert_eq!(sc.events[1].end, 3000);
        let mut sc2 = script(&[(0, 1000, &"a".repeat(30))]);
        let rep = fix_cps(&mut sc2, &[], 20.0, CpsStrategy::SuggestOnly, &o);
        assert!(rep.changes.is_empty());
        assert_eq!(sc2.events[0].end, 1000);
        assert_eq!(rep.suggestions[0].needed_ms, 500);
    }

    #[test]
    fn stats() {
        let sc = script(&[(0, 1000, "0123456789"), (1000, 3000, "abcd efgh\\Nxy"), (3000, 3000, "zero")]);
        let s = reading_stats(&sc, &[], 12.0);
        assert_eq!(s.lines, 3);
        assert_eq!(s.invalid_duration, 1);
        assert_eq!(s.duration_ms, 3000);
        assert_eq!(s.max_line_len, 10);
        assert!((s.max_cps - 10.0).abs() < 1e-9);
        assert_eq!(s.max_cps_index, Some(0));
        assert_eq!(s.over_threshold, 0);
        assert!((s.avg_cps - 7.0).abs() < 1e-9); // 21 chars / 3 s
        assert!((s.median_cps - 7.75).abs() < 1e-9);
    }

    // ---- breaking and splitting
    #[test]
    fn auto_break_balances() {
        let mut sc = one("This is a rather long subtitle line that needs breaking");
        let ch = auto_break(&mut sc, &[], &BreakOpts::default());
        assert_eq!(ch.len(), 1);
        let t = &sc.events[0].text;
        let parts: Vec<&str> = t.split("\\N").collect();
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.chars().count() <= 42));
        assert!((parts[0].chars().count() as i32 - parts[1].chars().count() as i32).abs() <= 6, "{}", t);
        // short stays
        let mut sc = one("Short text");
        assert!(auto_break(&mut sc, &[], &BreakOpts::default()).is_empty());
    }

    #[test]
    fn auto_break_prefers_punctuation_and_keeps_tags() {
        let mut sc = one("{\\i1}I told you already, but you never listen to me anyway{\\i0}");
        auto_break(&mut sc, &[], &BreakOpts::default());
        let t = &sc.events[0].text;
        assert!(t.starts_with("{\\i1}") && t.ends_with("{\\i0}"));
        assert_eq!(tags::strip_tags(t).replace("\\N", " "), "I told you already, but you never listen to me anyway");
    }

    #[test]
    fn auto_break_unbreak_and_dialogue() {
        let mut sc = one("Short\\Nline");
        let o = BreakOpts { unbreak_shorter_than: 20, ..Default::default() };
        auto_break(&mut sc, &[], &o);
        assert_eq!(sc.events[0].text, "Short line");
        let mut sc = one("- Short one.\\N- Another short one.");
        assert!(auto_break(&mut sc, &[], &o).is_empty());
        let mut sc = one("- This speaker talks for far too long to fit on one line here\\N- Yes");
        auto_break(&mut sc, &[], &BreakOpts::default());
        let parts: Vec<String> = sc.events[0].text.split("\\N").map(String::from).collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2], "- Yes");
    }

    #[test]
    fn split_long_event() {
        let text = "This is the first sentence of the cue, short and sweet. This is the second sentence which is a bit longer than the first.";
        let mut sc = script(&[(0, 10_000, text), (20_000, 21_000, "tail")]);
        let ch = split_long_events(&mut sc, &[], &SplitOpts::default());
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].index, 0);
        assert!(sc.events.len() >= 3);
        let n = sc.events.len() - 1;
        assert_eq!(sc.events[0].start, 0);
        assert_eq!(sc.events[n - 1].end, 10_000);
        for w in sc.events[..n].windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
        for e in &sc.events[..n] {
            let d = tags::display_text(&e.text);
            assert!(d.split('\n').count() <= 2 && d.split('\n').all(|l| l.chars().count() <= 42), "{:?}", e.text);
        }
        assert_eq!(sc.events.len(), 3);
        assert!(tags::display_text(&sc.events[0].text).replace('\n', " ").ends_with("short and sweet."), "{}", sc.events[0].text);
        assert_eq!(sc.events[n].text, "tail");
        let joined: Vec<String> = sc.events[..n].iter().map(|e| tags::display_text(&e.text).replace('\n', " ")).collect();
        assert_eq!(joined.join(" "), text);
    }

    #[test]
    fn split_carries_tags_and_gap() {
        let text = "{\\an8\\i1}First part of a long italic sentence that goes on and on. Second part follows here and also keeps going for a while longer.{\\i0}";
        let mut sc = script(&[(0, 8000, text)]);
        let o = SplitOpts { gap_ms: 40, ..Default::default() };
        split_long_events(&mut sc, &[], &o);
        assert!(sc.events.len() >= 2);
        assert!(sc.events[0].text.starts_with("{\\an8\\i1}"));
        assert!(sc.events[1].text.starts_with("{\\an8\\i1}"), "{}", sc.events[1].text);
        assert_eq!(sc.events[1].start - sc.events[0].end, 40);
        assert_eq!(sc.events.last().unwrap().end, 8000);
    }

    #[test]
    fn split_skips_protected_and_fitting() {
        let long = "word ".repeat(40);
        let mut sc = script(&[(0, 5000, &long), (5000, 9000, "short")]);
        sc.events[0].comment = true;
        assert!(split_long_events(&mut sc, &[], &SplitOpts::default()).is_empty());
        assert_eq!(sc.events.len(), 2);
    }

    #[test]
    fn merge_short() {
        let mut sc = script(&[(0, 500, "Well,"), (600, 1500, "okay then."), (1600, 2600, "Next")]);
        let ch = merge_short_lines(&mut sc, &[], &MergeShortOpts::default());
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events.len(), 2);
        assert_eq!(sc.events[0].text, "Well, okay then.");
        assert_eq!((sc.events[0].start, sc.events[0].end), (0, 1500));
        // sentence end blocks merge
        let mut sc = script(&[(0, 500, "Stop."), (600, 1500, "Go on")]);
        assert!(merge_short_lines(&mut sc, &[], &MergeShortOpts::default()).is_empty());
        // long gap blocks merge
        let mut sc = script(&[(0, 500, "a"), (1500, 2500, "b")]);
        assert!(merge_short_lines(&mut sc, &[], &MergeShortOpts::default()).is_empty());
        // too long for two lines -> no merge
        let l = "x".repeat(40);
        let mut sc = script(&[(0, 500, &l), (600, 1500, &l)]);
        assert_eq!(merge_short_lines(&mut sc, &[], &MergeShortOpts::default()).len(), 1);
        assert_eq!(sc.events[0].text, format!("{}\\N{}", l, l));
        let l3 = "x".repeat(50);
        let mut sc = script(&[(0, 500, &l3), (600, 1500, "b")]);
        assert!(merge_short_lines(&mut sc, &[], &MergeShortOpts::default()).is_empty());
    }

    // ---- formatting / misc
    #[test]
    fn strip() {
        let mut sc = script(&[
            (0, 1, "{\\i1\\b1\\pos(1,2)}Hi{\\i0\\b0} there"),
            (1, 2, "{\\fs30\\c&HFF&}Col{\\r}or"),
            (2, 3, "{note}plain"),
            (3, 4, "{\\k20}ka{\\k30}ra"),
        ]);
        remove_style_tags(&mut sc, &[]);
        assert_eq!(sc.events[0].text, "{\\pos(1,2)}Hi there");
        let o = StripOpts { colors: true, fonts: true, ..Default::default() };
        strip_formatting(&mut sc, &[], &o);
        assert_eq!(sc.events[1].text, "Col{\\r}or");
        let ch = strip_formatting(&mut sc, &[], &StripOpts { karaoke: true, ..Default::default() });
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events[3].text, "kara");
        strip_formatting(&mut sc, &[], &StripOpts::everything());
        assert_eq!(sc.events[2].text, "plain");
        assert_eq!(sc.events[0].text, "Hi there");
    }

    #[test]
    fn protected_lines_untouched() {
        let mut sc = script(&[
            (0, 1, "{\\p1}m 0 0 l 10 10  10 0{\\p0}"),
            (1, 2, "{\\pos(5,5)}SHOUT  ,"),
            (2, 3, "{\\k20}HE{\\k20}LLO  X"),
            (3, 4, "FX  LINE"),
            (4, 5, "COMMENT  LINE"),
        ]);
        sc.events[1].layer = 3;
        sc.events[3].effect = "fx".into();
        sc.events[4].comment = true;
        let before = texts(&sc);
        fix_double_spaces(&mut sc, &[]);
        fix_all_caps(&mut sc, &[], Lang::English, &[]);
        fix_unneeded_spaces(&mut sc, &[]);
        assert_eq!(texts(&sc), before);
        assert!(remove_empty_lines(&mut sc, &[]).is_empty());
    }

    #[test]
    fn selection_limits_scope() {
        let mut sc = script(&[(0, 1, "a  b"), (1, 2, "c  d")]);
        let ch = fix_double_spaces(&mut sc, &[1, 99]);
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events[0].text, "a  b");
        assert_eq!(sc.events[1].text, "c d");
    }

    #[test]
    fn sort_and_renumber() {
        let mut sc = script(&[(3000, 4000, "c"), (0, 1000, "a"), (1000, 2000, "b")]);
        assert_eq!(renumber(&sc), vec![3, 1, 2]);
        let map = sort_by_time(&mut sc);
        assert_eq!(map, vec![1, 2, 0]);
        assert_eq!(texts(&sc), vec!["a", "b", "c"]);
    }

    #[test]
    fn bilingual() {
        let mut sc = script(&[(0, 1000, "Hello\\NMerhaba"), (1000, 2000, "Alone"), (2000, 3000, "a\\Nb\\Nc\\Nd")]);
        let (a, b) = split_bilingual(&sc, &[]);
        assert_eq!(texts(&a), vec!["Hello", "Alone", "a\\Nb"]);
        assert_eq!(texts(&b), vec!["Merhaba", "c\\Nd"]);
        assert_eq!(b.events[1].start, 2000);
        let merged = merge_bilingual(&a, &b);
        assert_eq!(merged.events[0].text, "Hello\\NMerhaba");
        assert_eq!(merged.events[1].text, "Alone");
        let ch = swap_bilingual_lines(&mut sc, &[]);
        assert_eq!(ch.len(), 1);
        assert_eq!(sc.events[0].text, "Merhaba\\NHello");
    }

    #[test]
    fn netflix_checks() {
        let long = "x".repeat(50);
        let mut sc = script(&[
            (0, 500, "Too short for that"),
            (600, 4000, &long),
            (4000, 4500, "No gap here ok ok ok ok ok"),
            (5000, 12_500, "Way too long on screen"),
            (13_000, 14_000, "- One\\NTwo\\Nthree"),
            (14_400, 15_500, "Wait..."),
            (16_000, 17_000, "- One\\NTwo"),
        ]);
        sc.events[4].text = "- One\\NTwo\\Nthree".into();
        let issues = qc_netflix(&sc, &NetflixConfig::default());
        let has = |i: usize, c: &str| issues.iter().any(|x| x.index == i && x.code == c);
        assert!(has(0, "nf-min-duration"));
        assert!(has(0, "nf-cps"));
        assert!(has(1, "nf-cpl"));
        assert!(has(2, "nf-gap"));
        assert!(has(3, "nf-max-duration"));
        assert!(has(4, "nf-lines"));
        assert!(has(6, "nf-dialogue-dash"));
        assert!(has(5, "nf-ellipsis"));
        assert!(has(5, "nf-gap-chain"));
        let child = NetflixConfig { children: true, ..Default::default() };
        assert_eq!(child.max_cps(), 17.0);
        // overlap
        let sc = script(&[(0, 2000, "a b c d e"), (1500, 3500, "f g h i j")]);
        assert!(qc_netflix(&sc, &NetflixConfig::default()).iter().any(|i| i.code == "nf-overlap" && i.index == 1));
    }

    #[test]
    fn common_errors_runner() {
        let mut sc = script(&[
            (0, 2500, "hello  world ,I am"),
            (2000, 4000, "[music]"),
            (4000, 6000, "Same"),
            (6000, 8000, "Same"),
        ]);
        let f = CommonFixes { hearing_impaired: true, merge_duplicates: true, ..Default::default() };
        let res = fix_common_errors(&mut sc, &[], &f);
        let names: Vec<&str> = res.iter().map(|r| r.0).collect();
        assert!(names.contains(&"double-spaces") && names.contains(&"hearing-impaired") && names.contains(&"merge-duplicates"));
        assert_eq!(sc.events.len(), 2);
        assert_eq!(sc.events[0].text, "hello world, I am");
        assert_eq!((sc.events[1].start, sc.events[1].end), (4000, 8000));
    }
}
