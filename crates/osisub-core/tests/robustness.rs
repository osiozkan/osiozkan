//! Rastgele/bozuk girdilerde hiçbir ayrıştırıcı panik atmamalı (çok baytlı karakterlerde dilimleme dahil).
use osisub_core::ass::{decode_bytes, Script};
use osisub_core::{fixes, formats, overrides, srt, tags, time, vtt};

fn rng(seed: u64) -> impl FnMut(usize) -> usize {
    let mut s = seed;
    move |n| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((s >> 33) as usize) % n.max(1)
    }
}

const FRAGS: &[&str] = &[
    "1", "2", "00:00:01,000", "-->", "00:00:02.500", "0:00:01.00", "\n", "\n\n", "\r\n", "[Events]", "[Script Info]", "[V4+ Styles]", "Dialogue: ", "Comment: ", "Style: ", "Format: ",
    "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text", ",", ",,,,,", "0", "Default", "{", "}", "\\", "\\N", "\\k20", "\\pos(", ")", "<i>", "</i>", "<font color=\"#ff0000\">",
    "<", ">", "&", "&amp;", "[00:01.50]", "<00:01.50>", "{25}{50}", "|", "ş", "İ", "日本語", "♪", "…", "\u{feff}", "WEBVTT", "NOTE", "<p begin=\"1s\" end=\"2s\">", "</p>", "<br/>", "begin=\"", "\"", ":", ";", ".", " ", "x",
];

fn rounds(default: usize) -> usize {
    std::env::var("OSISUB_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn random_text(r: &mut impl FnMut(usize) -> usize) -> String {
    (0..r(40)).map(|_| FRAGS[r(FRAGS.len())]).collect()
}

#[test]
fn parsers_never_panic() {
    let mut r = rng(0xABCDEF0123456789);
    for _ in 0..rounds(3000) {
        let t = random_text(&mut r);
        let _ = Script::parse(&t);
        let _ = srt::parse(&t);
        let _ = vtt::parse(&t);
        let _ = formats::parse_lrc(&t);
        let _ = formats::parse_microdvd(&t, 23.976);
        let _ = formats::parse_sbv(&t);
        let _ = formats::parse_ttml(&t);
        let _ = overrides::parse(&t);
        let _ = overrides::clean(&t);
        let _ = tags::visible_text(&t);
        let _ = tags::karaoke_syllables(&t);
        let _ = osisub_core::karaoke::parse(&t);
        let _ = srt::ass_text_to_srt(&t);
        let _ = fixes::fix_text(&t, &fixes::Fixes { ellipsis: true, sentence_case: true, remove_hi: true, ..Default::default() });
        let _ = osisub_core::qc::balance_lines(&t, 20);
        let _ = osisub_core::qc::clean_text(&t);
        let _ = time::parse_generic(&t);
        let _ = osisub_core::color::Color::parse(&t);
        // Rastgele ham baytlar
        let bytes: Vec<u8> = (0..r(64)).map(|_| r(256) as u8).collect();
        let _ = decode_bytes(&bytes);
    }
}

#[test]
fn script_ops_never_panic_on_garbage_scripts() {
    let mut r = rng(0x0F0F0F0F12345678);
    for _ in 0..rounds(500) {
        let t = random_text(&mut r);
        let mut s = Script::parse(&t);
        let _ = s.to_ass_string();
        let _ = osisub_core::qc::check(&s, &osisub_core::qc::Rules::default());
        osisub_core::qc::fix_overlaps(&mut s);
        osisub_core::qc::sort_by_start(&mut s);
        let _ = fixes::apply(&mut s, &fixes::Fixes { split_long: Some(10), min_duration_ms: Some(500), min_gap_ms: Some(40), sentence_case: true, ..Default::default() });
        osisub_core::timing::post_process(&mut s, &osisub_core::timing::PostOptions::default());
        osisub_core::timing::resample(&mut s, 640, 360);
        let _ = osisub_core::stats::compute(&s);
        let _ = osisub_core::fonts_used::used_fonts(&s);
        let o = osisub_core::search::Options { pattern: "a".into(), replacement: "b".into(), ..Default::default() };
        let _ = osisub_core::search::replace_all(&mut s, &o);
        let _ = osisub_core::formats::write_lrc(&s);
        let _ = osisub_core::formats::write_ttml(&s);
        let _ = osisub_core::srt::write(&s);
    }
}
