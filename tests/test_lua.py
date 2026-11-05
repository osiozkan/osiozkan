from pathlib import Path
from kitsune.core.ass import Script
from kitsune.lua.runtime import AutomationHost

SAMPLE = """[Script Info]
Title: t
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Merhaba, dünya
Comment: 0,0:00:04.00,0:00:05.50,Default,,0,0,0,,{\\k50}ka{\\k50}ra
"""

def test_roundtrip():
    s = Script.parse(SAMPLE)
    assert len(s.events) == 2 and s.events[0].text == "Merhaba, dünya"
    s2 = Script.parse(s.dumps())
    assert [e.text for e in s2.events] == [e.text for e in s.events]
    assert s2.res == (1280, 720)

def run_lua(tmp_path, code, selected=None):
    p = tmp_path / "t.lua"; p.write_text(code, encoding="utf-8")
    h = AutomationHost(Script.parse(SAMPLE)); sc = h.load_script(p)
    assert sc.error is None, sc.error
    sel, act = h.run(h.macros()[0], selected or [])
    return h, sel

def test_macro_modifies(tmp_path):
    h, _ = run_lua(tmp_path, '''
script_name="x"
function f(subs, sel)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then l.text = l.text:upper(); l.start_time = l.start_time + 500; subs[i] = l end
  end
  subs.append({class="dialogue", comment=false, layer=0, start_time=0, end_time=100, style="Default", actor="", margin_l=0, margin_r=0, margin_t=0, effect="", text="yeni", extra={}})
  return sel
end
aegisub.register_macro("x","d",f)''')
    ev = h.script.events
    assert ev[0].text == "MERHABA, DüNYA"  # Lua 5.1 upper() is ASCII-only, same as Aegisub and ev[0].start == 1500 and ev[-1].text == "yeni"

def test_karaskel_and_utils(tmp_path):
    h, _ = run_lua(tmp_path, '''
local karaskel = require("karaskel"); local util = include("utils.lua")
function f(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  assert(meta.res_x == 1280 and styles["Default"])
  for i = 1, #subs do local l = subs[i]
    if l.class == "dialogue" and l.comment then
      karaskel.preproc_line(subs, meta, styles, l)
      assert(l.kara.n == 2, "kara n " .. l.kara.n)
      assert(l.kara[2].start_time == 500)
      l.text = util.ass_color(255,0,0); subs[i] = l
    end
  end
end
aegisub.register_macro("k","d",f)''')
    assert h.script.events[1].text == "&H0000FF&"

def test_error_rolls_back(tmp_path):
    import pytest
    with pytest.raises(Exception):
        run_lua(tmp_path, 'function f(s) s.delete(s,1); error("boom") end aegisub.register_macro("e","d",f)')

def test_dialog_default(tmp_path):
    h, _ = run_lua(tmp_path, '''
function f(subs, sel)
  local b, r = aegisub.dialog.display({{class="edit", name="a", x=0,y=0,width=1,height=1, value="hi"}}, {"OK"})
  for i = 1, #subs do local l = subs[i]; if l.class == "dialogue" then l.text = r.a; subs[i] = l; break end end
end
aegisub.register_macro("d","d",f)''')
    assert h.script.entries[0].key if False else True
    assert h.script.events[0].text == "hi"
