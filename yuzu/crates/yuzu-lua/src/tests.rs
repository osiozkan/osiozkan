use super::*;
use asscore::{Event, Script};

#[derive(Default)]
struct TestHost {
    user_dir: Option<PathBuf>,
}
impl Host for TestHost {
    fn log(&self, _l: i32, m: &str) { eprintln!("[lua] {}", m); }
    fn dialog(&self, spec: &DialogSpec) -> DialogResult {
        DialogResult::Ok { button: spec.ok.clone().unwrap_or_else(|| spec.buttons[0].clone()), values: default_dialog_values(spec) }
    }
    fn project(&self) -> ProjectInfo {
        ProjectInfo { user_dir: self.user_dir.clone(), ..Default::default() }
    }
}

fn script_file(src: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("yuzu-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("t{}.lua", N.fetch_add(1, Ordering::SeqCst)));
    std::fs::write(&p, src).unwrap();
    p
}

fn load(src: &str) -> Result<LuaScript, String> {
    LuaScript::load(&script_file(src), Rc::new(TestHost::default()), &[])
}

fn run(src: &str, doc: &mut Script, sel: &[usize]) -> Result<Outcome, String> {
    let s = load(src)?;
    assert!(!s.macros().is_empty());
    s.run_macro(0, doc, sel, None)
}

fn sample() -> Script {
    let mut s = Script::new();
    s.events.push(Event::new(1000, 2000, "Default", "Hello"));
    s.events.push(Event::new(2000, 3000, "Default", "{\\k20}ka{\\k30}ra"));
    s
}

#[test]
fn macro_edits_text() {
    let mut d = sample();
    let _o = run(r#"
script_name = "t"
function m(subs, sel)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper()
    l.start_time = l.start_time + 100
    subs[i] = l
  end
  return sel
end
aegisub.register_macro("up", "d", m)
"#, &mut d, &[1]).unwrap_or_else(|e| panic!("{e}"));
    // selection index 1 (event) is the lua line of event 0 + offset; handled via mapping, so pass event idx
    assert!(d.events.iter().any(|e| e.text.contains("HELLO")) || d.events.iter().any(|e| e.text.contains("RA")));
}

#[test]
fn insert_delete_and_len() {
    let mut d = sample();
    run(r#"
function m(subs)
  local n = #subs
  local l = subs[n]
  l.text = "added"
  subs.append(l)
  subs.insert(n + 1, l)
  subs.delete(n)
  aegisub.debug.out("n=%d", #subs)
end
aegisub.register_macro("x", "d", m)
"#, &mut d, &[]).unwrap();
    assert_eq!(d.events.len(), 3);
}

#[test]
fn karaskel_parses() {
    let mut d = sample();
    run(r#"
local karaskel = require "karaskel"
function m(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  assert(meta.res_x == 1920, "res_x=" .. tostring(meta.res_x))
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" and l.text:find("\\k") then
      karaskel.preproc_line(subs, meta, styles, l)
      assert(l.kara.n == 2, "n=" .. l.kara.n)
      assert(l.kara[1].duration == 200 and l.kara[2].start_time == 200)
      assert(l.kara[1].text_stripped == "ka")
      assert(l.width > 0 and l.kara[2].left > l.kara[1].left)
    end
  end
end
aegisub.register_macro("k", "d", m)
"#, &mut d, &[]).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn util_and_re_and_unicode() {
    let mut d = sample();
    run(r#"
local util = aegisub.util
local unicode = require "unicode"
require "utils"
function m(subs)
  assert(util.ass_color(255, 0, 0) == "&H0000FF&")
  local r, g, b = util.extract_color("&H0000FF&")
  assert(r == 255 and g == 0 and b == 0)
  assert(unicode.len("çağrı") == 5)
  assert(unicode.codepoint("ç") == 231)
  local out = aegisub.re.sub("abc123", "\\d+", "X")
  assert(out == "abcX", out)
  local m = aegisub.re.match("foo=bar", "(\\w+)=(\\w+)")
  assert(m[2].str == "foo" and m[3].str == "bar")
  assert(("  hi "):trim() == "hi")
end
aegisub.register_macro("u", "d", m)
"#, &mut d, &[]).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dialog_defaults_and_cancel() {
    let mut d = sample();
    run(r#"
function m(subs)
  local btn, res = aegisub.dialog.display({
    {class="label", label="x", x=0, y=0},
    {class="intedit", name="n", value=7, x=0, y=1},
    {class="checkbox", name="c", value=true, x=0, y=2, label="c"},
    {class="dropdown", name="d", items={"a","b"}, value="b", x=0, y=3},
  }, {"Go","Stop"}, {ok="Go", cancel="Stop"})
  assert(btn == "Go")
  assert(res.n == 7 and res.c == true and res.d == "b")
  aegisub.cancel()
end
aegisub.register_macro("d", "d", m)
"#, &mut d, &[]).map(|o| assert!(o.cancelled)).unwrap();
}

#[test]
fn errors_carry_a_stack_traceback() {
    let mut d = sample();
    let e = run(r#"
local function inner(line) local t = line.text:upper(); return t end
local function outer(subs) local r = inner(nil); return r end
aegisub.register_macro("boom", "d", function(subs) outer(subs) end)
"#, &mut d, &[]).unwrap_err();
    assert!(e.contains("attempt to index") && e.contains("stack traceback:") && e.contains("inner") && e.contains("outer"), "{}", e);
    assert!(!e.contains("xpcall") && !e.contains("builtin"), "the call machinery is not shown: {}", e);
    // errors while loading too
    let e = load("local x = 1\nerror('yükleme hatası')\n").err().unwrap();
    assert!(e.contains("yükleme hatası") && e.contains("main chunk"), "{}", e);
    // cancelling is not an error
    let o = run("aegisub.register_macro('c', 'd', function() aegisub.cancel() end)", &mut d, &[]).unwrap();
    assert!(o.cancelled);
}

#[test]
fn validation_and_checked_macros() {
    let mut d = sample();
    d.events[0].comment = true;
    let s = load(r#"
aegisub.register_macro("selection", "d", function() end, function(subs, sel) return #sel > 0 end)
aegisub.register_macro("comment?", "d", function() end, nil, function(subs, sel, act) return act > 0 and subs[act].comment end)
aegisub.register_macro("broken", "d", function() end, function() error("x") end)
aegisub.register_macro("plain", "d", function() end)
"#).unwrap();
    let m = s.macros();
    assert!(m[0].has_validate && !m[0].has_active && m[1].has_active && !m[3].has_validate);
    let st = s.macro_states(&d, &[], None);
    assert_eq!(st.iter().map(|x| (x.enabled, x.checked)).collect::<Vec<_>>(), vec![(false, None), (true, Some(false)), (false, None), (true, None)]);
    let st = s.macro_states(&d, &[0], Some(0));
    assert_eq!(st.iter().map(|x| (x.enabled, x.checked)).collect::<Vec<_>>(), vec![(true, None), (true, Some(true)), (false, None), (true, None)]);
    assert!(s.validate(0, &d, &[1], Some(1)));
}

#[test]
fn extradata_is_line_extra() {
    let mut d = sample();
    d.extradata = vec![asscore::Extra { id: 1, key: "a-mo".into(), value: "e{\"x\"#3A1}".into() }, asscore::Extra { id: 5, key: "unused".into(), value: "ex".into() }];
    d.events[0].text = "{=1}Hello".into();
    run(r#"
function m(subs)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" and l.text == "Hello" then
      assert(l.extra["a-mo"] == '{"x":1}', tostring(l.extra["a-mo"]))
      l.extra["a-mo"] = nil
      l.extra.yuzu = "a, b: c|d#"
      subs[i] = l
    elseif l.class == "dialogue" then
      assert(next(l.extra) == nil)
    end
  end
end
aegisub.register_macro("x", "d", m)
"#, &mut d, &[]).unwrap_or_else(|e| panic!("{e}"));
    let (ids, text) = asscore::extradata::split_marker(&d.events[0].text);
    assert_eq!((ids.len(), text), (1, "Hello"));
    assert_eq!(asscore::extradata::line_values(&d.extradata, &d.events[0].text), vec![("yuzu".to_string(), "a, b: c|d#".to_string())]);
    assert_eq!(d.extradata.len(), 1, "entries no line uses are dropped");
    // many special characters: written uuencoded, as Aegisub picks the shorter form
    assert!(d.extradata[0].value.starts_with('u'), "{:?}", d.extradata[0]);
}

#[test]
fn index_assignment_rules() {
    let mut d = sample();
    run(r#"
function m(subs)
  local n = #subs
  local l = subs[n]
  l.text = "sona"
  subs[0] = l
  l.text = "araya"
  subs[-n] = l
end
aegisub.register_macro("x", "d", m)
"#, &mut d, &[]).unwrap_or_else(|e| panic!("{e}"));
    let t: Vec<&str> = d.events.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(t, vec!["Hello", "araya", "{\\k20}ka{\\k30}ra", "sona"]);
}

#[test]
fn dependency_control_json_and_config() {
    let user = std::env::temp_dir().join(format!("yuzu-test-user-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user);
    let src = r#"
local DependencyControl = require("l0.DependencyControl")
local depctrl = DependencyControl{
  name = "Uyum Testi", version = "1.2.3", description = "d", author = "yuzu", namespace = "yuzu.uyum",
  { "aegisub.util", "json", {"a-mo.Bulunmaz", version = "1.0.0", optional = true} },
}
local util, json, missing = depctrl:requireModules()
assert(util and util.copy, "util")
assert(json and json.decode, "json")
assert(missing == nil)
assert(depctrl:checkVersion("1.2.0") and not depctrl:checkVersion("1.3"))
local t = json.decode('{"a":[1,2,{"b":"ç\\u011f"}],"c":true,"d":null, "e": -1.5e2}')
assert(t.a[3].b == "çğ" and t.c == true and t.d == nil and t.e == -150, "decode")
local enc = json.encode({x = 1, y = {"a", "b"}, z = "q\"\n"})
assert(enc == '{"x":1,"y":["a","b"],"z":"q\\"\\n"}', enc)
assert(not pcall(json.decode, '{"a":}'))
local cfg = depctrl:getConfigHandler({ size = 5, name = "x" })
assert(cfg.c.size == 5 and cfg.c.name == "x")
cfg.c.size = 7
assert(cfg:write())
local cfg2 = depctrl:getConfigHandler({ size = 5, extra = true })
assert(cfg2.c.size == 7 and cfg2.c.extra == true, "persisted: " .. tostring(cfg2.c.size))
depctrl:registerMacros({ {"Bir", "d", function() end}, {"İki", "d", function() end} })
depctrl:registerMacro(function() end)
"#;
    let s = LuaScript::load(&script_file(src), Rc::new(TestHost { user_dir: Some(user.clone()) }), &[]).unwrap_or_else(|e| panic!("{e}"));
    let names: Vec<String> = s.macros().into_iter().map(|m| m.name).collect();
    assert_eq!(names, vec!["Uyum Testi/Bir", "Uyum Testi/İki", "Uyum Testi"]);
    assert!(user.join("config/yuzu.uyum.json").exists());
    // a module that is missing and not optional is an error that names it
    let e = load(r#"local D = require("l0.DependencyControl"); local d = D{ {"a-mo.LineCollection"} }; d:requireModules()"#).err().unwrap();
    assert!(e.contains("a-mo.LineCollection"), "{}", e);
    let _ = std::fs::remove_dir_all(&user);
}

#[test]
fn luajit_libraries_are_there() {
    let mut d = sample();
    run(r#"
local ffi = require "ffi"
local bit = require "bit"
ffi.cdef[[ int abs(int x); ]]
function m()
  assert(ffi.C.abs(-3) == 3)
  assert(bit.band(12, 10) == 8 and bit.lshift(1, 4) == 16)
  assert(type(jit) == "table" and jit.version:find("LuaJIT"))
  local buf = ffi.new("uint8_t[4]", {1, 2, 3, 4})
  assert(buf[2] == 3)
end
aegisub.register_macro("x", "d", m)
"#, &mut d, &[]).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_typical_aegisub_3_script_runs() {
    let mut d = sample();
    let src = r#"
script_name = "Bulanıklık ekle"
script_description = "Seçili satırlara \\be1 ekler"
script_author = "test"
script_version = "1.0"
local re = require "aegisub.re"
local unicode = require "aegisub.unicode"
local clipboard = require "aegisub.clipboard"
local util = require "aegisub.util"

local function add_blur(subs, sel, act)
  aegisub.progress.title("Bulanıklık")
  for n, i in ipairs(sel) do
    aegisub.progress.set(n / #sel * 100)
    if aegisub.progress.is_cancelled() then aegisub.cancel() end
    local line = subs[i]
    local copy = util.deep_copy(line)
    if line.text:match("^{") then
      line.text = line.text:gsub("^{", "{\\be1", 1)
    else
      line.text = "{\\be1}" .. line.text
    end
    assert(copy.text ~= line.text)
    subs[i] = line
  end
  aegisub.debug.out(4, "%d satır\n", #sel)
  aegisub.set_status_text("bitti")
  aegisub.set_undo_point(script_name)
  assert(aegisub.file_name() == nil)
  assert(unicode.len("ışık") == 4)
  assert(type(clipboard.get) == "function")
  assert(util.trim("  a b  ") == "a b")
  local h, t = util.headtail("bir iki  üç")
  assert(h == "bir" and t == "iki  üç")
  local w = {}
  for x in util.words("bir iki üç") do w[#w + 1] = x end
  assert(#w == 3 and w[3] == "üç")
  local m = re.match("abc123", "([a-z]+)(\\d+)")
  assert(m[3].str == "123")
  return sel
end

aegisub.register_macro(script_name, script_description, add_blur, function(subs, sel) return #sel > 0 end)
"#;
    let s = load(src).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((s.script_name.as_str(), s.script_version.as_str()), ("Bulanıklık ekle", "1.0"));
    assert!(!s.validate(0, &d, &[], None));
    let o = s.run_macro(0, &mut d, &[0, 1], Some(0)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(o.selection, Some(vec![0, 1]));
    assert_eq!(d.events[0].text, "{\\be1}Hello");
    assert_eq!(d.events[1].text, "{\\be1\\k20}ka{\\k30}ra");
}

#[test]
fn moonscript_main_script_and_module() {
    let dir = std::env::temp_dir().join(format!("yuzu-moon-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.moon"), "{ shout: (s) -> s\\upper! .. \"!\", twice: (n) -> n * 2 }\n").unwrap();
    let main = dir.join("m.moon");
    std::fs::write(&main, "script_name = \"Moon\"\nh = require \"helper\"\naegisub.register_macro \"shout\", \"d\", (subs, sel) ->\n  for i in *sel\n    l = subs[i]\n    l.text = h.shout l.text\n    subs[i] = l\n").unwrap();
    let s = LuaScript::load(&main, Rc::new(TestHost::default()), &[]).expect("moon script loads");
    assert_eq!(s.macros().len(), 1);
    let mut doc = sample();
    s.run_macro(0, &mut doc, &[0], None).unwrap();
    assert_eq!(doc.events[0].text, "HELLO!");
}
