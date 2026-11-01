"""Small reusable widgets: time edit, ASS syntax highlighter, color button."""
from __future__ import annotations

import re

from PySide6.QtCore import Qt, Signal, QRegularExpression
from PySide6.QtGui import QColor, QFont, QSyntaxHighlighter, QTextCharFormat, QPalette
from PySide6.QtWidgets import QLineEdit, QPushButton, QColorDialog

from ..core.timecode import ms_to_ass, ass_to_ms


class TimeEdit(QLineEdit):
    """H:MM:SS.cc editor. Accepts typed times; Up/Down = ±1 frame (Shift ±1s)."""
    timeChanged = Signal(int)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._ms = 0
        self.frame_ms = 41.7
        self.setFixedWidth(92)
        self.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setFont(QFont("monospace"))
        self.editingFinished.connect(self._commit)
        self.setText(ms_to_ass(0))

    def set_ms(self, ms: int):
        self._ms = int(ms)
        self.setText(ms_to_ass(ms))

    def ms(self) -> int:
        return self._ms

    def _commit(self):
        try:
            t = ass_to_ms(self.text())
        except ValueError:
            self.setText(ms_to_ass(self._ms))
            return
        if t != self._ms:
            self._ms = t
            self.setText(ms_to_ass(t))
            self.timeChanged.emit(t)
        else:
            self.setText(ms_to_ass(t))

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Up, Qt.Key.Key_Down):
            step = 1000 if e.modifiers() & Qt.KeyboardModifier.ShiftModifier else int(round(self.frame_ms))
            t = max(0, self._ms + (step if e.key() == Qt.Key.Key_Up else -step))
            self.set_ms(t)
            self.timeChanged.emit(t)
            return
        super().keyPressEvent(e)


class AssHighlighter(QSyntaxHighlighter):
    def __init__(self, doc, dark=True):
        super().__init__(doc)
        self.set_dark(dark)

    def set_dark(self, dark: bool):
        self.tag = QTextCharFormat()
        self.tag.setForeground(QColor("#7aa2f7" if dark else "#1a56db"))
        self.brace = QTextCharFormat()
        self.brace.setForeground(QColor("#e0af68" if dark else "#b45309"))
        self.nl = QTextCharFormat()
        self.nl.setForeground(QColor("#f7768e" if dark else "#be123c"))
        self.nl.setFontWeight(QFont.Weight.Bold)
        self.rehighlight()

    def highlightBlock(self, text: str):
        for m in re.finditer(r"\{[^}]*\}?", text):
            self.setFormat(m.start(), m.end() - m.start(), self.brace)
            for t in re.finditer(r"\\[a-zA-Z0-9]+", m.group(0)):
                self.setFormat(m.start() + t.start(), t.end() - t.start(), self.tag)
        for m in re.finditer(r"\\[Nnh]", text):
            self.setFormat(m.start(), 2, self.nl)


class ColorButton(QPushButton):
    colorChanged = Signal(str)

    def __init__(self, ass_color="&H00FFFFFF", parent=None):
        super().__init__(parent)
        self.setFixedSize(54, 24)
        self.set_ass(ass_color)
        self.clicked.connect(self._pick)

    def set_ass(self, c: str):
        from ..core.ass import norm_color
        self._c = norm_color(c)
        a, b, g, r = (int(self._c[2 + i * 2:4 + i * 2], 16) for i in range(4))
        self._qc = QColor(r, g, b, 255 - a)
        self.setStyleSheet(f"background:{QColor(r, g, b).name()}; border:1px solid #888;")
        self.setToolTip(f"{self._c}  (alpha {a})")

    def ass(self) -> str:
        return self._c

    def _pick(self):
        c = QColorDialog.getColor(self._qc, self, "Renk", QColorDialog.ColorDialogOption.ShowAlphaChannel)
        if c.isValid():
            self._c = "&H%02X%02X%02X%02X" % (255 - c.alpha(), c.blue(), c.green(), c.red())
            self.set_ass(self._c)
            self.colorChanged.emit(self._c)


def apply_theme(app, dark: bool):
    from PySide6.QtWidgets import QStyleFactory
    app.setStyle(QStyleFactory.create("Fusion"))
    p = QPalette()
    if dark:
        base, alt, text, hl = QColor("#1a1b26"), QColor("#24283b"), QColor("#c0caf5"), QColor("#7aa2f7")
        p.setColor(QPalette.ColorRole.Window, QColor("#16161e"))
        p.setColor(QPalette.ColorRole.WindowText, text)
        p.setColor(QPalette.ColorRole.Base, base)
        p.setColor(QPalette.ColorRole.AlternateBase, alt)
        p.setColor(QPalette.ColorRole.Text, text)
        p.setColor(QPalette.ColorRole.Button, alt)
        p.setColor(QPalette.ColorRole.ButtonText, text)
        p.setColor(QPalette.ColorRole.ToolTipBase, alt)
        p.setColor(QPalette.ColorRole.ToolTipText, text)
        p.setColor(QPalette.ColorRole.Highlight, QColor("#33467c"))
        p.setColor(QPalette.ColorRole.HighlightedText, QColor("#ffffff"))
        p.setColor(QPalette.ColorRole.PlaceholderText, QColor("#565f89"))
        p.setColor(QPalette.ColorRole.Link, hl)
        p.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, QColor("#565f89"))
        p.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, QColor("#565f89"))
    app.setPalette(p if dark else app.style().standardPalette())
