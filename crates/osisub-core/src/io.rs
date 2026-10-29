//! Dosya yükleme/kaydetme (uzantıya göre biçim seçimi).
use crate::ass::{decode_bytes, Script};
use crate::{srt, vtt};
use std::path::Path;

pub fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn load_str(text: &str, ext: &str) -> Script {
    match ext {
        "srt" => srt::to_script(text),
        "vtt" => Script::from_dialogues(vtt::parse(text)),
        "lrc" => Script::from_dialogues(crate::formats::parse_lrc(text)),
        "sub" if text.trim_start().starts_with('{') => Script::from_dialogues(crate::formats::parse_microdvd(text, 23.976)),
        "sbv" => Script::from_dialogues(crate::formats::parse_sbv(text)),
        "ttml" | "dfxp" | "xml" => Script::from_dialogues(crate::formats::parse_ttml(text)),
        _ => Script::parse(text),
    }
}

pub fn load(path: &Path) -> Result<Script, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    Ok(load_str(&decode_bytes(&bytes), &ext_of(path)))
}

pub fn to_string(s: &Script, ext: &str) -> String {
    match ext {
        "srt" => srt::write(s),
        "vtt" => vtt::write(s),
        "lrc" => crate::formats::write_lrc(s),
        "sub" => crate::formats::write_microdvd(s, 23.976),
        "sbv" => crate::formats::write_sbv(s),
        "ttml" | "dfxp" => crate::formats::write_ttml(s),
        "txt" => crate::formats::write_txt(s),
        "csv" => crate::formats::write_csv(s),
        _ => s.to_ass_string(),
    }
}

/// Önce geçici dosyaya yazıp yeniden adlandırır; yarım kalan yazma asıl dosyayı bozmaz.
pub fn save(s: &Script, path: &Path) -> Result<(), String> {
    let out = to_string(s, &ext_of(path));
    let tmp = path.with_extension(format!("{}.tmp", ext_of(path)));
    std::fs::write(&tmp, out).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_load() {
        let dir = std::env::temp_dir().join(format!("osisub_io_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.ass");
        let s = Script::new_default();
        save(&s, &p).unwrap();
        assert_eq!(load(&p).unwrap().entries, s.entries);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
