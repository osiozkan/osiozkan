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
}
