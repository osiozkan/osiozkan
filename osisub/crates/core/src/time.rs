//! Time helpers. All times are integer milliseconds.

/// Parse ASS `h:mm:ss.cc` (also tolerates `.ccc`, and 1-2 digit fractions).
pub fn parse_ass_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let (hms, frac) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    };
    let mut it = hms.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let sec: i64 = it.next()?.trim().parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(((h * 60 + m) * 60 + sec) * 1000 + frac_to_ms(frac)?)
}

fn frac_to_ms(frac: &str) -> Option<i64> {
    if frac.is_empty() {
        return Some(0);
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: i64 = 0;
    for (i, b) in frac.bytes().take(3).enumerate() {
        v += (b - b'0') as i64 * [100, 10, 1][i];
    }
    Some(v)
}

/// Format as ASS time, rounding to the nearest centisecond (like Aegisub).
pub fn format_ass_time(ms: i64) -> String {
    let cs_total = (ms.max(0) + 5) / 10;
    let cs = cs_total % 100;
    let s = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s / 3600, s / 60 % 60, s % 60, cs)
}

/// Parse `hh:mm:ss,mmm` or `hh:mm:ss.mmm` (SRT / WebVTT; hours optional for VTT).
pub fn parse_srt_time(s: &str) -> Option<i64> {
    let s = s.trim().replace(',', ".");
    let (hms, frac) = s.split_once('.')?;
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?, s.parse::<i64>().ok()?),
        [m, s] => (0, m.parse::<i64>().ok()?, s.parse::<i64>().ok()?),
        _ => return None,
    };
    Some(((h * 60 + m) * 60 + sec) * 1000 + frac_to_ms(frac)?)
}

pub fn format_srt_time(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, sep, ms % 1000)
}

/// Frame -> start time in ms for a constant frame rate.
pub fn frame_to_ms(frame: i64, fps: f64) -> i64 {
    (frame as f64 * 1000.0 / fps).round() as i64
}

/// ms -> frame index (the frame being displayed at that time).
pub fn ms_to_frame(ms: i64, fps: f64) -> i64 {
    (ms as f64 * fps / 1000.0 + 1e-6).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ass_roundtrip() {
        assert_eq!(parse_ass_time("0:01:02.50"), Some(62_500));
        assert_eq!(format_ass_time(62_500), "0:01:02.50");
        assert_eq!(format_ass_time(62_504), "0:01:02.50");
        assert_eq!(format_ass_time(62_505), "0:01:02.51");
    }
    #[test]
    fn srt() {
        assert_eq!(parse_srt_time("01:00:00,123"), Some(3_600_123));
        assert_eq!(format_srt_time(3_600_123, ','), "01:00:00,123");
        assert_eq!(parse_srt_time("00:05.500"), Some(5_500));
    }
    #[test]
    fn frames() {
        assert_eq!(ms_to_frame(frame_to_ms(100, 23.976), 23.976), 100);
    }
}
