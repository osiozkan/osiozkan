//! ASS colors: stored as 0xAABBGGRR (ASS alpha: 0 = opaque).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color(((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32)
    }
    pub fn r(&self) -> u8 { self.0 as u8 }
    pub fn g(&self) -> u8 { (self.0 >> 8) as u8 }
    pub fn b(&self) -> u8 { (self.0 >> 16) as u8 }
    pub fn a(&self) -> u8 { (self.0 >> 24) as u8 }

    /// Parse `&HAABBGGRR&`, `&HBBGGRR&`, `&H...`, or decimal (SSA).
    pub fn parse(s: &str) -> Option<Color> {
        let t = s.trim().trim_end_matches('&');
        let t = t.trim_start_matches('&');
        if let Some(h) = t.strip_prefix(['H', 'h']) {
            let v = u32::from_str_radix(h.trim_start_matches(['H', 'h']), 16).ok()?;
            Some(Color(v))
        } else if let Some(h) = t.strip_prefix('$') {
            Some(Color(u32::from_str_radix(h, 16).ok()?))
        } else {
            t.parse::<i64>().ok().map(|v| Color(v as u32))
        }
    }

    /// Style-line form: `&HAABBGGRR`.
    pub fn to_style(&self) -> String {
        format!("&H{:08X}", self.0)
    }
    /// Override-tag form: `&HBBGGRR&`.
    pub fn to_tag(&self) -> String {
        format!("&H{:02X}{:02X}{:02X}&", self.b(), self.g(), self.r())
    }
    pub fn alpha_tag(&self) -> String {
        format!("&H{:02X}&", self.a())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_fmt() {
        let c = Color::parse("&H00FFFFFF").unwrap();
        assert_eq!(c.to_style(), "&H00FFFFFF");
        let c = Color::parse("&H0000FF&").unwrap();
        assert_eq!((c.r(), c.g(), c.b()), (255, 0, 0));
        assert_eq!(c.to_tag(), "&H0000FF&");
    }
}
