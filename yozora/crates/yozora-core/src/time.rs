//! Zaman dönüşümleri (milisaniye tabanlı).

/// ASS zamanı `H:MM:SS.cc` -> ms. Hatalıysa `None`.
pub fn parse_ass(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let mut it = s.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let sec = it.next()?.trim();
    if it.next().is_some() {
        return None;
    }
    let (sp, fp) = match sec.split_once(['.', ',']) {
        Some((a, b)) => (a, b),
        None => (sec, ""),
    };
    let sv: i64 = sp.parse().ok()?;
    let frac: i64 = if fp.is_empty() {
        0
    } else {
        // Kesir basamaklarını ms'ye ölçekle (1..3 hane).
        let digits: String = fp.chars().take(3).collect();
        let v: i64 = digits.parse().ok()?;
        v * 10_i64.pow(3 - digits.len() as u32)
    };
    let ms = ((h * 60 + m) * 60 + sv) * 1000 + frac;
    Some(if neg { -ms } else { ms })
}

/// ms -> `H:MM:SS.cc` (en yakın santisaniyeye yuvarlar, negatif değerler 0'a sabitlenir).
pub fn format_ass(ms: i64) -> String {
    let cs_total = ((ms.max(0) + 5) / 10) as i64;
    let cs = cs_total % 100;
    let s_total = cs_total / 100;
    format!("{}:{:02}:{:02}.{:02}", s_total / 3600, (s_total / 60) % 60, s_total % 60, cs)
}

/// SRT zamanı `HH:MM:SS,mmm` (nokta da kabul edilir).
pub fn parse_srt(s: &str) -> Option<i64> {
    parse_ass(s)
}

pub fn format_srt(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn format_vtt(ms: i64) -> String {
    format_srt(ms).replace(',', ".")
}

/// Kare numarası -> ms (karenin başlangıcı; santisaniyeye yukarı yuvarlanır, Aegisub davranışı).
pub fn ms_from_frame(frame: i64, fps: f64) -> i64 {
    ((frame as f64) * 1000.0 / fps).round() as i64
}

/// ms -> kare numarası (ms'nin içinde bulunduğu kare).
pub fn frame_from_ms(ms: i64, fps: f64) -> i64 {
    ((ms as f64) * fps / 1000.0 + 1e-6).floor() as i64
}

/// "1500", "1500ms", "1.5s", "0:00:01.50" biçimlerini ms'ye çevirir.
pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("ms") {
        return v.trim().parse::<f64>().ok().map(|x| x.round() as i64);
    }
    if let Some(v) = s.strip_suffix('s') {
        return v.trim().parse::<f64>().ok().map(|x| (x * 1000.0).round() as i64);
    }
    if s.contains(':') {
        return parse_ass(s);
    }
    s.parse::<f64>().ok().map(|x| x.round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        assert_eq!(parse_ass("0:01:02.50"), Some(62_500));
        assert_eq!(format_ass(62_500), "0:01:02.50");
        assert_eq!(format_ass(62_504), "0:01:02.50");
        assert_eq!(format_ass(62_505), "0:01:02.51");
        assert_eq!(parse_srt("00:00:01,234"), Some(1234));
        assert_eq!(format_srt(3_723_004), "01:02:03,004");
        assert_eq!(parse_duration("1.5s"), Some(1500));
        assert_eq!(parse_duration("-250ms"), Some(-250));
    }
    #[test]
    fn frames() {
        assert_eq!(frame_from_ms(ms_from_frame(100, 23.976), 23.976), 100);
    }
}
