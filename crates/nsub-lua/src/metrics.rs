//! Text measurement used by `aegisub.text_extents` (and thus karaskel positioning).
//!
//! The default implementation is a proportional-width heuristic that is stable across machines.
//! A font-backed implementation can be plugged in via [`Automation::set_metrics`].

use nsub_core::Style;

#[derive(Clone, Copy, Debug, Default)]
pub struct Extents {
    pub width: f64,
    pub height: f64,
    pub descent: f64,
    pub extlead: f64,
}

pub trait TextMetrics {
    fn extents(&self, style: &Style, text: &str) -> Extents;
}

pub struct HeuristicMetrics;

fn glyph_em(c: char) -> f64 {
    match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' | 'ı' | 'İ' => 0.26,
        'f' | 't' | 'r' | 'I' | '(' | ')' | '[' | ']' | '"' | '-' => 0.34,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.85,
        c if c.is_ascii_uppercase() => 0.68,
        c if c.is_ascii_digit() => 0.55,
        c if (c as u32) >= 0x2E80 => 1.0, // CJK full width
        _ => 0.52,
    }
}

impl TextMetrics for HeuristicMetrics {
    fn extents(&self, style: &Style, text: &str) -> Extents {
        let size = style.fontsize;
        let em: f64 = text.chars().map(glyph_em).sum();
        let n = text.chars().count() as f64;
        let bold = if style.bold { 1.07 } else { 1.0 };
        Extents {
            width: (em * size * bold + n * style.spacing) * style.scale_x / 100.0,
            height: size * 1.15 * style.scale_y / 100.0,
            descent: size * 0.2 * style.scale_y / 100.0,
            extlead: 0.0,
        }
    }
}
