//! Time helpers (milliseconds based, like Aegisub).

/// Format ms as ASS time `H:MM:SS.cc` (centisecond rounding like Aegisub).
pub fn ass_time(ms: i64) -> String {
    let ms = ms.max(0);
    let cs = (ms + 5) / 10;
    format!("{}:{:02}:{:02}.{:02}", cs / 360000, cs / 6000 % 60, cs / 100 % 60, cs % 100)
}

pub fn parse_ass_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let mut it = s.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let rest = it.next()?;
    if it.next().is_some() { return None; }
    let (sec, frac) = match rest.split_once(['.', ',']) {
        Some((a, b)) => (a, b),
        None => (rest, "0"),
    };
    let sec: i64 = sec.trim().parse().ok()?;
    let mut f = frac.trim().to_string();
    f.truncate(3);
    let fv: i64 = if f.is_empty() { 0 } else { f.parse().ok()? };
    let ms_frac = match f.len() { 0 => 0, 1 => fv * 100, 2 => fv * 10, _ => fv };
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms_frac)
}

/// `HH:MM:SS,mmm` (SRT) or with '.' (VTT).
pub fn fmt_hms_ms(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, sep, ms % 1000)
}

/// Parses `HH:MM:SS,mmm`, `MM:SS.mmm`, etc.
pub fn parse_hms_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, rest) = match parts.len() {
        3 => (parts[0].trim().parse::<i64>().ok()?, parts[1].trim().parse::<i64>().ok()?, parts[2]),
        2 => (0, parts[0].trim().parse::<i64>().ok()?, parts[1]),
        _ => return None,
    };
    let (sec, frac) = rest.split_once([',', '.']).unwrap_or((rest, "0"));
    let sec: i64 = sec.trim().parse().ok()?;
    let mut f = frac.trim().to_string();
    f.truncate(3);
    while f.len() < 3 { f.push('0'); }
    let fv: i64 = f.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + fv)
}

/// Frame <-> ms conversion with fixed fps (Aegisub-compatible rounding: frame start time).
#[derive(Clone, Copy, Debug)]
pub struct Fps(pub f64);

impl Fps {
    pub fn frame_from_ms(&self, ms: i64) -> i64 { (ms as f64 * self.0 / 1000.0).floor() as i64 }
    pub fn ms_from_frame(&self, f: i64) -> i64 { (f as f64 * 1000.0 / self.0).round() as i64 }
}
