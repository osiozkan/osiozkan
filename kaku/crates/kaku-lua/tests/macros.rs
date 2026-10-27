use kaku_core::ass::Script;
use kaku_lua::{run_by_name, Runtime};
use std::path::Path;

fn setup() -> (Runtime, Vec<kaku_lua::Registered>, Script) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let rt = Runtime::new().unwrap();
    let regs = rt.load_script(&root.join("demo_macros.lua")).unwrap();
    let sc = Script::parse(&std::fs::read_to_string(root.join("sample.ass")).unwrap());
    (rt, regs, sc)
}

#[test]
fn registers() {
    let (rt, regs, _) = setup();
    assert_eq!(regs.len(), 4);
    assert_eq!(rt.script_meta()["script_name"], "Kaku demo");
}

#[test]
fn uppercase_changes_text() {
    let (rt, regs, sc) = setup();
    let sel: Vec<usize> = sc.entries.iter().enumerate().filter(|(_, e)| matches!(e, kaku_core::ass::Entry::Line(_))).map(|(i, _)| i + 1).collect();
    let (out, _) = run_by_name(&rt, &regs, "Uppercase", sc, sel).unwrap();
    assert!(out.lines().nth(1).unwrap().text.contains("MERHABA"));
}

#[test]
fn karaoke_explodes_syllables() {
    let (rt, regs, sc) = setup();
    let (out, res) = run_by_name(&rt, &regs, "Kara FX", sc, vec![]).unwrap();
    assert_eq!(res.log, vec!["generated 3 syllable lines".to_string()]);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines.len(), 2 + 3);
    assert!(lines[0].is_comment());
    assert_eq!(lines[1].start, 1000);
    assert_eq!(lines[1].end, 1500);
    assert_eq!(lines[3].start, 2000);
    assert_eq!(lines[3].end, 3000);
    assert!(lines[1].text.starts_with("{\\pos("));
}

#[test]
fn dialog_headless_and_cancel() {
    let (rt, regs, sc) = setup();
    rt.dialog_values.borrow_mut().insert("suffix".into(), "?".into());
    rt.dialog_values.borrow_mut().insert("dbl".into(), "true".into());
    let sel: Vec<usize> = (1..=sc.entries.len()).collect();
    let (out, _) = run_by_name(&rt, &regs, "Dialog", sc.clone(), sel.clone()).unwrap();
    assert!(out.lines().next().unwrap().text.ends_with("oke??"));
    *rt.dialog_button.borrow_mut() = Some("Cancel".into());
    let (out2, res) = run_by_name(&rt, &regs, "Dialog", sc, sel).unwrap();
    assert!(res.cancelled);
    let _ = out2;
}

#[test]
fn regex_sub() {
    let (rt, regs, sc) = setup();
    let (out, _) = run_by_name(&rt, &regs, "Regex", sc, vec![]).unwrap();
    assert_eq!(out.lines().nth(1).unwrap().text, "Merhaba  dünya!");
}

#[test]
fn probe_dialog_then_answer() {
    use kaku_lua::DialogAnswer;
    let (rt, regs, sc) = setup();
    rt.probe_dialogs.set(true);
    let sel: Vec<usize> = (1..=sc.entries.len()).collect();
    let (_, res) = run_by_name(&rt, &regs, "Dialog", sc.clone(), sel.clone()).unwrap();
    let spec = res.pending_dialog.expect("dialog spec");
    assert_eq!(spec.buttons, ["Apply", "Cancel"]);
    assert!(spec.controls.iter().any(|c| c.name == "suffix" && c.value == "!"));
    rt.answers.borrow_mut().push(DialogAnswer { button: Some("Apply".into()), values: [("suffix".to_string(), "~".to_string())].into() });
    let (out, res) = run_by_name(&rt, &regs, "Dialog", sc, sel).unwrap();
    assert!(res.pending_dialog.is_none());
    assert!(out.lines().next().unwrap().text.ends_with("oke~"));
}

#[test]
fn ipairs_over_subs() {
    let rt = Runtime::new().unwrap();
    let regs = rt.load_source(b"aegisub.register_macro('c','',function(subs) local n=0 for i,l in ipairs(subs) do n=n+1 end aegisub.debug.out('n=%d',n) end)", "t").unwrap();
    let sc = Script::parse(&std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample.ass")).unwrap());
    let (_, out) = run_by_name(&rt, &regs, "c", sc.clone(), vec![]).unwrap();
    assert_eq!(out.log, vec![format!("n={}", sc.entries.len())]);
}

#[test]
fn utf8_and_unicode_modules() {
    let rt = Runtime::new().unwrap();
    let regs = rt.load_source(
        b"local u=require'utf8'; local un=require'unicode'; aegisub.register_macro('u','',function() aegisub.debug.out('%d %s %d %s', u.len('d\xc3\xbcnya'), u.sub('d\xc3\xbcnya',2,3), un.len('\xc4\xb1\xc5\x9f'), u.char(252)) end)",
        "t",
    ).unwrap();
    let (_, out) = run_by_name(&rt, &regs, "u", Script::new_default(), vec![]).unwrap();
    assert_eq!(out.log, vec!["5 ün 2 ü".to_string()]);
}

#[test]
fn karaoke_templater_generates_lines() {
    let rt = Runtime::new().unwrap();
    let regs = rt.load_builtins().unwrap();
    assert_eq!(regs[0].name, "Apply karaoke template");
    let sc = Script::parse(&std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/templater_demo.ass")).unwrap());
    let (out, res) = run_by_name(&rt, &regs, "Apply karaoke template", sc, vec![]).unwrap();
    assert!(res.log[0].contains("generated 5 line"), "{:?}", res.log);
    let fx: Vec<_> = out.lines().filter(|l| l.effect == "fx").collect();
    assert_eq!(fx.len(), 5);
    // first syl: starts 300 ms before the line start, ends at syl start (retime start2syl -300,0)
    assert_eq!(fx[0].start, 10000 - 300);
    assert_eq!(fx[0].end, 10000);
    assert!(fx[0].text.starts_with("{\\pos("));
    assert!(fx[0].text.contains("\\fad(300,200)"));
    assert!(fx[0].text.contains("\\t(0,500,"));
    assert!(fx[0].text.ends_with("}ka"));
    assert_eq!(fx[4].text, "{\\an2\\fad(100,100)}kara oke");
    // regenerating must not duplicate
    let (out2, _) = run_by_name(&rt, &regs, "Apply karaoke template", out, vec![]).unwrap();
    assert_eq!(out2.lines().filter(|l| l.effect == "fx").count(), 5);
}

#[test]
fn require_sets_globals_and_lfs() {
    let rt = Runtime::new().unwrap();
    let regs = rt.load_source(
        b"require 'karaskel'; require 'util'; local lfs = require 'lfs'; aegisub.register_macro('g','',function() aegisub.debug.out('%s %s %s', type(karaskel), type(util.copy), tostring(lfs.attributes('.', 'mode'))) end)",
        "t",
    ).unwrap();
    let (_, out) = run_by_name(&rt, &regs, "g", Script::new_default(), vec![]).unwrap();
    assert_eq!(out.log, vec!["table function directory".to_string()]);
}

#[test]
fn ffi_and_bit_available() {
    let rt = Runtime::new().unwrap();
    let regs = rt.load_source(
        b"local ffi=require'ffi'; ffi.cdef[[typedef struct {int x;} p;]]; aegisub.register_macro('f','',function() local v=ffi.new('p',7); aegisub.debug.out('%d %d', v.x, bit.bxor(5,1)) end)",
        "t",
    ).unwrap();
    let (_, out) = run_by_name(&rt, &regs, "f", Script::new_default(), vec![]).unwrap();
    assert_eq!(out.log, vec!["7 4".to_string()]);
}
