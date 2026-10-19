use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use subforge_core::ass::Script;
use subforge_lua::{Doc, Headless, LuaScript, SharedHost};

fn write_script(name: &str, code: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("subforge-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, code).unwrap();
    p
}

fn host() -> SharedHost {
    Rc::new(RefCell::new(Headless { quiet: true, ..Default::default() }))
}

fn sample() -> Script {
    Script::parse("[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,25,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\k50}ka{\\k50}ra{\\k100}oke\nDialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,Merhaba dünya\n")
}

#[test]
fn macro_edits_lines() {
    let p = write_script(
        "m1.lua",
        r#"
script_name = "Test macro"
script_description = "desc"
script_author = "me"
script_version = "1"
function upper_all(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = unicode.to_upper_tr(l.text)
    l.start_time = l.start_time + 100
    subs[i] = l
  end
  subs.insert(#subs + 1, {class="dialogue", comment=false, layer=0, start_time=0, end_time=500, style="Default", actor="", effect="", text="eklendi", margin_l=0, margin_r=0, margin_t=0})
  return sel
end
aegisub.register_macro("Upper", "makes upper", upper_all)
"#,
    );
    let s = LuaScript::load(&p, host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    assert_eq!(s.name, "Test macro");
    assert_eq!(s.macros.len(), 1);
    let doc = Rc::new(RefCell::new(Doc::from_script(&sample())));
    let first = doc.borrow().event_to_flat(0);
    let second = doc.borrow().event_to_flat(1);
    let out = s.run_macro(0, doc.clone(), &[second], second).unwrap();
    assert_eq!(out.selection, Some(vec![second]));
    let script = doc.borrow().to_script();
    assert_eq!(script.events.len(), 3);
    assert_eq!(script.events[1].text, "MERHABA DÜNYA");
    assert_eq!(script.events[1].start, 4100);
    assert_eq!(script.events[2].text, "eklendi");
    assert_eq!(script.events[0].text, "{\\k50}ka{\\k50}ra{\\k100}oke");
    let _ = first;
}

#[test]
fn karaskel_and_utils() {
    let p = write_script(
        "m2.lua",
        r#"
script_name = "kara"
include("karaskel.lua")
local out = {}
function t(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  assert(meta.res_x == 1280 and meta.res_y == 720, "meta")
  assert(styles["Default"].fontsize == 40)
  local i = sel[1]
  local line = subs[i]
  karaskel.preproc_line(subs, meta, styles, line)
  assert(line.kara.n == 3, "syls " .. tostring(line.kara.n))
  assert(line.kara[1].text == "ka" and line.kara[1].duration == 500)
  assert(line.kara[3].start_time == 1000 and line.kara[3].end_time == 2000)
  assert(line.width > 0 and line.height > 0)
  assert(line.kara[2].left > line.kara[1].left)
  assert(line.halign == "center" and line.valign == "bottom")
  assert(line.bottom == 720 - 25, "bottom " .. tostring(line.bottom))
  assert(util.ass_color(255, 128, 0) == "&H0080FF&")
  local r, g, b = util.extract_color("&H00FF8040")
  assert(r == 0x40 and g == 0x80 and b == 0xFF)
  assert(string.trim("  x ") == "x")
  local h, tl = string.headtail("a b c")
  assert(h == "a" and tl == "b c")
  assert(re.sub("hello world", "o", "0") == "hell0 w0rld")
  assert(unicode.len("çğş") == 3)
  assert(bit.band(0xff, 0x0f) == 15)
  local m = re.match("abc123", "(\\d+)")
  assert(m[2].str == "123")
  aegisub.debug.out(3, "ok %d\n", 1)
end
aegisub.register_macro("kara", "k", t)
"#,
    );
    let s = LuaScript::load(&p, host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    let doc = Rc::new(RefCell::new(Doc::from_script(&sample())));
    let first = doc.borrow().event_to_flat(0);
    s.run_macro(0, doc, &[first], first).unwrap();
}

#[test]
fn dialog_defaults_and_cancel() {
    let p = write_script(
        "m3.lua",
        r#"
script_name = "dlg"
function t(subs, sel)
  local btn, res = aegisub.dialog.display({
    {class="label", label="Shift (ms):", x=0, y=0},
    {class="intedit", name="ms", value=250, min=0, max=10000, x=1, y=0},
    {class="checkbox", name="start", label="start", value=true, x=0, y=1},
    {class="dropdown", name="mode", items={"a","b"}, value="b", x=0, y=2},
  }, {"Apply", "Cancel"}, {ok="Apply", cancel="Cancel"})
  if not btn then aegisub.cancel() end
  assert(btn == "Apply")
  assert(res.ms == 250 and res.start == true and res.mode == "b")
  local l = subs[sel[1]]
  l.start_time = l.start_time + res.ms
  subs[sel[1]] = l
end
function c(subs, sel) aegisub.cancel() end
aegisub.register_macro("dlg", "d", t)
aegisub.register_macro("cancel", "c", c)
"#,
    );
    let s = LuaScript::load(&p, host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    let doc = Rc::new(RefCell::new(Doc::from_script(&sample())));
    let i = doc.borrow().event_to_flat(1);
    s.run_macro(0, doc.clone(), &[i], i).unwrap();
    assert_eq!(doc.borrow().to_script().events[1].start, 4250);
    assert!(matches!(s.run_macro(1, doc, &[i], i), Err(subforge_lua::ScriptError::Cancelled)));
}

#[test]
fn load_errors_are_captured() {
    let p = write_script("bad.lua", "this is not lua !!!");
    let s = LuaScript::load(&p, host(), &[]);
    assert!(s.load_error.is_some());
    let p = write_script("inc.lua", "include('nonexistent.lua')");
    let s = LuaScript::load(&p, host(), &[]);
    assert!(s.load_error.unwrap().contains("include not found"));
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aegisub-autoload").join(name)
}

const KARA_SCRIPT: &str = "[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,25,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nComment: 0,0:00:00.00,0:00:00.00,Default,,0,0,0,code once,fxvar = 3\nComment: 0,0:00:00.00,0:00:00.00,Default,,0,0,0,template syl,{\\pos(!$sx+$swidth/2!,!$sy!)\\an5\\fad(100,100)\\blur!fxvar!}\nComment: 0,0:00:00.00,0:00:00.00,Default,,0,0,0,template line,{\\be1}\nDialogue: 0,0:00:10.00,0:00:14.00,Default,,0,0,0,karaoke,{\\k50}ka{\\k50}ra{\\k100}o{\\k100}ke\n";

#[test]
fn real_kara_templater_runs() {
    let p = fixture("kara-templater.lua");
    let s = LuaScript::load(&p, host(), &[]);
    assert!(s.load_error.is_none(), "{:?}", s.load_error);
    assert!(s.macros.iter().any(|m| m.name.contains("karaoke template")), "{:?}", s.macros);
    let doc = Rc::new(RefCell::new(Doc::from_script(&Script::parse(KARA_SCRIPT))));
    let n = doc.borrow().counts().2;
    let sel: Vec<usize> = (0..n).map(|e| doc.borrow().event_to_flat(e)).collect();
    let idx = s.macros.iter().position(|m| m.name.contains("karaoke template")).unwrap();
    let out = s.run_macro(idx, doc.clone(), &sel, sel[0]);
    out.unwrap();
    let script = doc.borrow().to_script();
    let fx: Vec<_> = script.events.iter().filter(|e| e.effect == "fx" && !e.is_comment()).collect();
    // 1 line-template result + syllable 0 (empty, before the first \k) + 4 syllables, as in Aegisub
    assert_eq!(fx.len(), 6, "{:#?}", script.events.iter().map(|e| (&e.effect, &e.text)).collect::<Vec<_>>());
    let syl_lines: Vec<_> = fx.iter().filter(|e| e.text.contains("\\pos(")).collect();
    assert_eq!(syl_lines.len(), 5);
    assert!(syl_lines.iter().all(|e| e.text.contains("\\blur3")));
    for (line, syl) in syl_lines.iter().skip(1).zip(["ka", "ra", "o", "ke"]) {
        assert!(line.text.ends_with(syl), "{}", line.text);
    }
    // layout is monotonic left→right and positions use real font metrics
    let xs: Vec<i32> = syl_lines.iter().map(|e| e.text.split("\\pos(").nth(1).unwrap().split(',').next().unwrap().parse().unwrap()).collect();
    assert!(xs.windows(2).all(|w| w[0] <= w[1]), "{xs:?}");
}

#[test]
fn real_cleantags_and_simple_macros() {
    for (file, idx) in [("cleantags-autoload.lua", 0), ("macro-1-edgeblur.lua", 0), ("macro-2-mkfullwitdh.lua", 0)] {
        let s = LuaScript::load(&fixture(file), host(), &[]);
        assert!(s.load_error.is_none(), "{file}: {:?}", s.load_error);
        assert!(!s.macros.is_empty(), "{file}");
        let mut sc = sample();
        sc.events[1].text = "{\\b1}{\\i1}Merhaba".into();
        let doc = Rc::new(RefCell::new(Doc::from_script(&sc)));
        let sel = vec![doc.borrow().event_to_flat(1)];
        s.run_macro(idx, doc.clone(), &sel, sel[0]).unwrap_or_else(|e| panic!("{file}: {e}"));
        if file.starts_with("cleantags") {
            assert_eq!(doc.borrow().to_script().events[1].text, "{\\b1\\i1}Merhaba");
        }
    }
}
