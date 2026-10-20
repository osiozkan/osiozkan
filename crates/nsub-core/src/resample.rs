//! Resample script resolution (Aegisub "Resample Resolution"): scales styles, margins and the
//! geometry-bearing override tags so the script looks the same at a new PlayRes.

use crate::ass::Script;
use crate::tags::{parse_block, Segment, Tag};

#[derive(Clone, Copy, Debug)]
pub struct ResampleOptions {
    pub margins: bool,
    /// Also scale `\p` drawings' coordinates.
    pub drawings: bool,
    /// Keep aspect: use the geometric mean of x/y scale for sizes instead of just the y scale.
    pub uniform_by_y: bool,
}

impl Default for ResampleOptions {
    fn default() -> Self {
        ResampleOptions { margins: true, drawings: true, uniform_by_y: true }
    }
}

fn fmt(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        let s = format!("{:.3}", r);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn nums(s: &str) -> Option<Vec<f64>> {
    let t = s.trim().trim_start_matches('(').trim_end_matches(')');
    t.split(',').map(|x| x.trim().parse::<f64>().ok()).collect()
}

fn scale_pair(v: &str, sx: f64, sy: f64, count: usize) -> String {
    // scale the first `count` numbers as (x,y) pairs, keep trailing params (times) untouched
    let t = v.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = t.split(',').collect();
    let mut out = vec![];
    for (i, p) in parts.iter().enumerate() {
        match p.trim().parse::<f64>() {
            Ok(n) if i < count => out.push(fmt(n * if i % 2 == 0 { sx } else { sy })),
            _ => out.push(p.trim().to_string()),
        }
    }
    format!("({})", out.join(","))
}

fn scale_drawing(d: &str, sx: f64, sy: f64) -> String {
    let mut out = String::new();
    let mut toks = d.split_whitespace().peekable();
    let mut coord = 0usize;
    while let Some(t) = toks.next() {
        if !out.is_empty() {
            out.push(' ');
        }
        match t.parse::<f64>() {
            Ok(n) => {
                out.push_str(&fmt(n * if coord % 2 == 0 { sx } else { sy }));
                coord += 1;
            }
            Err(_) => {
                out.push_str(t);
                coord = 0;
            }
        }
    }
    out
}

fn scale_tag(t: &Tag, sx: f64, sy: f64, s: f64) -> Tag {
    let one = |v: &str| v.trim().parse::<f64>().map(|n| fmt(n * s)).unwrap_or_else(|_| v.to_string());
    let value = match t.name.as_str() {
        "fs" | "fsp" | "bord" | "xbord" | "ybord" | "shad" | "xshad" | "yshad" => {
            let f = match t.name.as_str() {
                "xbord" | "xshad" => sx,
                "ybord" | "yshad" => sy,
                _ => s,
            };
            t.value.trim().parse::<f64>().map(|n| fmt(n * f)).unwrap_or_else(|_| t.value.clone())
        }
        "pos" | "org" => scale_pair(&t.value, sx, sy, 2),
        "move" => scale_pair(&t.value, sx, sy, 4),
        "clip" | "iclip" => {
            if let Some(n) = nums(&t.value) {
                if n.len() == 4 {
                    scale_pair(&t.value, sx, sy, 4)
                } else {
                    t.value.clone()
                }
            } else {
                // vector clip: optional scale then drawing commands
                let inner = t.value.trim().trim_start_matches('(').trim_end_matches(')');
                match inner.split_once(',') {
                    Some((sc, d)) if sc.trim().parse::<f64>().is_ok() => format!("({},{})", sc.trim(), scale_drawing(d, sx, sy)),
                    _ => format!("({})", scale_drawing(inner, sx, sy)),
                }
            }
        }
        "t" => {
            // scale tags nested in the transform: \t(t1,t2,accel,\tags)
            let inner = t.value.trim().trim_start_matches('(').trim_end_matches(')');
            match inner.find('\\') {
                Some(p) => {
                    let head = &inner[..p];
                    let tags = parse_block(&inner[p..]);
                    let tags: String = tags.iter().map(|x| scale_tag(x, sx, sy, s).to_ass()).collect();
                    format!("({head}{tags})")
                }
                None => t.value.clone(),
            }
        }
        _ => {
            let _ = one;
            t.value.clone()
        }
    };
    Tag { name: t.name.clone(), value }
}

/// Scale the script to `new_w × new_h`. Returns the number of events changed.
pub fn resample(script: &mut Script, new_w: i32, new_h: i32, opt: ResampleOptions) -> usize {
    let (old_w, old_h) = script.resolution();
    if old_w <= 0 || old_h <= 0 || new_w <= 0 || new_h <= 0 || (old_w == new_w && old_h == new_h) {
        return 0;
    }
    let sx = new_w as f64 / old_w as f64;
    let sy = new_h as f64 / old_h as f64;
    let s = if opt.uniform_by_y { sy } else { (sx * sy).sqrt() };

    for st in &mut script.styles {
        st.fontsize = (st.fontsize * s * 100.0).round() / 100.0;
        st.outline = (st.outline * s * 100.0).round() / 100.0;
        st.shadow = (st.shadow * s * 100.0).round() / 100.0;
        st.spacing = (st.spacing * s * 100.0).round() / 100.0;
        if opt.margins {
            st.margin_l = (st.margin_l as f64 * sx).round() as i32;
            st.margin_r = (st.margin_r as f64 * sx).round() as i32;
            st.margin_t = (st.margin_t as f64 * sy).round() as i32;
            st.margin_b = st.margin_t;
        }
    }
    let mut changed = 0;
    for e in &mut script.events {
        let mut segs = crate::tags::parse(&e.text);
        let mut drawing = false;
        for seg in &mut segs {
            match seg {
                Segment::Block(tags) => {
                    for t in tags.iter_mut() {
                        if t.name == "p" {
                            drawing = t.value.trim().parse::<f64>().unwrap_or(0.0) > 0.0;
                        }
                        *t = scale_tag(t, sx, sy, s);
                    }
                }
                Segment::Text(txt) if drawing && opt.drawings => *txt = scale_drawing(txt, sx, sy),
                _ => {}
            }
        }
        let new = crate::tags::serialize(&segs);
        let mut touched = new != e.text;
        e.text = new;
        if opt.margins {
            let (l, r, v) = ((e.margin_l as f64 * sx).round() as i32, (e.margin_r as f64 * sx).round() as i32, (e.margin_t as f64 * sy).round() as i32);
            touched |= (l, r, v) != (e.margin_l, e.margin_r, e.margin_t);
            e.margin_l = l;
            e.margin_r = r;
            e.margin_t = v;
        }
        if touched {
            changed += 1;
        }
    }
    script.set_info("PlayResX", &new_w.to_string());
    script.set_info("PlayResY", &new_h.to_string());
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;

    #[test]
    fn scales_styles_tags_and_drawings() {
        let mut s = Script::new();
        s.set_info("PlayResX", "640");
        s.set_info("PlayResY", "360");
        s.styles[0].fontsize = 20.0;
        s.styles[0].margin_l = 10;
        s.events.push(Event {
            text: "{\\pos(100,50)\\fs30\\bord2\\move(0,0,640,360,0,500)\\clip(10,10,100,100)\\t(0,500,\\fs60)}x{\\p1}m 0 0 l 10 20{\\p0}".into(),
            margin_t: 8,
            ..Event::default()
        });
        let n = resample(&mut s, 1280, 720, ResampleOptions::default());
        assert_eq!(n, 1);
        assert_eq!(s.resolution(), (1280, 720));
        assert_eq!(s.styles[0].fontsize, 40.0);
        assert_eq!(s.styles[0].margin_l, 20);
        let t = &s.events[0].text;
        assert!(t.contains("\\pos(200,100)"), "{t}");
        assert!(t.contains("\\fs60"), "{t}");
        assert!(t.contains("\\bord4"), "{t}");
        assert!(t.contains("\\move(0,0,1280,720,0,500)"), "{t}");
        assert!(t.contains("\\clip(20,20,200,200)"), "{t}");
        assert!(t.contains("\\t(0,500,\\fs120)"), "{t}");
        assert!(t.contains("m 0 0 l 20 40"), "{t}");
        assert_eq!(s.events[0].margin_t, 16);
        assert_eq!(resample(&mut s, 1280, 720, ResampleOptions::default()), 0);
    }
}
