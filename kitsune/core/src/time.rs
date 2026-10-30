//! Zaman biçimleri. Tüm dahili zamanlar milisaniyedir.

/// ASS biçimini (`h:mm:ss.cc`) ms'ye çevirir.
pub fn parse_ass(s: &str) -> Option<i64> {
    let s = s.trim();
    let (hms, frac) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s, "0"),
    };
    let mut it = hms.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let sec: i64 = it.next()?.trim().parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    let mut f = frac.trim().to_string();
    f.truncate(3);
    let ms: i64 = if f.is_empty() { 0 } else { f.parse::<i64>().ok()? * 10i64.pow(3 - f.len() as u32) };
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// ms → ASS (`h:mm:ss.cc`), Aegisub gibi yüzde saniyeye yuvarlar.
pub fn fmt_ass(ms: i64) -> String {
    let cs_total = (ms.max(0) + 5) / 10;
    let cs = cs_total % 100;
    let s = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s / 3600, (s / 60) % 60, s % 60, cs)
}

/// SRT (`HH:MM:SS,mmm`) → ms.
pub fn parse_srt(s: &str) -> Option<i64> {
    parse_ass(&s.trim().replace(',', "."))
}

pub fn fmt_srt(ms: i64) -> String {
    let ms = ms.max(0);
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02},{:03}", s / 3600, (s / 60) % 60, s % 60, ms % 1000)
}

/// Kare numarası → ms (Aegisub ile aynı: kare ortası değil, kare başlangıcı + yarım kare yuvarlama).
pub fn ms_from_frame(frame: i64, fps: f64) -> i64 {
    (frame as f64 * 1000.0 / fps).round() as i64
}

pub fn frame_from_ms(ms: i64, fps: f64) -> i64 {
    (ms as f64 * fps / 1000.0).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass("0:01:02.50"), Some(62_500));
        assert_eq!(fmt_ass(62_504), "0:01:02.50");
        assert_eq!(fmt_ass(62_505), "0:01:02.51");
        assert_eq!(parse_srt("01:02:03,004"), Some(3_723_004));
        assert_eq!(fmt_srt(3_723_004), "01:02:03,004");
    }
}
