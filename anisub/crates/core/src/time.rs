//! Time helpers. All internal times are milliseconds.

pub type Ms = i64;

/// Format as ASS `h:mm:ss.cc` (rounded to nearest centisecond, like Aegisub).
pub fn format_ass(ms: Ms) -> String {
    let cs_total = (ms.max(0) + 5) / 10;
    let cs = cs_total % 100;
    let s = (cs_total / 100) % 60;
    let m = (cs_total / 6000) % 60;
    let h = cs_total / 360000;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// Format as `hh:mm:ss<sep>mmm` (SRT uses ',', WebVTT uses '.').
pub fn format_clock(ms: Ms, sep: char) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02}{}{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        sep,
        ms % 1000
    )
}

/// Parse `h:mm:ss.cc`, `hh:mm:ss,mmm`, `mm:ss.mmm` or plain seconds.
/// Fractional digits are interpreted by their count (2 = centiseconds, 3 = ms).
pub fn parse(s: &str) -> Option<Ms> {
    let s = s.trim();
    let (main, frac) = match s.find(['.', ',']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let mut total: Ms = 0;
    for part in main.split(':') {
        total = total * 60 + part.trim().parse::<Ms>().ok()?;
    }
    total *= 1000;
    if !frac.is_empty() {
        if !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits: String = frac.chars().take(3).collect();
        let scale = 10_i64.pow(3 - digits.len() as u32);
        total += digits.parse::<Ms>().ok()? * scale;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(format_ass(3_723_450), "1:02:03.45");
        assert_eq!(format_ass(1_004), "0:00:01.00");
        assert_eq!(format_ass(1_005), "0:00:01.01");
        assert_eq!(parse("1:02:03.45"), Some(3_723_450));
        assert_eq!(parse("00:00:01,500"), Some(1_500));
        assert_eq!(parse("02:03.5"), Some(123_500));
        assert_eq!(format_clock(3_723_450, ','), "01:02:03,450");
        assert_eq!(parse("x"), None);
    }
}
