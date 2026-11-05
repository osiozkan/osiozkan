from kitsune.core import srt, checks
from kitsune.core.ass import Script, Event
from kitsune.core.timecode import ms_to_ass, ass_to_ms
from kitsune.core.tags import karaoke_syllables, strip_tags

def test_time():
    assert ms_to_ass(3723450) == "1:02:03.45" and ass_to_ms("1:02:03.45") == 3723450
def test_srt_roundtrip():
    sc = srt.parse_srt("1\n00:00:01,000 --> 00:00:02,500\n<i>Selam</i>\nnaber\n\n2\n00:00:03,000 --> 00:00:04,000\nhey\n")
    assert sc.events[0].text == "{\\i1}Selam{\\i0}\\Nnaber"
    out = srt.dump_srt(sc)
    assert "<i>Selam</i>\nnaber" in out and "00:00:01,000 --> 00:00:02,500" in out
def test_kara():
    assert karaoke_syllables("{\\k20}a{\\kf30}b") == [(20, "k", "a"), (30, "kf", "b")]
def test_lint():
    sc = Script(); sc.entries.append(Event(start=0, end=300, text="x" * 60, style="Nope"))
    codes = {i.code for i in checks.lint(sc)}
    assert {"short", "style", "linelen"} <= codes
