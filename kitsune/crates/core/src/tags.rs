//! Override etiket bloklarını ayıran ve metin analizi yapan yardımcılar.

#[derive(Debug, Clone, PartialEq)]
pub enum Part<'a> {
    /// `{...}` içeriği (süslü parantezler hariç)
    Tags(&'a str),
    Text(&'a str),
}

/// Metni etiket bloklarına ve düz metne böler.
pub fn split(text: &str) -> Vec<Part<'_>> {
    let mut out = vec![];
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(open) = rest.find('{') {
            if open > 0 {
                out.push(Part::Text(&rest[..open]));
            }
            match rest[open..].find('}') {
                Some(close) => {
                    out.push(Part::Tags(&rest[open + 1..open + close]));
                    rest = &rest[open + close + 1..];
                }
                None => {
                    out.push(Part::Text(&rest[open..]));
                    break;
                }
            }
        } else {
            out.push(Part::Text(rest));
            break;
        }
    }
    out
}

/// Etiketleri siler; `\N`/`\n`/`\h` korunur (isteğe bağlı olarak boşluğa çevrilebilir).
pub fn strip_tags(text: &str) -> String {
    split(text)
        .into_iter()
        .filter_map(|p| if let Part::Text(t) = p { Some(t) } else { None })
        .collect()
}

/// Gösterilen düz metin: etiketsiz, `\N`->'\n', `\h`->' '.
pub fn plain(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", " ").replace("\\h", " ")
}

/// Okunabilirlik için sayılan karakter sayısı (boşluk ve satır sonu hariç).
pub fn visible_chars(text: &str) -> usize {
    plain(text).chars().filter(|c| !c.is_whitespace()).count()
}

/// Satırlar (\N ile) halinde en uzun satırın karakter sayısı.
pub fn max_line_len(text: &str) -> usize {
    plain(text).lines().map(|l| l.chars().count()).max().unwrap_or(0)
}

pub fn line_count(text: &str) -> usize {
    let p = plain(text);
    if p.is_empty() { 0 } else { p.lines().count() }
}

/// Bir etiket bloğundaki etiketleri (ad, argüman) olarak ayırır. İç içe `\t(...)` tek etiket sayılır.
pub fn parse_block(block: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let b = block.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        let mut depth = 0;
        while j < b.len() {
            match b[j] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'\\' if depth <= 0 => break,
                _ => {}
            }
            j += 1;
        }
        let tag = &block[start..j];
        let nl = tag.find(|c: char| c.is_ascii_digit() || c == '(' || c == '-' || c == '&' || c == '.')
            .unwrap_or(tag.len());
        // 1c, 2a gibi rakamla başlayan etiket adları
        let (name, arg) = split_name(tag, nl);
        out.push((name.to_string(), arg.trim().to_string()));
        i = j;
    }
    out
}

fn split_name(tag: &str, nl: usize) -> (&str, &str) {
    // \1c &H..& , \2a, \3c, \4a, \fn Arial, \fs20, \fscx100, \fad(…), \pos(…)
    let bytes = tag.as_bytes();
    if !bytes.is_empty() && matches!(bytes[0], b'1'..=b'4') && tag.len() > 1 {
        let mut e = 2;
        while e < tag.len() && tag.as_bytes()[e].is_ascii_alphabetic() { e += 1; }
        return (&tag[..e], &tag[e..]);
    }
    for pre in ["fn", "fscx", "fscy", "fsp", "fs", "fe", "fad", "fade", "frx", "fry", "frz", "fr", "bord", "xbord", "ybord", "shad", "xshad", "yshad", "be", "blur", "pos", "move", "org", "clip", "iclip", "an", "a", "alpha", "b", "i", "u", "s", "k", "K", "kf", "ko", "kt", "q", "r", "p", "pbo", "fax", "fay", "c", "t"] {
        if tag.starts_with(pre) {
            let rest = &tag[pre.len()..];
            let next_is_alpha = rest.chars().next().map_or(false, |c| c.is_ascii_alphabetic());
            // \fn ve \r serbest metin alır
            if pre == "fn" || pre == "r" || !next_is_alpha {
                return (pre, rest);
            }
        }
    }
    (&tag[..nl], &tag[nl..])
}

/// Bloktaki tüm etiketleri sil (`{}` bırakmaz).
pub fn remove_all_tags(text: &str) -> String {
    strip_tags(text)
}

/// Boş `{}` bloklarını ve ardışık blokları birleştirir.
pub fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut pending = String::new();
    for p in split(text) {
        match p {
            Part::Tags(t) => pending.push_str(t),
            Part::Text(t) => {
                if !pending.is_empty() {
                    out.push('{');
                    out.push_str(&pending);
                    out.push('}');
                    pending.clear();
                }
                out.push_str(t);
            }
        }
    }
    if !pending.is_empty() {
        out.push('{');
        out.push_str(&pending);
        out.push('}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basics() {
        assert_eq!(strip_tags("a{\\i1}b{\\i0}c"), "abc");
        assert_eq!(plain("Merhaba\\Ndünya"), "Merhaba\ndünya");
        assert_eq!(visible_chars("ab {\\b1}c\\Nd"), 4);
        assert_eq!(max_line_len("abc\\Nabcde"), 5);
        assert_eq!(clean("{\\i1}{\\b1}x"), "{\\i1\\b1}x");
    }
    #[test]
    fn tag_parse() {
        let t = parse_block("\\an8\\pos(100,200)\\1c&HFF0000&\\fnArial\\fs30\\t(0,500,\\fscx120)\\b1");
        let names: Vec<_> = t.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, ["an", "pos", "1c", "fn", "fs", "t", "b"]);
        assert_eq!(t[1].1, "(100,200)");
        assert_eq!(t[3].1, "Arial");
    }
}
