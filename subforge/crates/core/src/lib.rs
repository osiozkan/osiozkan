//! SubForge core: ASS/SRT/VTT model, parsing, tags, timing and QC.
pub mod ass;
pub mod color;
pub mod encoding;
pub mod formats;
pub mod qc;
pub mod srt;
pub mod tags;
pub mod time;
pub mod tools;
pub mod timing;
pub mod turkish;

pub use ass::{Event, EventKind, Script, Style};
pub use formats::detect::detect_format;
pub use time::Ms;

/// Subtitle formats SubForge can read/write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ass,
    Srt,
    Vtt,
    /// MicroDVD `.sub` (frame based)
    MicroDvd,
    /// SubViewer 2.0 `.sub`
    SubViewer,
    /// YouTube `.sbv`
    Sbv,
    /// SAMI `.smi`
    Sami,
    Lrc,
    /// TTML / DFXP
    Ttml,
    /// Plain text, one event per line
    Txt,
}

impl Format {
    pub const ALL: [Format; 10] =
        [Format::Ass, Format::Srt, Format::Vtt, Format::MicroDvd, Format::SubViewer, Format::Sbv, Format::Sami, Format::Lrc, Format::Ttml, Format::Txt];

    /// By extension. `.sub` is MicroDVD here; [`load_path`] sorts out SubViewer by content.
    pub fn from_path(path: &str) -> Option<Format> {
        let p = path.to_ascii_lowercase();
        let ext = p.rsplit_once('.')?.1;
        Some(match ext {
            "ass" | "ssa" => Format::Ass,
            "srt" => Format::Srt,
            "vtt" => Format::Vtt,
            "sub" => Format::MicroDvd,
            "sbv" => Format::Sbv,
            "smi" | "sami" => Format::Sami,
            "lrc" => Format::Lrc,
            "ttml" | "dfxp" | "xml" | "tt" => Format::Ttml,
            "txt" => Format::Txt,
            _ => return None,
        })
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Ass => "ass",
            Format::Srt => "srt",
            Format::Vtt => "vtt",
            Format::MicroDvd | Format::SubViewer => "sub",
            Format::Sbv => "sbv",
            Format::Sami => "smi",
            Format::Lrc => "lrc",
            Format::Ttml => "ttml",
            Format::Txt => "txt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Ass => "Advanced SubStation Alpha",
            Format::Srt => "SubRip",
            Format::Vtt => "WebVTT",
            Format::MicroDvd => "MicroDVD",
            Format::SubViewer => "SubViewer 2.0",
            Format::Sbv => "YouTube SBV",
            Format::Sami => "SAMI",
            Format::Lrc => "LRC lyrics",
            Format::Ttml => "TTML/DFXP",
            Format::Txt => "Plain text",
        }
    }

    /// Whether files of this format are written with a UTF-8 BOM (VSFilter needs it for ASS).
    fn writes_bom(self) -> bool {
        matches!(self, Format::Ass | Format::Srt)
    }
}

/// Parameters that only some formats need when reading.
#[derive(Debug, Clone, Copy)]
pub struct LoadOptions {
    /// Frame rate for frame based formats (MicroDVD); a file header wins. 0 = 23.976.
    pub fps: f64,
    /// Event length for plain text import.
    pub txt_duration: Ms,
}

impl Default for LoadOptions {
    fn default() -> Self {
        LoadOptions { fps: 0.0, txt_duration: 3000 }
    }
}

/// Parameters that only some formats need when writing.
#[derive(Debug, Clone, Copy)]
pub struct WriteOptions {
    /// Frame rate for MicroDVD. 0 = 23.976.
    pub fps: f64,
    pub srt: srt::SrtOptions,
    pub vtt: srt::VttOptions,
    /// LRC: `<mm:ss.xx>` word stamps from karaoke tags.
    pub lrc_enhanced: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions { fps: 0.0, srt: srt::SrtOptions::default(), vtt: srt::VttOptions::default(), lrc_enhanced: false }
    }
}

/// Parse already decoded text in a known format.
pub fn parse_text(text: &str, format: Format, opts: &LoadOptions) -> Script {
    use formats::*;
    match format {
        Format::Ass => Script::parse(text),
        Format::Srt => srt::parse_srt(text),
        Format::Vtt => srt::parse_vtt(text),
        Format::MicroDvd => microdvd::parse_microdvd(text, opts.fps),
        Format::SubViewer => subviewer::parse_subviewer(text),
        Format::Sbv => sbv::parse_sbv(text),
        Format::Sami => sami::parse_sami(text),
        Format::Lrc => lrc::parse_lrc(text),
        Format::Ttml => ttml::parse_ttml(text),
        Format::Txt => txt::parse_txt(text, opts.txt_duration),
    }
}

/// Load any supported subtitle file from bytes (encoding auto-detected).
pub fn load_bytes(bytes: &[u8], format: Format) -> Result<(Script, encoding::Detected), String> {
    load_bytes_with(bytes, format, &LoadOptions::default())
}

pub fn load_bytes_with(bytes: &[u8], format: Format, opts: &LoadOptions) -> Result<(Script, encoding::Detected), String> {
    let (text, det) = encoding::decode(bytes);
    Ok((parse_text(&text, format, opts), det))
}

/// Load a file; the format comes from the content when it disagrees with the extension
/// (or the extension is missing/unknown), so a `.sub` can be MicroDVD or SubViewer.
pub fn load_path(path: &str) -> Result<(Script, encoding::Detected), String> {
    load_path_with(path, &LoadOptions::default())
}

pub fn load_path_with(path: &str, opts: &LoadOptions) -> Result<(Script, encoding::Detected), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let (text, det) = encoding::decode(&bytes);
    let fmt = detect_format(&text).or_else(|| Format::from_path(path)).unwrap_or(Format::Ass);
    Ok((parse_text(&text, fmt, opts), det))
}

/// Serialize in the given format.
pub fn serialize(script: &Script, format: Format) -> String {
    serialize_with(script, format, &WriteOptions::default())
}

pub fn serialize_with(script: &Script, format: Format, opts: &WriteOptions) -> String {
    use formats::*;
    match format {
        Format::Ass => script.to_ass(),
        Format::Srt => srt::write_srt_with(script, &opts.srt),
        Format::Vtt => srt::write_vtt_with(script, &opts.vtt),
        Format::MicroDvd => microdvd::write_microdvd(script, opts.fps),
        Format::SubViewer => subviewer::write_subviewer(script),
        Format::Sbv => sbv::write_sbv(script),
        Format::Sami => sami::write_sami(script),
        Format::Lrc => lrc::write_lrc(script, opts.lrc_enhanced),
        Format::Ttml => ttml::write_ttml(script),
        Format::Txt => txt::write_txt(script),
    }
}

/// Save to disk as UTF-8. ASS/SRT get a BOM (VSFilter needs it to detect UTF-8).
pub fn save_path(script: &Script, path: &str) -> Result<(), String> {
    save_path_with_encoding(script, path, "utf-8")
}

/// Save with a named encoding (`utf-8`, `utf-16le`, `windows-1254`, `iso-8859-9`, ...).
/// Fails instead of writing replacement characters when the text does not fit the encoding.
pub fn save_path_with_encoding(script: &Script, path: &str, encoding: &str) -> Result<(), String> {
    let fmt = Format::from_path(path).unwrap_or(Format::Ass);
    save_path_format(script, path, fmt, encoding, &WriteOptions::default())
}

/// Like [`save_path_with_encoding`] but with an explicit format and options (the extension is ignored).
pub fn save_path_format(script: &Script, path: &str, fmt: Format, encoding: &str, opts: &WriteOptions) -> Result<(), String> {
    let text = serialize_with(script, fmt, opts);
    let enc = encoding::canonical_name(encoding).ok_or_else(|| format!("unknown encoding: {encoding}"))?;
    let body = encoding::encode_strict(&text, enc).map_err(|bad| {
        let list: String = bad.iter().take(10).collect();
        format!("{path}: cannot represent {list:?} in {enc}")
    })?;
    let mut out = Vec::with_capacity(body.len() + 3);
    match enc {
        "UTF-8" if fmt.writes_bom() => out.extend_from_slice(&[0xEF, 0xBB, 0xBF]),
        "UTF-16LE" => out.extend_from_slice(&[0xFF, 0xFE]),
        "UTF-16BE" => out.extend_from_slice(&[0xFE, 0xFF]),
        _ => {}
    }
    out.extend_from_slice(&body);
    std::fs::write(path, out).map_err(|e| format!("{path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("subforge-core-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name).to_string_lossy().into_owned()
    }

    fn sample() -> Script {
        let mut s = Script::new();
        s.events.push(Event::new(1000, 2500, "Default", "{\\i1}İyi{\\i0} günler, şüphesiz ığ\\Nçok"));
        s.events.push(Event::new(3000, 4000, "Default", "Ömür boyu"));
        s
    }

    #[test]
    fn from_path_covers_extensions() {
        for (p, f) in [("a.ASS", Format::Ass), ("a.ssa", Format::Ass), ("a.sub", Format::MicroDvd), ("a.sbv", Format::Sbv), ("a.smi", Format::Sami), ("a.lrc", Format::Lrc), ("a.dfxp", Format::Ttml), ("a.xml", Format::Ttml), ("a.txt", Format::Txt)] {
            assert_eq!(Format::from_path(p), Some(f), "{p}");
        }
        assert_eq!(Format::from_path("noext"), None);
        assert_eq!(Format::from_path("a.mkv"), None);
        for f in Format::ALL {
            assert_eq!(Format::from_path(&format!("x.{}", f.extension())).is_some(), true);
        }
    }

    #[test]
    fn every_format_roundtrips_turkish_through_disk() {
        let s = sample();
        for f in Format::ALL {
            let path = tmp(&format!("rt-{f:?}.{}", f.extension()));
            save_path_format(&s, &path, f, "utf-8", &WriteOptions::default()).unwrap();
            let (back, det) = load_path(&path).unwrap();
            assert!(!det.lossy, "{f:?}");
            assert_eq!(back.events.len(), 2, "{f:?}");
            let vis: Vec<String> = back.events.iter().map(|e| tags::visible_text(&e.text).replace('\n', " ")).collect();
            assert_eq!(vis, ["İyi günler, şüphesiz ığ çok", "Ömür boyu"], "{f:?}");
            if f != Format::Lrc && f != Format::Txt {
                assert!((back.events[0].start - 1000).abs() <= 1, "{f:?}");
            }
        }
    }

    #[test]
    fn content_detection_beats_wrong_extension() {
        let srt = srt::write_srt(&sample());
        let p = tmp("looks-like-ass.ass");
        std::fs::write(&p, srt.as_bytes()).unwrap();
        assert_eq!(load_path(&p).unwrap().0.events.len(), 2);
        // .sub holding SubViewer vs MicroDVD
        let p = tmp("sv.sub");
        std::fs::write(&p, formats::subviewer::write_subviewer(&sample())).unwrap();
        assert!((load_path(&p).unwrap().0.events[0].start - 1000).abs() <= 1);
        let p = tmp("md.sub");
        std::fs::write(&p, formats::microdvd::write_microdvd(&sample(), 25.0)).unwrap();
        assert!((load_path(&p).unwrap().0.events[0].start - 1000).abs() <= 1);
        // no extension at all
        let p = tmp("noext");
        std::fs::write(&p, formats::sbv::write_sbv(&sample())).unwrap();
        assert_eq!(load_path(&p).unwrap().0.events[1].start, 3000);
    }

    #[test]
    fn save_with_encoding() {
        let s = sample();
        let p = tmp("tr.srt");
        save_path_with_encoding(&s, &p, "iso-8859-9").unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert!(bytes.contains(&0xDD)); // İ
        let (t, det) = encoding::decode(&bytes);
        assert!(t.contains("<i>İyi</i> günler, şüphesiz ığ"), "{}", det.name);
        let p = tmp("tr16.srt");
        save_path_with_encoding(&s, &p, "utf-16le").unwrap();
        let (t, det) = encoding::decode(&std::fs::read(&p).unwrap());
        assert_eq!(det.name, "UTF-16LE");
        assert!(t.contains("Ömür"));
        let err = save_path_with_encoding(&s, &tmp("x.srt"), "windows-1252").unwrap_err();
        assert!(err.contains("cannot represent"), "{err}");
        assert!(save_path_with_encoding(&s, &tmp("x.srt"), "nope").is_err());
    }
}
