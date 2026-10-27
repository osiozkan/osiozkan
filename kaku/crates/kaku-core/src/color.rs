//! ASS colour handling. Stored as 0xAABBGGRR where alpha 0 = opaque.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub a: u8,
    pub b: u8,
    pub g: u8,
    pub r: u8,
}

impl Color {
    pub const WHITE: Color = Color { a: 0, b: 255, g: 255, r: 255 };
    pub const BLACK: Color = Color { a: 0, b: 0, g: 0, r: 0 };
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { a: 0, b, g, r }
    }
    /// Parse `&HAABBGGRR&`, `&HBBGGRR&`, `H..`, or a decimal number.
    pub fn parse(s: &str) -> Option<Color> {
        let t = s.trim().trim_end_matches('&');
        let t = t.trim_start_matches('&');
        let v: u32 = if let Some(h) = t.strip_prefix(['H', 'h']) {
            u32::from_str_radix(h, 16).ok()?
        } else if t.is_empty() {
            0
        } else if let Some(neg) = t.strip_prefix('-') {
            // Very old scripts store signed 32-bit ints.
            (-neg.parse::<i64>().ok()?) as i32 as u32
        } else {
            t.parse::<u64>().ok()? as u32
        };
        Some(Color { a: (v >> 24) as u8, b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8 })
    }
    pub fn to_u32(self) -> u32 {
        (self.a as u32) << 24 | (self.b as u32) << 16 | (self.g as u32) << 8 | self.r as u32
    }
    /// `&HBBGGRR&` as used in override tags.
    pub fn tag(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }
    /// `&HAA&` as used by `\alpha`.
    pub fn alpha_tag(&self) -> String {
        format!("&H{:02X}&", self.a)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_formats() {
        assert_eq!(Color::parse("&H00FFFFFF").unwrap(), Color::WHITE);
        assert_eq!(Color::parse("&HFF&").unwrap().r, 255);
        assert_eq!(Color::parse("16777215").unwrap().b, 255);
        assert_eq!(Color::parse("&H80000000").unwrap().a, 0x80);
        assert_eq!(Color::rgb(1, 2, 3).to_string(), "&H00030201");
    }
}
