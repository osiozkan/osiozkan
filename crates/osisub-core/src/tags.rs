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

/// Tüm `{...}` bloklarını kaldırır.
pub fn strip_tags(text: &str) -> String {
    split(text).into_iter().filter_map(|p| if let Part::Text(t) = p { Some(t) } else { None }).collect()
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
    fn karaoke() {
        let k = karaoke_syllables("{\\k20}ka{\\k30}ra");
        assert_eq!(k, vec![(20, "ka".into()), (30, "ra".into())]);
    }
}
