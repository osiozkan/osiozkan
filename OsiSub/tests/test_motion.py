"""Hareket takibi: iz modeli, dosya biçimleri, satıra uygulama, yer gerçekli izleme,
libass ile yakma doğrulaması ve bölme (ekransız)."""

import cmath
import math
import os
import re
import subprocess
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"
os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())
os.environ.setdefault("LOCALAPPDATA", tempfile.mkdtemp())
os.environ.setdefault("APPDATA", tempfile.mkdtemp())

import numpy as np  # noqa: E402

from osisub.core import motion as M  # noqa: E402
from osisub.core import tags as T  # noqa: E402
from osisub.core.assfile import AssDocument, Event, Style  # noqa: E402
from osisub.core.framemap import START, FrameMap  # noqa: E402
from tests.media_fixtures import have_ffmpeg  # noqa: E402

try:
    import cv2  # noqa: F401
    HAVE_CV = True
except ImportError:
    HAVE_CV = False

FM = FrameMap.from_fps(24000 / 1001, 400)


def make_track(n=48, start=10, ref=None, dx=2.0, dy=-1.0, ds=0.005, dr=0.5, w=640, h=360):
    """Bilinen dönüşüm: referansa göre kare başına dx,dy kayma, ds ölçek, dr derece (görüntü açısı)."""
    ref = start if ref is None else ref
    frames = []
    for k in range(start, start + n):
        i = k - ref
        c = cmath.rect(1 + ds * i, math.radians(dr * i))
        frames.append(M.TrackFrame(k, c, complex(dx * i, dy * i)))
    return M.MotionTrack(frames, ref, w, h, 23.976, (100, 100, 80, 40), "test")


def ev_for(f0, f1, text):
    from osisub.core.asstime import round_ms
    return Event(start=round_ms(FM.time_at_frame(f0, START)), end=round_ms(FM.time_at_frame(f1 + 1, START)),
                 text=text)


class TestTrackModel(unittest.TestCase):
    def test_json_roundtrip(self):
        tr = make_track()
        tr.frames[5].ok = False
        tr.frames[5].conf = 0.2
        p = os.path.join(tempfile.mkdtemp(), "a.assmotiontrack")
        tr.save(p)
        tr2 = M.MotionTrack.load(p)
        self.assertEqual(len(tr2), len(tr))
        self.assertEqual(tr2.ref_frame, tr.ref_frame)
        self.assertEqual(tr2.roi, tr.roi)
        self.assertFalse(tr2.frames[5].ok)
        for a, b in zip(tr.frames, tr2.frames):
            self.assertLess(abs(a.c - b.c), 1e-6)
            self.assertLess(abs(a.t - b.t), 1e-3)
        with open(p, encoding="utf-8") as fh:
            import json
            d = json.load(fh)
        self.assertEqual(d["format"], "assmotiontrack")
        self.assertIn("x", d["frames"][0])          # okunabilir alanlar

    def test_ae_roundtrip(self):
        tr = make_track(ref=10)
        txt = tr.to_ae()
        self.assertTrue(txt.startswith("Adobe After Effects 6.0 Keyframe Data"))
        back = M.from_ae(txt, start_frame=10)
        self.assertEqual((back.start_frame, back.end_frame, back.ref_frame), (10, 57, 10))
        self.assertEqual(back.width, 640)
        for a, b in zip(tr.frames, back.frames):
            for z in (0j, complex(300, 200), complex(-50, 400)):
                za, zb = a.c * z + a.t, b.c * z + b.t
                self.assertLess(abs(za - zb), 2e-3)

    def test_ae_mocha_with_anchor(self):
        # Mocha tarzı: çapa sabit (200,100), konum kayıyor, ölçek ve dönüş değişiyor
        rows_pos = [(0, 300, 200), (1, 310, 205), (2, 320, 210)]
        rows_sc = [(0, 100), (1, 110), (2, 120)]
        rows_rot = [(0, 0), (1, 5), (2, 10)]
        lines = ["Adobe After Effects 6.0 Keyframe Data", "", "\tUnits Per Second\t23.976",
                 "\tSource Width\t1920", "\tSource Height\t1080", "", "Anchor Point",
                 "\tFrame\tX pixels\tY pixels\tZ pixels\t"]
        lines += [f"\t{f}\t200\t100\t0\t" for f, _x, _y in rows_pos]
        lines += ["", "Position", "\tFrame\tX pixels\tY pixels\tZ pixels\t"]
        lines += [f"\t{f}\t{x}\t{y}\t0\t" for f, x, y in rows_pos]
        lines += ["", "Scale", "\tFrame\tX percent\tY percent\tZ percent\t"]
        lines += [f"\t{f}\t{s}\t{s}\t100\t" for f, s in rows_sc]
        lines += ["", "Rotation", "\tFrame\tDegrees\t"]
        lines += [f"\t{f}\t{r}\t" for f, r in rows_rot]
        lines += ["", "End of Keyframe Data"]
        tr = M.from_ae("\n".join(lines), start_frame=100)
        self.assertEqual((tr.start_frame, tr.end_frame, tr.width, tr.height), (100, 102, 1920, 1080))

        def L(k, q):
            _f, x, y = rows_pos[k]
            c = cmath.rect(rows_sc[k][1] / 100, math.radians(rows_rot[k][1]))
            return complex(x, y) + c * (q - complex(200, 100))
        for k in range(3):
            for q in (complex(200, 100), complex(250, 80), complex(0, 0)):
                z_ref = L(0, q)
                exp = L(k, q)
                got = tr.frames[k].c * z_ref + tr.frames[k].t
                self.assertLess(abs(exp - got), 1e-6)
        self.assertAlmostEqual(tr.frames[2].scale, 1.2, 6)
        self.assertAlmostEqual(tr.frames[2].rot, -10.0, 6)       # AE saat yönü + -> \frz -

    def test_rebase_fill_smooth(self):
        tr = make_track(ref=20)
        rb = tr.rebased(30)
        self.assertEqual(rb.ref_frame, 30)
        f = rb.at(30)
        self.assertLess(abs(f.c - 1), 1e-12)
        self.assertLess(abs(f.t), 1e-9)
        # zincir tutarlılığı: rb_k(orig_ref_noktası) == tr_k(tr_30⁻¹(...)) özdeşliği
        z = complex(150, 120)
        z30 = tr.at(30).c * z + tr.at(30).t
        for k in (15, 40, 57):
            self.assertLess(abs(rb.at(k).c * z30 + rb.at(k).t - (tr.at(k).c * z + tr.at(k).t)), 1e-6)
        # güvensiz kareleri doldurma
        bad = make_track(ref=10)
        for i in (5, 6, 7):
            bad.frames[i].ok = False
            bad.frames[i].c, bad.frames[i].t = 3 + 0j, complex(999, 999)
        filled = bad.filled()
        ref = make_track(ref=10)
        for i in (5, 6, 7):
            self.assertLess(abs(filled.frames[i].t - ref.frames[i].t), 1e-6)
            self.assertLess(abs(filled.frames[i].scale - ref.frames[i].scale), 1e-3)
        # yumuşatma: doğrusal izde (iç karelerde) değişmez, referans kimlik kalır
        sm = ref.smoothed(3)
        self.assertLess(abs(sm.at(10).c - 1), 1e-9)
        for i in range(5, 40):
            self.assertLess(abs(sm.frames[i].t - ref.frames[i].t), 0.05)
        # gürültü azaltma
        rng = np.random.default_rng(1)
        noisy = make_track(ref=10)
        for f in noisy.frames[1:]:
            f.t += complex(*rng.normal(0, 1.0, 2))
        err_before = np.mean([abs(a.t - b.t) for a, b in zip(noisy.frames, ref.frames)])
        sm2 = noisy.smoothed(3)
        err_after = np.mean([abs(a.t - b.t) for a, b in zip(sm2.frames[4:-4], ref.frames[4:-4])])
        self.assertLess(err_after, err_before * 0.6)


class TestApply(unittest.TestCase):
    def setUp(self):
        self.style = Style(name="Default", fontsize=40, outline=3, shadow=1, scale_x=100, scale_y=100)

    def test_frame_mode_position_scale_rotation(self):
        tr = make_track(start=10, n=30, ref=10)
        ev = ev_for(10, 29, "{\\pos(100,100)\\blur2}Merhaba")
        lines, warns = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360))
        self.assertEqual(warns, [])
        self.assertEqual(len(lines), 20)
        for i, ln in enumerate(lines):
            k = 10 + i
            self.assertEqual(FM.frame_at_time(ln.start, START), k)
            self.assertEqual(FM.frame_at_time(ln.end, 2), k)            # END kipinde aynı kare
            if i:
                self.assertEqual(ln.start, lines[i - 1].end)
            f = tr.at(k)
            x, y = T.get_pos(ln.text)
            ex, ey = f.map(100, 100)
            self.assertAlmostEqual(x, ex, delta=0.006)
            self.assertAlmostEqual(y, ey, delta=0.006)
            self.assertAlmostEqual(T.get_float(ln.text, "fscx"), 100 * f.scale, delta=0.006)
            self.assertAlmostEqual(T.get_float(ln.text, "bord"), 3 * f.scale, delta=0.006)
            self.assertAlmostEqual(T.get_float(ln.text, "shad"), 1 * f.scale, delta=0.006)
            self.assertAlmostEqual(T.get_float(ln.text, "blur"), 2 * f.scale, delta=0.006)
            self.assertAlmostEqual(T.get_float(ln.text, "frz"), T.norm_angle(f.rot), delta=0.006)
            self.assertTrue(ln.text.endswith("}Merhaba"))
        # referans karede satır değişmeden kalır (yalnız eksik etiketler eklenir)
        self.assertEqual(T.get_pos(lines[0].text), (100, 100))
        self.assertEqual(T.get_float(lines[0].text, "fscx"), 100)

    def test_default_position_and_options_off(self):
        tr = make_track(start=0, n=10, ref=0)
        ev = ev_for(0, 9, "Alt yazı")              # \pos yok: stil hizası (an2) ve kenar boşluğu
        lines, _ = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360),
                                 M.ApplyOptions(scale=False, rotation=False))
        x0, y0 = T.default_pos(ev, self.style, (640, 360))
        x, y = T.get_pos(lines[5].text)
        ex, ey = tr.at(5).map(x0, y0)
        self.assertAlmostEqual(x, ex, delta=0.006)
        self.assertAlmostEqual(y, ey, delta=0.006)
        self.assertIsNone(T.get(lines[5].text, "fscx"))
        self.assertIsNone(T.get(lines[5].text, "frz"))

    def test_play_res_differs_from_video(self):
        # betik 1280x720, video 640x360: video pikselinde 2 px kayma = betikte 4 px
        tr = make_track(start=0, n=5, ref=0, ds=0, dr=0, dx=2, dy=0)
        ev = ev_for(0, 4, "{\\pos(200,200)}x")
        lines, _ = M.apply_track(ev, self.style, tr, FM, (1280, 720), (640, 360))
        self.assertEqual(T.get_pos(lines[3].text), (212, 200))

    def test_time_shift_t_and_fad(self):
        tr = make_track(start=0, n=40, ref=0, ds=0, dr=0)
        ev = ev_for(0, 29, "{\\pos(10,10)\\fad(200,300)\\t(\\fscx150)\\t(100,500,2,\\frz10)}a")
        dur = ev.end - ev.start
        lines, _ = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360))
        ln = lines[12]
        off = ln.start - ev.start
        self.assertGreater(off, 0)
        self.assertIn(f"\\fade(255,0,255,{-off},{200 - off},{dur - 300 - off},{dur - off})", ln.text)
        self.assertIn(f"\\t({-off},{dur - off},\\fscx150)", ln.text)
        self.assertIn(f"\\t({100 - off},{500 - off},2,\\frz10)", ln.text)
        # satır 0'da kayma yok: metin aynen
        self.assertIn("\\fad(200,300)", lines[0].text)
        # shift_times tek başına
        self.assertEqual(M.shift_times("{\\t(\\bord5)}x", 100, 1000), "{\\t(-100,900,\\bord5)}x")
        self.assertEqual(M.shift_times("{\\t(0.5,\\bord5)}x", 100, 1000), "{\\t(-100,900,0.5,\\bord5)}x")

    def test_clips(self):
        tr = make_track(start=0, n=10, ref=0, ds=0, dr=0, dx=3, dy=1)
        ev = ev_for(0, 9, "{\\pos(10,10)\\clip(100,100,200,150)}a")
        lines, _ = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360))
        self.assertEqual(T.get_clip(lines[4].text), ("rect", False, (112, 104, 212, 154)))
        # dönüş varken dikdörtgen clip vektöre çevrilir (köşeler dönüşle taşınır)
        tr2 = make_track(start=0, n=10, ref=0, ds=0.01, dr=2)
        lines, _ = M.apply_track(ev, self.style, tr2, FM, (640, 360), (640, 360))
        kind, inv, (sc, draw) = T.get_clip(lines[6].text)
        self.assertEqual(kind, "vector")
        pts = T.parse_drawing(draw)
        f = tr2.at(6)
        for (cmd, x, y), (ox, oy) in zip(pts, ((100, 100), (200, 100), (200, 150), (100, 150))):
            ex, ey = f.map(ox, oy)
            self.assertAlmostEqual(x, ex, delta=0.006)
            self.assertAlmostEqual(y, ey, delta=0.006)
        # ölçekli (\iclip(2,...)) vektör clip: koordinatlar 2^(2-1) katında
        ev3 = ev_for(0, 9, "{\\pos(10,10)\\iclip(2,m 200 200 l 400 200 400 300)}a")
        tr3 = make_track(start=0, n=10, ref=0, ds=0, dr=0, dx=5, dy=0)
        lines, _ = M.apply_track(ev3, self.style, tr3, FM, (640, 360), (640, 360))
        kind, inv, (sc, draw) = T.get_clip(lines[2].text)
        self.assertEqual((kind, inv, sc), ("vector", True, 2))
        self.assertEqual(T.parse_drawing(draw)[0][1:], [220.0, 200.0])     # 2 kare * 5 px * 2

    def test_move_rejected_and_apply_to_lines(self):
        tr = make_track(start=0, n=30, ref=0)
        with self.assertRaises(M.MotionError):
            M.apply_track(ev_for(0, 5, "{\\move(1,2,3,4)}a"), self.style, tr, FM, (640, 360), (640, 360))
        evs = [ev_for(0, 4, "{\\pos(1,1)}a"), ev_for(0, 2, "{\\move(1,2,3,4)}b"),
               ev_for(5, 9, "{\\pos(2,2)}c"), ev_for(0, 1, "d")]
        out, rows, warns = M.apply_to_lines(_doc_with(evs), [0, 1, 2], tr, FM, (640, 360))
        self.assertEqual(len(out), 5 + 1 + 5 + 1)
        self.assertEqual(rows, [0, 1, 2, 3, 4, 6, 7, 8, 9, 10])
        self.assertEqual(out[5].text, "{\\move(1,2,3,4)}b")
        self.assertEqual(out[11].text, "d")
        self.assertTrue(any("Satır 2" in w and "\\move" in w for w in warns))

    def test_outside_track_holds(self):
        tr = make_track(start=5, n=5, ref=5)
        lines, warns = M.apply_track(ev_for(0, 14, "{\\pos(0,0)}a"), self.style, tr, FM, (640, 360), (640, 360))
        self.assertEqual(len(lines), 15)
        self.assertTrue(any("izin dışında" in w for w in warns))
        self.assertEqual(T.get_pos(lines[0].text), T.get_pos(lines[5].text))
        self.assertEqual(T.get_pos(lines[14].text), T.get_pos(lines[9].text))

    def test_compact_reproduces_frame_mode(self):
        # yumuşak, doğrusal olmayan hareket (parçalara bölünmeli)
        frames = []
        for k in range(0, 72):
            c = cmath.rect(1 + 0.3 * math.sin(k / 20), math.radians(15 * math.sin(k / 30)))
            frames.append(M.TrackFrame(k, c, complex(40 * math.sin(k / 9), 3 * k)))
        tr = M.MotionTrack(frames, 0, 640, 360).rebased(0)
        ev = ev_for(0, 71, "{\\pos(320,180)\\bord2}Tabela")
        full, _ = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360))
        comp, warns = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360),
                                    M.ApplyOptions(mode="compact"))
        self.assertEqual(warns, [])
        self.assertLess(len(comp), len(full) / 2)
        self.assertEqual(comp[0].start, full[0].start)
        self.assertEqual(comp[-1].end, full[-1].end)
        for k in range(72):
            t_ms = FM.time_at_frame(k)
            ln = next(x for x in comp if x.start <= t_ms < x.end)
            got = _eval_compact(ln, t_ms)
            ref = full[k].text
            ex, ey = T.get_pos(ref)
            self.assertAlmostEqual(got["x"], ex, delta=0.52)
            self.assertAlmostEqual(got["y"], ey, delta=0.52)
            self.assertAlmostEqual(got["fscx"], T.get_float(ref, "fscx"), delta=0.52)
            self.assertAlmostEqual(T.norm_angle(got["frz"] - T.get_float(ref, "frz")), 0, delta=0.32)

    def test_compact_fallbacks(self):
        tr = make_track(start=0, n=20, ref=0)
        ev = ev_for(0, 19, "{\\pos(10,10)\\clip(m 0 0 l 100 0 100 100)}a")
        lines, warns = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360), M.ApplyOptions(mode="compact"))
        self.assertEqual(len(lines), 20)
        self.assertTrue(any("vektör clip" in w for w in warns))
        ev = ev_for(0, 19, "{\\pos(10,10)\\t(\\fscx120)}a")
        lines, warns = M.apply_track(ev, self.style, tr, FM, (640, 360), (640, 360), M.ApplyOptions(mode="compact"))
        self.assertTrue(any("\\t" in w for w in warns))


def _doc_with(evs):
    doc = AssDocument.from_text(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 640\nPlayResY: 360\n\n[V4+ Styles]\n"
        "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, "
        "Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, "
        "MarginL, MarginR, MarginV, Encoding\n"
        "Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n"
        "\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n")
    doc.ensure_section("events").items[:] = list(evs)
    return doc


def _eval_compact(ln, t_ms):
    """libass anlamıyla \\move ve \\t(t1,t2,...) değerini t anında hesapla."""
    rel = t_ms - ln.start
    text = ln.text
    mv = T.get_move(text)
    out = {}
    if mv:
        x1, y1, x2, y2, t1, t2 = mv
        u = 0 if rel <= t1 else (1 if rel >= t2 else (rel - t1) / (t2 - t1))
        out["x"], out["y"] = x1 + (x2 - x1) * u, y1 + (y2 - y1) * u
    else:
        out["x"], out["y"] = T.get_pos(text)
    out["fscx"] = T.get_float(text, "fscx")
    out["frz"] = T.get_float(text, "frz")
    m = re.search(r"\\t\((-?\d+),(-?\d+),([^)]*)\)", text)
    if m:
        t1, t2 = int(m.group(1)), int(m.group(2))
        u = 0 if rel <= t1 else (1 if rel >= t2 else (rel - t1) / (t2 - t1))
        body = m.group(3)
        for key in ("fscx", "frz"):
            mm = re.search(rf"\\{key}(-?\d+(?:\.\d+)?)", body)
            if mm:
                out[key] = out[key] + (float(mm.group(1)) - out[key]) * u
    return out


# ---------------------------------------------------------------------------
# Yer gerçekli izleme + libass yakma
# ---------------------------------------------------------------------------

@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestTrackerGroundTruth(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        from tests import media_fixtures as mf
        cls.mf = mf
        cls.path = mf.path("motion.mkv")
        cls.idx = build_index(cls.path)
        cls.grab = FrameGrabber(cls.path, cls.idx)
        cls.ref = 40
        C, c = mf.motion_truth(cls.ref)
        s = abs(c)
        pw, ph = mf.MOTION_PATCH
        cls.roi = (C.real - pw * s * 0.4, C.imag - ph * s * 0.35, pw * s * 0.8, ph * s * 0.7)

    @classmethod
    def tearDownClass(cls):
        cls.grab.close()

    def errors(self, tr, frames=None):
        Cr, cr = self.mf.motion_truth(tr.ref_frame)
        z = complex(self.roi[0] + self.roi[2] / 2, self.roi[1] + self.roi[3] / 2)
        ep = es = er = 0.0
        for f in tr.frames:
            if frames is not None and f.frame not in frames:
                continue
            Ck, ck = self.mf.motion_truth(f.frame)
            cc = ck / cr
            ep = max(ep, abs((Ck + cc * (z - Cr)) - (f.c * z + f.t)))
            es = max(es, abs(abs(cc) - f.scale) * 100)
            er = max(er, abs(T.norm_angle(-math.degrees(cmath.phase(cc)) - f.rot)))
        return ep, es, er

    def test_points_similarity(self):
        from osisub.media import tracker
        calls = []
        tr = tracker.track(self.grab.get, 0, self.mf.MOTION_N - 1, self.ref, self.roi,
                           progress=lambda p, k: calls.append(k))
        self.assertEqual((tr.start_frame, tr.end_frame, len(tr)), (0, 95, 96))
        self.assertEqual(tr.bad_frames(), [])
        self.assertEqual(len(calls), 96)
        ep, es, er = self.errors(tr)
        # ölçek 0.86 -> 1.38 kat, dönüş -12° -> +16.5°, kayma ~300 px
        self.assertLess(ep, 0.5)
        self.assertLess(es, 1.0)
        self.assertLess(er, 0.5)
        TestTrackerGroundTruth.track_points = tr

    def test_template_translation_window(self):
        from osisub.media import tracker
        tr = tracker.track(self.grab.get, 36, 44, self.ref, self.roi, method="template")
        ep, _es, _er = self.errors(tr)
        self.assertLess(ep, 2.0)
        self.assertTrue(all(abs(f.scale - 1) < 1e-9 for f in tr.frames))

    def test_cancel_and_bad_roi(self):
        from osisub.media import tracker
        with self.assertRaises(tracker.Cancelled):
            tracker.track(self.grab.get, 0, 95, 40, self.roi, cancel=lambda: True)
        with self.assertRaises(ValueError):
            tracker.track(self.grab.get, 0, 10, 40, self.roi)          # ref aralık dışında
        flat = (5, 5, 20, 20)       # dokusuz köşe değil ama küçük/bulanık arka plan
        try:
            tracker.track(self.grab.get, 0, 3, 0, flat, method="template")
        except ValueError:
            pass

    def test_libass_burn_follows_sign(self):
        """İzlenen satırı libass ile yak; yeşil karenin merkezi/ölçeği/açısı tabelayı izlemeli."""
        import cv2
        from osisub.media import tracker
        tr = getattr(TestTrackerGroundTruth, "track_points", None) or tracker.track(
            self.grab.get, 0, 95, self.ref, self.roi)
        fm = FrameMap(self.idx.ms)
        C, c = self.mf.motion_truth(self.ref)
        style = Style(name="Default")
        ev = Event(start=0, end=fm.time_at_frame(96, START),
                   text="{\\an5\\pos(%.2f,%.2f)\\bord0\\shad0\\1c&H00FF00&\\p1}m 0 0 l 80 0 80 80 0 80"
                   % (C.real, C.imag))
        d = tempfile.mkdtemp()
        for mode in ("frame", "compact"):
            lines, warns = M.apply_track(ev, style, tr, fm, (640, 360), (640, 360), M.ApplyOptions(mode=mode))
            self.assertEqual(warns, [])
            sub = os.path.join(d, f"{mode}.ass")
            fmt = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
            head = _doc_with([]).to_text()
            with open(sub, "w", encoding="utf-8") as fh:
                fh.write(head.rstrip("\r\n") + "\n")
                for ln in lines:
                    fh.write(ln.serialize(fmt) + "\n")
            raw = subprocess.run(["ffmpeg", "-v", "error", "-i", self.path, "-vf", f"ass={sub}",
                                  "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True,
                                 check=True).stdout
            fr = np.frombuffer(raw, np.uint8).reshape(-1, 360, 640, 3)
            self.assertEqual(len(fr), 96)
            meas = {}
            for k in range(96):
                im = fr[k].astype(int)
                m = ((im[..., 1] > 200) & (im[..., 0] < 90) & (im[..., 2] < 90)).astype(np.uint8)
                _n, lab, st, _ = cv2.connectedComponentsWithStats(m)
                j = 1 + int(np.argmax(st[1:, cv2.CC_STAT_AREA]))
                ys, xs = np.nonzero(lab == j)
                rect = cv2.minAreaRect(np.stack([xs, ys], 1).astype(np.float32))
                meas[k] = (complex(xs.mean() + 0.5, ys.mean() + 0.5), math.sqrt(len(xs)), rect[2])
            ep = es = er = 0.0
            for k in range(96):
                Ck, ck = self.mf.motion_truth(k)
                cc = ck / c
                ep = max(ep, abs(meas[k][0] - Ck))
                es = max(es, abs(meas[k][1] / meas[self.ref][1] - abs(cc)) * 100)
                ang = math.degrees(cmath.phase(cc))
                diff = (meas[k][2] - ang) % 90
                er = max(er, min(diff, 90 - diff))
            self.assertLess(ep, 1.0, mode)
            self.assertLess(es, 1.5, mode)
            self.assertLess(er, 2.0, mode)


# ---------------------------------------------------------------------------
# Örtme (önden geçen direk) ve toplu izleme — yer gerçeğiyle
# ---------------------------------------------------------------------------

def _mask_from_polys(polys, shape=(360, 640)):
    import cv2
    m = np.zeros(shape, np.uint8)
    for poly in polys:
        cv2.fillPoly(m, [np.round(np.array(poly) - 0.5).astype(np.int32)], 1)
    return m


def _burn(path, lines, d, name):
    sub = os.path.join(d, name + ".ass")
    fmt = ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
    with open(sub, "w", encoding="utf-8") as fh:
        fh.write(_doc_with([]).to_text().rstrip("\r\n") + "\n")
        for ln in lines:
            fh.write(ln.serialize(fmt) + "\n")
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vf", f"ass={sub}", "-f", "rawvideo",
                          "-pix_fmt", "rgb24", "-"], capture_output=True, check=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(-1, 360, 640, 3)


def _green(im):
    im = im.astype(int)
    return (im[..., 1] > 200) & (im[..., 0] < 90) & (im[..., 2] < 90)


@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestOcclusion(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media import tracker
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        from tests import media_fixtures as mf
        cls.mf = mf
        cls.path = mf.path("occlude.mkv")
        cls.idx = build_index(cls.path)
        cls.grab = FrameGrabber(cls.path, cls.idx)
        cls.fm = FrameMap(cls.idx.ms)
        pw, ph = mf.MOTION_PATCH

        def roi_at(ref):
            C, c = mf.motion_truth(ref)
            s = abs(c)
            return (C.real - pw * s * 0.45, C.imag - ph * s * 0.42, pw * s * 0.9, ph * s * 0.84)
        cls.roi_at = staticmethod(roi_at)
        cls.roi = roi_at(10)
        cls.track = tracker.track(cls.grab.get, 0, 95, 10, cls.roi)

    @classmethod
    def tearDownClass(cls):
        cls.grab.close()

    def truth(self, k, tr, roi):
        import cv2
        from osisub.media import occlude
        q = occlude._quad(roi, tr.at(k))
        qm = np.zeros((360, 640), np.uint8)
        cv2.fillConvexPoly(qm, np.round(q).astype(np.int32), 1)
        tm = np.zeros_like(qm)
        x0, x1 = self.mf.occluder_x(k)
        if x1 > 0:
            tm[:, max(0, x0):x1] = 1
        return tm & qm, qm

    def test_detect_against_truth(self):
        from osisub.media import occlude
        res = occlude.detect(self.grab.get, self.track)
        self.assertEqual(res.uncertain_frames(), [])
        checked = 0
        for k in range(96):
            tm, qm = self.truth(k, self.track, self.roi)
            # yöntem alan kenarından 2 px içeriyi yok sayar (ara değer hatası); yer gerçeği de öyle
            import cv2
            qe = cv2.erode(qm, np.ones((7, 7), np.uint8))
            tm = tm & qe
            dm = _mask_from_polys(res.frames[k].polys) & qe
            if tm.sum() > 900:          # direk alana belirgin biçimde girmiş
                checked += 1
                recall = (tm & dm).sum() / tm.sum()
                iou = (tm & dm).sum() / (tm | dm).sum()
                self.assertGreater(recall, 0.95, k)
                self.assertGreater(iou, 0.7, k)
            elif tm.sum() == 0:
                self.assertEqual(res.frames[k].polys, [], k)         # yanlış alarm yok
        self.assertGreater(checked, 30)
        occ = res.occluded_frames()
        self.assertLessEqual(occ[0], 33)
        self.assertGreaterEqual(occ[-1], 75)
        vis = [res.frames[k].visibility for k in range(96)]
        self.assertGreater(min(vis[:25]), 0.99)
        self.assertLess(min(vis[40:70]), 0.85)

    def test_median_model_when_reference_is_occluded(self):
        from osisub.media import occlude, tracker
        roi = self.roi_at(40)           # 40. karede direk tabelanın önünde
        tr = tracker.track(self.grab.get, 0, 95, 40, roi)
        med = occlude.detect(self.grab.get, tr, roi)
        self.assertTrue(any("referans" in n for n in med.notes))
        ref = occlude.detect(self.grab.get, tr, roi, occlude.OcclusionOptions(model="ref"))
        for k in (40, 50):
            tm, qm = self.truth(k, tr, roi)
            dm = _mask_from_polys(med.frames[k].polys) & qm
            self.assertGreater((tm & dm).sum() / tm.sum(), 0.95, k)
        # direk yokken (90. kare) tek-referans yöntemi referanstaki direği "örtme" sanır; ortanca sanmaz
        self.assertEqual(med.frames[90].polys, [])
        self.assertTrue(ref.frames[90].polys)

    def test_burn_iclip_hides_line_behind_occluder(self):
        from osisub.media import occlude
        res = occlude.detect(self.grab.get, self.track)
        C, _c = self.mf.motion_truth(10)
        ev = Event(start=0, end=self.fm.time_at_frame(96, START),
                   text="{\\an5\\pos(%.2f,%.2f)\\bord0\\shad0\\1c&H00FF00&\\p1}m 0 0 l 120 0 120 70 0 70"
                   % (C.real, C.imag))
        d = tempfile.mkdtemp()
        plain, _ = M.apply_track(ev, Style(), self.track, self.fm, (640, 360), (640, 360))
        base = _burn(self.path, plain, d, "plain")
        for mode in ("frame", "compact"):
            lines, warns = M.apply_track(ev, Style(), self.track, self.fm, (640, 360), (640, 360),
                                         M.ApplyOptions(mode=mode, occlusion=res))
            self.assertEqual(warns, [])
            if mode == "compact":
                self.assertLess(len(lines), 96)
            fr = _burn(self.path, lines, d, mode)
            for k in range(96):
                g, g0 = _green(fr[k]), _green(base[k])
                x0, x1 = self.mf.occluder_x(k)
                if x1 - 2 > 0:
                    self.assertEqual(int(g[:, max(0, x0 + 2):x1 - 2].sum()), 0, (mode, k))
                # direğin uzağında satır aynen görünür
                far = np.ones_like(g)
                far[:, max(0, x0 - 6):max(0, x1 + 6)] = False
                self.assertGreaterEqual(int((g & far).sum()), int((g0 & far).sum() * 0.99) - 5, (mode, k))

    def test_hide_below_and_persist(self):
        from osisub.media import occlude
        res = occlude.detect(self.grab.get, self.track)
        ev = ev_for(0, 95, "{\\pos(100,100)}x")
        lines, warns = M.apply_track(ev, Style(), self.track, FM, (640, 360), (640, 360),
                                     M.ApplyOptions(occlusion=res, hide_below=0.9))
        hidden = [k for k in range(96) if res.frames[k].visibility < 0.9]
        self.assertTrue(hidden)
        self.assertEqual(len(lines), 96 - len(hidden))
        self.assertTrue(any("görünürlük" in w for w in warns))
        # compact: gizli kareler atlanır, parçalar onları aşmaz
        comp, _ = M.apply_track(ev, Style(), self.track, FM, (640, 360), (640, 360),
                                M.ApplyOptions(occlusion=res, hide_below=0.9, mode="compact"))
        for k in hidden:
            t = FM.time_at_frame(k)
            self.assertFalse(any(x.start <= t < x.end for x in comp), k)
        # satırda zaten \clip varsa örtme eklenmez, uyarılır
        ev2 = ev_for(0, 95, "{\\pos(100,100)\\clip(0,0,300,300)}x")
        l2, w2 = M.apply_track(ev2, Style(), self.track, FM, (640, 360), (640, 360), M.ApplyOptions(occlusion=res))
        self.assertTrue(any("tek clip" in w for w in w2))
        self.assertFalse(any("iclip" in x.text for x in l2))
        # .assmotiontrack içinde saklanır
        tr = self.track._copy(list(self.track.frames))
        tr.occlusion = res.to_json()
        pth = os.path.join(tempfile.mkdtemp(), "o.assmotiontrack")
        tr.save(pth)
        back = occlude.OcclusionResult.from_json(M.MotionTrack.load(pth).occlusion)
        self.assertEqual(back.occluded_frames(), res.occluded_frames())
        k = res.occluded_frames()[10]
        self.assertAlmostEqual(back.frames[k].visibility, res.frames[k].visibility, 3)


@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestBatch(unittest.TestCase):
    def test_plan_run_apply(self):
        from osisub.core.asstime import round_ms
        from osisub.media import batch, occlude
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        from tests import media_fixtures as mf
        p = mf.path("occlude.mkv")
        idx = build_index(p)
        fm = FrameMap(idx.ms)
        g = FrameGrabber(p, idx)
        C10, _ = mf.motion_truth(10)
        C60, c60 = mf.motion_truth(60)

        def t(k):
            return round_ms(fm.time_at_frame(k, START))
        evs = [Event(start=t(10), end=t(50), text="{\\an5\\pos(%.1f,%.1f)\\fs30}SIGN TEXT" % (C10.real, C10.imag)),
               Event(start=t(60), end=t(96), text="{\\an5\\pos(%.2f,%.2f)\\bord0\\shad0\\p1}m 0 0 l 100 0 100 50 0 50"
                     % (C60.real, C60.imag)),
               Event(start=0, end=500, text="diyalog"),
               Event(start=t(5), end=t(8), text="{\\pos(-500,-500)}ekran dışı")]
        doc = _doc_with(evs)
        jobs = batch.plan(doc, [0, 1, 3], fm, len(idx), (640, 360), scenes=[30])
        self.assertEqual([(j.row, j.start, j.end, j.ref) for j in jobs], [(0, 10, 29, 10), (1, 60, 95, 60), (3, 5, 7, 5)])
        self.assertTrue(any("sahne kesmesi" in n for n in jobs[0].notes))
        self.assertIsNotNone(jobs[0].roi)
        self.assertIsNotNone(jobs[2].error)            # alan görüntü dışında
        # çizim kutusu: 100x50 \an5 -> pad %15 ile ~130x65, merkezi \pos
        x, y, w, h = jobs[1].roi
        self.assertAlmostEqual(x + w / 2, C60.real, delta=1)
        self.assertAlmostEqual(w, 130, delta=2)
        progress = []
        batch.run(jobs, g.get, occlusion_opts=occlude.OcclusionOptions(),
                  progress=lambda p_, row, k: progress.append(p_))
        g.close()
        self.assertIsNone(jobs[0].error)
        self.assertIsNone(jobs[1].error)
        self.assertAlmostEqual(progress[-1], 1.0, 3)
        self.assertTrue(jobs[1].occlusion.occluded_frames())
        events, rows, warns = batch.apply_jobs(doc, jobs, fm, (640, 360), M.ApplyOptions())
        # satır 1: 40 kare (iz kesmede biter, sonrası tutulur), satır 2: 36 kare
        self.assertEqual(len(events), 40 + 36 + 1 + 1)
        self.assertEqual(events[40].text.count("\\p1"), 1)
        self.assertEqual(events[76].text, "diyalog")
        self.assertEqual(events[77].text, "{\\pos(-500,-500)}ekran dışı")      # hatalı iş: satır aynen
        self.assertTrue(any("Satır 4" in w for w in warns))
        self.assertTrue(any("sahne kesmesi" in w for w in warns))
        # izlenen çizim satırı tabelayı izler (yer gerçeği)
        for k in (60, 75, 95):
            ln = events[40 + (k - 60)]
            x, y = T.get_pos(ln.text)
            Ck, _ = mf.motion_truth(k)
            self.assertLess(abs(complex(x, y) - Ck), 1.5, k)


# ---------------------------------------------------------------------------
# ECC ve renk anahtarlı izleyiciler, libass ile yakarak görünürlük denetimi
# ---------------------------------------------------------------------------

@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestMoreTrackers(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from osisub.media.frameindex import build_index
        from osisub.media.frames import FrameGrabber
        from tests import media_fixtures as mf
        cls.mf = mf
        cls.g = {}
        for name in ("motion.mkv", "color.mkv"):
            p = mf.path(name)
            cls.g[name] = FrameGrabber(p, build_index(p))

    @classmethod
    def tearDownClass(cls):
        for g in cls.g.values():
            g.close()

    def _err(self, tr, roi, rotate=True, frames=None):
        mf = self.mf
        Cr, cr = mf.motion_truth(tr.ref_frame, rotate)
        z = complex(roi[0] + roi[2] / 2, roi[1] + roi[3] / 2)
        ep = es = er = 0.0
        for f in tr.frames:
            if frames is not None and f.frame not in frames:
                continue
            Ck, ck = mf.motion_truth(f.frame, rotate)
            cc = ck / cr
            ep = max(ep, abs((Ck + cc * (z - Cr)) - (f.c * z + f.t)))
            es = max(es, abs(abs(cc) - f.scale) * 100)
            er = max(er, abs(T.norm_angle(-math.degrees(cmath.phase(cc)) - f.rot)))
        return ep, es, er

    def test_ecc_similarity(self):
        from osisub.media import tracker
        mf = self.mf
        C, c = mf.motion_truth(40)
        s = abs(c)
        pw, ph = mf.MOTION_PATCH
        roi = (C.real - pw * s * 0.4, C.imag - ph * s * 0.35, pw * s * 0.8, ph * s * 0.7)
        tr = tracker.track(self.g["motion.mkv"].get, 0, 95, 40, roi, method="ecc")
        self.assertEqual(tr.bad_frames(), [])
        ep, es, er = self._err(tr, roi)
        self.assertLess(ep, 0.4)
        self.assertLess(es, 0.6)
        self.assertLess(er, 0.2)
        with self.assertRaises(ValueError):          # dokusuz alan
            tracker._EccRunner(np.full((100, 100), 128, np.uint8), (10, 10, 50, 50))

    def test_color_key_with_occlusion(self):
        from osisub.media import tracker
        mf = self.mf
        C, c = mf.motion_truth(10, False)
        s = abs(c)
        pw, ph = mf.MOTION_PATCH
        roi = (C.real - pw * s * 0.6, C.imag - ph * s * 0.6, pw * s * 1.2, ph * s * 1.2)
        tr = tracker.track(self.g["color.mkv"].get, 0, 95, 10, roi, method="color")
        self.assertTrue(any("anahtar ton" in n for n in tr.notes))
        ok = {f.frame for f in tr.frames if f.ok}
        # direk tabelayı belirgin örttüğü kareler güvensiz işaretlenmeli, sonra yeniden yakalanmalı
        self.assertFalse(ok & set(range(34, 71)))
        self.assertTrue(set(range(80, 96)) <= ok)
        self.assertGreater(len(ok), 40)
        ep, es, _er = self._err(tr, roi, rotate=False, frames=ok)
        self.assertLess(ep, 4.0)
        self.assertLess(es, 4.0)
        # anahtar renk elle verilebilir
        tr2 = tracker.track(self.g["color.mkv"].get, 5, 25, 10, roi, method="color", key_rgb=(210, 30, 30))
        ep2, _es, _er = self._err(tr2, roi, rotate=False, frames={f.frame for f in tr2.frames if f.ok})
        self.assertLess(ep2, 4.0)


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestRenderQc(unittest.TestCase):
    def test_measured_visibility(self):
        from osisub.media import render_qc
        sq = "\\an7\\bord0\\shad0\\p1"
        evs = [Event(start=0, end=40, text="{%s\\pos(100,100)\\iclip(0,0,150,400)}m 0 0 l 100 0 100 100 0 100" % sq),
               Event(start=40, end=80, text="{%s\\pos(100,100)}m 0 0 l 100 0 100 100 0 100" % sq),
               Event(start=0, end=3000, text="{\\an5\\pos(320,300)\\clip(0,0,1,1)}Gizli yazı"),
               Event(start=5000, end=5040, text="{%s\\pos(100,100)\\1a&H80&\\clip(m 100 100 l 200 100 100 200)}"
                     "m 0 0 l 100 0 100 100 0 100" % sq)]
        res = {v.row: v for v in render_qc.measure(_doc_with(evs), range(4), (640, 360))}
        self.assertAlmostEqual(res[0].ratio, 0.5, delta=0.02)
        self.assertAlmostEqual(res[1].ratio, 1.0, delta=0.01)
        self.assertAlmostEqual(res[1].full, 10000, delta=300)
        self.assertEqual(res[2].visible, 0)
        self.assertGreater(res[2].full, 500)
        self.assertAlmostEqual(res[3].ratio, 0.5, delta=0.03)
        self.assertEqual(render_qc.strip_clips("{\\pos(1,2)\\iclip(m 0 0 l 1 1)}a{\\clip(1,2,3,4)}b"),
                         "{\\pos(1,2)}ab")


# ---------------------------------------------------------------------------
# Bölme (ekransız, yedek kare gösterici)
# ---------------------------------------------------------------------------

@unittest.skipUnless(have_ffmpeg() and HAVE_CV, "ffmpeg ya da OpenCV yok")
class TestMotionPanel(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        os.environ.pop("OSISUB_MPV_VO", None)
        from PySide6.QtWidgets import QApplication
        from osisub.config import Config
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        from tests import media_fixtures as mf
        from tests.test_video_ui import pump
        cls.app = QApplication.instance() or QApplication([])
        cls.pump = staticmethod(pump)
        cls.mf = mf
        d = tempfile.mkdtemp()
        cls.ass = os.path.join(d, "tabela.ass")
        C, _c = mf.motion_truth(40)
        doc = _doc_with([])
        with open(cls.ass, "w", encoding="utf-8") as fh:
            fh.write(doc.to_text().rstrip("\r\n") + "\n")
            fh.write("Dialogue: 0,0:00:00.00,0:00:04.00,Default,,0,0,0,,"
                     "{\\an5\\pos(%.2f,%.2f)\\clip(%d,%d,%d,%d)}TABELA\n"
                     % (C.real, C.imag, C.real - 50, C.imag - 25, C.real + 50, C.imag + 25))
            fh.write("Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,Sonraki\n")
        cfg = Config(os.path.join(tempfile.mkdtemp(), "c.json"))
        cfg["load_aegisub_scripts"] = False
        cfg["auto_load_media"] = False
        ctx = Context(cfg)
        cls.win = MainWindow(ctx, headless=True)
        cls.ctx = ctx
        cls.win.resize(1500, 1000)
        cls.win.show()
        cls.win.open_path(cls.ass, force=True)
        cls.win.media.open_video(mf.path("motion.mkv"), with_audio=False)
        assert pump(until=lambda: cls.win.media.index is not None, timeout=30)
        pump(0.2)

    @classmethod
    def tearDownClass(cls):
        cls.win.close()

    def test_full_flow(self):
        win, ctx = self.win, self.ctx
        mp = win.motion_panel
        ctx.set_active(0, select_only=True)
        win.video.seek_frame(40)
        self.pump(0.1)
        win.show_motion()
        self.assertEqual(win.video.tool, "track")
        self.assertEqual((mp.sp_start.value(), mp.sp_end.value(), mp.sp_ref.value()), (0, 95, 40))
        self.assertIsNotNone(mp.roi)                      # \clip'ten alındı
        self.assertAlmostEqual(mp.roi[2], 100, delta=1)
        # tuvalde fareyle alan çizme
        cv = win.video.canvas
        got = []
        cv.roi_selected.connect(got.append)
        from PySide6.QtCore import QPoint, Qt
        from PySide6.QtTest import QTest
        a = cv.vid_to_widget(*(self.mf.motion_truth(40)[0].real - 50, self.mf.motion_truth(40)[0].imag - 25))
        b = cv.vid_to_widget(*(self.mf.motion_truth(40)[0].real + 50, self.mf.motion_truth(40)[0].imag + 25))
        QTest.mousePress(cv, Qt.LeftButton, Qt.NoModifier, QPoint(int(a.x()), int(a.y())))
        QTest.mouseMove(cv, QPoint(int(b.x()), int(b.y())))
        QTest.mouseRelease(cv, Qt.LeftButton, Qt.NoModifier, QPoint(int(b.x()), int(b.y())))
        self.assertEqual(len(got), 1)
        self.assertAlmostEqual(got[0][2], 100, delta=4)
        self.assertEqual(mp.roi, got[0])
        # izle
        self.assertTrue(mp.start_tracking())
        self.assertTrue(mp.wait(60))
        self.assertIsNone(mp.error, mp.error)
        self.assertEqual(len(mp.track), 96)
        self.assertIn("96 kare", mp.summary.text())
        cv.grab()                                          # iz çizimi hata vermemeli
        mp.plot.grab()
        # grafiğe tıklama -> kare
        from PySide6.QtCore import QPoint as QP
        QTest.mouseClick(mp.plot, Qt.LeftButton, Qt.NoModifier, QP(mp.plot.width() - 7, 50))
        self.pump(0.1)
        self.assertEqual(win.media.frame, 95)
        # kaydet / aç
        p = os.path.join(tempfile.mkdtemp(), "iz.assmotiontrack")
        mp.save(p)
        tr2 = mp.load(p)
        self.assertEqual(len(tr2), 96)
        # uygula (sıkıştırılmış) — tek geri al
        n_before = len(ctx.doc.events)
        mp.mode.setCurrentIndex(1)
        ctx.set_selection({0}, 0)
        rows = mp.apply()
        self.assertTrue(rows)
        self.assertEqual(len(ctx.doc.events), n_before - 1 + len(rows))
        self.assertEqual(ctx.doc.events[len(rows)].text, "Sonraki")
        self.assertTrue(all("\\clip" in ctx.doc.events[r].text for r in rows))
        ctx.do_undo()
        self.assertEqual(len(ctx.doc.events), n_before)
        # kare kare
        mp.mode.setCurrentIndex(0)
        ctx.set_selection({0}, 0)
        rows = mp.apply()
        self.assertEqual(len(rows), 96)
        ctx.do_undo()
        # AE yapıştırma: satırın ilk karesine yerleşir
        ae = mp.track.to_ae()
        ctx.set_selection({0}, 0)
        tr3 = mp.load_ae_text(ae)
        self.assertEqual((tr3.start_frame, tr3.end_frame), (0, 95))
        self.assertEqual(mp.ctx.config["motion"]["mode"], "frame")
        # örtme: temiz videoda yanlış alarm yok
        mp.set_track(mp.track)
        tr_main = tracker_main = None  # noqa: F841
        win.video.seek_frame(40)
        self.pump(0.1)
        mp.range_from_lines()
        self.assertTrue(mp.start_tracking())
        self.assertTrue(mp.wait(60))
        self.assertTrue(mp.start_occlusion())
        self.assertTrue(mp.wait(60))
        self.assertIsNone(mp.error, mp.error)
        self.assertIsNotNone(mp.occlusion)
        self.assertEqual(mp.occlusion.occluded_frames(), [])
        self.assertIn("0 karede", mp.occ_lbl.text())
        cv.grab()
        mp.plot.grab()
        # toplu: iki satır, tek geri al
        n0 = len(ctx.doc.events)
        ctx.set_selection({0, 1}, 0)
        self.assertTrue(mp.start_batch())
        self.assertTrue(mp.wait(120))
        self.assertIsNone(mp.error, mp.error)
        self.assertIn("2/2", mp.summary.text())
        self.assertGreater(len(ctx.doc.events), n0 + 50)
        ctx.do_undo()
        self.assertEqual(len(ctx.doc.events), n0)
        # elle düzeltme: İzleme aracında Ctrl+sürükle bu kareyi, Ctrl+Shift+sürükle sonrakileri kaydırır
        ctx.set_active(0, select_only=True)
        win.video.seek_frame(40)
        self.pump(0.1)
        mp.range_from_lines()
        self.assertTrue(mp.start_tracking())
        self.assertTrue(mp.wait(60))
        self.assertFalse(mp.b_undo_track.isEnabled())
        win.video.set_tool("track")
        win.video.seek_frame(60)
        self.pump(0.2)
        tr0 = mp.track
        f60 = tr0.at(60)
        p0 = cv.vid_to_widget(*f60.map(mp.roi[0] + 10, mp.roi[1] + 10))
        p1 = cv.vid_to_widget(*f60.map(mp.roi[0] + 10, mp.roi[1] + 10))
        p1.setX(p1.x() + 20)
        QTest.mousePress(cv, Qt.LeftButton, Qt.ControlModifier, QPoint(int(p0.x()), int(p0.y())))
        QTest.mouseMove(cv, QPoint(int(p1.x()), int(p0.y())))
        QTest.mouseRelease(cv, Qt.LeftButton, Qt.ControlModifier, QPoint(int(p1.x()), int(p0.y())))
        moved = mp.track.at(60).t.real - tr0.at(60).t.real
        self.assertGreater(moved, 5)
        self.assertEqual(mp.track.at(61).t, tr0.at(61).t)            # yalnız bu kare
        self.assertIn("elle düzeltildi", mp.track.notes)
        self.assertTrue(mp.b_undo_track.isEnabled())
        mp.begin_track_edit()
        mp.shift_track(70, 3 + 0j, True)
        self.assertEqual(mp.track.at(95).t, tr0.at(95).t + 3)         # referanstan uzaktakiler de
        self.assertEqual(mp.track.at(69).t, tr0.at(69).t)
        mp.undo_track()
        self.assertAlmostEqual(mp.track.at(60).t.real - tr0.at(60).t.real, moved, 3)
        mp.undo_track()
        self.assertEqual(mp.track.at(60).t, tr0.at(60).t)
        # libass denetimi: uygulanan satırlar ölçülür, gizli satır seçilir
        ctx.set_selection({0}, 0)
        mp.mode.setCurrentIndex(0)
        rows = mp.apply()
        ctx.update_events({rows[5]: {"text": ctx.doc.events[rows[5]].text + "{\\clip(0,0,1,1)}"}}, "test")
        ev5 = ctx.doc.events[rows[5]]
        ctx.update_events({rows[5]: {"text": "{\\clip(0,0,1,1)}" + ev5.text}}, "test")
        ctx.set_selection(set(rows[:10]), rows[0])
        self.assertTrue(mp.start_render_qc())
        self.assertTrue(mp.wait(60))
        self.assertIsNone(mp.error, mp.error)
        self.assertEqual(len(mp.qc_result), 10)
        self.assertEqual(ctx.selected_rows(), [rows[5]])
        self.assertIn("1 satırın", mp.warn_lbl.text())
        ctx.do_undo()
        ctx.do_undo()
        ctx.do_undo()
        self.assertEqual(len(ctx.doc.events), n0)
        win.video.set_tool("video")


if __name__ == "__main__":
    unittest.main()
