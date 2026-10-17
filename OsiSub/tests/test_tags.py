"""Etiket kütüphanesi testleri."""

import unittest

from osisub.core import tags as T
from osisub.core.assfile import Event, Style


class TestTags(unittest.TestCase):
    def test_tokens_and_get(self):
        t = "{\\an8\\fs40\\fscx120\\t(0,500,\\frz30\\fscx80)\\fr10\\pos(1.5,2)}Metin{\\i1}"
        self.assertEqual(T.get(t, "fs"), "40")
        self.assertEqual(T.get(t, "fscx"), "120")        # \t içindeki 80 değil
        self.assertEqual(T.get(t, "frz"), "10")          # \fr = \frz
        self.assertEqual(T.get_pos(t), (1.5, 2.0))
        self.assertIsNone(T.get(t, "i"))                 # yalnız ilk blok
        self.assertIsNone(T.get("Düz metin", "pos"))

    def test_set_replaces_all_and_aliases(self):
        t = "{\\fr5\\frz7\\bord2}x"
        t2 = T.set_tag(t, "frz", "45")
        self.assertEqual(t2, "{\\bord2\\frz45}x")
        self.assertEqual(T.set_tag("x", "pos", "(1,2)"), "{\\pos(1,2)}x")
        self.assertEqual(T.set_tag("{\\c&H00FF00&}x", "1c", "&H0000FF&"), "{\\1c&H0000FF&}x")
        self.assertEqual(T.remove("{\\pos(1,2)}x", "pos"), "x")
        # \t içine dokunulmaz
        t3 = T.set_tag("{\\t(\\frz90)}x", "frz", "10")
        self.assertEqual(t3, "{\\t(\\frz90)\\frz10}x")

    def test_move(self):
        t = "{\\move(10,20,30,40,100,900)}x"
        self.assertEqual(T.get_move(t), (10, 20, 30, 40, 100, 900))
        ev = Event(start=0, end=1000, text=t)
        self.assertEqual(T.position(ev, None, (640, 360), 500), (20.0, 30.0))
        self.assertEqual(T.set_move("x", 1, 2, 3, 4), "{\\move(1,2,3,4)}x")

    def test_clip_rect_and_vector(self):
        self.assertEqual(T.get_clip("{\\clip(1,2,30,40)}x"), ("rect", False, (1, 2, 30, 40)))
        kind, inv, (scale, d) = T.get_clip("{\\iclip(m 0 0 l 10 0 10 10)}x")
        self.assertEqual((kind, inv, scale), ("vector", True, 1))
        pts = T.parse_drawing(d)
        self.assertEqual(pts, [["m", 0, 0], ["l", 10, 0], ["l", 10, 10]])
        self.assertEqual(T.drawing_str(pts), "m 0 0 l 10 0 10 10")
        self.assertEqual(T.set_rect_clip("{\\iclip(1,1,2,2)}x", 50, 60, 5, 6), "{\\clip(5,6,50,60)}x")
        # b koşusundan nokta silinince l'ye düşer
        pts = T.parse_drawing("m 0 0 b 1 1 2 2 3 3")
        del pts[2]
        self.assertEqual(T.drawing_str(pts), "m 0 0 l 1 1 3 3")

    def test_default_pos(self):
        st = Style(align=2, margin_l=20, margin_r=40, margin_v=30)
        self.assertEqual(T.default_pos(Event(text="x"), st, (1920, 1080)), (950.0, 1050.0))
        self.assertEqual(T.default_pos(Event(text="{\\an7}x"), st, (1920, 1080)), (20.0, 30.0))
        self.assertEqual(T.default_pos(Event(text="{\\an6}x", margin_r=100), st, (1920, 1080)), (1820.0, 540.0))

    def test_fmt_and_angle(self):
        self.assertEqual(T.fmt(12.0), "12")
        self.assertEqual(T.fmt(12.345), "12.35")
        self.assertEqual(T.fmt(-0.001), "0")
        self.assertEqual(T.norm_angle(370), 10)
        self.assertEqual(T.norm_angle(-190), 170)


if __name__ == "__main__":
    unittest.main()
