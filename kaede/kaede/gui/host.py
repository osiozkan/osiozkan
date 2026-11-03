from __future__ import annotations
from PySide6.QtGui import QFont, QFontMetricsF
from PySide6.QtWidgets import QFileDialog

from ..lua.engine import Host
from .luadialog import run_lua_dialog


class GuiHost(Host):
    def __init__(self, win):
        super().__init__()
        self.win = win

    def log(self, level, msg):
        self.logs.append(msg)

    def progress(self, kind, value=None):
        if kind == "task": self.win.statusBar().showMessage(str(value))
        elif kind == "cancelled": return False
        return None

    def text_extents(self, style, text):
        f = QFont(str(style.get("fontname", "Arial")))
        f.setPixelSize(max(1, int(float(style.get("fontsize", 48)))))
        f.setBold(bool(style.get("bold"))); f.setItalic(bool(style.get("italic")))
        f.setUnderline(bool(style.get("underline"))); f.setStrikeOut(bool(style.get("strikeout")))
        fm = QFontMetricsF(f)
        sx = float(style.get("scale_x", 100)) / 100; sy = float(style.get("scale_y", 100)) / 100
        w = (fm.horizontalAdvance(text) + float(style.get("spacing", 0)) * len(text)) * sx
        return w, (fm.ascent() + fm.descent()) * sy, fm.descent() * sy, fm.leading() * sy

    def dialog(self, dialog, buttons, ids):
        return run_lua_dialog(self.win, dialog, buttons, ids, title=self.win.current_script_title)

    def file_dialog(self, mode, *args):
        title = args[0] if args else ""
        if mode == "open":
            p, _ = QFileDialog.getOpenFileName(self.win, str(title), str(args[1]) if len(args) > 1 else "")
        else:
            p, _ = QFileDialog.getSaveFileName(self.win, str(title), str(args[1]) if len(args) > 1 else "")
        return p or None

    def undo_point(self, desc):
        pass
