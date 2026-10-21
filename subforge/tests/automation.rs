use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use subforge::ass::{Document, Entry};
use subforge::automation::{RunOptions, Script};

fn fx(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn doc() -> Rc<RefCell<Document>> {
    Rc::new(RefCell::new(Document::parse(&std::fs::read_to_string(fx("sample.ass")).unwrap())))
}

#[test]
fn lists_macros() {
    let s = Script::load(&fx("test_macro.lua"), doc(), &RunOptions::default()).unwrap();
    let names: Vec<_> = s.list().unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["Collapse spaces", "Kara dup", "Dialog", "Boom", "Cancel"]);
    assert_eq!(s.meta()["script_name"], "Test macro");
}

#[test]
fn regex_macro_edits_text() {
    let d = doc();
    let s = Script::load(&fx("test_macro.lua"), d.clone(), &RunOptions::default()).unwrap();
    let r = s.run("Collapse spaces", &RunOptions::default()).unwrap();
    assert!(r.log.iter().any(|l| l.contains("fixed 3 lines")));
    assert!(d.borrow().events().any(|e| e.text == "Hello world"));
}

#[test]
fn karaskel_and_append() {
    let d = doc();
    let s = Script::load(&fx("test_macro.lua"), d.clone(), &RunOptions::default()).unwrap();
    let r = s.run("Kara dup", &RunOptions::default()).unwrap();
    assert!(r.log.iter().any(|l| l.contains("first syl start=0 dur=200")), "{:?}", r.log);
    let d = d.borrow();
    assert_eq!(d.events().count(), 4);
    assert_eq!(d.events().last().unwrap().text, "{\\an5}Karaoke");
}

#[test]
fn dialog_params_and_cancel_rollback() {
    let d = doc();
    let mut opts = RunOptions::default();
    opts.params = HashMap::from([("ms".to_string(), "250".to_string())]);
    let s = Script::load(&fx("test_macro.lua"), d.clone(), &opts).unwrap();
    s.run("Dialog", &opts).unwrap();
    assert_eq!(d.borrow().events().next().unwrap().start, 1250);
    let before = d.borrow().entries.len();
    let r = s.run("Cancel", &RunOptions::default()).unwrap();
    assert!(r.cancelled);
    assert_eq!(d.borrow().entries.len(), before, "cancel must roll back deletions");
}

#[test]
fn error_rolls_back_and_reports() {
    let d = doc();
    let s = Script::load(&fx("test_macro.lua"), d.clone(), &RunOptions::default()).unwrap();
    let e = s.run("Boom", &RunOptions::default()).unwrap_err();
    assert!(e.to_string().contains("kaboom"));
    assert!(matches!(d.borrow().entries[0], Entry::Info { .. }));
}
