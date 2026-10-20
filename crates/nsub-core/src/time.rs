//! Time helpers. All internal times are integer milliseconds.

/// Round to the nearest centisecond (ASS precision); ties round up.
pub fn round_cs(ms: i64) -> i64 {
    if ms < 0 {
        0
    } else {
        (ms + 5) / 10 * 10
    }
}

/// `H:MM:SS.cc`
pub fn format_ass(ms: i64) -> String {
    let cs_total = round_cs(ms) / 10;
    let cs = cs_total % 100;
    let s_total = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s_total / 3600, s_total / 60 % 60, s_total % 60, cs)
}

/// Parse `H:MM:SS.cc` (also tolerant of `.c`, `.ccc`, `,` separators).
pub fn parse_ass(s: &str) -> Option<i64> {
    parse_clock(s.trim())
}

/// `HH:MM:SS,mmm`
pub fn format_srt(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// `HH:MM:SS.mmm`
pub fn format_vtt(ms: i64) -> String {
    format_srt(ms).replace(',', ".")
}

/// Generic clock parser: `[[H:]M:]S[.,fraction]`.
pub fn parse_clock(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (clock, frac) = match s.find(['.', ',']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut secs: i64 = 0;
    for p in &parts {
        secs = secs * 60 + p.trim().parse::<i64>().ok()?;
    }
    let mut ms = 0i64;
    if !frac.is_empty() {
        if !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut digits: String = frac.chars().take(3).collect();
        while digits.len() < 3 {
            digits.push('0');
        }
        ms = digits.parse().ok()?;
    }
    Some(secs * 1000 + ms)
}

/// Frame ↔ time conversion for a constant frame rate.
#[derive(Clone, Copy, Debug)]
pub struct FrameRate {
    pub fps: f64,
}

impl FrameRate {
    pub fn new(fps: f64) -> Self {
        FrameRate { fps }
    }
    /// Start-time semantics (Aegisub): the first ms at which the frame is shown.
    pub fn frame_from_ms(&self, ms: i64) -> i64 {
        (ms as f64 * self.fps / 1000.0).floor() as i64
    }
    pub fn ms_from_frame(&self, frame: i64) -> i64 {
        (frame as f64 * 1000.0 / self.fps).ceil() as i64
    }
    /// Time in the middle of the frame is what renderers sample, and what Aegisub uses for end times.
    pub fn ms_from_frame_mid(&self, frame: i64) -> i64 {
        ((frame as f64 + 0.5) * 1000.0 / self.fps).round() as i64
    }
    /// Snap a start time to the frame boundary that is shown at that time.
    pub fn snap_start(&self, ms: i64) -> i64 {
        self.ms_from_frame(self.frame_from_ms(ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ass_roundtrip() {
        assert_eq!(format_ass(3_723_450), "1:02:03.45");
        assert_eq!(parse_ass("1:02:03.45"), Some(3_723_450));
        assert_eq!(format_ass(1005), "0:00:01.01");
    }
    #[test]
    fn srt() {
        assert_eq!(format_srt(3_723_456), "01:02:03,456");
        assert_eq!(parse_clock("01:02:03,456"), Some(3_723_456));
        assert_eq!(parse_clock("02:03.5"), Some(123_500));
        assert_eq!(parse_clock("a:b"), None);
    }
    #[test]
    fn frames() {
        let f = FrameRate::new(23.976);
        assert_eq!(f.frame_from_ms(f.ms_from_frame(100)), 100);
    }
}
