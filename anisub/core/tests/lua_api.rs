use anisub_core::ass::Script;
use anisub_core::lua::{HostConfig, LuaScript};

fn run(src: &str, subs: &str) -> (Script, String) {
    let dir = std::env::temp_dir().join(format!("anisub-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("t{}.lua", src.len()));
    std::fs::write(&p, src).unwrap();
    let s = LuaScript::load(&p, &HostConfig::default()).unwrap();
    let mut script = Script::parse(subs);
    let r = s.run(0, &mut script, &[], 0).unwrap();
    (script, r.log)
}

const SUBS: &str = "[Script Info]\nPlayResX: 640\n\n[V4+ Styles]\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,hello\n";

#[test]
fn insert_delete_and_len() {
    let (s, log) = run(r#"
        aegisub.register_macro("m", "", function(subs)
            local n = #subs
            local l = subs[n]
            l.text = "changed"
            subs.insert(n + 1, l)
            subs.delete(n)
            aegisub.log("n=%d", n)
        end)"#, SUBS);
    assert_eq!(log, "n=3");
    assert_eq!(s.events().count(), 1);
    assert_eq!(s.events().next().unwrap().text, "changed");
}

#[test]
fn bit_and_ffi_available() {
    let (_, log) = run(r#"aegisub.register_macro("m","",function() aegisub.log("%d", bit.band(7, 3)) end)"#, SUBS);
    assert_eq!(log, "3");
}

#[test]
fn cancel_is_not_an_error() {
    let (_, _) = run(r#"aegisub.register_macro("m","",function() aegisub.cancel() end)"#, SUBS);
}

#[test]
fn re_module() {
    let (_, log) = run(r#"
        local re = require("aegisub.re")
        aegisub.register_macro("x","",function()
            local r = re.sub("abc 123", "(\\d+)", "<\\1>")
            local m = re.find("foo bar", "(b)(a)")
            aegisub.log("%s|%d|%s", r, #m, m[2].str)
        end)
    "#, SUBS);
    assert_eq!(log, "abc <123>|3|b");
}
