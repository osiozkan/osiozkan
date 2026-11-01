from sample import SAMPLE
from subforge.core import *
from subforge.core.formats import parse_srt, write_srt, parse_vtt, write_vtt
from subforge.core.tags import parse_karaoke, plain_text
from subforge.core.timecode import ms_to_ass, ass_to_ms


def test_roundtrip():
    d = parse_ass(SAMPLE)
    d2 = parse_ass(write_ass(d))
    assert [e.text for e in d.events] == [e.text for e in d2.events]
    assert d.events[1].comment and d.events[1].text == "note, with comma"
    assert d.play_res == (1280, 720)


def test_time_rounding():
    assert ms_to_ass(1005) == "0:00:01.01"
    assert ass_to_ms("1:02:03.45") == 3723450


def test_karaoke():
    syl = parse_karaoke(parse_ass(SAMPLE).events[0].text)
    assert [s.text for s in syl] == ["hel", "lo wor", "ld"]
    assert [s.duration_cs for s in syl] == [20, 30, 10]


def test_srt_convert():
    d = parse_ass(SAMPLE)
    s = write_srt(d)
    assert "<i>Selam</i>\r\ndünya" in s
    back = parse_srt(s)
    assert len(back.events) == 2 and "\\i1" in back.events[1].text
    assert "WEBVTT" in write_vtt(d)


def test_plain():
    assert plain_text(r"{\an8}a\Nb\hc") == "a\nb c"


def test_ssa_upgrade():
    ssa = ("[Script Info]\nScriptType: v4.00\n[V4 Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, "
           "TertiaryColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, AlphaLevel, Encoding\n"
           "Style: Default,Arial,20,16777215,255,0,0,-1,0,1,2,2,6,10,10,10,0,0\n[Events]\n")
    d = parse_ass(ssa)
    assert d.styles[0].align == 8 and d.styles[0].bold
    assert "V4+ Styles" in write_ass(d)
