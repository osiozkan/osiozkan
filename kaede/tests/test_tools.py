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
