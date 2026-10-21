//! Time helpers. All times are integer milliseconds.

/// Parse `H:MM:SS.cc` (ASS), also tolerating `.mmm` and `,mmm`.
pub fn parse_ass_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let (hms, frac) = match s.find(['.', ',']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let mut it = hms.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let sec: i64 = it.next()?.trim().parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    let ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 100,
        2 => frac.parse::<i64>().ok()? * 10,
        3 => frac.parse::<i64>().ok()?,
        _ => frac[..3].parse::<i64>().ok()?,
    };
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Format as ASS time, rounding to centiseconds (as Aegisub does).
pub fn fmt_ass_time(ms: i64) -> String {
    let ms = ms.max(0);
    let cs_total = (ms + 5) / 10;
    let cs = cs_total % 100;
    let s_total = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s_total / 3600, (s_total / 60) % 60, s_total % 60, cs)
}

/// Format as SRT time `HH:MM:SS,mmm` (or `.` for VTT).
pub fn fmt_srt_time(ms: i64, sep: char) -> String {
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

/// Parse SRT/VTT times (`HH:MM:SS,mmm`, `MM:SS.mmm`).
pub fn parse_srt_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, rest) = match parts.len() {
        3 => (parts[0].parse::<i64>().ok()?, parts[1].parse::<i64>().ok()?, parts[2]),
        2 => (0, parts[0].parse::<i64>().ok()?, parts[1]),
        _ => return None,
    };
    let (sec, ms) = match rest.find(['.', ',']) {
        Some(i) => {
            let f = &rest[i + 1..];
            let mut v: i64 = f.get(..f.len().min(3))?.parse().ok()?;
            for _ in f.len().min(3)..3 {
                v *= 10;
            }
            (rest[..i].parse::<i64>().ok()?, v)
        }
        None => (rest.parse::<i64>().ok()?, 0),
    };
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Frame number -> start ms of that frame (Aegisub "start" semantics use ceil-ish; we use round).
pub fn ms_from_frame(frame: i64, fps: f64) -> i64 {
    ((frame as f64) * 1000.0 / fps).round() as i64
}

pub fn frame_from_ms(ms: i64, fps: f64) -> i64 {
    ((ms as f64) * fps / 1000.0).floor() as i64
}

/// Format a number like ASS does: no trailing zeros, no exponent.
pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass_time("0:01:02.34"), Some(62_340));
        assert_eq!(fmt_ass_time(62_340), "0:01:02.34");
        assert_eq!(fmt_ass_time(62_345), "0:01:02.35");
        assert_eq!(parse_srt_time("00:01:02,345"), Some(62_345));
        assert_eq!(fmt_srt_time(62_345, ','), "00:01:02,345");
        assert_eq!(parse_srt_time("01:02.5"), Some(62_500));
    }
    #[test]
    fn nums() {
        assert_eq!(fmt_num(20.0), "20");
        assert_eq!(fmt_num(2.5), "2.5");
        assert_eq!(fmt_num(0.1 + 0.2), "0.3");
    }
}
