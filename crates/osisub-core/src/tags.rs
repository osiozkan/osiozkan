//! ASS geçersiz kılma ({...}) etiketleri için yardımcılar.

/// Metin parçası: etiket bloğu veya düz metin.
#[derive(Clone, Debug, PartialEq)]
pub enum Part<'a> {
    Block(&'a str),
    Text(&'a str),
}

pub fn split(text: &str) -> Vec<Part<'_>> {
    let mut parts = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with('{') {
            match rest.find('}') {
                Some(e) => {
                    parts.push(Part::Block(&rest[1..e]));
                    rest = &rest[e + 1..];
                }
                None => {
                    parts.push(Part::Text(rest));
                    break;
                }
            }
        } else {
            let e = rest.find('{').unwrap_or(rest.len());
            parts.push(Part::Text(&rest[..e]));
            rest = &rest[e..];
        }
    }
    parts
}

/// Bloktaki son `\pN` çizim kipi ayarı (`\pos`/`\pbo` değil): Some(açık mı).
fn drawing_mode(block: &str) -> Option<bool> {
    let b = block.as_bytes();
    let mut res = None;
    let mut i = 0;
    while let Some(k) = block[i..].find("\\p") {
        let j = i + k + 2;
        let digits: String = block[j..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && b.get(j + digits.len()).map_or(true, |&c| c == b'\\' || c == b'}' || c == b' ') {
            res = Some(digits.parse::<u32>().unwrap_or(0) > 0);
        }
        i = j;
    }
    res
}

/// Tüm `{...}` bloklarını kaldırır; `\p1` çizim kipindeki metin (şekil koordinatları) görünmez sayılır.
pub fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut drawing = false;
    for p in split(text) {
        match p {
            Part::Block(b) => {
                if let Some(d) = drawing_mode(b) {
                    drawing = d;
                }
            }
            Part::Text(t) if !drawing => out.push_str(t),
            Part::Text(_) => {}
        }
    }
    out
}

/// `\N`, `\n`, `\h` dönüşümleriyle görünür düz metin.
pub fn visible_text(text: &str) -> String {
    strip_tags(text).replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Okuma hızı hesabı için görünür karakter sayısı (boşluk ve satır sonları hariç).
pub fn visible_len(text: &str) -> usize {
    visible_text(text).chars().filter(|c| !c.is_whitespace()).count()
}

/// Satır başına en uzun görünür satır uzunluğu.
pub fn max_line_len(text: &str) -> usize {
    visible_text(text).lines().map(|l| l.trim().chars().count()).max().unwrap_or(0)
}

/// Bloklardaki `\k`, `\kf`, `\ko`, `\K` süreleri (santisaniye) ve ardından gelen hece metni.
pub fn karaoke_syllables(text: &str) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = Vec::new();
    let mut pending: Option<i64> = None;
    let mut start_new = false;
    for p in split(text) {
        match p {
            Part::Block(b) => {
                for t in b.split('\\').skip(1) {
                    let rest = t.strip_prefix("kf").or_else(|| t.strip_prefix("ko")).or_else(|| t.strip_prefix('k')).or_else(|| t.strip_prefix('K'));
                    if let Some(num) = rest {
                        if let Ok(v) = num.trim().parse::<i64>() {
                            if let Some(p) = pending.take() {
                                if !start_new {
                                    out.push((p, String::new()));
                                }
                            }
                            pending = Some(v);
                            start_new = true;
                        }
                    }
                }
            }
            Part::Text(t) => {
                if let Some(d) = pending.take() {
                    out.push((d, t.to_string()));
                    start_new = false;
                } else if let Some(last) = out.last_mut() {
                    last.1.push_str(t);
                } else {
                    out.push((0, t.to_string()));
                }
            }
        }
    }
    if let Some(p) = pending {
        out.push((p, String::new()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strip() {
        assert_eq!(strip_tags("{\\an8}Selam {\\i1}dünya{\\i0}"), "Selam dünya");
        assert_eq!(visible_text("a\\Nb\\hc"), "a\nb c");
        assert_eq!(visible_len("ab c"), 3);
    }
    #[test]
    fn drawings_are_not_visible_text() {
        assert_eq!(strip_tags("{\\an7\\pos(1,2)\\p1}m 0 0 l 10 0 10 10{\\p0}Metin"), "Metin");
        assert_eq!(visible_len("{\\p2}m 0 0 l 100 100"), 0);
        assert_eq!(strip_tags("{\\pos(1,2)\\pbo3}Yazı"), "Yazı");
        assert_eq!(strip_tags("{\\p1\\fs20}m 0 0{\\fs30}l 5 5"), "");
    }

    #[test]
    fn karaoke() {
        let k = karaoke_syllables("{\\k20}ka{\\k30}ra");
        assert_eq!(k, vec![(20, "ka".into()), (30, "ra".into())]);
    }
}
