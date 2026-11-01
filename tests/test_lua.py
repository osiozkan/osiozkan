import pytest
from sample import SAMPLE
from subforge.core import parse_ass
from subforge.automation import AutomationScript, Host, HeadlessUI, ScriptError, CancelledError


def make(tmp_path, code):
    p = tmp_path / "s.lua"
    p.write_text(code, encoding="utf-8")
    return AutomationScript(str(p))


def run(tmp_path, code, sel=None, answers=None):
    s = make(tmp_path, code)
    assert s.error is None, s.error
    h = Host(parse_ass(SAMPLE), sel or [], ui=HeadlessUI(answers))
    s.macros[0].run(h)
    return h


def test_edit_and_len(tmp_path):
    h = run(tmp_path, '''
script_name="x"
aegisub.register_macro("m","",function(subs, sel)
  for _, i in ipairs(sel) do local l = subs[i]; l.text = l.text:upper(); subs[i] = l end
  subs.append({class="dialogue", section="[Events]", start_time=0, end_time=1, style="Default", text="z"})
  assert(#subs == subs.n)
  return sel end)''', [6])
    assert "HEL" in h.doc.lines[5].text
    assert h.doc.lines[5].text == h.doc.lines[5].text.upper()
    assert h.doc.events[-1].text == "z"


def test_insert_delete_selection_remap(tmp_path):
    h = run(tmp_path, '''
aegisub.register_macro("m","",function(subs, sel)
  subs.delete(sel[1])
end)''', [6, 8])
    assert h.selection == [7]


def test_cancel(tmp_path):
    s = make(tmp_path, 'aegisub.register_macro("m","",function() aegisub.cancel() end)')
    with pytest.raises(CancelledError):
        s.macros[0].run(Host(parse_ass(SAMPLE)))


def test_error_reported(tmp_path):
    s = make(tmp_path, 'aegisub.register_macro("m","",function() error("boom") end)')
    with pytest.raises(ScriptError, match="boom"):
        s.macros[0].run(Host(parse_ass(SAMPLE)))


def test_validate_and_dialog(tmp_path):
    h = run(tmp_path, '''
aegisub.register_macro("m","",function(subs, sel)
  local b, r = aegisub.dialog.display({{class="intedit", name="n", value=3}}, {"Go","Cancel"}, {ok="Go", cancel="Cancel"})
  if not b then aegisub.cancel() end
  local l = subs[6]; l.layer = r.n; subs[6] = l end)''', [6], answers={"n": 9})
    assert h.doc.lines[5].layer == 9


def test_re_and_unicode(tmp_path):
    h = run(tmp_path, '''
local re = require "aegisub.re"
local unicode = require "unicode"
aegisub.register_macro("m","",function(subs, sel)
  local m = re.find("héllo wörld", "(w\\\\S+)")
  local l = subs[6]
  l.actor = m[2].str .. ":" .. m[2].first .. ":" .. unicode.len("héé")
  local n = 0 for c in unicode.chars("añb") do n = n + 1 end
  l.effect = tostring(n)
  subs[6] = l end)''', [6])
    assert h.doc.lines[5].actor == "wörld:8:3" and h.doc.lines[5].effect == "3"


def test_karaskel(tmp_path):
    h = run(tmp_path, '''
include("karaskel.lua")
aegisub.register_macro("m","",function(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local l = subs[6]; karaskel.preproc_line(subs, meta, styles, l)
  assert(l.kara.n == 3 and l.kara[2].duration == 300 and l.kara[2].start_time == 200)
  assert(l.halign == "center" and l.valign == "bottom")
  l.effect = string.format("%d", l.kara.n); subs[6] = l end)''', [6])
    assert h.doc.lines[5].effect == "3"


def test_styles_and_info(tmp_path):
    h = run(tmp_path, '''
aegisub.register_macro("m","",function(subs, sel)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "style" then l.fontsize = 55; l.bold = true; l.color1 = "&H0000FFFF"; subs[i] = l end
    if l.class == "info" and l.key == "Title" then l.value = "new"; subs[i] = l end
  end end)''')
    st = h.doc.styles[0]
    assert st.fontsize == 55 and st.bold and st.color1 == "&H0000FFFF"
    assert h.doc.get_info("Title") == "new"


def test_byte_level_strings_and_unicode_roundtrip(tmp_path):
    h = run(tmp_path, '''
aegisub.register_macro("m","",function(subs, sel)
  local l = subs[6]
  l.text = "ğüşiçö İıÖ 日本語"
  -- byte-slicing through a multibyte char must not crash; reassembly must be lossless
  local cut = l.text:sub(1, 3) .. l.text:sub(4)
  l.actor = cut == l.text and "same" or "diff"
  l.effect = tostring(#l.text)
  subs[6] = l
  local bad = subs[7]
  bad.text = l.text:sub(1, 1)  -- invalid utf-8 fragment
  subs[7] = bad
  aegisub.log("başarılı ✓\\n")
end)''', [6])
    e = h.doc.lines[5]
    assert e.text == "ğüşiçö İıÖ 日本語" and e.actor == "same"
    assert e.effect == str(len(e.text.encode("utf-8")))


def test_non_ascii_script_path_and_name(tmp_path):
    d = tmp_path / "betikler ğü"
    d.mkdir()
    p = d / "özel.lua"
    p.write_text('script_name="Özel Betik"\naegisub.register_macro("Türkçe/Makro","açıklama",function() end)', encoding="utf-8")
    s = AutomationScript(str(p))
    assert s.error is None and s.name == "Özel Betik" and s.macros[0].name == "Türkçe/Makro"


def test_filter_run_with_config(tmp_path):
    p = tmp_path / "f.lua"
    p.write_text('''
aegisub.register_filter("Mark", "d", 5,
  function(subs, cfg) for i=1,#subs do local l=subs[i]; if l.class=="dialogue" then l.effect=cfg.tag; subs[i]=l end end end,
  function(subs, old) return {{class="edit", name="tag", text="X"}} end)''')
    s = AutomationScript(str(p))
    f = s.filters[0]
    h = Host(parse_ass(SAMPLE), [], ui=HeadlessUI({"tag": "ZZ"}))
    cfg = f.get_config(h)
    assert cfg == {"tag": "ZZ"}
    f.run(h, cfg)
    assert all(e.effect == "ZZ" for e in h.doc.events)
