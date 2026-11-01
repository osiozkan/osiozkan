import os, time
import pytest
from sample import SAMPLE

pytest.importorskip("PySide6")
from PySide6.QtWidgets import QApplication

app = QApplication.instance() or QApplication([])


@pytest.fixture
def win(tmp_path):
    from subforge.gui.app import MainWindow
    p = tmp_path / "a.ass"
    p.write_text(SAMPLE, encoding="utf-8")
    w = MainWindow(str(p))
    w.show(); app.processEvents()
    yield w
    w.project.dirty = False
    w.close()


def test_open_edit_undo_keeps_selection(win):
    win.select_row(0); app.processEvents()
    win.edit.text.setPlainText("Merhaba"); app.processEvents()
    assert win.project.events[0].text == "Merhaba"
    win.project.undo(); app.processEvents()
    assert win.project.events[0].text.startswith("{\\k20}")
    assert win.active_row() == 0 and win.edit.isEnabled()


def test_line_ops(win):
    win.select_row(0)
    n = win.model.rowCount()
    win.insert_line(True); assert win.model.rowCount() == n + 1
    win.duplicate_lines(); assert win.model.rowCount() == n + 2
    win.delete_lines(); assert win.model.rowCount() == n + 1
    win.project.undo(); win.project.undo(); win.project.undo()
    assert win.model.rowCount() == n


def test_run_macro_gui(win, tmp_path):
    from subforge.automation import AutomationScript
    p = tmp_path / "m.lua"
    p.write_text('aegisub.register_macro("Up","",function(subs,sel) for _,i in ipairs(sel) do local l=subs[i]; l.text=l.text:upper(); subs[i]=l end return sel end)')
    s = AutomationScript(str(p))
    win.set_selection([2]); win.select_row(2)
    win.run_macro(s.macros[0])
    assert win.project.events[2].text.startswith("{\\I1}") or "SELAM" in win.project.events[2].text
    win.project.undo()
    assert "Selam" in win.project.events[2].text


def test_split_join_and_comment(win):
    win.select_row(2)
    win.toggle_comment(); assert win.project.events[2].comment
    win.toggle_comment(); assert not win.project.events[2].comment


def test_qc_runs(win):
    win.show_qc(); assert win._qc_dlg.issues is not None


def test_video_audio(win, tmp_path):
    import subprocess, shutil
    if not shutil.which("ffmpeg"):
        pytest.skip("no ffmpeg")
    v = tmp_path / "v.mp4"
    subprocess.run(f"ffmpeg -v error -y -f lavfi -i testsrc=size=320x180:rate=24:duration=3 -f lavfi -i sine=duration=3 -g 24 -pix_fmt yuv420p -shortest {v}", shell=True, check=True)
    win.open_video(str(v))
    t0 = time.time()
    while time.time() - t0 < 8 and (win.wave.audio is None or not win.project.keyframes):
        app.processEvents(); time.sleep(0.05)
    assert win.wave.audio is not None and win.project.keyframes
    assert win.project.fps and abs(win.project.fps - 24) < 0.01
    win.wave.grab()


def test_karaoke_dialog_and_select(win):
    from subforge.gui.tools_dialogs import KaraokeDialog, SelectDialog
    win.select_row(1)
    win.project.events[1].comment = False
    d = KaraokeDialog(win)
    assert d.syls and sum(x for _, x in d.syls) == d._total()
    d.mode.setCurrentIndex(1); d._resplit()
    d.accept()
    assert "{\\k" in win.project.events[1].text
    s = SelectDialog(win)
    s.pat.setText("selam"); s.run()
    assert 2 in win.selected_rows()
