//! Kare hızı: sabit (CFR) ya da değişken (VFR, timecode dosyası). Aegisub'ın `agi::vfr` anlambilimi:
//! bir kare, zaman damgasından bir sonraki karenin damgasına kadar ekrandadır; dosyanın sonundan
//! sonrası son bölümün ortalama hızıyla uzatılır.

#[derive(Clone, Debug, PartialEq)]
pub enum Framerate {
    Cfr(f64),
    /// Her karenin başlangıç zamanı (ms, artan). `tail_fps`: son kareden sonrası için hız.
    Vfr { ts: Vec<i64>, tail_fps: f64 },
}

impl Framerate {
    pub fn cfr(fps: f64) -> Framerate {
        Framerate::Cfr(fps)
    }

    pub fn is_vfr(&self) -> bool {
        matches!(self, Framerate::Vfr { .. })
    }

    /// Ortalama kare hızı (VFR'de tüm dosya üzerinden).
    pub fn fps(&self) -> f64 {
        match self {
            Framerate::Cfr(f) => *f,
            Framerate::Vfr { ts, tail_fps } => {
                if ts.len() >= 2 && ts[ts.len() - 1] > ts[0] {
                    (ts.len() - 1) as f64 * 1000.0 / (ts[ts.len() - 1] - ts[0]) as f64
                } else {
                    *tail_fps
                }
            }
        }
    }

    /// Karenin başlangıç zamanı (ms).
    pub fn time_at(&self, frame: i64) -> i64 {
        match self {
            Framerate::Cfr(f) => crate::time::ms_from_frame(frame, *f),
            Framerate::Vfr { ts, tail_fps } => {
                if frame < 0 {
                    return ts[0] + (frame as f64 * 1000.0 / tail_fps).round() as i64;
                }
                let n = ts.len() as i64;
                if frame < n {
                    ts[frame as usize]
                } else {
                    ts[ts.len() - 1] + ((frame - n + 1) as f64 * 1000.0 / tail_fps).round() as i64
                }
            }
        }
    }

    /// `ms` anında ekranda olan kare.
    pub fn frame_at(&self, ms: i64) -> i64 {
        match self {
            Framerate::Cfr(f) => {
                // time_at'in tam tersi: time_at(n) <= ms olan en büyük n (time_at ms'ye yuvarlar;
                // düz floor(ms*fps/1000) 23.976/24'te bazı karelerde bir önceki kareyi verir).
                let mut n = ((ms as f64 + 0.5) * f / 1000.0).ceil() as i64 - 1;
                while self.time_at(n + 1) <= ms {
                    n += 1;
                }
                while self.time_at(n) > ms {
                    n -= 1;
                }
                n
            }
            Framerate::Vfr { ts, tail_fps } => {
                if ms < ts[0] {
                    return ((ms - ts[0]) as f64 * tail_fps / 1000.0).floor() as i64;
                }
                let last = ts[ts.len() - 1];
                if ms >= last {
                    return ts.len() as i64 - 1 + ((ms - last) as f64 * tail_fps / 1000.0).floor() as i64;
                }
                // ts[i] <= ms olan en büyük i
                (ts.partition_point(|&t| t <= ms) as i64) - 1
            }
        }
    }

    /// Aegisub `TimeAtFrame(n, START)`: kare n'de görünmeye başlayan bir satırın başlangıç zamanı.
    /// Kare n-1 ile n'nin ortasıdır; ASS'in santisaniye yuvarlamasından sonra da doğru karede kalır
    /// (tam kare zamanı yuvarlanınca bir sonraki kareye kayabilir: 23.976'da 125.1 ms → 0.13).
    pub fn time_at_start(&self, n: i64) -> i64 {
        let (prev, cur) = (self.time_at(n - 1), self.time_at(n));
        (prev + (cur - prev + 1).div_euclid(2)).max(0)
    }

    /// Aegisub `TimeAtFrame(n, END)`: son görünen karesi n olan satırın bitiş zamanı.
    pub fn time_at_end(&self, n: i64) -> i64 {
        self.time_at_start(n + 1)
    }

    /// Aegisub `FrameAtTime(ms, START)`: `ms`'de başlayan satırın ilk görünen karesi.
    pub fn frame_at_start(&self, ms: i64) -> i64 {
        self.frame_at(ms - 1) + 1
    }

    /// Aegisub `FrameAtTime(ms, END)`: `ms`'de biten satırın son görünen karesi.
    pub fn frame_at_end(&self, ms: i64) -> i64 {
        self.frame_at(ms - 1)
    }

    /// En yakın kare sınırına (ASS-güvenli START zamanı) oturtur; sürüklerken kullanılır.
    pub fn snap(&self, ms: i64) -> i64 {
        let f = self.frame_at(ms);
        let (a, b) = (self.time_at_start(f), self.time_at_start(f + 1));
        if (ms - a).abs() <= (b - ms).abs() { a } else { b }
    }

    /// Başlangıç zamanını, satırın şu an ilk göründüğü karenin ASS-güvenli zamanına çeker.
    pub fn snap_start(&self, ms: i64) -> i64 {
        self.time_at_start(self.frame_at_start(ms))
    }

    /// Bitiş zamanını, satırın şu an son göründüğü karenin ASS-güvenli bitişine çeker.
    pub fn snap_end(&self, ms: i64) -> i64 {
        self.time_at_end(self.frame_at_end(ms))
    }

    /// Videodan okunan kare zaman damgalarından (ms) kare hızı. Damgalar (ms yuvarlamasıyla)
    /// tekdüzeyse `None` (CFR; sabit hız yeterli), değilse VFR döndürür.
    pub fn from_timestamps(mut ts: Vec<i64>) -> Option<Framerate> {
        ts.sort_unstable();
        ts.dedup();
        if ts.len() < 3 {
            return None;
        }
        let mean = (ts[ts.len() - 1] - ts[0]) as f64 / (ts.len() - 1) as f64;
        if mean <= 0.0 {
            return None;
        }
        // Matroska ms yuvarlaması: 23.976 fps → 41/42 ms. ±1.5 ms içi sabit sayılır.
        if ts.windows(2).all(|w| ((w[1] - w[0]) as f64 - mean).abs() <= 1.5) {
            return None;
        }
        let last = (ts[ts.len() - 1] - ts[ts.len() - 2]).max(1);
        Some(Framerate::Vfr { ts, tail_fps: 1000.0 / last as f64 })
    }

    /// Anahtar kare zamanlarını (tam kare zamanı, ms) ASS-güvenli START zamanlarına çevirir;
    /// satırlar sahne değişiminde bir kare geç başlamasın/erken bitmesin ("sahne taşması").
    pub fn keyframe_boundaries(&self, kf_ms: &[i64]) -> Vec<i64> {
        let mut v: Vec<i64> = kf_ms.iter().map(|&k| self.time_at_start(self.frame_at(k))).collect();
        v.dedup();
        v
    }

    /// Aegisub/mkvmerge timecode dosyası (v1 ya da v2) okur.
    pub fn parse_timecodes(text: &str) -> Result<Framerate, String> {
        let mut lines = text.lines().map(|l| l.trim_start_matches('\u{feff}').trim()).filter(|l| !l.is_empty());
        let head = lines.next().ok_or("boş timecode dosyası")?.to_ascii_lowercase();
        // v4 (mkvextract): v2 ile aynı, ama satırlar depolama sırasında olabilir → sıralanır
        let v4 = head.starts_with("# timestamp format v4") || head.starts_with("# timecode format v4");
        if head.starts_with("# timecode format v2") || head.starts_with("# timestamp format v2") || v4 {
            let mut ts = vec![];
            for l in lines {
                if l.starts_with('#') {
                    continue;
                }
                let v: f64 = l.parse().map_err(|_| format!("geçersiz zaman damgası: {}", l))?;
                ts.push(v.round() as i64);
            }
            if ts.is_empty() {
                return Err("timecode dosyasında kare yok".into());
            }
            if v4 {
                ts.sort_unstable();
            }
            if ts.windows(2).any(|w| w[1] < w[0]) {
                return Err("zaman damgaları artan sırada değil".into());
            }
            let tail_fps = if ts.len() >= 2 && ts[ts.len() - 1] > ts[ts.len() - 2] { 1000.0 / (ts[ts.len() - 1] - ts[ts.len() - 2]) as f64 } else { 23.976 };
            return Ok(Framerate::Vfr { ts, tail_fps });
        }
        if head.starts_with("# timecode format v1") {
            let mut assume: Option<f64> = None;
            let mut ranges: Vec<(i64, i64, f64)> = vec![];
            for l in lines {
                if l.starts_with('#') {
                    continue;
                }
                if let Some(v) = l.strip_prefix("Assume ").or_else(|| l.strip_prefix("assume ")) {
                    assume = Some(v.trim().parse().map_err(|_| format!("geçersiz Assume: {}", l))?);
                    continue;
                }
                let p: Vec<&str> = l.split(',').map(|x| x.trim()).collect();
                if p.len() != 3 {
                    return Err(format!("geçersiz satır: {}", l));
                }
                let a: i64 = p[0].parse().map_err(|_| format!("geçersiz satır: {}", l))?;
                let b: i64 = p[1].parse().map_err(|_| format!("geçersiz satır: {}", l))?;
                let f: f64 = p[2].parse().map_err(|_| format!("geçersiz satır: {}", l))?;
                if b < a || f <= 0.0 || a < 0 {
                    return Err(format!("geçersiz aralık: {}", l));
                }
                ranges.push((a, b, f));
            }
            let assume = assume.ok_or("v1 dosyasında Assume satırı yok")?;
            if assume <= 0.0 {
                return Err("Assume değeri pozitif olmalı".into());
            }
            ranges.sort_by_key(|r| r.0);
            let end = ranges.iter().map(|r| r.1).max().unwrap_or(0);
            // bozuk/kötü niyetli dosya milyarlarca kare ayırtmasın (10 M kare ≈ 24 fps'de 4.8 gün)
            if end > 10_000_000 {
                return Err(format!("aralık çok büyük: {} kare", end));
            }
            let mut ts = Vec::with_capacity(end as usize + 2);
            let mut t = 0.0f64;
            let mut ri = 0;
            for fr in 0..=end + 1 {
                ts.push(t.round() as i64);
                while ri < ranges.len() && ranges[ri].1 < fr {
                    ri += 1;
                }
                let fps = match ranges.get(ri) {
                    Some(r) if r.0 <= fr && fr <= r.1 => r.2,
                    _ => assume,
                };
                t += 1000.0 / fps;
            }
            return Ok(Framerate::Vfr { ts, tail_fps: assume });
        }
        Err("tanınmayan timecode biçimi (v1/v2 bekleniyor)".into())
    }

    /// v2 biçiminde `frames` kare için zaman damgaları yazar.
    pub fn to_v2(&self, frames: usize) -> String {
        let mut s = String::from("# timecode format v2\n");
        for f in 0..frames as i64 {
            s.push_str(&self.time_at(f).to_string());
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_lookup() {
        let fr = Framerate::parse_timecodes("# timecode format v2\n0\n40\n80\n100\n120\n").unwrap();
        assert!(fr.is_vfr());
        assert_eq!(fr.time_at(3), 100);
        assert_eq!(fr.frame_at(0), 0);
        assert_eq!(fr.frame_at(79), 1);
        assert_eq!(fr.frame_at(80), 2);
        assert_eq!(fr.frame_at(110), 3);
        // sondan sonrası son aralıkla (20 ms) uzatılır
        assert_eq!(fr.time_at(5), 140);
        assert_eq!(fr.frame_at(145), 5);
        // sınırlar: 70 (80'in START'ı = 40+(80-40+1)/2), 90 (100'ün START'ı)
        assert_eq!(fr.time_at_start(2), 60);
        assert_eq!(fr.time_at_start(3), 90);
        assert_eq!(fr.snap(88), 90);
        assert_eq!(fr.snap(70), 60);
        for f in -3..20 {
            assert_eq!(fr.frame_at(fr.time_at(f)), f, "kare {}", f);
        }
    }

    #[test]
    fn v4_unsorted() {
        let fr = Framerate::parse_timecodes("# timestamp format v4\n0\n83\n42\n125\n").unwrap();
        assert_eq!((fr.time_at(1), fr.time_at(2), fr.time_at(3)), (42, 83, 125));
        assert!(Framerate::parse_timecodes("# timecode format v2\n0\n83\n42\n").is_err());
    }

    #[test]
    fn v1_ranges() {
        // 0-1: 25 fps (40 ms), 2-3: 50 fps (20 ms), sonrası Assume 10 fps (100 ms)
        let fr = Framerate::parse_timecodes("# timecode format v1\nAssume 10\n0,1,25\n2,3,50\n").unwrap();
        assert_eq!((0..6).map(|f| fr.time_at(f)).collect::<Vec<_>>(), vec![0, 40, 80, 100, 120, 220]);
    }

    #[test]
    fn v2_roundtrip_and_errors() {
        let fr = Framerate::parse_timecodes("# timecode format v2\n0\n42\n83\n").unwrap();
        assert_eq!(fr.to_v2(3), "# timecode format v2\n0\n42\n83\n");
        assert!(Framerate::parse_timecodes("# timecode format v2\n10\n5\n").is_err());
        assert!(Framerate::parse_timecodes("merhaba").is_err());
        assert!(Framerate::parse_timecodes("# timecode format v1\n0,1,25\n").is_err());
        assert!(Framerate::parse_timecodes("# timecode format v1\nAssume 24\n0,1000000000,30\n").is_err());
    }

    #[test]
    fn detect_vfr_from_timestamps() {
        // 23.976 CFR, ms yuvarlamalı → CFR sayılır
        let cfr: Vec<i64> = (0..200).map(|i| (i as f64 * 1001.0 / 24.0).round() as i64).collect();
        assert_eq!(Framerate::from_timestamps(cfr), None);
        // 24 fps + 60 fps bölümleri → VFR
        let mut v: Vec<i64> = (0..48).map(|i| (i as f64 * 1000.0 / 24.0).round() as i64).collect();
        v.extend((0..60).map(|i| 2000 + (i as f64 * 1000.0 / 60.0).round() as i64));
        let fr = Framerate::from_timestamps(v).unwrap();
        assert!(fr.is_vfr());
        assert_eq!(fr.time_at(48), 2000);
        assert_eq!(fr.frame_at(2020), 49);
    }

    /// ASS santisaniye yuvarlamasından sonra START/END zamanları doğru karede kalmalı.
    #[test]
    fn start_end_survive_centisecond_rounding() {
        let cs = |ms: i64| crate::time::parse_ass(&crate::time::format_ass(ms)).unwrap();
        let mut v: Vec<i64> = (0..300).map(|i| (i as f64 * 1000.0 / 24.0).round() as i64).collect();
        v.extend((0..300).map(|i| 12_500 + (i as f64 * 1000.0 / 60.0).round() as i64));
        for fr in [Framerate::cfr(24000.0 / 1001.0), Framerate::cfr(30000.0 / 1001.0), Framerate::cfr(25.0), Framerate::from_timestamps(v).unwrap()] {
            for n in 0..2000 {
                assert_eq!(fr.frame_at_start(cs(fr.time_at_start(n))), n, "{:?} START {}", fr.fps(), n);
                assert_eq!(fr.frame_at_end(cs(fr.time_at_end(n))), n, "{:?} END {}", fr.fps(), n);
            }
        }
        // eski yöntemin hatası: tam kare zamanı 23.976'da 125 ms → "0:00:00.13" → kare 4'te başlar
        let f = Framerate::cfr(24000.0 / 1001.0);
        assert_eq!(f.frame_at_start(cs(f.time_at(3))), 4);
        assert_eq!(f.frame_at_start(cs(f.snap_start(f.time_at(3)))), 3);
    }

    #[test]
    fn cfr_roundtrip() {
        for fps in [23.976, 24000.0 / 1001.0, 24.0, 25.0, 29.97, 30000.0 / 1001.0, 60.0] {
            let fr = Framerate::cfr(fps);
            for n in -5..5000 {
                assert_eq!(fr.frame_at(fr.time_at(n)), n, "fps {} kare {}", fps, n);
                assert_eq!(fr.frame_at(fr.time_at(n + 1) - 1), n, "fps {} kare {} sonu", fps, n);
            }
        }
        // 24 fps'de 47. kare 1958 ms'de başlar (1958.33 yuvarlanmış)
        assert_eq!(Framerate::cfr(24.0).frame_at(1958), 47);
    }

    #[test]
    fn cfr_matches_time_helpers() {
        let fr = Framerate::cfr(23.976);
        assert_eq!(fr.time_at(24), crate::time::ms_from_frame(24, 23.976));
        assert_eq!(fr.frame_at(1001), 24);
        assert_eq!(fr.frame_at(1001), crate::time::frame_from_ms(1001, 23.976));
    }
}
