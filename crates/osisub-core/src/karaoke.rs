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
    fn leading_plain_text() {
        let s = parse("giriş {\\k10}a");
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].dur, s[0].text.as_str()), (0, "giriş "));
    }
}
