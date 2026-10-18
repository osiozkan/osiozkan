import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
import pytest

pytest.importorskip("PySide6.QtWidgets")


def test_main_window_basic(tmp_path):
    from PySide6.QtWidgets import QApplication
    from osisub.gui.app import MainWindow
    from osisub.core.ass import Document, Event
    app = QApplication.instance() or QApplication([])
    w = MainWindow()
    d = Document.new()
    for i in range(5):
        d.events.append(Event(start=i * 2000, end=i * 2000 + 1500, text=f"x{i}"))
    w.set_document(d)
    w.select_rows([1])
    w.e_text.setPlainText("yeni")
    w.commit("t")
    assert w.doc.events[1].text == "yeni"
    w.undo()
    assert w.doc.events[1].text == "x1"
    w.redo()
    w.delete_lines()
    assert len(w.doc.events) == 4
    p = tmp_path / "o.ass"
    w.doc.filename = str(p)
    w.save()
    assert p.exists()
    w.history.mark_saved()
    w.close()
