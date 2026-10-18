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


def test_video_and_pick(tmp_path):
    import shutil, subprocess
    if not shutil.which("ffmpeg"):
        pytest.skip("ffmpeg missing")
    from PySide6.QtWidgets import QApplication
    from osisub.gui.app import MainWindow
    from osisub.core.ass import Document, Event
    import time
    vid = tmp_path / "v.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=d=2:s=320x180:r=24",
                    "-f", "lavfi", "-i", "sine=d=2", "-g", "12", "-shortest", str(vid)], check=True)
    app = QApplication.instance() or QApplication([])
    w = MainWindow()
    d = Document.new()
    d.events.append(Event(start=0, end=1500, text="abc"))
    w.set_document(d)
    w.select_rows([0])
    w.load_video(str(vid))
    for _ in range(40):
        app.processEvents(); time.sleep(0.05)
    assert w.video and w.video.width == 320
    assert w.wave.peaks is not None and len(w.keyframes) >= 2
    w._preview_clicked(100.4, 200.6)
    assert w.doc.events[0].text.startswith("{\\pos(100,201)}")
    w._preview_clicked(5, 5)
    assert w.doc.events[0].text.count("\\pos") == 1
    w.history.mark_saved()
    w.close()
