"""Medya çekirdeği testleri: kare dizini, Aegisub normalize ms, ses hizalama,
tepe zarfı, spektrogram, kare çözme (ffmpeg yer gerçeğiyle)."""

import os
import subprocess
import tempfile
import unittest

import numpy as np

os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())
os.environ.setdefault("LOCALAPPDATA", tempfile.mkdtemp())

from tests.media_fixtures import have_ffmpeg, path  # noqa: E402

try:
    import av  # noqa: F401
    HAVE_AV = True
except ImportError:
    HAVE_AV = False


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestFrameIndex(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media.frameindex import build_index
        cls.mkv = build_index(path("cfr.mkv"), use_cache=False)
        cls.mp4 = build_index(path("cfr.mp4"), use_cache=False)
        cls.vfr = build_index(path("vfr.mkv"), use_cache=False)

    def test_counts_match_ffprobe(self):
        for idx, name in ((self.mkv, "cfr.mkv"), (self.mp4, "cfr.mp4"), (self.vfr, "vfr.mkv")):
            r = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_packets",
                                "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0", path(name)],
                               capture_output=True, text=True)
            self.assertEqual(len(idx), int(r.stdout.strip()), name)

    def test_keyframes(self):
        self.assertEqual(self.mkv.keyframes, [0, 48, 96, 144, 192])
        self.assertEqual(self.mp4.keyframes, self.mkv.keyframes)

    def test_aegisub_normalised_ms(self):
        # MP4'te video 21 ms geç başlıyor; Aegisub ilk kareyi 0'a çeker
        self.assertAlmostEqual(self.mp4.offset_s, 0.021, places=3)
        self.assertEqual(self.mkv.ms[:6], [0, 42, 83, 125, 167, 209])
        self.assertEqual(self.mp4.ms, self.mkv.ms)

    def test_vfr_detection(self):
        self.assertFalse(self.mkv.is_vfr)          # 41/42 ms salınımı VFR değil
        self.assertTrue(self.vfr.is_vfr)
        d = np.diff(self.vfr.ms)
        self.assertTrue(set(d[:80]) <= {33, 34} and set(d[-50:]) <= {16, 17})

    def test_framemap_roundtrip_all_frames(self):
        for idx in (self.mkv, self.vfr):
            fm = idx.frame_map()
            for f in range(1, len(idx)):
                self.assertEqual(fm.frame_from_ms(fm.ms_from_frame(f)), f)

    def test_mpv_position_to_frame(self):
        for f in (0, 1, 24, 100, 238):
            t = self.mkv.seconds_of(f)
            self.assertEqual(self.mkv.frame_at_seconds(t), f)
            self.assertEqual(self.mkv.frame_at_seconds(t - 0.0004), f)   # µs kırpması
            self.assertEqual(self.mkv.frame_at_seconds(t + 0.02), f)

    def test_cache(self):
        from osisub.media.frameindex import build_index
        a = build_index(path("cfr.mkv"))
        b = build_index(path("cfr.mkv"))
        self.assertEqual(a.pts, b.pts)
        self.assertEqual(a.keyframes, b.keyframes)


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestAudio(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media.audio import decode_audio
        cls.mkv = decode_audio(path("cfr.mkv"), use_cache=False)
        cls.mp4 = decode_audio(path("cfr.mp4"), use_cache=False)

    def onset(self, ad, a, b):
        mn, mx = ad.peaks.window(a, b, b - a)
        return a + int(np.flatnonzero(mx > 0.2)[0])

    def test_tone_positions_and_alignment(self):
        for ad in (self.mkv, self.mp4):
            self.assertLessEqual(abs(self.onset(ad, 900, 1100) - 1000), 1)
            self.assertLessEqual(abs(self.onset(ad, 4900, 5100) - 5000), 1)
            mn, mx = ad.peaks.window(2100, 4900, 100)
            self.assertLess(float(mx.max()), 0.01)

    def test_window_shapes_and_zoom(self):
        for width in (1, 7, 640, 3000):
            mn, mx = self.mkv.peaks.window(0, 10000, width)
            self.assertEqual(len(mn), width)
        mn, mx = self.mkv.peaks.window(1500, 1502, 200)     # örnek düzeyi yakınlık
        self.assertGreater(float(mx.max()), 0.3)

    def test_spectrogram_band(self):
        sp = self.mkv.spectrogram
        col = sp.window(1200, 1800, 20).mean(axis=1)
        edges = np.geomspace(40.0, sp.rate / 2.0, sp.BANDS + 1)
        expect = int(np.searchsorted(edges, 440.0)) - 1
        self.assertLessEqual(abs(int(col.argmax()) - expect), 1)
        self.assertEqual(int(sp.window(3000, 4000, 10).max()), 0)

    def test_slice_bytes(self):
        b = self.mkv.slice_bytes(1000, 2000)
        self.assertEqual(len(b), 2 * self.mkv.rate)


@unittest.skipUnless(have_ffmpeg() and HAVE_AV, "ffmpeg/PyAV yok")
class TestFrameGrabber(unittest.TestCase):
    def test_against_ffmpeg(self):
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        for name in ("cfr.mkv", "cfr.mp4", "vfr.mkv"):
            idx = build_index(path(name))
            g = FrameGrabber(path(name), idx)
            for f in (0, 1, 47, 48, 49, 120, 5, 121):
                img = g.get(f)
                ref = subprocess.run(["ffmpeg", "-v", "error", "-i", path(name), "-vf", f"select=eq(n\\,{f})",
                                      "-vframes", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                                     capture_output=True).stdout
                ref = np.frombuffer(ref, np.uint8).reshape(img.shape)
                self.assertLess(float(np.abs(ref.astype(int) - img).mean()), 1.0, f"{name} kare {f}")
            g.close()


if __name__ == "__main__":
    unittest.main()
