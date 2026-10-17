"""Örtme: temiz videoda alt-piksel iz hatası keskin kenarlarda yanlış alarm üretmemeli (izleme alanı
±2 px oynasa da). 1.2.0'da bu ortamın x264 sürümüyle kare 0–1 ya da 89–95 %1 'örtülü' çıkıyordu."""

import os
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from tests import media_fixtures as mf  # noqa: E402


@unittest.skipUnless(mf.have_ffmpeg(), "ffmpeg yok")
class TestNoFalseAlarm(unittest.TestCase):
    def test_clean_video_any_roi(self):
        try:
            import cv2  # noqa: F401
        except ImportError:
            self.skipTest("OpenCV yok")
        from osisub.media import occlude, tracker
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        p = mf.path("motion.mkv")
        g = FrameGrabber(p, build_index(p))
        try:
            C, _c = mf.motion_truth(40)
            for dx, dw in ((0, 0), (-2, 4), (2, -4)):
                roi = (C.real - 50 + dx, C.imag - 25 + dx, 100 + dw, 50 + dw)
                tr = tracker.track(g.get, 0, 95, 40, roi, width=640, height=360)
                res = occlude.detect(g.get, tr, roi, occlude.OcclusionOptions())
                self.assertEqual(res.occluded_frames(), [], roi)
                # tolerans kapalıyken eski davranış (en az bir ROI'de yanlış alarm) — ölçüm belgesi
                legacy = occlude.detect(g.get, tr, roi, occlude.OcclusionOptions(shift_tol=0))
                self.assertLessEqual(max((f.ratio for f in legacy.frames.values()), default=0), 0.05)
        finally:
            g.close()


if __name__ == "__main__":
    unittest.main()
