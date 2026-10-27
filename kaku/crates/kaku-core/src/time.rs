//! Time helpers. All internal times are milliseconds (`i64`).

/// Parse `h:mm:ss.cc` (also tolerates `m:ss`, `ss.mmm`, and `,` as decimal separator).
pub fn parse_ass_time(s: &str) -> Option<i64> {
    let s = s.trim().replace(',', ".");
    let mut parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let sec_part = parts.pop()?;
    let (sec_i, frac) = match sec_part.split_once('.') {
        Some((a, b)) => (a, b),
        None => (sec_part, ""),
    };
    let sec: i64 = if sec_i.is_empty() { 0 } else { sec_i.trim().parse().ok()? };
    let ms = frac_to_ms(frac)?;
    let min: i64 = match parts.pop() {
        Some(p) => p.trim().parse().ok()?,
        None => 0,
    };
    let hour: i64 = match parts.pop() {
        Some(p) => p.trim().parse().ok()?,
        None => 0,
    };
    Some(((hour * 60 + min) * 60 + sec) * 1000 + ms)
}

fn frac_to_ms(frac: &str) -> Option<i64> {
    if frac.is_empty() {
        return Some(0);
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut f: String = frac.chars().take(3).collect();
    while f.len() < 3 {
        f.push('0');
    }
    f.parse().ok()
}

/// Round to centiseconds (ASS precision).
pub fn round_cs(ms: i64) -> i64 {
    // Aegisub rounds to nearest cs; ties go up.
    (ms + 5).div_euclid(10) * 10
}

/// Format as `h:mm:ss.cc` after rounding to centiseconds.
pub fn format_ass_time(ms: i64) -> String {
    let cs_total = round_cs(ms.max(0)) / 10;
    let cs = cs_total % 100;
    let s = (cs_total / 100) % 60;
    let m = (cs_total / 6000) % 60;
    let h = cs_total / 360000;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// Format as SRT `hh:mm:ss,mmm`.
pub fn format_srt_time(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Format as WebVTT `hh:mm:ss.mmm`.
pub fn format_vtt_time(ms: i64) -> String {
    format_srt_time(ms).replace(',', ".")
}

/// Frame ↔ time conversion for constant frame rate video.
#[derive(Debug, Clone, Copy)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub const NTSC_FILM: Fps = Fps { num: 24000, den: 1001 };
    pub const FILM: Fps = Fps { num: 24, den: 1 };
    pub const fn new(num: u32, den: u32) -> Self {
        Fps { num, den }
    }
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }
    /// Start time of a frame (the earliest ms that displays this frame).
    pub fn frame_to_ms(&self, frame: i64) -> i64 {
        // Aegisub convention: time of frame n is midpoint-rounded so that
        // ms_to_frame(frame_to_ms(n)) == n.
        let t = frame as f64 * 1000.0 * self.den as f64 / self.num as f64;
        t.ceil() as i64
    }
    pub fn ms_to_frame(&self, ms: i64) -> i64 {
        (ms as f64 * self.num as f64 / (1000.0 * self.den as f64)).floor() as i64
    }
    /// Start-time of a line that should *show* at frame `f` (middle of the frame).
    pub fn frame_mid_ms(&self, frame: i64) -> i64 {
        (self.frame_to_ms(frame) + self.frame_to_ms(frame + 1)) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass_time("0:01:02.34"), Some(62_340));
        assert_eq!(format_ass_time(62_340), "0:01:02.34");
        assert_eq!(parse_ass_time("1:02,5"), Some(62_500));
        assert_eq!(format_srt_time(3_723_004), "01:02:03,004");
        assert_eq!(format_ass_time(1_005), "0:00:01.01");
        assert_eq!(parse_ass_time("x"), None);
    }
    #[test]
    fn frames() {
        let f = Fps::NTSC_FILM;
        for n in 0..2000 {
            assert_eq!(f.ms_to_frame(f.frame_to_ms(n)), n);
        }
    }
}
