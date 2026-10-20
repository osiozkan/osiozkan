//! Helpers for visual typesetting tools: read/replace tags in a line's first override block.

use crate::model::fmt_num;
use crate::tags::{self, Part};

/// Replace (or insert) tag `name` with raw argument `arg` in the first override block (creating one if necessary).
/// `names_to_remove` lists tag names that are mutually exclusive with it (e.g. `pos` ↔ `move`).
pub fn set_tag(text: &str, name: &str, arg: &str, names_to_remove: &[&str]) -> String {
    // an extradata marker ({=1}) is not an override block
    let (ids, text) = crate::extradata::split_marker(text);
    if !ids.is_empty() { return crate::extradata::with_marker(&ids, &set_tag(text, name, arg, names_to_remove)); }
    let mut parts = tags::split_parts(text);
    let new_tag = format!("\\{}{}", name, arg);
    match parts.first_mut() {
        Some(Part::Block(b)) => {
            let mut kept: String = tags::split_block_tags(b)
                .into_iter()
                .filter(|t| {
                    let (n, _) = tags::tag_name_arg(t);
                    n != name && !names_to_remove.contains(&n.as_str())
                })
                .collect();
            kept.push_str(&new_tag);
            *b = kept;
        }
        _ => parts.insert(0, Part::Block(new_tag)),
    }
    tags::join_parts(&parts)
}

pub fn remove_tag(text: &str, name: &str) -> String {
    tags::strip_tag_names(text, &[name])
}

/// First occurrence of a tag argument anywhere in the text.
pub fn get_tag(text: &str, name: &str) -> Option<String> {
    for p in tags::split_parts(text) {
        if let Part::Block(b) = p {
            for t in tags::split_block_tags(&b) {
                let (n, a) = tags::tag_name_arg(&t);
                if n == name { return Some(a); }
            }
        }
    }
    None
}

fn parse_pair(a: &str) -> Option<(f64, f64)> {
    let v: Vec<f64> = a.trim_start_matches('(').trim_end_matches(')').split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if v.len() >= 2 { Some((v[0], v[1])) } else { None }
}

pub fn get_pos(text: &str) -> Option<(f64, f64)> {
    get_tag(text, "pos").and_then(|a| parse_pair(&a)).or_else(|| get_tag(text, "move").and_then(|a| parse_pair(&a)))
}

pub fn get_org(text: &str) -> Option<(f64, f64)> {
    get_tag(text, "org").and_then(|a| parse_pair(&a))
}

pub fn set_pos(text: &str, x: f64, y: f64) -> String {
    set_tag(text, "pos", &format!("({},{})", fmt_num(x), fmt_num(y)), &["move"])
}

pub fn set_org(text: &str, x: f64, y: f64) -> String {
    set_tag(text, "org", &format!("({},{})", fmt_num(x), fmt_num(y)), &[])
}

pub fn set_frz(text: &str, deg: f64) -> String {
    set_tag(text, "frz", &fmt_num((deg * 100.0).round() / 100.0), &["fr"])
}

pub fn set_clip_rect(text: &str, x1: f64, y1: f64, x2: f64, y2: f64, inverse: bool) -> String {
    let n = if inverse { "iclip" } else { "clip" };
    set_tag(text, n, &format!("({},{},{},{})", fmt_num(x1.min(x2)), fmt_num(y1.min(y2)), fmt_num(x1.max(x2)), fmt_num(y1.max(y2))), &["clip", "iclip"])
}

/// A numeric tag (`\frz`, `\fscx`, …) from its first occurrence.
pub fn get_num(text: &str, name: &str) -> Option<f64> {
    get_tag(text, name).and_then(|a| a.trim().trim_start_matches('(').trim_end_matches(')').trim().parse().ok())
}

/// Set a numeric tag in the first override block (rounded to 2 decimals).
pub fn set_num(text: &str, name: &str, v: f64, names_to_remove: &[&str]) -> String {
    set_tag(text, name, &fmt_num((v * 100.0).round() / 100.0), names_to_remove)
}

/// `\move(x1,y1,x2,y2[,t1,t2])`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move { pub x1: f64, pub y1: f64, pub x2: f64, pub y2: f64, pub times: Option<(i64, i64)> }

pub fn get_move(text: &str) -> Option<Move> {
    let a = get_tag(text, "move")?;
    let v: Vec<f64> = a.trim_start_matches('(').trim_end_matches(')').split(',').filter_map(|x| x.trim().parse().ok()).collect();
    match v.len() {
        4 => Some(Move { x1: v[0], y1: v[1], x2: v[2], y2: v[3], times: None }),
        n if n >= 6 => Some(Move { x1: v[0], y1: v[1], x2: v[2], y2: v[3], times: Some((v[4] as i64, v[5] as i64)) }),
        _ => None,
    }
}

/// Write `\move` (replacing `\pos`).
pub fn set_move(text: &str, m: &Move) -> String {
    let mut arg = format!("({},{},{},{}", fmt_num(m.x1), fmt_num(m.y1), fmt_num(m.x2), fmt_num(m.y2));
    if let Some((a, b)) = m.times { arg.push_str(&format!(",{},{}", a, b)); }
    arg.push(')');
    set_tag(text, "move", &arg, &["pos"])
}

/// A drawing (vector clip): commands with their points, e.g. `m 0 0 l 100 0 100 100`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing { pub cmds: Vec<(char, Vec<(f64, f64)>)> }

impl Drawing {
    pub fn parse(s: &str) -> Drawing {
        let mut cmds: Vec<(char, Vec<(f64, f64)>)> = Vec::new();
        let mut pending: Option<f64> = None;
        for tok in s.split_whitespace() {
            let c = tok.chars().next().unwrap_or(' ');
            if tok.len() == 1 && "mnlbspc".contains(c) { cmds.push((c, vec![])); pending = None; continue; }
            let Ok(v) = tok.parse::<f64>() else { continue };
            if cmds.is_empty() { cmds.push(('m', vec![])); }
            match pending.take() { Some(x) => cmds.last_mut().unwrap().1.push((x, v)), None => pending = Some(v) }
        }
        Drawing { cmds }
    }

    pub fn to_ass(&self) -> String {
        let mut o = Vec::new();
        for (c, pts) in &self.cmds {
            o.push(c.to_string());
            for (x, y) in pts { o.push(fmt_num(x.round())); o.push(fmt_num(y.round())); }
        }
        o.join(" ")
    }

    pub fn points(&self) -> Vec<(f64, f64)> { self.cmds.iter().flat_map(|(_, p)| p.iter().copied()).collect() }

    /// Point `k` in reading order.
    pub fn point_mut(&mut self, k: usize) -> Option<&mut (f64, f64)> { self.cmds.iter_mut().flat_map(|(_, p)| p.iter_mut()).nth(k) }

    /// Add a point: a move first, then straight lines.
    pub fn push_point(&mut self, x: f64, y: f64) {
        match self.cmds.last_mut() {
            None => self.cmds.push(('m', vec![(x, y)])),
            Some((c, p)) if *c == 'l' => p.push((x, y)),
            Some(_) => self.cmds.push(('l', vec![(x, y)])),
        }
    }

    /// Remove point `k`; a bezier losing a point becomes lines, empty commands go away.
    pub fn remove_point(&mut self, k: usize) {
        let mut n = 0;
        for (c, p) in self.cmds.iter_mut() {
            if k < n + p.len() { p.remove(k - n); if *c == 'b' { *c = 'l'; } break; }
            n += p.len();
        }
        self.cmds.retain(|(_, p)| !p.is_empty());
        // the shape starts with a single move; the rest of a former line command stays a line
        if let Some((c, p)) = self.cmds.first_mut() {
            let rest = if p.len() > 1 && *c != 'm' { p.split_off(1) } else { vec![] };
            let was = *c;
            *c = 'm';
            if !rest.is_empty() { self.cmds.insert(1, (if was == 'b' { 'l' } else { was }, rest)); }
        }
    }
}

/// A clip in script coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Clip {
    Rect { x1: f64, y1: f64, x2: f64, y2: f64, inverse: bool },
    Vector { drawing: Drawing, inverse: bool },
}

pub fn get_clip(text: &str) -> Option<Clip> {
    let (arg, inverse) = match get_tag(text, "clip") { Some(a) => (a, false), None => (get_tag(text, "iclip")?, true) };
    let inner = arg.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() == 4 {
        let v: Vec<f64> = parts.iter().filter_map(|x| x.parse().ok()).collect();
        if v.len() == 4 { return Some(Clip::Rect { x1: v[0], y1: v[1], x2: v[2], y2: v[3], inverse }); }
    }
    // \clip(drawing) or \clip(scale,drawing): coordinates are divided by 2^(scale-1)
    let (scale, d) = if parts.len() == 2 { (parts[0].parse::<i32>().unwrap_or(1), parts[1]) } else { (1, inner) };
    let mut drawing = Drawing::parse(d);
    let k = 2f64.powi(scale.max(1) - 1);
    if k != 1.0 { for (_, p) in drawing.cmds.iter_mut() { for q in p.iter_mut() { q.0 /= k; q.1 /= k; } } }
    Some(Clip::Vector { drawing, inverse })
}

pub fn set_clip_vector(text: &str, d: &Drawing, inverse: bool) -> String {
    let n = if inverse { "iclip" } else { "clip" };
    set_tag(text, n, &format!("({})", d.to_ass()), &["clip", "iclip"])
}

pub fn remove_clip(text: &str) -> String { tags::strip_tag_names(text, &["clip", "iclip"]) }

/// Default anchor point of a line (before \pos) for alignment `an` inside margins; script coordinates.
pub fn default_anchor(an: i32, res: (i32, i32), margin_l: i32, margin_r: i32, margin_v: i32) -> (f64, f64) {
    let (rx, ry) = (res.0 as f64, res.1 as f64);
    let x = match (an - 1) % 3 { 0 => margin_l as f64, 1 => (rx + margin_l as f64 - margin_r as f64) / 2.0, _ => rx - margin_r as f64 };
    let y = match (an - 1) / 3 { 0 => ry - margin_v as f64, 1 => ry / 2.0, _ => margin_v as f64 };
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pos_roundtrip() {
        let t = set_pos("{\\an8}Hi", 100.4, 50.0);
        assert_eq!(t, "{\\an8\\pos(100.4,50)}Hi");
        assert_eq!(get_pos(&t), Some((100.4, 50.0)));
        let t2 = set_pos(&t, 1.0, 2.0);
        assert_eq!(t2, "{\\an8\\pos(1,2)}Hi");
        assert_eq!(set_pos("Hi", 3.0, 4.0), "{\\pos(3,4)}Hi");
    }
    #[test]
    fn extradata_marker_is_left_alone() {
        assert_eq!(set_pos("{=2}Hi", 3.0, 4.0), "{=2}{\\pos(3,4)}Hi");
        assert_eq!(set_pos("{=2}{\\b1}Hi", 3.0, 4.0), "{=2}{\\b1\\pos(3,4)}Hi");
    }
    #[test]
    fn move_replaced() {
        assert_eq!(set_pos("{\\move(0,0,5,5)}x", 1.0, 1.0), "{\\pos(1,1)}x");
    }
    #[test]
    fn clip_and_frz() {
        assert_eq!(set_clip_rect("x", 10.0, 20.0, 5.0, 2.0, false), "{\\clip(5,2,10,20)}x");
        assert_eq!(set_frz("{\\frz10}x", 33.333), "{\\frz33.33}x");
    }
    #[test]
    fn numbers_and_move() {
        let t = set_num("{\\an7\\frz10}x", "frz", 12.3456, &["fr"]);
        assert_eq!(t, "{\\an7\\frz12.35}x");
        assert_eq!(get_num(&t, "frz"), Some(12.35));
        assert_eq!(get_num("{\\fscx120}x", "fscy"), None);
        let m = Move { x1: 10.0, y1: 20.0, x2: 30.5, y2: 40.0, times: Some((0, 500)) };
        let t = set_move("{\\pos(1,2)\\b1}x", &m);
        assert_eq!(t, "{\\b1\\move(10,20,30.5,40,0,500)}x");
        assert_eq!(get_move(&t), Some(m));
        assert_eq!(get_move("{\\move(1,2,3,4)}"), Some(Move { x1: 1.0, y1: 2.0, x2: 3.0, y2: 4.0, times: None }));
        // \pos reads the start of a \move
        assert_eq!(get_pos(&t), Some((10.0, 20.0)));
    }
    #[test]
    fn vector_clips() {
        let mut d = Drawing::parse("m 0 0 l 100 0 100 100 b 50 150 0 150 0 100");
        assert_eq!(d.points().len(), 6);
        *d.point_mut(2).unwrap() = (120.4, 99.6);
        assert_eq!(d.to_ass(), "m 0 0 l 100 0 120 100 b 50 150 0 150 0 100");
        d.remove_point(4);
        assert_eq!(d.to_ass(), "m 0 0 l 100 0 120 100 l 50 150 0 100");
        d.remove_point(0);
        assert_eq!(d.to_ass(), "m 100 0 l 120 100 l 50 150 0 100");
        let mut e = Drawing::default();
        e.push_point(1.0, 2.0);
        e.push_point(3.0, 4.0);
        e.push_point(5.0, 6.0);
        assert_eq!(e.to_ass(), "m 1 2 l 3 4 5 6");
        let t = set_clip_vector("{\\clip(1,2,3,4)}x", &e, true);
        assert_eq!(t, "{\\iclip(m 1 2 l 3 4 5 6)}x");
        assert_eq!(get_clip(&t), Some(Clip::Vector { drawing: e.clone(), inverse: true }));
        assert_eq!(get_clip("{\\clip(1,2,30,40)}x"), Some(Clip::Rect { x1: 1.0, y1: 2.0, x2: 30.0, y2: 40.0, inverse: false }));
        // scaled drawings are read in script coordinates
        match get_clip("{\\clip(3,m 40 40 l 80 40)}") { Some(Clip::Vector { drawing, .. }) => assert_eq!(drawing.points(), vec![(10.0, 10.0), (20.0, 10.0)]), c => panic!("{:?}", c) }
        assert_eq!(remove_clip("{\\b1\\iclip(m 1 2 l 3 4)}x"), "{\\b1}x");
    }
    #[test]
    fn anchors() {
        assert_eq!(default_anchor(2, (1920, 1080), 10, 10, 20), (960.0, 1060.0));
        assert_eq!(default_anchor(7, (1920, 1080), 10, 10, 20), (10.0, 20.0));
    }
}
