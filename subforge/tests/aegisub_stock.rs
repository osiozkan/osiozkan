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
