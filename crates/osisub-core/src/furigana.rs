//! Furigana düzeni (karaskel'in `漢字|かんじ` sözdizimi; şablonsuz).
//!
//! Karaoke hecesi `{\k20}漢字|かんじ` biçimindeyse `|` (ya da tam genişlik `｜`) sonrası okunuştur.
//! `layout` ana satırı okunuşlardan arındırır ve her okunuşu, hecenin ortasına `\an2\pos` ile
//! (ana satırın üst kenarına) oturan küçük bir satır olarak üretir; okunuş, hecesi söylenirken
//! aynı karaoke türüyle vurgulanır (`\k<gecikme>\kf<süre>`). Okunuş öneki `#`/`<` (karaskel'de
//! "birleştirme"/"sola taşma" işaretleri) düzende yok sayılır.
use crate::ass::{Dialogue, Style};
use crate::karaoke::{self, Syl};
use crate::overrides;

/// Okunuş stillerinin ölçeği (karaskel `furigana_scale` varsayılanı).
pub const SCALE: f64 = 0.5;

/// Hece metnini (taban, okunuş) olarak ayırır; `|` etiket bloklarının dışında aranır.
pub fn split_reading(text: &str) -> (String, Option<String>) {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = (depth - 1).max(0),
            '|' | '｜' if depth == 0 => {
                let r = text[i + c.len_utf8()..].trim_start_matches(['#', '＃', '<', '＜']);
                return (text[..i].to_string(), Some(r.to_string()));
            }
            _ => {}
        }
    }
    (text.to_string(), None)
}

/// Satırda furigana okunuşu var mı?
pub fn has_furigana(text: &str) -> bool {
    karaoke::parse(text).iter().any(|s| split_reading(&s.text).1.is_some_and(|r| !r.is_empty()))
}

/// Okunuşları kaldırılmış metin (karaoke etiketleri korunur).
pub fn strip(text: &str) -> String {
    if !text.contains(['|', '｜']) {
        return text.to_string();
    }
    let (lead, syls) = karaoke::parse_with_lead(text);
    let syls: Vec<Syl> = syls.into_iter().map(|mut s| { s.text = split_reading(&s.text).0; s }).collect();
    karaoke::build_with_lead(&lead, &syls)
}

/// Ana stilden okunuş stili: `<ad>-furigana`, yazı boyutu/kenar/gölge `scale` ile küçültülür.
pub fn style_for(st: &Style, scale: f64) -> Style {
    let mut f = st.clone();
    f.name = format!("{}-furigana", st.name);
    f.fontsize = (st.fontsize * scale * 100.0).round() / 100.0;
    f.outline = (st.outline * scale * 100.0).round() / 100.0;
    f.shadow = (st.shadow * scale * 100.0).round() / 100.0;
    f.align = 2;
    f
}

fn num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

/// Ana satırın ve okunuş satırlarının düzeni. `measure(metin, okunuş_mu)` görünür metnin piksel
/// genişliğini döndürür (yazı tipi yoksa `None`); `main_h` ana satırın yüksekliğidir.
/// Dönüş: (ana satır, okunuş satırları). Ana satır `\pos` almaz: hizalaması aynen korunur.
pub fn layout(d: &Dialogue, st: &Style, res: (i32, i32), main_h: f64, measure: &mut dyn FnMut(&str, bool) -> Option<f64>) -> Result<(Dialogue, Vec<Dialogue>), String> {
    if crate::tags::visible_text(&d.text).contains('\n') {
        return Err("Çok satırlı (\\N) karaoke satırında furigana düzeni desteklenmiyor".into());
    }
    let (lead, syls) = karaoke::parse_with_lead(&d.text);
    if !syls.iter().any(|s| split_reading(&s.text).1.is_some()) {
        return Err("Satırda furigana yok (hece biçimi: {\\k20}漢字|かんじ)".into());
    }
    let fail = || "Yazı tipi bulunamadı (genişlik ölçülemedi)".to_string();
    let mut x = measure(&crate::tags::visible_text(&lead), false).ok_or_else(fail)?;
    // (hece ortasının satır başına uzaklığı, başlangıç cs, hece)
    let mut parts: Vec<(f64, i64, Syl, String)> = vec![];
    let mut t = 0;
    for s in &syls {
        let (base, reading) = split_reading(&s.text);
        let w = measure(&crate::tags::visible_text(&base), false).ok_or_else(fail)?;
        if let Some(r) = reading.filter(|r| !r.is_empty()) {
            parts.push((x + w / 2.0, t, s.clone(), r));
        }
        x += w;
        t += s.dur;
    }
    // libass/VSFilter satır başı ve sonundaki boşlukları hizalamada saymaz
    let vis = crate::tags::visible_text(&strip(&d.text));
    let lead_ws: String = vis.chars().take_while(|c| c.is_whitespace()).collect();
    let trail_ws: String = if lead_ws.len() == vis.len() { String::new() } else { vis.chars().rev().take_while(|c| c.is_whitespace()).collect() };
    let lw = if lead_ws.is_empty() { 0.0 } else { measure(&lead_ws, false).unwrap_or(0.0) };
    let tw = if trail_ws.is_empty() { 0.0 } else { measure(&trail_ws, false).unwrap_or(0.0) };
    let total_w = (x - lw - tw).max(0.0);
    let get = |n: &str| overrides::get_initial(&d.text, n);
    let an = get("an").and_then(|t| t.number()).map(|v| v as i32).unwrap_or(st.align);
    let col = an.rem_euclid(3); // 1 sol, 2 orta, 0 sağ
    let row = (an - 1).div_euclid(3); // 0 alt, 1 orta, 2 üst
    let pos = get("pos").or_else(|| get("move")).map(|t| t.args()).and_then(|a| Some((a.first()?.trim().parse::<f64>().ok()?, a.get(1)?.trim().parse::<f64>().ok()?)));
    let (left, top) = match pos {
        Some((px, py)) => {
            let l = match col { 1 => px, 2 => px - total_w / 2.0, _ => px - total_w };
            let t = match row { 0 => py - main_h, 1 => py - main_h / 2.0, _ => py };
            (l, t)
        }
        None => {
            let ml = if d.margin_l > 0 { d.margin_l } else { st.margin_l } as f64;
            let mr = if d.margin_r > 0 { d.margin_r } else { st.margin_r } as f64;
            let mv_t = if d.margin_t > 0 { d.margin_t } else { st.margin_t } as f64;
            let mv_b = if d.margin_b > 0 { d.margin_b } else { st.margin_b } as f64;
            let (rx, ry) = (res.0 as f64, res.1 as f64);
            let l = match col { 1 => ml, 2 => ml + (rx - ml - mr - total_w) / 2.0, _ => rx - mr - total_w };
            let t = match row { 0 => ry - mv_b - main_h, 1 => (ry - main_h) / 2.0, _ => mv_t };
            (l, t)
        }
    };
    let fad = get("fad").map(|t| format!("\\fad({})", t.arg.trim_matches(|c| c == '(' || c == ')')));
    let mut main = d.clone();
    main.text = strip(&d.text);
    let mut out = vec![];
    for (cx, start, s, r) in parts {
        let cx = cx - lw;
        let mut f = d.clone();
        f.style = format!("{}-furigana", d.style);
        f.margin_l = 0;
        f.margin_r = 0;
        f.margin_t = 0;
        f.margin_b = 0;
        f.extra.clear();
        // gecikme ayrı (boş) hecede: libass/VSFilter aynı bloktaki iki \k'yı farklı yorumlayabilir
        let head = format!("\\an2\\pos({},{}){}", num(left + cx), num(top), fad.clone().unwrap_or_default());
        f.text = if start > 0 { format!("{{{head}\\k{start}}}{{\\{}{}}}{r}", s.kind, s.dur) } else { format!("{{{head}\\{}{}}}{r}", s.kind, s.dur) };
        out.push(f);
    }
    Ok((main, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mono(t: &str, furi: bool) -> Option<f64> {
        Some(t.chars().count() as f64 * if furi { 10.0 } else { 20.0 })
    }

    #[test]
    fn ayir_ve_temizle() {
        assert_eq!(split_reading("漢字|かんじ"), ("漢字".into(), Some("かんじ".into())));
        assert_eq!(split_reading("{\\fs5|x}a｜#b"), ("{\\fs5|x}a".into(), Some("b".into())));
        assert_eq!(split_reading("abc"), ("abc".into(), None));
        let t = "{\\an8}{\\k20}君|きみ{\\kf30}の{\\k40}名|な{\\k10}前|まえ";
        assert!(has_furigana(t));
        assert!(!has_furigana("{\\k20}ki{\\k10}mi"));
        assert_eq!(strip(t), "{\\an8}{\\k20}君{\\kf30}の{\\k40}名{\\k10}前");
        assert_eq!(strip("düz metin"), "düz metin");
    }

    #[test]
    fn duzen_alt_orta() {
        let st = Style { margin_l: 10, margin_r: 10, margin_b: 20, align: 2, fontsize: 40.0, ..Default::default() };
        let d = Dialogue { text: "{\\k20}君|きみ{\\kf30}の{\\k40}名前|なまえ".into(), start_time: 1000, end_time: 3000, ..Default::default() };
        let (main, f) = layout(&d, &st, (640, 360), 40.0, &mut mono).unwrap();
        assert_eq!(main.text, "{\\k20}君{\\kf30}の{\\k40}名前");
        // genişlik 4×20 = 80; sol = 10 + (620-80)/2 = 280; üst = 360-20-40 = 300
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].style, "Default-furigana");
        assert_eq!(f[0].text, "{\\an2\\pos(290,300)\\k20}きみ");
        assert_eq!(f[1].text, "{\\an2\\pos(340,300)\\k50}{\\k40}なまえ");
        assert_eq!((f[1].start_time, f[1].end_time), (1000, 3000));
    }

    #[test]
    fn duzen_konumlu_ve_ust() {
        let st = Style { align: 2, fontsize: 40.0, ..Default::default() };
        let d = Dialogue { text: "{\\an7\\pos(100,50)\\fad(100,200)}{\\k10}a|x{\\k10}b".into(), ..Default::default() };
        let (_, f) = layout(&d, &st, (640, 360), 40.0, &mut mono).unwrap();
        assert_eq!(f[0].text, "{\\an2\\pos(110,50)\\fad(100,200)\\k10}x");
        // üst orta, kenar boşluğu satırdan
        let d = Dialogue { text: "{\\an8}{\\k10}ab|x".into(), margin_t: 15, ..Default::default() };
        let (_, f) = layout(&d, &st, (640, 360), 40.0, &mut mono).unwrap();
        assert_eq!(f[0].text, "{\\an2\\pos(320,15)\\k10}x");
        // sağ alt
        let st3 = Style { align: 3, margin_r: 40, margin_b: 10, ..st.clone() };
        let d = Dialogue { text: "{\\k10}ab|x{\\k5}c".into(), ..Default::default() };
        let (_, f) = layout(&d, &st3, (640, 360), 40.0, &mut mono).unwrap();
        // genişlik 60, sol = 640-40-60 = 540, orta 560; üst 360-10-40
        assert_eq!(f[0].text, "{\\an2\\pos(560,310)\\k10}x");
    }

    #[test]
    fn bosluklar_kirpilir() {
        let st = Style { margin_l: 10, margin_r: 10, margin_b: 20, align: 2, fontsize: 40.0, ..Default::default() };
        let a = Dialogue { text: "{\\k20}君|きみ{\\k30}の".into(), ..Default::default() };
        let b = Dialogue { text: "{\\k0} {\\k20}君|きみ{\\k30}の{\\k0} ".into(), ..Default::default() };
        let (_, fa) = layout(&a, &st, (640, 360), 40.0, &mut mono).unwrap();
        let (_, fb) = layout(&b, &st, (640, 360), 40.0, &mut mono).unwrap();
        let pos = |t: &str| overrides::get_initial(t, "pos").unwrap().arg;
        assert_eq!(pos(&fa[0].text), pos(&fb[0].text));
    }

    #[test]
    fn hatalar_ve_stil() {
        let st = Style::default();
        let d = Dialogue { text: "{\\k10}a\\N{\\k10}b|c".into(), ..Default::default() };
        assert!(layout(&d, &st, (640, 360), 40.0, &mut mono).is_err());
        let d = Dialogue { text: "{\\k10}a".into(), ..Default::default() };
        assert!(layout(&d, &st, (640, 360), 40.0, &mut mono).is_err());
        let s = style_for(&Style { fontsize: 45.0, outline: 3.0, shadow: 1.0, align: 8, ..Default::default() }, SCALE);
        assert_eq!((s.name.as_str(), s.fontsize, s.outline, s.shadow, s.align), ("Default-furigana", 22.5, 1.5, 0.5, 2));
    }
}
