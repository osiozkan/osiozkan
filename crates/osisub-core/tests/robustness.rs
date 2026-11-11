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
    "<", ">", "&", "&amp;", "[00:01.50]", "<00:01.50>", "{25}{50}", "|", "ş", "İ", "日本語", "♪", "…", "\u{feff}", "WEBVTT", "NOTE", "<p begin=\"1s\" end=\"2s\">", "</p>", "<br/>", "begin=\"", "\"", ":", ";", ".", " ", "x", "<SYNC Start=", "<sync start=\"", "<P Class=ENCC>", "class=", "&nbsp;", "</body>", "[INFORMATION]", "[br]", "<br",
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
        let _ = formats::parse_sami(&t);
        let _ = formats::parse_subviewer(&t);
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

const FRAGS2: &[&str] = &[
    "m", "l", "b", "n", "s", "p", "c", " ", "0", "-5", "12.5", "1e9", "x", "\n", "# timecode format v1", "# timecode format v2", "Assume", "Assume 23.976", "0,10,30", ",", "-1", "nan", "inf",
    "fontname: a.ttf", "filename: b.png", "[Fonts]", "[Graphics]", "!", "`", "[", "]", "\t", "kya", "n'", "tt", "っ", "ょ", "ー", "漢", "kitto", "ki", "a", "{\\k10}", "{\\an8}", "{\\kf5}", "ş",
];

fn random_text2(r: &mut impl FnMut(usize) -> usize) -> String {
    (0..r(30)).map(|_| FRAGS2[r(FRAGS2.len())]).collect()
}

/// Bu gecenin yeni ayrıştırıcıları da rastgele girdide panik atmamalı.
#[test]
fn new_parsers_never_panic() {
    use osisub_core::{attach, compare, drawing, karaoke, select, tm, vfr};
    let mut r = rng(0x5EED_2026_1110);
    for _ in 0..rounds(3000) {
        let t = random_text2(&mut r);
        let mut d = drawing::parse(&t);
        let _ = drawing::to_string(&d);
        let _ = drawing::polylines(&d);
        let pts = drawing::points(&d);
        if let Some(&(ci, pi, _)) = pts.get(r(pts.len().max(1))) {
            drawing::remove_point(&mut d, ci, pi);
        }
        drawing::push_point(&mut d, (1.0, 2.0));
        if let Ok(fr) = vfr::Framerate::parse_timecodes(&t) {
            for f in [-3i64, 0, 1, 5, 1000] {
                let ms = fr.time_at(f);
                let _ = (fr.frame_at(ms), fr.time_at_start(f), fr.time_at_end(f), fr.frame_at_start(ms), fr.snap(ms), fr.snap_start(ms), fr.snap_end(ms));
            }
            let _ = fr.to_v2(3);
        }
        let _ = attach::decode(&t.lines().collect::<Vec<_>>());
        let _ = tm::Memory::parse(&t).suggest(&t, 3, 50);
        let _ = karaoke::auto_syllables(&t);
        let (lead, syls) = karaoke::parse_with_lead(&t);
        let _ = karaoke::build_with_lead(&lead, &syls);
        let s1 = Script::parse(&format!("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:0{}.00,0:00:0{}.00,Default,,0,0,0,,{}\n", r(9), r(9), t.replace('\n', " ")));
        let s2 = Script::parse(&random_text(&mut r));
        let _ = compare::compare(&s1, &s2);
        let _ = attach::list(&s2);
        let c = select::Criteria { pattern: t.chars().take(5).collect(), mode: [select::Mode::Contains, select::Mode::Exact, select::Mode::Regex][r(3)], ..Default::default() };
        let _ = select::select(&s1, &c, &[0, 5, 99]);
        let fr = vfr::Framerate::cfr([23.976, 24.0, 29.97, 60.0][r(4)]);
        let mut s3 = s1.clone();
        let _ = osisub_core::qc::frame_issues(&s3, &fr, &[0, 1000, 2002], 3);
        osisub_core::qc::snap_to_keyframes(&mut s3, &fr, &[0, 1000], 3);
        osisub_core::qc::shift_frames(&mut s3, &fr, r(7) as i64 - 3, None);
        osisub_core::qc::snap_to_framerate(&mut s3, &fr);
    }
}
