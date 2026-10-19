//! `cargo run -p subforge-lua --example probe -- script.lua [macro-index] [file.ass]`
//! Loads a script, lists its macros/filters and optionally runs one headlessly.
use std::cell::RefCell;
use std::rc::Rc;
use subforge_core::ass::Script;
use subforge_lua::{Doc, Headless, LuaScript, SharedHost};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host: SharedHost = Rc::new(RefCell::new(Headless::default()));
    let s = LuaScript::load(std::path::Path::new(&args[0]), host, &[]);
    if let Some(e) = &s.load_error {
        println!("LOAD ERROR: {e}");
        return;
    }
    println!("{} | {} | {}", s.name, s.author, s.version);
    for (i, m) in s.macros.iter().enumerate() {
        println!("macro {i}: {} -- {}", m.name, m.description);
    }
    for (i, f) in s.filters.iter().enumerate() {
        println!("filter {i}: {}", f.name);
    }
    if let Some(i) = args.get(1).and_then(|a| a.parse::<usize>().ok()) {
        let src = args.get(2).map(|p| std::fs::read_to_string(p).unwrap()).unwrap_or_default();
        let doc = Rc::new(RefCell::new(Doc::from_script(&Script::parse(&src))));
        let n = doc.borrow().counts().2;
        let sel: Vec<usize> = (0..n).map(|e| doc.borrow().event_to_flat(e)).collect();
        let active = sel.first().copied().unwrap_or(1);
        match s.run_macro(i, doc.clone(), &sel, active) {
            Ok(o) => println!("ok {o:?}"),
            Err(e) => println!("RUN ERROR: {e}"),
        }
        println!("{}", doc.borrow().to_script().to_ass());
    }
}
