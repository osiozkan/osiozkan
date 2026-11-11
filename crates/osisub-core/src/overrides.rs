//! ASS geçersiz kılma etiketlerinin yapısal ayrıştırması, değişmeden geri yazımı ve temizliği.

/// Tek etiket: `name` ters bölüsüz ad (`pos`, `fscx`, `1c` ...), `arg` ham argüman (parantezler dahil).
/// `name` boşsa `arg` etiket olmayan serbest metindir (yorum).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub arg: String,
}

impl Tag {
    pub fn new(name: &str, arg: &str) -> Tag {
        Tag { name: name.into(), arg: arg.into() }
    }
    pub fn to_ass(&self) -> String {
        if self.name.is_empty() { self.arg.clone() } else { format!("\\{}{}", self.name, self.arg) }
    }
    pub fn number(&self) -> Option<f64> {
        self.arg.trim().parse().ok()
    }
    /// `(a,b,c)` biçimindeki argümanların listesi (en üst düzey virgüllere göre).
    pub fn args(&self) -> Vec<String> {
        let a = self.arg.trim();
        let inner = a.strip_prefix('(').map(|s| s.strip_suffix(')').unwrap_or(s)).unwrap_or(a);
        split_top(inner, ',')
    }
}

fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' => { depth += 1; cur.push(c); }
            ')' => { depth -= 1; cur.push(c); }
            c if c == sep && depth == 0 => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Uzun öneklerden kısalara doğru eşleşmesi için sıralı etiket adları.
const NAMES: &[&str] = &[
    "fscx", "fscy", "fsp", "fax", "fay", "frx", "fry", "frz", "fad", "fade", "fn", "fs", "fr", "fe", "xbord", "ybord", "bord", "xshad", "yshad", "shad",
    "blur", "be", "alpha", "iclip", "clip", "pos", "move", "org", "an", "pbo", "kf", "ko", "kt", "k", "K", "q", "p", "r", "t", "1a", "2a", "3a", "4a", "1c", "2c",
    "3c", "4c", "a", "c", "i", "b", "u", "s",
];

fn match_name(s: &str) -> &str {
    let mut best = "";
    for n in NAMES {
        if s.starts_with(n) && n.len() > best.len() {
            best = n;
        }
    }
    // \fade'i \fad'dan, \fscx'i \fs'den ayırmak için uzun ad önceliklidir (yukarıdaki döngü). `\fn` argümanı serbest metindir.
    best
}

/// Bir `{...}` bloğunun içini etiketlere ayırır.
pub fn parse_block(block: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    let b: Vec<char> = block.chars().collect();
    let mut i = 0;
    let mut lead = String::new();
    while i < b.len() && b[i] != '\\' {
        lead.push(b[i]);
        i += 1;
    }
    if !lead.is_empty() {
        tags.push(Tag { name: String::new(), arg: lead });
    }
    while i < b.len() {
        // b[i] == '\\'
        let start = i + 1;
        let mut j = start;
        let mut depth = 0;
        while j < b.len() {
            match b[j] {
                '(' => depth += 1,
                ')' => depth -= 1,
                '\\' if depth <= 0 => break,
                _ => {}
            }
            j += 1;
        }
        let body: String = b[start..j].iter().collect();
        let name = match_name(&body).to_string();
        let arg = body[name.len()..].to_string();
        if name.is_empty() {
            // Tanınmayan etiket: ham koru.
            tags.push(Tag { name: String::new(), arg: format!("\\{}", body) });
        } else {
            tags.push(Tag { name, arg });
        }
        i = j;
    }
    tags
}

pub fn block_to_string(tags: &[Tag]) -> String {
    tags.iter().map(|t| t.to_ass()).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seg {
    Block(Vec<Tag>),
    Text(String),
}

pub fn parse(text: &str) -> Vec<Seg> {
    crate::tags::split(text)
        .into_iter()
        .map(|p| match p {
            crate::tags::Part::Block(b) => Seg::Block(parse_block(b)),
            crate::tags::Part::Text(t) => Seg::Text(t.to_string()),
        })
        .collect()
}

pub fn to_string(segs: &[Seg]) -> String {
    let mut s = String::new();
    for g in segs {
        match g {
            Seg::Block(t) => {
                s.push('{');
                s.push_str(&block_to_string(t));
                s.push('}');
            }
            Seg::Text(t) => s.push_str(t),
        }
    }
    s
}

/// Metnin başındaki bloktan etiketi okur (satır başı etiketleri).
pub fn get_initial(text: &str, name: &str) -> Option<Tag> {
    match parse(text).into_iter().next() {
        Some(Seg::Block(tags)) => tags.into_iter().rev().find(|t| t.name == name),
        _ => None,
    }
}

/// Başlangıç bloğunda etiketi ayarlar (varsa değiştirir, yoksa blok sonuna ekler; blok yoksa oluşturur).
pub fn set_initial(text: &str, name: &str, arg: &str) -> String {
    let mut segs = parse(text);
    match segs.first_mut() {
        Some(Seg::Block(tags)) => {
            if let Some(t) = tags.iter_mut().rev().find(|t| t.name == name) {
                t.arg = arg.to_string();
            } else {
                tags.push(Tag::new(name, arg));
            }
        }
        _ => segs.insert(0, Seg::Block(vec![Tag::new(name, arg)])),
    }
    to_string(&segs)
}

/// Başlangıç bloğundan etiketi kaldırır.
pub fn remove_initial(text: &str, name: &str) -> String {
    let mut segs = parse(text);
    if let Some(Seg::Block(tags)) = segs.first_mut() {
        tags.retain(|t| t.name != name);
        if tags.is_empty() {
            segs.remove(0);
        }
    }
    to_string(&segs)
}

/// Tüm bloklardan belirli etiketi siler.
pub fn remove_everywhere(text: &str, name: &str) -> String {
    let segs: Vec<Seg> = parse(text)
        .into_iter()
        .filter_map(|g| match g {
            Seg::Block(mut t) => {
                t.retain(|x| x.name != name);
                if t.is_empty() { None } else { Some(Seg::Block(t)) }
            }
            s => Some(s),
        })
        .collect();
    to_string(&segs)
}

/// Etiket temizliği: bitişik blokları birleştirir, boş blokları siler, bir blokta aynı etiket
/// tekrarlanıyorsa sonuncuyu bırakır (`\t`, `\k*`, `\r`, `\clip` hariç), etkisiz yinelenenleri
/// (bir önceki blokla aynı değeri tekrar eden) atmaz — bu güvenli tutulur.
pub fn clean(text: &str) -> String {
    let mut merged: Vec<Seg> = Vec::new();
    for g in parse(text) {
        match (merged.last_mut(), g) {
            (Some(Seg::Block(prev)), Seg::Block(mut cur)) => prev.append(&mut cur),
            (_, Seg::Text(t)) if t.is_empty() => {}
            (_, g) => merged.push(g),
        }
    }
    let mut out: Vec<Seg> = Vec::new();
    for g in merged {
        match g {
            Seg::Block(tags) => {
                let mut kept: Vec<Tag> = Vec::new();
                for t in tags {
                    let multi = matches!(t.name.as_str(), "t" | "k" | "kf" | "ko" | "K" | "kt" | "r" | "" | "clip" | "iclip");
                    if !multi {
                        kept.retain(|x| x.name != t.name);
                    }
                    kept.push(t);
                }
                if !kept.is_empty() {
                    out.push(Seg::Block(kept));
                }
            }
            s => out.push(s),
        }
    }
    to_string(&out)
}

/// `\\move(x1,y1,x2,y2[,t1,t2])` başlangıç bloğundan okunur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub times: Option<(i64, i64)>,
}

impl Move {
    pub fn get(text: &str) -> Option<Move> {
        let a: Vec<f64> = get_initial(text, "move")?.args().iter().filter_map(|v| v.trim().parse().ok()).collect();
        match a.len() {
            4 => Some(Move { from: (a[0], a[1]), to: (a[2], a[3]), times: None }),
            6 => Some(Move { from: (a[0], a[1]), to: (a[2], a[3]), times: Some((a[4] as i64, a[5] as i64)) }),
            _ => None,
        }
    }

    pub fn arg(&self) -> String {
        let f = |v: f64| {
            let r = (v * 100.0).round() / 100.0;
            if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{}", r) }
        };
        match self.times {
            Some((t1, t2)) => format!("({},{},{},{},{},{})", f(self.from.0), f(self.from.1), f(self.to.0), f(self.to.1), t1, t2),
            None => format!("({},{},{},{})", f(self.from.0), f(self.from.1), f(self.to.0), f(self.to.1)),
        }
    }

    /// Satıra yazar; `\\pos` ile çakıştığı için onu kaldırır.
    pub fn apply(&self, text: &str) -> String {
        set_initial(&remove_initial(text, "pos"), "move", &self.arg())
    }

    /// `\\move`'u başlangıç noktasında `\\pos`'a çevirir.
    pub fn to_pos(text: &str) -> String {
        match Move::get(text) {
            Some(m) => set_initial(&remove_initial(text, "move"), "pos", &format!("({},{})", m.from.0.round() as i64, m.from.1.round() as i64)),
            None => text.to_string(),
        }
    }
}


/// `pos` (karakter dizini) metin içinde bir `{...}` bloğunun içindeyse o bloğun kapanışına,
/// hemen önünde bir blok bitiyorsa o bloğa ekler; değilse yeni `{tag}` bloğu açar.
pub fn insert_tag_at(text: &str, pos: usize, tag: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let pos = pos.min(chars.len());
    // pos bir bloğun içinde mi?
    let mut open: Option<usize> = None;
    for (i, &c) in chars[..pos].iter().enumerate() {
        match c {
            '{' => open = Some(i),
            '}' => open = None,
            _ => {}
        }
    }
    let insert_at = if open.is_some() {
        chars[pos..].iter().position(|&c| c == '}').map(|k| pos + k)
    } else if pos > 0 && chars[pos - 1] == '}' {
        Some(pos - 1)
    } else if pos < chars.len() && chars[pos] == '{' {
        // hemen ardından blok başlıyorsa onun başına
        Some(pos + 1)
    } else {
        None
    };
    let mut out: String;
    match insert_at {
        Some(k) => {
            out = chars[..k].iter().collect();
            out.push_str(tag);
            out.extend(chars[k..].iter());
        }
        None => {
            out = chars[..pos].iter().collect();
            out.push('{');
            out.push_str(tag);
            out.push('}');
            out.extend(chars[pos..].iter());
        }
    }
    out
}

/// `pos`'tan önceki son `\name` değeri (yoksa None).
pub fn tag_value_before(text: &str, pos: usize, name: &str) -> Option<String> {
    // imleç bir bloğun içindeyse bloğun tamamı sayılır
    let chars: Vec<char> = text.chars().collect();
    let pos = pos.min(chars.len());
    let inside = chars[..pos].iter().rposition(|&c| c == '{').map_or(false, |o| !chars[o..pos].contains(&'}'));
    let end = if inside { chars[pos..].iter().position(|&c| c == '}').map_or(chars.len(), |k| pos + k + 1) } else { pos };
    let prefix: String = chars[..end].iter().collect();
    let mut last = None;
    for seg in parse(&prefix) {
        if let Seg::Block(tags) = seg {
            for t in tags {
                if t.name == name {
                    last = Some(t.arg.trim().to_string());
                }
                if t.name == "r" {
                    last = None;
                }
            }
        }
    }
    last
}

/// Etiket eklendikten sonra seçimi yeni metinde yeniden bulur: eski seçimin metni, `a`'dan itibaren
/// ilk geçtiği yer (etiket eklemek seçili metni değiştirmez). Bulunamazsa None.
pub fn remap_selection(old: &str, new: &str, a: usize, b: usize) -> Option<(usize, usize)> {
    let oc: Vec<char> = old.chars().collect();
    let (a, b) = (a.min(oc.len()), b.min(oc.len()));
    let sel: String = oc[a..b].iter().collect();
    let nc: Vec<char> = new.chars().collect();
    let n = sel.chars().count();
    if n == 0 {
        // imleç: eklenen bloğun ardına
        let delta = nc.len().saturating_sub(oc.len());
        return Some((a + delta, a + delta));
    }
    (a..=nc.len().saturating_sub(n)).find(|&i| nc[i..i + n].iter().collect::<String>() == sel).map(|i| (i, i + n))
}

/// Aegisub düzenleme kutusu düğmeleri (Kalın/İtalik/Altı çizili/Üstü çizili): seçim boşsa imleçte
/// durumu tersine çevirir; seçim varsa seçimi sarar (sonda önceki durum geri yüklenir).
/// `style_on`: stilin bu özelliği açık mı. Karakter dizinleriyle çalışır.
pub fn toggle_bool_tag(text: &str, a: usize, b: usize, name: &str, style_on: bool) -> String {
    let (a, b) = (a.min(b), a.max(b));
    let state = tag_value_before(text, a, name).map(|v| v.parse::<i32>().map_or(style_on, |n| n != 0)).unwrap_or(style_on);
    let on = format!("\\{}{}", name, if state { 0 } else { 1 });
    let off = format!("\\{}{}", name, if state { 1 } else { 0 });
    if a == b {
        return insert_tag_at(text, a, &on);
    }
    let t = insert_tag_at(text, b, &off);
    insert_tag_at(&t, a, &on)
}

/// Seçimi (ya da imleçten sonrasını) bir değerle işaretler: başa `\name<value>`, seçim varsa
/// sona önceki değer (`restore`; yoksa `\name` = stil varsayılanı) eklenir. Renk/yazı tipi için.
pub fn set_tag_range(text: &str, a: usize, b: usize, name: &str, value: &str) -> String {
    let (a, b) = (a.min(b), a.max(b));
    if a == b {
        return insert_tag_at(text, a, &format!("\\{name}{value}"));
    }
    let prev = tag_value_before(text, a, name).unwrap_or_default();
    let t = insert_tag_at(text, b, &format!("\\{name}{prev}"));
    insert_tag_at(&t, a, &format!("\\{name}{value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for s in [
            "{\\an8\\pos(100,200)\\fad(200,300)}Merhaba",
            "{\\1c&HFF0000&\\t(0,500,\\fscx120\\fscy120)\\blur2}x{\\r}y",
            "{\\k20\\kf30}a{comment}b{\\fnArial Black\\fs40}c",
            "düz metin",
        ] {
            assert_eq!(to_string(&parse(s)), s);
        }
    }

    /// Rastgele (kapanmamış parantez, ters bölü, iç içe ()) metinlerde ayrıştır→yaz birebir olmalı.
    #[test]
    fn fuzz_parse_roundtrip() {
        let mut seed: u64 = 0x1234ABCD5678EF01;
        let mut rnd = move |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n.max(1)
        };
        let frags = ["{", "}", "\\", "\\an8", "\\pos(1,2)", "\\t(0,5,\\fs3)", "(", ")", ",", "a", "ş", "日", " ", "\\N", "\\k20", "\\fn Arial", "\\1c&HFF&", "\\clip(", "m 0 0 l 1 1"];
        for round in 0..2000 {
            let n = rnd(14);
            let t: String = (0..n).map(|_| frags[rnd(frags.len())]).collect();
            assert_eq!(to_string(&parse(&t)), t, "tur {} metin {:?}", round, t);
            // clean() yok etmemeli: görünür metin korunur
            let cleaned = clean(&t);
            assert_eq!(crate::tags::strip_tags(&cleaned).replace(' ', ""), crate::tags::strip_tags(&t).replace(' ', "").replace("", ""), "tur {} metin {:?} temiz {:?}", round, t, cleaned);
        }
    }

    #[test]
    fn names() {
        let t = parse_block("\\fscx120\\fs40\\fade(1,2)\\frz30\\1a&H80&\\an5");
        let names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["fscx", "fs", "fade", "frz", "1a", "an"]);
        let t = parse_block("\\pos(1,2)\\t(0,10,\\c&HFF&)");
        assert_eq!(t[1].name, "t");
        assert_eq!(t[0].args(), vec!["1", "2"]);
    }

    #[test]
    fn set_get_remove() {
        let s = set_initial("Selam", "an", "8");
        assert_eq!(s, "{\\an8}Selam");
        assert_eq!(set_initial(&s, "an", "2"), "{\\an2}Selam");
        assert_eq!(get_initial(&s, "an").unwrap().number(), Some(8.0));
        assert_eq!(remove_initial(&s, "an"), "Selam");
    }

    #[test]
    fn cleaning() {
        assert_eq!(clean("{\\i1}{\\b1}a{}{\\i0}{\\i1}b"), "{\\i1\\b1}a{\\i0\\i1}b".replace("\\i0\\i1", "\\i1"));
        assert_eq!(clean("{\\an8\\an2}x"), "{\\an2}x");
        assert_eq!(clean("{\\k10}a{\\k10}b"), "{\\k10}a{\\k10}b");
    }

    #[test]
    fn move_roundtrip() {
        let t = "{\\an8\\move(10,20,30.5,40,100,900)}Merhaba";
        let m = Move::get(t).unwrap();
        assert_eq!(m.from, (10.0, 20.0));
        assert_eq!(m.times, Some((100, 900)));
        let m2 = Move { to: (300.0, 400.0), ..m };
        assert_eq!(m2.apply(t), "{\\an8\\move(10,20,300,400,100,900)}Merhaba");
        let p = "{\\pos(5,6)}x";
        let m3 = Move { from: (5.0, 6.0), to: (7.0, 8.0), times: None };
        assert_eq!(m3.apply(p), "{\\move(5,6,7,8)}x");
        assert_eq!(Move::to_pos("{\\move(5.4,6,7,8)}x"), "{\\pos(5,6)}x");
        assert_eq!(Move::get("{\\move(1,2,3)}x"), None);
    }

    #[test]
    fn edit_box_tag_buttons() {
        // imleçte: açık değil → aç
        assert_eq!(toggle_bool_tag("Merhaba dünya", 8, 8, "i", false), "Merhaba {\\i1}dünya");
        // seçim: sarar
        assert_eq!(toggle_bool_tag("Merhaba dünya", 8, 13, "b", false), "Merhaba {\\b1}dünya{\\b0}");
        // italik içindeyken kapatır; bloğun hemen arkasındaysa bloğa ekler
        assert_eq!(toggle_bool_tag("{\\i1}Merhaba dünya", 4, 4, "i", false), "{\\i1\\i0}Merhaba dünya");
        assert_eq!(toggle_bool_tag("{\\i1}Merhaba dünya", 13, 13, "i", false), "{\\i1}Merhaba {\\i0}dünya");
        // stil zaten kalınsa ilk tıklama kapatır
        assert_eq!(toggle_bool_tag("abc", 0, 0, "b", true), "{\\b0}abc");
        // blok içindeki imleç bloğun sonuna yazar
        assert_eq!(insert_tag_at("{\\pos(1,2)}x", 3, "\\i1"), "{\\pos(1,2)\\i1}x");
        // renk: seçimde önceki değer geri yüklenir
        assert_eq!(set_tag_range("{\\c&H0000FF&}ab cd", 16, 18, "c", "&HFFFFFF&"), "{\\c&H0000FF&}ab {\\c&HFFFFFF&}cd{\\c&H0000FF&}");
        assert_eq!(set_tag_range("ab cd", 3, 5, "c", "&HFFFFFF&"), "ab {\\c&HFFFFFF&}cd{\\c}");
        assert_eq!(set_tag_range("ab", 0, 0, "fn", "Arial"), "{\\fnArial}ab");
        // ardışık iki düğme aynı seçime uygulanır
        let t0 = "Merhaba dünya";
        let t1 = toggle_bool_tag(t0, 8, 13, "b", false);
        let (a, b) = remap_selection(t0, &t1, 8, 13).unwrap();
        let t2 = toggle_bool_tag(&t1, a, b, "i", false);
        assert_eq!(t2, "Merhaba {\\b1\\i1}dünya{\\i0\\b0}");
    }
}
