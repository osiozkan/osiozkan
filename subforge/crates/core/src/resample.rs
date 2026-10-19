//! Resample script resolution: scale styles, margins and positioning tags
//! (what Aegisub's "Resample Resolution" does).
use crate::ass::Script;
use crate::tags::{self, Item, Segment};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resample {
    pub new_x: i32,
    pub new_y: i32,
    /// Also scale `\fs`, `\bord`, `\shad`, `\fsp` in override tags (and style sizes).
    pub scale_sizes: bool,
    /// Shift horizontally by (new aspect − old aspect) so 4:3 content stays centered on 16:9 etc.
    pub aspect_shift: bool,
}

fn fmt(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        let s = format!("{r:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn split_args(v: &str) -> Option<Vec<&str>> {
    let v = v.trim();
    let inner = v.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner.split(',').map(|s| s.trim()).collect())
}

struct Scale {
    sx: f64,
    sy: f64,
    s: f64, // uniform scale for sizes
    dx: f64,
    sizes: bool,
}

impl Scale {
    fn x(&self, v: f64) -> f64 {
        v * self.sx + self.dx
    }
    fn y(&self, v: f64) -> f64 {
        v * self.sy
    }
}

fn scale_tag(name: &str, value: &str, sc: &Scale) -> String {
    let num = |s: &str| s.trim().parse::<f64>().ok();
    match name {
        "pos" | "org" => {
            if let Some(a) = split_args(value) {
                if a.len() == 2 {
                    if let (Some(x), Some(y)) = (num(a[0]), num(a[1])) {
                        return format!("({},{})", fmt(sc.x(x)), fmt(sc.y(y)));
                    }
                }
            }
            value.to_string()
        }
        "move" => {
            if let Some(a) = split_args(value) {
                if a.len() >= 4 {
                    let n: Vec<Option<f64>> = a.iter().map(|s| num(s)).collect();
                    if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (n[0], n[1], n[2], n[3]) {
                        let mut out = vec![fmt(sc.x(x1)), fmt(sc.y(y1)), fmt(sc.x(x2)), fmt(sc.y(y2))];
                        out.extend(a[4..].iter().map(|s| s.to_string()));
                        return format!("({})", out.join(","));
                    }
                }
            }
            value.to_string()
        }
        "clip" | "iclip" => {
            if let Some(a) = split_args(value) {
                // rectangle form: x1,y1,x2,y2 (optionally preceded by nothing)
                if a.len() == 4 {
                    let n: Vec<Option<f64>> = a.iter().map(|s| num(s)).collect();
                    if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (n[0], n[1], n[2], n[3]) {
                        return format!("({},{},{},{})", fmt(sc.x(x1)), fmt(sc.y(y1)), fmt(sc.x(x2)), fmt(sc.y(y2)));
                    }
                }
                // vector form: [scale,] drawing commands
                let inner = &value.trim()[1..value.trim().len() - 1];
                return format!("({})", scale_drawing(inner, sc));
            }
            value.to_string()
        }
        "fs" if sc.sizes => num(value).map(|v| fmt(v * sc.s)).unwrap_or_else(|| value.to_string()),
        "fsp" | "bord" | "xbord" | "ybord" | "shad" | "xshad" | "yshad" if sc.sizes => {
            let f = if name == "xbord" || name == "xshad" { sc.sx } else if name == "ybord" || name == "yshad" { sc.sy } else { sc.s };
            num(value).map(|v| fmt(v * f)).unwrap_or_else(|| value.to_string())
        }
        "t" => {
            // \t(t1,t2,accel,tags) — scale the nested tags
            if let Some(inner) = value.trim().strip_prefix('(').and_then(|v| v.strip_suffix(')')) {
                if let Some(bs) = inner.find('\\') {
                    let (head, tail) = inner.split_at(bs);
                    let nested = tags::parse(&format!("{{{tail}}}"));
                    let scaled = scale_segments(nested, sc);
                    let body = tags::to_string(&scaled);
                    let body = &body[1..body.len() - 1];
                    return format!("({head}{body})");
                }
            }
            value.to_string()
        }
        _ => value.to_string(),
    }
}

fn scale_drawing(cmds: &str, sc: &Scale) -> String {
    // tokens: letters are commands, numbers alternate x y
    let mut out = Vec::new();
    let mut axis = 0;
    for tok in cmds.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() {
            out.push(fmt(if axis % 2 == 0 { sc.x(v) } else { sc.y(v) }));
            axis += 1;
        } else {
            out.push(tok.to_string());
            axis = 0;
        }
    }
    out.join(" ")
}

fn scale_segments(segs: Vec<Segment>, sc: &Scale) -> Vec<Segment> {
    segs.into_iter()
        .map(|seg| match seg {
            Segment::Block(items) => Segment::Block(
                items
                    .into_iter()
                    .map(|it| match it {
                        Item::Tag { name, value } => {
                            let v = scale_tag(&name, &value, sc);
                            Item::Tag { name, value: v }
                        }
                        c => c,
                    })
                    .collect(),
            ),
            t => t,
        })
        .collect()
}

/// Apply the resample in place. Returns the number of events touched.
pub fn resample(script: &mut Script, r: Resample) -> usize {
    let (ox, oy) = script.play_res();
    if ox <= 0 || oy <= 0 || r.new_x <= 0 || r.new_y <= 0 {
        return 0;
    }
    let sy = r.new_y as f64 / oy as f64;
    let sx = if r.aspect_shift { sy } else { r.new_x as f64 / ox as f64 };
    let dx = if r.aspect_shift { (r.new_x as f64 - ox as f64 * sy) / 2.0 } else { 0.0 };
    let sc = Scale { sx, sy, s: sy, dx, sizes: r.scale_sizes };
    let iscale = |v: i32, f: f64| (v as f64 * f).round() as i32;
    for st in script.styles.iter_mut() {
        if r.scale_sizes {
            st.fontsize = (st.fontsize * sc.s * 1000.0).round() / 1000.0;
            st.outline = (st.outline * sc.s * 1000.0).round() / 1000.0;
            st.shadow = (st.shadow * sc.s * 1000.0).round() / 1000.0;
            st.spacing = (st.spacing * sc.s * 1000.0).round() / 1000.0;
        }
        st.margin_l = iscale(st.margin_l, sx);
        st.margin_r = iscale(st.margin_r, sx);
        st.margin_t = iscale(st.margin_t, sy);
        st.margin_b = iscale(st.margin_b, sy);
    }
    let mut n = 0;
    for e in script.events.iter_mut() {
        e.margin_l = iscale(e.margin_l, sx);
        e.margin_r = iscale(e.margin_r, sx);
        e.margin_t = iscale(e.margin_t, sy);
        e.margin_b = iscale(e.margin_b, sy);
        if e.text.contains('\\') {
            let segs = scale_segments(tags::parse(&e.text), &sc);
            let new = tags::to_string(&segs);
            if new != e.text {
                e.text = new;
                n += 1;
            }
        }
    }
    script.info_set("PlayResX", &r.new_x.to_string());
    script.info_set("PlayResY", &r.new_y.to_string());
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Event;

    #[test]
    fn scales_positions_sizes_and_margins() {
        let mut s = Script::new();
        s.info_set("PlayResX", "1280");
        s.info_set("PlayResY", "720");
        s.styles[0].fontsize = 40.0;
        s.styles[0].margin_l = 10;
        s.events.push(Event::new(0, 1, "Default", "{\\pos(640,360)\\fs30\\bord2\\move(0,0,1280,720)\\t(0,100,\\fs60\\fscx120)\\clip(0,0,640,360)}x"));
        let n = resample(&mut s, Resample { new_x: 1920, new_y: 1080, scale_sizes: true, aspect_shift: false });
        assert_eq!(n, 1);
        assert_eq!(s.play_res(), (1920, 1080));
        assert_eq!(s.styles[0].fontsize, 60.0);
        assert_eq!(s.styles[0].margin_l, 15);
        let t = &s.events[0].text;
        assert!(t.contains("\\pos(960,540)"), "{t}");
        assert!(t.contains("\\fs45"), "{t}");
        assert!(t.contains("\\bord3"), "{t}");
        assert!(t.contains("\\move(0,0,1920,1080)"), "{t}");
        assert!(t.contains("\\t(0,100,\\fs90\\fscx120)"), "{t}");
        assert!(t.contains("\\clip(0,0,960,540)"), "{t}");
    }

    #[test]
    fn aspect_shift_centers() {
        let mut s = Script::new();
        s.info_set("PlayResX", "960");
        s.info_set("PlayResY", "720");
        s.events.push(Event::new(0, 1, "Default", "{\\pos(480,360)}x"));
        resample(&mut s, Resample { new_x: 1280, new_y: 720, scale_sizes: true, aspect_shift: true });
        assert!(s.events[0].text.contains("\\pos(640,360)"), "{}", s.events[0].text);
    }
}
