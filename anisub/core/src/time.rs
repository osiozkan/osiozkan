//! Timecode helpers (milliseconds based).

/// Format milliseconds as ASS time `H:MM:SS.cc` (centisecond precision, rounded down like Aegisub).
pub fn format_ass(ms: i64) -> String {
    let ms = ms.max(0);
    let cs = ms / 10;
    let (h, m, s, c) = (cs / 360000, (cs / 6000) % 60, (cs / 100) % 60, cs % 100);
    format!("{h}:{m:02}:{s:02}.{c:02}")
}

/// Parse `H:MM:SS.cc` (also tolerates `.mmm`, missing hours, and `,` as separator).
pub fn parse_ass(s: &str) -> Option<i64> {
    parse_generic(s)
}

pub fn format_srt(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn parse_srt(s: &str) -> Option<i64> {
    parse_generic(s)
}

fn parse_generic(s: &str) -> Option<i64> {
    let s = s.trim().replace(',', ".");
    let (hms, frac) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s.as_str(), ""),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    let nums: Option<Vec<i64>> = parts.iter().map(|p| p.trim().parse::<i64>().ok()).collect();
    let nums = nums?;
    let secs = match nums.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        [s] => *s,
        _ => return None,
    };
    let frac_ms = if frac.is_empty() {
        0
    } else {
        if !frac.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        // scale the fraction to milliseconds: "5" -> 500? ASS uses centiseconds, ".5" is 0.5s.
        let mut f = frac.to_string();
        f.truncate(3);
        while f.len() < 3 {
            f.push('0');
        }
        f.parse::<i64>().ok()?
    };
    Some(secs * 1000 + frac_ms)
}

/// Frame <-> ms conversion for a constant frame rate.
#[derive(Clone, Copy, Debug)]
pub struct Fps(pub f64);

impl Fps {
    pub fn frame_from_ms(&self, ms: i64) -> i64 {
        ((ms as f64) * self.0 / 1000.0).floor() as i64
    }
    /// Aegisub style: time in the middle of the frame's display window.
    pub fn ms_from_frame(&self, frame: i64) -> i64 {
        ((frame as f64) * 1000.0 / self.0).round() as i64
    }
    pub fn snap_start(&self, ms: i64) -> i64 {
        // start time of a line: first ms in which frame is displayed
        let f = (ms as f64 * self.0 / 1000.0).ceil() as i64;
        ((f as f64) * 1000.0 / self.0).ceil() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(format_ass(3_723_450), "1:02:03.45");
        assert_eq!(parse_ass("1:02:03.45"), Some(3_723_450));
        assert_eq!(parse_srt("00:00:01,500"), Some(1500));
        assert_eq!(format_srt(3_723_456), "01:02:03,456");
        assert_eq!(parse_ass("0:00:00.5"), Some(500));
    }
}
