//! Export filters (`register_filter`) with config dialogs, and legacy global-style scripts.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use subforge_core::ass::{Event, Script};
use subforge_lua::{Doc, DialogValue, Headless, LuaScript, SharedHost};

fn load(name: &str, code: &str) -> LuaScript {
    let dir = std::env::temp_dir().join(format!("subforge-filter-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, code).unwrap();
    let host: SharedHost = Rc::new(RefCell::new(Headless { quiet: true, ..Default::default() }));
    let s = LuaScript::load(&p, host, &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    s
}

fn doc() -> Rc<RefCell<Doc>> {
    let mut sc = Script::new();
    sc.events = vec![Event::new(1000, 2000, "Default", "hello")];
    Rc::new(RefCell::new(Doc::from_script(&sc)))
}

#[test]
fn filter_with_config_dialog() {
    let s = load(
        "f1.lua",
        r#"
script_name = "Suffix filter"
script_description = "adds a suffix"
function cfg(subs, old)
  return { {class="label", label="Suffix:", x=0, y=0}, {class="edit", name="suffix", value=old.suffix or "!", x=1, y=0} }
end
function proc(subs, config)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then l.text = l.text .. config.suffix; subs[i] = l end
  end
end
aegisub.register_filter("Suffix", "adds suffix", 2000, proc, cfg)
"#,
    );
    assert_eq!(s.filters.len(), 1);
    assert_eq!(s.filters[0].priority, 2000);
    let d = doc();
    let dlg = s.filter_dialog(0, d.clone()).unwrap().expect("dialog");
    let f = dlg.fields.iter().find(|f| f.name == "suffix").unwrap();
    assert_eq!(f.value, Some(DialogValue::Str("!".into())));
    let mut cfg = HashMap::new();
    cfg.insert("suffix".to_string(), DialogValue::Str("?!".into()));
    s.run_filter(0, d.clone(), cfg).unwrap();
    assert_eq!(d.borrow().to_script().events[0].text, "hello?!");
}

#[test]
fn dependency_control_stub() {
    let dir = std::env::temp_dir().join(format!("subforge-depctrl-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("l0")).unwrap();
    std::fs::write(
        dir.join("l0/Helper.moon"),
        "DependencyControl = require 'l0.DependencyControl'\nversion = DependencyControl{ moduleName: 'l0.Helper', version: '1.2.3' }\nHelper = { twice: (x) -> x * 2 }\nversion\\register Helper\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("dc.lua"),
        r#"
script_name = "dc"
script_version = "0.1.0"
local DependencyControl = require("l0.DependencyControl")
local depRec = DependencyControl{ feed = "x", { "l0.Helper", version = "1.0.0" }, { "no.such.module", optional = true } }
local Helper, Missing = depRec:requireModules()
assert(Helper.twice(4) == 8 and Missing == nil)
assert(Helper.version.versionString == "1.2.3" and depRec:checkVersion("0.0.9") and not depRec:checkVersion("0.2.0"))
depRec:registerMacros({ { "One", "first", function() end }, { "Two", "second", function() end } }, true)
depRec:registerMacro(function() end)
"#,
    )
    .unwrap();
    let host: SharedHost = Rc::new(RefCell::new(Headless { quiet: true, ..Default::default() }));
    let s = LuaScript::load(&dir.join("dc.lua"), host, &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    let names: Vec<_> = s.macros.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["dc/One", "dc/Two", "dc"]);
}

#[test]
fn validate_and_active_line_arguments() {
    let s = load(
        "f2.lua",
        r#"
script_name = "args"
local seen
aegisub.register_macro("args", "", function(subs, sel, active)
  assert(type(sel) == "table" and #sel == 1 and type(active) == "number")
  return {sel[1]}, active
end, function(subs, sel, active) return #sel > 0 end)
"#,
    );
    let d = doc();
    let i = d.borrow().event_to_flat(0);
    assert!(s.validate_macro(0, d.clone(), &[i], i));
    assert!(!s.validate_macro(0, d.clone(), &[], i));
    let out = s.run_macro(0, d, &[i], i).unwrap();
    assert_eq!(out.selection, Some(vec![i]));
    assert_eq!(out.active, Some(i));
}
