use osisub_core::ass::{Entry, Script};
use osisub_lua::{Engine, HeadlessHost};
use std::path::Path;

const ASS: &str = "[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\k20}ka{\\k30}ra\n";

fn engine() -> Engine {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    e.load_script(Path::new("tests/fixtures/sample.lua")).unwrap();
    e
}

fn last_dialogue_idx(e: &Engine) -> usize {
    e.script().entries.iter().position(|x| matches!(x, Entry::Dialogue(_))).unwrap() + 1
}

#[test]
fn registers_and_shifts() {
    let mut e = engine();
    assert_eq!(e.macros().len(), 3);
    let idx = last_dialogue_idx(&e);
    let out = e.run_macro(0, &[idx], idx).unwrap();
    assert_eq!(out.selection, Some(vec![idx]));
    assert_eq!(out.undo_desc.as_deref(), Some("kaydir"));
    let d = e.script().dialogues().next().unwrap().clone();
    assert_eq!(d.start_time, 2000);
    assert!(d.text.ends_with('!'));
}

#[test]
fn karaoke_and_libs() {
    let mut e = engine();
    let idx = last_dialogue_idx(&e);
    e.run_macro(1, &[idx], idx).unwrap();
    let log = e.take_log();
    assert!(log.contains("sylls=2 first=\"ka\" dur=200"), "{}", log);
    assert!(log.contains("re=k,a up=İŞ len=3"), "{}", log);
    assert_eq!(e.script().dialogues().count(), 2);
    assert_eq!(e.script().dialogues().next().unwrap().text, "kara!".replace('!', ""));
}

#[test]
fn cancel_and_dialog_defaults() {
    let mut e = engine();
    let out = e.run_macro(2, &[], 0).unwrap();
    assert!(out.cancelled);
}

#[test]
fn real_font_extents_when_available() {
    use osisub_lua::StyleMetrics;
    let mut db = osisub_lua::fonts::FontDb::default();
    if db.family_count() == 0 {
        return; // ortamda yazı tipi yok
    }
    // Herhangi bir sistem yazı tipi için ölçüler tutarlı olmalı: ikiye katlanan metin ~iki kat geniş.
    let mut found = false;
    for fam in ["dejavu sans", "arial", "liberation sans", "noto sans"] {
        let st = StyleMetrics { fontname: fam.into(), fontsize: 40.0, scale_x: 100.0, scale_y: 100.0, ..Default::default() };
        if let (Some(a), Some(b)) = (db.extents(&st, "Hello"), db.extents(&st, "HelloHello")) {
            assert!((b.0 - 2.0 * a.0).abs() < 0.01);
            assert_eq!(a.1, 40.0);
            found = true;
            break;
        }
    }
    eprintln!("fonts: {} aile, eşleşme={}", db.family_count(), found);
}

#[test]
fn compat_script_patterns() {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    let info = e.load_script(Path::new("tests/fixtures/compat.lua")).unwrap();
    assert_eq!(info.macros.len(), 2);
    let idx = e.script().entries.iter().position(|x| matches!(x, Entry::Dialogue(_))).unwrap() + 1;
    assert!(e.validate_macro(0, &[idx], idx));
    assert!(!e.validate_macro(0, &[], 0));
    let out = e.run_macro(0, &[idx], idx).unwrap();
    assert!(out.selection.is_some());
    assert_eq!(out.undo_desc.as_deref(), Some("uyumluluk"));
    // Filtre
    e.run_macro(1, &[], 0).unwrap();
    assert!(e.script().dialogues().all(|d| d.text.ends_with('?')));
}

#[test]
fn api_misuse_yields_errors_not_panics() {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    e.load_script(Path::new("tests/fixtures/abuse.lua")).unwrap();
    let r = e.run_macro(0, &[], 0);
    assert!(r.is_ok(), "{:?}", r.err());
    let log = e.take_log();
    assert!(log.contains("hata="), "{}", log);
    // Betik bozuk durumda da çalışmaya devam etmeli ve belge tutarlı kalmalı
    let _ = e.script().to_ass_string();
}
