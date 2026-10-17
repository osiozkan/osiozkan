"""Hız: denetim (QC) komşu dizini eski O(n²) taramayla BİREBİR aynı sonucu verir ve doğrusal ölçeklenir;
CPL bilgisi önbellekli ve sınır değişince doğru."""

import random
import time
import unittest

from osisub.core.assfile import Event as E
from osisub.core.qc import RULES, QcOptions, run_qc
from osisub.core.textutil import cpl_info, set_cpl_limit


def make(n, seed=1):
    rnd = random.Random(seed)
    evs = []
    t = 0
    for _ in range(n):
        d = rnd.randint(500, 4000)
        evs.append(E(start=t, end=t + d, style=rnd.choice(["Default", "Default", "Sign"]), layer=rnd.choice([0, 0, 1]),
                     comment=rnd.random() < 0.05,
                     text=rnd.choice(["merhaba  dünya ..", "Bu satır kırk üç karakterden daha uzun, değil mi?",
                                      "kısa", "evet evet tamam", "nasılsın? iyiyim."])))
        t += d + rnd.choice([0, 30, 200, 1000]) - (600 if rnd.random() < 0.15 else 0)
    # aynı başlangıçlı satırlar (sıra bağı) da olsun
    evs.insert(10, evs[10].replace(text="aynı an"))
    return evs


def slow_qc(events, rows, enabled, o):
    o._nav = None
    out = []
    for i in rows:
        if events[i].comment:
            continue
        for r in RULES:
            if r.id in enabled:
                out.extend(r.check(events, i, o))
    return out


class TestQcIndex(unittest.TestCase):
    def test_identical_to_slow_path(self):
        evs = make(700)
        rows = list(range(len(evs)))
        enabled = {r.id for r in RULES}
        for same_style in (True, False):
            fast = run_qc(evs, rows, enabled, QcOptions(same_style_only=same_style))
            slow = slow_qc(evs, rows, enabled, QcOptions(same_style_only=same_style))
            self.assertEqual(len(fast), len(slow))
            self.assertEqual([(x.row, x.rule, x.detail, x.fix) for x in fast],
                             [(x.row, x.rule, x.detail, x.fix) for x in slow])
        sub = rows[100:200]
        self.assertEqual([(x.row, x.rule) for x in run_qc(evs, sub, enabled, QcOptions())],
                         [(x.row, x.rule) for x in slow_qc(evs, sub, enabled, QcOptions())])

    def test_scales_linearly(self):
        enabled = {r.id for r in RULES if r.default}
        evs = make(5000)
        t = time.time()
        run_qc(evs, list(range(len(evs))), enabled, QcOptions())
        self.assertLess(time.time() - t, 5.0)          # eskiden ~20 sn


class TestCplCache(unittest.TestCase):
    def tearDown(self):
        set_cpl_limit(42)

    def test_limit_change_not_stale(self):
        t = "x" * 40
        self.assertEqual(cpl_info(t)["status"], "ok")
        set_cpl_limit(30)
        self.assertEqual(cpl_info(t)["status"], "warn")
        set_cpl_limit(42)
        self.assertEqual(cpl_info(t)["status"], "ok")
        self.assertIs(cpl_info(t), cpl_info(t))         # önbellekten



class TestCplPanelIncremental(unittest.TestCase):
    def test_incremental_equals_full(self):
        import os
        import tempfile
        os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        from PySide6.QtWidgets import QApplication

        from osisub.config import Config
        from osisub.ui.context import Context
        from osisub.ui.panels import CplPanel
        app = QApplication.instance() or QApplication([])
        ctx = Context(Config(os.path.join(tempfile.mkdtemp(), "c.json")))
        long_ = "Bu satır kırk üç karakterden daha uzun, değil mi?"
        ctx.replace_all_events([E(start=i * 1000, end=i * 1000 + 900, text=long_ if i % 3 == 0 else "kısa")
                                for i in range(60)], "t")
        p = CplPanel(ctx)
        p.refresh()

        def snapshot():
            return [tuple(p.tree.topLevelItem(k).text(c) for c in range(4)) for k in range(p.tree.topLevelItemCount())]
        self.assertEqual(p.tree.topLevelItemCount(), 20)
        # tek tek düzenlemeler: artımlı güncelleme tam yenilemeyle aynı sonucu vermeli
        ctx.update_events({0: {"text": "artık kısa"}}, "a")
        ctx.update_events({1: {"text": long_}}, "b")
        ctx.update_events({4: {"text": long_ + "\\N" + long_}}, "c")      # hata
        p._apply_pending()
        inc, summ = snapshot(), p.summary.text()
        p.refresh()
        self.assertEqual(inc, snapshot())
        self.assertEqual(summ, p.summary.text())
        self.assertIn("1 hata", summ)
        p.only_err.setChecked(True)
        ctx.update_events({7: {"text": long_ + "\\N" + long_}}, "d")
        p._apply_pending()
        inc = snapshot()
        p.refresh()
        self.assertEqual(inc, snapshot())
        self.assertEqual(len(inc), 2)
        app.processEvents()


if __name__ == "__main__":
    unittest.main()
