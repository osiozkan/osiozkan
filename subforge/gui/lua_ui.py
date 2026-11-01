"""GUI side of the Auto4 API: dynamic dialogs, progress, log, clipboard, file dialogs."""
from __future__ import annotations

import re

from PySide6.QtCore import Qt, QElapsedTimer
from PySide6.QtGui import QGuiApplication
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QDoubleSpinBox, QFileDialog, QGridLayout,
                               QHBoxLayout, QLabel, QLineEdit, QMessageBox, QPlainTextEdit, QProgressDialog,
                               QPushButton, QSpinBox, QVBoxLayout, QWidget, QSlider)

from ..automation import HeadlessUI
from ..core.ass import norm_color
from .widgets import ColorButton


def _override_color(ass_style: str) -> str:
    c = norm_color(ass_style)
    return "&H" + c[4:] + "&"


def _style_color(c: str) -> str:
    return norm_color(c)


def _alpha(c: str) -> str:
    n = norm_color(c)
    return "&H" + n[2:4] + "&"


class DynamicDialog(QDialog):
    def __init__(self, controls, buttons, ids, title="Automation", parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.controls = controls
        self.widgets: dict[str, tuple[str, QWidget]] = {}
        self.pressed: str | None = None
        self.ids = ids or {}
        grid = QGridLayout()
        for c in controls:
            cls = c.get("class")
            x, y = int(c.get("x", 0)), int(c.get("y", 0))
            w, h = int(c.get("width", 1) or 1), int(c.get("height", 1) or 1)
            name = c.get("name")
            wid: QWidget | None = None
            if cls == "label":
                wid = QLabel(str(c.get("label", "")))
                wid.setWordWrap(True)
            elif cls == "edit":
                wid = QLineEdit(str(c.get("text", "")))
            elif cls == "textbox":
                wid = QPlainTextEdit(str(c.get("text", "")))
                wid.setMinimumHeight(24 * max(2, h))
            elif cls == "checkbox":
                wid = QCheckBox(str(c.get("label", "")))
                wid.setChecked(bool(c.get("value", False)))
            elif cls == "dropdown":
                wid = QComboBox()
                items = [str(i) for i in (c.get("items") or [])]
                wid.addItems(items)
                if c.get("value") in items:
                    wid.setCurrentText(str(c.get("value")))
            elif cls == "intedit":
                wid = QSpinBox()
                wid.setRange(int(c.get("min", -2**31)), int(c.get("max", 2**31 - 1)))
                wid.setValue(int(c.get("value", 0) or 0))
            elif cls == "floatedit":
                wid = QDoubleSpinBox()
                wid.setDecimals(4)
                wid.setRange(float(c.get("min", -1e12)), float(c.get("max", 1e12)))
                if c.get("step"):
                    wid.setSingleStep(float(c["step"]))
                wid.setValue(float(c.get("value", 0) or 0))
            elif cls in ("color", "coloralpha"):
                v = c.get("value", "&H00FFFFFF&")
                if cls == "color":
                    v = "&H00" + norm_color(v)[4:]
                wid = ColorButton(v)
            elif cls == "alpha":
                wid = QSpinBox(); wid.setRange(0, 255)
                try:
                    wid.setValue(int(norm_color(c.get("value", "&H00&").strip("&H") and c.get("value"))[2:4], 16))
                except Exception:
                    wid.setValue(0)
            elif cls == "slider":
                wid = QSlider(Qt.Orientation.Horizontal)
                wid.setRange(int(c.get("min", 0)), int(c.get("max", 100)))
                wid.setValue(int(c.get("value", 0) or 0))
            else:
                wid = QLabel(f"[{cls}]")
            if c.get("hint"):
                wid.setToolTip(str(c["hint"]))
            grid.addWidget(wid, y, x, h, w)
            if name and cls not in ("label",):
                self.widgets[name] = (cls, wid)
        lay = QVBoxLayout(self)
        lay.addLayout(grid)
        row = QHBoxLayout()
        row.addStretch(1)
        labels = buttons or ["OK", "Cancel"]
        for lab in labels:
            b = QPushButton(lab)
            b.clicked.connect(lambda _=False, l=lab: self._press(l))
            if lab == (self.ids.get("ok") or labels[0]):
                b.setDefault(True)
            row.addWidget(b)
        lay.addLayout(row)

    def _press(self, label):
        self.pressed = label
        self.accept()

    def results(self) -> dict:
        r = {}
        for name, (cls, w) in self.widgets.items():
            if cls == "edit":
                r[name] = w.text()
            elif cls == "textbox":
                r[name] = w.toPlainText()
            elif cls == "checkbox":
                r[name] = w.isChecked()
            elif cls == "dropdown":
                r[name] = w.currentText()
            elif cls in ("intedit", "slider"):
                r[name] = int(w.value())
            elif cls == "floatedit":
                r[name] = float(w.value())
            elif cls == "color":
                r[name] = _override_color(w.ass())
            elif cls == "coloralpha":
                r[name] = _style_color(w.ass())
            elif cls == "alpha":
                r[name] = "&H%02X&" % w.value()
        return r


class LogWindow(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Otomasyon günlüğü")
        self.resize(640, 360)
        self.text = QPlainTextEdit(); self.text.setReadOnly(True)
        self.text.setFont(__import__("PySide6.QtGui", fromlist=["QFont"]).QFont("monospace"))
        b = QPushButton("Kapat"); b.clicked.connect(self.accept)
        lay = QVBoxLayout(self); lay.addWidget(self.text); lay.addWidget(b)

    def append(self, s: str):
        self.text.moveCursor(self.text.textCursor().MoveOperation.End)
        self.text.insertPlainText(s)


class QtUI(HeadlessUI):
    """Interactive UI used when running macros from the GUI."""

    def __init__(self, parent: QWidget, title="Otomasyon"):
        super().__init__()
        self.parent = parent
        self.title = title
        self._progress: QProgressDialog | None = None
        self._cancelled = False
        self._timer = QElapsedTimer(); self._timer.start()
        self.log_lines: list[str] = []

    def dialog(self, controls, buttons, ids):
        self._hide_progress()
        d = DynamicDialog(controls, buttons, ids, self.title, self.parent)
        d.exec()
        pressed = d.pressed
        res = d.results() if pressed else {}
        if not pressed:
            return None, res
        if ids and ids.get("cancel") == pressed:
            return None, res
        if buttons is None:
            return (True if pressed == "OK" else None), res
        return pressed, res

    def file_dialog(self, kind, *a):
        title = a[0] if a else ""
        default = (a[1] if len(a) > 1 else "") or ""
        directory = (a[2] if len(a) > 2 else "") or ""
        wild = a[3] if len(a) > 3 else ""
        start = (directory + "/" + default) if directory else default
        if kind == "open":
            multi = bool(a[4]) if len(a) > 4 else False
            if multi:
                files, _ = QFileDialog.getOpenFileNames(self.parent, title, start)
                return files or None
            f, _ = QFileDialog.getOpenFileName(self.parent, title, start)
            return f or None
        f, _ = QFileDialog.getSaveFileName(self.parent, title, start)
        return f or None

    def clipboard_get(self):
        return QGuiApplication.clipboard().text()

    def clipboard_set(self, s):
        QGuiApplication.clipboard().setText(s)
        return True

    def log(self, level, msg):
        self.log_lines.append(msg)
        super().log(level, msg)

    def _hide_progress(self):
        if self._progress:
            self._progress.hide()

    def progress(self, kind, value):
        super().progress(kind, value)
        if self._timer.elapsed() < 400 and self._progress is None:
            return
        if self._progress is None:
            self._progress = QProgressDialog("", "İptal", 0, 100, self.parent)
            self._progress.setWindowModality(Qt.WindowModality.WindowModal)
            self._progress.setMinimumDuration(0)
            self._progress.canceled.connect(lambda: setattr(self, "_cancelled", True))
        pd = self._progress
        if kind == "set":
            pd.setValue(int(float(value)))
        elif kind == "task":
            pd.setLabelText(str(value))
        elif kind == "title":
            pd.setWindowTitle(str(value))
        pd.show()
        QApplication.processEvents()

    def cancelled(self):
        QApplication.processEvents()
        return self._cancelled

    def finish(self):
        if self._progress:
            self._progress.close()
            self._progress = None
