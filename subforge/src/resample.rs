//! Resolution resampling (Aegisub: Subtitle > Resample Resolution): rescales styles, margins and
//! override tags (\pos \move \org \clip \fs \bord \shad \fsp ... and \p drawings).

use crate::ass::{Document, Entry};
use crate::tags::{split_blocks, Seg};
use regex::{Captures, Regex};

fn n(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.to_string() }
}

fn pf(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

struct Rx {
    pos: Regex,
    mv: Regex,
    rect_clip: Regex,
    vec_clip: Regex,
    fs: Regex,
    fsp: Regex,
    bord: Regex,
    xbord: Regex,
    ybord: Regex,
    shad: Regex,
    xshad: Regex,
    yshad: Regex,
    p: Regex,
    num: Regex,
}

impl Rx {
    fn new() -> Rx {
        let num = r"-?\d+(?:\.\d+)?";
        Rx {
            pos: Regex::new(&format!(r"\\(pos|org)\(\s*({n})\s*,\s*({n})\s*\)", n = num)).unwrap(),
            mv: Regex::new(&format!(r"\\move\(\s*({n})\s*,\s*({n})\s*,\s*({n})\s*,\s*({n})\s*((?:,\s*{n}\s*,\s*{n}\s*)?)\)", n = num)).unwrap(),
            rect_clip: Regex::new(&format!(r"\\(i?clip)\(\s*({n})\s*,\s*({n})\s*,\s*({n})\s*,\s*({n})\s*\)", n = num)).unwrap(),
            vec_clip: Regex::new(r"\\(i?clip)\(\s*(?:(\d+)\s*,\s*)?([mnlbspc][^)]*)\)").unwrap(),
            fs: Regex::new(&format!(r"\\fs({})", num)).unwrap(),
            fsp: Regex::new(&format!(r"\\fsp({})", num)).unwrap(),
            bord: Regex::new(&format!(r"\\bord({})", num)).unwrap(),
            xbord: Regex::new(&format!(r"\\xbord({})", num)).unwrap(),
            ybord: Regex::new(&format!(r"\\ybord({})", num)).unwrap(),
            shad: Regex::new(&format!(r"\\shad({})", num)).unwrap(),
            xshad: Regex::new(&format!(r"\\xshad({})", num)).unwrap(),
            yshad: Regex::new(&format!(r"\\yshad({})", num)).unwrap(),
            p: Regex::new(r"\\p(\d+)").unwrap(),
            num: Regex::new(num).unwrap(),
        }
    }

    /// Scale alternating x,y numbers inside a drawing / vector clip.
    fn drawing(&self, d: &str, fx: f64, fy: f64) -> String {
        let mut i = 0;
        self.num
            .replace_all(d, |c: &Captures| {
                let v = pf(&c[0]);
                let r = if i % 2 == 0 { v * fx } else { v * fy };
                i += 1;
                n(r)
            })
            .to_string()
    }

    fn block(&self, t: &str, fx: f64, fy: f64) -> String {
        let mut s = t.to_string();
        s = self.pos.replace_all(&s, |c: &Captures| format!("\\{}({},{})", &c[1], n(pf(&c[2]) * fx), n(pf(&c[3]) * fy))).to_string();
        s = self
            .mv
            .replace_all(&s, |c: &Captures| format!("\\move({},{},{},{}{})", n(pf(&c[1]) * fx), n(pf(&c[2]) * fy), n(pf(&c[3]) * fx), n(pf(&c[4]) * fy), &c[5]))
            .to_string();
        s = self
            .rect_clip
            .replace_all(&s, |c: &Captures| format!("\\{}({},{},{},{})", &c[1], n(pf(&c[2]) * fx), n(pf(&c[3]) * fy), n(pf(&c[4]) * fx), n(pf(&c[5]) * fy)))
            .to_string();
        s = self
            .vec_clip
            .replace_all(&s, |c: &Captures| {
                let scale = c.get(2).map(|m| format!("{},", m.as_str())).unwrap_or_default();
                format!("\\{}({}{})", &c[1], scale, self.drawing(&c[3], fx, fy))
            })
            .to_string();
        let one = |re: &Regex, tag: &str, f: f64, s: &str| re.replace_all(s, |c: &Captures| format!("\\{}{}", tag, n(pf(&c[1]) * f))).to_string();
        s = one(&self.fs, "fs", fy, &s);
        s = one(&self.fsp, "fsp", fx, &s);
        s = one(&self.bord, "bord", fy, &s);
        s = one(&self.xbord, "xbord", fx, &s);
        s = one(&self.ybord, "ybord", fy, &s);
        s = one(&self.shad, "shad", fy, &s);
        s = one(&self.xshad, "xshad", fx, &s);
        s = one(&self.yshad, "yshad", fy, &s);
        s
    }

    fn text(&self, text: &str, fx: f64, fy: f64) -> String {
        let mut out = String::new();
        let mut drawing = false;
        for seg in split_blocks(text) {
            match seg {
                Seg::Tags(t) => {
                    if let Some(c) = self.p.captures_iter(t).last() {
                        drawing = c[1].parse::<i32>().unwrap_or(0) > 0;
                    }
                    out.push('{');
                    out.push_str(&self.block(t, fx, fy));
                    out.push('}');
                }
                Seg::Text(t) => {
                    if drawing {
                        out.push_str(&self.drawing(t, fx, fy));
                    } else {
                        out.push_str(t);
                    }
                }
            }
        }
        out
    }
}

/// Resample to a new script resolution. Returns false when the resolution is unchanged.
pub fn resample(doc: &mut Document, new_x: i32, new_y: i32) -> bool {
    let (ox, oy) = doc.res();
    if (ox, oy) == (new_x, new_y) || ox <= 0 || oy <= 0 {
        return false;
    }
    let fx = new_x as f64 / ox as f64;
    let fy = new_y as f64 / oy as f64;
    let rx = Rx::new();
    for e in doc.entries.iter_mut() {
        match e {
            Entry::Style(s) => {
                s.fontsize = (s.fontsize * fy * 1000.0).round() / 1000.0;
                s.spacing = (s.spacing * fx * 1000.0).round() / 1000.0;
                s.outline = (s.outline * fy * 1000.0).round() / 1000.0;
                s.shadow = (s.shadow * fy * 1000.0).round() / 1000.0;
                s.margin_l = (s.margin_l as f64 * fx).round() as i32;
                s.margin_r = (s.margin_r as f64 * fx).round() as i32;
                s.margin_v = (s.margin_v as f64 * fy).round() as i32;
            }
            Entry::Event(ev) => {
                ev.margin_l = (ev.margin_l as f64 * fx).round() as i32;
                ev.margin_r = (ev.margin_r as f64 * fx).round() as i32;
                ev.margin_v = (ev.margin_v as f64 * fy).round() as i32;
                ev.text = rx.text(&ev.text, fx, fy);
            }
            _ => {}
        }
    }
    doc.set_info("PlayResX", &new_x.to_string());
    doc.set_info("PlayResY", &new_y.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::{Event, Style};

    #[test]
    fn scales_tags_styles_and_drawings() {
        let mut d = Document::new_default();
        d.set_info("PlayResX", "1280");
        d.set_info("PlayResY", "720");
        d.entries.push(Entry::Style(Style { name: "S".into(), fontsize: 40.0, outline: 2.0, shadow: 1.0, margin_l: 10, margin_r: 10, margin_v: 20, ..Style::default() }));
        d.entries.push(Entry::Event(Event {
            text: "{\\pos(640,360)\\fs30\\bord2\\move(0,0,100,50,0,500)\\clip(10,10,20,20)\\t(0,100,\\fs60)}Hi{\\p1}m 0 0 l 100 0 100 50{\\p0}x".into(),
            margin_v: 10,
            ..Event::default()
        }));
        assert!(resample(&mut d, 1920, 1080));
        assert_eq!(d.res(), (1920, 1080));
        let s = d.style("S").unwrap();
        assert_eq!(s.fontsize, 60.0);
        assert_eq!(s.margin_l, 15);
        assert_eq!(s.margin_v, 30);
        let t = &d.events().next().unwrap().text;
        assert_eq!(
            t,
            "{\\pos(960,540)\\fs45\\bord3\\move(0,0,150,75,0,500)\\clip(15,15,30,30)\\t(0,100,\\fs90)}Hi{\\p1}m 0 0 l 150 0 150 75{\\p0}x"
        );
        assert!(!resample(&mut d, 1920, 1080));
    }
}
