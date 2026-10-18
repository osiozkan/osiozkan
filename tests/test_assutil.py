from osisub.core.ass import Document, Event
from osisub.core import assutil as au


def test_resample():
    d = Document.new()
    d.set_info("PlayResX", "640"); d.set_info("PlayResY", "360")
    d.styles[0].fontsize = 20
    d.events.append(Event(text=r"{\pos(320,180)\fs30\bord2\clip(0,0,100,50)}x{\p1}m 0 0 l 10 10{\p0}"))
    au.resample(d, 1280, 720)
    assert d.styles[0].fontsize == 40
    t = d.events[0].text
    assert r"\pos(640,360)" in t and r"\fs60" in t and r"\bord4" in t and r"\clip(0,0,200,100)" in t
    assert "m 0 0 l 20 20" in t
    assert d.res == (1280, 720)


def test_karaoke_roundtrip():
    s = au.parse_syllables(r"{\k10}ka{\k20\b1}ra{\k30}oke")
    assert [x["dur"] for x in s] == [0, 10, 20, 30]
    assert au.build_karaoke(s) == r"{\k10}ka{\k20}{\b1}ra{\k30}oke"


def test_auto_karaoke():
    t = au.auto_karaoke("merhaba dünya", 2000, "word")
    assert t.count(r"\kf") == 2
    total = sum(x["dur"] for x in au.parse_syllables(t))
    assert total == 200


def test_lead_in():
    d = Document.new()
    d.events.append(Event(start=1000, end=2000, text=r"{\k50}a"))
    au.lead_in_out(d, [0], 300, 200)
    assert d.events[0].start == 700 and d.events[0].end == 2200
    assert d.events[0].text.startswith(r"{\k30}")
