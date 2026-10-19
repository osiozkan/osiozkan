//! Content-based format detection.
use crate::Format;
use regex::Regex;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

/// Guess the format from the text itself; `None` when nothing recognisable is found.
pub fn detect_format(text: &str) -> Option<Format> {
    static SRT: OnceLock<Regex> = OnceLock::new();
    static VTT_TIME: OnceLock<Regex> = OnceLock::new();
    static SBV: OnceLock<Regex> = OnceLock::new();
    static MDVD: OnceLock<Regex> = OnceLock::new();
    static LRC: OnceLock<Regex> = OnceLock::new();
    let text = text.trim_start_matches('\u{feff}');
    let head: String = text.chars().take(8192).collect();
    let trimmed = head.trim_start();
    let lower = head.to_ascii_lowercase();
    if trimmed.starts_with("WEBVTT") {
        return Some(Format::Vtt);
    }
    if lower.contains("[script info]") || lower.contains("[v4+ styles]") || lower.contains("[v4 styles]") || lower.contains("[events]") {
        return Some(Format::Ass);
    }
    if lower.contains("<sami") || (lower.contains("<sync") && lower.contains("start=")) {
        return Some(Format::Sami);
    }
    if lower.contains("http://www.w3.org/ns/ttml") || (lower.contains("<tt") && lower.contains("<p ") && lower.contains("begin=")) {
        return Some(Format::Ttml);
    }
    if lower.contains("[information]") || lower.contains("[subtitle]") || lower.contains("[end information]") {
        return Some(Format::SubViewer);
    }
    let lines = || head.lines().map(str::trim).filter(|l| !l.is_empty()).take(20);
    if lines().any(|l| re(&MDVD, r"^\{\d+\}\{\d*\}").is_match(l)) {
        return Some(Format::MicroDvd);
    }
    if lines().any(|l| re(&SRT, r"^\d+:\d{1,2}:\d{1,2}[,.:]?\d*\s*-{1,2}>\s*\d+:\d{1,2}:\d{1,2}").is_match(l)) {
        return Some(Format::Srt);
    }
    if lines().any(|l| re(&VTT_TIME, r"^\d+:\d{2}(?:[.,]\d+)?\s*-{1,2}>\s*\d+:\d{2}").is_match(l)) {
        return Some(Format::Vtt);
    }
    if lines().any(|l| re(&SBV, r"^\d+:\d{2}:\d{2}[.,]\d{1,3},\d+:\d{2}:\d{2}[.,]\d{1,3}$").is_match(l)) {
        return Some(Format::Sbv);
    }
    if lines().any(|l| re(&LRC, r"^\[(?:\d+:\d{2}(?:[.:]\d{1,3})?|[A-Za-z]+:[^\]]*)\]").is_match(l)) {
        return Some(Format::Lrc);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_each() {
        let cases: &[(&str, Format)] = &[
            ("\u{feff}WEBVTT\n\n00:01.000 --> 00:02.000\nx", Format::Vtt),
            ("[Script Info]\nTitle: x", Format::Ass),
            ("[V4 Styles]\nFormat: Name", Format::Ass),
            ("<SAMI><BODY><SYNC Start=0><P Class=EN>x", Format::Sami),
            ("<?xml version=\"1.0\"?><tt xmlns=\"http://www.w3.org/ns/ttml\"><body/></tt>", Format::Ttml),
            ("<tt><body><div><p begin=\"1s\" end=\"2s\">x</p></div></body></tt>", Format::Ttml),
            ("[INFORMATION]\n[TITLE]x\n[END INFORMATION]", Format::SubViewer),
            ("{1}{1}25\n{25}{50}hello", Format::MicroDvd),
            ("{100}{200}hello|world", Format::MicroDvd),
            ("1\n00:00:01,000 --> 00:00:02,000\nx\n", Format::Srt),
            ("0:00:01.000 --> 0:00:02.000\nx\n", Format::Srt),
            ("00:01.000 --> 00:02.000\nx\n", Format::Vtt),
            ("0:00:01.599,0:00:04.160\nx\n", Format::Sbv),
            ("[ti:Song]\n[00:01.00]la\n", Format::Lrc),
            ("[00:01.00]la\n", Format::Lrc),
        ];
        for (t, f) in cases {
            assert_eq!(detect_format(t), Some(*f), "{t:?}");
        }
        assert_eq!(detect_format("just some words\nmore words"), None);
        assert_eq!(detect_format(""), None);
    }
}
