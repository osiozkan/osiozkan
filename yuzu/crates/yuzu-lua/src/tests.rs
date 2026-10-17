use super::*;
use asscore::{Event, Script};

struct TestHost;
impl Host for TestHost {
    fn log(&self, _l: i32, m: &str) { eprintln!("[lua] {}", m); }
    fn dialog(&self, spec: &DialogSpec) -> DialogResult {
        DialogResult::Ok { button: spec.ok.clone().unwrap_or_else(|| spec.buttons[0].clone()), values: default_dialog_values(spec) }
    }
}

fn run(src: &str, doc: &mut Script, sel: &[usize]) -> Result<Outcome, String> {
    let dir = std::env::temp_dir().join(format!("yuzu-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let n = format!("t{}.lua", src.len() + sel.len());
    let p = dir.join(n);
    std::fs::write(&p, src).unwrap();
    let s = LuaScript::load(&p, Rc::new(TestHost), &[])?;
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
    let o = run(r#"
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
