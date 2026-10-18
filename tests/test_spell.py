import pytest

pytest.importorskip("spylls")
from osisub.core import spell

if not spell.find_dictionary("tr_TR"):
    pytest.skip("tr_TR dictionary missing", allow_module_level=True)


def test_turkish():
    sc = spell.SpellChecker("tr_TR")
    bad = [w for _, _, w in sc.errors(r"{\i1}Merhaba{\i0} dünyaa\Nkitaplar ABD 42")]
    assert bad == ["dünyaa"]
    assert "dünya" in sc.suggest("dünyaa")


def test_spell_dialog():
    import os
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication
    from osisub.gui.dialogs import SpellDialog
    from osisub.core.ass import Document, Event
    app = QApplication.instance() or QApplication([])
    d = Document.new()
    d.events = [Event(text="Merhaba dünyaa"), Event(text="kitaplar")]
    sc = spell.SpellChecker("tr_TR")
    sc.user_path = "/tmp/_osisub_user_test.txt"
    changes = []
    dlg = SpellDialog(sc, d, 0, lambda: changes.append(1), lambda r: None)
    assert dlg.cur and dlg.cur[3] == "dünyaa"
    dlg.edit.setText("dünya")
    dlg.replace()
    assert d.events[0].text == "Merhaba dünya" and dlg.cur is None and changes
