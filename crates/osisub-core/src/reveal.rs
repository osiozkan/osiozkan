//! Metin belirme efektleri (Subtitle Edit Advanced effects: typewriter, word by word, fade in).
//! Subtitle Edit her adım için ayrı satır yazar ve görünen metni büyütür (ortalı metin kayar);
//! burada TEK satır yazılır: her birim (harf ya da sözcük) baştan saydamdır ve kendi anında
//! `\t` ile görünür olur — yerleşim değişmez, tabela/şarkı satırları için uygundur.
use crate::tags::{self, Part};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Char,
    Word,
}

/// `text`'e belirme etiketleri ekler. Birimler `span_ms` içinde eşit aralıklarla belirir; her biri
/// `fade_ms` sürede saydamdan görünüre geçer (0 = anında, daktilo). Etiket blokları, `\N` ve boşluklar
/// birim sayılmaz; çizim (`\p1`) satırına dokunulmaz.
pub fn apply(text: &str, unit: Unit, span_ms: i64, fade_ms: i64) -> String {
    apply_alpha(text, unit, span_ms, fade_ms, [0, 0, 0])
}

/// `apply`, ama görünür hâldeki saydamlıklar verilir: [birincil, kenar, gölge] (ASS alfa, stilden).
/// Yarı saydam gölgesi olan stillerde gölge belirdikten sonra opaklaşmasın diye kanallar ayrı canlandırılır.
pub fn apply_alpha(text: &str, unit: Unit, span_ms: i64, fade_ms: i64, target: [u8; 3]) -> String {
    if (1..=9).any(|k| text.contains(&format!("\\p{k}"))) {
        return text.to_string();
    }
    // birim başlangıçlarının (parça, bayt) konumları
    let parts = tags::split(text);
    let mut starts: Vec<(usize, usize)> = vec![];
    let mut prev_space = true;
    for (pi, p) in parts.iter().enumerate() {
        let Part::Text(t) = p else { continue };
        let mut it = t.char_indices().peekable();
        while let Some((bi, c)) = it.next() {
            // \N, \n, \h kaçışları boşluk sayılır
            if c == '\\' && matches!(it.peek().map(|x| x.1), Some('N' | 'n' | 'h')) {
                it.next();
                prev_space = true;
                continue;
            }
            if c.is_whitespace() {
                prev_space = true;
                continue;
            }
            if unit == Unit::Char || prev_space {
                starts.push((pi, bi));
            }
            prev_space = false;
        }
    }
    let n = starts.len();
    if n == 0 {
        return text.to_string();
    }
    let step = span_ms.max(0) as f64 / n as f64;
    let mut out = String::with_capacity(text.len() + n * 40);
    let mut k = 0;
    for (pi, p) in parts.iter().enumerate() {
        match p {
            Part::Block(b) => {
                out.push('{');
                out.push_str(b);
                out.push('}');
            }
            Part::Text(t) => {
                let mut last = 0;
                while k < n && starts[k].0 == pi {
                    let bi = starts[k].1;
                    out.push_str(&t[last..bi]);
                    let a = (k as f64 * step).round() as i64;
                    let b = a + fade_ms.max(1);
                    if target == [0, 0, 0] {
                        out.push_str(&format!("{{\\alpha&HFF&\\t({a},{b},\\alpha&H00&)}}"));
                    } else {
                        out.push_str(&format!("{{\\alpha&HFF&\\t({a},{b},\\1a&H{:02X}&\\3a&H{:02X}&\\4a&H{:02X}&)}}", target[0], target[1], target[2]));
                    }
                    last = bi;
                    k += 1;
                }
                out.push_str(&t[last..]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_chars() {
        assert_eq!(apply("Bir iki", Unit::Word, 1000, 0), "{\\alpha&HFF&\\t(0,1,\\alpha&H00&)}Bir {\\alpha&HFF&\\t(500,501,\\alpha&H00&)}iki");
        let c = apply("{\\an8}ab\\Nc", Unit::Char, 300, 100);
        assert_eq!(c, "{\\an8}{\\alpha&HFF&\\t(0,100,\\alpha&H00&)}a{\\alpha&HFF&\\t(100,200,\\alpha&H00&)}b\\N{\\alpha&HFF&\\t(200,300,\\alpha&H00&)}c");
        // satır içi etiket sözcüğü bölmez; görünür metin aynı kalır
        let w = apply("A{\\i1}b c{\\i0}", Unit::Word, 200, 0);
        assert_eq!(tags::strip_tags(&w), "Ab c");
        assert_eq!(w.matches("\\t(").count(), 2);
        // Türkçe harfler bayt sınırında bölünmez
        assert_eq!(tags::strip_tags(&apply("Çağ ışık", Unit::Char, 600, 0)), "Çağ ışık");
        assert_eq!(apply("{\\p1}m 0 0 l 1 1", Unit::Char, 100, 0), "{\\p1}m 0 0 l 1 1");
        assert_eq!(apply("{\\an8}", Unit::Word, 100, 0), "{\\an8}");
    }

    #[test]
    fn keeps_style_transparency() {
        let t = apply_alpha("ab", Unit::Word, 100, 0, [0, 0, 0x80]);
        assert_eq!(t, "{\\alpha&HFF&\\t(0,1,\\1a&H00&\\3a&H00&\\4a&H80&)}ab");
    }
}
