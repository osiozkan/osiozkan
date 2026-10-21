//! Runs Aegisub's *unmodified* stock scripts. Skipped unless AEGISUB_SCRIPTS points at a directory
//! filled by `scripts/fetch-aegisub-scripts.sh`.
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use subforge::ass::Document;
use subforge::automation::{RunOptions, Script};

#[test]
fn stock_kara_templater_runs() {
    let Ok(dir) = std::env::var("AEGISUB_SCRIPTS") else {
        eprintln!("AEGISUB_SCRIPTS not set; skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/karaoke.ass")).unwrap();
    let doc = Rc::new(RefCell::new(Document::parse(&src)));
    let opts = RunOptions { include_dirs: vec![dir.clone()], ..Default::default() };
    let s = Script::load(&dir.join("kara-templater.lua"), doc.clone(), &opts).unwrap();
    let r = s.run("Apply karaoke template", &opts).unwrap();
    assert!(!r.cancelled, "{:?}", r.log);
    let d = doc.borrow();
    let fx: Vec<_> = d.events().filter(|e| e.effect == "fx").collect();
    assert!(fx.len() >= 5, "expected generated fx lines, got {}", fx.len());
    assert!(fx.iter().any(|e| e.text.contains("\\pos(") && e.text.ends_with("ke")));
    // the template/code comment lines must survive
    assert!(d.events().any(|e| e.comment && e.effect.starts_with("template")));
}

fn stock_dir() -> Option<PathBuf> {
    std::env::var("AEGISUB_SCRIPTS").ok().map(PathBuf::from)
}

fn run_stock(file: &str, name: &str, fixture: &str) -> Document {
    let dir = stock_dir().unwrap();
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture)).unwrap();
    let doc = Rc::new(RefCell::new(Document::parse(&src)));
    let opts = RunOptions { include_dirs: vec![dir.clone()], ..Default::default() };
    let s = Script::load(&dir.join(file), doc.clone(), &opts).unwrap();
    let r = s.run(name, &opts).unwrap();
    assert!(!r.cancelled, "{:?}", r.log);
    let d = doc.borrow().clone();
    d
}

#[test]
fn stock_simple_macros() {
    if stock_dir().is_none() {
        return;
    }
    let d = run_stock("macro-1-edgeblur.lua", "Add edgeblur", "typeset.ass");
    assert!(d.events().all(|e| e.text.starts_with("{\\be1}")));
    let d = run_stock("strip-tags.lua", "Strip tags", "typeset.ass");
    assert!(d.events().all(|e| !e.text.contains('{')));
    let d = run_stock("cleantags-autoload.lua", "Clean Tags", "typeset.ass");
    assert_eq!(d.events().count(), 6);
}

/// Bundled copies work without any download or env var.
#[test]
fn builtin_kara_templater_and_macros() {
    use subforge::automation::materialize_builtin;
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/karaoke.ass")).unwrap();
    let doc = Rc::new(RefCell::new(Document::parse(&src)));
    let opts = RunOptions::default();
    let p = materialize_builtin("builtin:kara-templater.lua").unwrap();
    let s = Script::load(&p, doc.clone(), &opts).unwrap();
    let r = s.run("Apply karaoke template", &opts).unwrap();
    assert!(!r.cancelled, "{:?}", r.log);
    let d = doc.borrow();
    let fx: Vec<_> = d.events().filter(|e| e.effect == "fx").collect();
    assert!(fx.len() >= 5, "generated {} fx lines", fx.len());
    assert!(fx.iter().any(|e| e.text.contains("\\pos(") && e.text.ends_with("ke")));
    drop(d);
    for (name, macro_name) in [("macro-1-edgeblur.lua", "Add edgeblur"), ("strip-tags.lua", "Strip tags"), ("cleantags-autoload.lua", "Clean Tags")] {
        let p = materialize_builtin(name).unwrap();
        let doc = Rc::new(RefCell::new(Document::parse(&src)));
        let s = Script::load(&p, doc.clone(), &opts).unwrap();
        s.run(macro_name, &opts).unwrap();
        assert!(doc.borrow().events().count() >= 1);
    }
}
