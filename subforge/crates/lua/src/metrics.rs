//! Text measurement used by `aegisub.text_extents` (and karaskel layout).
//!
//! Without a real font rasterizer available we estimate advance widths from
//! character classes. Karaoke templates only need *consistent* relative widths;
//! accurate font-backed metrics are on the roadmap.
use subforge_core::ass::Style;

fn char_em(c: char) -> f64 {
    match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | 'ı' | 'ǀ' => 0.26,
        'f' | 't' | 'r' | 'I' | '(' | ')' | '[' | ']' | '-' | '"' => 0.34,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.86,
        c if c.is_ascii_uppercase() || matches!(c, 'İ' | 'Ş' | 'Ç' | 'Ö' | 'Ü' | 'Ğ') => 0.68,
        c if (c as u32) >= 0x2E80 => 1.0, // CJK full-width
        _ => 0.54,
    }
}

/// (width, height, descent, external leading)
pub fn text_extents(style: &Style, text: &str) -> (f64, f64, f64, f64) {
    let size = style.fontsize.max(1.0);
    let bold = if style.bold { 1.06 } else { 1.0 };
    let mut w = 0.0;
    let mut n = 0.0;
    for c in text.chars() {
        w += char_em(c) * size * bold + style.spacing;
        n += 1.0;
    }
    let _ = n;
    let w = w * style.scale_x / 100.0;
    let h = size * 1.18 * style.scale_y / 100.0;
    let descent = size * 0.22 * style.scale_y / 100.0;
    (w, h, descent, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wider_text_is_wider() {
        let s = Style::default();
        assert!(text_extents(&s, "WWWW").0 > text_extents(&s, "iiii").0);
        assert_eq!(text_extents(&s, "").0, 0.0);
    }
}
