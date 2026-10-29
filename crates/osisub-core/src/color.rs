//! ASS renkleri: `&HAABBGGRR` (stil) ve `&HBBGGRR&` (etiket).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// ASS alfa: 0 = opak, 255 = saydam.
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    /// `&H..&`/`H..` önekliyse onaltılık, öneksizse (eski SSA) ondalık; ondalık başarısızsa onaltılık denenir.
    pub fn parse(s: &str) -> Option<Color> {
        let raw = s.trim();
        let hex_prefixed = raw.starts_with('&') || raw.starts_with('H') || raw.starts_with('h');
        let t = raw.trim_start_matches('&').trim_end_matches('&');
        let t = t.strip_prefix(['H', 'h']).unwrap_or(t);
        let v = if hex_prefixed {
            u32::from_str_radix(t, 16).ok()?
        } else if let Ok(d) = t.parse::<i64>() {
            d as u32
        } else {
            u32::from_str_radix(t, 16).ok()?
        };
        Some(Color { a: (v >> 24) as u8, b: (v >> 16) as u8, g: (v >> 8) as u8, r: v as u8 })
    }

    /// Stil biçimi: `&H00BBGGRR`.
    pub fn to_style_string(self) -> String {
        format!("&H{:02X}{:02X}{:02X}{:02X}", self.a, self.b, self.g, self.r)
    }

    /// Etiket biçimi: `&HBBGGRR&`.
    pub fn to_tag_string(self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b, self.g, self.r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors() {
        let c = Color::parse("&H00FF8040").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (0x40, 0x80, 0xFF, 0));
        assert_eq!(c.to_style_string(), "&H00FF8040");
        assert_eq!(c.to_tag_string(), "&HFF8040&");
        assert_eq!(Color::parse("16777215").unwrap().to_style_string(), "&H00FFFFFF");
        assert_eq!(Color::parse("&HFF&").unwrap().r, 255);
    }
}
