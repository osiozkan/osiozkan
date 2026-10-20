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

#[test]
fn dialog_handler_and_progress() {
    use nsub_lua::dialog::{DialogResponse, Val};
    use std::cell::RefCell;
    use std::rc::Rc;
    let seen = Rc::new(RefCell::new(vec![]));
    let seen2 = seen.clone();
    let opts = Options {
        echo_log: false,
        dialog_handler: Some(Rc::new(|req| {
            assert_eq!(req.buttons, vec!["Go", "Stop"]);
            let mut r = DialogResponse { button: Some("Go".into()), ..Default::default() };
            r.values.insert("n".into(), Val::Num(7.0));
            r.values.insert("pick".into(), Val::Str("b".into()));
            r
        })),
        progress_handler: Some(Rc::new(move |p| seen2.borrow_mut().push((p.fraction, p.task)))),
        ..Default::default()
    };
    let mut a = Automation::new(opts).unwrap();
    let dir = std::env::temp_dir().join(format!("nsub-dlg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("d.lua");
    std::fs::write(
        &f,
        r#"aegisub.register_macro("D", "", function(subs)
  aegisub.progress.task("working %d", 3)
  aegisub.progress.set(50)
  local b, r = aegisub.dialog.display({
    {class="intedit", name="n", value=1, x=0, y=0},
    {class="dropdown", name="pick", items={"a","b"}, value="a", x=0, y=1},
  }, {"Go", "Stop"}, {ok="Go", cancel="Stop"})
  aegisub.log("%s %d %s", b, r.n, r.pick)
end)"#,
    )
    .unwrap();
    let m = a.load_script(&f).unwrap();
    let mut s = script();
    let r = a.run(m[0].id, &mut s, &[], 1).unwrap();
    assert_eq!(r.log, "Go 7 b");
    let seen = seen.borrow();
    assert!(seen.contains(&(None, Some("working 3".to_string()))));
    assert!(seen.iter().any(|(f, _)| *f == Some(50.0)));
}

#[test]
fn lpeg_and_moonscript() {
    let a = Automation::new(Options { echo_log: false, ..Default::default() }).unwrap();
    assert_eq!(a.eval("local lpeg = require('lpeg'); return tostring(lpeg.match(lpeg.C(lpeg.R('az')^1), 'hello world'))").unwrap(), "hello");
    let r = a
        .eval("local moon = require('moonscript'); local f = assert(moon.loadstring('f = (x) -> x * 2\\nreturn f 21')); return tostring(f())")
        .unwrap();
    assert_eq!(r, "42");
}

#[test]
fn moon_script_file_registers_macro() {
    let dir = std::env::temp_dir().join(format!("nsub-moon-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("m.moon"),
        "export script_name = 'moon'\naegisub.register_macro 'Moon/Hello', 'says hi', (subs, sel) ->\n  l = subs[sel[1]]\n  l.text = 'moon: ' .. l.text\n  subs[sel[1]] = l\n",
    )
    .unwrap();
    // a .moon module next to the script is found by require
    std::fs::write(dir.join("helper.moon"), "{ twice: (x) -> x * 2 }\n").unwrap();
    std::fs::write(dir.join("uses.lua"), "local h = require('helper'); aegisub.register_macro('Uses', '', function() aegisub.log('%d', h.twice(4)) end)\n").unwrap();
    let mut a = Automation::new(Options { echo_log: false, ..Default::default() }).unwrap();
    let m = a.load_script(&dir.join("m.moon")).unwrap();
    assert_eq!(m[0].name, "Moon/Hello");
    let mut s = script();
    let i = Automation::flat_index_of_event(&s, 1);
    a.run(m[0].id, &mut s, &[i], i).unwrap();
    assert_eq!(s.events[1].text, "moon: hello world");
    let u = a.load_script(&dir.join("uses.lua")).unwrap();
    let r = a.run(u[0].id, &mut s, &[], 1).unwrap();
    assert_eq!(r.log, "8");
    std::fs::remove_dir_all(dir).ok();
}
