import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from osisub.ass import Document, parse_time, format_time, color_to_ass, Event, decode_bytes
from osisub import formats, qc, tools
from osisub.lua.runtime import Script

ASS = """[Script Info]
Title: t
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,20,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,{\\k50}Merhaba, {\\k50}dünya
Comment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,not shown
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,Overlap line çok çok çok çok çok çok çok çok uzun satır burada
"""


def test_time():
    assert parse_time("0:01:02.50") == 62500
    assert format_time(62500) == "0:01:02.50"
    assert color_to_ass("&HFF00FF&") == "&H00FF00FF&"


def test_roundtrip():
    d = Document.loads(ASS)
    assert len(d.events) == 3 and d.events[1].comment
    assert d.styles[0].bold and d.play_res == (1280, 720)
    d2 = Document.loads(d.dumps())
    assert d2.dumps() == d.dumps()
    assert d.events[0].text.startswith("{\\k50}Merhaba, ")  # comma in text preserved


def test_srt_vtt():
    d = Document.loads(ASS)
    srt = formats.to_srt(d)
    assert "00:00:01,000 --> 00:00:03,500" in srt
    d3 = formats.parse_srt(srt)
    assert len(d3.events) == 2 and d3.events[0].text == "Merhaba, dünya"
    assert formats.sniff(formats.to_vtt(d)) == "vtt"
    assert len(formats.parse_vtt(formats.to_vtt(d)).events) == 2


def test_encoding():
    assert decode_bytes("şğüİ".encode("cp1254")) == "şğüİ"
    assert decode_bytes("şğü".encode("utf-8-sig")) == "şğü"


def test_qc_and_tools():
    d = Document.loads(ASS)
    kinds = {i.kind for i in qc.check(d, max_line_chars=30)}
    assert {"overlap", "length"} <= kinds
    assert tools.fix_overlaps(d) == 1
    assert d.events[0].end == 3000 or d.events[2].start >= d.events[0].end
    tools.shift(d, 1000, sel=[0])
    assert d.events[0].start == 2000
    assert tools.find_replace(d, "dünya", "evren") == 1


def test_lua_macro():
    here = os.path.dirname(__file__)
    s = Script(os.path.join(here, "..", "automation", "shift_and_tag.lua"))
    assert s.macros[0]["name"] == "OsiSub/500ms kaydır"
    d = Document.loads(ASS)
    r = s.run_macro(1, d, [0])
    assert r.doc.events[0].start == 1500 and r.doc.events[0].effect.startswith("w=")
    assert len(r.doc.events) == 3 and r.doc.styles[0].name == "Default"
    assert r.log and "kaydırıldı" in r.log[0]


def test_lua_insert_delete(tmp_path):
    p = tmp_path / "x.lua"
    p.write_text('''script_name="x"
aegisub.register_macro("dup","",function(subs,sel)
  local l = subs[sel[1]]; local c = table.copy(l); c.text="copy"
  subs.insert(sel[1]+1, c)
  subs.delete(sel[1])
  assert(subs.n == #subs)
  local re = require("aegisub.re")
  c.text = re.sub("ab", "a(b)", "x\\\\1")
  return {sel[1]}
end)''')
    s = Script(str(p))
    d = Document.loads(ASS)
    r = s.run_macro(1, d, [0])
    assert any(e.text == "xb" for e in r.doc.events)
