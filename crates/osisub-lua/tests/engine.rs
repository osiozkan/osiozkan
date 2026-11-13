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

#[test]
fn moonscript_macro_runs() {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    e.load_script(Path::new("tests/fixtures/sample.moon")).unwrap();
    let id = e.macros().iter().position(|m| m.name == "Moon örneği").expect("makro kaydı");
    let idx = last_dialogue_idx(&e);
    e.run_macro(id, &[idx], idx).unwrap();
    assert!(e.take_log().contains("degisti=1"));
    assert_eq!(e.script().dialogues().next().unwrap().text, "{\\K20}KA{\\K30}RA");
}

#[test]
fn require_finds_include_dir_modules() {
    let dir = std::env::temp_dir().join(format!("osisub-req-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("l0")).unwrap();
    std::fs::write(dir.join("l0/Yardim.moon"), "{ ikiKat: (x) -> x * 2 }\n").unwrap();
    std::fs::write(dir.join("duz.lua"), "return { ad = 'düz' }\n").unwrap();
    let scr = dir.join("kullan.lua");
    std::fs::write(&scr, "script_name='k'\nlocal y = require 'l0.Yardim'\nlocal d = require 'duz'\naegisub.register_macro('k','k',function() aegisub.log('%d %s', y.ikiKat(21), d.ad) end)\n").unwrap();
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.add_include_dir(&dir);
    e.load_script(&scr).unwrap();
    e.run_macro(0, &[], 0).unwrap();
    assert_eq!(e.take_log(), "42 düz");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn moonscript_errors_point_to_moon_lines() {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    e.load_script(Path::new("tests/fixtures/hatali.moon")).unwrap();
    let err = e.run_macro(0, &[], 0).unwrap_err().message;
    // hata 10. satırda (y.alan = 3)
    assert!(err.contains("hatali.moon:10:"), "{}", err);
    assert_eq!(osisub_lua::error_location(&err).map(|x| x.1), Some(10));
}

#[test]
fn dependency_control_shim() {
    let tmp = std::env::temp_dir().join(format!("osisub-depctrl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    e.set_script(Script::parse(ASS));
    e.set_property("user_dir", &tmp.to_string_lossy());
    e.add_include_dir("tests/fixtures/depctrl_include");
    let info = e.load_script(Path::new("tests/fixtures/depctrl_script.lua")).unwrap();
    assert_eq!(info.version, "1.0.2");
    let names: Vec<String> = e.macros().iter().map(|m| m.name.clone()).collect();
    assert_eq!(names, vec!["DepCtrl Testi/Say", "DepCtrl Testi/Ayar", "DepCtrl Testi"]);
    let idx = last_dialogue_idx(&e);
    e.run_macro(0, &[idx], idx).unwrap();
    let log = e.take_log();
    assert!(log.contains("sayi=1"), "{log}");
    assert!(log.contains("json=3,ğüşİ\n\"x\",true"), "{log}");
    e.run_macro(1, &[idx], idx).unwrap();
    let log = e.take_log();
    assert!(log.contains("kalinlik=3 ad=varsayılan"), "{log}");
    let saved = std::fs::read_to_string(tmp.join("config/osisub.DepTest.json")).unwrap();
    assert!(saved.contains("\"kalinlik\":3"), "{saved}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn get_frame_api() {
    struct FrameHost;
    impl osisub_lua::Host for FrameHost {
        fn get_frame(&mut self, frame: i64, subs: Option<String>) -> Option<(u32, u32, Vec<u8>)> {
            // 2×1 kare: (255,0,0) ve (1,2,3); altyazılı istenirse ikinci piksel beyaz
            let white = subs.map_or(false, |s| s.contains("[Events]"));
            (frame == 5).then(|| (2, 1, vec![255, 0, 0, 255, if white { 255 } else { 1 }, if white { 255 } else { 2 }, if white { 255 } else { 3 }, 255]))
        }
    }
    let mut e = Engine::new(Box::new(FrameHost)).unwrap();
    e.set_script(Script::parse(ASS));
    let dir = std::env::temp_dir().join(format!("osisub-gf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("gf.lua");
    std::fs::write(&f, r#"
script_name = "gf"
aegisub.register_macro("gf", "", function()
  local fr = aegisub.get_frame(5, false)
  aegisub.log("%d %d %06X %s %s\n", fr:width(), fr:height(), fr:getPixel(0, 0), fr:getPixelFormatted(1, 0), tostring(fr:getPixel(9, 9)))
  aegisub.log("%s %s\n", aegisub.get_frame(5, true):getPixelFormatted(1, 0), tostring(aegisub.get_frame(6)))
end)
"#).unwrap();
    e.load_script(&f).unwrap();
    e.run_macro(0, &[], 0).unwrap();
    let log = e.take_log();
    assert!(log.contains("2 1 FF0000 &H030201& nil"), "{log}");
    assert!(log.contains("&HFFFFFF& nil"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn line_extra_roundtrips_through_extradata() {
    let mut e = Engine::new(Box::new(HeadlessHost { quiet: true, ..Default::default() })).unwrap();
    let src = ASS.replace("{\\k20}ka", "{=1}{\\k20}ka") + "\n[Aegisub Extradata]\nData: 1,eski,edeğer#2C virgül\n";
    e.set_script(Script::parse(&src));
    let f = std::env::temp_dir().join(format!("osisub_extra_{}.lua", std::process::id()));
    std::fs::write(
        &f,
        r#"
aegisub.register_macro("x", "", function(subs, sel)
  local i = sel[1]
  local l = subs[i]
  assert(l.extra.eski == "değer, virgül", "okuma: " .. tostring(l.extra.eski))
  assert(not l.text:find("{=", 1, true), "ön ek metinde görünmemeli")
  l.extra["a-mo.json"] = '{"x":[1,2]}'
  l.extra.sayi = 5
  subs[i] = l
  local c = subs[i]
  assert(c.extra["a-mo.json"] == '{"x":[1,2]}' and c.extra.sayi == "5" and c.extra.eski == "değer, virgül", "yeniden okuma")
  c.extra = {}
  subs.append(c)
end)
"#,
    )
    .unwrap();
    e.load_script(&f).unwrap();
    let idx = last_dialogue_idx(&e);
    e.run_macro(0, &[idx], idx).unwrap();
    let _ = std::fs::remove_file(&f);
    let sc = e.script();
    let ds: Vec<_> = sc.dialogues().cloned().collect();
    assert_eq!(ds.len(), 2);
    assert_eq!(ds[0].extra.len(), 3);
    assert!(ds[1].extra.is_empty());
    let out = sc.to_ass_string();
    // eklenen olay [Events] içinde kalır, Extradata bölümü tek başlıklı
    assert_eq!(out.matches("[Events]").count(), 1, "{out}");
    assert_eq!(out.matches("[Aegisub Extradata]").count(), 1, "{out}");
    assert!(out.find("[Aegisub Extradata]").unwrap() > out.rfind("Dialogue:").unwrap(), "{out}");
    let back = Script::parse(&out);
    let d0 = back.dialogues().next().unwrap();
    let vals = back.extra_values(&d0.extra);
    assert_eq!(vals["a-mo.json"], "{\"x\":[1,2]}");
    assert_eq!(vals["eski"], "değer, virgül");
}
