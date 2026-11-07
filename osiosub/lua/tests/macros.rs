use osiosub_core::ass::AssFile;
use osiosub_lua::{DefaultHost, ScriptRuntime};

const FILE: &str = "[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\k50}ka{\\k50}ra\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,second\n";

fn rt() -> ScriptRuntime { ScriptRuntime::new(Box::new(DefaultHost)).unwrap() }

#[test]
fn macro_edits_and_selection() {
    let mut r = rt();
    let m = r.load_source(r#"
script_name = "t"
local util = require "util"
include("karaskel.lua")
function proc(subs, sel, act)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper()
    l.start_time = l.start_time + 500
    subs[i] = l
  end
  subs.append({class="dialogue", text="added", start_time=0, end_time=100, style="Default", layer=0})
  aegisub.log("n=%d\n", #subs)
  return {sel[1]}
end
aegisub.register_macro("up", "d", proc)
"#, "t.lua").unwrap();
    assert_eq!(m.len(), 1);
    let f = AssFile::parse(FILE).unwrap();
    let res = r.run(0, &f, &[0, 1], 0);
    assert!(res.error.is_none(), "{:?}", res.error);
    let out = res.file.unwrap();
    assert_eq!(out.events.len(), 3);
    assert_eq!(out.events[1].text, "SECOND");
    assert_eq!(out.events[1].start, 3500);
    assert_eq!(out.events[2].text, "added");
    assert_eq!(res.selection, Some(vec![0]));
    assert!(res.log.contains("n=")); 
}

#[test]
fn karaskel_and_utils() {
    let mut r = rt();
    r.load_source(r#"
include("karaskel.lua")
local util = require "util"
function proc(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local l = subs[#subs-1]
  karaskel.preproc_line(subs, meta, styles, l)
  assert(l.kara.n == 2, "kara n " .. tostring(l.kara.n))
  assert(l.kara[1].duration == 500)
  assert(l.width > 0 and l.left and l.x)
  assert(util.ass_color(255,0,0) == "&H0000FF&")
  local r,g,b = util.extract_color("&H0000FF&")
  assert(r==255 and g==0 and b==0)
  assert(re.find("abc123", "(\\d+)")[1].str == "123")
  assert(re.sub("a b", "\\s", "_") == "a_b")
  assert(unicode.len("çğ") == 2)
  assert(utf8.len("çğ") == 2)
  assert(("  x "):trim() == "x")
end
aegisub.register_macro("k", "d", proc)
"#, "k.lua").unwrap();
    let f = AssFile::parse(FILE).unwrap();
    let res = r.run(0, &f, &[0], 0);
    assert!(res.error.is_none(), "{:?}", res.error);
}

#[test]
fn dialog_defaults_and_cancel() {
    let mut r = rt();
    r.load_source(r#"
function a(subs)
  local b, res = aegisub.dialog.display({{class="label",label="x",x=0,y=0},{class="edit",name="v",text="hey",x=0,y=1},{class="checkbox",name="c",label="c",value=true,x=0,y=2}})
  assert(b == "OK" and res.v == "hey" and res.c == true)
  aegisub.cancel()
end
aegisub.register_macro("a","",a)
"#, "d.lua").unwrap();
    let f = AssFile::parse(FILE).unwrap();
    let res = r.run(0, &f, &[], 0);
    assert!(res.cancelled, "{:?}", res.error);
}
