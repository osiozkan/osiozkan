//! Zaman ayrıştırma/biçimlendirme (milisaniye tabanlı).

/// ASS zamanı `H:MM:SS.cc` (santisaniye, yuvarlanarak).
pub fn format_ass(ms: i64) -> String {
    let ms = ms.max(0);
    let cs_total = (ms + 5) / 10;
    let cs = cs_total % 100;
    let s = (cs_total / 100) % 60;
    let m = (cs_total / 6000) % 60;
    let h = cs_total / 360000;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// ASS zamanı ayrıştır. `H:MM:SS.cc` (cc 1-3 hane toleranslı).
pub fn parse_ass(s: &str) -> Option<i64> {
    parse_generic(s)
}

pub fn format_srt(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

pub fn format_vtt(ms: i64) -> String {
    format_srt(ms).replace(',', ".")
}

/// `HH:MM:SS,mmm` / `HH:MM:SS.mmm` / `MM:SS.mmm` / `H:MM:SS.cc` biçimlerini kabul eder.
pub fn parse_generic(s: &str) -> Option<i64> {
    let s = s.trim();
    let (hms, frac) = match s.find(|c| c == '.' || c == ',') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    let nums: Option<Vec<i64>> = parts.iter().map(|p| p.trim().parse::<i64>().ok()).collect();
    let nums = nums?;
    let (h, m, sec) = match nums.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0, *m, *s),
        _ => return None,
    };
    if !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut f = frac.to_string();
    f.truncate(3);
    while f.len() < 3 && !frac.is_empty() {
        f.push('0');
    }
    let ms = if frac.is_empty() { 0 } else { f.parse::<i64>().ok()? };
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Kare numarasından ms (fps ile). Aegisub'ın yaklaşımı: karenin orta noktasına yakın zaman.
pub fn ms_from_frame(frame: i64, fps: f64) -> i64 {
    (frame as f64 * 1000.0 / fps).round() as i64
}

pub fn frame_from_ms(ms: i64, fps: f64) -> i64 {
    (ms as f64 * fps / 1000.0).floor() as i64
}

/// Zamanı en yakın kare sınırına oturtur.
pub fn snap_to_frame(ms: i64, fps: f64) -> i64 {
    ms_from_frame((ms as f64 * fps / 1000.0).round() as i64, fps)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass("0:01:02.50"), Some(62_500));
        assert_eq!(format_ass(62_500), "0:01:02.50");
        assert_eq!(parse_generic("00:00:01,5"), Some(1500));
        assert_eq!(format_srt(3_723_004), "01:02:03,004");
        assert_eq!(format_ass(999), "0:00:01.00");
    }
}
