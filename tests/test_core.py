import os
from yorusub.core import AssFile, ms_to_ass, ass_to_ms
from yorusub.core.formats import parse_srt, to_srt, to_vtt, parse_vtt
from yorusub.core import timing, qc, tags, search

D = os.path.join(os.path.dirname(__file__), "data")


def load():
    return AssFile.load(os.path.join(D, "sample.ass"))


def test_time_roundtrip():
    assert ms_to_ass(3723450) == "1:02:03.45"
    assert ass_to_ms("1:02:03.45") == 3723450


def test_parse_and_roundtrip_preserves_sections():
    a = load()
    assert len(a.events) == 4 and a.events[2].comment
    assert a.events[0].text.startswith("Merhaba, dünya!")      # metindeki virgül korunur
    assert a.style("Sign").bold and a.style("Sign").align == 8
    out = a.dumps()
    assert "[Aegisub Project Garbage]" in out and "Video File: ep01.mkv" in out
    b = AssFile.parse(out)
    assert [e.text for e in b.events] == [e.text for e in a.events]
    assert b.dumps() == out


def test_srt_vtt():
    a = load()
    srt = to_srt(a)
    assert "00:00:01,000 --> 00:00:03,500" in srt and "Merhaba, dünya!\nIkinci satır" in srt
    b = parse_srt(srt)
    assert len(b.events) == 3 and b.events[0].start == 1000
    assert parse_vtt(to_vtt(a)).events[0].end == 3500
    i = parse_srt("1\n00:00:01,000 --> 00:00:02,000\n<i>Selam</i>\n")
    assert i.events[0].text == "{\\i1}Selam{\\i0}"


def test_qc():
    kinds = {i.kind for i in qc.check(load())}
    assert {"overlap", "spaces", "short"} <= kinds


def test_timing():
    a = load()
    assert timing.fix_overlaps(a.events) == 1
    assert a.events[0].end == a.events[1].start
    assert timing.enforce_min_duration(a.events, 500) >= 1
    timing.shift(a.events, 500)
    assert a.events[0].start == 1500


def test_tags_and_search():
    assert tags.strip_tags("{\\an8}Selam{\\i1}!") == "Selam!"
    assert tags.parse_karaoke("{\\k20}Ka{\\k30}ra")[1] == ("k", 30, "ra")
    assert tags.unbalanced_braces("{\\i1") and not tags.unbalanced_braces("{\\i1}a")
    a = load()
    assert search.find(a, "çift") == [1]
    assert search.tr_upper("istanbul ı") == "İSTANBUL I"
