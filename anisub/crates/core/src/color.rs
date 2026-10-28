//! ASS colors. Stored as `0xAABBGGRR` (alpha 0 = opaque), matching the file format.

pub fn parse(s: &str) -> Option<u32> {
    let t = s.trim().trim_end_matches('&');
    let t = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")).or_else(|| t.strip_prefix("H"));
    match t {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => {
            let v = s.trim();
            if let Ok(n) = v.parse::<i64>() {
                Some(n as u32)
            } else {
                None
            }
        }
    }
}

pub fn format(c: u32) -> String {
    format!("&H{c:08X}")
}

pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (b as u32) << 16 | (g as u32) << 8 | r as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors() {
        assert_eq!(parse("&H00FFFFFF"), Some(0x00FFFFFF));
        assert_eq!(parse("&HFF&"), Some(0xFF));
        assert_eq!(parse("16777215"), Some(0xFFFFFF));
        assert_eq!(format(0x0000FF), "&H000000FF");
        assert_eq!(rgb(1, 2, 3), 0x030201);
    }
}
