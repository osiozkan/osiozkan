"""Renk modeliyle maske (hairtrack): yer gerçeği, çizgi-sanat koruması, libass yakma, bölme akışı."""

import os
import subprocess
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"

import numpy as np  # noqa: E402

from tests.media_fixtures import have_ffmpeg  # noqa: E402

try:
    import cv2
    HAVE_CV = True
except ImportError:
    HAVE_CV = False


def _fill(polys, shape=(360, 640)):
    m = np.zeros(shape, np.uint8)
    for poly in polys:
        cv2.fillPoly(m, [np.round(np.array(poly) - 0.5).astype(np.int32)], 1)
    return m


def _core(mf, k):
    m = np.zeros((360, 640), np.uint8)
    cv2.polylines(m, [mf.hair_polyline(k)], False, 1, 18, cv2.LINE_8)
    return m


@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestColorMask(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        from tests import media_fixtures as mf
        cls.mf = mf
        cls.path = mf.path("hair.mkv")
        cls.idx = build_index(cls.path)
        cls.g = FrameGrabber(cls.path, cls.idx)
        x, y, w, h = mf.HAIR_SIGN
        cls.crop = (x - 20, y - 20, w + 40, h + 40)
        cls.cm = np.zeros((360, 640), np.uint8)
        cls.cm[y - 20:y + h + 20, x - 20:x + w + 20] = 1
        cls.seed = 30

    @classmethod
    def tearDownClass(cls):
        cls.g.close()

    def seeds(self, pos_thick=8):
        mf = self.mf
        pos = np.zeros((360, 640), np.uint8)
        cv2.polylines(pos, [mf.hair_polyline(self.seed)], False, 1, pos_thick)
        neg = np.zeros_like(pos)
        x, y, w, h = mf.HAIR_SIGN
        cv2.rectangle(neg, (x + 6, y + 6), (x + w - 6, y + h - 6), 1, 4)
        cv2.line(neg, (20, 300), (120, 330), 1, 6)
        neg &= (cv2.dilate(mf.hair_mask(self.seed), np.ones((9, 9), np.uint8)) == 0)
        return pos, neg

    def test_masks_against_truth(self):
        from osisub.media import colormask
        pos, neg = self.seeds()
        res, model = colormask.track_masks(self.g.get, 0, self.mf.HAIR_N - 1, self.seed, pos, neg, self.crop)
        self.assertEqual(sorted(res.frames), list(range(self.mf.HAIR_N)))
        checked = 0
        for k in range(self.mf.HAIR_N):
            core = _core(self.mf, k) & self.cm
            full = self.mf.hair_mask(k) & self.cm
            dm = _fill(res.frames[k].polys) & self.cm
            if core.sum() > 300:
                checked += 1
                self.assertGreater((core & dm).sum() / core.sum(), 0.95, k)          # telin içi bulunur
                self.assertLess((dm & (cv2.dilate(full, np.ones((5, 5), np.uint8)) == 0)).sum(), 60, k)
            elif full.sum() == 0:
                self.assertEqual(dm.sum(), 0, k)                                       # tel yokken boş
        self.assertGreater(checked, 40)
        # kırpma dışında maske yok
        for k in (30, 50):
            self.assertEqual((_fill(res.frames[k].polys) & (self.cm == 0)).sum(), 0)

    def test_lineart_guard_and_errors(self):
        from osisub.media import colormask
        # pozitif tohum telin siyah kenarını da kapsıyor: siyah mod atılmalı, tabelanın yazısı maskeye girmemeli
        pos, neg = self.seeds(pos_thick=22)
        res, model = colormask.track_masks(self.g.get, 20, 60, self.seed, pos, neg, self.crop)
        self.assertTrue(any("çizgi-sanat: 1" in n for n in res.notes), res.notes)
        k = 50
        dm = _fill(res.frames[k].polys)
        outside = dm & (cv2.dilate(self.mf.hair_mask(k), np.ones((7, 7), np.uint8)) == 0)
        self.assertLess(outside.sum(), 1000)
        img = self.g.get(self.seed)
        lab = colormask._lab(img)
        with self.assertRaises(ValueError):
            colormask.ColorModel.fit(lab, np.zeros((360, 640), np.uint8), neg)
        with self.assertRaises(ValueError):                    # aynı renk iki tarafta
            colormask.ColorModel.fit(lab, neg, neg)
        with self.assertRaises(ValueError):
            colormask.track_masks(self.g.get, 0, 10, 30, pos, neg)

    def test_burn_static_sign_iclip(self):
        """Duran tabelanın üstündeki satır, kimlik iziyle yalnız \\iclip alır; libass yakmasında tel önde kalır."""
        from osisub.core import motion as M
        from osisub.core.assfile import Event, Style
        from osisub.core.framemap import START, FrameMap
        from osisub.media import colormask
        from tests.test_motion import _doc_with
        pos, neg = self.seeds()
        res, _m = colormask.track_masks(self.g.get, 0, self.mf.HAIR_N - 1, self.seed, pos, neg, self.crop)
        fm = FrameMap(self.idx.ms)
        x, y, w, h = self.mf.HAIR_SIGN
        ev = Event(start=0, end=fm.time_at_frame(self.mf.HAIR_N, START),
                   text="{\\an7\\pos(%d,%d)\\bord0\\shad0\\1c&H00FF00&\\p1}m 0 0 l %d 0 %d %d 0 %d"
                   % (x + 10, y + 10, w - 20, w - 20, h - 20, h - 20))
        ident = M.MotionTrack([M.TrackFrame(k) for k in range(self.mf.HAIR_N)], 0, 640, 360)
        lines, warns = M.apply_track(ev, Style(), ident, fm, (640, 360), (640, 360),
                                     M.ApplyOptions(occlusion=res, mode="compact"))
        self.assertEqual(warns, [])
        self.assertLess(len(lines), self.mf.HAIR_N + 1)
        d = tempfile.mkdtemp()
        sub = os.path.join(d, "s.ass")
        fmt = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
        with open(sub, "w", encoding="utf-8") as fh:
            fh.write(_doc_with([]).to_text().rstrip("\r\n") + "\n")
            for ln in lines:
                fh.write(ln.serialize(fmt) + "\n")
        raw = subprocess.run(["ffmpeg", "-v", "error", "-i", self.path, "-vf", f"ass={sub}", "-f", "rawvideo",
                              "-pix_fmt", "rgb24", "-"], capture_output=True, check=True).stdout
        fr = np.frombuffer(raw, np.uint8).reshape(-1, 360, 640, 3)
        for k in range(self.mf.HAIR_N):
            im = fr[k].astype(int)
            g = (im[..., 1] > 200) & (im[..., 0] < 90) & (im[..., 2] < 90)
            core = cv2.erode(_core(self.mf, k), np.ones((3, 3), np.uint8)) > 0
            self.assertLessEqual(int((g & core).sum()), 3, k)                 # tel önde: satır görünmüyor
            self.assertGreater(int(g.sum()), 5000, k)                         # satırın geri kalanı duruyor


@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestColorMaskPanel(unittest.TestCase):
    def test_seed_tool_and_apply_without_track(self):
        from PySide6.QtCore import QPoint, Qt
        from PySide6.QtTest import QTest
        from PySide6.QtWidgets import QApplication

        from osisub.config import Config
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        from tests import media_fixtures as mf
        from tests.test_motion import _doc_with
        from tests.test_video_ui import pump
        QApplication.instance() or QApplication([])
        os.environ.pop("OSISUB_MPV_VO", None)
        d = tempfile.mkdtemp()
        ass = os.path.join(d, "sac.ass")
        x, y, w, h = mf.HAIR_SIGN
        with open(ass, "w", encoding="utf-8") as fh:
            fh.write(_doc_with([]).to_text().rstrip("\r\n") + "\n")
            fh.write("Dialogue: 0,0:00:00.00,0:00:03.00,Default,,0,0,0,,{\\an5\\pos(%d,%d)}SIGN\n"
                     % (x + w // 2, y + h // 2))
        cfg = Config(os.path.join(tempfile.mkdtemp(), "c.json"))
        cfg["load_aegisub_scripts"] = False
        cfg["auto_load_media"] = False
        ctx = Context(cfg)
        win = MainWindow(ctx, headless=True)
        win.resize(1500, 1000)
        win.show()
        try:
            win.open_path(ass, force=True)
            win.media.open_video(mf.path("hair.mkv"), with_audio=False)
            self.assertTrue(pump(until=lambda: win.media.index is not None, timeout=30))
            mp = win.motion_panel
            ctx.set_active(0, select_only=True)
            win.video.seek_frame(30)
            pump(0.2)
            mp.range_from_lines()
            mp.set_roi((x - 20, y - 20, w + 40, h + 40))
            mp.draw_seeds()
            self.assertEqual(win.video.tool, "seed")
            cv = win.video.canvas
            pl = mf.hair_polyline(30)
            pts = [cv.vid_to_widget(float(px), float(py)) for px, py in pl[3:30:3]]
            QTest.mousePress(cv, Qt.LeftButton, Qt.NoModifier, QPoint(int(pts[0].x()), int(pts[0].y())))
            for q in pts[1:]:
                QTest.mouseMove(cv, QPoint(int(q.x()), int(q.y())))
            QTest.mouseRelease(cv, Qt.LeftButton, Qt.NoModifier, QPoint(int(pts[-1].x()), int(pts[-1].y())))
            # negatif: tabelanın telden uzak sol ve sağ kenarı (yazı + beyaz)
            a = cv.vid_to_widget(x + 12, y + 12)
            b = cv.vid_to_widget(x + 12, y + h - 12)
            c = cv.vid_to_widget(x + 25, y + h - 12)
            QTest.mousePress(cv, Qt.RightButton, Qt.NoModifier, QPoint(int(a.x()), int(a.y())))
            QTest.mouseMove(cv, QPoint(int(b.x()), int(b.y())))
            QTest.mouseMove(cv, QPoint(int(c.x()), int(c.y())))
            QTest.mouseRelease(cv, Qt.RightButton, Qt.NoModifier, QPoint(int(c.x()), int(c.y())))
            for p0, p1 in (((x + w - 15, y + 12), (x + w - 15, y + h - 12)), ((x + 140, y + 50), (x + 175, y + 60))):
                q0, q1 = cv.vid_to_widget(*p0), cv.vid_to_widget(*p1)
                QTest.mousePress(cv, Qt.RightButton, Qt.NoModifier, QPoint(int(q0.x()), int(q0.y())))
                QTest.mouseMove(cv, QPoint(int(q1.x()), int(q1.y())))
                QTest.mouseRelease(cv, Qt.RightButton, Qt.NoModifier, QPoint(int(q1.x()), int(q1.y())))
            self.assertEqual(mp.seed_frame, 30)
            self.assertGreater(int(mp.seed_pos.sum()), 100)
            self.assertGreater(int(mp.seed_neg.sum()), 100)
            cv.grab()
            self.assertTrue(mp.start_occlusion())
            self.assertTrue(mp.wait(60))
            self.assertIsNone(mp.error, mp.error)
            self.assertIn("renk modeli", mp.occ_lbl.text())
            self.assertTrue(mp.occlusion.occluded_frames())
            n0 = len(ctx.doc.events)
            self.assertIsNone(mp.track)
            ctx.set_selection({0}, 0)
            rows = mp.apply()
            self.assertTrue(rows)
            self.assertTrue(any("\\iclip" in ctx.doc.events[r].text for r in rows))
            ctx.do_undo()
            self.assertEqual(len(ctx.doc.events), n0)
            # başka karede çizince tohumlar sıfırlanır
            win.video.seek_frame(40)
            pump(0.2)
            mp.paint_seed(10, 10, 20, 20, True)
            self.assertEqual(mp.seed_frame, 40)
            win.video.set_tool("video")
        finally:
            win.close()


if __name__ == "__main__":
    unittest.main()
