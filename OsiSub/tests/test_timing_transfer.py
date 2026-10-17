"""Zamanlama son işlemcisi, SRT/VTT ve referans aktarma testleri (arayüzsüz)."""

import os
import tempfile
import unittest

from osisub.core.assfile import Event as E
from osisub.core.formats import ass_to_html, html_to_ass, load_any, parse_srt, parse_vtt, to_srt
from osisub.core.framemap import END, START, FrameMap
from osisub.core.timing import TimingOptions, postprocess
from osisub.core.transfer import TransferOptions, plan_transfer


def fm24():
    fm = FrameMap.from_fps(24000 / 1001, 3000)
    fm.keyframes = [0, 48, 96, 144, 192, 240]
    return fm


class TestTiming(unittest.TestCase):
    def test_lead_in_out_clamps_zero(self):
        evs = [E(start=50, end=1000), E(start=2000, end=3000)]
        r = postprocess(evs, [0, 1], TimingOptions(lead_in=100, lead_out=200))
        self.assertEqual(r.changes[0], {"start": 0, "end": 1200})
        self.assertEqual(r.changes[1], {"start": 1900, "end": 3200})

    def test_adjacent_gap_and_overlap(self):
        evs = [E(start=0, end=1000), E(start=1200, end=2000), E(start=1900, end=3000), E(start=5000, end=6000)]
        r = postprocess(evs, [0, 1, 2, 3], TimingOptions(adjacent=True, adjacent_threshold=250,
                                                         adjacent_bias=0.5))
        self.assertEqual(r.changes[0]["end"], 1100)
        self.assertEqual(r.changes[1]["start"], 1100)
        self.assertEqual(r.changes[1]["end"], 1950)      # 100 ms çakışma ortadan bölündü
        self.assertEqual(r.changes[2]["start"], 1950)
        self.assertNotIn(3, r.changes)                   # 2000 ms boşluk eşik dışı
        r0 = postprocess(evs, [0, 1], TimingOptions(adjacent=True, adjacent_threshold=250, adjacent_bias=0.0))
        self.assertEqual(r0.changes[1]["start"], 1000)   # bias 0: öncekinin bitişi
        self.assertNotIn(0, r0.changes)

    def test_keyframe_snap(self):
        fm = fm24()
        evs = [E(start=1900, end=3000), E(start=3100, end=3900), E(start=4100, end=5900),
               E(start=600, end=900)]
        r = postprocess(evs, [0, 1, 2, 3], TimingOptions(keyframes=True), fm)
        frames = {k: (fm.frame_at_time(v["start"], START), fm.frame_at_time(v["end"], END))
                  for k, v in r.changes.items()}
        self.assertEqual(frames[0][0], 48)          # 2 kare sonraki anahtar kareye
        self.assertEqual(frames[1][1], 95)          # bitiş: 96. anahtar kareden hemen önce
        self.assertEqual(frames[2], (96, 143))
        self.assertNotIn(3, r.changes)              # eşik dışında
        # .ass'a yazılan (10 ms'ye yuvarlanan) zaman da aynı kareye düşmeli
        from osisub.core.asstime import round_ms
        self.assertEqual(fm.frame_at_time(round_ms(r.changes[0]["start"]), START), 48)

    def test_filters(self):
        evs = [E(start=0, end=1000, style="A"), E(start=0, end=1000, style="B"),
               E(start=0, end=1000, comment=True, style="A")]
        r = postprocess(evs, [0, 1, 2], TimingOptions(lead_out=100, styles={"A"}))
        self.assertEqual(set(r.changes), {0})


class TestFormats(unittest.TestCase):
    SRT = ("1\r\n00:00:01,000 --> 00:00:03,500\r\n<i>Merhaba</i> dünya\r\nikinci satır\r\n\r\n"
           "2\r\n00:00:04,250 --> 00:00:06,000\r\nTek satır\r\n")

    def test_srt_parse(self):
        ev = parse_srt(self.SRT)
        self.assertEqual([(e.start, e.end) for e in ev], [(1000, 3500), (4250, 6000)])
        self.assertEqual(ev[0].text, "{\\i1}Merhaba{\\i0} dünya\\Nikinci satır")

    def test_srt_roundtrip(self):
        ev = parse_srt(self.SRT)
        self.assertEqual(parse_srt(to_srt(ev))[0].text, ev[0].text)
        self.assertEqual(ass_to_html("{\\an8\\i1}a{\\i0}\\Nb"), "<i>a</i>\nb")

    def test_vtt(self):
        ev = parse_vtt("WEBVTT\n\n00:01.500 --> 00:03.000 align:start\n<v Ali>Selam</v>\n")
        self.assertEqual((ev[0].start, ev[0].end, ev[0].text), (1500, 3000, "Selam"))

    def test_cp1254_and_bom(self):
        d = tempfile.mkdtemp()
        p = os.path.join(d, "tr.srt")
        with open(p, "wb") as f:
            f.write("1\r\n00:00:01,000 --> 00:00:02,000\r\nŞüphe ğİ\r\n".encode("cp1254"))
        self.assertEqual(load_any(p).events[0].text, "Şüphe ğİ")
        with open(p, "wb") as f:
            f.write(b"\xef\xbb\xbf" + "1\n00:00:01,000 --> 00:00:02,000\nÇ\n".encode())
        self.assertEqual(load_any(p).events[0].text, "Ç")

    def test_font_color(self):
        self.assertEqual(html_to_ass('<font color="#FF8000">x</font>'), "{\\c&H0080FF&}x{\\c}")


class TestTransfer(unittest.TestCase):
    def setUp(self):
        self.en = [
            E(start=1000, end=3000, style="JoJo-main", actor="Form", text="{\\an8}Hello there"),
            E(start=3000, end=5000, style="JoJo-main", text="How are you"),
            E(start=5000, end=6000, style="JoJo-main", text="second half"),
            E(start=5000, end=9000, style="Sign", text="{\\pos(1,2)}SIGN"),
            E(start=10000, end=12000, style="JoJo-main", text="No TR here"),
            E(comment=True, start=0, end=1, text="not"),
        ]
        self.tr = [
            E(start=1050, end=2950, text="Merhaba"),
            E(start=3000, end=4000, text="Nasılsın"), E(start=4000, end=5000, text="iyi misin"),
            E(start=5100, end=6100, text="ikinci yarı"),
            E(start=13000, end=14000, text="Yetim satır"),
        ]

    def plan(self, **kw):
        return plan_transfer(self.en, self.tr, TransferOptions(host_styles={"JoJo-main"}, **kw))

    def test_basic_fill_keeps_style_actor_tags(self):
        p = self.plan()
        e0 = p.events[0]
        self.assertEqual((e0.text, e0.style, e0.actor, e0.start), ("{\\an8}Merhaba", "JoJo-main", "Form", 1000))
        self.assertEqual(p.events[1].text, "Nasılsın\\Niyi misin")
        self.assertEqual(p.events[3].text, "{\\pos(1,2)}SIGN")       # tabela stili dışarıda

    def test_unmatched_en_preserved_and_reported(self):
        p = self.plan()
        self.assertEqual(p.events[4].text, "No TR here")
        kinds = {(i.kind, i.row) for i in p.issues}
        self.assertIn(("no_ref", 4), kinds)
        self.assertEqual(sum(1 for e in p.events if e.comment), 1)

    def test_orphan_inserted_or_skipped(self):
        p = self.plan()
        orphan = [e for e in p.events if e.text == "Yetim satır"]
        self.assertEqual(len(orphan), 1)
        self.assertEqual(orphan[0].style, "JoJo-main")
        self.assertIn("orphan", {i.kind for i in p.issues})
        p2 = self.plan(insert_orphans=False)
        self.assertFalse(any(e.text == "Yetim satır" for e in p2.events))
        self.assertIn("orphan_skipped", {i.kind for i in p2.issues})
        self.assertEqual(len(p2.events), len(self.en))

    def test_multi_and_ambiguous(self):
        p = self.plan()
        self.assertEqual(p.stats["multi"], 1)
        tr = [E(start=2500, end=3500, text="ikisinin ortası")]
        p2 = plan_transfer(self.en, tr, TransferOptions(host_styles={"JoJo-main"}))
        self.assertIn("ambiguous", {i.kind for i in p2.issues})

    def test_options(self):
        p = self.plan(keep_leading_tags=False, keep_original=True, use_ref_timing=True)
        self.assertEqual(p.events[0].text, "Merhaba{EN: Hello there}")
        self.assertEqual((p.events[0].start, p.events[0].end), (1050, 2950))

    def test_never_deletes_en(self):
        p = self.plan(insert_orphans=False)
        self.assertEqual(len(p.events), len(self.en))
        for a, b in zip(self.en, p.events):
            self.assertEqual((a.style, a.actor, a.start, a.end, a.comment), (b.style, b.actor, b.start, b.end, b.comment))


if __name__ == "__main__":
    unittest.main()
