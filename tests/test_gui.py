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


def test_visual_tools(win):
    from PySide6.QtCore import QPointF
    win.select_row(0)
    win.video.set_tool("pos")
    win.video.tagEdit.emit("pos", "(100,200)", False)
    win.video.tagEdit.emit("clip", "(1,2,3,4)", False)
    t = win.project.events[0].text
    assert t.startswith("{\\clip(1,2,3,4)\\pos(100,200)") or "\\pos(100,200)" in t
    win.project.undo(); win.project.undo()
    assert win.project.events[0].text.startswith("{\\k20}")


def test_all_dialogs_construct(win):
    from subforge.gui import dialogs as D, misc_dialogs as M, manager
    win.select_row(0)
    for mk in (lambda: M.FontsDialog(win), lambda: M.AttachmentsDialog(win), lambda: M.StatsDialog(win.project, win),
               lambda: M.ShortcutsDialog(win), lambda: M.ExportFiltersDialog(win), lambda: D.StylesDialog(win.project, win),
               lambda: D.PropertiesDialog(win.project, win), lambda: manager.ManagerDialog(win),
               lambda: D.ShiftDialog(win, 24, 1), lambda: D.TimingDialog(win, 1, False), lambda: D.SyncDialog(win, 0, 1000),
               lambda: D.FpsDialog(win), lambda: D.QCDialog(win.project, win), lambda: D.FindReplaceDialog(win)):
        d = mk(); d.close()


def test_shortcut_persistence(win):
    win.set_shortcut("a_dup", "Ctrl+Alt+D")
    assert win.a_dup.shortcut().toString() == "Ctrl+Alt+D"
    assert win.settings.value("shortcut/a_dup") == "Ctrl+Alt+D"
    win.set_shortcut("a_dup", win.default_shortcuts["a_dup"])
    win.settings.remove("shortcut/a_dup")


def test_properties_resample(win):
    from subforge.gui.dialogs import PropertiesDialog
    d = PropertiesDialog(win.project, win)
    d.rx.setValue(1920); d.ry.setValue(1080)
    d.apply()
    assert win.project.doc.play_res == (1920, 1080)
    win.project.undo()
    assert win.project.doc.play_res == (1280, 720)


def _macro(tmp_path, code):
    from subforge.automation import AutomationScript
    p = tmp_path / "x.lua"
    p.write_text(code)
    s = AutomationScript(str(p))
    assert s.error is None, s.error
    return s.macros[0]


def test_failed_macro_restores_document(win, tmp_path):
    before = win.project.doc.to_string()
    m = _macro(tmp_path, '''aegisub.register_macro("bad","",function(subs, sel)
      local l = subs[6]; l.text = "CHANGED"; subs[6] = l
      subs.append({class="dialogue", section="[Events]", start_time=0,end_time=1,style="Default",text="x"})
      error("kaboom") end)''')
    import subforge.gui.lua_ui as L
    L.LogWindow.exec = lambda self: 0     # don't block on the modal log window
    win.set_selection([0]); win.select_row(0)
    win.run_macro(m)
    assert win.project.doc.to_string() == before
    assert not win.project.can_undo()


def test_macro_new_selection_and_single_undo(win, tmp_path):
    import subforge.gui.lua_ui as L
    L.LogWindow.exec = lambda self: 0
    m = _macro(tmp_path, '''aegisub.register_macro("dup","",function(subs, sel)
      local out = {}
      for i = #sel, 1, -1 do
        local l = subs[sel[i]]; l.text = l.text .. "!"; subs.insert(sel[i] + 1, l)
      end
      return {sel[1] + 1} end)''')
    n = win.model.rowCount()
    win.set_selection([0]); win.select_row(0)
    win.run_macro(m)
    assert win.model.rowCount() == n + 1 and win.selected_rows() == [1]
    win.project.undo()
    assert win.model.rowCount() == n


def test_style_rename_updates_events(win):
    from subforge.gui.dialogs import StylesDialog
    d = StylesDialog(win.project, win)
    d.items[0][1].name = "Ana"
    assert d.apply()
    assert all(e.style == "Ana" for e in win.project.events)
    assert win.project.doc.styles[0].name == "Ana"
    win.project.undo()
    assert win.project.doc.styles[0].name == "Default"


def test_waveform_drag_commit_is_one_undo(win):
    win.wave._drag = None
    win._boundary_drag(0, "end", 4000)
    win._boundary_drag(0, "end", 4200)
    win._boundary_commit(0, "end", 4300)
    assert win.project.events[0].end_time == 4300
    win.project.undo()
    assert win.project.events[0].end_time == 3500


def test_save_roundtrip_and_srt_default_name(win, tmp_path):
    out = tmp_path / "o.ass"
    win.project.save(str(out))
    from subforge.core import parse_ass
    assert [e.text for e in parse_ass(out.read_text(encoding="utf-8-sig")).events] == [e.text for e in win.project.events]
    srt = tmp_path / "a.srt"
    srt.write_text("1\n00:00:01,000 --> 00:00:02,000\nmerhaba\n", encoding="utf-8")
    win.project.open(str(srt))
    assert win.project.path is None and win.project.title == "a.srt" and win.project.dirty
