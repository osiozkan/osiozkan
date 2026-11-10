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

fn is_cjk(c: char) -> bool {
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
