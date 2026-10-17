"""Aegisub araçları: satır seç, yeniden örnekleme, imleçten böl, sürekli zaman, 1 kare kaydır, üstüne yapıştır,
anahtar kare dosyaları ve yazı tipi toplayıcı (gerçek sistem yazı tipleriyle eksik karakter denetimi)."""

import os
import tempfile
import time
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"
os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())
os.environ.setdefault("LOCALAPPDATA", tempfile.mkdtemp())
os.environ.setdefault("APPDATA", tempfile.mkdtemp())

from osisub.core import aegisub_tools as A  # noqa: E402
from osisub.core import fonts as F  # noqa: E402
from osisub.core.assfile import AssDocument, Event as E, Style  # noqa: E402
from osisub.core.framemap import END, START, FrameMap  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SAMPLE = os.path.join(HERE, "fixtures", "sample.ass")


class TestSelect(unittest.TestCase):
    def test_modes_and_actions(self):
        ev = [E(text="Merhaba Naruto", style="Default"), E(text="naruto?", style="Sign"),
              E(text="Sasuke", style="Default", actor="Ali"), E(text="NARUTO", comment=True)]
        S = A.SelectOptions
        self.assertEqual(A.select_rows(ev, set(), S("naruto")), {0, 1})
        self.assertEqual(A.select_rows(ev, set(), S("naruto", comments=True)), {0, 1, 3})
        self.assertEqual(A.select_rows(ev, set(), S("naruto", case=True)), {1})
        self.assertEqual(A.select_rows(ev, set(), S("Default", field="style", mode="exact")), {0, 2})
        self.assertEqual(A.select_rows(ev, set(), S(r"^S", mode="regex")), {2})
        self.assertEqual(A.select_rows(ev, set(), S("naruto", invert=True)), {2})
        self.assertEqual(A.select_rows(ev, {2}, S("naruto", action="add")), {0, 1, 2})
        self.assertEqual(A.select_rows(ev, {0, 2}, S("naruto", action="sub")), {2})
        self.assertEqual(A.select_rows(ev, {0, 2}, S("naruto", action="and")), {0})


class TestResample(unittest.TestCase):
    def doc(self):
        d = AssDocument.new()
        d.set_info("PlayResX", "1280")
        d.set_info("PlayResY", "720")
        d.ensure_section("styles").items = [Style(fontsize=48, outline=2, shadow=1, margin_l=20, margin_r=20,
                                                  margin_v=40, spacing=1)]
        d.ensure_section("events").items = [
            E(text="{\\pos(640,360)\\fs40\\bord2\\fscx120\\t(0,500,\\fs20\\blur1)}Yazı"),
            E(text="{\\move(0,0,100,50,0,1000)\\clip(10,20,110,220)}A"),
            E(text="{\\iclip(m 0 0 l 100 0 100 100 0 100)}B", margin_l=10, margin_v=8),
            E(text="{\\an7\\pos(0,0)\\p1}m 0 0 l 128 0 128 72{\\p0}"),
            E(text="{\\clip(2,m 0 0 l 64 64)}C"),
        ]
        return d

    def test_scale(self):
        d = self.doc()
        styles, evs, rx, ry = A.resample_doc(d, A.ResampleOptions(1920, 1080))
        self.assertEqual((rx, ry), (1.5, 1.5))
        st = styles[0]
        self.assertEqual((st.fontsize, st.outline, st.shadow, st.margin_l, st.margin_v, st.spacing),
                         (72, 3, 1.5, 30, 60, 1.5))
        self.assertEqual(evs[0].text, "{\\pos(960,540)\\fs60\\bord3\\fscx120\\t(0,500,\\fs30\\blur1.5)}Yazı")
        self.assertEqual(evs[1].text, "{\\move(0,0,150,75,0,1000)\\clip(15,30,165,330)}A")
        self.assertEqual(evs[2].text, "{\\iclip(m 0 0 l 150 0 150 150 0 150)}B")
        self.assertEqual((evs[2].margin_l, evs[2].margin_v), (15, 12))
        self.assertEqual(evs[3].text, "{\\an7\\pos(0,0)\\p1}m 0 0 l 192 0 192 108{\\p0}")
        self.assertEqual(evs[4].text, "{\\clip(2,m 0 0 l 96 96)}C")
        # en/boy farklı: x ve y ayrı
        _s, evs2, rx2, ry2 = A.resample_doc(d, A.ResampleOptions(1920, 1440))
        self.assertEqual(evs2[0].text.split("\\fs")[0], "{\\pos(960,720)")
        # yalnız konum (boyutlar kapalı)
        _s, evs3, _x, _y = A.resample_doc(d, A.ResampleOptions(1920, 1080, sizes=False, margins=False))
        self.assertIn("\\fs40\\bord2", evs3[0].text)
        self.assertEqual(evs3[2].margin_l, 10)


class TestLineOps(unittest.TestCase):
    def test_split_at(self):
        ev = E(start=1000, end=5000, text="{\\an8}Birinci cümle.\\NIkinci uzun cümle burada.")
        pos = ev.text.index("\\N")
        a, b = A.split_at(ev, pos, "estimate")
        self.assertEqual((a.text, b.text), ("{\\an8}Birinci cümle.", "{\\an8}Ikinci uzun cümle burada."))
        self.assertTrue(1000 < a.end == b.start < 5000)
        a, b = A.split_at(ev, pos, "preserve")
        self.assertEqual((a.start, a.end, b.start, b.end), (1000, 5000, 1000, 5000))
        a, b = A.split_at(ev, pos, "video", split_ms=2000)
        self.assertEqual((a.end, b.start), (2000, 2000))
        self.assertIsNone(A.split_at(E(text=""), 0))
        a, b = A.split_at(ev, 3)                                 # etiket içi → bloğun sonuna
        self.assertEqual(a.text, "{\\an8}")

    def test_continuous(self):
        ev = [E(start=0, end=1000), E(start=1500, end=2500), E(start=2400, end=3000)]
        self.assertEqual(A.make_continuous(ev, [0, 1, 2], "start"), {1: {"start": 1000}, 2: {"start": 2500}})
        self.assertEqual(A.make_continuous(ev, [0, 1, 2], "end"), {0: {"end": 1500}, 1: {"end": 2400}})

    def test_duplicate_shift(self):
        fm = FrameMap.from_fps(24000 / 1001, 1000)
        ev = [E(start=fm.time_at_frame(10, START), end=fm.time_at_frame(10, END), text="x")]
        (pos, dup), = A.duplicate_shift(ev, [0], fm)
        self.assertEqual(pos, 1)
        self.assertEqual((fm.frame_at_time(dup.start, START), fm.frame_at_time(dup.end, END)), (11, 11))

    def test_paste_over(self):
        ev = [E(start=0, end=1000, text="a", style="Default"), E(start=1000, end=2000, text="b")]
        pasted = A.parse_clipboard_lines("bir\r\niki\r\n\r\nüç", ["Layer", "Start", "End", "Style", "Name", "MarginL",
                                                                  "MarginR", "MarginV", "Effect", "Text"])
        self.assertEqual([p.text for p in pasted], ["bir", "iki", "üç"])
        self.assertEqual(A.paste_over(ev, [0, 1], pasted, {"text"}), {0: {"text": "bir"}, 1: {"text": "iki"}})
        fmt = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
        full = A.parse_clipboard_lines("Dialogue: 0,0:00:05.00,0:00:06.00,Sign,Ali,0,0,0,,X", fmt)
        ch = A.paste_over(ev, [1], full, {"start", "end", "style", "actor"})
        self.assertEqual(ch, {1: {"start": 5000, "end": 6000, "style": "Sign", "actor": "Ali"}})


class TestKeyframes(unittest.TestCase):
    def test_formats(self):
        self.assertEqual(A.load_keyframes("# keyframe format v1\nfps 23.976\n0\n48\n96\n"), [0, 48, 96])
        xvid = "# XviD 2pass stat file\n# comment\ni 1 2 3\np 1 2 3\nb 1\ni 9\n"
        self.assertEqual(A.load_keyframes(xvid), [0, 3])
        x264 = "#options: 640x360 fps=24000/1001\nin:0 out:0 type:I dur:2\nin:1 out:2 type:P dur:2\nin:2 out:1 type:i dur:2\n"
        self.assertEqual(A.load_keyframes(x264), [0, 2])
        self.assertEqual(A.load_keyframes("5\n0\n5\n"), [0, 5])
        with self.assertRaises(ValueError):
            A.load_keyframes("bu bir şey değil")
        out = A.save_keyframes([96, 0, 48], 24000 / 1001)
        self.assertTrue(out.startswith("# keyframe format v1\nfps 23.976\n"))
        self.assertEqual(A.load_keyframes(out), [0, 48, 96])


class TestFonts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.index = F.FontIndex(F.system_font_dirs())
        cls.have = {n for f in cls.index.faces for n in f.names}

    def test_used_fonts(self):
        d = AssDocument.new()
        d.ensure_section("styles").items = [Style(name="Default", fontname="Arial"),
                                            Style(name="Sign", fontname="Verdana", bold=True)]
        d.ensure_section("events").items = [
            E(text="Ab{\\fn@DejaVu Sans\\b1}ş{\\r}c{\\rSign}d{\\i1}e"),
            E(text="{\\p1}m 0 0 l 1 1{\\p0}", style="Sign"),
            E(text="gizli", comment=True, style="Sign"),
        ]
        u = F.used_fonts(d)
        self.assertEqual(set(u), {("arial", False, False), ("dejavu sans", True, False), ("verdana", True, False),
                                  ("verdana", True, True)})
        self.assertEqual(u[("arial", False, False)].chars, {"A", "b", "c"})
        self.assertEqual(u[("dejavu sans", True, False)].chars, {"ş"})
        self.assertEqual(u[("verdana", True, True)].chars, {"e"})

    def test_collect_and_missing_glyphs(self):
        if "dejavu sans" not in self.have:
            self.skipTest("DejaVu Sans yok")
        d = AssDocument.new()
        d.ensure_section("styles").items = [Style(name="Default", fontname="DejaVu Sans")]
        evs = [E(text="Türkçe: ğüşıöç İĞÜŞÖÇ"), E(text="{\\fnYokBöyleBirFont}x")]
        lacking = [f for f in self.index.faces if f.cmap and not f.has(ord("ş")) and f.has(ord("a"))]
        if lacking:
            evs.append(E(text="{\\fn" + lacking[0].family + "}aşğ"))
        d.ensure_section("events").items = evs
        rep = {r.use.name.casefold(): r for r in F.collect(d, self.index)}
        self.assertTrue(rep["dejavu sans"].ok)
        self.assertIsNone(rep["yokböylebirfont"].face)
        if lacking:
            r = rep[lacking[0].family.casefold()]
            self.assertIsNotNone(r.face)
            self.assertIn("ş", r.missing)
        out = tempfile.mkdtemp()
        copied = F.copy_fonts(list(rep.values()), out)
        self.assertTrue(copied and all(os.path.exists(p) for p in copied))
        z = os.path.join(out, "f.zip")
        self.assertEqual(len(F.zip_fonts(list(rep.values()), z)), len(copied))

    def test_cache_roundtrip(self):
        cache = os.path.join(tempfile.mkdtemp(), "fonts.json")
        a = F.FontIndex(F.system_font_dirs(), cache)
        t = time.time()
        b = F.FontIndex(F.system_font_dirs(), cache)
        self.assertLess(time.time() - t, 5)
        self.assertEqual(len(a.faces), len(b.faces))
        if a.faces:
            fa, fb = a.faces[0], b.faces[0]
            self.assertEqual((fa.names, fa.bold, fa.weight, fa.cmap[:3]), (fb.names, fb.bold, fb.weight, fb.cmap[:3]))


class TestDialogs(unittest.TestCase):
    def setUp(self):
        from PySide6.QtWidgets import QApplication

        from osisub.config import Config
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        self.app = QApplication.instance() or QApplication([])
        cfg = Config(os.path.join(tempfile.mkdtemp(), "c.json"))
        cfg["load_aegisub_scripts"] = False
        cfg["auto_load_media"] = False
        self.ctx = Context(cfg)
        self.win = MainWindow(self.ctx, headless=True)
        self.win.show()
        self.win.open_path(SAMPLE, force=True)

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        self.app.processEvents()

    def test_select_resample_paste_split(self):
        w, ctx = self.win, self.ctx
        dlg = w.show_select_lines()
        dlg.pattern.setText("satır")
        self.assertEqual(dlg.apply(), {1, 4})
        self.assertEqual(ctx.selection, {1, 4})
        # yeniden örnekle 1920x1080 -> 1280x720, tek geri al
        dlg = w.show_resample()
        dlg.w.setValue(1280)
        dlg.h.setValue(720)
        dlg.apply()
        self.assertEqual(ctx.doc.play_res(), (1280, 720))
        self.assertEqual(ctx.doc.events[3].text, "{\\pos(640,66.667)\\fs53.333}TABELA")
        self.assertAlmostEqual(ctx.doc.styles[0].fontsize, 48)
        ctx.do_undo()
        self.assertEqual(ctx.doc.play_res(), (1920, 1080))
        self.assertFalse(ctx.undo.modified)
        # üstüne yapıştır (düz metin -> yalnız Metin)
        ctx.set_selection({0, 1}, 0)
        dlg = w.paste_over("Çeviri bir\nÇeviri iki")
        self.assertFalse(dlg.full)
        self.assertEqual(dlg.fields(), {"text"})
        self.assertEqual(dlg.apply(), 2)
        self.assertEqual(ctx.doc.events[1].text, "Çeviri iki")
        self.assertEqual(ctx.doc.events[1].start, 4000)
        # imleçten böl (düzenleme kutusundaki imleç)
        ctx.set_active(4, select_only=True)
        self.app.processEvents()
        c = w.editbox.text.textCursor()
        c.setPosition(ctx.doc.events[4].text.index("\\N"))
        w.editbox.text.setTextCursor(c)
        n = len(ctx.doc.events)
        w.split_at_cursor("estimate")
        self.assertEqual(len(ctx.doc.events), n + 1)
        self.assertEqual(ctx.doc.events[4].text, "Kısa satır")
        self.assertEqual(ctx.active, 5)
        # yer değiştir + sürekli zaman
        ctx.set_selection({0, 1}, 0)
        self.assertTrue(w.swap_lines())
        self.assertEqual(ctx.doc.events[0].text, "Çeviri iki")
        ctx.update_events({0: {"start": 0, "end": 1000}, 1: {"start": 1500, "end": 2000}}, "boşluk")
        ctx.set_selection({0, 1}, 0)
        self.assertEqual(w.make_continuous("end"), 1)
        self.assertEqual(ctx.doc.events[0].end, 1500)
        self.assertIn("Zamanları sürekli", ctx.undo.undo_desc())

    def test_fonts_dialog(self):
        dlg = self.win.show_fonts()
        names = {dlg.tree.topLevelItem(i).text(0) for i in range(dlg.tree.topLevelItemCount())}
        self.assertEqual(names, {"Arial", "Arial Black"})          # örnek dosyanın stilleri (Karaoke satırı dahil)
        self.assertIn("yazı tipi kullanımı", dlg.summary.text())
        dlg._select_rows(dlg.tree.topLevelItem(0))
        self.assertTrue(self.ctx.selection)

    def test_duplicate_shift_needs_video(self):
        self.assertEqual(self.win.duplicate_shift(), 0)
        self.ctx.frame_map = FrameMap.from_fps(24000 / 1001, 2000)
        self.ctx.set_selection({0, 1}, 0)
        n = len(self.ctx.doc.events)
        self.assertEqual(self.win.duplicate_shift(), 2)
        self.assertEqual(len(self.ctx.doc.events), n + 2)
        self.assertEqual(self.ctx.selection, {1, 3})
        fm = self.ctx.frame_map
        self.assertEqual(fm.frame_at_time(self.ctx.doc.events[1].start, START),
                         fm.frame_at_time(self.ctx.doc.events[0].start, START) + 1)


class TestKeyframeFiles(unittest.TestCase):
    def test_load_save_with_video(self):
        from tests.media_fixtures import have_ffmpeg, path
        if not have_ffmpeg():
            self.skipTest("ffmpeg yok")
        from tests.test_video_ui import make, pump
        win, ctx, _ass = make(False)
        try:
            idx = win.media.index
            self.assertEqual(idx.keyframes[:3], [0, 48, 96])
            d = tempfile.mkdtemp()
            p = win.save_keyframes(os.path.join(d, "kf.txt"))
            with open(p, encoding="utf-8") as f:
                self.assertTrue(f.read().startswith("# keyframe format v1"))
            kf2 = os.path.join(d, "kf2.txt")
            with open(kf2, "w", encoding="utf-8") as f:
                f.write("# keyframe format v1\nfps 0\n0\n30\n31\n500000\n")
            self.assertEqual(win.open_keyframes(kf2), 3)             # video dışı kare atılır
            self.assertEqual(ctx.frame_map.keyframes, [0, 30, 31])
            win.media.scenes = [60, 120]
            self.assertEqual(win.keyframes_from_scenes(), 2)
            self.assertEqual(win.media.index.keyframes, [0, 30, 31, 60, 120])
            pump(0.1)
            _ = path
        finally:
            win.close()


if __name__ == "__main__":
    unittest.main()
