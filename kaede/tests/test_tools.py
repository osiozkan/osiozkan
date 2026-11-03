from kaede.core.ass import Document, Event
from kaede.core import tools
from kaede.core.history import History


def mk(*evs):
    d = Document(); d.events = [Event(start=a, end=b, text=t) for a, b, t in evs]; return d


def test_turkish_case():
    assert tools.tr_upper("istanbul ışık") == "İSTANBUL IŞIK"
    assert tools.tr_lower("İSTANBUL IŞIK") == "istanbul ışık"
    assert tools.apply_case("{\\i1}iyi{\\i0}\\Nışık", "upper") == "{\\i1}İYİ{\\i0}\\NIŞIK"
    assert tools.apply_case("İYİ OLDU. HAYIR", "sentence") == "İyi oldu. Hayır"


def test_chain_overlap():
    d = mk((0, 1000, "a"), (1200, 2000, "b"), (1900, 3000, "c"))
    tools.chain_gaps(d, 500); assert d.events[0].end == 1200
    assert tools.fix_overlaps(d) == 1 and d.events[1].end == 1900


def test_split_merge():
    d = mk((0, 1000, "{\\i1}abcd{\\i0}efgh"))
    tools.split_at(d, 0, 4)
    assert [e.text for e in d.events] == ["{\\i1}abcd{\\i0}", "efgh"] or d.events[0].text.startswith("{\\i1}abcd")
    assert d.events[0].end == d.events[1].start == 500
    tools.merge_events(d, [0, 1]); assert len(d.events) == 1 and d.events[0].end == 1000


def test_find_replace_turkish():
    d = mk((0, 1000, "İstanbul ISIK"))
    assert tools.find_replace(d, "istanbul", "Ankara") == 1
    assert d.events[0].text.startswith("Ankara")


def test_qc():
    d = mk((0, 500, "x" * 30), (400, 2000, "ok line"), (3000, 3000, "zero"), (4000, 5000, ""))
    codes = {i.code for i in tools.run_qc(d)}
    assert {"cps", "overlap", "badtime", "empty"} <= codes
    d2 = mk((0, 2000, "KIRILDI Ã§ok"))
    assert any(i.code == "turkish" for i in tools.run_qc(d2))


def test_enforce_duration():
    d = mk((0, 300, "hi"), (800, 2000, "next"))
    tools.enforce_duration(d, min_ms=1000)
    assert d.events[0].end == 800


def test_history():
    d = mk((0, 1, "a")); h = History(); h.push("x", d, [0]); d.events[0].text = "b"
    l, old, s = h.undo_step(d, [0]); assert old.events[0].text == "a"
    l, new, s = h.redo_step(old, [0]); assert new.events[0].text == "b"


def test_resample():
    d = mk((0, 1, "{\\pos(100,50)\\fs20}x")); d.set_info("PlayResX", "100"); d.set_info("PlayResY", "100")
    tools.resample(d, 200, 200); assert "\\pos(200,100)" in d.events[0].text and "\\fs40" in d.events[0].text


def test_linear_sync():
    d = mk((1000, 2000, "a"), (11000, 12000, "b"))
    tools.linear_sync(d, 1000, 2000, 11000, 22000)
    assert (d.events[0].start, d.events[1].start) == (2000, 22000)
    assert d.events[1].end == 24000


def test_balance_and_hi():
    t = tools.balance_lines("Bu çok uzun bir cümle ve ortasından bölünmesi gerekiyor evet", 30)
    assert "\\N" in t and abs(t.index("\\N") - len(t) / 2) < 12
    d = mk((0, 1000, "[kapı çarpar]\\NMerhaba"), (1, 2, "AYŞE: Selam"), (2, 3, "(gülüyor)"))
    tools.remove_hi(d)
    assert d.events[0].text == "Merhaba" and d.events[1].text == "Selam" and d.events[2].text == ""


def test_merge_short_stats():
    d = mk((0, 500, "a"), (600, 900, "b"), (5000, 6000, "c"))
    assert tools.merge_short(d) == 1 and len(d.events) == 2 and d.events[0].text == "a b"
    assert tools.statistics(d)["lines"] == 2


def test_fonts_and_timecodes():
    from kaede.core import fonts, timecodes
    d = Document.parse("[V4+ Styles]\nFormat: Name, Fontname, Bold\nStyle: Default,Foo Bar,-1\n[Events]\n"
                       "Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{\\fnBaz}x")
    names = {u.family for u in fonts.used_fonts(d)}
    assert names == {"Foo Bar", "Baz"}
    tc = timecodes.parse_timecodes("# timecode format v2\n0\n42\n84\n125\n")
    assert tc.frame_at(84) == 2 and tc.time_at(3) == 125
    assert timecodes.parse_keyframes("# keyframe format v1\nfps 23.976\n0\n240\n") == [0, 240]
