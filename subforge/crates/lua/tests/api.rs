//! Behavioural tests for the Automation 4 API surface (checked against Aegisub's own Lua sources).
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use subforge_core::ass::Script;
use subforge_lua::{Doc, Headless, LuaScript, SharedHost};

fn write_script(name: &str, code: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("subforge-api-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, code).unwrap();
    p
}

fn sample() -> Script {
    Script::parse("[Script Info]\nPlayResX: 1280\nPlayResY: 720\nCustom Key: some value\n; a comment\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,25,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Hello world\nComment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,note\n")
}

fn run_with(host: Headless, code: &str) -> (Rc<RefCell<Doc>>, Rc<RefCell<Headless>>) {
    let host = Rc::new(RefCell::new(host));
    let shared: SharedHost = host.clone();
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let p = write_script(&format!("t{}.lua", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)), code);
    let s = LuaScript::load(&p, shared, &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    let doc = Rc::new(RefCell::new(Doc::from_script(&sample())));
    let n = doc.borrow().lines.len();
    let sel = vec![n - 1];
    s.run_macro(0, doc.clone(), &sel, n - 1).unwrap_or_else(|e| panic!("{e}"));
    (doc, host)
}

fn run(code: &str) -> (Rc<RefCell<Doc>>, Rc<RefCell<Headless>>) {
    run_with(Headless { quiet: true, ..Default::default() }, code)
}

fn body(code: &str) -> String {
    format!("script_name='t'\naegisub.register_macro('t','',function(subs, sel, act)\n{code}\nend)")
}

#[test]
fn re_module_matches_aegisub_semantics() {
    run(&body(
        r#"
local re = require 'aegisub.re'
local f = re.find("a1 b22 c333", "\\d+")
assert(#f == 3 and f[2].str == "22" and f[2].first == 5 and f[2].last == 6)
assert(re.find("abc", "\\d") == nil)
local m = re.match("abc123def", "([a-z]+)(\\d+)")
assert(m[1].str == "abc123" and m[2].str == "abc" and m[3].str == "123" and m[3].first == 4 and m[3].last == 6)
assert(re.sub("hello world", "o", "0") == "hell0 w0rld")
assert(re.sub("hello world", "o", "0", 1) == "hell0 world")
assert(re.sub("abc", "(b)", "[$1]") == "a[b]c")
assert(re.sub("abc", "(b)", "[\\1]") == "a[b]c")
assert(re.sub("abc", "B", "x", re.ICASE) == "axc")
assert(re.sub("abc", "(b)", function(s) return s:upper() end) == "aBc")
local parts = re.split("a,b,,c", ",")
assert(#parts == 4 and parts[3] == "")
assert(#re.split("a,b,,c", ",", true) == 3)
local n = 0
for s, a, b in re.gfind("x1y22", "\\d+") do n = n + 1 end
assert(n == 2)
local r = re.compile("(\\w)(\\w)")
assert(r:sub("abcd", "$2$1") == "badc")
assert(re.sub("ab", "(?<x>a)", "\\U$1") == "Ab")
assert(not pcall(re.find, "x", ""))
assert(unicode.len("çğş") == 3 and re.find("çğş", "ğ")[1].first == 3)
"#,
    ));
}

#[test]
fn unicode_module_has_aegisub_names() {
    run(&body(
        r#"
local u = require 'aegisub.unicode'
assert(u == unicode or u.len)
assert(u.to_upper_case("abç") == "ABÇ" and u.to_lower_case("ABÇ") == "abç" and u.to_fold_case("AB") == "ab")
assert(u.charwidth("é", 1) == 2 and u.charwidth("a", 1) == 1)
local out = {}
for c, i in u.chars("aé") do out[#out + 1] = c .. i end
assert(table.concat(out, ",") == "a1,é2")
assert(u.codepoint("é") == 233)
"#,
    ));
}

#[test]
fn clipboard_progress_and_log() {
    let (_, host) = run(&body(
        r#"
local cb = require 'aegisub.clipboard'
assert(cb.set("hi") ~= nil)
assert(aegisub.clipboard.get() == "hi")
aegisub.progress.task("step %d of %s", 2, "x")
aegisub.log(0, "100% sure\n")
aegisub.log("fmt %s\n", "ok")
aegisub.debug.out("only 100%\n")
"#,
    ));
    let h = host.borrow();
    assert!(h.log_buf.iter().any(|l| l == "100% sure\n"), "{:?}", h.log_buf);
    assert!(h.log_buf.iter().any(|l| l == "fmt ok\n"));
    assert!(h.log_buf.iter().any(|l| l == "only 100%\n"));
}

#[test]
fn line_fields_follow_aegisub() {
    let (doc, _) = run(&body(
        r#"
local n = #subs
local l = subs[n]
assert(l.class == "dialogue" and l.comment == true and l.section == "Events")
assert(type(l.extra) == "table")
l.start_time = 1500.9
l.end_time = "2000"
l.layer = 2.7
subs[n] = l
local info
for i = 1, n do
  local x = subs[i]
  if x.class == "info" and x.key == "Custom Key" then info = x end
end
assert(info and info.value == "some value")
assert(subs.script_resolution() == 1280)
local s = subs[3 + 0] -- first style after the info lines
"#,
    ));
    let sc = doc.borrow().to_script();
    let e = sc.events.last().unwrap();
    assert_eq!((e.start, e.end, e.layer), (1500, 2000, 2));
    assert!(sc.info.iter().any(|(k, _)| k == ";"));
}

#[test]
fn require_module_next_to_script_and_include() {
    let dir = std::env::temp_dir().join(format!("subforge-api-mod-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("mymod.lua"), "return { v = 41 }").unwrap();
    std::fs::write(dir.join("lib/sub.lua"), "return { v = 1 }").unwrap();
    std::fs::write(dir.join("inc.lua"), "INC_LOADED = (INC_LOADED or 0) + 1").unwrap();
    std::fs::write(
        dir.join("main.lua"),
        body("assert(require('mymod').v == 41); assert(require('lib.sub').v == 1); include('inc.lua'); include('inc.lua'); assert(INC_LOADED == 1)"),
    )
    .unwrap();
    let host: SharedHost = Rc::new(RefCell::new(Headless { quiet: true, ..Default::default() }));
    let s = LuaScript::load(&dir.join("main.lua"), host, &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    let doc = Rc::new(RefCell::new(Doc::from_script(&sample())));
    s.run_macro(0, doc, &[1], 1).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dialog_extras_and_frame_functions() {
    let mut h = Headless { quiet: true, fps: Some(25.0), ..Default::default() };
    h.answers.insert("c".into(), "#112233".into());
    run_with(
        h,
        &body(
            r##"
assert(aegisub.frame_from_ms(1000) == 25 and aegisub.ms_from_frame(25) == 1000)
local b, r = aegisub.dialog.display({
  {class="color", name="c", value="#FFFFFF", x=0, y=0, hint="pick"},
  {class="textbox", name="t", value="multi\nline", x=0, y=1, width=2, height=3},
  {class="floatedit", name="f", value=1.5, x=0, y=4},
  {class="label", label="hello", x=0, y=5},
}, {"OK", "Cancel"}, {ok="OK", cancel="Cancel"})
assert(b == "OK" and r.c == "#112233" and r.t == "multi\nline" and r.f == 1.5)
"##,
        ),
    );
}
