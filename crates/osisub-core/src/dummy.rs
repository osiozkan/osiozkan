//! Aegisub sahte videosu: `?dummy:kare-hızı:kare-sayısı:genişlik:yükseklik:r:g:b:dama`.
//!
//! Videosuz zamanlama/tipografi için Aegisub projelerinde `Video File` olarak saklanır.
//! osisub bunu ffmpeg ile önbelleğe üretilen düz renkli (isteğe bağlı damalı) bir videoya çevirir.

#[derive(Clone, Debug, PartialEq)]
pub struct DummyVideo {
    pub fps: f64,
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    pub rgb: [u8; 3],
    pub checkerboard: bool,
}

impl Default for DummyVideo {
    /// Aegisub'ın varsayılanı: 23.976 kare/sn, 30 dakika, 640×480, koyu mavi, damalı.
    fn default() -> Self {
        DummyVideo { fps: 23.976, frames: 43_157, width: 640, height: 480, rgb: [47, 163, 254], checkerboard: true }
    }
}

impl DummyVideo {
    pub fn parse(s: &str) -> Option<DummyVideo> {
        let rest = s.trim().strip_prefix("?dummy:")?;
        let p: Vec<&str> = rest.split(':').collect();
        if p.len() < 7 {
            return None;
        }
        let fps: f64 = p[0].parse().ok().filter(|f: &f64| *f > 0.0 && *f <= 1000.0)?;
        let frames: u64 = p[1].parse().ok().filter(|f| *f > 0)?;
        let width: u32 = p[2].parse().ok().filter(|w| (16..=8192).contains(w))?;
        let height: u32 = p[3].parse().ok().filter(|h| (16..=8192).contains(h))?;
        let c = |i: usize| p[i].parse::<u8>().ok();
        let rgb = [c(4)?, c(5)?, c(6)?];
        // 8. alan: "c" damalı, boş = düz (Aegisub "...:254:c")
        let checkerboard = p.get(7).map_or(false, |v| v.starts_with('c'));
        Some(DummyVideo { fps, frames, width, height, rgb, checkerboard })
    }

    pub fn to_aegisub(&self) -> String {
        format!(
            "?dummy:{:.6}:{}:{}:{}:{}:{}:{}:{}",
            self.fps,
            self.frames,
            self.width,
            self.height,
            self.rgb[0],
            self.rgb[1],
            self.rgb[2],
            if self.checkerboard { "c" } else { "" }
        )
    }

    pub fn duration_ms(&self) -> i64 {
        (self.frames as f64 * 1000.0 / self.fps).round() as i64
    }

    /// ffmpeg için kesirli kare hızı: 23.976 → 24000/1001 (NTSC hızları tam kesir olsun).
    pub fn fps_rational(&self) -> String {
        for base in [24.0, 30.0, 60.0, 48.0, 120.0] {
            let ntsc: f64 = base * 1000.0 / 1001.0;
            if (self.fps - ntsc).abs() < 0.002 {
                return format!("{}/1001", base as u32 * 1000);
            }
        }
        if (self.fps - self.fps.round()).abs() < 1e-6 {
            format!("{}", self.fps.round() as u32)
        } else {
            format!("{}/1000", (self.fps * 1000.0).round() as u64)
        }
    }

    /// ffmpeg lavfi kaynağı (düz renk; damalıysa 8 piksellik açık/koyu kareler — desen 1/8
    /// çözünürlükte üretilip en yakın komşuyla büyütülür, her karede tam boy `geq` çok yavaş).
    pub fn lavfi_source(&self) -> String {
        let color = format!("0x{:02X}{:02X}{:02X}", self.rgb[0], self.rgb[1], self.rgb[2]);
        let (r, d) = (self.fps_rational(), self.duration_ms() as f64 / 1000.0);
        if self.checkerboard {
            format!(
                "color=c={}:s={}x{}:r={}:d={:.3},geq=lum='if(mod(X+Y\\,2)\\,lum(X\\,Y)\\,lum(X\\,Y)*0.75)':cb='cb(X\\,Y)':cr='cr(X\\,Y)',scale={}:{}:flags=neighbor",
                color,
                self.width.div_ceil(8),
                self.height.div_ceil(8),
                r,
                d,
                self.width,
                self.height
            )
        } else {
            format!("color=c={}:s={}x{}:r={}:d={:.3}", color, self.width, self.height, r, d)
        }
    }

    /// Önbellek dosya adı (parametrelerden türetilir).
    pub fn cache_name(&self) -> String {
        format!(
            "dummy-{}-{}-{}x{}-{:02x}{:02x}{:02x}{}.mkv",
            self.fps_rational().replace('/', "_"),
            self.frames,
            self.width,
            self.height,
            self.rgb[0],
            self.rgb[1],
            self.rgb[2],
            if self.checkerboard { "-c" } else { "" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aegisub_strings() {
        let d = DummyVideo::parse("?dummy:23.976000:40000:640:480:47:163:254:").unwrap();
        assert_eq!((d.frames, d.width, d.height, d.rgb, d.checkerboard), (40000, 640, 480, [47, 163, 254], false));
        assert_eq!(d.fps_rational(), "24000/1001");
        assert_eq!(d.duration_ms(), 1_668_335);
        let c = DummyVideo::parse("?dummy:25:100:1920:1080:0:0:0:c").unwrap();
        assert!(c.checkerboard);
        assert_eq!(c.fps_rational(), "25");
        assert_eq!(DummyVideo::parse(&c.to_aegisub()), Some(c.clone()));
        assert!(c.cache_name().ends_with("-c.mkv"));
        assert_eq!(d.lavfi_source(), "color=c=0x2FA3FE:s=640x480:r=24000/1001:d=1668.335");
        assert!(c.lavfi_source().contains(",geq="));
        assert!(DummyVideo::parse("?dummy:0:1:640:480:0:0:0").is_none());
        assert!(DummyVideo::parse("video.mkv").is_none());
        assert!(DummyVideo::parse("?dummy:23.976:10:640:480:300:0:0").is_none());
    }
}
