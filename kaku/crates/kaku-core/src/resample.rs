//! Resample script resolution: scale styles, line margins and override tags (like Aegisub's "Resample Resolution").

use crate::ass::{fmt_num, Entry, Script};
use crate::tags::{self, Seg, Tag};

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Scale line/style margins horizontally by rx and vertically by ry.
    pub margins: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { margins: true }
    }
}

fn nums(arg: &str) -> Option<Vec<f64>> {
    let inner = arg.trim().strip_prefix('(')?.strip_suffix(')').unwrap_or_else(|| arg.trim().trim_start_matches('(')); // tolerate missing ')'
    inner.split(',').map(|x| x.trim().parse::<f64>().ok()).collect()
}

fn paren(vals: &[f64]) -> String {
    format!("({})", vals.iter().map(|v| fmt_num(round2(*v))).collect::<Vec<_>>().join(","))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn scale_drawing(d: &str, rx: f64, ry: f64) -> String {
    // Commands (m n l b s p c) and number pairs; scale x then y alternately.
    let mut out: Vec<String> = Vec::new();
    let mut idx = 0usize;
    for tok in d.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() {
            let s = if idx % 2 == 0 { v * rx } else { v * ry };
            out.push(fmt_num(round2(s)));
            idx += 1;
        } else {
            out.push(tok.to_string());
            if tok.len() == 1 && matches!(tok, "m" | "n" | "l" | "b" | "s" | "p" | "c") {
                idx = 0;
            }
        }
    }
    out.join(" ")
}

fn scale_tag(t: &mut Tag, rx: f64, ry: f64) {
    let scalar = |arg: &str, k: f64| -> Option<String> { arg.trim().parse::<f64>().ok().map(|v| fmt_num(round2(v * k))) };
    match t.name.as_str() {
        "fs" => {
            if let Some(s) = scalar(&t.arg, ry) {
                t.arg = s;
            }
        }
        "bord" | "shad" | "yshad" | "ybord" | "blur" | "pbo" => {
            if let Some(s) = scalar(&t.arg, ry) {
                t.arg = s;
            }
        }
        "xbord" | "xshad" | "fsp" => {
            if let Some(s) = scalar(&t.arg, rx) {
                t.arg = s;
            }
        }
        "pos" | "org" => {
            if let Some(v) = nums(&t.arg).filter(|v| v.len() == 2) {
                t.arg = paren(&[v[0] * rx, v[1] * ry]);
            }
        }
        "move" => {
            if let Some(mut v) = nums(&t.arg).filter(|v| v.len() == 4 || v.len() == 6) {
                for i in 0..4 {
                    v[i] *= if i % 2 == 0 { rx } else { ry };
                }
                t.arg = paren(&v);
            }
        }
        "clip" | "iclip" => {
            let inner = t.arg.trim().trim_start_matches('(').trim_end_matches(')');
            if let Some(v) = nums(&t.arg).filter(|v| v.len() == 4) {
                t.arg = paren(&[v[0] * rx, v[1] * ry, v[2] * rx, v[3] * ry]);
            } else {
                // optional scale prefix: (scale,drawing) or (drawing)
                let (scale, draw) = match inner.split_once(',') {
                    Some((a, b)) if a.trim().parse::<f64>().is_ok() => (Some(a.trim().to_string()), b),
                    _ => (None, inner),
                };
                let d = scale_drawing(draw, rx, ry);
                t.arg = match scale {
                    Some(s) => format!("({s},{d})"),
                    None => format!("({d})"),
                };
            }
        }
        "t" => {
            // \t([t1,t2,][accel,]\tags...) – scale the nested tags.
            let inner = t.arg.trim().trim_start_matches('(').trim_end_matches(')');
            if let Some(pos) = inner.find('\\') {
                let (head, rest) = inner.split_at(pos);
                let mut nested = tags::parse_block(rest);
                for n in nested.iter_mut() {
                    scale_tag(n, rx, ry);
                }
                let body: String = nested.iter().map(|n| if n.name.is_empty() { n.arg.clone() } else { n.render() }).collect();
                t.arg = format!("({head}{body})");
            }
        }
        _ => {}
    }
}

/// Scale text of one dialogue line.
pub fn scale_text(text: &str, rx: f64, ry: f64) -> String {
    let mut segs = tags::parse(text);
    let mut drawing = false;
    for seg in segs.iter_mut() {
        match seg {
            Seg::Block(tags_) => {
                for t in tags_.iter_mut() {
                    if t.name == "p" {
                        drawing = t.arg.trim().parse::<f64>().map(|v| v > 0.0).unwrap_or(false);
                    }
                    scale_tag(t, rx, ry);
                }
            }
            Seg::Text(s) if drawing => {
                *s = scale_drawing(s, rx, ry);
            }
            _ => {}
        }
    }
    tags::render(&segs)
}

/// Resample the whole script to `(new_x, new_y)`; updates PlayRes headers. Returns false if nothing to do.
pub fn resample(script: &mut Script, new_x: i32, new_y: i32, opt: Options) -> bool {
    let (ox, oy) = script.play_res();
    if (ox, oy) == (new_x, new_y) || ox <= 0 || oy <= 0 {
        return false;
    }
    let (rx, ry) = (new_x as f64 / ox as f64, new_y as f64 / oy as f64);
    let r = |v: i32, k: f64| (v as f64 * k).round() as i32;
    for e in script.entries.iter_mut() {
        match e {
            Entry::Style(s) => {
                s.fontsize = round2(s.fontsize * ry);
                s.outline = round2(s.outline * ry);
                s.shadow = round2(s.shadow * ry);
                s.spacing = round2(s.spacing * rx);
                if opt.margins {
                    s.margin_l = r(s.margin_l, rx);
                    s.margin_r = r(s.margin_r, rx);
                    s.margin_t = r(s.margin_t, ry);
                }
            }
            Entry::Line(l) => {
                l.text = scale_text(&l.text, rx, ry);
                if opt.margins {
                    l.margin_l = r(l.margin_l, rx);
                    l.margin_r = r(l.margin_r, rx);
                    l.margin_t = r(l.margin_t, ry);
                }
            }
            _ => {}
        }
    }
    script.set_info("PlayResX", &new_x.to_string());
    script.set_info("PlayResY", &new_y.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;

    #[test]
    fn scales_tags() {
        let t = scale_text("{\\pos(100,50)\\fs40\\bord2\\fsp1\\clip(0,0,100,100)\\move(0,0,10,10,0,500)}x", 2.0, 2.0);
        assert_eq!(t, "{\\pos(200,100)\\fs80\\bord4\\fsp2\\clip(0,0,200,200)\\move(0,0,20,20,0,500)}x");
    }

    #[test]
    fn nested_t_and_drawing() {
        let t = scale_text("{\\t(0,500,\\fs20\\bord1)\\p1}m 0 0 l 10 5{\\p0}", 2.0, 3.0);
        assert_eq!(t, "{\\t(0,500,\\fs60\\bord3)\\p1}m 0 0 l 20 15{\\p0}");
    }

    #[test]
    fn whole_script() {
        let mut s = Script::new_default();
        s.set_info("PlayResX", "1280");
        s.set_info("PlayResY", "720");
        s.push_line(Line::new(0, 1000, "{\\pos(640,360)}x"));
        assert!(resample(&mut s, 1920, 1080, Options::default()));
        assert_eq!(s.play_res(), (1920, 1080));
        assert_eq!(s.lines().next().unwrap().text, "{\\pos(960,540)}x");
        let st = s.styles().next().unwrap();
        assert_eq!(st.fontsize, 72.0);
        assert!(!resample(&mut s, 1920, 1080, Options::default()));
    }
}
