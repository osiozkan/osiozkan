from sample import SAMPLE
from subforge.core import parse_ass
from subforge.core import tools, checks
from subforge.core.ass import Event
from subforge.core.textutil import tr_upper, tr_lower


def ev(a, b, t="x", **k):
    return Event(start_time=a, end_time=b, text=t, **k)


def test_shift_sync():
    e = [ev(1000, 2000), ev(3000, 4000)]
    tools.shift(e, 500)
    assert (e[0].start_time, e[1].end_time) == (1500, 4500)
    tools.two_point_sync(e, 1500, 1000, 4500, 4000)
    assert (e[0].start_time, e[1].end_time) == (1000, 4000)


def test_bridge_and_overlap():
    e = [ev(0, 1000), ev(1100, 2000), ev(1900, 3000)]
    assert tools.bridge_gaps(e, 250) == 1 and e[0].end_time == 1100
    tools.fix_overlaps(e)
    assert e[1].end_time == 1900


def test_snap():
    e = [ev(1010, 2995)]
    tools.snap_to_keyframes(e, [1000, 3000], 100, 100, 100, 100)
    assert (e[0].start_time, e[0].end_time) == (1000, 3000)


def test_turkish_case():
    assert tr_upper("iğdır ı") == "İĞDIR I"
    assert tr_lower("İSTANBUL IŞIK") == "istanbul ışık"
    e = [ev(0, 1, r"{\i1}ışık\Nistek")]
    tools.change_case(e, "upper")
    assert e[0].text == r"{\i1}IŞIK\NİSTEK"


def test_qc_and_fix():
    d = parse_ass(SAMPLE)
    d.events[0].end_time = d.events[0].start_time + 100
    codes = {i.code for i in checks.check(d)}
    assert "short" in codes
    e = [ev(0, 1000, "a  b ,c....")]
    tools.fix_common_errors(e)
    assert e[0].text == "a b,c..."


def test_split_merge():
    d = parse_ass(SAMPLE)
    e = d.events[2]
    b = tools.split_event(d, e, at_char=5)
    assert e.end_time == b.start_time and len(d.events) == 4
    tools.merge_events(d, e, b)
    assert len(d.events) == 3


def test_resample():
    d = parse_ass(SAMPLE)
    d.events[0].text = r"{\pos(100,50)\fs20}x"
    tools.resample(d, 1920, 1080)
    assert d.play_res == (1920, 1080)
    assert r"\pos(150,75)" in d.events[0].text and r"\fs30" in d.events[0].text
    assert d.styles[0].fontsize == 60


def test_balance():
    e = [ev(0, 1, "word " * 12)]
    tools.balance_lines(e, 30)
    assert "\\N" in e[0].text
