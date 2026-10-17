"""Terim sözlüğü, konuşma algılama (VAD) ve Whisper testleri."""

import os
import tempfile
import unittest

os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())

from osisub.core import qc as Q  # noqa: E402
from osisub.core.assfile import Event as E  # noqa: E402
from osisub.core.glossary import Glossary, Term, tr_fold  # noqa: E402
from tests.media_fixtures import have_ffmpeg, path  # noqa: E402


def glossary():
    return Glossary([Term("Stand", "Stand", ["Stant", "Ruh"]), Term("Hokage", "Hokage", ["Ateş Gölgesi"]),
                     Term("Nen", "Nen", ["nen gücü"], "büyük harf")])


class TestGlossary(unittest.TestCase):
    def test_tsv_roundtrip(self):
        p = os.path.join(tempfile.mkdtemp(), "terimler.tsv")
        g = glossary()
        g.save(p)
        g2 = Glossary.load(p)
        self.assertEqual([(t.source, t.target, t.forbidden, t.note) for t in g2.terms],
                         [(t.source, t.target, t.forbidden, t.note) for t in g.terms])
        with open(p, "a", encoding="utf-8") as f:
            f.write("# yorum\n\nTitan\tDev\n")
        self.assertEqual(Glossary.load(p).terms[-1].target, "Dev")

    def test_forbidden_with_suffix_and_case(self):
        g = glossary()
        self.assertEqual(g.fix_forbidden("Stantlar geldi"), "Standlar geldi")
        self.assertEqual(g.fix_forbidden("Bu ateş gölgesi'nin emri"), "Bu Hokage'nin emri")
        self.assertEqual(g.fix_forbidden("ISTANBUL"), "ISTANBUL")
        self.assertEqual(tr_fold("IŞIK İz"), "ışık iz")

    def test_missing_terms(self):
        g = glossary()
        self.assertEqual([t.source for t in g.missing_terms("The Hokage uses Nen", "Köy liderinin gücü")],
                         ["Hokage", "Nen"])
        self.assertEqual(g.missing_terms("The Hokage", "Hokage'nin emri"), [])

    def test_qc_rule(self):
        evs = [E(start=0, end=2000, text="{\\i1}Stantı{\\i0} çağır"), E(start=3000, end=5000, text="Köy lideri")]
        o = Q.QcOptions()
        o.glossary = glossary()
        o.ref_text = lambda i: ["Summon your Stand", "The Hokage"][i]
        iss = Q.run_qc(evs, [0, 1], {"glossary"}, o)
        self.assertEqual(len(iss), 2)
        self.assertEqual(iss[0].fix["text"], "{\\i1}Standı{\\i0} çağır")       # etiketler korunur
        self.assertIsNone(iss[1].fix)
        self.assertIn("Hokage", iss[1].detail)


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestVad(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media.audio import decode_audio
        from osisub.media.vad import speech_segments
        cls.ad = decode_audio(path("bursts.wav"), use_cache=False)
        cls.segs = speech_segments(cls.ad.pcm, cls.ad.rate)

    def test_segments_match_ground_truth(self):
        self.assertEqual(len(self.segs), 2)
        for (a, b), (ta, tb) in zip(self.segs, [(1000, 2500), (4000, 6000)]):
            self.assertLessEqual(abs(a - ta), 50)
            self.assertLessEqual(abs(b - tb), 60)

    def test_snap(self):
        from osisub.media.vad import snap_to_speech
        evs = [E(start=1300, end=2200), E(start=3700, end=5600), E(start=7000, end=7500)]
        ch = snap_to_speech(evs, [0, 1, 2], self.segs, 400, 500)
        self.assertEqual(ch[0]["start"], self.segs[0][0])
        self.assertEqual(ch[0]["end"], self.segs[0][1])
        self.assertEqual(ch[1], {"start": self.segs[1][0], "end": self.segs[1][1]})
        self.assertNotIn(2, ch)

    def test_lines_from_speech(self):
        from osisub.media.vad import lines_from_speech
        new = lines_from_speech(self.segs, [E(start=900, end=2600, text="var")], max_ms=1500)
        self.assertEqual(len(new), 2)                  # ilki dolu aralıkta; ikincisi 2000 ms -> 2 parça
        self.assertEqual(new[0].start, self.segs[1][0])
        self.assertEqual(new[-1].end, self.segs[1][1])


def _whisper_ready():
    try:
        import faster_whisper  # noqa: F401
    except ImportError:
        return False
    return have_ffmpeg() and os.path.exists(path("speech.wav"))


@unittest.skipUnless(_whisper_ready(), "faster-whisper ya da espeak-ng yok")
class TestWhisper(unittest.TestCase):
    def test_transcribe_lines(self):
        from osisub.core.asr import transcribe, words_to_lines
        from osisub.media.audio import decode_audio
        from osisub.media.vad import speech_segments
        ad = decode_audio(path("speech.wav"), use_cache=False)
        segs = speech_segments(ad.pcm, ad.rate)
        out = transcribe(ad.pcm, ad.rate, "en", "tiny", speech_segs=segs)
        text = " ".join(t for _s, _e, t in out).lower()
        self.assertIn("market", text)
        self.assertIn("tomorrow", text)
        last = out[-1]
        self.assertLessEqual(abs(last[1] - segs[-1][1]), 50)          # bitiş VAD'e yaslandı
        self.assertGreaterEqual(len(out), 2)
        lines = words_to_lines([(0, .5, " Merhaba."), (.6, 1.2, " Nasılsın?"), (3.0, 3.4, " Evet")], min_dur=0.4)
        self.assertEqual([ln[2] for ln in lines], ["Merhaba.", "Nasılsın?", "Evet"])


if __name__ == "__main__":
    unittest.main()
