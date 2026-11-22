//! Zamanlama son-işleyicisi (Aegisub "Timing Post-Processor" benzeri) ve çözünürlük yeniden örnekleme.
use crate::ass::{Entry, Script};
use crate::overrides::{self, Seg, Tag};

#[derive(Clone, Debug)]
pub struct PostOptions {
    pub lead_in_ms: i64,
    pub lead_out_ms: i64,
    pub keyframes: Vec<i64>,
    /// Anahtar kareye yapışma pencereleri (ms; Aegisub gibi): `*_before` satır zamanı anahtar
    /// kareden ÖNCEYKEN (kare ileride), `*_after` satır zamanı anahtar kareden SONRAYKEN.
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
    // Aegisub: (kare > t && kare - t <= before) || (kare < t && t - kare <= after)
    kfs.iter().copied().filter(|&k| k >= t - after && k <= t + before).min_by_key(|&k| (k - t).abs())
}

/// Seçili (veya tüm) satırlara lead-in/out, bitişik satır ve anahtar kare işlemlerini uygular.
pub fn post_process(script: &mut Script, o: &PostOptions) {
    // Seçim verilmemişse tipografi/karaoke/fx satırları atlanır (videoya kare kare oturtulmuş tabelalar bozulmasın).
    let special = |d: &crate::ass::Dialogue| crate::qc::is_typeset(d) || crate::qc::is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx");
    let idxs: Vec<usize> = script
        .entries
        .iter()
        .enumerate()
        .filter(|(i, e)| matches!(e, Entry::Dialogue(d) if !d.comment && match &o.only { Some(v) => v.contains(i), None => !special(d) }))
        .map(|(i, _)| i)
        .collect();
    // Aegisub sırası: lead-in/out → bitişik satırlar → anahtar kareler
    let mut order = idxs.clone();
    order.sort_by_key(|&i| if let Entry::Dialogue(d) = &script.entries[i] { (d.start_time, d.end_time) } else { (0, 0) });
    let times = |sc: &Script, i: usize| match &sc.entries[i] { Entry::Dialogue(d) => (d.start_time, d.end_time), _ => (0, 0) };
    // 1. lead in/out: komşu satıra taşmaz (zaten çakışan satırlarda o yön değişmez)
    for (k, &i) in order.iter().enumerate() {
        let prev_end = (k > 0).then(|| times(script, order[k - 1]).1);
        let next_start = order.get(k + 1).map(|&j| times(script, j).0);
        if let Entry::Dialogue(d) = &mut script.entries[i] {
            if o.lead_in_ms > 0 {
                let mut s = (d.start_time - o.lead_in_ms).max(0);
                if let Some(pe) = prev_end { s = s.max(pe.min(d.start_time)); }
                d.start_time = s;
            }
            if o.lead_out_ms > 0 {
                let mut e = d.end_time + o.lead_out_ms;
                if let Some(ns) = next_start { e = e.min(ns.max(d.end_time)); }
                d.end_time = e;
            }
        }
    }
    // 2. bitişik satırlar (başlangıca göre sıralı çiftler): eşik içindeki boşluk ya da küçük çakışma
    // kapatılır; büyük çakışma (aynı anda konuşan iki kişi) olduğu gibi kalır
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (a_end, b_start) = (times(script, a).1, times(script, b).0);
        let gap = b_start - a_end;
        if gap != 0 && gap.abs() <= o.adjacent_gap_ms {
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
    // 3. anahtar kare yapışması
    if !o.keyframes.is_empty() {
        for &i in &idxs {
            if let Entry::Dialogue(d) = &mut script.entries[i] {
                if let Some(k) = nearest_kf(&o.keyframes, d.start_time, o.kf_start_before, o.kf_start_after) {
                    if k < d.end_time { d.start_time = k; }
                }
                if let Some(k) = nearest_kf(&o.keyframes, d.end_time, o.kf_end_before, o.kf_end_after) {
                    d.end_time = k.max(d.start_time + 1);
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

/// Yeniden örnekleme durumu (Aegisub `resample_state`): rx/ry ölçek, `ar` yatay germe
/// (\fscx ve stil ScaleX çarpanı), ml/mt sol/üst kenar eklemesi (kenarlık kiplerinde).
#[derive(Clone, Copy)]
struct Rs {
    rx: f64,
    ry: f64,
    ar: f64,
    ml: f64,
    mt: f64,
}

/// En-boy oranı değişince ne yapılacağı (Aegisub Resample Resolution "Aspect Ratio Handling").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArMode {
    /// Yatay ger: \fscx ve stil ScaleX yeni/eski oranla çarpılır (Aegisub varsayılanı).
    #[default]
    Stretch,
    /// Kenarlık ekle (pillarbox/letterbox): yazı oranı korunur, konumlar ortalanır.
    AddBorder,
    /// Kenarlık kaldır: tersine kırpma.
    RemoveBorder,
}

fn scale_drawing(text: &str, sx: f64, sy: f64, shx: f64, shy: f64) -> String {
    let mut axis = 0usize;
    text.split_whitespace().map(|tk| match tk.parse::<f64>() {
        Ok(v) => {
            let r = if axis % 2 == 0 { fmt((v + shx) * sx) } else { fmt((v + shy) * sy) };
            axis += 1;
            r
        }
        Err(_) => tk.to_string(),
    }).collect::<Vec<_>>().join(" ")
}

fn scale_tag(t: &mut Tag, st: &Rs) {
    let (sx, sy) = (st.rx, st.ry);
    match t.name.as_str() {
        // \fs+5 / \fs-5 göreli: dokunma (Aegisub: NORMAL sınıfı)
        "fs" if t.arg.trim_start().starts_with(['+', '-']) => {}
        "fs" | "bord" | "shad" | "fsp" | "pbo" => t.arg = scale_num(&t.arg, sy),
        "xbord" | "xshad" => t.arg = scale_num(&t.arg, sx),
        "ybord" | "yshad" => t.arg = scale_num(&t.arg, sy),
        "fscx" if st.ar != 1.0 => t.arg = scale_num(&t.arg, st.ar),
        "pos" | "org" | "move" | "clip" | "iclip" => {
            let args = t.args();
            let all_num = args.iter().all(|a| a.trim().parse::<f64>().is_ok());
            if all_num && (t.name != "move" || args.len() >= 4) {
                let rect = t.name == "clip" || t.name == "iclip";
                let scaled: Vec<String> = args.iter().enumerate().map(|(i, a)| {
                    // move: x1 y1 x2 y2 [t1 t2] — ilk dört koordinat
                    if t.name == "move" && i >= 4 { return a.clone(); }
                    let v = a.trim().parse::<f64>().unwrap_or(0.0);
                    let r = if i % 2 == 0 { (v + st.ml) * sx } else { (v + st.mt) * sy };
                    // dikdörtgen kırpma tam sayıdır (Aegisub INT, +0.5 yuvarlama)
                    if rect { format!("{}", (r + 0.5).floor() as i64) } else { fmt(r) }
                }).collect();
                t.arg = format!("({})", scaled.join(","));
            } else if t.name == "clip" || t.name == "iclip" {
                let inner = t.arg.trim().trim_start_matches('(').trim_end_matches(')');
                // ilk alan sayı ve virgülle ayrılmışsa ölçek: \clip(2,m 0 0 l ...)
                let (head, body) = match inner.split_once(',') {
                    Some((h, b)) if h.trim().parse::<f64>().is_ok() => (format!("{},", h.trim()), b),
                    _ => (String::new(), inner),
                };
                t.arg = format!("({}{})", head, scale_drawing(body, sx, sy, st.ml, st.mt));
            }
        }
        "t" => {
            // (t1,t2,accel,\tags) veya (\tags): içteki etiketleri ölçekle
            let inner = t.arg.trim().trim_start_matches('(').trim_end_matches(')').to_string();
            if let Some(p) = inner.find('\\') {
                let (head, tags) = inner.split_at(p);
                let mut sub = overrides::parse_block(tags);
                for s in sub.iter_mut() {
                    scale_tag(s, st);
                }
                t.arg = format!("({}{})", head, overrides::block_to_string(&sub));
            }
        }
        _ => {}
    }
}

fn scale_text(text: &str, st: &Rs) -> String {
    let mut segs = overrides::parse(text);
    let mut in_drawing = false;
    for s in segs.iter_mut() {
        match s {
            Seg::Block(tags) => {
                for t in tags.iter_mut() {
                    if t.name == "p" {
                        in_drawing = t.arg.trim().parse::<f64>().map_or(false, |v| v > 0.0);
                    }
                    scale_tag(t, st);
                }
            }
            // çizim \pos'a göreli: kaydırma yok; yatay germede \fscx zaten çarpıldığından rx/ar
            Seg::Text(t) if in_drawing => *t = scale_drawing(t, st.rx / st.ar, st.ry, 0.0, 0.0),
            _ => {}
        }
    }
    overrides::to_string(&segs)
}

/// Betik çözünürlüğünü değiştirir; stilleri, kenar boşluklarını ve etiketleri ölçekler (yatay germe).
pub fn resample(script: &mut Script, new_x: i32, new_y: i32) {
    resample_ar(script, new_x, new_y, ArMode::Stretch)
}

/// Aegisub `ResampleResolution` ile aynı: en-boy oranı %1'den az değişiyorsa dönüştürülmez.
pub fn resample_ar(script: &mut Script, new_x: i32, new_y: i32, mode: ArMode) {
    let (ox, oy) = script.res();
    if (ox, oy) == (new_x, new_y) || ox <= 0 || oy <= 0 || new_x <= 0 || new_y <= 0 {
        return;
    }
    let old_ar = ox as f64 / oy as f64;
    let new_ar = new_x as f64 / new_y as f64;
    let mut margin = [0.0f64; 4]; // sol, sağ, üst, alt
    let mut ar = 1.0;
    if (old_ar - new_ar).abs() / new_ar > 0.01 {
        let mut horiz = new_ar > old_ar;
        match mode {
            ArMode::Stretch => ar = new_ar / old_ar,
            ArMode::AddBorder | ArMode::RemoveBorder => {
                if mode == ArMode::RemoveBorder { horiz = !horiz; }
                if horiz {
                    let m = ((oy as f64 * new_ar - ox as f64) / 2.0).trunc();
                    margin[0] = m;
                    margin[1] = m;
                } else {
                    let m = ((ox as f64 / new_ar - oy as f64) / 2.0).trunc();
                    margin[2] = m;
                    margin[3] = m;
                }
            }
        }
    }
    let sx_src = ox as f64 + margin[0] + margin[1];
    let sy_src = oy as f64 + margin[2] + margin[3];
    let st = Rs { rx: new_x as f64 / sx_src, ry: new_y as f64 / sy_src, ar, ml: margin[0], mt: margin[2] };
    let mg = |v: i32, add: f64, r: f64| ((v as f64 + add) * r + 0.5).floor() as i32;
    for e in script.entries.iter_mut() {
        match e {
            Entry::Style(s) => {
                s.fontsize = (s.fontsize * st.ry * 100.0).round() / 100.0;
                s.outline = (s.outline * st.ry * 100.0).round() / 100.0;
                s.shadow = (s.shadow * st.ry * 100.0).round() / 100.0;
                s.spacing = (s.spacing * st.ry * 100.0).round() / 100.0;
                s.scale_x = (s.scale_x * st.ar * 100.0).round() / 100.0;
                s.margin_l = mg(s.margin_l, margin[0], st.rx);
                s.margin_r = mg(s.margin_r, margin[1], st.rx);
                s.margin_t = mg(s.margin_t, margin[2], st.ry);
                s.margin_b = mg(s.margin_b, margin[2], st.ry);
            }
            Entry::Dialogue(d) => {
                // karaoke şablonu / kod satırları ölçeklenmez (Aegisub)
                if d.comment && (d.effect.starts_with("template") || d.effect.starts_with("code")) {
                    continue;
                }
                // satır kenar boşluğu yalnızca 0 değilse (0 = stilinkini kullan)
                if d.margin_l != 0 { d.margin_l = mg(d.margin_l, margin[0], st.rx); }
                if d.margin_r != 0 { d.margin_r = mg(d.margin_r, margin[1], st.rx); }
                if d.margin_t != 0 { d.margin_t = mg(d.margin_t, margin[2], st.ry); }
                if d.margin_b != 0 { d.margin_b = mg(d.margin_b, margin[2], st.ry); }
                d.text = scale_text(&d.text, &st);
            }
            _ => {}
        }
    }
    script.set_info("PlayResX", &new_x.to_string());
    script.set_info("PlayResY", &new_y.to_string());
}


/// Satır başlangıç/bitişlerini en yakın konuşma sınırına (VAD bölgesi başı/sonu) çeker; yalnızca
/// `max_ms` içindeki sınırlar dikkate alınır. Tipografi/karaoke/fx ve yorum satırları atlanır.
/// `only`: işlenecek entry indeksleri (None = tümü). Değişen satır sayısı.
pub fn snap_to_speech(script: &mut Script, segs: &[(i64, i64)], max_ms: i64, only: Option<&[usize]>) -> usize {
    let starts: Vec<i64> = segs.iter().map(|s| s.0).collect();
    let ends: Vec<i64> = segs.iter().map(|s| s.1).collect();
    let nearest = |v: &[i64], t: i64| v.iter().copied().filter(|x| (x - t).abs() <= max_ms).min_by_key(|x| (x - t).abs());
    let mut n = 0;
    for (i, e) in script.entries.iter_mut().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if only.map_or(false, |o| !o.contains(&i)) || d.comment || crate::qc::is_typeset(d) || crate::qc::is_karaoke(d) || d.effect.eq_ignore_ascii_case("fx") {
            continue;
        }
        let ns = nearest(&starts, d.start_time).unwrap_or(d.start_time);
        let ne = nearest(&ends, d.end_time).unwrap_or(d.end_time);
        if ne > ns && (ns, ne) != (d.start_time, d.end_time) {
            d.start_time = ns;
            d.end_time = ne;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_process_skips_signs_unless_selected() {
        let mk = |a: i64, b: i64, t: &str| crate::ass::Dialogue { start_time: a, end_time: b, text: t.into(), ..Default::default() };
        let mut sc = Script::from_dialogues(vec![mk(1000, 2000, "diyalog"), mk(1000, 1500, "{\\pos(1,1)}TABELA")]);
        let o = PostOptions { lead_in_ms: 100, lead_out_ms: 100, adjacent_gap_ms: 0, keyframes: vec![], only: None, ..PostOptions::default() };
        post_process(&mut sc, &o);
        let v: Vec<_> = sc.dialogues().map(|d| (d.start_time, d.end_time)).collect();
        assert_eq!(v, vec![(900, 2100), (1000, 1500)]);
        let sign = sc.entries.iter().position(|e| matches!(e, Entry::Dialogue(d) if d.text.contains("TABELA"))).unwrap();
        post_process(&mut sc, &PostOptions { only: Some(vec![sign]), ..o });
        assert_eq!(sc.dialogues().nth(1).map(|d| (d.start_time, d.end_time)), Some((900, 1600)));
    }
    use crate::ass::Dialogue;

    fn d(s: i64, e: i64) -> Dialogue {
        Dialogue { start_time: s, end_time: e, text: "x".into(), ..Default::default() }
    }

    #[test]
    fn inceleme_son_isleyici_aegisub_gibi() {
        let run = |v: Vec<(i64, i64)>, o: PostOptions| {
            let mut s = Script::from_dialogues(v.into_iter().map(|(a, b)| d(a, b)).collect());
            post_process(&mut s, &o);
            s.dialogues().map(|d| (d.start_time, d.end_time)).collect::<Vec<_>>()
        };
        let kf = |k: i64| PostOptions { lead_in_ms: 0, lead_out_ms: 0, keyframes: vec![k], ..Default::default() };
        // satır sahne kesmesinden 300 ms önce başlıyor → kesmeye çekilir (Aegisub "starts before")
        assert_eq!(run(vec![(1000, 4000)], kf(1300)), vec![(1300, 4000)]);
        assert_eq!(run(vec![(1000, 4000)], kf(750)), vec![(1000, 4000)]);
        // bitiş kesmeden 300 ms taşıyor → kesmeye çekilir; 350 ms sonraki kare için uzatılmaz
        assert_eq!(run(vec![(1000, 4000)], kf(3700)), vec![(1000, 3700)]);
        assert_eq!(run(vec![(1000, 4000)], kf(4350)), vec![(1000, 4000)]);
        // lead-in/out komşuya taşmaz
        assert_eq!(run(vec![(1000, 2000), (2000, 3000)], PostOptions::default()), vec![(880, 2000), (2000, 3200)]);
        assert_eq!(run(vec![(1000, 2000), (2000, 3000)], PostOptions { adjacent_bias: 1.0, ..Default::default() }), vec![(880, 2000), (2000, 3200)]);
        // aynı anda konuşanlar (büyük çakışma) bozulmaz
        let no_lead = PostOptions { lead_in_ms: 0, lead_out_ms: 0, adjacent_bias: 1.0, ..Default::default() };
        assert_eq!(run(vec![(1000, 5000), (2000, 3000)], no_lead.clone()), vec![(1000, 5000), (2000, 3000)]);
        // küçük boşluk kapatılır
        assert_eq!(run(vec![(1000, 2000), (2100, 3000)], PostOptions { adjacent_bias: 0.0, ..no_lead }), vec![(1000, 2000), (2000, 3000)]);
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

    const HEAD: &str = "[Script Info]\nPlayResX: 640\nPlayResY: 480\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,1,0,1,2,1,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

    #[test]
    fn resample_aegisub_ayrintilari() {
        // 4:3 → 16:9 yatay germe: \fscx ve ScaleX ×1.333, çizim ry ile, \fs+ ve \be dokunulmaz
        let mut s = Script::parse(&format!("{}Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{{\\fs+5\\be1\\blur2\\pbo10\\fscx100\\iclip(1,1,3,3)\\clip(m 0 0 l 64 48)}}a{{\\p1}}m 0 0 l 10 10{{\\p0}}\nComment: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,template line,{{\\fs30\\pos($x,$y)}}\n", HEAD));
        resample(&mut s, 1280, 720);
        let st = s.styles().next().unwrap();
        assert_eq!((st.fontsize, st.spacing, st.scale_x), (30.0, 1.5, 133.33));
        let ds: Vec<_> = s.dialogues().collect();
        let t = &ds[0].text;
        for want in ["\\fs+5", "\\be1", "\\blur2", "\\pbo15", "\\fscx133.33", "\\iclip(2,2,6,5)", "\\clip(m 0 0 l 128 72)", "m 0 0 l 15 15"] {
            assert!(t.contains(want), "{} yok: {}", want, t);
        }
        assert_eq!(ds[1].text, "{\\fs30\\pos($x,$y)}", "şablon satırı ölçeklenmez");
    }

    #[test]
    fn resample_kenarlik_ekle() {
        // 640x480 → 1280x720 kenarlık: kaynak 853x480 sayılır (sol/sağ +106), \pos ortalanır, \fscx değişmez
        let mut s = Script::parse(&format!("{}Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{{\\pos(320,240)\\fscx100}}a\n", HEAD));
        resample_ar(&mut s, 1280, 720, ArMode::AddBorder);
        let t = &s.dialogues().next().unwrap().text;
        assert!(t.contains("\\fscx100"), "{}", t);
        let x: f64 = t.split("pos(").nth(1).unwrap().split(',').next().unwrap().parse().unwrap();
        assert!((x - 640.0).abs() < 2.0, "{}", t);
        assert_eq!(s.styles().next().unwrap().scale_x, 100.0);
        // oran neredeyse aynıysa (848x480 → 1280x720) germe yok
        let mut s = Script::parse(&format!("{}Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{{\\fscx100}}a\n", HEAD.replace("640", "848")));
        resample(&mut s, 1280, 720);
        assert!(s.dialogues().next().unwrap().text.contains("\\fscx100"));
    }

    #[test]
    fn snaps_to_speech_boundaries() {
        let mut s = Script::from_dialogues(vec![
            Dialogue { start_time: 1100, end_time: 2900, text: "a".into(), ..Default::default() },
            Dialogue { start_time: 5000, end_time: 6000, text: "uzak".into(), ..Default::default() },
            Dialogue { start_time: 1050, end_time: 2950, text: "{\\pos(1,1)}tabela".into(), ..Default::default() },
        ]);
        let n = snap_to_speech(&mut s, &[(1000, 3000), (7000, 8000)], 200, None);
        assert_eq!(n, 1);
        let t: Vec<(i64, i64)> = s.dialogues().map(|d| (d.start_time, d.end_time)).collect();
        assert_eq!(t, vec![(1000, 3000), (5000, 6000), (1050, 2950)]);
    }
}
