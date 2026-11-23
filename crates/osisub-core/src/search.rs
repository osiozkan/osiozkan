//! Bul/Değiştir: düz metin ya da düzenli ifade; Türkçe i/İ ı/I duyarsızlığı.
use crate::ass::{Dialogue, Entry, Script};
use fancy_regex::{Regex, RegexBuilder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Text,
    Actor,
    Effect,
    Style,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub pattern: String,
    pub replacement: String,
    pub regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
    /// Yalnızca bu alanlarda ara.
    pub fields: Vec<Field>,
    pub skip_comments: bool,
    /// Yalnızca bu entry indeksleri (None = tümü).
    pub only: Option<Vec<usize>>,
    /// Etiketleri atlayarak yalnızca görünür metinde ara (düz Text alanı için).
    pub ignore_tags: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { pattern: String::new(), replacement: String::new(), regex: false, match_case: false, whole_word: false, fields: vec![Field::Text], skip_comments: false, only: None, ignore_tags: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub entry: usize,
    pub field: Field,
}

fn tr_class(c: char) -> Option<&'static str> {
    match c {
        // harf duyarsız kipte de i/ı ayrımı korunsun (ı ↔ i katlanmasın)
        'i' | 'İ' => Some("(?-i:[iİ])"),
        'ı' | 'I' => Some("(?-i:[ıI])"),
        _ => None,
    }
}

/// Düzenli ifadede karakter sınıfı, kaçış ve `(?…)` grup başlıkları dışındaki i/İ/ı/I harflerini
/// Türkçe sınıflara çevirir (harf duyarsız kipte "ilk" "İlk"i bulur, "Ilk"i bulmaz).
fn tr_regex(p: &str) -> String {
    let c: Vec<char> = p.chars().collect();
    let mut out = String::with_capacity(p.len() + 16);
    let (mut i, mut in_class) = (0, false);
    while i < c.len() {
        let ch = c[i];
        if ch == '\\' {
            out.push(ch);
            if let Some(&n) = c.get(i + 1) {
                out.push(n);
                i += 2;
                // \p{Lu}, \x{41}, \u{…}: süslü parantezin içi olduğu gibi
                if "pPxu".contains(n) && c.get(i) == Some(&'{') {
                    while i < c.len() {
                        out.push(c[i]);
                        i += 1;
                        if c[i - 1] == '}' { break; }
                    }
                }
                continue;
            }
            i += 1;
            continue;
        }
        if in_class {
            if ch == ']' { in_class = false; }
            out.push(ch);
        } else if ch == '[' {
            in_class = true;
            out.push(ch);
            // "[]…]" ve "[^]…]": ilk ] sınıfın parçası
            if c.get(i + 1) == Some(&'^') { out.push('^'); i += 1; }
            if c.get(i + 1) == Some(&']') { out.push(']'); i += 1; }
        } else if ch == '(' && c.get(i + 1) == Some(&'?') {
            // (?i) (?:…) (?<ad>…) (?P<ad>…): başlık olduğu gibi
            while i < c.len() {
                out.push(c[i]);
                if i > 0 && matches!(c[i], ')' | ':' | '>' | '=' | '!') && c[i] != '(' { break; }
                i += 1;
            }
        } else if let Some(cls) = tr_class(ch) {
            out.push_str(cls);
        } else {
            out.push(ch);
        }
        i += 1;
    }
    out
}

pub fn build(o: &Options) -> Result<Regex, String> {
    if o.pattern.is_empty() {
        return Err("Boş arama".into());
    }
    let mut pat = if o.regex && !o.match_case {
        tr_regex(&o.pattern)
    } else if o.regex {
        o.pattern.clone()
    } else if o.match_case {
        fancy_regex::escape(&o.pattern).into_owned()
    } else {
        // Büyük/küçük harf duyarsız düz arama: i/İ ve ı/I'yı Türkçe kurallarıyla eşle.
        o.pattern.chars().map(|c| tr_class(c).map(String::from).unwrap_or_else(|| fancy_regex::escape(&c.to_string()).into_owned())).collect()
    };
    if o.whole_word {
        pat = format!(r"(?<![\p{{L}}\p{{N}}_])(?:{})(?![\p{{L}}\p{{N}}_])", pat);
    }
    // fancy-regex: ileri/geri bakış içeren desende `case_insensitive` bayrağı etkisiz kalıyor
    // (tam sözcük + harf duyarsız hiç eşleşmiyordu) — satır içi `(?i)` kullan
    if !o.match_case {
        pat = format!("(?i){pat}");
    }
    RegexBuilder::new(&pat).build().map_err(|e| e.to_string())
}

fn eligible(o: &Options, i: usize, d: &Dialogue) -> bool {
    !(o.skip_comments && d.comment) && o.only.as_ref().map_or(true, |v| v.contains(&i))
}

fn field_ref(d: &Dialogue, f: Field) -> &str {
    match f {
        Field::Text => &d.text,
        Field::Actor => &d.actor,
        Field::Effect => &d.effect,
        Field::Style => &d.style,
    }
}

pub fn find_all(script: &Script, o: &Options) -> Result<Vec<Hit>, String> {
    let re = build(o)?;
    let mut hits = vec![];
    for (i, e) in script.entries.iter().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if !eligible(o, i, d) {
            continue;
        }
        for &f in &o.fields {
            // \N \n \h harf sayılmasın (tam sözcük satır başında da eşleşsin)
            let hay = if f == Field::Text && o.ignore_tags { crate::tags::strip_tags(&d.text).replace("\\N", " ").replace("\\n", " ").replace("\\h", " ") } else { field_ref(d, f).to_string() };
            if re.is_match(&hay).map_err(|e| e.to_string())? {
                hits.push(Hit { entry: i, field: f });
            }
        }
    }
    Ok(hits)
}

fn convert_repl(r: &str, regex: bool) -> String {
    if !regex {
        return r.replace('$', "$$");
    }
    // \1 -> ${1}; $1 -> ${1}
    let b: Vec<char> = r.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if (b[i] == '\\' || b[i] == '$') && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
            out.push_str(&format!("${{{}}}", b[i + 1]));
            i += 2;
        } else if b[i] == '\\' && i + 1 < b.len() && b[i + 1] == 'n' {
            out.push_str("\\N");
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// `\N` `\n` `\h` kaçışlarını koruyarak yalnızca aralarındaki metinde değiştirir
/// ("N" → "M" kuralı `\N`'yi bozmasın; satır sonundan sonraki sözcük tam sözcük sayılsın).
fn replace_between_escapes(re: &Regex, t: &str, repl: &str) -> String {
    let mut out = String::new();
    let mut seg = String::new();
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && matches!(it.peek(), Some('N' | 'n' | 'h')) {
            out.push_str(&re.replace_all(&seg, repl));
            seg.clear();
            out.push(c);
            out.push(it.next().unwrap());
        } else {
            seg.push(c);
        }
    }
    out.push_str(&re.replace_all(&seg, repl));
    out
}

/// Tümünü değiştirir; değişen alan sayısını döndürür.
pub fn replace_all(script: &mut Script, o: &Options) -> Result<usize, String> {
    let re = build(o)?;
    let repl = convert_repl(&o.replacement, o.regex);
    let mut n = 0;
    for (i, e) in script.entries.iter_mut().enumerate() {
        let Entry::Dialogue(d) = e else { continue };
        if !eligible(o, i, d) {
            continue;
        }
        for &f in &o.fields {
            let cur = field_ref(d, f).to_string();
            let new = if f == Field::Text && o.ignore_tags {
                // Yalnızca etiket dışı parçalarda değiştir.
                let mut out = String::new();
                for p in crate::tags::split(&cur) {
                    match p {
                        crate::tags::Part::Block(b) => {
                            out.push('{');
                            out.push_str(b);
                            out.push('}');
                        }
                        crate::tags::Part::Text(t) => out.push_str(&replace_between_escapes(&re, t, &repl)),
                    }
                }
                out
            } else {
                re.replace_all(&cur, repl.as_str()).into_owned()
            };
            if new != cur {
                n += 1;
                match f {
                    Field::Text => d.text = new,
                    Field::Actor => d.actor = new,
                    Field::Effect => d.effect = new,
                    Field::Style => d.style = new,
                }
            }
        }
    }
    Ok(n)
}

/// Aegisub "Sonrakini değiştir": `entry` satırında, alanlar sırasıyla `(field, from)` konumundan
/// (bayt) itibaren ilk eşleşmeyi değiştirir. `ignore_tags` ise etiket bloklarına ve `\N`/`\h`
/// kaçışlarına değen eşleşmeler atlanır. Dönüş: değiştirilen alan ve sonraki arama konumu
/// (yerleşen metnin sonu — yerine konan metin deseni içerse de döngüye girmez).
pub fn replace_next(script: &mut Script, o: &Options, entry: usize, field: Field, from: usize) -> Result<Option<(Field, usize)>, String> {
    let re = build(o)?;
    let repl = convert_repl(&o.replacement, o.regex);
    let Some(Entry::Dialogue(d)) = script.entries.get_mut(entry) else { return Ok(None) };
    if !eligible(o, entry, d) {
        return Ok(None);
    }
    let start_k = o.fields.iter().position(|f| *f == field).unwrap_or(0);
    for (k, &f) in o.fields.iter().enumerate().skip(start_k) {
        let cur = field_ref(d, f).to_string();
        let from = if k == start_k { from.min(cur.len()) } else { 0 };
        // yasak bölgeler: {…} blokları ve \N \n \h kaçışları
        let mut banned: Vec<(usize, usize)> = vec![];
        if f == Field::Text && o.ignore_tags {
            let b = cur.as_bytes();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'{' {
                    let e = cur[i..].find('}').map_or(b.len(), |e| i + e + 1);
                    banned.push((i, e));
                    i = e;
                } else if b[i] == b'\\' && matches!(b.get(i + 1), Some(b'N' | b'n' | b'h')) {
                    banned.push((i, i + 2));
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
        // yasak bölgeler aynı bayt uzunluğunda boşlukla maskelenir: tam sözcük `\N`'den sonra da eşleşsin
        let mut masked = cur.clone().into_bytes();
        for &(a, b) in &banned {
            masked[a..b].fill(b' ');
        }
        let masked = String::from_utf8(masked).unwrap_or_else(|_| cur.clone());
        let mut pos = from;
        while pos <= cur.len() {
            if !masked.is_char_boundary(pos) {
                pos += 1;
                continue;
            }
            let Some(c) = re.captures_from_pos(&masked, pos).map_err(|e| e.to_string())? else { break };
            let m = c.get(0).unwrap();
            if banned.iter().any(|&(a, b)| m.start() < b && m.end() > a) || (m.start() == m.end()) {
                pos = m.end().max(m.start() + 1);
                continue;
            }
            let mut rep = String::new();
            c.expand(&repl, &mut rep);
            let new = format!("{}{}{}", &cur[..m.start()], rep, &cur[m.end()..]);
            let next = m.start() + rep.len();
            match f {
                Field::Text => d.text = new,
                Field::Actor => d.actor = new,
                Field::Effect => d.effect = new,
                Field::Style => d.style = new,
            }
            return Ok(Some((f, next)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    #[test]
    fn replace_next_one_at_a_time() {
        let mut sc = Script::from_dialogues(vec![Dialogue { text: "{\\an8}an a an\\Nan".into(), ..Default::default() }]);
        let e = sc.entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap();
        let o = Options { pattern: "an".into(), replacement: "anan".into(), whole_word: true, ignore_tags: true, ..Default::default() };
        let (f, p) = replace_next(&mut sc, &o, e, Field::Text, 0).unwrap().unwrap();
        let t = |sc: &Script| sc.dialogues().next().unwrap().text.clone();
        assert_eq!(t(&sc), "{\\an8}anan a an\\Nan");
        let (f, p) = replace_next(&mut sc, &o, e, f, p).unwrap().unwrap();
        assert_eq!(t(&sc), "{\\an8}anan a anan\\Nan");
        replace_next(&mut sc, &o, e, f, p).unwrap().unwrap();
        assert_eq!(t(&sc), "{\\an8}anan a anan\\Nanan");
        let n = t(&sc).len();
        let last = replace_next(&mut sc, &o, e, Field::Text, n).unwrap();
        assert!(last.is_none());
        // düzenli ifade grupları
        let mut sc = Script::from_dialogues(vec![Dialogue { text: "ab12 cd34".into(), ..Default::default() }]);
        let o = Options { pattern: r"([a-z]+)(\d+)".into(), replacement: r"\2\1".into(), regex: true, match_case: true, ..Default::default() };
        replace_next(&mut sc, &o, e, Field::Text, 0).unwrap().unwrap();
        assert_eq!(t(&sc), "12ab cd34");
    }

    use super::*;

    #[test]
    fn tam_sozcuk_harf_duyarsiz_ve_kacislar() {
        let mut sc = Script::from_dialogues(vec![
            Dialogue { text: "Naruto geldi, NARUTO!".into(), ..Default::default() },
            Dialogue { text: "Merhaba.\\Nnaruto geldi".into(), ..Default::default() },
            Dialogue { text: "Bir\\Niki ilik".into(), ..Default::default() },
        ]);
        let o = Options { pattern: "naruto".into(), replacement: "Sasuke".into(), whole_word: true, ignore_tags: true, ..Default::default() };
        assert_eq!(find_all(&sc, &o).unwrap().len(), 2);
        assert_eq!(replace_all(&mut sc, &o).unwrap(), 2);
        let t: Vec<String> = sc.dialogues().map(|d| d.text.clone()).collect();
        assert_eq!(t[0], "Sasuke geldi, Sasuke!");
        assert_eq!(t[1], "Merhaba.\\NSasuke geldi");
        // \N korunur
        let o = Options { pattern: "N".into(), replacement: "M".into(), match_case: true, ignore_tags: true, ..Default::default() };
        replace_all(&mut sc, &o).unwrap();
        assert!(sc.dialogues().nth(2).unwrap().text.starts_with("Bir\\Niki"));
        // Türkçe ı/i ayrımı harf duyarsız kipte de
        let o = Options { pattern: "ılık".into(), ..Default::default() };
        assert!(find_all(&sc, &o).unwrap().is_empty(), "ılık, ilik'i bulmamalı");
        let o = Options { pattern: "çağrı".into(), whole_word: true, ..Default::default() };
        let sc2 = Script::from_dialogues(vec![Dialogue { text: "Çağrı geldi".into(), ..Default::default() }]);
        assert_eq!(find_all(&sc2, &o).unwrap().len(), 1);
    }

    fn script() -> Script {
        Script::from_dialogues(vec![
            Dialogue { text: "İstanbul'da ışık".into(), actor: "Ali".into(), ..Default::default() },
            Dialogue { text: "{\\i1}istanbul{\\i0} yok".into(), ..Default::default() },
            Dialogue { text: "x".into(), comment: true, ..Default::default() },
        ])
    }

    #[test]
    fn turkish_case_insensitive() {
        let s = script();
        let o = Options { pattern: "istanbul".into(), ..Default::default() };
        assert_eq!(find_all(&s, &o).unwrap().len(), 2);
        let o = Options { pattern: "ISIK".into(), ..Default::default() };
        assert_eq!(find_all(&s, &o).unwrap().len(), 0, "ş/s farklı harf, ama I/ı eşleşmeli: ISIK != ışık");
        let o = Options { pattern: "IŞIK".into(), ..Default::default() };
        assert_eq!(find_all(&s, &o).unwrap().len(), 1);
    }

    #[test]
    fn replace_and_tags() {
        let mut s = script();
        let o = Options { pattern: "(\\w+) yok".into(), replacement: "var: $1".into(), regex: true, ignore_tags: true, ..Default::default() };
        // Etiket bloğu araya girdiği için eşleşme yalnızca düz parçalarda aranır → eşleşmez.
        assert_eq!(replace_all(&mut s, &o).unwrap(), 0);
        let o = Options { pattern: "yok".into(), replacement: "var".into(), ..Default::default() };
        assert_eq!(replace_all(&mut s, &o).unwrap(), 1);
        assert!(s.dialogues().nth(1).unwrap().text.ends_with("var"));
    }

    #[test]
    fn whole_word_and_scope() {
        let s = script();
        let o = Options { pattern: "stanbul".into(), whole_word: true, ..Default::default() };
        assert_eq!(find_all(&s, &o).unwrap().len(), 0);
        let o = Options { pattern: "ali".into(), fields: vec![Field::Actor], ..Default::default() };
        assert_eq!(find_all(&s, &o).unwrap().len(), 1);
        let o = Options { pattern: "x".into(), skip_comments: true, ..Default::default() };
        assert_eq!(find_all(&s, &o).unwrap().len(), 0);
    }

    #[test]
    fn inceleme_regex_turkce_ve_etiketler() {
        let o = Options { pattern: "ilk".into(), regex: true, ..Default::default() };
        let re = build(&o).unwrap();
        assert!(re.is_match("İlk").unwrap());
        assert!(!re.is_match("Ilk").unwrap());
        let o = Options { pattern: r"(?<a>i)\p{Lu}[i]".into(), regex: true, ..Default::default() };
        assert!(build(&o).unwrap().is_match("İAi").unwrap());
        assert!(Options::default().ignore_tags, "varsayılan: etiketlerin içi değiştirilmez");
    }
}