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


def test_uuencode_roundtrip():
    import os
    for n in (0, 1, 2, 3, 4, 100, 257):
        data = os.urandom(n)
        assert au.uudecode(au.uuencode(data)) == data


def test_attach_roundtrip(tmp_path):
    f = tmp_path / "x.ttf"
    f.write_bytes(bytes(range(256)) * 3)
    d = Document.new()
    au.attach_font(d, str(f))
    d2 = Document.parse(d.dumps())
    assert d2.attachments and au.uudecode(d2.attachments[0][2]) == f.read_bytes()
    assert au.extract_fonts(d2, str(tmp_path / "out"))


def test_postprocess_and_select():
    from osisub.core import tools
    d = Document.new()
    d.events = [Event(start=1000, end=2000, text="a"), Event(start=2100, end=3000, text="b")]
    n = tools.timing_postprocess(d, lead_in=200, lead_out=100, join_gap=500)
    assert n == 2 and d.events[0].end == d.events[1].start
    assert tools.select_lines(d, "b") == [1]


def test_attachment_line_looks_like_header():
    txt = "[Script Info]\nTitle: x\n\n[Fonts]\nfontname: a.ttf\n[abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd]\n!!!\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,hi\n"
    d = Document.parse(txt)
    assert len(d.attachments[0][2]) == 2 and len(d.events) == 1


def test_kara_durations():
    t = r"{\k10\b1}a{\kf20}b{\ko5}c"
    assert au.kara_durations(t) == [10, 20, 5]
    assert au.set_kara_durations(t, [1, 2, 3]) == r"{\k1\b1}a{\kf2}b{\ko3}c"
    assert au.kara_syllable_texts(t) == ["a", "b", "c"]
