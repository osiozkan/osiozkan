"""Çekirdek testleri: bayt koruma, zaman, CPL, geri al, kare eşlemesi."""

import os
import unittest

from osisub.core.assfile import AssDocument, Event, Style
from osisub.core.asstime import format_time, parse_time, round_ms
from osisub.core.framemap import END, EXACT, START, FrameMap
from osisub.core.textutil import (STATUS_ERROR, STATUS_OK, STATUS_WARN, char_count, cpl_info,
                                  cps_chars, strip_tags)
from osisub.core.undo import UndoStack

HERE = os.path.dirname(os.path.abspath(__file__))
SAMPLE = os.path.join(HERE, "fixtures", "sample.ass")


class TestRoundTrip(unittest.TestCase):
    def test_sample_byte_identical(self):
        data = open(SAMPLE, "rb").read()
        self.assertEqual(AssDocument.from_bytes(data).to_bytes(), data)

    def test_lf_no_bom_no_final_newline(self):
        data = b"[Script Info]\nTitle: a\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,x"
        d = AssDocument.from_bytes(data)
        self.assertFalse(d.bom)
        self.assertEqual(d.newline, "\n")
        self.assertEqual(d.to_bytes(), data)

    def test_marked_field_preserved(self):
        txt = ("[Events]\r\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\n"
               "Dialogue: Marked=0,0:00:01.00,0:00:02.00,Default,NTP,0000,0000,0000,!Effect,Metin, virgüllü\r\n")
        d = AssDocument.from_text(txt)
        ev = d.events[0]
        self.assertEqual(ev.text, "Metin, virgüllü")
        self.assertEqual(ev.extra, (("Marked", "Marked=0"),))
        self.assertEqual(d.to_text(), txt)
        d.events[0] = ev.replace(text="Yeni")
        # Osi kuralı: değişmeyen alanlar (Marked, 0000 kenarlar) bayt bayt korunur
        self.assertIn("Dialogue: Marked=0,0:00:01.00,0:00:02.00,Default,NTP,0000,0000,0000,!Effect,Yeni", d.to_text())
        d.events[0] = ev.replace(margin_l=12, start=1500)
        self.assertIn("Dialogue: Marked=0,0:00:01.50,0:00:02.00,Default,NTP,12,0000,0000,!Effect,", d.to_text())

    def test_only_edited_line_changes(self):
        data = open(SAMPLE, "rb").read()
        d = AssDocument.from_bytes(data)
        d.events[0] = d.events[0].replace(text="değişti")
        a = data.decode("utf-8-sig").split("\r\n")
        b = d.to_bytes().decode("utf-8-sig").split("\r\n")
        diff = [i for i, (x, y) in enumerate(zip(a, b)) if x != y]
        self.assertEqual(len(diff), 1)
        self.assertEqual(b[diff[0]], "Dialogue: 0,0:00:01.00,0:00:03.50,Default,Ali,0000,0000,0000,,değişti")

    def test_revert_returns_raw(self):
        d = AssDocument.load(SAMPLE)
        ev = d.events[0]
        d.events[0] = ev.replace(text="x").replace(text=ev.text)
        self.assertEqual(d.to_bytes(), open(SAMPLE, "rb").read())

    def test_comments_inside_events_kept(self):
        d = AssDocument.load(SAMPLE)
        self.assertEqual(d.events[2].lead, ("; arada yorum satırı",))
        self.assertTrue(d.events[2].comment)

    def test_garbage_and_info(self):
        d = AssDocument.load(SAMPLE)
        self.assertEqual(d.garbage()["Video File"], "ornek.mkv")
        self.assertEqual(d.play_res(), (1920, 1080))
        d.set_info("Title", "Yeni")
        self.assertIn("Title: Yeni", d.to_text())
        d.set_info("Eklenen", "1")
        self.assertIn("Eklenen: 1", d.to_text())

    def test_style_parse_and_serialize(self):
        d = AssDocument.load(SAMPLE)
        st = d.styles[1]
        self.assertEqual((st.name, st.fontname, st.fontsize, st.spacing, st.encoding), ("Sign", "Arial Black", 60, 1.5, 162))
        st2 = st.replace(fontsize=61.25, bold=True)
        line = st2.serialize(d.ensure_section("styles").format_fields)
        self.assertTrue(line.startswith("Style: Sign,Arial Black,61.25,"))
        self.assertIn(",-1,0,0,0,", line)

    def test_new_document(self):
        d = AssDocument.new()
        d2 = AssDocument.from_bytes(d.to_bytes())
        self.assertEqual(len(d2.events), 1)
        self.assertEqual(d2.styles[0].name, "Default")

    def test_clone_isolated(self):
        d = AssDocument.load(SAMPLE)
        c = d.clone()
        c.events[0] = c.events[0].replace(text="kopya")
        self.assertNotEqual(d.events[0].text, "kopya")


class TestTime(unittest.TestCase):
    def test_parse_format(self):
        self.assertEqual(parse_time("0:01:02.34"), 62340)
        self.assertEqual(parse_time("1:00:00.00"), 3600000)
        self.assertEqual(parse_time("0:00:01.5"), 1500)
        self.assertEqual(format_time(62340), "0:01:02.34")
        self.assertEqual(format_time(-5), "0:00:00.00")

    def test_round_like_aegisub(self):
        self.assertEqual(round_ms(1234), 1230)
        self.assertEqual(round_ms(1235), 1240)
        self.assertEqual(round_ms(-1), 0)


class TestCpl(unittest.TestCase):
    def test_rules(self):
        self.assertEqual(cpl_info("a" * 42)["status"], STATUS_OK)
        self.assertEqual(cpl_info("a" * 43)["status"], STATUS_WARN)
        self.assertEqual(cpl_info("a" * 30 + "\\N" + "b" * 42)["status"], STATUS_OK)
        info = cpl_info("a" * 43 + "\\N" + "b" * 16)
        self.assertEqual(info["status"], STATUS_ERROR)
        self.assertEqual(info["label"], "43+16")
        self.assertEqual(info["total"], 59)
        self.assertEqual(info["longest"], 43)

    def test_turkish_and_tags(self):
        self.assertEqual(char_count("ĞÜŞİÖÇğüşıöç"), 12)
        # ayrışık (NFD) İ ve ğ da tek karakter sayılır
        self.assertEqual(char_count("İğ"), 2)
        self.assertEqual(cpl_info("{\\an8}Merhaba, dünya!")["longest"], 15)
        self.assertEqual(cpl_info("a\\hb")["longest"], 3)
        self.assertEqual(strip_tags("{\\p1}m 0 0 l 10 10{\\p0}metin"), "metin")

    def test_cps_chars(self):
        self.assertEqual(cps_chars("Merhaba, dünya!"), 12)


class TestUndo(unittest.TestCase):
    def test_undo_redo_coalesce(self):
        d = AssDocument.load(SAMPLE)
        u = UndoStack(d)
        orig = d.events[0].text
        for t in ("a", "ab", "abc"):
            d.events[0] = d.events[0].replace(text=t)
            u.commit("Metin", coalesce="text:0")
        self.assertEqual(len(u.states), 2)
        d.events[1] = d.events[1].replace(text="x")
        u.commit("Diğer")
        u.undo()
        self.assertEqual(d.events[1].text, "Bu satır kırk üç karakterden daha uzun, değil mi?")
        self.assertEqual(d.events[0].text, "abc")
        u.undo()
        self.assertEqual(d.events[0].text, orig)
        self.assertFalse(u.modified)
        u.redo()
        self.assertEqual(d.events[0].text, "abc")
        self.assertTrue(u.modified)

    def test_saved_point_breaks_coalesce(self):
        d = AssDocument.load(SAMPLE)
        u = UndoStack(d)
        d.events[0] = d.events[0].replace(text="a")
        u.commit("Metin", coalesce="k")
        u.mark_saved()
        d.events[0] = d.events[0].replace(text="ab")
        u.commit("Metin", coalesce="k")
        self.assertTrue(u.modified)
        u.undo()
        self.assertEqual(d.events[0].text, "a")


class TestFrameMap(unittest.TestCase):
    def setUp(self):
        self.fm = FrameMap.from_fps(24000 / 1001, 1000)

    def test_cfr_values(self):
        fm = self.fm
        self.assertEqual(fm.time_at_frame(0, EXACT), 0)
        self.assertEqual(fm.time_at_frame(24, EXACT), 1001)
        self.assertEqual(fm.frame_at_time(1001, EXACT), 24)
        self.assertEqual(fm.frame_at_time(1000, EXACT), 23)

    def test_start_roundtrip(self):
        fm = self.fm
        for f in range(1, 500):
            self.assertEqual(fm.frame_from_ms(fm.ms_from_frame(f)), f)

    def test_start_end_semantics(self):
        fm = self.fm
        # Satır, kare sınırında başlarsa o kare görünür
        self.assertEqual(fm.frame_at_time(1001, START), 24)
        self.assertEqual(fm.frame_at_time(1001, END), 23)

    def test_vfr_list(self):
        ts = [0, 42, 83, 125, 167, 300, 333]
        fm = FrameMap(ts)
        self.assertEqual(fm.frame_at_time(200, EXACT), 4)
        self.assertEqual(fm.time_at_frame(5, START), 167 + (300 - 167 + 1) // 2)


if __name__ == "__main__":
    unittest.main()
