import os
os.environ["QT_QPA_PLATFORM"] = "offscreen"
from PySide6.QtWidgets import QApplication
from kitsune.gui.app import MainWindow
from kitsune.core.ass import Script
from tests.test_lua import SAMPLE

def test_gui_flow(tmp_path):
    app = QApplication.instance() or QApplication([])
    w = MainWindow(); w.set_script(Script.parse(SAMPLE))
    assert w.model.rowCount() == 2
    w.table.selectRow(0); w.textEdit.setPlainText("değişti"); assert w.events()[0].text == "değişti"
    w.do_undo(); assert w.events()[0].text == "Merhaba, dünya"
    w.insert_line(1); assert w.model.rowCount() == 3
    w.table.selectRow(0); w.delete_lines(); assert w.model.rowCount() == 2
    names = [m.name for m in w.host.macros()]; assert "Kitsune/Etiketleri temizle" in names
    w.table.selectRow(1); m = next(m for m in w.host.macros() if m.name.endswith("temizle")); w.run_macro(m)
    assert w.events()[1].text == "karaa" or "{" not in w.events()[1].text
    w.run_qc()
