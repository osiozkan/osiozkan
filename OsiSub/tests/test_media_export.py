"""Videoyla dışa aktarma: kısmi yakma (zaman hizası piksel düzeyinde), MKV'ye gömme + yazı tipi eki,
gömülü izi geri açma, ekleri çıkarma ve pencere akışları."""

import os
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"
os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())
os.environ.setdefault("LOCALAPPDATA", tempfile.mkdtemp())
os.environ.setdefault("APPDATA", tempfile.mkdtemp())

import numpy as np  # noqa: E402

from osisub.core.assfile import AssDocument, Event as E, Style  # noqa: E402
from osisub.media import export as X  # noqa: E402
from tests.media_fixtures import have_ffmpeg, path  # noqa: E402

DEJAVU = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"


def frame_at(video, t, w=640, h=360):
    from osisub.media.probe import run, tool
    r = run([tool("ffmpeg"), "-v", "error", "-ss", f"{t:.3f}", "-i", video, "-frames:v", "1", "-f", "rawvideo",
             "-pix_fmt", "gray", "-s", f"{w}x{h}", "-"])
    return np.frombuffer(r.stdout, np.uint8).reshape(h, w).astype(np.int16)


def make_doc():
    doc = AssDocument.new()
    doc.set_info("PlayResX", "640")
    doc.set_info("PlayResY", "360")
    doc.ensure_section("styles").items = [Style(name="Default", fontname="DejaVu Sans", fontsize=40, outline=3,
                                                margin_v=20)]
    doc.ensure_section("events").items = [E(start=500, end=3000, text="Merhaba dünya şğı"),
                                          E(start=4000, end=6000, text="{\\an8}İkinci satır")]
    return doc


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestCore(unittest.TestCase):
    def test_partial_burn_timing(self):
        d = tempfile.mkdtemp()
        doc = make_doc()
        ass = os.path.join(d, "a.ass")
        doc.save(ass)
        out = os.path.join(d, "h.mp4")
        cmd = X.burn_cmd(path("cfr.mkv"), ass, out, X.BurnOptions(preset="ultrafast", crf=12, start_ms=2000,
                                                                   end_ms=5000), os.path.dirname(DEJAVU))
        job = X.Job(cmd, 3.0)
        job.start()
        self.assertTrue(job.wait(120))
        self.assertEqual(job.returncode, 0, job.error)
        self.assertEqual(job.progress, 1.0)
        self.assertAlmostEqual(X.duration_s(out), 3.0, delta=0.15)
        src = path("cfr.mkv")
        # çıktı 0,5 sn = özgün 2,5 sn: alttaki satır görünür; 1,5 sn = özgün 3,5 sn: altyazı yok;
        # 2,5 sn = özgün 4,5 sn: \an8 satırı üstte
        diff_on = np.abs(frame_at(out, 0.5) - frame_at(src, 2.5))
        diff_off = np.abs(frame_at(out, 1.5) - frame_at(src, 3.5))
        diff_top = np.abs(frame_at(out, 2.5) - frame_at(src, 4.5))
        self.assertGreater((diff_on[250:, :] > 60).sum(), 800)
        self.assertLess((diff_off > 60).sum(), 200)
        self.assertGreater((diff_top[:110, :] > 60).sum(), 600)
        self.assertLess((diff_top[250:, :] > 60).sum(), 200)

    def test_mux_extract_attachments(self):
        if not os.path.exists(DEJAVU):
            self.skipTest("DejaVu yok")
        d = tempfile.mkdtemp()
        doc = make_doc()
        ass = os.path.join(d, "a.ass")
        doc.save(ass)
        mkv = os.path.join(d, "s.mkv")
        rc, err = X.run_sync(X.mux_cmd_full(path("cfr.mkv"), ass, mkv, X.MuxOptions(), [DEJAVU]))
        self.assertEqual(rc, 0, err)
        tr = X.subtitle_tracks(mkv)
        self.assertEqual(len(tr), 1)
        self.assertEqual((tr[0].codec, tr[0].language, tr[0].title, tr[0].default), ("ass", "tur", "Türkçe", True))
        self.assertEqual(X.attachment_count(mkv), 1)
        # yeniden göm: eski iz + ek korunur, yeni iz varsayılan, yeni ekin mimetype'ı doğru sırada
        mkv2 = os.path.join(d, "s2.mkv")
        rc, err = X.run_sync(X.mux_cmd_full(mkv, ass, mkv2, X.MuxOptions(keep_subs=True, title="İki"), [DEJAVU]))
        self.assertEqual(rc, 0, err)
        tr = X.subtitle_tracks(mkv2)
        self.assertEqual([(t.title, t.default) for t in tr], [("Türkçe", False), ("İki", True)])
        self.assertEqual(X.attachment_count(mkv2), 2)
        x = os.path.join(d, "x.ass")
        rc, err = X.run_sync(X.extract_cmd(mkv2, tr[1], x))
        self.assertEqual(rc, 0, err)
        back = AssDocument.load(x)
        self.assertEqual([(e.start, e.end, e.text) for e in back.events],
                         [(e.start, e.end, e.text) for e in doc.events])
        files = X.dump_attachments(mkv, os.path.join(d, "ek"))
        self.assertEqual([os.path.basename(f) for f in files], ["DejaVuSans.ttf"])


@unittest.skipUnless(have_ffmpeg(), "ffmpeg yok")
class TestDialogs(unittest.TestCase):
    def test_flows(self):
        from tests.test_video_ui import make, pump
        win, ctx, _ass = make(False)
        try:
            d = tempfile.mkdtemp()
            ctx.doc.ensure_section("styles").items[0] = ctx.doc.styles[0].replace(fontname="DejaVu Sans")
            ctx.commit_external("yazı tipi")
            # MKV'ye göm (kaydedilmemiş değişiklik de gider)
            dlg = win.show_mux()
            dlg.out.setText(os.path.join(d, "out.mkv"))
            self.assertTrue(dlg.start())
            self.assertTrue(dlg.wait(120), dlg.info.text())
            self.assertIn("Bitti", dlg.info.text())
            self.assertGreaterEqual(X.attachment_count(os.path.join(d, "out.mkv")), 1)
            # gömülü izi belge olarak aç
            ctx.undo.mark_saved()
            ex = win.show_extract_subs(os.path.join(d, "out.mkv"))
            self.assertEqual(len(ex.tracks), 1)
            n = sum(1 for e in ctx.doc.events)
            self.assertTrue(ex.open_track("doc"))
            self.assertIsNone(ctx.doc.path)
            self.assertTrue(ctx.doc.source_path.endswith("out.tur.ass"))
            self.assertEqual(len(ctx.doc.events), n)
            self.assertNotIn(os.path.join(tempfile.gettempdir(), "osisub-extract"), " ".join(ctx.config["recent_files"]))
            # referans olarak
            ex = win.show_extract_subs(os.path.join(d, "out.mkv"))
            self.assertTrue(ex.open_track("ref"))
            self.assertTrue(win.reference.loaded)
            fonts = ex.extract_fonts(os.path.join(d, "ekler"))
            self.assertTrue(fonts)
            # yak: seçili satır aralığı
            ctx.set_selection({0}, 0)
            b = win.show_burn()
            b.out.setText(os.path.join(d, "h.mp4"))
            b.preset.setCurrentText("ultrafast")
            b.range.setCurrentIndex(1)
            self.assertTrue(b.start())
            self.assertTrue(b.wait(120), b.info.text())
            self.assertGreater(X.duration_s(os.path.join(d, "h.mp4")), 1.0)
            self.assertEqual(ctx.config["burn"]["preset"], "ultrafast")
            pump(0.1)
        finally:
            win.close()


if __name__ == "__main__":
    unittest.main()
