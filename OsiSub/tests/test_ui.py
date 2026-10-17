"""Arayüz testleri — ekransız (QT_QPA_PLATFORM=offscreen), gerçek bileşenlerle."""

import os
import tempfile
import unittest

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["APPDATA"] = tempfile.mkdtemp()          # Windows'ta kullanıcı klasörü
os.environ["HOME"] = os.environ.get("HOME", tempfile.mkdtemp())

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from osisub.config import Config  # noqa: E402
from osisub.core.assfile import AssDocument  # noqa: E402

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
    win.resize(1400, 900)
    win.show()
    win.open_path(SAMPLE, force=True)
    app.processEvents()
    return win, ctx


class TestEditing(unittest.TestCase):
    def setUp(self):
        self.win, self.ctx = make_window()
        self.eb = self.win.editbox

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def test_editbox_loads_active(self):
        self.ctx.set_active(1, select_only=True)
        app.processEvents()
        self.assertEqual(self.eb.text.toPlainText(), self.ctx.doc.events[1].text)
        self.assertEqual(self.eb.cpl_long.text(), "49")
        self.assertEqual(self.eb.start.text(), "0:00:04.00")

    def test_typing_shift_enter_and_undo(self):
        self.ctx.set_active(0, select_only=True)
        app.processEvents()
        self.eb.focus_text()
        QTest.keyClicks(self.eb.text, " ek")
        QTest.keyClick(self.eb.text, Qt.Key_Return, Qt.ShiftModifier)
        QTest.keyClicks(self.eb.text, "alt")
        app.processEvents()
        self.assertEqual(self.ctx.doc.events[0].text, "{\\an8}Merhaba dünya, nasılsın? ek\\Nalt")
        self.assertEqual(self.eb.cpl_parts.text(), "27+3")
        # tüm yazım tek geri al adımı
        self.ctx.do_undo()
        app.processEvents()
        self.assertEqual(self.ctx.doc.events[0].text, "{\\an8}Merhaba dünya, nasılsın?")
        self.assertEqual(self.eb.text.toPlainText(), "{\\an8}Merhaba dünya, nasılsın?")

    def test_enter_moves_next_and_appends_at_end(self):
        n = len(self.ctx.doc.events)
        self.ctx.set_active(n - 1, select_only=True)
        app.processEvents()
        self.eb.focus_text()
        QTest.keyClick(self.eb.text, Qt.Key_Return)
        app.processEvents()
        self.assertEqual(len(self.ctx.doc.events), n + 1)
        self.assertEqual(self.ctx.active, n)
        self.assertEqual(self.ctx.doc.events[n].start, self.ctx.doc.events[n - 1].end)

    def test_fields_apply_to_selection(self):
        self.ctx.set_selection({0, 1, 4}, 0)
        app.processEvents()
        i = self.eb.style_box.findText("Sign")
        self.eb.style_box.setCurrentIndex(i)
        self.eb.style_box.activated.emit(i)
        app.processEvents()
        self.assertEqual([self.ctx.doc.events[r].style for r in (0, 1, 4)], ["Sign"] * 3)
        self.assertEqual(self.ctx.doc.events[3].style, "Sign")
        self.assertEqual(self.ctx.doc.events[5].style, "Default")
        # metin yalnız etkin satıra
        self.eb.focus_text()
        QTest.keyClicks(self.eb.text, "!")
        app.processEvents()
        self.assertTrue(self.ctx.doc.events[0].text.endswith("!"))
        self.assertFalse(self.ctx.doc.events[1].text.endswith("!"))

    def test_time_edit_overwrite(self):
        self.ctx.set_active(0, select_only=True)
        app.processEvents()
        te = self.eb.start
        te.setFocus()
        te.setCursorPosition(5)          # 0:00:|01.00
        QTest.keyClicks(te, "0250")
        QTest.keyClick(te, Qt.Key_Return)
        app.processEvents()
        self.assertEqual(self.ctx.doc.events[0].start, 2500)

    def test_grid_selection_sync(self):
        g = self.win.grid
        g.selectRow(2)
        app.processEvents()
        self.assertEqual(self.ctx.active, 2)
        from osisub.ui.grid import C_TEXT
        rect = g.visualRect(g.model().index(4, C_TEXT))
        QTest.mouseClick(g.viewport(), Qt.LeftButton, Qt.ControlModifier, rect.center())
        app.processEvents()
        self.assertEqual(self.ctx.selection, {2, 4})
        self.assertEqual(self.ctx.active, 4)

    def test_line_ops(self):
        w, ctx = self.win, self.ctx
        n = len(ctx.doc.events)
        ctx.set_selection({4}, 4)
        w.split_at_n()
        self.assertEqual(len(ctx.doc.events), n + 1)
        self.assertEqual(ctx.doc.events[4].text, "Kısa satır")
        ctx.set_selection({4, 5}, 4)
        w.join_lines("\\N")
        self.assertEqual(len(ctx.doc.events), n)
        ctx.set_selection({0, 1}, 0)
        w.duplicate_lines()
        self.assertEqual(len(ctx.doc.events), n + 2)
        w.delete_lines()
        self.assertEqual(len(ctx.doc.events), n)
        ctx.set_selection({0}, 0)
        w.toggle_comment()
        self.assertTrue(ctx.doc.events[0].comment)
        w.copy_lines()
        w.paste_lines()
        self.assertEqual(len(ctx.doc.events), n + 1)
        self.assertEqual(ctx.doc.events[1].text, ctx.doc.events[0].text)
        for _ in range(10):
            ctx.do_undo()
        self.assertEqual(ctx.doc.to_bytes(), open(SAMPLE, "rb").read())
        self.assertFalse(ctx.undo.modified)

    def test_save_preserves_bytes(self):
        p = os.path.join(tempfile.mkdtemp(), "kayit.ass")
        self.ctx.save_document(p)
        self.assertEqual(open(p, "rb").read(), open(SAMPLE, "rb").read())


class TestAutomationGui(unittest.TestCase):
    def setUp(self):
        self.win, self.ctx = make_window()
        self.win.scripts.add(os.path.join(HERE, "lua", "test_macros.lua"))
        self.win.load_scripts()
        self.win.scripts.add(os.path.join(HERE, "lua", "test_macros.lua"))
        self.win.rebuild_automation_menu()

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def m(self, name):
        return next(m for m in self.win.scripts.macros() if m.name == name)

    def test_menu_has_submenus(self):
        titles = [a.text() for a in self.win.automation_menu.actions()]
        self.assertIn("Test", titles)
        self.assertIn("Strip tags", titles)

    def test_run_macro_commits_one_undo(self):
        self.ctx.set_selection({0, 3}, 0)
        r = self.win.run_macro(self.m("Strip tags"))
        self.assertEqual(r.status, "ok")
        self.assertEqual(self.ctx.doc.events[0].text, "Merhaba dünya, nasılsın?")
        self.assertEqual(self.ctx.undo.undo_desc(), "Strip tags")
        self.ctx.do_undo()
        self.assertEqual(self.ctx.doc.to_bytes(), open(SAMPLE, "rb").read())

    def test_dialog_and_selection(self):
        self.win.gui_host.dialog_answers.append(("Tamam", {"txt": "x", "up": False, "n": 1}))
        self.ctx.set_selection({1}, 1)
        self.win.run_macro(self.m("Test/dialog"))
        self.assertEqual(self.ctx.doc.events[1].text, "x#1")

    def test_error_keeps_doc_and_logs(self):
        r = self.win.run_macro(self.m("Test/hata"))
        self.assertEqual(r.status, "error")
        self.assertIn("attempt to index", self.win.last_macro_log)
        self.assertFalse(self.ctx.undo.modified)

    def test_validate_disables(self):
        self.ctx.set_selection({0, 1}, 0)
        self.win._validate_automation_menu()
        act = next(a for a, m in self.win._macro_actions if m.name == "Test/kare")
        self.assertFalse(act.isEnabled())
        self.ctx.set_selection({0}, 0)
        self.win._validate_automation_menu()
        self.assertTrue(act.isEnabled())

    def test_lua_dialog_widget(self):
        from osisub.ui.automation_ui import LuaDialog
        dlg = LuaDialog([
            {"class": "edit", "name": "e", "text": "ğ", "x": 0, "y": 0},
            {"class": "dropdown", "name": "d", "items": ["a", "b"], "value": "b", "x": 0, "y": 1},
            {"class": "color", "name": "c", "value": "&H0000FF&", "x": 0, "y": 2},
            {"class": "coloralpha", "name": "ca", "value": "&H80FF0000", "x": 0, "y": 3},
            {"class": "floatedit", "name": "f", "value": 1.5, "x": 0, "y": 4},
            {"class": "checkbox", "name": "k", "value": True, "label": "k", "x": 0, "y": 5},
        ], ["Uygula", "Vazgeç"], {"cancel": "Vazgeç"}, "t")
        v = dlg.values()
        self.assertEqual(v, {"e": "ğ", "d": "b", "c": "#FF0000", "ca": "#0000FF80", "f": 1.5, "k": True})
        dlg._press("Vazgeç", 1)
        self.assertIs(dlg.pressed, False)
        dlg._press("Uygula", 0)
        self.assertEqual(dlg.pressed, "Uygula")


class TestDialogs(unittest.TestCase):
    def setUp(self):
        self.win, self.ctx = make_window()

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def test_style_manager_rename(self):
        from osisub.ui.dialogs import StyleManager
        sm = StyleManager(self.ctx, self.win)
        sm.styles[1] = sm.styles[1].replace(name="Tabela")
        sm.renames = {"Sign": "Tabela"}
        sm._apply()
        self.assertEqual([s.name for s in self.ctx.doc.styles], ["Default", "Tabela"])
        self.assertEqual(self.ctx.doc.events[3].style, "Tabela")
        self.ctx.do_undo()
        self.assertEqual(self.ctx.doc.events[3].style, "Sign")

    def test_find_replace_skips_tags(self):
        from osisub.ui.dialogs import FindReplace
        fr = FindReplace(self.ctx, self.win)
        fr.find.setText("an")
        fr.repl.setText("AN")
        fr.replace_all()
        self.assertEqual(self.ctx.doc.events[0].text, "{\\an8}Merhaba dünya, nasılsın?")
        fr.find.setText("a")
        fr.repl.setText("@")
        fr.sel_only.setChecked(True)
        self.ctx.set_selection({0}, 0)
        fr.replace_all()
        self.assertEqual(self.ctx.doc.events[0].text, "{\\an8}Merh@b@ düny@, n@sılsın?")

    def test_shift_times(self):
        from osisub.ui.dialogs import ShiftTimes
        st = ShiftTimes(self.ctx, self.win)
        st.scope.setCurrentIndex(0)
        st.time.setValue(1500)
        st.back.setChecked(True)
        st._ok()
        self.assertEqual(self.ctx.doc.events[0].start, 0)
        self.assertEqual(self.ctx.doc.events[1].start, 2500)

    def test_cpl_panel(self):
        self.win.cpl_panel.refresh()
        self.assertEqual(self.win.cpl_panel.summary.text(), "1 hata · 1 uyarı")

    def test_script_props(self):
        from osisub.ui.dialogs import ScriptProps
        sp = ScriptProps(self.ctx, self.win)
        sp.resx.setValue(1280)
        sp.resy.setValue(720)
        sp._ok()
        self.assertEqual(self.ctx.doc.play_res(), (1280, 720))
        self.assertIn("PlayResX: 1280", self.ctx.doc.to_text())

    def test_screenshot(self):
        out = os.environ.get("OSISUB_SHOT")
        if out:
            self.win.cpl_panel.refresh()
            self.ctx.set_active(4, select_only=True)
            app.processEvents()
            self.win.grab().save(out)


if __name__ == "__main__":
    unittest.main()


class TestTranslationMode(unittest.TestCase):
    """Referans altyazı, Referans sütunu, aktarma ve elle düzeltme."""

    def setUp(self):
        self.win, self.ctx = make_window()
        d = tempfile.mkdtemp()
        self.srt = os.path.join(d, "tr.srt")
        with open(self.srt, "w", encoding="utf-8") as f:
            f.write("1\n00:00:01,050 --> 00:00:03,400\nMerhaba TR\n\n"
                    "2\n00:00:04,000 --> 00:00:06,000\nUzun TR satırı\n\n"
                    "3\n00:00:20,000 --> 00:00:21,000\nYetim TR\n")

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def test_reference_column_and_editbox(self):
        from osisub.ui.grid import C_REF
        self.win.open_reference(self.srt)
        app.processEvents()
        self.assertFalse(self.win.grid.isColumnHidden(C_REF))
        m = self.win.grid.model()
        self.assertEqual(m.data(m.index(0, C_REF)), "Merhaba TR")
        self.assertEqual(m.data(m.index(3, C_REF)), "")
        self.ctx.set_active(1, select_only=True)
        app.processEvents()
        self.assertTrue(self.win.editbox.ref_row.isVisibleTo(self.win))
        self.assertEqual(self.win.editbox.ref_label.text(), "Uzun TR satırı")
        self.win.editbox.copy_reference()
        self.assertEqual(self.ctx.doc.events[1].text, "Uzun TR satırı")
        self.ctx.do_undo()
        self.win.reference.close()
        self.assertTrue(self.win.grid.isColumnHidden(C_REF))

    def test_transfer_single_undo_and_report(self):
        self.win.open_reference(self.srt)
        dlg = self.win.transfer_from_reference()
        plan = dlg.preview()
        self.assertEqual(plan.stats["orphans"], 1)
        dlg.apply()
        self.win.ref_panel.show_issues(plan)
        evs = self.ctx.doc.events
        self.assertEqual(evs[0].text, "{\\an8}Merhaba TR")
        self.assertTrue(any(e.text == "Yetim TR" for e in evs))
        self.assertGreater(self.win.ref_panel.report.topLevelItemCount(), 0)
        self.ctx.do_undo()
        self.assertEqual(self.ctx.doc.to_bytes(), open(SAMPLE, "rb").read())

    def test_manual_ref_actions(self):
        self.win.open_reference(self.srt)
        panel = self.win.ref_panel
        panel.list.setCurrentItem(panel.list.topLevelItem(2))     # Yetim TR
        self.ctx.set_active(3, select_only=True)
        panel._apply_ref("append")
        self.assertTrue(self.ctx.doc.events[3].text.endswith("\\NYetim TR"))
        n = len(self.ctx.doc.events)
        panel._apply_ref("new")
        self.assertEqual(len(self.ctx.doc.events), n + 1)
        new = [e for e in self.ctx.doc.events if e.text == "Yetim TR"][0]
        self.assertEqual((new.start, new.end), (20000, 21000))

    def test_open_srt_as_document(self):
        self.win.open_path(self.srt, force=True)
        self.assertIsNone(self.ctx.doc.path)
        self.assertIn("içe aktarıldı", self.ctx.title)
        self.assertEqual(len(self.ctx.doc.events), 3)

    def test_timing_dialog_without_video(self):
        from osisub.ui.timing_dialog import TimingDialog
        dlg = TimingDialog(self.ctx, self.win)
        self.assertFalse(dlg.g_kf.isEnabled())
        dlg.scope.setCurrentIndex(1)
        dlg.g_lead.setChecked(True)
        dlg.lead_in.setValue(200)
        r = dlg.preview()
        self.assertGreater(len(r.changes), 3)
        dlg.apply()
        self.assertEqual(self.ctx.doc.events[0].start, 800)
        self.assertEqual(self.ctx.undo.undo_desc(), "Zamanlama son işlemcisi")
        self.ctx.do_undo()
        self.assertEqual(self.ctx.doc.events[0].start, 1000)


class TestQcAndSpellUi(unittest.TestCase):
    def setUp(self):
        self.win, self.ctx = make_window()

    def tearDown(self):
        self.win.close()
        self.win.deleteLater()
        app.processEvents()

    def test_qc_run_and_fix_single_undo(self):
        ctx = self.ctx
        ctx.update_events({0: {"text": "{\\an8}merhaba  dünya ,nasılsın"}}, "hazırlık")
        issues = self.win.run_qc()
        rules = {(i.row, i.rule) for i in issues}
        self.assertIn((0, "double_space"), rules)
        self.assertIn((0, "space_before_punct"), rules)
        n_before = ctx.undo.index
        self.win.qc_panel.fix_checked()
        self.assertEqual(ctx.doc.events[0].text, "{\\an8}Merhaba dünya, nasılsın")
        self.assertEqual(ctx.undo.index, n_before + 1)
        ctx.do_undo()
        self.assertEqual(ctx.doc.events[0].text, "{\\an8}merhaba  dünya ,nasılsın")

    def test_qc_jump(self):
        self.win.run_qc()
        tree = self.win.qc_panel.tree
        self.assertGreater(tree.topLevelItemCount(), 0)
        it = tree.topLevelItem(tree.topLevelItemCount() - 1)
        self.win.qc_panel._go(it)
        self.assertEqual(self.ctx.active, int(it.text(0)) - 1)

    def test_spell_highlight_and_menu(self):
        from osisub.core.spell import find_dictionary
        if not find_dictionary("tr_TR"):
            self.skipTest("sözlük yok")
        sp = self.win.spell
        self.assertTrue(sp.start(blocking=True))
        self.assertTrue(sp.ready)
        self.ctx.update_events({0: {"text": "{\\i1}Bugün{\\i0} okula gidiyrum"}}, "hazırlık")
        self.ctx.set_active(0, select_only=True)
        app.processEvents()
        te = self.win.editbox.text
        hit = te.word_at(len("{\\i1}Bugün{\\i0} okula gid"))
        self.assertEqual(hit[0], "gidiyrum")
        self.assertFalse(sp.check("gidiyrum"))
        # vurgulayıcı yanlış kelimenin altını çiziyor mu
        from PySide6.QtGui import QTextCharFormat
        blk = te.document().firstBlock()
        fmts = blk.layout().formats()
        under = [f for f in fmts if f.format.underlineStyle() == QTextCharFormat.SpellCheckUnderline]
        self.assertTrue(any(f.start == hit[1] and f.length == hit[2] - hit[1] for f in under))
        # öneriyle değiştir
        self.assertIn("gidiyorum", sp.suggest("gidiyrum"))
        te._replace(hit[1], hit[2], "gidiyorum")
        app.processEvents()
        self.assertEqual(self.ctx.doc.events[0].text, "{\\i1}Bugün{\\i0} okula gidiyorum")
