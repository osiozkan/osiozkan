//! Time helpers. All times are integer milliseconds.
pub type Ms = i64;

/// Round to centiseconds (ASS precision), half up.
pub fn round_cs(ms: Ms) -> Ms {
    ((ms + 5).div_euclid(10)) * 10
}

/// `h:mm:ss.cc`
pub fn format_ass(ms: Ms) -> String {
    let cs_total = round_cs(ms.max(0)) / 10;
    let cs = cs_total % 100;
    let s = (cs_total / 100) % 60;
    let m = (cs_total / 6000) % 60;
    let h = cs_total / 360000;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// `hh:mm:ss,mmm` (SRT) or with `.` (VTT)
pub fn format_clock(ms: Ms, sep: char) -> String {
    let ms = ms.max(0);
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    let f = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}{sep}{f:03}")
}

/// Parse `h:mm:ss.cc`, `hh:mm:ss,mmm`, `mm:ss.mmm`, `ss.mmm` leniently.
pub fn parse_time(s: &str) -> Option<Ms> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (main, frac) = match s.find(['.', ',']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let mut total: i64 = 0;
    for part in main.split(':') {
        let v: i64 = part.trim().parse().ok()?;
        total = total * 60 + v;
    }
    let mut ms = total * 1000;
    if !frac.is_empty() {
        if !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits: String = frac.chars().take(3).collect();
        let mut v: i64 = digits.parse().ok()?;
        for _ in digits.len()..3 {
            v *= 10;
        }
        ms += v;
    }
    Some(if neg { -ms } else { ms })
}

/// Frame rate as a rational.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub const NTSC_FILM: Fps = Fps { num: 24000, den: 1001 };
    pub const FILM: Fps = Fps { num: 24, den: 1 };
    pub const PAL: Fps = Fps { num: 25, den: 1 };
    pub const NTSC: Fps = Fps { num: 30000, den: 1001 };

    pub fn from_f64(f: f64) -> Fps {
        for (n, d) in [(24000, 1001), (30000, 1001), (60000, 1001), (48000, 1001)] {
            if (f - n as f64 / d as f64).abs() < 0.005 {
                return Fps { num: n, den: d };
            }
        }
        Fps { num: (f * 1000.0).round() as u32, den: 1000 }
    }

    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Frame displayed at `ms`.
    pub fn frame_at(&self, ms: Ms) -> i64 {
        (ms * self.num as i64).div_euclid(self.den as i64 * 1000)
    }

    /// First millisecond of `frame` (what a subtitle START snaps to).
    pub fn start_ms(&self, frame: i64) -> Ms {
        let n = frame * self.den as i64 * 1000;
        (n + self.num as i64 - 1).div_euclid(self.num as i64)
    }

    /// Time to use as END for a subtitle whose last visible frame is `frame`.
    pub fn end_ms(&self, frame: i64) -> Ms {
        self.start_ms(frame + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_time("0:01:02.50"), Some(62_500));
        assert_eq!(parse_time("00:00:01,5"), Some(1_500));
        assert_eq!(format_ass(62_504), "0:01:02.50");
        assert_eq!(format_ass(62_505), "0:01:02.51");
        assert_eq!(format_clock(3_723_004, ','), "01:02:03,004");
    }
    #[test]
    fn frames() {
        let f = Fps::NTSC_FILM;
        assert_eq!(f.frame_at(0), 0);
        assert_eq!(f.start_ms(24), 1001);
        assert_eq!(f.frame_at(f.start_ms(1000)), 1000);
    }
}
