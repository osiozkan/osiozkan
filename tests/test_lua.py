import os
import pytest
from osisub.automation import Engine
from osisub.core.ass import Document, Event

SCRIPT = r'''
script_name = "T"
script_description = "d"
include("karaskel.lua")
include("utils.lua")
local function up(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  for _, i in ipairs(sel) do
    local l = subs[i]
    karaskel.preproc_line(subs, meta, styles, l)
    l.text = l.text_stripped:upper() .. tostring(l.kara.n)
    subs[i] = l
  end
  subs.insert(sel[1] + 1, subs[sel[1]])
  return {sel[1]}
end
aegisub.register_macro("A/Up", "", up, function(subs, sel) return #sel > 0 end)
aegisub.register_macro("A/Fail", "", function() error("boom") end)
aegisub.register_macro("A/Cancel", "", function() aegisub.cancel() end)
aegisub.register_macro("A/Dlg", "", function(subs, sel)
  local b, r = aegisub.dialog.display({{class="edit", name="x", text="hi"}, {class="checkbox", name="c", value=true}}, {"Go","No"})
  local l = subs[sel[1]]; l.text = tostring(r.x) .. tostring(r.c); subs[sel[1]] = l
end)
aegisub.register_macro("A/Re", "", function(subs, sel)
  local l = subs[sel[1]]
  l.text = re.sub(l.text, "(\\w+)", "<\\1>"); subs[sel[1]] = l
  aegisub.debug.out(1, "%s\n", string.trim("  x ") .. ass_color(255,0,0) .. util.ass_style_color(1,2,3))
end)
'''


@pytest.fixture
def eng(tmp_path):
    p = tmp_path / "t.lua"
    p.write_text(SCRIPT)
    e = Engine()
    s = e.load_script(str(p))
    assert not s.error, s.error
    return e, s


def doc():
    d = Document.new()
    d.events.append(Event(start=0, end=2000, text=r"{\k10}ab{\k20}cd"))
    return d


def test_macro(eng):
    e, s = eng
    m = {m.name: m for m in s.macros}
    assert e.validate_macro(m["A/Up"], doc(), [0], 0)
    assert not e.validate_macro(m["A/Up"], doc(), [], 0)
    r = e.run_macro(m["A/Up"], doc(), [0], 0)
    assert r.ok and len(r.document.events) == 2
    assert r.document.events[0].text == "ABCD2"
    assert r.selection == [0]


def test_errors(eng):
    e, s = eng
    m = {m.name: m for m in s.macros}
    r = e.run_macro(m["A/Fail"], doc(), [0], 0)
    assert not r.ok and "boom" in r.error
    r = e.run_macro(m["A/Cancel"], doc(), [0], 0)
    assert r.cancelled


def test_dialog_and_re(eng):
    e, s = eng
    m = {m.name: m for m in s.macros}
    r = e.run_macro(m["A/Dlg"], doc(), [0], 0)
    assert r.document.events[0].text == "hitrue"
    d = doc(); d.events[0].text = "foo bar"
    r = e.run_macro(m["A/Re"], d, [0], 0)
    assert r.document.events[0].text == "<foo> <bar>"
    assert "&H0000FF&" in r.log[0][1]


def test_dialog_defaults(tmp_path):
    p = tmp_path / "d.lua"
    p.write_text(r'''
aegisub.register_macro("D", "", function(subs, sel)
  local ok, r = aegisub.dialog.display({{class="color", name="c", value="&H0000FF&"}, {class="alpha", name="a", value="&H80&"},
    {class="dropdown", name="d", items={"x","y"}, value="y"}, {class="intedit", name="n", value=5}})
  local l = subs[sel[1]]
  l.text = tostring(ok) .. r.c .. r.a .. r.d .. r.n
  subs[sel[1]] = l
end)''')
    e = Engine()
    s = e.load_script(str(p))
    r = e.run_macro(s.macros[0], doc(), [0], 0)
    assert r.ok, r.error
    assert r.document.events[0].text == "true&H0000FF&&H80&y5"


def test_review_fixes(tmp_path):
    p = tmp_path / "r.lua"
    p.write_text(r'''
aegisub.register_macro("R", "", function(subs, sel)
  local n = 0
  for m in re.gfind("a1b2", "\\d") do n = n + 1 end
  local l = subs[sel[1]]
  l.text = n .. re.sub("ab12", "[[:digit:]]+", "N") .. re.sub("x", "x", "\\an8") .. util.ass_color(300, -5, 12.7)
  subs[sel[1]] = l
  local s = subs[1]
  for i = 1, #subs do local st = subs[i]; if st.class == "style" then st.color1 = util.ass_color(1,2,3); subs[i] = st end end
  return {sel[1], 99999}
end)''')
    e = Engine(); s = e.load_script(str(p))
    r = e.run_macro(s.macros[0], doc(), [0], 0)
    assert r.ok, r.error
    assert r.document.events[0].text == "2abNan8&H0D00FF&"
    assert r.document.styles[0].primary == "&H00030201"
    assert r.selection == [0]
