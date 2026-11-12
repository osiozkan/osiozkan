//! Etiket uygulama ve yeniden hesaplama (Aegisub tipografi betikleri HYDRA ve Recalculator'ın
//! temel işlevleri): seçili satırlara etiketleri başlangıç bloğuna, tüm bloklara, `\t` olarak,
//! harf harf ya da satırlar boyunca degrade olarak uygulamak; sayısal etiket değerlerini toplu
//! olarak toplamak/çarpmak (`\t` içleri ve `\pos/\move/\org` koordinatları dahil).
use crate::overrides::{self, Seg, Tag};
use crate::typeset::{interp_tag, tag_list};

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    /// başlangıç bloğuna (varsa değiştir)
    Start,
    /// başlangıç bloğuna ve etiketin geçtiği diğer tüm bloklara
    AllBlocks,
    /// başlangıç bloğuna `\t(t1,t2,ivme,etiketler)`; t1 = t2 = 0 → `\t(etiketler)`
    Transform { t1: i64, t2: i64, accel: f64 },
    /// harf harf degrade: ilk harf `tags`, son harf `to` (boşluklar atlanır, `\N` sayılmaz)
    ByChar { to: String, accel: f64 },
}

/// Eşanlamlı etiket adları: `\c` = `\1c`, `\fr` = `\frz`.
fn canon(name: &str) -> &str {
    match name {
        "c" => "1c",
        "fr" => "frz",
        n => n,
    }
}

fn same(a: &str, b: &str) -> bool {
    canon(a) == canon(b)
}

fn set_in_block(tags: &mut Vec<Tag>, t: &Tag) {
    // eşanlamlısı dahil hepsini kaldırıp sona ekle (son etiket geçerlidir)
    tags.retain(|x| !same(&x.name, &t.name));
    tags.push(t.clone());
}

/// Bir satır metnine etiketleri uygular. `tags`: `\bord2\c&H0000FF&` gibi (süslü parantezli ya da değil).
pub fn apply(text: &str, tags: &str, mode: &Mode) -> String {
    let list = tag_list(tags);
    if list.is_empty() && !matches!(mode, Mode::ByChar { .. }) {
        return text.to_string();
    }
    let mut segs = overrides::parse(text);
    let ensure_start = |segs: &mut Vec<Seg>| {
        if !matches!(segs.first(), Some(Seg::Block(_))) {
            segs.insert(0, Seg::Block(vec![]));
        }
    };
    match mode {
        Mode::Start | Mode::AllBlocks => {
            ensure_start(&mut segs);
            for (i, g) in segs.iter_mut().enumerate() {
                let Seg::Block(b) = g else { continue };
                for t in &list {
                    if i == 0 || b.iter().any(|x| same(&x.name, &t.name)) && matches!(mode, Mode::AllBlocks) {
                        set_in_block(b, t);
                    }
                }
            }
        }
        Mode::Transform { t1, t2, accel } => {
            ensure_start(&mut segs);
            let body = overrides::block_to_string(&list);
            let arg = match (*t1, *t2, *accel) {
                (0, 0, a) if a == 1.0 => format!("({body})"),
                (0, 0, a) => format!("({},{body})", num(a)),
                (a, b, c) if c == 1.0 => format!("({a},{b},{body})"),
                (a, b, c) => format!("({a},{b},{},{body})", num(c)),
            };
            if let Some(Seg::Block(b)) = segs.first_mut() {
                b.push(Tag::new("t", &arg));
            }
        }
        Mode::ByChar { to, accel } => return by_char(text, &list, &tag_list(to), *accel),
    }
    overrides::to_string(&segs)
}

fn num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Metin parçasını görünür birimlere böler: `\N`/`\n` (sayılmaz), `\h` ve tek karakterler.
fn units(t: &str) -> Vec<(String, bool)> {
    let mut out = vec![];
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&n) = it.peek() {
                if n == 'N' || n == 'n' {
                    it.next();
                    out.push((format!("\\{n}"), false));
                    continue;
                }
                if n == 'h' {
                    it.next();
                    out.push(("\\h".into(), false));
                    continue;
                }
            }
        }
        out.push((c.to_string(), !c.is_whitespace()));
    }
    out
}

fn by_char(text: &str, from: &[Tag], to: &[Tag], accel: f64) -> String {
    // her iki uçta da bulunan ve enterpole edilebilen etiketler
    let pairs: Vec<(&Tag, &Tag)> = from.iter().filter_map(|a| to.iter().rev().find(|b| same(&a.name, &b.name)).map(|b| (a, b))).filter(|(a, b)| interp_tag(canon(&a.name), &a.arg, &b.arg, 0.5).is_some()).collect();
    let segs = overrides::parse(text);
    let total: usize = segs.iter().map(|g| match g { Seg::Text(t) => units(t).iter().filter(|u| u.1).count(), _ => 0 }).sum();
    if pairs.is_empty() || total == 0 {
        return text.to_string();
    }
    let mut out: Vec<Seg> = vec![];
    let mut k = 0usize;
    for g in segs {
        match g {
            Seg::Block(b) => out.push(Seg::Block(b)),
            Seg::Text(t) => {
                for (u, counts) in units(&t) {
                    if counts {
                        let x = if total > 1 { k as f64 / (total - 1) as f64 } else { 0.0 };
                        let x = x.powf(accel.max(0.01));
                        let new: Vec<Tag> = pairs.iter().filter_map(|(a, b)| interp_tag(canon(&a.name), &a.arg, &b.arg, x).map(|v| Tag::new(canon(&a.name), &v))).collect();
                        // hemen önceki blokla birleştir (önceki blok kendi değerlerini korur, bizimkiler sonra gelir)
                        match out.last_mut() {
                            Some(Seg::Block(prev)) => {
                                for t in &new {
                                    set_in_block(prev, t);
                                }
                            }
                            _ => out.push(Seg::Block(new)),
                        }
                        k += 1;
                    }
                    match out.last_mut() {
                        Some(Seg::Text(s)) => s.push_str(&u),
                        _ => out.push(Seg::Text(u)),
                    }
                }
            }
        }
    }
    overrides::to_string(&out)
}

/// Satırlar boyunca degrade: `n` satırın i'incisine `from` → `to` arası değerler başlangıç bloğuna yazılır.
pub fn by_line(texts: &[String], from: &str, to: &str, accel: f64) -> Vec<String> {
    let (fa, fb) = (tag_list(from), tag_list(to));
    let n = texts.len();
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let x = if n > 1 { (i as f64 / (n - 1) as f64).powf(accel.max(0.01)) } else { 0.0 };
            let mut s = t.clone();
            for a in &fa {
                let v = fb.iter().rev().find(|b| same(&a.name, &b.name)).and_then(|b| interp_tag(canon(&a.name), &a.arg, &b.arg, x)).unwrap_or_else(|| a.arg.clone());
                s = apply(&s, &format!("\\{}{}", canon(&a.name), v), &Mode::Start);
            }
            s
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add(f64),
    Mul(f64),
}

impl Op {
    fn on(self, v: f64) -> f64 {
        match self {
            Op::Add(a) => v + a,
            Op::Mul(m) => v * m,
        }
    }
}

/// Yeniden hesaplama (Recalculator): `name` etiketinin tüm görünümlerinde (başlangıç ve satır içi
/// bloklar, `\t` içleri) değeri `op` ile değiştirir. `\pos/\org/\move` için `op` x'e, `op_y` y'ye
/// uygulanır (`\move` zamanları değişmez). Etiket yoksa ve `default` verilmişse başlangıç bloğuna
/// `op(default)` yazılır (ör. stilin değeri). Değişen etiket sayısını da döndürür.
pub fn recalc(text: &str, name: &str, op: Op, op_y: Op, default: Option<f64>) -> (String, usize) {
    let mut segs = overrides::parse(text);
    let mut n = 0;
    for g in segs.iter_mut() {
        if let Seg::Block(b) = g {
            n += recalc_block(b, name, op, op_y);
        }
    }
    let mut out = overrides::to_string(&segs);
    if n == 0 {
        if let Some(d) = default {
            if !matches!(name, "pos" | "org" | "move") {
                out = overrides::set_initial(&out, canon(name), &num(op.on(d)));
                n = 1;
            }
        }
    }
    (out, n)
}

fn recalc_block(b: &mut [Tag], name: &str, op: Op, op_y: Op) -> usize {
    let mut n = 0;
    for t in b.iter_mut() {
        if t.name == "t" {
            // \t(...,etiketler): son argümanın içindeki etiketler
            let a = t.args();
            if let Some(last) = a.last() {
                let mut inner = overrides::parse_block(last);
                let k = recalc_block(&mut inner, name, op, op_y);
                if k > 0 {
                    let mut parts: Vec<String> = a[..a.len() - 1].to_vec();
                    parts.push(overrides::block_to_string(&inner));
                    t.arg = format!("({})", parts.join(","));
                    n += k;
                }
            }
            continue;
        }
        if !same(&t.name, name) {
            continue;
        }
        match canon(name) {
            "pos" | "org" | "move" => {
                let a = t.args();
                let mut v: Vec<String> = a.clone();
                let coords = if canon(name) == "move" { 4.min(a.len()) } else { 2.min(a.len()) };
                for i in 0..coords {
                    if let Ok(x) = a[i].trim().parse::<f64>() {
                        v[i] = num(if i % 2 == 0 { op.on(x) } else { op_y.on(x) });
                    }
                }
                t.arg = format!("({})", v.join(","));
                n += 1;
            }
            _ => {
                if let Some(x) = t.number() {
                    t.arg = num(op.on(x));
                    n += 1;
                }
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_all_blocks() {
        assert_eq!(apply("Merhaba", "\\bord3\\c&H0000FF&", &Mode::Start), "{\\bord3\\1c&H0000FF&}Merhaba".replace("\\1c", "\\c"));
        // eşanlamlı: \1c varken \c uygulanınca ikisi birden kalmaz
        assert_eq!(apply("{\\1c&HFFFFFF&\\blur1}a", "{\\c&H000000&}", &Mode::Start), "{\\blur1\\c&H000000&}a");
        // tüm bloklar: yalnızca etiketin geçtiği bloklarda değiştirilir
        let t = "{\\bord1}a{\\bord5\\i1}b{\\i0}c";
        assert_eq!(apply(t, "\\bord2", &Mode::AllBlocks), "{\\bord2}a{\\i1\\bord2}b{\\i0}c");
        assert_eq!(apply(t, "\\bord2", &Mode::Start), "{\\bord2}a{\\bord5\\i1}b{\\i0}c");
    }

    #[test]
    fn transform() {
        assert_eq!(apply("{\\pos(1,2)}a", "\\fscx120\\blur3", &Mode::Transform { t1: 0, t2: 500, accel: 1.0 }), "{\\pos(1,2)\\t(0,500,\\fscx120\\blur3)}a");
        assert_eq!(apply("a", "\\alpha&HFF&", &Mode::Transform { t1: 0, t2: 0, accel: 2.0 }), "{\\t(2,\\alpha&HFF&)}a");
    }

    #[test]
    fn char_gradient() {
        let r = apply("{\\b1}ab c\\Nd", "\\c&H000000&\\bord0", &Mode::ByChar { to: "\\c&HFFFFFF&\\bord3".into(), accel: 1.0 });
        // 4 harf: a, b, c, d (boşluk ve \N sayılmaz) → 0, 1/3, 2/3, 1
        assert_eq!(r, "{\\b1\\1c&H000000&\\bord0}a{\\1c&H555555&\\bord1}b {\\1c&HAAAAAA&\\bord2}c\\N{\\1c&HFFFFFF&\\bord3}d");
        // ara etiketler korunur
        let r = apply("a{\\i1}b", "\\fscx100", &Mode::ByChar { to: "\\fscx200".into(), accel: 1.0 });
        assert_eq!(r, "{\\fscx100}a{\\i1\\fscx200}b");
        // enterpole edilemeyen etiket → değişmez
        assert_eq!(apply("ab", "\\fnArial", &Mode::ByChar { to: "\\fnTimes".into(), accel: 1.0 }), "ab");
    }

    #[test]
    fn line_gradient() {
        let v = by_line(&["x".into(), "{\\pos(1,1)}y".into(), "z".into()], "\\alpha&H00&\\blur1", "\\alpha&HFF&", 1.0);
        assert_eq!(v, vec!["{\\alpha&H00&\\blur1}x", "{\\pos(1,1)\\alpha&H80&\\blur1}y", "{\\alpha&HFF&\\blur1}z"]);
    }

    #[test]
    fn recalculator() {
        let t = "{\\fscx120\\t(0,500,\\fscx150\\blur2)\\pos(100,200)}a{\\fscx80}b";
        let (r, n) = recalc(t, "fscx", Op::Mul(0.5), Op::Mul(0.5), None);
        assert_eq!((r.as_str(), n), ("{\\fscx60\\t(0,500,\\fscx75\\blur2)\\pos(100,200)}a{\\fscx40}b", 3));
        let (r, _) = recalc(t, "pos", Op::Add(10.0), Op::Add(-20.0), None);
        assert!(r.contains("\\pos(110,180)"), "{r}");
        let (r, _) = recalc("{\\move(0,0,100,100,0,500)}a", "move", Op::Mul(1.5), Op::Mul(2.0), None);
        assert_eq!(r, "{\\move(0,0,150,200,0,500)}a");
        // yoksa varsayılan (stil) değerinden
        assert_eq!(recalc("a", "bord", Op::Add(1.0), Op::Add(1.0), Some(2.0)), ("{\\bord3}a".into(), 1));
        assert_eq!(recalc("a", "bord", Op::Add(1.0), Op::Add(1.0), None).1, 0);
        // \fr = \frz
        assert_eq!(recalc("{\\fr10}a", "frz", Op::Add(5.0), Op::Add(0.0), None).0, "{\\fr15}a");
    }
}
