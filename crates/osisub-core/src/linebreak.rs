//! Satır kırma/dengeleme (Subtitle Edit "Auto balance lines" / "Remove line breaks" benzeri).
//!
//! Metin etiketleri korunarak yalnızca görünür metindeki boşluklar aday kırma noktasıdır.
//! Puanlama: iki satırın uzunluk farkı; noktalama sonrası kırma ödüllendirilir; ilk satırın
//! bağlaç/edat/tanımlıkla bitmesi (Türkçe "ve, ama, ile, bir…", İngilizce "and, the, a…")
//! cezalandırılır; satır sınırını aşan satır ağır cezalıdır. İkinci konuşmacının diyalog çizgisi
//! (`- Merhaba. - Selam.`) varsa kırma oraya yapılır.
use crate::tags::{self, Part};

/// İlk satırın sonunda kalmaması gereken kısa sözcükler (küçük harfle).
const GLUE: &[&str] = &[
    // Türkçe
    "ve", "veya", "ya", "ile", "ama", "fakat", "ancak", "çünkü", "ki", "bir", "bu", "şu", "o", "en", "çok", "daha", "hem", "ne", "gibi", "için", "kadar",
    // İngilizce
    "a", "an", "the", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "i", "my", "your", "if", "that", "so",
];

/// Görünür metin uzunluğu (etiketsiz, `\N` hariç) ve kırma adayları.
struct Layout {
    /// Ham metindeki boşluk bayt konumu ve o noktaya kadarki görünür karakter sayısı
    spaces: Vec<(usize, usize)>,
    /// Diyalog çizgisi adayı: " - " ya da " – " (ham konum = boşluğun konumu)
    dash: Vec<usize>,
    visible: usize,
}

fn layout(text: &str) -> Layout {
    let mut spaces = Vec::new();
    let mut dash = Vec::new();
    let mut visible = 0;
    let mut pos = 0;
    let mut drawing = false;
    for part in tags::split(text) {
        match part {
            Part::Block(b) => {
                if let Some(i) = b.find("\\p") {
                    let d: String = b[i + 2..].chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !d.is_empty() {
                        drawing = d != "0";
                    }
                }
                pos += b.len() + 2;
            }
            Part::Text(t) => {
                if !drawing {
                    let mut it = t.char_indices().peekable();
                    while let Some((i, c)) = it.next() {
                        if c == ' ' {
                            spaces.push((pos + i, visible));
                            let rest = &t[i + 1..];
                            if (rest.starts_with("- ") || rest.starts_with("– ") || rest.starts_with("— ")) && visible > 0 {
                                dash.push(pos + i);
                            }
                        }
                        visible += 1;
                    }
                }
                pos += t.len();
            }
        }
    }
    Layout { spaces, dash, visible }
}

/// Satır sonlarını (`\N`, `\n`) boşluğa çevirir; çift boşlukları teke indirir.
pub fn unbreak(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for part in tags::split(text) {
        match part {
            Part::Block(b) => {
                out.push('{');
                out.push_str(b);
                out.push('}');
            }
            Part::Text(t) => {
                let t = t.replace("\\N", " ").replace("\\n", " ");
                let mut prev_space = out.ends_with(' ');
                for c in t.chars() {
                    if c == ' ' && prev_space {
                        continue;
                    }
                    prev_space = c == ' ';
                    out.push(c);
                }
            }
        }
    }
    out.trim().to_string()
}

fn last_word_before(text: &str, at: usize) -> String {
    let head = tags::strip_tags(&text[..at]);
    head.trim_end().rsplit(' ').next().unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Metni en fazla iki satıra dengeli kırar. `max`: satır başına karakter sınırı.
/// Kısa metin (≤ max) tek satır yapılır. Çizim, `\q2` ya da üç+ satır gerektiren metne dokunulmaz.
pub fn balance(text: &str, max: usize) -> String {
    if text.contains("\\q2") || text.contains("\\p1") {
        return text.to_string();
    }
    let flat = unbreak(text);
    let lay = layout(&flat);
    let has_dash = !lay.dash.is_empty();
    if lay.visible <= max && !has_dash {
        return flat;
    }
    // Diyalog: ilk ikinci-konuşmacı çizgisinden kır
    if let Some(&p) = lay.dash.first() {
        return format!("{}\\N{}", &flat[..p], &flat[p + 1..]);
    }
    let total = lay.visible;
    let best = lay
        .spaces
        .iter()
        .filter(|(_, v)| *v > 0 && *v + 1 < total)
        .map(|&(p, v)| {
            let a = v;
            let b = total - v - 1;
            let mut score = (a as f64 - b as f64).abs();
            // hafif alt-ağır tercih (üst satır biraz kısa olsun)
            if a > b {
                score += 2.0;
            }
            if a > max || b > max {
                score += 1000.0 + (a.max(b) - max) as f64 * 10.0;
            }
            let prev = tags::strip_tags(&flat[..p]);
            match prev.trim_end().chars().last() {
                Some('.' | '!' | '?' | '…') => score -= 12.0,
                Some(',' | ';' | ':') => score -= 8.0,
                _ => {}
            }
            if GLUE.contains(&last_word_before(&flat, p).as_str()) {
                score += 10.0;
            }
            (p, score)
        })
        .min_by(|x, y| x.1.total_cmp(&y.1));
    match best {
        Some((p, _)) => format!("{}\\N{}", &flat[..p], &flat[p + 1..]),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lines_are_unbroken() {
        assert_eq!(balance("Merhaba\\Ndünya", 42), "Merhaba dünya");
        assert_eq!(unbreak("{\\i1}a \\N b{\\i0}\\Nc"), "{\\i1}a b{\\i0} c");
    }

    #[test]
    fn balances_and_prefers_punctuation() {
        let t = "Bugün okula gitmeyeceğim, çünkü hastayım ve evde kalmalıyım.";
        let b = balance(t, 42);
        assert_eq!(b, "Bugün okula gitmeyeceğim,\\Nçünkü hastayım ve evde kalmalıyım.");
        // virgül yoksa ortaya en yakın; ilk satır "ve" ile bitmez
        let b = balance("Kardeşimle birlikte sinemaya gittik ve çok güzel bir film izledik", 42);
        let first = b.split("\\N").next().unwrap();
        assert!(!first.ends_with(" ve"), "{b}");
        assert!(b.split("\\N").all(|l| l.chars().count() <= 42), "{b}");
    }

    #[test]
    fn keeps_tags_and_dialogue_dashes() {
        let b = balance("{\\i1}Bunu yapmamalıydın,{\\i0} şimdi hepimiz büyük bir belanın içindeyiz.", 42);
        // 19/42 yerine dengeli 33/28; etiketler yerinde kalır
        assert_eq!(b, "{\\i1}Bunu yapmamalıydın,{\\i0} şimdi hepimiz\\Nbüyük bir belanın içindeyiz.");
        assert_eq!(balance("- Geliyor musun? - Hayır.", 42), "- Geliyor musun?\\N- Hayır.");
        assert_eq!(balance("{\\p1}m 0 0 l 100 100 l 0 100{\\p0}", 10), "{\\p1}m 0 0 l 100 100 l 0 100{\\p0}");
        assert_eq!(balance("Tekkelimelikçokuzunbirsözcük", 10), "Tekkelimelikçokuzunbirsözcük");
    }
}
