use osisub_core::{Entry, Script};
use osisub_lua::{Automation, RunError};

fn sample() -> Script {
    Script::parse(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/data/sample.ass")).unwrap()).unwrap()
}
fn setup() -> Automation {
    let a = Automation::new(sample()).unwrap();
    a.load_file(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/lua/basic.lua")).unwrap();
    a
}
fn texts(a: &Automation) -> Vec<String> {
    a.script().events().map(|e| e.text.clone()).collect()
}
fn first_dialogue(a: &Automation) -> usize {
    a.script().entries.iter().position(|e| matches!(e, Entry::Dialogue(_))).unwrap() + 1
}

#[test]
fn registers_macros_with_script_name() {
    let a = setup();
    let m = a.list_macros();
    assert_eq!(m.len(), 10);
    assert!(m.iter().all(|m| m.script == "Test suite macros"));
    assert!(m.iter().any(|m| m.is_filter && m.name == "Filt"));
}

#[test]
fn uppercase_macro_edits_selection() {
    let a = setup();
    let f = first_dialogue(&a);
    a.run("Upper", &[f, f + 2], f).unwrap();
    let t = texts(&a);
    assert_eq!(t[0], "MERHABA DüNYA"); // string.upper is ASCII-only in LuaJIT, like Aegisub
    assert_eq!(t[1], "{\\k50}Ka{\\k30}ra{\\kf40}o{\\k20}ke");
}

#[test]
fn insert_delete_append() {
    let a = setup();
    let f = first_dialogue(&a);
    let r = a.run("InsDel", &[f, f + 1], f).unwrap();
    assert_eq!(r.selection, vec![f]);
    let t = texts(&a);
    assert_eq!(t[0], "Merhaba dünya");
    assert_eq!(t[1], "inserted");
    assert_eq!(t.last().unwrap(), "appended");
    assert_eq!(t.len(), 4);
}

#[test]
fn karaskel_and_syllables() {
    let a = setup();
    let f = first_dialogue(&a);
    a.run("Kara", &[f + 1], f + 1).unwrap();
    let log = a.logs().join("");
    assert!(log.contains("res 1280x720 styles=1"), "{log}");
    assert!(log.contains("kara=4"), "{log}");
    assert!(log.contains("syl 2 'ra' 500..800"), "{log}");
    assert!(log.contains("valign=bottom x=640"), "{log}");
}

#[test]
fn error_rolls_back() {
    let a = setup();
    let f = first_dialogue(&a);
    let before = a.take_script();
    let e = a.run("Boom", &[f], f).err().unwrap();
    assert!(matches!(e, RunError::Lua(ref s) if s.contains("kaboom")));
    assert_eq!(a.take_script(), before);
}

#[test]
fn cancel_rolls_back() {
    let a = setup();
    let f = first_dialogue(&a);
    let before = a.take_script();
    assert!(matches!(a.run("Cancel", &[f], f), Err(RunError::Cancelled)));
    assert_eq!(a.take_script(), before);
}

#[test]
fn util_and_unicode() {
    let a = setup();
    a.run("Colors", &[], 1).unwrap();
    assert!(a.logs().join("").contains("util ok"));
}

#[test]
fn dialog_defaults_and_overrides() {
    let a = setup();
    a.run("Dlg", &[], 1).unwrap();
    assert!(a.logs().join("").contains("dlg Apply 3 true b"), "{:?}", a.logs());
    a.set_dialog_values(&[("amount".into(), "7".into())]).unwrap();
    a.run("Dlg", &[], 1).unwrap();
    assert!(a.logs().join("").contains("dlg Apply 7 true b"));
}

#[test]
fn frames_need_video() {
    let a = setup();
    a.run("Frames", &[], 1).unwrap();
    assert!(a.logs().join("").contains("frame nil ms nil"));
    a.set_video(1920, 1080, 24.0);
    a.run("Frames", &[], 1).unwrap();
    assert!(a.logs().join("").contains("frame 24 ms 1000"));
}

#[test]
fn validate_and_filter() {
    let a = setup();
    let f = first_dialogue(&a);
    assert!(matches!(a.run("NeedsTwo", &[f], f), Err(RunError::Invalid)));
    a.run("NeedsTwo", &[f, f + 1], f).unwrap();
    a.run("Filt", &[], 1).unwrap();
    assert!(a.script().events().all(|e| e.effect == "filtered"));
    assert!(matches!(a.run("Nope", &[], 1), Err(RunError::NotFound(_))));
}

#[test]
fn style_margin_alias() {
    let a = setup();
    a.load_file(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/lua/basic.lua")).unwrap();
    let lua_src = "script_name='m'\naegisub.register_macro('M','',function(subs) for i=1,#subs do local l=subs[i] if l.class=='style' then l.margin_v=77 subs[i]=l end end end)\n";
    let p = std::env::temp_dir().join("osisub_margin.lua");
    std::fs::write(&p, lua_src).unwrap();
    a.load_file(p.to_str().unwrap()).unwrap();
    a.run("M", &[], 1).unwrap();
    assert_eq!(a.script().style("Default").unwrap().margin_v, 77);
}
