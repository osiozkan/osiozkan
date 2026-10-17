"""Görsel tipografi araçları — ekransız, gerçek fare olaylarıyla."""

import math
import os
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"
os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())

import numpy as np  # noqa: E402
from PySide6.QtCore import QPoint, QPointF, Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402

from osisub.core import tags as T  # noqa: E402
from tests.media_fixtures import have_ffmpeg  # noqa: E402
from tests.test_video_ui import HAVE_MPV, make, pump  # noqa: E402


def P(pt: QPointF) -> QPoint:
    return QPoint(int(round(pt.x())), int(round(pt.y())))


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestVisualTools(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.win, cls.ctx, cls.ass = make(False)
        cls.video = cls.win.video
        cls.cv = cls.video.canvas

    @classmethod
    def tearDownClass(cls):
        cls.video.stop_backend()
        cls.win.strip.shutdown()
        cls.win.close()

    def setUp(self):
        self.ctx.set_selection({0}, 0)
        self.video.set_tool("drag")
        self.cv.resize(640, 360)          # video 640x360, PlayRes 1920x1080 -> 1/3 ölçek
        pump(0.05)
        self.n_undo = self.ctx.undo.index

    def tearDown(self):
        while self.ctx.undo.index > self.n_undo:
            self.ctx.do_undo()
        self.cv.inverse = False
        self.video.set_tool("video")

    def drag(self, a: QPointF, b: QPointF, mods=Qt.NoModifier, button=Qt.LeftButton):
        QTest.mousePress(self.cv, button, mods, P(a))
        QTest.mouseMove(self.cv, P((a + b) / 2))
        QTest.mouseMove(self.cv, P(b))
        QTest.mouseRelease(self.cv, button, mods, P(b))
        pump(0.02)

    def text(self, row=0):
        return self.ctx.doc.events[row].text

    def test_canvas_shows_frame(self):
        self.assertFalse(self.cv.isHidden())
        self.assertIsNotNone(self.cv.image)
        self.assertEqual(self.video.tool, "drag")
        self.video.set_tool("video")
        self.assertTrue(self.cv.isHidden())

    def test_mapping(self):
        r = self.cv.frame_rect()
        self.assertEqual((r.width(), r.height()), (640, 360))
        x, y = self.cv.to_script(self.cv.to_widget(960, 540))
        self.assertAlmostEqual(x, 960)
        self.assertAlmostEqual(y, 540)

    def test_drag_default_position_to_pos(self):
        # Default stil an2, satırda \an8: x=(80+1920-80)/2=960, y=MarginV=50
        start = self.cv.to_widget(960, 50)
        self.drag(start, start + QPointF(30, 20))
        self.assertEqual(T.get_pos(self.text()), (1050.0, 110.0))
        self.assertIn("\\an8", self.text())
        # tek geri al adımı
        self.ctx.do_undo()
        self.assertEqual(self.text(), "{\\an8}Merhaba dünya, nasılsın?")
        self.n_undo = self.ctx.undo.index

    def test_ctrl_drag_makes_move_and_moves_endpoints(self):
        start = self.cv.to_widget(960, 50)
        self.drag(start, start + QPointF(10, 0), Qt.ControlModifier)
        mv = T.get_move(self.text())
        self.assertEqual(mv[:4], (960.0, 50.0, 990.0, 50.0))
        self.assertIsNone(T.get_pos(self.text()))
        b = self.cv.to_widget(990, 50)
        self.drag(b, b + QPointF(0, 10))
        self.assertEqual(T.get_move(self.text())[:4], (960.0, 50.0, 990.0, 80.0))

    def test_drag_moves_all_selected(self):
        self.ctx.update_events({1: {"text": "{\\pos(100,200)}İkinci"}}, "hazırlık")
        self.ctx.set_selection({0, 1}, 0)
        start = self.cv.to_widget(960, 50)
        self.drag(start, start + QPointF(10, 10))
        self.assertEqual(T.get_pos(self.text(1)), (130.0, 230.0))

    def test_double_click_sets_pos(self):
        pt = self.cv.to_widget(300, 600)
        QTest.mouseDClick(self.cv, Qt.LeftButton, Qt.NoModifier, P(pt))
        pump(0.02)
        x, y = T.get_pos(self.text())
        self.assertAlmostEqual(x, 300, delta=2)
        self.assertAlmostEqual(y, 600, delta=2)

    def test_rotz_counterclockwise_positive(self):
        self.video.set_tool("rotz")
        o = self.cv.to_widget(960, 50)
        self.drag(o + QPointF(60, 30), o + QPointF(-30, 60))   # saat yönünde 90° (ekranda) -> -90
        self.assertAlmostEqual(T.get_float(self.text(), "frz"), -90, delta=2)
        self.ctx.do_undo()
        self.drag(o + QPointF(60, 0), o + QPointF(0, -60))     # saat yönü tersi -> +90
        self.assertAlmostEqual(T.get_float(self.text(), "frz"), 90, delta=2)

    def test_org_drag(self):
        self.video.set_tool("rotz")
        o = self.cv.to_widget(960, 50)
        self.drag(o, o + QPointF(20, 20))
        self.assertEqual(T.get_org(self.text()), (1020.0, 110.0))

    def test_rotxy_and_scale(self):
        self.video.set_tool("rotxy")
        o = self.cv.to_widget(960, 50)
        self.drag(o + QPointF(80, 80), o + QPointF(100, 60))
        self.assertAlmostEqual(T.get_float(self.text(), "fry"), 10, delta=0.6)
        self.assertAlmostEqual(T.get_float(self.text(), "frx"), 10, delta=0.6)
        self.video.set_tool("scale")
        hx = o + QPointF(100, 0)                 # fscx=100 -> tutamaç 100 px sağda
        self.drag(hx, hx + QPointF(50, 0))
        self.assertEqual(T.get_float(self.text(), "fscx"), 150)
        self.assertEqual(T.get_float(self.text(), "fscy"), 100)

    def test_rect_clip_new_corner_inverse(self):
        self.video.set_tool("clip")
        a, b = QPointF(100, 100), QPointF(200, 150)
        self.drag(a, b)
        self.assertEqual(T.get_clip(self.text()), ("rect", False, (300, 300, 600, 450)))
        self.drag(QPointF(200, 150), QPointF(220, 160))       # sağ alt köşe
        self.assertEqual(T.get_clip(self.text())[2], (300, 300, 660, 480))
        self.drag(QPointF(150, 120), QPointF(160, 120))       # içini taşı
        self.assertEqual(T.get_clip(self.text())[2], (330, 300, 690, 480))
        self.ctx.update_events({0: {"text": T.remove(self.text(), "clip")}}, "temizle")
        self.cv.inverse = True
        self.drag(QPointF(10, 10), QPointF(20, 30))
        self.assertEqual(T.get_clip(self.text()), ("rect", True, (30, 30, 60, 90)))

    def test_vector_clip_add_drag_delete_insert(self):
        self.video.set_tool("vclip")
        for x, y in ((100, 100), (200, 100), (200, 200)):
            QTest.mouseClick(self.cv, Qt.LeftButton, Qt.NoModifier, QPoint(x, y))
        self.assertEqual(T.get_clip(self.text())[2][1], "m 300 300 l 600 300 600 600")
        self.drag(QPointF(200, 200), QPointF(150, 210))
        self.assertEqual(T.get_clip(self.text())[2][1], "m 300 300 l 600 300 450 630")
        QTest.mouseClick(self.cv, Qt.RightButton, Qt.NoModifier, QPoint(200, 100))
        self.assertEqual(T.get_clip(self.text())[2][1], "m 300 300 l 450 630")
        QTest.mouseClick(self.cv, Qt.LeftButton, Qt.ControlModifier, QPoint(125, 160))
        pts = T.parse_drawing(T.get_clip(self.text())[2][1])
        self.assertEqual(len(pts), 3)
        self.assertEqual((pts[1][1], pts[1][2]), (375, 480))

    def test_eyedropper(self):
        self.video.set_tool("eyedrop")
        self.ctx.set_selection({0, 1}, 0)
        pt = QPoint(50, 200)
        QTest.mouseClick(self.cv, Qt.LeftButton, Qt.NoModifier, pt)
        arr = self.win.media.grabber.get(self.win.media.frame)
        r, g, b = (int(v) for v in arr[200, 50])
        want = f"&H{b:02X}{g:02X}{r:02X}&"
        self.assertEqual(T.get(self.text(0), "1c"), want)
        self.assertEqual(T.get(self.text(1), "1c"), want)

    def test_paint_all_tools(self):
        for key in ("cross", "drag", "rotz", "rotxy", "scale", "clip", "vclip", "eyedrop"):
            self.video.set_tool(key)
            self.cv.mouse = QPointF(100, 100)
            img = self.cv.grab().toImage()
            self.assertFalse(img.isNull(), key)


@unittest.skipUnless(have_ffmpeg() and HAVE_MPV, "ffmpeg ya da libmpv yok")
class TestVisualRendering(unittest.TestCase):
    """Sürükleme sonrası tuvaldeki görüntü (libass çizimi) gerçekten değişiyor mu?"""

    @classmethod
    def setUpClass(cls):
        cls.win, cls.ctx, cls.ass = make(True)

    @classmethod
    def tearDownClass(cls):
        cls.win.video.stop_backend()
        cls.win.strip.shutdown()
        cls.win.close()

    def test_rendered_text_follows_pos(self):
        v = self.win.video
        cv = v.canvas
        ctx = self.ctx
        ctx.set_active(0, select_only=True)
        v.seek_frame(48)
        ctx.update_events({0: {"text": "{\\an5\\pos(480,540)\\fs200\\bord0\\1c&H0000FF&}███"}}, "test")
        v.set_tool("drag")
        cv.resize(640, 360)

        def wait_img():
            cv._img_frame = None
            cv.refresh_image()
            return pump(until=lambda: cv._img_frame == 48, timeout=5)

        self.assertTrue(wait_img())
        base = v.grab_frame(False)                     # altyazısız kare: testsrc2'nin kendi kırmızısı
        red = lambda a: ((a[..., 0] > 180) & (a[..., 1] < 80) & (a[..., 2] < 80)  # noqa: E731
                         & ~((base[..., 0] > 180) & (base[..., 1] < 80) & (base[..., 2] < 80)))
        before = red(cv._buf).astype(int)
        self.assertGreater(before.sum(), 500)
        start = cv.to_widget(480, 540)
        QTest.mousePress(cv, Qt.LeftButton, Qt.NoModifier, P(start))
        QTest.mouseMove(cv, P(start + QPointF(200, 0)))
        QTest.mouseRelease(cv, Qt.LeftButton, Qt.NoModifier, P(start + QPointF(200, 0)))
        pump(0.05)
        self.assertEqual(T.get_pos(ctx.doc.events[0].text), (1080.0, 540.0))
        self.assertTrue(wait_img())
        after = red(cv._buf).astype(int)
        cx_before = np.nonzero(before.sum(axis=0))[0].mean()
        cx_after = np.nonzero(after.sum(axis=0))[0].mean()
        self.assertGreater(cx_after - cx_before, 150)      # metin ~200 px sağa kaydı
        while ctx.undo.can_undo:
            ctx.do_undo()
        v.set_tool("video")


if __name__ == "__main__":
    unittest.main()
