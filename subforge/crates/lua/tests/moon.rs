//! MoonScript (.moon) support and real Aegisub autoload macros.
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use subforge_core::ass::{Event, Script};
use subforge_lua::{Doc, Headless, LuaScript, SharedHost};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aegisub-autoload").join(name)
}

fn host() -> SharedHost {
    Rc::new(RefCell::new(Headless { quiet: true, ..Default::default() }))
}

fn doc_with(events: Vec<Event>) -> Rc<RefCell<Doc>> {
    let mut sc = Script::new();
    sc.events = events;
    Rc::new(RefCell::new(Doc::from_script(&sc)))
}

#[test]
fn real_moonscript_macro_select_overlaps() {
    let s = LuaScript::load(&fixture("select-overlaps.moon"), host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    assert_eq!(s.name, "Select overlaps");
    let evs = [(0, 3000), (1000, 2000), (3000, 4000), (3500, 5000)].iter().map(|&(a, b)| Event::new(a, b, "Default", "x")).collect();
    let doc = doc_with(evs);
    let first = doc.borrow().event_to_flat(0);
    let out = s.run_macro(0, doc, &[first], first).unwrap();
    assert_eq!(out.selection, Some(vec![first + 1, first + 3]));
}

#[test]
fn moon_modules_and_lpeg_are_available() {
    let dir = std::env::temp_dir().join(format!("subforge-moon-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.moon"), "class Counter\n  new: => @n = 0\n  bump: => @n += 1\n{ :Counter }\n").unwrap();
    std::fs::write(
        dir.join("main.moon"),
        "import Counter from require 'helper'\nexport script_name = 'moon main'\naegisub.register_macro 'm', '', (subs, sel) ->\n  c = Counter!\n  c\\bump!\n  c\\bump!\n  assert c.n == 2\n  lpeg = require 'lpeg'\n  assert (lpeg.match lpeg.C(lpeg.R('09')^1), '123x') == '123'\n  assert require('moonscript').to_lua\n",
    )
    .unwrap();
    let s = LuaScript::load(&dir.join("main.moon"), host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    assert_eq!(s.name, "moon main");
    s.run_macro(0, doc_with(vec![]), &[], 1).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn moon_syntax_error_is_a_load_error() {
    let dir = std::env::temp_dir().join(format!("subforge-moon-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bad.moon"), "x = = 1\n").unwrap();
    let s = LuaScript::load(&dir.join("bad.moon"), host(), &[]);
    assert!(s.load_error.unwrap().contains("MoonScript"));
}

#[test]
fn real_strip_tags_macro() {
    let s = LuaScript::load(&fixture("strip-tags.lua"), host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    let doc = doc_with(vec![Event::new(0, 1000, "Default", "{\\b1}Hello{\\i1} world")]);
    let first = doc.borrow().event_to_flat(0);
    s.run_macro(0, doc.clone(), &[first], first).unwrap();
    assert_eq!(doc.borrow().to_script().events[0].text, "Hello world");
}
