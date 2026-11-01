import os
from sample import SAMPLE
from subforge.core import parse_ass, write_ass
from subforge.core import fonts


def test_uu_roundtrip():
    for n in (0, 1, 2, 3, 4, 59, 60, 61, 240, 1001):
        data = bytes((i * 7 + 3) % 256 for i in range(n))
        assert fonts.uudecode(fonts.uuencode(data)) == data
    assert "\r\n" in fonts.uuencode(bytes(100))


def test_attachments_roundtrip():
    d = parse_ass(SAMPLE)
    blob = os.urandom(500)
    fonts.set_attachments(d, [fonts.Attachment("x_0.ttf", blob)])
    d2 = parse_ass(write_ass(d))
    atts = fonts.list_attachments(d2)
    assert len(atts) == 1 and atts[0].name == "x_0.ttf" and atts[0].data == blob


def test_used_fonts():
    d = parse_ass(SAMPLE)
    d.events[0].text = r"{\fnComic Sans MS\b1}x"
    u = fonts.used_fonts(d)
    assert fonts.FontUse("Arial") in u and fonts.FontUse("Comic Sans MS", True, False) in u
