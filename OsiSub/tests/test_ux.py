"""Kullanıcı deneyimi: komut paleti (Türkçe duyarsız bulanık arama), Ayarlar (canlı uygulama, kısayol
çakışması, CPL sınırı), çalışma düzenleri, ızgara sütun gizleme, Enter ayarı, durum çubuğu."""

import os
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ.setdefault("APPDATA", tempfile.mkdtemp())
os.environ.setdefault("XDG_CACHE_HOME", tempfile.mkdtemp())

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtGui import QKeySequence  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from osisub.config import Config  # noqa: E402
from osisub.ui.palette import fold, score  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SAMPLE = os.path.join(HERE, "fixtures", "sample.ass")
app = QApplication.instance() or QApplication([])


class TestFuzzy(unittest.TestCase):
    def test_fold_and_score(self):
        self.assertEqual(fold("Dengeli KIR, Yazı Tipi, ÇĞŞÖÜİ"), "dengeli kir, yazi tipi, cgsoui")
        self.assertIsNotNone(score("kirma", fold("Kırmaları kaldır")))
        self.assertIsNone(score("xyz", fold("Kırmaları kaldır")))
        a = score("yazi tipi", fold("Yazı tipi toplayıcı…"))
        b = score("yazi tipi", fold("Kısa satırları birleştir…"))
        self.assertTrue(a is not None and (b is None or a > b))
        # sözcük başı eşleşmesi, ortadaki eşleşmeden önce
        self.assertGreater(score("ses", fold("Ses aç…")), score("ses", fold("Konuşmadan boş satırlar")) or -1)


class TestUx(unittest.TestCase):
    def setUp(self):
        from osisub.core.textutil import set_cpl_limit
        from osisub.ui import theme
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        set_cpl_limit(42)
        self.cfg = Config(os.path.join(tempfile.mkdtemp(), "config.json"))
        self.cfg["load_aegisub_scripts"] = False
        theme.apply(app, "dark")
        self.ctx = Context(self.cfg)
        self.win = MainWindow(self.ctx, headless=True)
        self.win.resize(1400, 900)
        self.win.show()
        self.win.open_path(SAMPLE, force=True)
        app.processEvents()

    def tearDown(self):
        from osisub.core.textutil import set_cpl_limit
        set_cpl_limit(42)
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def test_palette(self):
        p = self.win.show_palette()
        QTest.keyClicks(p.edit, "yazi tipi top")
        self.assertGreater(p.list.count(), 0)
        kind, cmd = p.list.item(0).data(Qt.UserRole)
        self.assertEqual(cmd.key, "fonts")
        p.edit.setText("dengeli kir")
        self.assertEqual(p.list.item(0).data(Qt.UserRole)[1].key, "break_lines")
        from osisub.ui.palette import SHORTCUT_ROLE
        self.assertEqual(p.list.item(0).data(SHORTCUT_ROLE), "Ctrl+Shift+N")
        # bölme komutu
        p.edit.setText("bolmeyi goster karsilastirma")
        k, c = p.list.item(0).data(Qt.UserRole)
        self.assertEqual(c.key, "dock:Karşılaştırma")
        c.run()
        self.assertTrue(self.win.dock_compare.isVisible())
        # :satır
        p.edit.setText(":5")
        self.assertEqual(p.run_item(p.list.item(0)), "line")
        self.assertEqual(self.ctx.active, 4)
        # çalıştır + son kullanılanlar üstte
        p = self.win.show_palette()
        p.edit.setText("istatistik")
        it = p.list.item(0)
        self.assertEqual(it.data(Qt.UserRole)[1].key, "tool_stats")
        p.run_item(it)
        self.assertEqual(self.ctx.config["palette_recent"][0], "tool_stats")
        p2 = self.win.show_palette()
        self.assertEqual(p2.list.item(0).data(Qt.UserRole)[1].key, "tool_stats")

    def test_prefs_apply_and_conflicts(self):
        w, ctx = self.win, self.ctx
        dlg = w.show_prefs()
        dlg.cpl.setValue(30)
        dlg.grid_font.setValue(14)
        dlg.eb_font.setValue(18)
        dlg.enter_next.setChecked(False)
        dlg.theme.setCurrentIndex(1)
        # kısayol: çakışma reddedilir
        dlg.sc_edits["tool_stats"].setKeySequence(QKeySequence("Ctrl+Shift+N"))
        self.assertIn("Ctrl+Shift+N", dlg.conflicts())
        self.assertFalse(dlg.apply())
        dlg.sc_edits["tool_stats"].setKeySequence(QKeySequence("Ctrl+Alt+İ"))
        dlg.sc_edits["tool_stats"].setKeySequence(QKeySequence("F9"))
        dlg.sc_edits["sort_time"].setKeySequence(QKeySequence())
        self.assertTrue(dlg.apply())
        self.assertEqual(ctx.config["shortcuts"]["tool_stats"], "F9")
        self.assertEqual(w.acts["tool_stats"].shortcut().toString(), "F9")
        self.assertEqual(ctx.config["theme"], "light")
        self.assertEqual(w.editbox.text.font().pointSize(), 18)
        self.assertEqual(w.grid.font().pointSize(), 14)
        from osisub.core.textutil import cpl_info
        self.assertEqual(cpl_info("x" * 35)["status"], "warn")            # sınır 30
        # Enter artık sonraki satıra geçmiyor
        ctx.set_active(0, select_only=True)
        app.processEvents()
        w.editbox.focus_text()
        QTest.keyClick(w.editbox.text, Qt.Key_Return)
        self.assertEqual(ctx.active, 0)
        # yeni pencere: kısayol değişiklikleri kalıcı
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        w2 = MainWindow(Context(ctx.config), headless=True)
        try:
            self.assertEqual(w2.acts["tool_stats"].shortcut().toString(), "F9")
            self.assertTrue(w2.acts["sort_time"].shortcut().isEmpty())
            self.assertEqual(w2.default_shortcuts["break_lines"].toString(), "Ctrl+Shift+N")
        finally:
            w2.close()
        # dış araç düzenleyici
        dlg = w.show_prefs()
        dlg._add_tool_row({"name": "assmotion", "program": "python", "args": "a.py {ass}", "reload": False})
        self.assertTrue(dlg.apply())
        self.assertEqual(ctx.config["external_tools"], [{"name": "assmotion", "program": "python",
                                                          "args": "a.py {ass}", "reload": False}])

    def test_layouts_and_grid_columns(self):
        w = self.win
        self.assertTrue(w.apply_layout("Denetim"))
        self.assertTrue(w.dock_qc.isVisible())
        self.assertFalse(w.dock_audio.isVisible())
        self.assertTrue(w.apply_layout("Çeviri"))
        self.assertTrue(w.dock_ref.isVisible())
        self.assertFalse(w.dock_qc.isVisible())
        w.save_layout("Benim")
        w.apply_layout("Denetim")
        self.assertTrue(w.apply_layout("Benim"))
        self.assertTrue(w.dock_ref.isVisible())
        w.layouts_menu.aboutToShow.emit()
        names = [a.text() for a in w.layouts_menu.actions()]
        self.assertIn("Benim", names)
        w.delete_layout("Benim")
        self.assertNotIn("Benim", w.ctx.config["layouts"])
        # ızgara sütunu gizle -> kalıcı
        from osisub.ui.grid import C_ACTOR, C_EFFECT
        w.grid.set_column_visible(C_EFFECT, False)
        self.assertEqual(w.ctx.config["grid_hidden_cols"], ["Efekt"])
        from osisub.ui.context import Context
        from osisub.ui.mainwindow import MainWindow
        w2 = MainWindow(Context(w.ctx.config), headless=True)
        try:
            self.assertTrue(w2.grid.isColumnHidden(C_EFFECT))
            self.assertFalse(w2.grid.isColumnHidden(C_ACTOR))
        finally:
            w2.close()

    def test_line_status(self):
        self.ctx.set_selection({1, 2, 3}, 2)
        self.assertEqual(self.win.line_lbl.text(), "Satır 3/6 · 3 seçili")


if __name__ == "__main__":
    unittest.main()
