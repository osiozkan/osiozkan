//! WebVTT okuma/yazma.
use crate::ass::{Dialogue, Script};
use crate::{srt, time};

pub fn parse(text: &str) -> Vec<Dialogue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let mut body = text.as_str();
    if body.starts_with("WEBVTT") {
        body = body.split_once("\n\n").map(|x| x.1).unwrap_or("");
    }
    // Cue başlığı satırı (`id`) srt ayrıştırıcısı tarafından atlanır; NOTE/STYLE bloklarını ele.
    let cleaned: Vec<&str> = body
        .split("\n\n")
        .filter(|b| !b.starts_with("NOTE") && !b.starts_with("STYLE") && !b.starts_with("REGION"))
        .collect();
    srt::parse(&cleaned.join("\n\n"))
}

pub fn write(script: &Script) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for d in script.dialogues().filter(|d| !d.comment) {
        out.push_str(&format!("{} --> {}\n{}\n\n", time::format_vtt(d.start_time), time::format_vtt(d.end_time), srt::ass_text_to_srt(&d.text)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vtt() {
        let d = parse("WEBVTT\n\nNOTE x\n\ncue1\n00:00:01.000 --> 00:00:02.000 align:start\nHi\n");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].end_time, 2000);
    }
}
