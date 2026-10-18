from osisub.core.ass import Document, Event
from osisub.core import tools
from osisub.formats import parse_srt, dump_srt, dump_vtt
from osisub.core.timeutil import format_ass_time, parse_ass_time

SAMPLE = """\ufeff[Script Info]
; comment
Title: T
PlayResX: 640
PlayResY: 360

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.50,Default,Bob,0,0,0,,Hello, {\\i1}world{\\i0}\\Nline2
Comment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,note

[Aegisub Project Garbage]
Video File: x.mkv

[Custom]
a=b
"""


def test_roundtrip():
    d = Document.parse(SAMPLE)
    assert d.res == (640, 360)
    assert d.events[0].text.startswith("Hello, {\\i1}")
    assert d.styles[0].bold is True
    d2 = Document.parse(d.dumps())
    assert [e.to_line() for e in d.events] == [e.to_line() for e in d2.events]
    assert d2.garbage == [("Video File", "x.mkv")]
    assert d2.extra_sections[0][0] == "Custom"


def test_time():
    assert parse_ass_time("1:02:03.45") == 3723450
    assert format_ass_time(3723450) == "1:02:03.45"


def test_srt():
    d = Document.parse(SAMPLE)
    srt = dump_srt(d)
    assert "<i>world</i>" in srt and "00:00:01,000 --> 00:00:03,500" in srt
    back = parse_srt(srt)
    assert back.events[0].text.endswith("\\Nline2")
    assert dump_vtt(d).startswith("WEBVTT")


def test_tools():
    d = Document.parse(SAMPLE)
    tools.shift(d, 500)
    assert d.events[0].start == 1500
    d.events.append(Event(start=2000, end=2600, text="b"))
    assert tools.fix_overlaps(d) == 1
    assert tools.check(d)
    d.events[0].text = "i ıs"
    tools.change_case(d, "upper")
    assert d.events[0].text == "İ IS"
    assert tools.search_replace(d, "IS", "x") == 1


def test_split_merge():
    d = Document.new()
    d.events.append(Event(start=0, end=2000, text="ab cd"))
    tools.split_event(d, 0, at_char=2)
    assert len(d.events) == 2 and d.events[1].text == "cd"
    tools.merge_events(d, [0, 1])
    assert len(d.events) == 1


def test_hi_and_mojibake_and_stats():
    d = Document.new()
    d.events = [Event(start=0, end=2000, text="[müzik]"), Event(start=2000, end=4000, text="AHMET: Selam (güler)"),
                Event(start=4000, end=6000, text="KÃ¼Ã§Ã¼k")]
    assert tools.remove_hearing_impaired(d) == 2
    assert [e.text for e in d.events] == ["Selam", "KÃ¼Ã§Ã¼k"]
    assert tools.fix_mojibake(d) == 1 and d.events[1].text == "Küçük"
    st = tools.statistics(d)
    assert st["lines"] == 2 and st["chars"] == 10
