import os
import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
pytest.importorskip("PySide6.QtWidgets")
from PySide6.QtWidgets import QApplication

SAMPLE = os.path.join(os.path.dirname(__file__), "..", "samples", "demo.ass")


@pytest.fixture(scope="module")
def app():
    return QApplication.instance() or QApplication([])


def test_window_flow(app, tmp_path):
    from kaede.gui.main_window import MainWindow
    w = MainWindow(SAMPLE)
    w.show()
    assert len(w.doc.events) == 4
    w.goto_row(1)
    assert w.edit_text.toPlainText().startswith("{\\i1}")
    # edit text -> doc + undo
    w.edit_text.setPlainText("Yeni metin")
    assert w.doc.events[1].text == "Yeni metin"
    w.undo()
    assert w.doc.events[1].text.startswith("{\\i1}")
    # insert/delete
    w.insert_line(True); assert len(w.doc.events) == 5
    w.delete_lines(); assert len(w.doc.events) == 4
    # shift via tool
    w.grid.selectAll()
    w.mutate("x", lambda d, s: __import__("kaede.core.tools", fromlist=["x"]).shift_times(d, 1000, s))
    assert w.doc.events[0].start == 2000
    w.undo(); assert w.doc.events[0].start == 1000
    # overlay frame rendered (libass)
    w._refresh_overlay()
    px = w.grab()
    px.save(str(tmp_path / "shot.png"))
    out = tmp_path / "o.ass"; assert w._write(str(out)); assert out.exists()
    w.dirty = False
    w.close()


def test_macro_from_gui(app, tmp_path):
    from kaede.gui.main_window import MainWindow
    p = tmp_path / "m.lua"
    p.write_text('script_name="t"\naegisub.register_macro("Test/Upper","",function(subs,sel)\n'
                 'for _,i in ipairs(sel) do local l=subs[i]; l.text=unicode.to_upper(l.text); subs[i]=l end return sel end)')
    w = MainWindow(SAMPLE)
    w.engine.load_script(str(p))
    mac = w.engine.all_macros()[-1]
    w.select_rows([0])
    w.run_macro(mac)
    assert w.doc.events[0].text.startswith("MERHABA")
    w.undo(); assert w.doc.events[0].text.startswith("Merhaba")
    w.dirty = False; w.close()


def test_visual_tools(app):
    from kaede.gui.main_window import MainWindow
    w = MainWindow(SAMPLE); w.goto_row(0); w.show()
    w.visual.set_mode("pos")
    vw, vh = w.video.vw, w.video.vh
    w.visual.press(vw / 2, vh / 2, 1); w.visual.release(vw / 2 + 100, vh / 2 + 50)
    t = w.doc.events[0].text
    assert t.startswith("{\\pos(") and "Merhaba" in t
    x0 = w.visual.to_script(vw / 2, vh / 2)
    assert w.undo() is None
    assert "\\pos" not in w.doc.events[0].text
    w.visual.set_mode("clip")
    w.visual.press(10, 10, 1); w.visual.release(200, 100)
    assert "\\clip(" in w.doc.events[0].text
    w.visual.set_mode("rotz"); w.visual.press(vw / 2 + 50, vh / 2, 1); w.visual.release(vw / 2, vh / 2 + 50)
    assert "\\frz" in w.doc.events[0].text
    w.dirty = False; w.close()
