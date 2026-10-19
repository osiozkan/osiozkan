//! ASS colors. Stored as RGBA with ASS-style alpha (0 = opaque).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 0 }
    }

    /// Parse `&H00BBGGRR`, `&HBBGGRR&`, `H..`, or decimal integer.
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim();
        let v: u32 = if let Some(h) = s.strip_prefix("&H").or_else(|| s.strip_prefix("&h")).or_else(|| s.strip_prefix('H')) {
            let h = h.trim_end_matches('&');
            if h.is_empty() {
                return None;
            }
            u32::from_str_radix(h, 16).ok()?
        } else if let Some(neg) = s.strip_prefix('-') {
            let n: i64 = neg.parse().ok()?;
            (-n) as i32 as u32
        } else {
            s.parse::<u64>().ok()? as u32
        };
        Some(Color { a: (v >> 24) as u8, b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8 })
    }

    /// Style form: `&HAABBGGRR`.
    pub fn to_style(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }

    /// Override form: `&HBBGGRR&`.
    pub fn to_override(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }

    pub fn alpha_override(a: u8) -> String {
        format!("&H{a:02X}&")
    }

    pub fn to_hex_rgb(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors() {
        let c = Color::parse("&H00FF8040").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (0x40, 0x80, 0xFF, 0));
        assert_eq!(c.to_style(), "&H00FF8040");
        assert_eq!(c.to_override(), "&HFF8040&");
        assert_eq!(Color::parse("16777215").unwrap().r, 255);
    }
}
