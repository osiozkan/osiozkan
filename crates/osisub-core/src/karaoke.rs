//! Karaoke (\k, \kf, \ko) hece modeli ve düzenleme işlemleri.
use crate::overrides::{self, Seg, Tag};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Syl {
    /// Süre, santisaniye.
    pub dur: i64,
    /// "k", "kf" veya "ko".
    pub kind: String,
    /// Hece metni; satır içi etiket bloklarını (`{\fs50}`) içerebilir, `\k` etiketi içermez.
    pub text: String,
}

impl Syl {
    pub fn visible(&self) -> String {
        crate::tags::strip_tags(&self.text)
    }
    fn plain(&self) -> bool {
        !self.text.contains('{')
    }
}

fn is_k(name: &str) -> bool {
    matches!(name, "k" | "kf" | "ko" | "K")
}

/// Satırı hecelere ayırır. Önceki metin (ilk \k'dan önce) süresi 0 olan ayrı bir hece olur.
pub fn parse(text: &str) -> Vec<Syl> {
    let mut out: Vec<Syl> = vec![];
    let mut cur: Option<Syl> = None;
    for seg in overrides::parse(text) {
        match seg {
            Seg::Block(tags) => {
                let ki = tags.iter().position(|t| is_k(&t.name));
                match ki {
                    Some(i) => {
                        if let Some(c) = cur.take() {
                            out.push(c);
                        }
                        let k = &tags[i];
                        let kind = if k.name == "K" { "kf".to_string() } else { k.name.clone() };
                        let rest: Vec<Tag> = tags.iter().enumerate().filter(|(j, t)| *j != i && !is_k(&t.name)).map(|(_, t)| t.clone()).collect();
                        let mut s = Syl { dur: k.number().unwrap_or(0.0).round() as i64, kind, text: String::new() };
                        if !rest.is_empty() {
                            s.text = format!("{{{}}}", overrides::block_to_string(&rest));
                        }
                        cur = Some(s);
                    }
                    None => {
                        let c = cur.get_or_insert(Syl { dur: 0, kind: "k".into(), text: String::new() });
                        c.text.push_str(&format!("{{{}}}", overrides::block_to_string(&tags)));
                    }
                }
            }
            Seg::Text(t) => {
                let c = cur.get_or_insert(Syl { dur: 0, kind: "k".into(), text: String::new() });
                c.text.push_str(&t);
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

/// İlk `\\k` etiketinden önce metin ya da etiket bloğu var mı? (Aegisub'daki "0. hece".)
pub fn has_lead(text: &str) -> bool {
    for seg in overrides::parse(text) {
        match seg {
            Seg::Block(tags) => {
                if tags.iter().any(|t| is_k(&t.name)) {
                    return false;
                }
                return true;
            }
            Seg::Text(t) => {
                if !t.is_empty() {
                    return true;
                }
            }
        }
    }
    false
}

/// İlk `\\k` öncesi metni (ör. `{\\an8}`) ayrı döndürür; heceler yalnızca `\\k`'lı parçalardır.
pub fn parse_with_lead(text: &str) -> (String, Vec<Syl>) {
    let mut v = parse(text);
    let lead = if has_lead(text) && !v.is_empty() { v.remove(0).text } else { String::new() };
    (lead, v)
}

/// `parse_with_lead`'in tersi.
pub fn build_with_lead(lead: &str, syls: &[Syl]) -> String {
    format!("{}{}", lead, build(syls))
}

/// Hecelerden metin üretir (`{\kfNN<diğer etiketler>}metin`).
pub fn build(syls: &[Syl]) -> String {
    let mut out = String::new();
    for s in syls {
        // Hecenin başındaki etiket bloğunu \k ile birleştir.
        if s.text.starts_with('{') {
            if let Some(e) = s.text.find('}') {
                out.push_str(&format!("{{\\{}{}{}}}", s.kind, s.dur, &s.text[1..e]));
                out.push_str(&s.text[e + 1..]);
                continue;
            }
        }
        out.push_str(&format!("{{\\{}{}}}{}", s.kind, s.dur, s.text));
    }
    out
}

/// Satırdaki tüm hecelerin karaoke türünü değiştirir (`k`, `kf`, `ko`); baştaki etiketler ve
/// hece içi etiketler korunur. Karaoke değilse metin aynen döner.
pub fn set_kind(text: &str, kind: &str) -> String {
    let (lead, syls) = parse_with_lead(text);
    if syls.is_empty() || !matches!(kind, "k" | "kf" | "ko") {
        return text.to_string();
    }
    let v: Vec<Syl> = syls.into_iter().map(|mut s| { s.kind = kind.to_string(); s }).collect();
    build_with_lead(&lead, &v)
}

pub fn total(syls: &[Syl]) -> i64 {
    syls.iter().map(|s| s.dur).sum()
}

/// Hece `i`'yi görünür karakter konumunda böler (etiketsiz heceler için); süre karakter oranında paylaşılır.
pub fn split(syls: &mut Vec<Syl>, i: usize, char_pos: usize) -> bool {
    let Some(s) = syls.get(i) else { return false };
    if !s.plain() {
        return false;
    }
    let chars: Vec<char> = s.text.chars().collect();
    if char_pos == 0 || char_pos >= chars.len() {
        return false;
    }
    let left: String = chars[..char_pos].iter().collect();
    let right: String = chars[char_pos..].iter().collect();
    let d1 = ((s.dur as f64) * char_pos as f64 / chars.len() as f64).round() as i64;
    let (kind, dur) = (s.kind.clone(), s.dur);
    syls[i] = Syl { dur: d1, kind: kind.clone(), text: left };
    syls.insert(i + 1, Syl { dur: dur - d1, kind, text: right });
    true
}

/// Hece `i` ile `i+1`'i birleştirir.
pub fn join(syls: &mut Vec<Syl>, i: usize) -> bool {
    if i + 1 >= syls.len() {
        return false;
    }
    let n = syls.remove(i + 1);
    syls[i].dur += n.dur;
    syls[i].text.push_str(&n.text);
    true
}

/// `i` ile `i+1` arasındaki sınırı `delta` cs kaydırır (toplam değişmez).
pub fn move_boundary(syls: &mut [Syl], i: usize, delta: i64) {
    if i + 1 >= syls.len() {
        return;
    }
    let d = delta.clamp(-syls[i].dur, syls[i + 1].dur);
    syls[i].dur += d;
    syls[i + 1].dur -= d;
}

fn weight(s: &Syl) -> f64 {
    let v = s.visible();
    let letters = v.chars().filter(|c| !c.is_whitespace()).count() as f64;
    let spaces = v.chars().filter(|c| c.is_whitespace()).count() as f64;
    (letters + spaces * 0.3).max(0.0)
}

/// Toplam süreyi hecelere harf sayısına orantılı dağıtır (boş heceler 0 alır).
pub fn distribute(syls: &mut [Syl], total_cs: i64) {
    let w: Vec<f64> = syls.iter().map(weight).collect();
    let sum: f64 = w.iter().sum();
    if sum <= 0.0 || syls.is_empty() {
        return;
    }
    let mut acc = 0.0;
    let mut given = 0;
    for (i, s) in syls.iter_mut().enumerate() {
        acc += w[i];
        let upto = (total_cs as f64 * acc / sum).round() as i64;
        s.dur = upto - given;
        given = upto;
    }
}

/// Tuşla zamanlama: `taps_ms[i]` i. hecenin başlangıcı (satır başına göre, ms). Her hece bir sonrakinin
/// başlangıcına, sonuncusu satır sonuna (`line_cs`) kadar sürer; ilk dokunuştan önceki boşluk
/// (≥ 5 cs) boş bir başlangıç hecesi olur. Dokunulmayan heceler 0 süre alır.
pub fn apply_taps(syls: &[Syl], taps_ms: &[i64], line_cs: i64) -> Vec<Syl> {
    let n = syls.len().min(taps_ms.len());
    let mut out: Vec<Syl> = syls.to_vec();
    if n == 0 {
        return out;
    }
    let starts: Vec<i64> = taps_ms.iter().map(|ms| ms / 10).collect();
    for i in 0..n {
        let end = if i + 1 < n { starts[i + 1] } else { line_cs };
        out[i].dur = (end - starts[i]).max(0);
    }
    for s in out.iter_mut().skip(n) {
        s.dur = 0;
    }
    if starts[0] >= 5 {
        let kind = out[0].kind.clone();
        out.insert(0, Syl { dur: starts[0], kind, text: String::new() });
    }
    out
}

/// Mevcut oranları koruyarak toplam süreyi değiştirir.
pub fn scale_to(syls: &mut [Syl], total_cs: i64) {
    let cur = total(syls);
    if cur <= 0 {
        return distribute(syls, total_cs);
    }
    let mut acc = 0i64;
    let mut given = 0;
    for s in syls.iter_mut() {
        acc += s.dur;
        let upto = (total_cs as f64 * acc as f64 / cur as f64).round() as i64;
        s.dur = upto - given;
        given = upto;
    }
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{31f0}'..='\u{31ff}' | '\u{ff66}'..='\u{ff9f}')
}

pub(crate) fn is_cjk(c: char) -> bool {
    is_kana(c) || matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{f900}'..='\u{faff}')
}

/// Kendinden önceki moraya yapışan küçük kana ve uzatma işareti.
fn kana_attaches(c: char) -> bool {
    matches!(c, 'ゃ' | 'ゅ' | 'ょ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ゎ' | 'ャ' | 'ュ' | 'ョ' | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ヮ' | 'ー' | '〜' | '～')
}

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u') || matches!(c, 'ā' | 'ē' | 'ī' | 'ō' | 'ū' | 'Ā' | 'Ē' | 'Ī' | 'Ō' | 'Ū' | 'â' | 'ê' | 'î' | 'ô' | 'û')
}

/// Japonca şarkı sözünü karaoke hecelerine (mora) böler. Kana/kanji varsa karakter tabanlı
/// (küçük kana ve ー önceki moraya yapışır), yoksa romaji kuralları: ünsüz(ler)+ünlü, tek başına
/// `n`, ikiz ünsüzün ilki (sokuon). Boşluk ve noktalama önceki heceye eklenir.
pub fn auto_syllables(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
    let mut out: Vec<String> = vec![];
    let push_attach = |out: &mut Vec<String>, c: char| match out.last_mut() {
        Some(l) => l.push(c),
        None => out.push(c.to_string()),
    };
    if chars.iter().any(|&c| is_cjk(c)) {
        for &c in &chars {
            if kana_attaches(c) || !(is_cjk(c) || c.is_alphanumeric()) {
                push_attach(&mut out, c);
            } else {
                out.push(c.to_string());
            }
        }
        return out;
    }
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if !c.is_alphabetic() {
            // boşluk/noktalama: bekleyen ünsüzleri kapat, sonra önceki heceye yapıştır
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            push_attach(&mut out, c);
            i += 1;
            continue;
        }
        if is_vowel(c) {
            cur.push(c);
            out.push(std::mem::take(&mut cur));
            i += 1;
            continue;
        }
        let lc = c.to_ascii_lowercase();
        // tek başına n: ardından ünlü ya da 'y' gelmiyorsa
        if lc == 'n' && cur.is_empty() && !next.map_or(false, |n| is_vowel(n) || n.eq_ignore_ascii_case(&'y')) {
            out.push(c.to_string());
            i += 1;
            continue;
        }
        // ikiz ünsüz (kk, tt, ss, pp, cch): ilki ayrı mora
        if cur.is_empty() && next.map_or(false, |n| n.eq_ignore_ascii_case(&c) || (lc == 't' && n.eq_ignore_ascii_case(&'c'))) && lc != 'n' {
            out.push(c.to_string());
            i += 1;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        // ünlüsüz kalan son ünsüzler (ör. İngilizce sözcük sonu) önceki heceye eklenir
        match out.last_mut() {
            Some(l) if !l.ends_with(' ') => l.push_str(&cur),
            _ => out.push(cur),
        }
    }
    out
}

/// Parçaları eşit süreli `\kf` hecelerine çevirir (kalan santisaniyeler baştan dağıtılır).
pub fn equal_syllables(parts: &[String], total_cs: i64, kind: &str) -> Vec<Syl> {
    let n = parts.len() as i64;
    if n == 0 {
        return vec![];
    }
    let base = total_cs.max(0) / n;
    let rem = total_cs.max(0) % n;
    parts.iter().enumerate().map(|(i, t)| Syl { dur: base + if (i as i64) < rem { 1 } else { 0 }, kind: kind.to_string(), text: t.clone() }).collect()
}

#[cfg(test)]
mod set_kind_tests {
    use super::*;
    #[test]
    fn tur_degistir() {
        assert_eq!(set_kind("{\\an8}{\\k20}ki{\\K30\\1c&HFF&}mi", "ko"), "{\\an8}{\\ko20}ki{\\ko30\\1c&HFF&}mi");
        assert_eq!(set_kind("düz metin", "kf"), "düz metin");
        assert_eq!(set_kind("{\\k5}a", "x"), "{\\k5}a");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_build_roundtrip() {
        let t = "{\\an8\\k20}ka{\\k30\\fs50}ra{\\kf40}o ke";
        let s = parse(t);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].dur, s[0].kind.as_str(), s[0].text.as_str()), (20, "k", "{\\an8}ka"));
        assert_eq!(s[1].text, "{\\fs50}ra");
        assert_eq!(build(&s), "{\\k20\\an8}ka{\\k30\\fs50}ra{\\kf40}o ke");
        assert_eq!(parse(&build(&s)), s);
    }

    #[test]
    fn split_join_distribute() {
        let mut s = parse("{\\k100}karaoke");
        assert!(split(&mut s, 0, 4));
        assert_eq!((s[0].text.as_str(), s[0].dur, s[1].dur), ("kara", 57, 43));
        assert!(join(&mut s, 0));
        assert_eq!(s[0].text, "karaoke");
        let mut s = parse("{\\k0}a{\\k0}bb{\\k0}ccc");
        distribute(&mut s, 60);
        assert_eq!(s.iter().map(|x| x.dur).collect::<Vec<_>>(), vec![10, 20, 30]);
        scale_to(&mut s, 120);
        assert_eq!(total(&s), 120);
        move_boundary(&mut s, 0, 5);
        assert_eq!((s[0].dur, s[1].dur), (25, 35));
    }

    #[test]
    fn taps() {
        let s = parse("{\\k0}ka{\\k0}ra{\\k0}o");
        let r = apply_taps(&s, &[500, 800, 1500], 300);
        assert_eq!(r.iter().map(|x| x.dur).collect::<Vec<_>>(), vec![50, 30, 70, 150]);
        assert_eq!(r[0].text, "");
        assert_eq!(build(&r), "{\\kf50}{\\k30}ka{\\k70}ra{\\k150}o".replace("kf50", "k50"));
        // eksik dokunuş
        let r = apply_taps(&s, &[0, 100], 300);
        assert_eq!(r.iter().map(|x| x.dur).collect::<Vec<_>>(), vec![10, 290, 0]);
    }

    #[test]
    fn leading_plain_text() {
        let s = parse("giriş {\\k10}a");
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].dur, s[0].text.as_str()), (0, "giriş "));
    }

    #[test]
    fn lead_detection() {
        assert!(!has_lead("{\\k20}ka{\\k30}ra"));
        assert!(has_lead("pre{\\k20}ka"));
        assert!(has_lead("{\\an8}{\\k20}ka"));
        assert!(!has_lead("{\\an8\\k20}ka"));
    }

    #[test]
    fn romaji_morae() {
        let j = |s: &str| auto_syllables(s).join("|");
        assert_eq!(j("kimi no na wa"), "ki|mi |no |na |wa");
        assert_eq!(j("kitto"), "ki|t|to");
        assert_eq!(j("shinjitsu"), "shi|n|ji|tsu");
        assert_eq!(j("kon'nichiwa"), "ko|n'|ni|chi|wa");
        assert_eq!(j("kyou mo"), "kyo|u |mo");
        assert_eq!(j("matcha"), "ma|t|cha");
        assert_eq!(j("Sayonara!"), "Sa|yo|na|ra!");
        assert_eq!(j("san"), "sa|n");
        assert_eq!(j("onna"), "o|n|na");
    }

    #[test]
    fn kana_morae() {
        let j = |s: &str| auto_syllables(s).join("|");
        assert_eq!(j("きょうも"), "きょ|う|も");
        assert_eq!(j("ちょっと待って"), "ちょ|っ|と|待|っ|て");
        assert_eq!(j("ラーメン、好き"), "ラー|メ|ン、|好|き");
    }

    #[test]
    fn equal_split() {
        let v = equal_syllables(&["a".into(), "b".into(), "c".into()], 100, "kf");
        assert_eq!(v.iter().map(|s| s.dur).collect::<Vec<_>>(), vec![34, 33, 33]);
        assert_eq!(build(&v), "{\\kf34}a{\\kf33}b{\\kf33}c");
    }

    #[test]
    fn lead_roundtrip_and_boundary_drag() {
        let t = "{\\an8}{\\k20}ka{\\k30}ra{\\k50}te";
        let (lead, mut v) = parse_with_lead(t);
        assert_eq!(lead, "{\\an8}");
        assert_eq!(v.len(), 3);
        assert_eq!(build_with_lead(&lead, &v), t);
        move_boundary(&mut v, 0, 5);
        assert_eq!(build_with_lead(&lead, &v), "{\\an8}{\\k25}ka{\\k25}ra{\\k50}te");
        let (l2, v2) = parse_with_lead("{\\k10}a");
        assert_eq!((l2.as_str(), v2.len()), ("", 1));
    }
}

/// Hece sınırlarını sesteki hece başlangıçlarına (onset) oturtur. `onset[i]`: satır başından
/// i. santisaniyedeki başlangıç gücü (ör. spektral akı, `osisub_media::onset_envelope`).
/// Her sınır mevcut konumunun ±`window_cs` içinde, güçlü bir başlangıca doğru kayar; uzaklık
/// küçük bir cezayla dengelenir (yakında başlangıç yoksa sınır yerinde kalır). Sınırlar sıralı kalır,
/// her hece en az 3 cs sürer; 0 süreli heceler (boş başlangıç vb.) olduğu gibi kalır. Dinamik
/// programlama: tüm sınırlar birlikte en iyi şekilde seçilir.
pub fn snap_to_onsets(syls: &[Syl], onset: &[f32], window_cs: i64) -> Vec<Syl> {
    const MIN: i64 = 3;
    let total = total(syls);
    // süreli hecelerin indeksleri ve aralarındaki sınırlar (satır başına göre cs)
    let idx: Vec<usize> = (0..syls.len()).filter(|&i| syls[i].dur > 0).collect();
    if idx.len() < 2 || onset.is_empty() {
        return syls.to_vec();
    }
    let mut prior = Vec::new();
    let mut acc = 0;
    for (i, s) in syls.iter().enumerate() {
        if s.dur > 0 && i != idx[0] {
            prior.push(acc);
        }
        acc += s.dur;
    }
    let maxv = onset.iter().cloned().fold(0.0f32, f32::max);
    if maxv <= 0.0 {
        return syls.to_vec();
    }
    let w = window_cs.max(1);
    let val = |p: i64| onset.get(p as usize).copied().unwrap_or(0.0) / maxv;
    let score = |p: i64, pr: i64| val(p) - 0.15 * (p - pr).abs() as f32 / w as f32;
    // ilk süreli hecenin başı ve son süreli hecenin sonu sabit
    let first_start: i64 = syls[..idx[0]].iter().map(|s| s.dur).sum();
    let end = total;
    let cands: Vec<Vec<i64>> = prior.iter().map(|&p| ((p - w).max(first_start + MIN)..=(p + w).min(end - MIN)).collect()).collect();
    if cands.iter().any(|c| c.is_empty()) {
        return syls.to_vec();
    }
    // dp[j][k]: j. sınır k. adayda iken en iyi toplam; geri izleme
    let n = prior.len();
    let mut dp: Vec<Vec<f32>> = Vec::with_capacity(n);
    let mut back: Vec<Vec<usize>> = Vec::with_capacity(n);
    for j in 0..n {
        let mut row = vec![f32::NEG_INFINITY; cands[j].len()];
        let mut brow = vec![0usize; cands[j].len()];
        for (k, &p) in cands[j].iter().enumerate() {
            let s = score(p, prior[j]);
            if j == 0 {
                if p - first_start >= MIN {
                    row[k] = s;
                }
            } else {
                let mut best = (f32::NEG_INFINITY, 0);
                for (q, &pp) in cands[j - 1].iter().enumerate() {
                    if p - pp >= MIN && dp[j - 1][q] > best.0 {
                        best = (dp[j - 1][q], q);
                    }
                }
                if best.0 > f32::NEG_INFINITY {
                    row[k] = best.0 + s;
                    brow[k] = best.1;
                }
            }
            if j == n - 1 && end - p < MIN {
                row[k] = f32::NEG_INFINITY;
            }
        }
        dp.push(row);
        back.push(brow);
    }
    let Some((mut k, _)) = dp[n - 1].iter().enumerate().filter(|(_, v)| v.is_finite()).max_by(|a, b| a.1.total_cmp(b.1)) else {
        return syls.to_vec();
    };
    let mut pos = vec![0i64; n];
    for j in (0..n).rev() {
        pos[j] = cands[j][k];
        k = back[j][k];
    }
    // sınırlardan süreler
    let mut out = syls.to_vec();
    let mut bounds = vec![first_start];
    bounds.extend(pos);
    bounds.push(end);
    for (m, &i) in idx.iter().enumerate() {
        out[i].dur = bounds[m + 1] - bounds[m];
    }
    out
}

#[cfg(test)]
mod snap_tests {
    use super::*;

    fn syl(d: i64, t: &str) -> Syl {
        Syl { dur: d, kind: "kf".into(), text: t.into() }
    }

    #[test]
    fn moves_boundaries_to_nearby_onsets() {
        // gerçek başlangıçlar 0, 31, 72, 105 cs; ilk tahmin eşit (30'ar)
        let mut on = vec![0.05f32; 140];
        for (p, v) in [(31, 1.0), (72, 0.8), (105, 0.9), (50, 0.2)] {
            on[p] = v;
        }
        let s = vec![syl(0, ""), syl(30, "ka"), syl(30, "ze "), syl(30, "no"), syl(50, " u")];
        let r = snap_to_onsets(&s, &on, 20);
        assert_eq!(r.iter().map(|s| s.dur).collect::<Vec<_>>(), vec![0, 31, 41, 33, 35]);
        assert_eq!(total(&r), total(&s));
    }

    #[test]
    fn far_or_absent_onsets_keep_positions() {
        let mut on = vec![0.0f32; 100];
        on[90] = 1.0; // pencerenin dışında (75±10)
        let s = vec![syl(25, "a"), syl(25, "b"), syl(25, "c"), syl(25, "d")];
        let r = snap_to_onsets(&s, &on, 10);
        assert_eq!(r.iter().map(|s| s.dur).collect::<Vec<_>>(), vec![25, 25, 25, 25]);
        assert_eq!(snap_to_onsets(&s, &[], 10), s);
    }

    #[test]
    fn keeps_order_and_minimum_length() {
        // iki sınır da aynı güçlü başlangıca çekilmek ister: biri en az 3 cs ayrı kalmalı
        let mut on = vec![0.0f32; 60];
        on[20] = 1.0;
        let s = vec![syl(18, "a"), syl(4, "b"), syl(38, "c")];
        let r = snap_to_onsets(&s, &on, 10);
        let d: Vec<i64> = r.iter().map(|s| s.dur).collect();
        assert!(d.iter().all(|&x| x >= 3), "{d:?}");
        assert_eq!(d.iter().sum::<i64>(), 60);
        assert!(d[0] == 20 || d[0] + d[1] == 20, "{d:?}");
    }
}

/// Tuşla zamanlamada kalan sistematik gecikme: her dokunuşun (satıra göre ms) ±300 ms içindeki en
/// güçlü ses başlangıcına uzaklığının ortancası (dokunuş − başlangıç; pozitif = geç basılmış).
/// Güvenilir eşleşme 3'ten azsa None. `onset`: santisaniye başına güç (`snap_to_onsets` gibi).
pub fn tap_offset_ms(taps_ms: &[i64], onset: &[f32]) -> Option<i64> {
    let maxv = onset.iter().cloned().fold(0.0f32, f32::max);
    if maxv <= 0.0 {
        return None;
    }
    let mut d: Vec<i64> = taps_ms
        .iter()
        .filter_map(|&t| {
            let c = t / 10;
            let (a, b) = ((c - 30).max(0) as usize, ((c + 30).max(0) as usize).min(onset.len().saturating_sub(1)));
            if a > b {
                return None;
            }
            let (k, v) = (a..=b).map(|k| (k, onset[k])).max_by(|x, y| x.1.total_cmp(&y.1))?;
            (v >= 0.3 * maxv).then_some(t - k as i64 * 10)
        })
        .collect();
    if d.len() < 3 {
        return None;
    }
    d.sort();
    Some(d[d.len() / 2])
}

#[cfg(test)]
mod tap_offset_tests {
    #[test]
    fn median_of_tap_to_onset_distances() {
        let mut on = vec![0.0f32; 300];
        for p in [20, 80, 150, 210, 260] {
            on[p] = 1.0;
        }
        // her dokunuş ~120 ms geç, biri gürültülü (+300: eşleşmez/aykırı)
        let taps = [320, 920, 1610, 2230, 2600 + 290];
        assert_eq!(super::tap_offset_ms(&taps, &on), Some(120));
        assert_eq!(super::tap_offset_ms(&taps[..2], &on), None);
        assert_eq!(super::tap_offset_ms(&taps, &[]), None);
    }
}
