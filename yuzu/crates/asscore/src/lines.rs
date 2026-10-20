//! Line-level editing as in Aegisub: clipboard rows, paste over chosen fields, Select Lines.

use crate::model::*;
use crate::tags;
use regex::Regex;
use std::collections::BTreeSet;

/// Selected lines as Aegisub puts them on the clipboard: one `Dialogue:`/`Comment:` row each.
pub fn to_clipboard(sc: &Script, idx: &[usize]) -> String {
    idx.iter().filter_map(|&i| sc.events.get(i)).map(|e| e.to_ass()).collect::<Vec<_>>().join("\n")
}

/// Clipboard text → lines. ASS rows are read as they are; any other non-empty line becomes a
/// line with that text and zero times (Aegisub does the same).
pub fn from_clipboard(text: &str) -> Vec<Event> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| crate::ass::parse_event_line(l).unwrap_or_else(|| Event::new(0, 0, "Default", l)))
        .collect()
}

/// Fields "Paste over" can overwrite (Aegisub's list, same order).
pub mod field {
    pub const COMMENT: u16 = 1;
    pub const LAYER: u16 = 1 << 1;
    pub const START: u16 = 1 << 2;
    pub const END: u16 = 1 << 3;
    pub const STYLE: u16 = 1 << 4;
    pub const ACTOR: u16 = 1 << 5;
    pub const MARGIN_L: u16 = 1 << 6;
    pub const MARGIN_R: u16 = 1 << 7;
    pub const MARGIN_V: u16 = 1 << 8;
    pub const EFFECT: u16 = 1 << 9;
    pub const TEXT: u16 = 1 << 10;
    pub const ALL: [(u16, &str); 11] = [
        (COMMENT, "Comment"), (LAYER, "Layer"), (START, "Start time"), (END, "End time"), (STYLE, "Style"), (ACTOR, "Actor"),
        (MARGIN_L, "Left margin"), (MARGIN_R, "Right margin"), (MARGIN_V, "Vertical margin"), (EFFECT, "Effect"), (TEXT, "Text"),
    ];
}

/// Copy the chosen fields of `src` onto `dst`.
pub fn paste_over(dst: &mut Event, src: &Event, fields: u16) {
    use field::*;
    if fields & COMMENT != 0 { dst.comment = src.comment; }
    if fields & LAYER != 0 { dst.layer = src.layer; }
    if fields & START != 0 { dst.start = src.start; }
    if fields & END != 0 { dst.end = src.end; }
    if fields & STYLE != 0 { dst.style = src.style.clone(); }
    if fields & ACTOR != 0 { dst.actor = src.actor.clone(); }
    if fields & MARGIN_L != 0 { dst.margin_l = src.margin_l; }
    if fields & MARGIN_R != 0 { dst.margin_r = src.margin_r; }
    if fields & MARGIN_V != 0 { dst.margin_v = src.margin_v; }
    if fields & EFFECT != 0 { dst.effect = src.effect.clone(); }
    if fields & TEXT != 0 { dst.text = src.text.clone(); }
}

/// Rows "Paste over" writes to (Aegisub): with one line selected, downwards from the active
/// line; with several, the selected lines in order.
pub fn paste_over_targets(sc: &Script, selection: &BTreeSet<usize>, active: usize) -> Vec<usize> {
    if selection.len() < 2 { (active..sc.events.len()).collect() } else { selection.iter().copied().filter(|&i| i < sc.events.len()).collect() }
}

/// Paste `lines` over `rows` (one each, in order); returns the rows changed.
pub fn paste_over_rows(sc: &mut Script, rows: &[usize], lines: &[Event], fields: u16) -> Vec<usize> {
    let mut done = Vec::new();
    for (&r, src) in rows.iter().zip(lines) {
        let Some(dst) = sc.events.get_mut(r) else { break };
        paste_over(dst, src, fields);
        done.push(r);
    }
    done
}

// ---------------------------------------------------------------- Select Lines

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMode { Contains, Exact, Regex }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectAction { Set, Add, Subtract, Intersect }

#[derive(Clone, Debug)]
pub struct SelectOpts {
    pub pattern: String,
    /// 0 text, 1 style, 2 actor, 3 effect, 4 text without tags
    pub field: u8,
    pub mode: MatchMode,
    pub case_sensitive: bool,
    /// select the lines that do NOT match
    pub invert: bool,
    pub dialogue: bool,
    pub comments: bool,
}

impl Default for SelectOpts {
    fn default() -> Self {
        SelectOpts { pattern: String::new(), field: 0, mode: MatchMode::Contains, case_sensitive: false, invert: false, dialogue: true, comments: false }
    }
}

/// A pattern for case-insensitive plain matching that also treats Turkish i/İ/ı/I alike
/// (Unicode simple case folding keeps them apart).
pub fn plain_pattern(text: &str, case_sensitive: bool) -> String {
    if case_sensitive { return regex::escape(text); }
    let mut p = String::from("(?i)");
    for c in text.chars() {
        match c {
            'i' | 'I' | 'ı' | 'İ' => p.push_str("[iIıİ]"),
            c => p.push_str(&regex::escape(&c.to_string())),
        }
    }
    p
}

fn field_text(e: &Event, f: u8) -> String {
    match f { 1 => e.style.clone(), 2 => e.actor.clone(), 3 => e.effect.clone(), 4 => tags::display_text(&e.text), _ => e.text.clone() }
}

/// Lines matching the options (Aegisub's Select Lines).
pub fn select_lines(sc: &Script, o: &SelectOpts) -> Result<Vec<usize>, String> {
    let re = match o.mode {
        MatchMode::Regex => Some(Regex::new(&if o.case_sensitive { o.pattern.clone() } else { format!("(?i){}", o.pattern) }).map_err(|e| e.to_string())?),
        MatchMode::Contains => Some(Regex::new(&plain_pattern(&o.pattern, o.case_sensitive)).map_err(|e| e.to_string())?),
        MatchMode::Exact => Some(Regex::new(&format!("^(?:{})$", plain_pattern(&o.pattern, o.case_sensitive))).map_err(|e| e.to_string())?),
    };
    Ok(sc.events.iter().enumerate().filter(|(_, e)| {
        if (e.comment && !o.comments) || (!e.comment && !o.dialogue) { return false; }
        let hay = field_text(e, o.field);
        let m = re.as_ref().map(|r| r.is_match(&hay)).unwrap_or(false);
        m != o.invert
    }).map(|(i, _)| i).collect())
}

/// Combine a match with the current selection.
pub fn combine(current: &BTreeSet<usize>, matched: &[usize], action: SelectAction) -> BTreeSet<usize> {
    let m: BTreeSet<usize> = matched.iter().copied().collect();
    match action {
        SelectAction::Set => m,
        SelectAction::Add => current.union(&m).copied().collect(),
        SelectAction::Subtract => current.difference(&m).copied().collect(),
        SelectAction::Intersect => current.intersection(&m).copied().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Script {
        let mut sc = Script::new();
        sc.events = vec![
            Event::new(0, 1000, "Default", "{\\i1}Işık{\\i0} geldi"),
            Event { actor: "Ali".into(), ..Event::new(1000, 2000, "Sign", "IŞIK") },
            Event { comment: true, ..Event::new(2000, 3000, "Default", "not: ışık") },
            Event::new(3000, 4000, "Default", "karanlık"),
        ];
        sc
    }

    #[test]
    fn clipboard_round_trip_and_plain_text() {
        let sc = doc();
        let clip = to_clipboard(&sc, &[1, 2]);
        assert_eq!(clip.lines().count(), 2);
        let back = from_clipboard(&clip);
        assert_eq!(back, vec![sc.events[1].clone(), sc.events[2].clone()]);
        let plain = from_clipboard("Merhaba\r\n\r\nDünya, nasılsın?\n");
        assert_eq!(plain.len(), 2);
        assert_eq!(plain[1].text, "Dünya, nasılsın?");
        assert_eq!((plain[1].start, plain[1].end), (0, 0));
    }

    #[test]
    fn paste_over_only_chosen_fields() {
        let mut sc = doc();
        let src = from_clipboard("Bir\nİki\nÜç\nDört\nBeş");
        let one: BTreeSet<usize> = [2].into_iter().collect();
        let targets = paste_over_targets(&sc, &one, 2);
        let rows = paste_over_rows(&mut sc, &targets, &src, field::TEXT);
        assert_eq!(rows, vec![2, 3]);
        let many: BTreeSet<usize> = [0, 3].into_iter().collect();
        assert_eq!(paste_over_targets(&sc, &many, 3), vec![0, 3]);
        assert_eq!(sc.events[2].text, "Bir");
        assert_eq!(sc.events[3].text, "İki");
        assert_eq!(sc.events[3].start, 3000);
        assert!(sc.events[2].comment);
        let mut e = Event::new(5, 6, "A", "x");
        paste_over(&mut e, &Event { layer: 3, actor: "B".into(), ..Event::new(7, 8, "C", "y") }, field::LAYER | field::START | field::STYLE);
        assert_eq!((e.layer, e.start, e.end, e.style.as_str(), e.actor.as_str(), e.text.as_str()), (3, 7, 6, "C", "", "x"));
    }

    #[test]
    fn select_modes_and_turkish_case() {
        let sc = doc();
        let mut o = SelectOpts { pattern: "ışık".into(), ..Default::default() };
        assert_eq!(select_lines(&sc, &o).unwrap(), vec![0, 1]);
        o.comments = true;
        assert_eq!(select_lines(&sc, &o).unwrap(), vec![0, 1, 2]);
        o.case_sensitive = true;
        assert_eq!(select_lines(&sc, &o).unwrap(), vec![2]);
        o = SelectOpts { pattern: "sign".into(), field: 1, mode: MatchMode::Exact, ..Default::default() };
        assert_eq!(select_lines(&sc, &o).unwrap(), vec![1]);
        o.invert = true;
        assert_eq!(select_lines(&sc, &o).unwrap(), vec![0, 3]);
        o = SelectOpts { pattern: r"^\{".into(), mode: MatchMode::Regex, ..Default::default() };
        assert_eq!(select_lines(&sc, &o).unwrap(), vec![0]);
        o.field = 4;
        assert!(select_lines(&sc, &o).unwrap().is_empty());
        o.pattern = "(".into();
        assert!(select_lines(&sc, &o).is_err());
        let cur: BTreeSet<usize> = [0, 3].into_iter().collect();
        assert_eq!(combine(&cur, &[1, 3], SelectAction::Add).into_iter().collect::<Vec<_>>(), vec![0, 1, 3]);
        assert_eq!(combine(&cur, &[1, 3], SelectAction::Subtract).into_iter().collect::<Vec<_>>(), vec![0]);
        assert_eq!(combine(&cur, &[1, 3], SelectAction::Intersect).into_iter().collect::<Vec<_>>(), vec![3]);
        assert_eq!(combine(&cur, &[1, 3], SelectAction::Set).into_iter().collect::<Vec<_>>(), vec![1, 3]);
    }
}
