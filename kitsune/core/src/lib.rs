//! Kitsune core: subtitle model, formats, tag parsing, quality tools.
pub mod ass;
pub mod model;
pub mod srt;
pub mod tags;
pub mod time;
pub mod tools;

pub use model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format { Ass, Srt, Vtt }

impl Format {
    pub fn from_path(p: &str) -> Option<Format> {
        let l = p.to_ascii_lowercase();
        if l.ends_with(".ass") || l.ends_with(".ssa") { Some(Format::Ass) }
        else if l.ends_with(".srt") { Some(Format::Srt) }
        else if l.ends_with(".vtt") { Some(Format::Vtt) }
        else { None }
    }
}

pub fn load(path: &str) -> Result<Script, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = decode(&bytes);
    Ok(match Format::from_path(path) {
        Some(Format::Srt) => srt::parse_srt(&text),
        Some(Format::Vtt) => srt::parse_vtt(&text),
        _ => ass::parse(&text),
    })
}

pub fn save(script: &Script, path: &str) -> Result<(), String> {
    let s = match Format::from_path(path) {
        Some(Format::Srt) => srt::write_srt(script),
        Some(Format::Vtt) => srt::write_vtt(script),
        _ => ass::write(script),
    };
    std::fs::write(path, s).map_err(|e| e.to_string())
}

/// Decode UTF-8 / UTF-16 (BOM) text; falls back to lossy UTF-8.
pub fn decode(b: &[u8]) -> String {
    if b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) {
        let le = b[0] == 0xFF;
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&u);
    }
    String::from_utf8_lossy(b).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_ass() {
        let src = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, {\\i1}world{\\i0}\\Nline2\n";
        let s = ass::parse(src);
        assert_eq!(s.events().count(), 1);
        assert_eq!(s.events().next().unwrap().text, "Hello, {\\i1}world{\\i0}\\Nline2");
        let out = ass::write(&s);
        let s2 = ass::parse(&out);
        assert_eq!(s.entries, s2.entries);
        assert_eq!(s.play_res(), (1280, 720));
    }
    #[test]
    fn srt_conv() {
        let s = srt::parse_srt("1\n00:00:01,000 --> 00:00:02,500\n<i>Hi</i>\nthere\n\n");
        let e = s.events().next().unwrap();
        assert_eq!((e.start, e.end), (1000, 2500));
        assert_eq!(srt::write_srt(&s).trim(), "1\n00:00:01,000 --> 00:00:02,500\n<i>Hi</i>\nthere");
    }
    #[test]
    fn times() {
        assert_eq!(time::ass_time(3_723_450), "1:02:03.45");
        assert_eq!(time::parse_ass_time("1:02:03.45"), Some(3_723_450));
    }
}
