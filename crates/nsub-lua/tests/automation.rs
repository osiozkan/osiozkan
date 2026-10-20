use nsub_core::Script;
use nsub_lua::{Automation, Options};
use std::path::PathBuf;

fn script() -> Script {
    Script::parse("[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,20,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\k20}ta{\\k30}be{\\k50}ru\nDialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,hello world\n")
}

fn auto() -> (Automation, Vec<nsub_lua::MacroInfo>) {
    let mut a = Automation::new(Options { echo_log: false, ..Default::default() }).unwrap();
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scripts/sample.lua");
    let m = a.load_script(&p).unwrap();
    (a, m)
}

fn find(m: &[nsub_lua::MacroInfo], n: &str) -> usize {
    m.iter().find(|x| x.name == n).unwrap().id
}

#[test]
fn registers_macros() {
    let (_, m) = auto();
    assert_eq!(m.len(), 5);
    assert_eq!(m[0].name, "Test/Upper");
}

#[test]
fn upper_macro_modifies_script() {
    let (a, m) = auto();
    let mut s = script();
    let first_ev = Automation::flat_index_of_event(&s, 1);
    let r = a.run(find(&m, "Test/Upper"), &mut s, &[first_ev], first_ev).unwrap();
    assert_eq!(s.events[1].text, "HELLO WORLD");
    assert_eq!(s.events[0].text, "{\\k20}ta{\\k30}be{\\k50}ru");
    assert_eq!(r.selection, vec![first_ev]);
}

#[test]
fn karaskel_and_utils() {
    let (a, m) = auto();
    let mut s = script();
    let i = Automation::flat_index_of_event(&s, 0);
    let r = a.run(find(&m, "Test/Karaoke"), &mut s, &[i], i).unwrap();
    assert!(r.log.contains("syls=3 width>0=true"), "{}", r.log);
    assert_eq!(s.events[0].effect, "k3");
    assert_eq!(s.events[0].actor, "&H0080FF&");
}

#[test]
fn insert_delete_append_and_re() {
    let (a, m) = auto();
    let mut s = script();
    let i = Automation::flat_index_of_event(&s, 1);
    a.run(find(&m, "Test/Dup"), &mut s, &[i], i).unwrap();
    assert_eq!(s.events.len(), 3);
    assert_eq!(s.events[2].text, "<hello> <world>");
}

#[test]
fn cancel_leaves_script_alone() {
    let (a, m) = auto();
    let mut s = script();
    let before = s.clone();
    let r = a.run(find(&m, "Test/Cancel"), &mut s, &[], 1).unwrap();
    assert!(r.canceled);
    assert_eq!(s, before);
}

#[test]
fn headless_dialog_defaults() {
    let (a, m) = auto();
    let mut s = script();
    let r = a.run(find(&m, "Test/Dialog"), &mut s, &[], 1).unwrap();
    assert_eq!(r.log, "true abc true");
}

#[test]
fn lua_errors_surface() {
    let a = Automation::new(Options { echo_log: false, ..Default::default() }).unwrap();
    assert!(a.eval("error('boom')").unwrap_err().contains("boom"));
    assert_eq!(a.eval("return unicode and 1 or 'none'").unwrap(), "none");
    assert_eq!(a.eval("local u = require('unicode'); return u.upper_tr('işık')").unwrap(), "İŞIK");
}
