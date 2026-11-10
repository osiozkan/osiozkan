//! Aegisub'ın kendi kara-templater.lua betiğinin osisub Lua ortamında birebir çalıştığını doğrular.
use osisub_core::ass::Script;
use osisub_lua::{Engine, HeadlessHost};
use std::path::Path;

#[test]
fn kara_templater_generates_fx_lines() {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(&std::fs::read_to_string("tests/fixtures/templater.ass").unwrap()));
    e.load_script(Path::new("lua/autoload/kara-templater.lua")).unwrap();
    let id = e.macros().iter().position(|m| m.name.contains("template")).expect("templater makrosu");
    e.run_macro(id, &[], 0).unwrap();
    let s = e.script();
    let fx: Vec<_> = s.dialogues().filter(|d| d.effect == "fx").collect();
    // "template syl noblank": 4 hece; "template line": 1 satır
    assert_eq!(fx.len(), 5, "{:#?}\nlog: {}", fx.iter().map(|d| &d.text).collect::<Vec<_>>(), e.take_log());
    let syl0 = fx.iter().find(|d| d.text.contains("\\pos(")).unwrap();
    assert!(syl0.text.contains("\\fad(300,200)"), "{}", syl0.text);
    assert!(syl0.text.contains("\\t(0,500,\\1c&H00FFFF&)"), "{}", syl0.text);
    assert_eq!(syl0.start_time, 10000 - 300, "retime start2syl -300");
    // Orijinal karaoke satırı yorum satırına dönüşmeli
    assert!(s.dialogues().any(|d| d.comment && d.effect == "karaoke"));
}
