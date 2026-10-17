"""Denetim (QC) kuralları, yazım denetimi ve sahne algılama testleri."""

import os
import tempfile
import unittest

os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())

from osisub.core import qc as Q  # noqa: E402
from osisub.core.assfile import Event as E  # noqa: E402
from osisub.core.spell import SpellChecker, find_dictionary  # noqa: E402
from tests.media_fixtures import have_ffmpeg, path  # noqa: E402

DICT = find_dictionary("tr_TR")


class TestFixers(unittest.TestCase):
    def test_spaces(self):
        self.assertEqual(Q.fix_double_space("a  b   c"), "a b c")
        self.assertEqual(Q.fix_edge_space("  Baş \\N  son  "), "Baş\\Nson")
        self.assertEqual(Q.fix_space_before_punct("dünya , nasılsın ?"), "dünya, nasılsın?")
        self.assertEqual(Q.fix_space_before_punct("Ama ... dur"), "Ama ... dur")      # üç nokta korunur
        self.assertEqual(Q.fix_space_after_punct("a,b;c bitti.Sonra"), "a, b; c bitti. Sonra")
        self.assertEqual(Q.fix_space_after_punct("3,5 kilo, saat 3.00"), "3,5 kilo, saat 3.00")

    def test_tags_untouched(self):
        t = "{\\pos(1,2)\\fs  40}Merhaba  dünya{\\i1}  ,x"
        self.assertEqual(Q.fix_double_space(t), "{\\pos(1,2)\\fs  40}Merhaba dünya{\\i1} ,x")
        self.assertEqual(Q.fix_double_space("{\\p1}m 0 0  l 1 1{\\p0}a  b"), "{\\p1}m 0 0  l 1 1{\\p0}a b")

    def test_dots_repeat_case(self):
        self.assertEqual(Q.fix_dots("Hmm.. bu....mu…"), "Hmm... bu...mu...")
        self.assertEqual(Q.fix_dots("Hmm..", Q.QcOptions(ellipsis="…")), "Hmm…")
        self.assertEqual(Q.fix_repeated_word("ve ve bunu Bunu yap"), "ve bunu yap")
        self.assertEqual(Q.fix_lowercase_start("{\\an8}ıslak"), "{\\an8}Islak")
        self.assertEqual(Q.fix_lowercase_start("istanbul"), "İstanbul")
        self.assertEqual(Q.fix_dash_space("-Selam\\N-Hey"), "- Selam\\N- Hey")
        self.assertEqual(Q.fix_linebreak_edge("\\Nmetin\\N\\Nson\\N"), "metin\\Nson")

    def test_turkish_case(self):
        self.assertEqual(Q.tr_upper("istanbul ılık"), "İSTANBUL ILIK")
        self.assertEqual(Q.tr_lower("İSTANBUL IŞIK"), "istanbul ışık")

    def test_words_positions(self):
        t = "{\\i1}Merhaba{\\i0} dünya\\NAli'nin {\\p1}m 0 0{\\p0}evi"
        w = Q.words_in(t)
        self.assertEqual([x[0] for x in w], ["Merhaba", "dünya", "Ali'nin", "evi"])
        for word, a, b in w:
            self.assertEqual(t[a:b], word)


class TestRules(unittest.TestCase):
    def events(self):
        return [
            E(start=0, end=300, text="kısa  satır"),                      # 0: çok kısa, çift boşluk, küçük harf
            E(start=1000, end=3000, text="Bu bir cümle"),                 # 1
            E(start=2900, end=4000, text="devam ediyor."),                # 2: çakışma (1 ile), küçük ama önceki bitmemiş
            E(start=4050, end=6000, text="sonraki."),                     # 3: 50 ms boşluk, küçük harf (önceki bitti)
            E(start=7000, end=8000, text="{\\pos(1,2)}"),                 # 4: boş
            E(start=9000, end=9500, text="Çok çok çok uzun uzun bir okuma hızı metni burada"),
            E(start=0, end=1, comment=True, text="yorum  satırı"),
        ]

    def test_run_and_kinds(self):
        evs = self.events()
        enabled = {r.id for r in Q.RULES} - {"spell"}
        iss = Q.run_qc(evs, list(range(len(evs))), enabled)
        got = {(i.row, i.rule) for i in iss}
        for exp in [(0, "too_short"), (0, "double_space"), (0, "lowercase_start"), (1, "overlap"),
                    (3, "lowercase_start"), (2, "gap"), (4, "empty"), (5, "cps"), (5, "repeated_word")]:
            self.assertIn(exp, got)
        self.assertNotIn((2, "lowercase_start"), got)       # önceki cümle noktayla bitmedi
        self.assertFalse(any(i.row == 6 for i in iss))       # yorum atlanır

    def test_apply_fixes_combined(self):
        evs = self.events()
        iss = Q.run_qc(evs, [0, 1], {"double_space", "lowercase_start", "too_short", "overlap"})
        ch = Q.apply_fixes(evs, iss)
        self.assertEqual(ch[0]["text"], "Kısa satır")
        self.assertEqual(ch[0]["end"], 700)
        self.assertEqual(ch[1]["end"], 2900)                 # çakışma giderildi


@unittest.skipUnless(DICT, "tr_TR sözlüğü yok (Araçlar > Türkçe sözlüğü indir)")
class TestSpell(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sp = SpellChecker(DICT, os.path.join(tempfile.mkdtemp(), "uw.txt"))
        assert cls.sp.load(), cls.sp.error

    def test_turkish_words(self):
        for w in ("kitaplarımızdan", "gidiyorum", "İSTANBUL", "Işık", "geliyormuşsun", "İstanbul'da",
                  "Ali'nin", "Naruto'yu", "TV", "şıp-şıp"):
            self.assertTrue(self.sp.check(w), w)
        for w in ("gidiyrum", "kitaplarmızdan", "Nasılsn"):
            self.assertFalse(self.sp.check(w), w)

    def test_suggest_keeps_suffix(self):
        self.assertIn("gidiyorum", self.sp.suggest("gidiyrum"))
        self.assertIn("Ankara'dan", self.sp.suggest("Ankaraa'dan"))

    def test_user_dict(self):
        self.assertFalse(self.sp.check("Sasukecik"))
        self.sp.add_word("Sasukecik")
        self.assertTrue(self.sp.check("Sasukecik"))
        with open(self.sp.user_path, encoding="utf-8") as f:
            self.assertIn("Sasukecik", f.read())

    def test_qc_spell_rule(self):
        evs = [E(start=0, end=2000, text="{\\i1}Bugün{\\i0} okula gidiyrum")]
        o = Q.QcOptions()
        o.spell = self.sp
        iss = Q.run_qc(evs, [0], {"spell"}, o)
        self.assertEqual([i.detail for i in iss], ["gidiyrum"])


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestScenes(unittest.TestCase):
    def test_detect_cuts(self):
        from osisub.media.frameindex import build_index
        from osisub.media.scenes import detect_scenes
        idx = build_index(path("cuts.mkv"))
        self.assertEqual(idx.keyframes, [0])                  # kodlayıcı kesmeleri işaretlemedi
        self.assertEqual(detect_scenes(path("cuts.mkv"), idx, use_cache=False), [48, 96, 144])
        self.assertEqual(detect_scenes(path("cfr.mkv"), build_index(path("cfr.mkv")), use_cache=False), [])

    def test_timing_snaps_to_scenes(self):
        import copy
        from osisub.core.timing import TimingOptions, postprocess
        from osisub.media.frameindex import build_index
        idx = build_index(path("cuts.mkv"))
        fm = copy.copy(idx.frame_map())
        fm.keyframes = sorted(set(fm.keyframes) | {48, 96, 144})
        evs = [E(start=1950, end=3900)]                          # kare 47 .. 93
        r = postprocess(evs, [0], TimingOptions(keyframes=True), fm)
        self.assertEqual(fm.frame_at_time(r.changes[0]["start"], 1), 48)
        self.assertEqual(fm.frame_at_time(r.changes[0]["end"], 2), 95)


if __name__ == "__main__":
    unittest.main()
