import os
import pytest
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
pytest.importorskip("PySide6")
from PySide6.QtWidgets import QApplication
from yorusub.gui.app import MainWindow

D = os.path.join(os.path.dirname(__file__), "data")


def test_gui_flow(tmp_path):
    app = QApplication.instance() or QApplication([])
    w = MainWindow(os.path.join(D, "sample.ass"))
    assert w.model.rowCount() == 4
    w.table.selectRow(0)
    w.e_text.setPlainText("Yeni metin")
    assert w.model.doc.events[0].text == "Yeni metin"
    assert w.model.undo() and w.model.doc.events[0].text.startswith("Merhaba")
    w.load_lua(os.path.join(D, "hello.lua"))
    mi = next(m for m in w.runtime.macros if m.name == "Büyük harf")
    w.table.selectRow(0)
    w.run_macro(mi)
    assert w.model.doc.events[0].text.startswith("MERHABA")
    w.run_qc()
    assert w.qc_list.count() > 0
    out = tmp_path / "o.srt"
    from yorusub.core.formats import save_any
    save_any(w.model.doc, str(out))
    assert out.read_text(encoding="utf-8").startswith("1\n")
    w.model.dirty = False
    w.close()
