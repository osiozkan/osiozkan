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

def test_timing():
    from kitsune.core import timing
    sc = Script(); sc.entries += [Event(start=1000, end=2000, text="a"), Event(start=2100, end=3000, text="b"),
                                   Event(start=2900, end=4000, text="c")]
    assert timing.close_gaps(sc, 200) == 1 and sc.events[0].end == 2100
    assert timing.fix_overlaps(sc) == 1 and sc.events[1].end == 2900
    assert timing.snap_to_keyframes(sc, [950], 100, 100, 100, 100) == 1 and sc.events[0].start == 950
    sc.events[2].text = "x\\Ny"; assert timing.split_lines_by_break(sc) == 1 and len(sc.events) == 4
