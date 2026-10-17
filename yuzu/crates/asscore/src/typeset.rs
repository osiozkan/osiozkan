//! Helpers for visual typesetting tools: read/replace tags in a line's first override block.

use crate::model::fmt_num;
use crate::tags::{self, Part};

/// Replace (or insert) tag `name` with raw argument `arg` in the first override block (creating one if necessary).
/// `names_to_remove` lists tag names that are mutually exclusive with it (e.g. `pos` ↔ `move`).
pub fn set_tag(text: &str, name: &str, arg: &str, names_to_remove: &[&str]) -> String {
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
    fn move_replaced() {
        assert_eq!(set_pos("{\\move(0,0,5,5)}x", 1.0, 1.0), "{\\pos(1,1)}x");
    }
    #[test]
    fn clip_and_frz() {
        assert_eq!(set_clip_rect("x", 10.0, 20.0, 5.0, 2.0, false), "{\\clip(5,2,10,20)}x");
        assert_eq!(set_frz("{\\frz10}x", 33.333), "{\\frz33.33}x");
    }
    #[test]
    fn anchors() {
        assert_eq!(default_anchor(2, (1920, 1080), 10, 10, 20), (960.0, 1060.0));
        assert_eq!(default_anchor(7, (1920, 1080), 10, 10, 20), (10.0, 20.0));
    }
}
