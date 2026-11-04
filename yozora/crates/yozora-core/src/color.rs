//! ASS renk/alfa türleri. ASS'te renk `&HAABBGGRR` sırasındadır.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// ASS alfası: 0 = opak, 255 = şeffaf.
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// `&H00BBGGRR`, `&HBBGGRR&`, `H..` ya da ondalık tamsayı biçimlerini çözer.
    pub fn parse(s: &str) -> Option<Color> {
        let t = s.trim().trim_end_matches('&');
        let v: u32 = if let Some(h) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")).or_else(|| t.strip_prefix('H')).or_else(|| t.strip_prefix('h')) {
            if h.is_empty() {
                return None;
            }
            u32::from_str_radix(h, 16).ok()?
        } else if let Ok(i) = t.parse::<i64>() {
            i as u32
        } else {
            return None;
        };
        Some(Color { a: (v >> 24) as u8, b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8 })
    }

    /// Stil satırı biçimi: `&HAABBGGRR`.
    pub fn to_style_string(self) -> String {
        format!("&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }

    /// Override etiketi biçimi: `&HBBGGRR&`.
    pub fn to_tag_string(self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }

    /// `#RRGGBB` (arayüz/CSS).
    pub fn to_hex_rgb(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_forms() {
        let c = Color::parse("&H00FF8040").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (0x40, 0x80, 0xFF, 0));
        assert_eq!(c.to_style_string(), "&H00FF8040");
        assert_eq!(c.to_tag_string(), "&HFF8040&");
        assert_eq!(Color::parse("16777215").unwrap(), Color { r: 255, g: 255, b: 255, a: 0 });
        assert!(Color::parse("zzz").is_none());
    }
}
