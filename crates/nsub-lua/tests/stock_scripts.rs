//! Runs Aegisub's own stock automation scripts against our runtime.
//! Set AEGISUB_AUTOMATION to a checkout's `automation/` directory to enable (skipped otherwise).

use nsub_core::Script;
use nsub_lua::{Automation, Options};
use std::path::PathBuf;

fn dir() -> Option<PathBuf> {
    std::env::var("AEGISUB_AUTOMATION").ok().map(PathBuf::from).filter(|p| p.join("autoload").is_dir())
}

fn fixture() -> Script {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kara.ass");
    Script::parse(&std::fs::read_to_string(p).unwrap())
}

fn run(script: &str, macro_name: &str) -> Option<Script> {
    let d = dir()?;
    let mut a = Automation::new(Options { echo_log: false, ..Default::default() }).unwrap();
    let m = a.load_script(&d.join("autoload").join(script)).unwrap();
    let id = m.iter().find(|m| m.name == macro_name).unwrap_or_else(|| panic!("macro {macro_name} not found")).id;
    let mut s = fixture();
    let first = Automation::flat_index_of_event(&s, 0);
    let sel: Vec<usize> = (first..first + s.events.len()).collect();
    a.run(id, &mut s, &sel, first).unwrap();
    Some(s)
}

#[test]
fn strip_tags() {
    let Some(s) = run("strip-tags.lua", "Strip tags") else { return };
    assert_eq!(s.events[3].text, "  Merhaba dünya  <<test>> x");
}

#[test]
fn edgeblur() {
    let Some(s) = run("macro-1-edgeblur.lua", "Add edgeblur") else { return };
    assert!(s.events[3].text.starts_with("{\\be1}"));
}

#[test]
fn clean_tags_keeps_content() {
    let Some(s) = run("cleantags-autoload.lua", "Clean Tags") else { return };
    assert!(s.events[3].text.contains("Merhaba"));
}

#[test]
fn kara_templater_generates_syllable_lines() {
    let Some(s) = run("kara-templater.lua", "Apply karaoke template") else { return };
    let fx: Vec<_> = s.events.iter().filter(|e| e.effect == "fx").collect();
    assert_eq!(fx.len(), 5, "one line per syllable");
    assert!(fx[0].text.contains("\\pos(") && fx[0].text.ends_with("ta"));
    // syllables are laid out left to right
    let x = |t: &str| t.split("\\pos(").nth(1).unwrap().split(',').next().unwrap().parse::<f64>().unwrap();
    assert!(x(&fx[0].text) < x(&fx[1].text) && x(&fx[1].text) < x(&fx[2].text));
}
