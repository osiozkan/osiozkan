//! Time helpers. All times are integer milliseconds.

/// Format as ASS `h:mm:ss.cc` (rounded to nearest centisecond like Aegisub).
pub fn fmt_ass(ms: i64) -> String {
    let ms = ms.max(0);
    let cs_total = (ms + 5) / 10;
    let cs = cs_total % 100;
    let s_total = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s_total / 3600, (s_total / 60) % 60, s_total % 60, cs)
}

/// Parse `h:mm:ss.cc` (also tolerates `.ms`, `,ms`, missing hours).
pub fn parse_ass(s: &str) -> Option<i64> {
    parse_generic(s)
}

pub fn fmt_srt(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn fmt_vtt(ms: i64) -> String {
    fmt_srt(ms).replace(',', ".")
}

pub fn parse_generic(s: &str) -> Option<i64> {
    let s = s.trim();
    let (main, frac) = match s.find(|c| c == '.' || c == ',') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let parts: Vec<&str> = main.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut total: i64 = 0;
    for p in &parts {
        total = total * 60 + p.trim().parse::<i64>().ok()?;
    }
    let mut ms = total * 1000;
    if !frac.is_empty() {
        if !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // 2 digits = centiseconds, 3 = ms, 1 = deciseconds
        let mut f: String = frac.chars().take(3).collect();
        while f.len() < 3 {
            f.push('0');
        }
        ms += f.parse::<i64>().ok()?;
    }
    Some(ms)
}

/// Frame <-> ms conversion for a constant frame rate (fps as rational num/den).
#[derive(Clone, Copy, Debug)]
pub struct FrameRate {
    pub num: f64,
    pub den: f64,
}

impl FrameRate {
    pub fn new(fps: f64) -> Self {
        if (fps - 23.976).abs() < 0.001 {
            return Self { num: 24000.0, den: 1001.0 };
        }
        if (fps - 29.97).abs() < 0.001 {
            return Self { num: 30000.0, den: 1001.0 };
        }
        if (fps - 59.94).abs() < 0.001 {
            return Self { num: 60000.0, den: 1001.0 };
        }
        Self { num: fps, den: 1.0 }
    }
    pub fn fps(&self) -> f64 {
        self.num / self.den
    }
    /// Frame start time (ms) like Aegisub's `ms_from_frame` for CFR (frame centre rounding).
    pub fn ms_from_frame(&self, frame: i64) -> i64 {
        ((frame as f64) * 1000.0 * self.den / self.num).round() as i64
    }
    pub fn frame_from_ms(&self, ms: i64) -> i64 {
        ((ms as f64) * self.num / (1000.0 * self.den)).floor() as i64
    }
    /// Time to use as a line start so the line appears exactly on `frame`.
    pub fn start_ms_for_frame(&self, frame: i64) -> i64 {
        let a = self.ms_from_frame(frame - 1);
        let b = self.ms_from_frame(frame);
        (a + b) / 2 + 1
    }
    pub fn end_ms_for_frame(&self, frame: i64) -> i64 {
        self.start_ms_for_frame(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ass_roundtrip() {
        assert_eq!(parse_ass("0:01:02.50"), Some(62_500));
        assert_eq!(fmt_ass(62_500), "0:01:02.50");
        assert_eq!(fmt_ass(62_504), "0:01:02.50");
        assert_eq!(fmt_ass(62_505), "0:01:02.51");
    }
    #[test]
    fn srt() {
        assert_eq!(parse_generic("00:00:01,500"), Some(1500));
        assert_eq!(fmt_srt(3_661_001), "01:01:01,001");
    }
    #[test]
    fn frames() {
        let f = FrameRate::new(23.976);
        assert_eq!(f.frame_from_ms(f.ms_from_frame(100) + 1), 100);
    }
}
