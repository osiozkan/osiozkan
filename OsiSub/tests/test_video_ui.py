"""Video/ses bölmeleri — ekransız, gerçek libmpv (vo=null) ve yedek kare gösterici ile."""

import os
import shutil
import tempfile
import time
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"
os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())
os.environ.setdefault("LOCALAPPDATA", tempfile.mkdtemp())
os.environ.setdefault("APPDATA", tempfile.mkdtemp())

import numpy as np  # noqa: E402
from PySide6.QtCore import QPoint, Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from osisub.config import Config  # noqa: E402
from tests.media_fixtures import have_ffmpeg, path  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
app = QApplication.instance() or QApplication([])

try:
    import mpv  # noqa: F401
    HAVE_MPV = True
except Exception:  # noqa: BLE001
    HAVE_MPV = False


def pump(sec=0.2, until=None, timeout=20.0):
    t = time.time()
    while True:
        app.processEvents()
        time.sleep(0.01)
        if until is not None:
            if until():
                return True
            if time.time() - t > timeout:
                return False
        elif time.time() - t > sec:
            return True


def make(use_mpv: bool):
    if use_mpv:
        os.environ["OSISUB_MPV_VO"] = "null"
    else:
        os.environ.pop("OSISUB_MPV_VO", None)
    from osisub.ui import theme
    from osisub.ui.context import Context
    from osisub.ui.mainwindow import MainWindow
    d = tempfile.mkdtemp()
    shutil.copy(path("cfr.mkv"), d)
    data = open(os.path.join(HERE, "fixtures", "sample.ass"), "rb").read()
    ass = os.path.join(d, "sample.ass")
    with open(ass, "wb") as f:
        f.write(data.replace(b"Video File: ornek.mkv", b"Video File: cfr.mkv"))
    cfg = Config(os.path.join(tempfile.mkdtemp(), "c.json"))
    cfg["load_aegisub_scripts"] = False
    theme.apply(app, "dark")
    ctx = Context(cfg)
    win = MainWindow(ctx, headless=True)
    win.resize(1500, 1000)
    win.show()
    win.open_path(ass, force=True)
    ok = pump(until=lambda: win.media.index is not None and win.media.audio is not None)
    assert ok, getattr(win, "last_media_error", "zaman aşımı")
    pump(0.2)
    return win, ctx, ass


@unittest.skipUnless(have_ffmpeg() and HAVE_MPV, "ffmpeg ya da libmpv yok")
class TestMpvVideo(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.win, cls.ctx, cls.ass = make(True)

    @classmethod
    def tearDownClass(cls):
        cls.win.video.stop_backend()
        cls.win.close()

    def wait_frame(self, f):
        v = self.win.video
        idx = self.win.media.index
        return pump(until=lambda: v.mpv.time_pos is not None and idx.frame_at_seconds(v.mpv.time_pos) == f,
                    timeout=5)

    def test_backend_and_project_media(self):
        self.assertEqual(self.win.video.backend, "mpv")
        self.assertEqual(len(self.win.media.index), 239)
        self.assertEqual(os.path.basename(self.win.media.audio_path), "cfr.mkv")
        self.assertIn("libmpv", self.win.status_lbl.text())

    def test_follow_line_and_exact_seek(self):
        self.ctx.set_active(1, select_only=True)      # 4000 ms
        pump(0.1)
        self.assertEqual(self.win.media.frame, 96)
        self.assertTrue(self.wait_frame(96))
        self.win.video.step(1)
        self.assertTrue(self.wait_frame(97))

    def test_set_times_from_frame(self):
        ctx = self.ctx
        ctx.set_active(0, select_only=True)
        pump(0.05)
        self.win.video.seek_frame(30)
        self.win._set_from_frame(False)
        fm = ctx.frame_map
        self.assertEqual(fm.frame_from_ms(ctx.doc.events[0].start), 30)
        self.win.video.seek_frame(70)
        self.win._set_from_frame(True)
        # bitiş: kare 70 hâlâ görünür, 71 görünmez
        e = ctx.doc.events[0].end
        self.assertEqual(fm.frame_at_time(e, 2), 70)
        ctx.do_undo()
        ctx.do_undo()

    def test_snap_to_frames(self):
        ctx = self.ctx
        ctx.set_selection({0, 1, 4}, 0)
        self.win.snap_to_frames()
        fm = ctx.frame_map
        for r in (0, 1, 4):
            ev = ctx.doc.events[r]
            s = fm.time_at_frame(fm.frame_at_time(ev.start, 1), 1)
            self.assertLessEqual(abs(ev.start - s), 5)
        ctx.do_undo()

    def test_live_subtitle_preview(self):
        v = self.win.video
        ctx = self.ctx
        ctx.set_active(0, select_only=True)
        ctx.update_events({0: {"text": "{\\an5\\fs120\\1c&H0000FF&\\bord0}██████"}}, "test")
        pump(0.4)
        with open(v._sub_file, "rb") as f:
            self.assertIn("██████".encode(), f.read())
        v.seek_frame(48)            # 2.0 sn — satır 1.0–3.5 arası görünür
        self.assertTrue(self.wait_frame(48))
        pump(0.2)
        with_s = v.grab_frame(True).astype(int)
        without = v.grab_frame(False).astype(int)
        changed = (np.abs(with_s - without).sum(axis=2) > 60).sum()
        self.assertGreater(changed, 2000)
        ctx.do_undo()

    def test_save_writes_project_media(self):
        p = os.path.join(os.path.dirname(self.ass), "kayit.ass")
        self.ctx.save_document(p)
        self.win._store_media_paths()
        g = self.ctx.doc.garbage()
        self.assertEqual(g["Video File"], "cfr.mkv")
        self.assertEqual(g["Audio File"], "?video")

    def test_lua_video_api(self):
        lua = os.path.join(tempfile.mkdtemp(), "v.lua")
        with open(lua, "w", encoding="utf-8") as f:
            f.write("""
aegisub.register_macro("V", "", function(subs, sel)
  local l = subs[sel[1]]
  local w, h = aegisub.video_size()
  local kf = aegisub.keyframes()
  local fr = aegisub.get_frame(24, false)
  local s, e = aegisub.get_audio_selection()
  l.effect = table.concat({w, h, #kf, kf[2], aegisub.frame_from_ms(1001), aegisub.ms_from_frame(24),
                           fr:width(), fr:getPixelFormatted(10, 10), tostring(s)}, ",")
  subs[sel[1]] = l
end)
""")
        sc = self.win.scripts.add(lua)
        self.assertIsNone(sc.error)
        self.ctx.set_selection({0}, 0)
        r = self.win.run_macro(sc.macros[0])
        self.assertEqual(r.status, "ok", r.error)
        parts = self.ctx.doc.events[0].effect.split(",")
        img = self.win.media.grabber.get(24)
        rr, gg, bb = (int(x) for x in img[10, 10])
        self.assertEqual(parts[:6], ["640", "360", "5", "48", "24", str(self.ctx.frame_map.ms_from_frame(24))])
        self.assertEqual(parts[6], "640")
        self.assertEqual(parts[7], f"&H{bb:02X}{gg:02X}{rr:02X}&")
        self.assertEqual(parts[8], str(self.ctx.doc.events[0].start))
        self.ctx.do_undo()


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestAudioPanel(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.win, cls.ctx, cls.ass = make(False)
        cls.view = cls.win.audio.view
        cls.view.resize(1000, 160)
        pump(0.1)

    @classmethod
    def tearDownClass(cls):
        cls.win.video.stop_backend()
        cls.win.close()

    def test_still_backend_shows_frame(self):
        self.assertEqual(self.win.video.backend, "still")
        self.win.video.seek_frame(50)
        self.assertIsNotNone(self.win.video.still.img)
        self.assertEqual(self.win.video.grab_frame(False).shape, (360, 640, 3))

    def click(self, ms, button=Qt.LeftButton, mods=Qt.NoModifier):
        v = self.view
        x = int(round(v.x_of(ms)))
        QTest.mouseClick(v, button, mods, QPoint(x, 80))
        pump(0.02)

    def test_left_right_click_times(self):
        ctx, v = self.ctx, self.view
        ctx.set_active(1, select_only=True)
        v.snap = False
        v.set_view(3000, 5.0)
        self.click(4500)
        self.assertLessEqual(abs(ctx.doc.events[1].start - 4500), 10)
        self.click(5800, Qt.RightButton)
        self.assertLessEqual(abs(ctx.doc.events[1].end - 5800), 10)
        ctx.do_undo()
        ctx.do_undo()
        self.assertEqual((ctx.doc.events[1].start, ctx.doc.events[1].end), (4000, 6000))

    def test_snap_to_keyframe(self):
        ctx, v = self.ctx, self.view
        ctx.set_active(1, select_only=True)
        v.snap = True
        v.set_view(3000, 5.0)
        kf_ms = ctx.frame_map.time_at_frame(96)        # 4004 ms
        self.click(4500)                               # başlangıcı uzaklaştır (sınır tutamacı devreye girmesin)
        self.click(kf_ms + 35)                         # 7 px: yaslama mesafesinde, tutamaç dışında
        self.assertEqual(ctx.frame_map.frame_from_ms(ctx.doc.events[1].start), 96)
        self.click(kf_ms + 60, mods=Qt.ShiftModifier)  # Shift: yaslama kapalı
        self.assertGreater(ctx.doc.events[1].start, kf_ms + 40)
        for _ in range(3):
            ctx.do_undo()
        self.assertEqual(ctx.doc.events[1].start, 4000)

    def test_drag_edge(self):
        ctx, v = self.ctx, self.view
        ctx.set_active(1, select_only=True)
        v.snap = False
        v.set_view(3000, 5.0)
        x0 = int(v.x_of(ctx.doc.events[1].end))
        QTest.mousePress(v, Qt.LeftButton, Qt.NoModifier, QPoint(x0, 80))
        QTest.mouseMove(v, QPoint(x0 + 60, 80))
        QTest.mouseRelease(v, Qt.LeftButton, Qt.NoModifier, QPoint(x0 + 60, 80))
        pump(0.02)
        self.assertLessEqual(abs(ctx.doc.events[1].end - 6300), 10)
        self.assertEqual(ctx.doc.events[1].start, 4000)
        ctx.do_undo()
        self.assertEqual(ctx.doc.events[1].end, 6000)      # sürükleme tek geri al adımı

    def test_render_waveform_and_spectrum(self):
        v = self.view
        v.set_view(0, 10.0)
        v.spectrum = False
        img = v.grab().toImage()
        self.assertFalse(img.isNull())
        v.spectrum = True
        img2 = v.grab().toImage()
        self.assertFalse(img2.isNull())
        v.spectrum = False

    def test_zoom_wheel_keeps_anchor(self):
        v = self.view
        v.set_view(1000, 10.0)
        anchor = v.ms_of(500)
        from PySide6.QtCore import QPointF
        from PySide6.QtGui import QWheelEvent
        ev = QWheelEvent(QPointF(500, 80), v.mapToGlobal(QPointF(500, 80)), QPoint(0, 0), QPoint(0, 120),
                         Qt.NoButton, Qt.ControlModifier, Qt.NoScrollPhase, False)
        v.wheelEvent(ev)
        self.assertLess(v.ms_per_px, 10.0)
        self.assertAlmostEqual(v.ms_of(500), anchor, delta=1.0)


if __name__ == "__main__":
    unittest.main()


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestStripLinkedTiming(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.win, cls.ctx, cls.ass = make(False)

    @classmethod
    def tearDownClass(cls):
        cls.win.video.stop_backend()
        cls.win.strip.shutdown()
        cls.win.close()

    def test_frame_strip_thumbnails_and_click(self):
        s = self.win.strip
        s.resize(900, 90)
        self.win.video.seek_frame(60)
        s._refresh()
        frames = s.visible_frames()
        self.assertIn(60, frames)
        ok = pump(until=lambda: all(f in s.cache for f in frames), timeout=15)
        self.assertTrue(ok)
        img = s.grab().toImage()
        self.assertFalse(img.isNull())
        # tıklanan küçük resim o kareye götürür
        s.repaint()
        f, r = s._slots[1]
        QTest.mouseClick(s, Qt.LeftButton, Qt.NoModifier, r.center().toPoint())
        pump(0.05)
        self.assertEqual(self.win.media.frame, f)
        # küçük resim, ffmpeg'in o karesiyle aynı içerikte (ortalama renk)
        from PySide6.QtGui import QColor
        th = s.cache[f]
        full = self.win.media.grabber.get(f)
        mean_full = full.reshape(-1, 3).mean(axis=0)
        px = [QColor(th.pixel(x, y)) for x in range(0, th.width(), 4) for y in range(0, th.height(), 4)]
        mean_th = np.array([[c.red(), c.green(), c.blue()] for c in px]).mean(axis=0)
        self.assertLess(float(np.abs(mean_full - mean_th).max()), 12)

    def test_linked_boundary_ctrl_drag(self):
        ctx = self.ctx
        # satır 1 bitişi = satır 2 başlangıcı olsun
        ctx.update_events({0: {"end": 4000}}, "hazırlık")
        v = self.win.audio.view
        v.resize(1000, 160)
        v.snap = False
        ctx.set_active(0, select_only=True)
        v.set_view(3000, 5.0)
        x0 = int(v.x_of(4000))
        QTest.mousePress(v, Qt.LeftButton, Qt.ControlModifier, QPoint(x0, 80))
        QTest.mouseMove(v, QPoint(x0 + 40, 80))
        QTest.mouseRelease(v, Qt.LeftButton, Qt.ControlModifier, QPoint(x0 + 40, 80))
        pump(0.02)
        self.assertLessEqual(abs(ctx.doc.events[0].end - 4200), 10)
        self.assertEqual(ctx.doc.events[1].start, ctx.doc.events[0].end)
        ctx.do_undo()
        self.assertEqual((ctx.doc.events[0].end, ctx.doc.events[1].start), (4000, 4000))
        ctx.do_undo()

    def test_timing_pp_keyframes(self):
        from osisub.ui.timing_dialog import TimingDialog
        ctx = self.ctx
        ctx.update_events({1: {"start": 3900}}, "hazırlık")     # kare 94; anahtar kare 96
        dlg = TimingDialog(ctx, self.win)
        self.assertTrue(dlg.g_kf.isEnabled())
        dlg.g_kf.setChecked(True)
        dlg.g_lead.setChecked(False)
        dlg.g_adj.setChecked(False)
        ctx.set_selection({1}, 1)
        dlg.scope.setCurrentIndex(0)
        dlg.apply()
        self.assertEqual(ctx.frame_map.frame_from_ms(ctx.doc.events[1].start), 96)
        ctx.do_undo()
        ctx.do_undo()


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestScenesUi(unittest.TestCase):
    def test_find_scenes_marks_and_timing(self):
        win, ctx, ass = make(False)
        try:
            win.media.open_video(path("cuts.mkv"), with_audio=False)
            self.assertTrue(pump(until=lambda: win.media.index is not None and len(win.media.index) > 100))
            win.find_scenes()
            self.assertTrue(pump(until=lambda: win.media.scenes == [48, 96, 144], timeout=30))
            from osisub.ui.timing_dialog import TimingDialog
            ctx.update_events({0: {"start": 1950, "end": 3900}}, "hazırlık")
            ctx.set_selection({0}, 0)
            dlg = TimingDialog(ctx, win)
            dlg.g_kf.setChecked(True)
            self.assertTrue(dlg.use_scenes.isEnabled())
            dlg.g_lead.setChecked(False)
            dlg.g_adj.setChecked(False)
            dlg.use_scenes.setChecked(True)
            dlg.scope.setCurrentIndex(0)
            dlg.apply()
            self.assertEqual(ctx.frame_map.frame_from_ms(ctx.doc.events[0].start), 48)
            win.strip.resize(800, 90)
            self.assertFalse(win.strip.grab().toImage().isNull())
        finally:
            win.video.stop_backend()
            win.strip.shutdown()
            win.close()


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestSpeechGlossaryUi(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.win, cls.ctx, cls.ass = make(False)
        cls.win.media.open_audio(path("bursts.wav"))
        # dikkat: video açılınca kendi sesi de yüklenir; bursts.wav'ın (8 sn) gelmesini bekle
        assert pump(until=lambda: cls.win.media.audio is not None and cls.win.media.audio.duration_ms == 8000
                    and len(cls.win.media.speech) == 2 and cls.win.media.speech[0][1] < 3000, timeout=20)

    @classmethod
    def tearDownClass(cls):
        cls.win.video.stop_backend()
        cls.win.strip.shutdown()
        cls.win.close()

    def test_timing_dialog_speech_snap(self):
        from osisub.ui.timing_dialog import TimingDialog
        ctx = self.ctx
        ctx.update_events({0: {"start": 1200, "end": 2300}}, "hazırlık")
        ctx.set_selection({0}, 0)
        dlg = TimingDialog(ctx, self.win)
        self.assertTrue(dlg.g_speech.isEnabled())
        dlg.g_speech.setChecked(True)
        for g in (dlg.g_lead, dlg.g_adj, dlg.g_kf):
            g.setChecked(False)
        dlg.scope.setCurrentIndex(0)
        dlg.apply()
        a, b = self.win.media.speech[0]
        from osisub.core.asstime import round_ms
        self.assertEqual((ctx.doc.events[0].start, ctx.doc.events[0].end), (round_ms(a), round_ms(b)))
        ctx.do_undo()
        ctx.do_undo()

    def test_lines_from_speech_single_undo(self):
        ctx = self.ctx
        # konuşma aralıklarını boşalt (örnek dosyanın satırları 1–3.5 ve 4–6 sn'yi kaplıyor)
        ctx.update_events({0: {"start": 20000, "end": 21000}, 1: {"start": 22000, "end": 23000}}, "hazırlık")
        n = len(ctx.doc.events)
        rows = self.win.lines_from_speech()
        self.assertGreaterEqual(len(ctx.doc.events), n + 1)
        self.assertTrue(all(ctx.doc.events[r].text == "" for r in rows))
        ctx.do_undo()
        self.assertEqual(len(ctx.doc.events), n)
        ctx.do_undo()

    def test_audio_view_draws_speech(self):
        v = self.win.audio.view
        v.resize(1000, 160)
        v.set_view(0, 8.0)
        img = v.grab().toImage()
        from PySide6.QtGui import QColor
        x = int(v.x_of(1700))
        c = QColor(img.pixel(x, v.height() - 2))
        self.assertGreater(c.green(), 120)          # turkuaz konuşma şeridi
        x2 = int(v.x_of(3200))
        self.assertLess(QColor(img.pixel(x2, v.height() - 2)).green(), 120)

    def test_glossary_panel_and_qc(self):
        gp = self.win.glossary_panel
        gp.add_term("Stand", "Stand")
        r = gp.table.rowCount() - 1
        from PySide6.QtWidgets import QTableWidgetItem
        gp.table.setItem(r, 2, QTableWidgetItem("Stant|Ruh"))
        self.assertEqual(self.ctx.glossary.terms[-1].forbidden, ["Stant", "Ruh"])
        p = os.path.join(os.path.dirname(self.ass), "terimler.tsv")
        self.assertTrue(gp.save_as(p))
        self.assertTrue(os.path.exists(p))
        self.ctx.update_events({0: {"text": "Stantını çağır"}}, "hazırlık")
        en = self.win.qc_panel.enabled()
        for i in range(self.win.qc_panel.rules.count()):
            it = self.win.qc_panel.rules.item(i)
            it.setCheckState(Qt.Checked if it.data(Qt.UserRole) == "glossary" else Qt.Unchecked)
        iss = self.win.run_qc()
        self.assertEqual([(i.row, i.rule) for i in iss], [(0, "glossary")])
        for k in range(self.win.qc_panel.tree.topLevelItemCount()):
            self.win.qc_panel.tree.topLevelItem(k).setCheckState(0, Qt.Checked)
        self.win.qc_panel.fix_checked()
        self.assertEqual(self.ctx.doc.events[0].text, "Standını çağır")
        while self.ctx.undo.can_undo:
            self.ctx.do_undo()
        for i in range(self.win.qc_panel.rules.count()):
            it = self.win.qc_panel.rules.item(i)
            it.setCheckState(Qt.Checked if it.data(Qt.UserRole) in en else Qt.Unchecked)


def _asr_ok():
    try:
        import faster_whisper  # noqa: F401
    except ImportError:
        return False
    return have_ffmpeg() and os.path.exists(path("speech.wav"))


@unittest.skipUnless(_asr_ok(), "faster-whisper ya da espeak-ng yok")
class TestAsrUi(unittest.TestCase):
    def test_whisper_to_reference_and_lines(self):
        win, ctx, ass = make(False)
        try:
            win.media.open_audio(path("speech.wav"))
            assert pump(until=lambda: win.media.audio is not None and abs(win.media.audio.duration_ms - 9580) < 30
                        and len(win.media.speech) == 3, timeout=20)
            dlg = win.show_asr()
            dlg.model.setCurrentText("tiny")
            dlg.lang.setCurrentIndex(dlg.lang.findData("en"))
            dlg.output.setCurrentIndex(0)
            res = dlg.start()
            self.assertTrue(res, dlg.error)
            self.assertTrue(win.reference.loaded)
            self.assertIn("Whisper", win.reference.path)
            self.assertIn("market", " ".join(e.text for e in win.reference.events).lower())
            dlg2 = win.show_asr()
            dlg2.model.setCurrentText("tiny")
            dlg2.lang.setCurrentIndex(dlg2.lang.findData("en"))
            dlg2.output.setCurrentIndex(1)
            n = len(ctx.doc.events)
            dlg2.start()
            self.assertGreater(len(ctx.doc.events), n)
            ctx.do_undo()
            self.assertEqual(len(ctx.doc.events), n)
        finally:
            win.video.stop_backend()
            win.strip.shutdown()
            win.close()
