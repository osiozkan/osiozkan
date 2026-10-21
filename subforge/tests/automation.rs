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

#[test]
fn dialog_hook_receives_spec_and_returns_values() {
    use std::sync::{Arc, Mutex};
    let d = doc();
    let seen: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let seen2 = seen.clone();
    let mut opts = RunOptions::default();
    opts.dialog_hook = Some(Arc::new(move |spec| {
        *seen2.lock().unwrap() = Some(spec);
        Some(serde_json::json!({"button": "Apply", "values": {"ms": 500, "tag": "zz"}}))
    }));
    let s = Script::load(&fx("test_macro.lua"), d.clone(), &opts).unwrap();
    s.run("Dialog", &opts).unwrap();
    let spec = seen.lock().unwrap().clone().unwrap();
    assert_eq!(spec["controls"][1]["name"], "ms");
    assert_eq!(spec["buttons"], serde_json::json!(["Apply", "Cancel"]));
    assert_eq!(spec["ids"]["ok"], "Apply");
    let e = d.borrow().events().next().unwrap().clone();
    assert_eq!((e.start, e.effect.as_str()), (1500, "zz"));
}

#[test]
fn dialog_hook_cancel_makes_script_cancel() {
    let d = doc();
    let mut opts = RunOptions::default();
    opts.dialog_hook = Some(std::sync::Arc::new(|_| Some(serde_json::json!({"button": null, "values": {}}))));
    let s = Script::load(&fx("test_macro.lua"), d.clone(), &opts).unwrap();
    let r = s.run("Dialog", &opts).unwrap();
    assert!(r.cancelled);
    assert_eq!(d.borrow().events().next().unwrap().start, 1000);
}

#[test]
fn parsers_never_panic_on_garbage() {
    use subforge::{ass::Document, srt};
    let nasty: Vec<String> = vec![
        "".into(),
        "[".into(),
        "[Events]".into(),
        "[Events]\nDialogue:".into(),
        "[Events]\nFormat: Layer, Start, End, Text\nDialogue: x,y,z,{unclosed".into(),
        "[V4+ Styles]\nStyle: ,,,,".into(),
        "{".repeat(1000),
        "\u{feff}\u{0}\u{1}\u{2}[Script Info]\nTitle".into(),
        "1\n00:00:01,000 --> \n\n2\n--> 00:00:02,000\ntext".into(),
        "WEBVTT\n\n00:01.000 --> 00:02.000 align:start\n<c.yellow>hi</c>".into(),
        (0..300).map(|i| format!("Dialogue: 0,0:00:0{}.00,0:00:01.00,S,,0,0,0,,{{\\k{}}}x\n", i % 10, i)).collect(),
    ];
    for t in nasty {
        let d = Document::parse(&t);
        let _ = d.to_string();
        let d = srt::parse_srt(&t);
        let _ = srt::to_srt(&d);
        let _ = srt::to_vtt(&d);
        let _ = subforge::tags::syllables(&t);
        let _ = subforge::tags::karaoke_data(&t);
        let _ = subforge::karaoke::strip_k(&t);
        let _ = subforge::cleanup::rebalance(&t, 40);
    }
}
