//! Zamanlama son-işleyicisi (Aegisub "Timing Post-Processor" benzeri) ve çözünürlük yeniden örnekleme.
use crate::ass::{Entry, Script};
use crate::overrides::{self, Seg, Tag};

#[derive(Clone, Debug)]
pub struct PostOptions {
    pub lead_in_ms: i64,
    pub lead_out_ms: i64,
    pub keyframes: Vec<i64>,
    /// Başlangıç için anahtar kareye yapışma pencereleri (anahtar karenin öncesi/sonrası, ms).
    pub kf_start_before: i64,
    pub kf_start_after: i64,
    pub kf_end_before: i64,
    pub kf_end_after: i64,
    /// Bu eşikten küçük boşluklu ardışık satırları birleştir.
    pub adjacent_gap_ms: i64,
    /// 0 = boşluk bir önceki satırın sonuna, 1 = sonrakinin başına eklenir.
    pub adjacent_bias: f64,
    pub only: Option<Vec<usize>>,
}

impl Default for PostOptions {
    fn default() -> Self {
        PostOptions { lead_in_ms: 120, lead_out_ms: 200, keyframes: vec![], kf_start_before: 350, kf_start_after: 100, kf_end_before: 100, kf_end_after: 400, adjacent_gap_ms: 300, adjacent_bias: 0.0, only: None }
    }
}

fn nearest_kf(kfs: &[i64], t: i64, before: i64, after: i64) -> Option<i64> {
    kfs.iter().copied().filter(|&k| k >= t - before && k <= t + after).min_by_key(|&k| (k - t).abs())
}

/// Seçili (veya tüm) satırlara lead-in/out, bitişik satır ve anahtar kare işlemlerini uygular.
pub fn post_process(script: &mut Script, o: &PostOptions) {
    let idxs: Vec<usize> = script.entries.iter().enumerate().filter(|(i, e)| matches!(e, Entry::Dialogue(d) if !d.comment) && o.only.as_ref().map_or(true, |v| v.contains(i))).map(|(i, _)| i).collect();
    // 1. lead in/out
    for &i in &idxs {
        if let Entry::Dialogue(d) = &mut script.entries[i] {
            d.start_time = (d.start_time - o.lead_in_ms).max(0);
            d.end_time += o.lead_out_ms;
        }
    }
    // 2. anahtar kare yapışması
    if !o.keyframes.is_empty() {
        for &i in &idxs {
            if let Entry::Dialogue(d) = &mut script.entries[i] {
                if let Some(k) = nearest_kf(&o.keyframes, d.start_time, o.kf_start_before, o.kf_start_after) {
                    d.start_time = k;
                }
                if let Some(k) = nearest_kf(&o.keyframes, d.end_time, o.kf_end_before, o.kf_end_after) {
                    d.end_time = k.max(d.start_time + 1);
                }
            }
        }
    }
    // 3. bitişik satırlar (başlangıca göre sıralı çiftler)
    let mut order = idxs.clone();
    order.sort_by_key(|&i| if let Entry::Dialogue(d) = &script.entries[i] { d.start_time } else { 0 });
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (a_end, b_start) = match (&script.entries[a], &script.entries[b]) {
            (Entry::Dialogue(x), Entry::Dialogue(y)) => (x.end_time, y.start_time),
            _ => continue,
        };
        let gap = b_start - a_end;
        // Çakışma da (gap < 0) bu eşik içinde düzeltilir.
        if gap <= o.adjacent_gap_ms {
            let boundary = a_end + (gap as f64 * o.adjacent_bias).round() as i64;
            if let Entry::Dialogue(x) = &mut script.entries[a] {
                if boundary > x.start_time {
                    x.end_time = boundary;
                }
            }
            if let Entry::Dialogue(y) = &mut script.entries[b] {
                if boundary < y.end_time {
                    y.start_time = boundary;
                }
            }
        }
    }
}

// ----------------------------------------------------------------- yeniden örnekleme

fn scale_num(s: &str, f: f64) -> String {
    match s.trim().parse::<f64>() {
        Ok(v) => fmt(v * f),
        Err(_) => s.to_string(),
    }
}

fn fmt(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{}", r) }
}

fn scale_tag(t: &mut Tag, sx: f64, sy: f64) {
    let uni = sy; // yazı boyutu/kenar gibi tek yönlü değerler dikey orana göre
    match t.name.as_str() {
        "fs" | "bord" | "shad" | "fsp" | "blur" | "be" => t.arg = scale_num(&t.arg, uni),
        "xbord" | "xshad" => t.arg = scale_num(&t.arg, sx),
        "ybord" | "yshad" => t.arg = scale_num(&t.arg, sy),
        "pos" | "org" | "move" | "clip" | "iclip" => {
            let args = t.args();
            // clip(vektör) çizim komutu: [scale,] m x y l x y ...
            let all_num = args.iter().all(|a| a.trim().parse::<f64>().is_ok());
            if all_num && (t.name != "move" || args.len() >= 4) {
                let scaled: Vec<String> = args.iter().enumerate().map(|(i, a)| {
                    // move: x1 y1 x2 y2 [t1 t2] — ilk dört koordinat; clip dikdörtgen: x1 y1 x2 y2
                    if t.name == "move" && i >= 4 { a.clone() } else { scale_num(a, if i % 2 == 0 { sx } else { sy }) }
                }).collect();
                t.arg = format!("({})", scaled.join(","));
            } else if t.name == "clip" || t.name == "iclip" {
                let inner = t.arg.trim().trim_start_matches('(').trim_end_matches(')');
                let toks: Vec<&str> = inner.split_whitespace().collect();
                let mut out: Vec<String> = Vec::new();
                let mut axis = 0;
                let mut start = 0;
                // İlk simge sayıysa (ölçek) ve ardından komut harfi geliyorsa ölçek olarak bırak.
                if let Some(first) = toks.first() {
                    if first.parse::<f64>().is_ok() && toks.get(1).map_or(false, |x| x.parse::<f64>().is_err()) {
                        out.push(first.to_string());
                        start = 1;
                    }
                }
                for tk in &toks[start..] {
                    if tk.parse::<f64>().is_ok() {
                        out.push(scale_num(tk, if axis % 2 == 0 { sx } else { sy }));
                        axis += 1;
                    } else {
                        out.push(tk.to_string());
                    }
                }
                t.arg = format!("({})", out.join(" "));
            }
        }
        "t" => {
            // (t1,t2,accel,\tags) veya (\tags): içteki etiketleri ölçekle
            let inner = t.arg.trim().trim_start_matches('(').trim_end_matches(')').to_string();
            if let Some(p) = inner.find('\\') {
                let (head, tags) = inner.split_at(p);
                let mut sub = overrides::parse_block(tags);
                for s in sub.iter_mut() {
                    scale_tag(s, sx, sy);
                }
                t.arg = format!("({}{})", head, overrides::block_to_string(&sub));
            }
        }
        _ => {}
    }
}

fn scale_text(text: &str, sx: f64, sy: f64) -> String {
    let mut segs = overrides::parse(text);
    let mut in_drawing = false;
    for s in segs.iter_mut() {
        match s {
            Seg::Block(tags) => {
                for t in tags.iter_mut() {
                    if t.name == "p" {
                        in_drawing = t.arg.trim().parse::<f64>().map_or(false, |v| v > 0.0);
                    }
                    scale_tag(t, sx, sy);
                }
            }
            Seg::Text(t) if in_drawing => {
                let toks: Vec<String> = t.split_whitespace().scan(0usize, |axis, tk| {
                    if tk.parse::<f64>().is_ok() {
                        let r = scale_num(tk, if *axis % 2 == 0 { sx } else { sy });
                        *axis += 1;
                        Some(r)
                    } else {
                        Some(tk.to_string())
                    }
                }).collect();
                *t = toks.join(" ");
            }
            _ => {}
        }
    }
    overrides::to_string(&segs)
}

/// Betik çözünürlüğünü değiştirir; stilleri, kenar boşluklarını ve etiketleri ölçekler.
pub fn resample(script: &mut Script, new_x: i32, new_y: i32) {
    let (ox, oy) = script.res();
    if (ox, oy) == (new_x, new_y) || ox == 0 || oy == 0 {
        return;
    }
    let (sx, sy) = (new_x as f64 / ox as f64, new_y as f64 / oy as f64);
    for e in script.entries.iter_mut() {
        match e {
            Entry::Style(s) => {
                s.fontsize = (s.fontsize * sy * 100.0).round() / 100.0;
                s.outline = (s.outline * sy * 100.0).round() / 100.0;
                s.shadow = (s.shadow * sy * 100.0).round() / 100.0;
                s.spacing = (s.spacing * sx * 100.0).round() / 100.0;
                s.margin_l = (s.margin_l as f64 * sx).round() as i32;
                s.margin_r = (s.margin_r as f64 * sx).round() as i32;
                s.margin_t = (s.margin_t as f64 * sy).round() as i32;
                s.margin_b = (s.margin_b as f64 * sy).round() as i32;
            }
            Entry::Dialogue(d) => {
                d.margin_l = (d.margin_l as f64 * sx).round() as i32;
                d.margin_r = (d.margin_r as f64 * sx).round() as i32;
                d.margin_t = (d.margin_t as f64 * sy).round() as i32;
                d.margin_b = (d.margin_b as f64 * sy).round() as i32;
                d.text = scale_text(&d.text, sx, sy);
            }
            _ => {}
        }
    }
    script.set_info("PlayResX", &new_x.to_string());
    script.set_info("PlayResY", &new_y.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn d(s: i64, e: i64) -> Dialogue {
        Dialogue { start_time: s, end_time: e, text: "x".into(), ..Default::default() }
    }

    #[test]
    fn post_lead_adjacent_keyframes() {
        let mut s = Script::from_dialogues(vec![d(1000, 2000), d(2100, 3000), d(10_000, 11_000)]);
        let o = PostOptions { lead_in_ms: 100, lead_out_ms: 100, adjacent_gap_ms: 300, keyframes: vec![900, 11_150], ..Default::default() };
        post_process(&mut s, &o);
        let v: Vec<(i64, i64)> = s.dialogues().map(|d| (d.start_time, d.end_time)).collect();
        assert_eq!(v[0].0, 900, "başlangıç anahtar kareye yapıştı");
        assert_eq!(v[0].1, v[1].0, "bitişik satırlar kesiksiz");
        assert_eq!(v[2].1, 11_150, "bitiş anahtar kareye yapıştı");
    }

    #[test]
    fn resample_scales_everything() {
        let mut s = Script::parse("[Script Info]\nPlayResX: 640\nPlayResY: 360\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,5,,{\\pos(320,180)\\fs30\\t(0,500,\\fs60)\\clip(0,0,640,360)}a{\\p1}m 0 0 l 10 20{\\p0}\n");
        resample(&mut s, 1280, 720);
        assert_eq!(s.res(), (1280, 720));
        let st = s.styles().next().unwrap();
        assert_eq!((st.fontsize, st.outline, st.margin_l, st.margin_t), (40.0, 4.0, 20, 20));
        let t = &s.dialogues().next().unwrap().text;
        assert!(t.contains("\\pos(640,360)") && t.contains("\\fs60") && t.contains("\\t(0,500,\\fs120)") && t.contains("\\clip(0,0,1280,720)"), "{}", t);
        assert!(t.contains("m 0 0 l 20 40"), "{}", t);
        assert_eq!(s.dialogues().next().unwrap().margin_t, 10);
    }
}
