"""Subtitle Edit araç pencereleri — ekransız, gerçek ana pencereyle (tek geri al, önizleme, ayar hatırlama)."""

import os
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ.setdefault("APPDATA", tempfile.mkdtemp())

from PySide6.QtWidgets import QApplication  # noqa: E402

from osisub.config import Config  # noqa: E402
from osisub.core.assfile import Event  # noqa: E402
from osisub.core.textutil import cpl_lengths  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SAMPLE = os.path.join(HERE, "fixtures", "sample.ass")
app = QApplication.instance() or QApplication([])


def make_window():
    from osisub.ui import theme
    from osisub.ui.context import Context
    from osisub.ui.mainwindow import MainWindow
    cfg = Config(os.path.join(tempfile.mkdtemp(), "config.json"))
    cfg["load_aegisub_scripts"] = False
    theme.apply(app, "dark")
    ctx = Context(cfg)
    win = MainWindow(ctx, headless=True)
    win.show()
    win.open_path(SAMPLE, force=True)
    app.processEvents()
    return win, ctx


class TestTools(unittest.TestCase):
    def setUp(self):
        self.win, self.ctx = make_window()

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def test_break_and_unbreak(self):
        w, ctx = self.win, self.ctx
        ctx.set_selection({0, 1, 4}, 1)
        self.assertEqual(w.break_lines(), 2)                      # 1. ve 4. satırlar (0 zaten sınır içinde)
        self.assertEqual(len(cpl_lengths(ctx.doc.events[1].text)), 2)
        self.assertTrue(all(n <= 42 for n in cpl_lengths(ctx.doc.events[4].text)))
        self.assertEqual(ctx.undo.undo_desc(), "Dengeli kır")
        ctx.set_selection({1}, 1)
        self.assertEqual(w.unbreak_lines(), 1)
        self.assertNotIn("\\N", ctx.doc.events[1].text)
        ctx.do_undo()
        ctx.do_undo()
        self.assertFalse(ctx.undo.modified)

    def test_case_dialog_remembers(self):
        dlg = self.win.show_tool("case")
        for b in dlg.mode_group.buttons():
            if b.property("mode") == "upper":
                b.setChecked(True)
        out = dlg.preview()
        self.assertEqual(dlg.tree.topLevelItemCount(), len(out.changes))
        self.assertIn("MERHABA DÜNYA, NASILSIN?", self.ctx.doc.events[0].text.upper())
        dlg.apply()
        self.assertEqual(self.ctx.doc.events[0].text, "{\\an8}MERHABA DÜNYA, NASILSIN?")
        self.assertEqual(self.ctx.doc.events[3].text, "{\\pos(960,100)\\fs80}TABELA")   # etiket korunur
        self.assertEqual(self.ctx.config["tools"]["case"]["mode"], "upper")
        self.ctx.do_undo()
        self.assertFalse(self.ctx.undo.modified)
        dlg2 = self.win.show_tool("case")
        self.assertEqual(dlg2.mode(), "upper")

    def test_hi_dialog_deletes_in_one_step(self):
        ctx = self.ctx
        n = len(ctx.doc.events)
        ctx.insert_events(0, [Event(text="[kapı çarpar]"), Event(text="ALİ: (güler) Selam!")], "ek")
        dlg = self.win.show_tool("hi")
        out = dlg.preview()
        self.assertIn("1 satır silinecek", out.summary)
        dlg.apply()
        self.assertEqual(len(ctx.doc.events), n + 1)
        self.assertEqual(ctx.doc.events[0].text, "Selam!")
        ctx.do_undo()
        self.assertEqual(len(ctx.doc.events), n + 2)

    def test_merge_split_sort(self):
        ctx = self.ctx
        long_ = ("Bunu sana daha önce de söylemiştim ama beni hiç dinlemedin. "
                 "Şimdi hepimiz bu karmaşanın ortasında kaldık ve çıkış yolu bulmamız lazım.")
        ctx.update_events({5: {"text": long_}}, "uzun")
        dlg = self.win.show_tool("split")
        out = dlg.preview()
        self.assertIn("1 satır", out.summary)
        n = len(ctx.doc.events)
        dlg.apply()
        self.assertEqual(len(ctx.doc.events), n + 1)
        # birleştir: iki kısa ardışık satır
        ctx.insert_events(len(ctx.doc.events), [Event(start=20000, end=20800, text="Evet."),
                                                Event(start=20900, end=21500, text="Tamam.")], "kısa")
        dlg = self.win.show_tool("merge")
        dlg.scope.setCurrentIndex(0)
        out = dlg.preview()
        self.assertTrue(out.events is not None)
        n = len(ctx.doc.events)
        dlg.apply()
        self.assertEqual(len(ctx.doc.events), n - 1)
        self.assertEqual(ctx.doc.events[-1].text, "Evet. Tamam.")
        # sırala: stile göre (seçili tüm satırlar)
        ctx.set_selection(range(len(ctx.doc.events)), 0)
        dlg = self.win.show_tool("sort")
        dlg.keys[0].setCurrentIndex(dlg.keys[0].findData("style"))
        dlg.rev.setChecked(True)
        dlg.apply()
        self.assertEqual(ctx.doc.events[0].style, "Sign")

    def test_timing_tools(self):
        ctx = self.ctx
        evs = ctx.doc.events
        # süreler: CPS'e göre, yalnız uzat; tabela stili varsayılan olarak dışarıda
        dlg = self.win.show_tool("durations")
        sign_item = [dlg.styles.item(i) for i in range(dlg.styles.count())
                     if dlg.styles.item(i).data(0x0100) == "Sign"][0]
        from PySide6.QtCore import Qt
        self.assertEqual(sign_item.checkState(), Qt.Unchecked)
        dlg.cps.setValue(5)
        out = dlg.preview()
        self.assertTrue(out.changes)
        self.assertNotIn(3, out.changes)
        dlg.apply()
        self.assertGreater(evs[0].end, 3500)
        self.assertLessEqual(evs[0].end, 4000 - 84 + 10)             # sonraki satıra taşmaz
        ctx.do_undo()
        # köprüle
        dlg = self.win.show_tool("bridge")
        dlg.max_gap.setValue(600)
        dlg.min_gap.setValue(40)
        dlg.apply()
        self.assertEqual(ctx.doc.events[0].end, 3960)
        ctx.do_undo()
        # kare hızı 25 -> 23.976
        dlg = self.win.show_tool("fps")
        dlg.scope.setCurrentIndex(0)
        dlg.src.setCurrentText("25")
        dlg.dst.setCurrentText("23.976")
        dlg.apply()
        self.assertEqual(ctx.doc.events[4].start, 10430)              # 25 fps (PAL, hızlı) → 23.976: 10000 · 25/23.976
        ctx.do_undo()
        # noktalı senkron: iki nokta
        ctx.set_selection({0, 5}, 0)
        dlg = self.win.show_tool("pointsync")
        self.assertEqual(len(dlg.edits), 2)
        dlg.edits[0].setValue(2000)
        dlg.edits[1].setValue(23000)                                   # 1000→2000, 12000→23000 (eğim 1,909…)
        dlg.apply()
        self.assertEqual(ctx.doc.events[0].start, 2000)
        self.assertEqual(ctx.doc.events[5].start, 23000)
        self.assertEqual(ctx.undo.undo_desc(), "Noktalı senkron")

    def test_stats(self):
        dlg = self.win.show_stats()
        self.assertEqual(dlg.data["dialogue"], 5)
        self.assertEqual(dlg.data["comments"], 1)


if __name__ == "__main__":
    unittest.main()
