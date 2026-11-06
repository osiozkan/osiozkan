//! Zaman yardımcıları. ASS santisaniye (h:mm:ss.cc), SRT milisaniye kullanır.

/// ASS zamanı "H:MM:SS.cc" -> ms. Hatalıysa None.
pub fn parse_ass(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let mut parts = s.split(':');
    let h: i64 = parts.next()?.trim().parse().ok()?;
    let m: i64 = parts.next()?.trim().parse().ok()?;
    let sec = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let (ss, frac) = match sec.split_once('.') {
        Some((a, b)) => (a, b),
        None => (sec, ""),
    };
    let ss: i64 = ss.trim().parse().ok()?;
    let cs: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 100,
        2 => frac.parse::<i64>().ok()? * 10,
        _ => frac[..3].parse::<i64>().ok()?,
    };
    let ms = ((h * 60 + m) * 60 + ss) * 1000 + cs;
    Some(if neg { -ms } else { ms })
}

/// ms -> "H:MM:SS.cc" (santisaniyeye yuvarlanır, negatifler 0'a kenetlenir).
pub fn format_ass(ms: i64) -> String {
    let cs_total = (ms.max(0) + 5) / 10;
    let cs = cs_total % 100;
    let s = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s / 3600, (s / 60) % 60, s % 60, cs)
}

/// SRT "HH:MM:SS,mmm" (nokta da kabul) -> ms.
pub fn parse_srt(s: &str) -> Option<i64> {
    let s = s.trim().replace(',', ".");
    let mut it = s.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let sec = it.next()?;
    let (ss, f) = sec.split_once('.').unwrap_or((sec, "0"));
    let ss: i64 = ss.trim().parse().ok()?;
    let mut f = f.trim().to_string();
    f.truncate(3);
    while f.len() < 3 {
        f.push('0');
    }
    let f: i64 = f.parse().ok()?;
    Some(((h * 60 + m) * 60 + ss) * 1000 + f)
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

/// Kare <-> ms (sabit fps). Aegisub'ın kare zamanlaması gibi karenin ortasına yakın değer verir.
pub fn ms_from_frame(frame: i64, fps: f64) -> i64 {
    (frame as f64 * 1000.0 / fps).round() as i64
}
pub fn frame_from_ms(ms: i64, fps: f64) -> i64 {
    (ms as f64 * fps / 1000.0).floor() as i64
}
/// Başlangıç için kare-başı, bitiş için bir önceki karenin sonu: Aegisub davranışı.
pub fn snap_start_to_frame(ms: i64, fps: f64) -> i64 {
    let f = (ms as f64 * fps / 1000.0).ceil() as i64;
    // ASS cs hassasiyeti: karenin ortasına
    let a = ms_from_frame(f - 1, fps);
    let b = ms_from_frame(f, fps);
    ((a + b) / 2 + 5) / 10 * 10
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass("0:01:02.50"), Some(62_500));
        assert_eq!(format_ass(62_500), "0:01:02.50");
        assert_eq!(parse_srt("00:01:02,500"), Some(62_500));
        assert_eq!(format_srt(3_723_004), "01:02:03,004");
        assert_eq!(parse_ass("junk"), None);
    }
}
