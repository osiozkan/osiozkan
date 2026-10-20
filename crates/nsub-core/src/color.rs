//! ASS color/alpha handling (`&HAABBGGRR`, `&HBBGGRR&`, `&HAA&`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// ASS alpha: 0 = opaque, 255 = transparent.
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Parse any of: `&H00FFFFFF`, `&HFFFFFF&`, `&HFFFFFF`, `H..`, or decimal (SSA style lines).
    pub fn parse(s: &str) -> Option<Color> {
        let t = s.trim();
        let t = t.trim_end_matches('&');
        let digits = if let Some(rest) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")) {
            rest
        } else if let Some(rest) = t.strip_prefix('H').or_else(|| t.strip_prefix('h')) {
            rest
        } else if let Some(rest) = t.strip_prefix("0x") {
            rest
        } else {
            // decimal (possibly negative) as in old SSA
            let v: i64 = t.parse().ok()?;
            let v = v as u32;
            return Some(Color { b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8, a: (v >> 24) as u8 });
        };
        if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(digits, 16).ok()?;
        Some(Color { b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8, a: (v >> 24) as u8 })
    }

    /// `&HAABBGGRR` as used in style lines.
    pub fn to_style_string(self) -> String {
        format!("&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }

    /// `&HBBGGRR&` as used in override tags.
    pub fn to_tag_string(self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }

    pub fn alpha_tag_string(self) -> String {
        format!("&H{:02X}&", self.a)
    }

    pub fn to_html(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn from_html(s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Color { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8, a: 0 })
    }

    pub fn lerp(self, other: Color, t: f64) -> Color {
        let f = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round().clamp(0.0, 255.0) as u8;
        Color { r: f(self.r, other.r), g: f(self.g, other.g), b: f(self.b, other.b), a: f(self.a, other.a) }
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
        assert_eq!(Color::parse("&HFF8040&").unwrap().to_tag_string(), "&HFF8040&");
        assert_eq!(Color::parse("16777215").unwrap().r, 255);
        assert!(Color::parse("zz").is_none());
        assert_eq!(Color::from_html("#ff0000").unwrap().to_tag_string(), "&H0000FF&");
    }
}
