import pytest
from kaede.core.ass import Document, parse_color, format_color
from kaede.core.timeutil import ass_time, parse_ass_time, FrameRate
from kaede.core import formats, tags

SAMPLE = """[Script Info]
Title: t
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,25,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,{\\i1}Merhaba{\\i0}, dünya\\Nİkinci satır
Comment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,not shown
"""


def test_roundtrip():
    d = Document.parse(SAMPLE)
    assert d.res == (1280, 720)
    assert d.styles[0].bold and d.styles[0].margin_t == 25
    assert len(d.events) == 2 and d.events[1].comment
    d2 = Document.parse(d.dump())
    assert [e.text for e in d2.events] == [e.text for e in d.events]
    assert d2.styles[0] == d.styles[0]


def test_time():
    assert ass_time(3500) == "0:00:03.50"
    assert ass_time(3504) == "0:00:03.50" and ass_time(3505) == "0:00:03.51"
    assert parse_ass_time("1:02:03.45") == 3723450


def test_color():
    assert parse_color("&H00FF8040") == (0x40, 0x80, 0xFF, 0)
    assert format_color(0x40, 0x80, 0xFF, 0) == "&H00FF8040"


def test_srt_roundtrip():
    d = Document.parse(SAMPLE)
    srt = formats.dump_srt(d)
    assert "<i>Merhaba</i>, dünya\nİkinci satır" in srt
    back = formats.parse_srt(srt)
    assert back.events[0].start == 1000 and back.events[0].end == 3500
    assert "\\N" in back.events[0].text


def test_tags():
    assert tags.strip_tags("{\\i1}ab{\\i0}c") == "abc"
    assert tags.char_count("ab\\Ncd e") == 5


def test_frames():
    fr = FrameRate(24000 / 1001)
    for f in (1, 100, 1000):
        assert fr.frame_at(fr.start_ms(f)) == f
    assert fr.snap_start(1001) == fr.start_ms(fr.frame_at(1001) + (1 if fr.start_ms(fr.frame_at(1001)) < 1001 else 0))


def test_cp1254():
    data = "Şöyle ğüişçö İI".encode("cp1254")
    assert formats.decode_bytes(data) == "Şöyle ğüişçö İI"
