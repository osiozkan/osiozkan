"""Konuşmaya göre senkron (bsync): kurallar, yapay konuşmalı ses ile yer gerçeği, pencere."""

import os
import tempfile
import unittest
import wave

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["OSISUB_NO_AUDIO"] = "1"

import numpy as np  # noqa: E402

from osisub.core import sync as S  # noqa: E402
from osisub.core.assfile import Event  # noqa: E402

RATE = 22050


def make_scenes(seed=3, n_scenes=6, per=10, start=5000, gap=8000):
    rng = np.random.default_rng(seed)
    t = start
    scenes = []
    for _ in range(n_scenes):
        lines = []
        for _ in range(per):
            d = int(rng.integers(800, 3000))
            lines.append((t, t + d))
            t += d + int(rng.integers(150, 900))
        scenes.append(lines)
        t += gap
    return scenes, t + 5000


def subs_for(scenes, shifts, lead=100, tail=200):
    evs = []
    for sc, sh in zip(scenes, shifts):
        for a, b in sc:
            evs.append(Event(start=a - lead - sh, end=b + tail - sh, text="satır"))
    return evs


class TestSyncCore(unittest.TestCase):
    def test_piecewise_shifts_recovered(self):
        scenes, total = make_scenes()
        act = S.activity_from_segments([ln for sc in scenes for ln in sc], total)
        shifts = [1200, 1200, -800, -800, 2500, 300]
        evs = subs_for(scenes, shifts)
        # ekran yazısı (çapa değil, parçasıyla kayar) ve tabela (\pos)
        a0 = scenes[2][3][0]
        evs.append(Event(start=a0 + 800, end=a0 + 3800, text="{\\an8}Dükkan\\N \\N \\N"))
        evs.append(Event(start=a0 + 900, end=a0 + 3000, text="{\\pos(100,100)}TABELA"))
        evs.sort(key=lambda e: e.start)
        segs, _ = S.plan(evs, act, total)
        self.assertEqual(len(segs), 6)
        for sg, sh in zip(segs, shifts):
            self.assertLess(abs(sg.shift - (sh - 50)), 120, (sg.shift, sh))   # −50: giriş/çıkış payı ortalanır
            self.assertGreater(sg.confidence, 0.02)
        sg2 = segs[2]
        self.assertEqual(len(sg2.rows), 12)
        self.assertEqual(len(sg2.anchors), 10)                  # ekran yazısı ve tabela çapa değil
        ch, warns = S.changes(evs, segs, total)
        self.assertEqual(len(ch), len(evs))                     # hepsi parçasıyla kayar
        self.assertEqual(warns, [])

    def test_scene_gap_rule_and_order(self):
        # boşluk < 4 sn olan iki grup tek parça: farklı kayma alamaz
        scenes, total = make_scenes(n_scenes=2, gap=2500)
        act = S.activity_from_segments([ln for sc in scenes for ln in sc], total)
        segs, _ = S.plan(subs_for(scenes, [300, -300]), act, total)
        self.assertEqual(len(segs), 1)
        # parçalar sıra değiştiremez / üst üste binemez
        scenes, total = make_scenes(n_scenes=3, gap=4200)
        act = S.activity_from_segments([ln for sc in scenes for ln in sc], total)
        evs = subs_for(scenes, [0, 0, 0])
        segs, _ = S.plan(evs, act, total)
        for a, b in zip(segs, segs[1:]):
            self.assertLessEqual(a.end + a.shift, b.start + b.shift)

    def test_oped_and_song_suppression(self):
        scenes, total = make_scenes(n_scenes=3, start=95000)
        segs_true = [ln for sc in scenes for ln in sc]
        # 0–90 sn: kesintisiz şarkı (OP); altyazıda OP stilinde karaoke satırları
        act = S.activity_from_segments(segs_true + [(1000, 90000)], total)
        evs = subs_for(scenes, [700, 700, 700])
        # OP satırları şarkının yalnız ilk ~30 sn'sini kapsıyor; geri kalan 60 sn "uzun kesintisiz ses" kuralıyla
        evs += [Event(start=2000 + 5000 * i, end=6000 + 5000 * i, style="OP", text="la la") for i in range(6)]
        evs.sort(key=lambda e: e.start)
        segs, removed = S.plan(evs, act, total)
        self.assertTrue(any("OP/ED" in r[2] for r in removed))
        self.assertTrue(any("şarkı" in r[2] for r in removed))
        op_seg = [s for s in segs if any(evs[r].style == "OP" for r in s.rows)][0]
        self.assertEqual(op_seg.anchors, [])
        for s in segs:
            if s.anchors:
                self.assertLess(abs(s.shift - 650), 120)
        # suppress tek başına
        a, rem = S.suppress(np.ones(10000, np.float32), [], 45000)
        self.assertEqual(a.sum(), 0)
        self.assertEqual(len(rem), 1)

    def test_clamp_to_audio(self):
        evs = [Event(start=100, end=900, text="a"), Event(start=9800, end=10500, text="b")]
        sg = [S.Segment([0], 100, 900, [0], shift=-500), S.Segment([1], 9800, 10500, [1], shift=300)]
        ch, warns = S.changes(evs, sg, 10000)
        self.assertEqual(ch[0], {"start": 0, "end": 400})
        self.assertLessEqual(ch[1]["end"], 10000)
        self.assertTrue(warns)


def write_speech_wav(path, scenes, total_ms, song=None, seed=1):
    """Konuşma bandında gürültü patlamaları (satır zamanlarında) + isteğe bağlı şarkı."""
    rng = np.random.default_rng(seed)
    n = int(total_ms * RATE / 1000)
    x = rng.normal(0, 0.002, n)
    spec_n = 4096
    for sc in scenes:
        for a, b in sc:
            i, j = int(a * RATE / 1000), int(b * RATE / 1000)
            seg = rng.normal(0, 1, j - i)
            f = np.fft.rfft(seg)
            fr = np.fft.rfftfreq(len(seg), 1 / RATE)
            f[(fr < 300) | (fr > 3000)] = 0
            seg = np.fft.irfft(f, len(seg))
            seg *= 0.25 / (seg.std() + 1e-9) * (0.6 + 0.4 * np.abs(np.sin(np.arange(len(seg)) / RATE * 2 * np.pi * 4)))
            x[i:j] += seg
    if song:
        a, b = song
        i, j = int(a * RATE / 1000), int(b * RATE / 1000)
        t = np.arange(j - i) / RATE
        x[i:j] += 0.2 * np.sin(2 * np.pi * 440 * t) + 0.15 * np.sin(2 * np.pi * 660 * t) + rng.normal(0, 0.05, j - i)
    del spec_n
    pcm = np.clip(x * 32767, -32768, 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    return pcm


class TestSyncAudio(unittest.TestCase):
    def test_vad_ensemble_end_to_end(self):
        from osisub.media.vad import speech_activity
        scenes, total = make_scenes(n_scenes=4, start=70000)
        d = tempfile.mkdtemp()
        p = os.path.join(d, "s.wav")
        pcm = write_speech_wav(p, scenes, total, song=(2000, 62000))
        act = speech_activity(pcm, RATE)
        truth = S.activity_from_segments([ln for sc in scenes for ln in sc], total)
        m = min(len(act), len(truth))
        # şarkı VAD'ce "ses" görünür (bastırma sync.plan'da); şarkı dışındaki uyum
        agree = float((act[6500:m] == truth[6500:m]).mean())
        self.assertGreater(agree, 0.97)
        self.assertGreater(float(act[300:6100].mean()), 0.9)
        shifts = [900, -600, -600, 1800]
        evs = subs_for(scenes, shifts)
        segs, removed = S.plan(evs, act, total)
        self.assertTrue(any("şarkı" in r[2] for r in removed))
        self.assertEqual(len(segs), 4)
        for sg, sh in zip(segs, shifts):
            self.assertLess(abs(sg.shift - (sh - 50)), 150, (sg.shift, sh))


class TestSyncDialog(unittest.TestCase):
    def test_dialog_flow(self):
        from PySide6.QtWidgets import QApplication

        from osisub.config import Config
        from osisub.core.assfile import AssDocument
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        from tests.test_video_ui import pump
        QApplication.instance() or QApplication([])
        scenes, total = make_scenes(n_scenes=3, start=4000)
        d = tempfile.mkdtemp()
        wav = os.path.join(d, "ses.wav")
        write_speech_wav(wav, scenes, total)
        shifts = [1000, -700, 400]
        doc = AssDocument.new()
        doc.ensure_section("events").items[:] = subs_for(scenes, shifts)
        ass = os.path.join(d, "a.ass")
        doc.save(ass)
        cfg = Config(os.path.join(d, "c.json"))
        cfg["load_aegisub_scripts"] = False
        cfg["auto_load_media"] = False
        ctx = Context(cfg)
        win = MainWindow(ctx, headless=True)
        try:
            win.open_path(ass, force=True)
            win.media.open_audio(wav)
            self.assertTrue(pump(until=lambda: win.media.audio is not None, timeout=30))
            dlg = win.show_sync()
            self.assertTrue(dlg.calculate())
            self.assertTrue(dlg.wait(60))
            self.assertIsNone(dlg.error)
            self.assertEqual(dlg.table.rowCount(), 3)
            self.assertIn("3 parça", dlg.info.text())
            before = [e.start for e in ctx.doc.events]
            ch = dlg.apply()
            self.assertEqual(len(ch), 30)
            after = [e.start for e in ctx.doc.events]
            for i, sh in enumerate(shifts):
                delta = after[i * 10] - before[i * 10]
                self.assertLess(abs(delta - (sh - 50)), 150)
            # yeniden hesaplayınca kalan kayma ~0
            dlg.calculate()
            dlg.wait(60)
            self.assertTrue(all(abs(s.shift) <= 120 for s in dlg.segs))
            # tek geri al
            ctx.do_undo()
            self.assertEqual([e.start for e in ctx.doc.events], before)
            # belge değişince eski hesap uygulanmaz
            ctx.update_events({0: {"text": "x"}}, "t")
            self.assertIsNone(dlg.apply())
        finally:
            win.close()


if __name__ == "__main__":
    unittest.main()
