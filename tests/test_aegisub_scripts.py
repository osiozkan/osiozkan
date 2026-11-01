"""Compatibility: run Aegisub's own bundled automation scripts on our Lua host."""
import glob
import os

import pytest

from subforge.automation import AutomationScript, Host, HeadlessUI, DEFAULT_INCLUDE_DIR
from subforge.core import parse_ass

AUTOLOAD = os.path.join(os.path.dirname(DEFAULT_INCLUDE_DIR), "autoload")

KARA = r"""[Script Info]
ScriptType: v4.00+
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Comment: 0,0:00:00.00,0:00:00.00,Default,,0,0,0,code once,myscale = 150
Comment: 0,0:00:00.00,0:00:00.00,Default,,0,0,0,template syl,{\an5\pos($scenter,$smiddle)\fscx!myscale!\t($sdur,$sdur,\fscx100)}
Dialogue: 0,0:00:10.00,0:00:13.00,Default,,0,0,0,karaoke,{\k20}ka{\k30}ra{\k50}o{\k10}ke
"""


def scripts():
    return sorted(glob.glob(os.path.join(AUTOLOAD, "*.lua")))


@pytest.mark.parametrize("path", scripts(), ids=os.path.basename)
def test_loads_and_every_macro_runs(path):
    s = AutomationScript(path)
    assert s.error is None, s.error
    assert s.macros
    for m in s.macros:
        doc = parse_ass(KARA)
        h = Host(doc, [5, 6, 7], 5, HeadlessUI())
        ok, _ = m.validate(h)
        assert ok
        m.run(h)


def test_kara_templater_output():
    s = AutomationScript(os.path.join(AUTOLOAD, "kara-templater.lua"))
    doc = parse_ass(KARA)
    h = Host(doc, [7], 7, HeadlessUI())
    s.macros[0].run(h)
    fx = [e for e in doc.events if e.effect == "fx"]
    assert len(fx) == 5  # zero'th (empty) + 4 syllables, per this templater version
    ka = next(e for e in fx if e.text.endswith("}ka"))
    assert "\\fscx150" in ka.text and "\\t(200,200,\\fscx100)" in ka.text and "\\pos(" in ka.text
    # template/code lines were turned into comments, original karaoke line commented out
    assert [e.comment for e in doc.events if e.effect.startswith(("code", "template", "karaoke"))] == [True] * 3


def test_templater_as_export_filter():
    s = AutomationScript(os.path.join(AUTOLOAD, "kara-templater.lua"))
    doc = parse_ass(KARA)
    h = Host(doc, [], 0, HeadlessUI())
    s.filters[0].run(h, {})
    assert sum(e.effect == "fx" for e in doc.events) == 5


def test_parse_karaoke_data_inline_tags(tmp_path):
    p = tmp_path / "k.lua"
    p.write_text('''
aegisub.register_macro("m","",function(subs, sel)
  local l = subs[6]
  l.text = "{\\\\an8\\\\k10}a{\\\\i1}b{\\\\kf20}cd"
  local k = aegisub.parse_karaoke_data(l)
  l.actor = #k .. ":" .. k[0].text .. ":" .. k[1].text .. ":" .. k[1].duration .. ":" .. k[2].text .. ":" .. k[2].tag .. ":" .. k[2].start_time
  subs[6] = l end)''')
    s = AutomationScript(str(p))
    doc = parse_ass(KARA)
    s.macros[0].run(Host(doc, [6], 6, HeadlessUI()))
    assert doc.lines[5].actor == "2:{\\an8}:a{\\i1}b:100:cd:\\kf:100"


def test_repo_example_script_runs():
    root = os.path.dirname(os.path.dirname(__file__))
    s = AutomationScript(os.path.join(root, "scripts", "ornek-makro.lua"))
    assert s.error is None, s.error
    assert [m.name for m in s.macros] == ["SubForge/Metin ekle", "SubForge/Satır ölçüleri"]
    doc = parse_ass(KARA)
    ui = HeadlessUI({"bas": "[", "son": "]"}, button="Uygula")
    h = Host(doc, [7], 7, ui)
    s.macros[0].run(h)
    assert doc.events[-1].text.startswith("[{\\k20}ka") and doc.events[-1].text.endswith("ke]")
    s.macros[1].run(Host(doc, [7], 7, ui))
    assert any("genişlik" in m for _, m in ui.logs)
