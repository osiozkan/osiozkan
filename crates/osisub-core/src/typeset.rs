//! Tipografi yardımcıları: kırpma şeritleriyle degrade (lyger "Gradient Everything" benzeri) ve
//! parıltı katmanı ("Blur & Glow" benzeri).
use crate::ass::Dialogue;
use crate::color::Color;
use crate::overrides::{self, Seg, Tag};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    /// soldan sağa (dikey şeritler)
    Horizontal,
    /// yukarıdan aşağıya (yatay şeritler)
    Vertical,
}

#[derive(Clone, Debug)]
pub struct GradientOptions {
    pub direction: Direction,
    /// şerit kalınlığı (piksel)
    pub strip: f64,
    /// başlangıç ve bitiş etiketleri (ör. `\c&H0000FF&\3c&H000000&\alpha&H00&`); ikisinde de bulunan
    /// renk/saydamlık/sayısal etiketler enterpole edilir
    pub from: String,
    pub to: String,
    /// ivme (1 = doğrusal)
    pub accel: f64,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn fmt_num(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn parse_alpha(s: &str) -> Option<u8> {
    let t = s.trim().trim_start_matches('&').trim_end_matches('&');
    let t = t.strip_prefix(['H', 'h']).unwrap_or(t);
    u32::from_str_radix(t, 16).ok().map(|v| (v & 0xFF) as u8)
}

/// İki etiket değeri arasında enterpolasyon; desteklenmeyen tür → None.
pub(crate) fn interp_tag(name: &str, a: &str, b: &str, t: f64) -> Option<String> {
    match name {
        "c" | "1c" | "2c" | "3c" | "4c" => {
            let (ca, cb) = (Color::parse(a)?, Color::parse(b)?);
            let m = |x: u8, y: u8| lerp(x as f64, y as f64, t).round().clamp(0.0, 255.0) as u8;
            Some(Color { r: m(ca.r, cb.r), g: m(ca.g, cb.g), b: m(ca.b, cb.b), a: 0 }.to_tag_string())
        }
        "alpha" | "1a" | "2a" | "3a" | "4a" => {
            let (x, y) = (parse_alpha(a)?, parse_alpha(b)?);
            Some(format!("&H{:02X}&", lerp(x as f64, y as f64, t).round().clamp(0.0, 255.0) as u8))
        }
        "fscx" | "fscy" | "fsp" | "frx" | "fry" | "frz" | "fax" | "fay" | "bord" | "xbord" | "ybord" | "shad" | "xshad" | "yshad" | "blur" | "be" | "fs" => {
            let (x, y): (f64, f64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            Some(fmt_num(lerp(x, y, t)))
        }
        _ => None,
    }
}

/// `{...}` olmadan ya da ile verilen etiket dizisini (ad, değer) listesine çevirir.
pub(crate) fn tag_list(s: &str) -> Vec<Tag> {
    let inner = s.trim().trim_start_matches('{').trim_end_matches('}');
    overrides::parse_block(inner).into_iter().filter(|t| !t.name.is_empty()).collect()
}

/// Satırın dikdörtgen `\clip`'ini okur (x1,y1,x2,y2).
pub fn rect_clip(text: &str) -> Option<(f64, f64, f64, f64)> {
    let t = overrides::get_initial(text, "clip")?;
    let v: Vec<f64> = t.args().iter().filter_map(|x| x.trim().parse().ok()).collect();
    (v.len() == 4).then(|| (v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])))
}

/// Degrade: satırın dikdörtgen kırpmasını (`rect`, yoksa satırdaki `\clip`) şeritlere böler; her şerit
/// ayrı bir satırdır ve `from`→`to` etiketlerinin enterpolasyonunu taşır. Kırpma yoksa hata.
pub fn gradient(d: &Dialogue, rect: Option<(f64, f64, f64, f64)>, o: &GradientOptions) -> Result<Vec<Dialogue>, String> {
    let (x1, y1, x2, y2) = rect.or_else(|| rect_clip(&d.text)).ok_or("Degrade için satırda dikdörtgen \\clip gerekli (Video aracı: Kırpma)")?;
    let from = tag_list(&o.from);
    let to = tag_list(&o.to);
    let pairs: Vec<(&Tag, &Tag)> = from.iter().filter_map(|a| to.iter().find(|b| b.name == a.name).map(|b| (a, b))).collect();
    if pairs.is_empty() {
        return Err("Başlangıç ve bitişte ortak, enterpole edilebilir etiket yok (ör. \\c, \\3c, \\alpha, \\bord)".into());
    }
    if let Some((a, _)) = pairs.iter().find(|(a, b)| interp_tag(&a.name, &a.arg, &b.arg, 0.0).is_none()) {
        return Err(format!("\\{} enterpole edilemiyor", a.name));
    }
    let len = match o.direction { Direction::Horizontal => x2 - x1, Direction::Vertical => y2 - y1 };
    let strip = o.strip.max(1.0);
    let n = ((len / strip).ceil() as usize).clamp(1, 2000);
    let mut out = vec![];
    for i in 0..n {
        let a0 = i as f64 * strip;
        let a1 = ((i + 1) as f64 * strip).min(len);
        let t = if n == 1 { 0.0 } else { (i as f64 / (n - 1) as f64).powf(o.accel.max(0.01)) };
        let clip = match o.direction {
            Direction::Horizontal => format!("({},{},{},{})", fmt_num(x1 + a0), fmt_num(y1), fmt_num(x1 + a1), fmt_num(y2)),
            Direction::Vertical => format!("({},{},{},{})", fmt_num(x1), fmt_num(y1 + a0), fmt_num(x2), fmt_num(y1 + a1)),
        };
        let mut text = overrides::set_initial(&d.text, "clip", &clip);
        for (a, b) in &pairs {
            let v = interp_tag(&a.name, &a.arg, &b.arg, t).unwrap();
            text = overrides::set_initial(&text, &a.name, &v);
            // satır içindeki sonraki bloklarda aynı etiket başlangıç değerini ezmesin
        }
        let mut nd = d.clone();
        nd.text = text;
        out.push(nd);
    }
    Ok(out)
}

/// Parıltı (glow): asıl satırın altına bir katman ekler — aynı satır, artırılmış `\blur`, verilen
/// kenar rengi ve saydamlıkla. Dönüş: [parıltı katmanı, asıl satır (katman +1)].
pub fn glow(d: &Dialogue, blur: f64, color: Option<&str>, alpha: u8) -> Vec<Dialogue> {
    let mut base = d.clone();
    base.layer = d.layer + 1;
    let mut g = d.clone();
    let mut t = overrides::set_initial(&g.text, "blur", &fmt_num(blur));
    if let Some(c) = color.and_then(Color::parse) {
        t = overrides::set_initial(&t, "3c", &c.to_tag_string());
    }
    t = overrides::set_initial(&t, "alpha", &format!("&H{:02X}&", alpha));
    // parıltı katmanında satır içi saydamlık/bulanıklık değişimleri parıltıyı bozmasın: sonraki bloklardan sil
    let mut segs = overrides::parse(&t);
    let mut first = true;
    for s in segs.iter_mut() {
        if let Seg::Block(tags) = s {
            if !first {
                tags.retain(|x| !matches!(x.name.as_str(), "alpha" | "1a" | "3a" | "blur" | "be"));
            }
            first = false;
        }
    }
    g.text = overrides::to_string(&segs);
    vec![g, base]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Dialogue {
        Dialogue { start_time: 0, end_time: 1000, layer: 2, style: "Sign".into(), text: text.into(), ..Default::default() }
    }

    #[test]
    fn horizontal_gradient() {
        let o = GradientOptions { direction: Direction::Horizontal, strip: 10.0, from: "\\c&H0000FF&\\alpha&H00&\\bord2".into(), to: "{\\c&HFF0000&\\alpha&HFF&\\bord6\\fn Arial}".into(), accel: 1.0 };
        let out = gradient(&d("{\\pos(50,20)\\clip(0,0,30,40)}Tabela"), None, &o).unwrap();
        assert_eq!(out.len(), 3);
        assert!(out[0].text.contains("\\clip(0,0,10,40)") && out[0].text.contains("\\c&H0000FF&") && out[0].text.contains("\\alpha&H00&") && out[0].text.contains("\\bord2"), "{}", out[0].text);
        assert!(out[1].text.contains("\\clip(10,0,20,40)") && out[1].text.contains("\\c&H800080&") && out[1].text.contains("\\alpha&H80&") && out[1].text.contains("\\bord4"), "{}", out[1].text);
        assert!(out[2].text.contains("\\clip(20,0,30,40)") && out[2].text.contains("\\c&HFF0000&") && out[2].text.contains("\\bord6"), "{}", out[2].text);
        assert!(out.iter().all(|x| x.text.ends_with("}Tabela") && x.text.contains("\\pos(50,20)")));
        // kalan şerit kısa olur, kırpma dışına taşmaz
        let out = gradient(&d("x"), Some((0.0, 0.0, 25.0, 5.0)), &o).unwrap();
        assert!(out[2].text.contains("\\clip(20,0,25,5)"), "{}", out[2].text);
    }

    #[test]
    fn vertical_and_errors() {
        let o = GradientOptions { direction: Direction::Vertical, strip: 20.0, from: "\\3c&H000000&".into(), to: "\\3c&HFFFFFF&".into(), accel: 1.0 };
        let out = gradient(&d("{\\clip(0,0,100,40)}x"), None, &o).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out[1].text.contains("\\clip(0,20,100,40)") && out[1].text.contains("\\3c&HFFFFFF&"), "{}", out[1].text);
        assert!(gradient(&d("kırpmasız"), None, &o).is_err());
        let bad = GradientOptions { from: "\\c&H0000FF&".into(), to: "\\3c&HFFFFFF&".into(), ..o };
        assert!(gradient(&d("{\\clip(0,0,1,1)}x"), None, &bad).is_err());
    }

    #[test]
    fn glow_layer() {
        let out = glow(&d("{\\bord3\\blur1}Parla{\\alpha&H80&}k"), 6.0, Some("&H00FFFF&"), 0x60);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].layer, out[1].layer), (2, 3));
        assert!(out[0].text.starts_with("{\\bord3\\blur6\\3c&H00FFFF&\\alpha&H60&}Parla"), "{}", out[0].text);
        assert!(!out[0].text.contains("&H80&"), "{}", out[0].text);
        assert_eq!(out[1].text, "{\\bord3\\blur1}Parla{\\alpha&H80&}k");
    }
}
