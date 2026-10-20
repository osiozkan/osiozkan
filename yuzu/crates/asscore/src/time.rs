//! Time helpers: millisecond timestamps and the text formats used by ASS, SRT and WebVTT.

/// Parse `h:mm:ss.cc` (ASS). Tolerates 1-3 fractional digits and missing hours.
pub fn parse_ass(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (main, frac) = match s.split_once(['.', ',']) {
        Some((m, f)) => (m, f),
        None => (s, ""),
    };
    let parts: Vec<&str> = main.split(':').collect();
    let nums: Option<Vec<i64>> = parts.iter().map(|p| p.trim().parse::<i64>().ok()).collect();
    let nums = nums?;
    let secs = match nums.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        [s] => *s,
        _ => return None,
    };
    let ms = if frac.is_empty() {
        0
    } else {
        if !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut f: String = frac.chars().take(3).collect();
        while f.len() < 3 {
            f.push('0');
        }
        f.parse::<i64>().ok()?
    };
    let t = secs * 1000 + ms;
    Some(if neg { -t } else { t })
}

/// ASS timestamps have centisecond precision; Aegisub rounds toward the nearest cs.
pub fn format_ass(ms: i64) -> String {
    let ms = ms.max(0);
    let cs = (ms + 5) / 10;
    let h = cs / 360000;
    let m = (cs / 6000) % 60;
    let s = (cs / 100) % 60;
    format!("{}:{:02}:{:02}.{:02}", h, m, s, cs % 100)
}

pub fn format_srt(ms: i64) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

pub fn format_vtt(ms: i64) -> String {
    format_srt(ms).replace(',', ".")
}

/// Round a time to the ASS centisecond grid.
pub fn snap_cs(ms: i64) -> i64 {
    ((ms + 5).div_euclid(10)) * 10
}

/// Frame tables: constant frame rate conversions (Aegisub-compatible semantics:
/// a frame's start time is the time at which it is first shown).
#[derive(Clone, Copy, Debug)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub const NTSC: Fps = Fps { num: 24000, den: 1001 };
    pub const FILM: Fps = Fps { num: 24, den: 1 };
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }
    /// Start time of a frame in ms (rounded to nearest).
    pub fn ms_from_frame(&self, frame: i64) -> i64 {
        ((frame as f64) * 1000.0 * self.den as f64 / self.num as f64).round() as i64
    }
    /// Frame displayed at time `ms`.
    pub fn frame_from_ms(&self, ms: i64) -> i64 {
        // Add half-ms epsilon so exact boundaries land on the new frame.
        ((ms as f64 + 0.5) * self.num as f64 / (1000.0 * self.den as f64)).floor() as i64
    }
    /// Aegisub "start" semantic: time at which frame starts showing.
    pub fn start_ms(&self, frame: i64) -> i64 {
        self.ms_from_frame(frame)
    }
    /// Aegisub "end" semantic: last ms before the following frame.
    pub fn end_ms(&self, frame: i64) -> i64 {
        self.ms_from_frame(frame + 1)
    }

    /// Frame on screen at `ms` (Aegisub `EXACT`): the last frame whose time is ≤ `ms`.
    pub fn frame_at(&self, ms: i64) -> i64 {
        let mut f = ((ms as f64) * self.num as f64 / (1000.0 * self.den as f64)).floor() as i64;
        while self.ms_from_frame(f + 1) <= ms { f += 1; }
        while f > 0 && self.ms_from_frame(f) > ms { f -= 1; }
        f.max(0)
    }

    /// Time for a line that should first appear on `frame` (Aegisub `START`): halfway between
    /// the previous frame and this one, so ASS's centisecond rounding cannot move it a frame.
    pub fn line_start(&self, frame: i64) -> i64 {
        if frame <= 0 { return 0; }
        let (p, c) = (self.ms_from_frame(frame - 1), self.ms_from_frame(frame));
        p + (c - p + 1) / 2
    }

    /// End time for a line whose last visible frame is `frame` (Aegisub `END`).
    pub fn line_end(&self, frame: i64) -> i64 {
        let (c, n) = (self.ms_from_frame(frame.max(0)), self.ms_from_frame(frame.max(0) + 1));
        c + (n - c + 1) / 2
    }

    /// First frame a line starting at `ms` is visible on (Aegisub `FrameAtTime(ms, START)`).
    pub fn frame_for_start(&self, ms: i64) -> i64 {
        if ms <= 0 { return 0; }
        self.frame_at(ms - 1) + 1
    }

    /// Last frame a line ending at `ms` is visible on (Aegisub `FrameAtTime(ms, END)`).
    pub fn frame_for_end(&self, ms: i64) -> i64 {
        if ms <= 0 { return -1; }
        self.frame_at(ms - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass("0:01:02.50"), Some(62_500));
        assert_eq!(format_ass(62_500), "0:01:02.50");
        assert_eq!(parse_ass("1:00:00.00"), Some(3_600_000));
        assert_eq!(parse_ass("0:00:01.5"), Some(1500));
        assert_eq!(format_srt(3_723_456), "01:02:03,456");
        assert_eq!(format_vtt(3_723_456), "01:02:03.456");
        assert_eq!(parse_ass("garbage"), None);
    }
    #[test]
    fn frames() {
        let f = Fps::NTSC;
        assert_eq!(f.frame_from_ms(0), 0);
        assert_eq!(f.frame_from_ms(f.ms_from_frame(100)), 100);
        assert_eq!(f.frame_from_ms(f.ms_from_frame(100) - 1), 99);
    }

    /// libass shows a line on a frame when start <= frame time < end; the times we write must
    /// select exactly the intended frames even after rounding to centiseconds, for container
    /// timestamps rounded to the millisecond (MKV) or exact (MP4).
    #[test]
    fn line_times_survive_centisecond_rounding() {
        for fps in [Fps::NTSC, Fps::FILM, Fps { num: 25, den: 1 }, Fps { num: 30000, den: 1001 }, Fps { num: 60000, den: 1001 }] {
            let exact = |n: i64| n as f64 * 1000.0 * fps.den as f64 / fps.num as f64;
            for n in 1..3000 {
                let s = parse_ass(&format_ass(fps.line_start(n))).unwrap();
                let e = parse_ass(&format_ass(fps.line_end(n + 10))).unwrap();
                for pts in [exact(n).round(), exact(n)] {
                    assert!(s as f64 <= pts, "{:?} frame {}: start {} after frame time {}", fps, n, s, pts);
                }
                for pts in [exact(n - 1).round(), exact(n - 1)] {
                    assert!((s as f64) > pts, "{:?} frame {}: start {} not after previous frame {}", fps, n, s, pts);
                }
                // visible on frame n+10, not on n+11
                assert!((e as f64) > exact(n + 10).round() && (e as f64) > exact(n + 10));
                assert!((e as f64) <= exact(n + 11).round() && (e as f64) <= exact(n + 11));
                assert_eq!(fps.frame_for_start(fps.line_start(n)), n);
                assert_eq!(fps.frame_for_end(fps.line_end(n)), n);
                assert_eq!(fps.frame_at(fps.ms_from_frame(n)), n);
                assert_eq!(fps.frame_at(fps.ms_from_frame(n) - 1), n - 1);
            }
        }
        assert_eq!(Fps::NTSC.line_start(0), 0);
        assert_eq!(Fps::NTSC.frame_for_end(0), -1);
    }
}
