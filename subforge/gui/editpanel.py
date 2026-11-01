"""Edit box: per-line fields + text editor with counters."""
from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QFont, QTextCursor
from PySide6.QtWidgets import (QCheckBox, QComboBox, QHBoxLayout, QLabel, QLineEdit, QPlainTextEdit, QPushButton,
                               QSpinBox, QToolButton, QVBoxLayout, QWidget)

from ..core.checks import cps as event_cps
from ..core.tags import plain_text
from ..core.timecode import ms_to_ass
from .widgets import AssHighlighter, TimeEdit


class TextEdit(QPlainTextEdit):
    """Plain text edit that sends Ctrl+Return as 'commit/next' and never inserts raw newlines."""
    nextLine = Signal()

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if e.modifiers() & Qt.KeyboardModifier.ControlModifier:
                self.nextLine.emit()
            else:
                self.insertPlainText("\\N")
            return
        super().keyPressEvent(e)

    def insertFromMimeData(self, src):
        self.insertPlainText(src.text().replace("\r\n", "\\N").replace("\n", "\\N"))


class EditPanel(QWidget):
    textChanged = Signal(str)
    fieldChanged = Signal(str, object)     # field name, value
    timeEdited = Signal(str, int)          # "start"|"end"|"dur"
    nextLine = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._loading = False
        self.comment = QCheckBox("Yorum")
        self.layer = QSpinBox(); self.layer.setRange(0, 9999); self.layer.setPrefix("Katman ")
        self.start, self.end, self.dur = TimeEdit(), TimeEdit(), TimeEdit()
        self.dur.setToolTip("Süre")
        self.style = QComboBox(); self.style.setMinimumWidth(110)
        self.actor = QLineEdit(); self.actor.setPlaceholderText("Aktör"); self.actor.setMaximumWidth(110)
        self.effect = QLineEdit(); self.effect.setPlaceholderText("Efekt"); self.effect.setMaximumWidth(110)
        self.ml, self.mr, self.mv = QSpinBox(), QSpinBox(), QSpinBox()
        for w, t in ((self.ml, "Sol"), (self.mr, "Sağ"), (self.mv, "Dikey")):
            w.setRange(0, 9999); w.setToolTip(f"{t} kenar boşluğu (0 = stil)"); w.setFixedWidth(56)
        self.text = TextEdit(); self.text.setFont(QFont("monospace", 11))
        self.text.setMinimumHeight(60)
        self.hl = AssHighlighter(self.text.document())
        self.info = QLabel("")
        self.info.setStyleSheet("font-size:11px")
        self.text.nextLine.connect(self.nextLine)
        self._build()
        self.text.textChanged.connect(self._text_changed)
        self.comment.toggled.connect(lambda v: self._emit("comment", v))
        self.layer.valueChanged.connect(lambda v: self._emit("layer", v))
        self.style.activated.connect(lambda _: self._emit("style", self.style.currentText()))
        self.actor.editingFinished.connect(lambda: self._emit("actor", self.actor.text()))
        self.effect.editingFinished.connect(lambda: self._emit("effect", self.effect.text()))
        self.ml.valueChanged.connect(lambda v: self._emit("margin_l", v))
        self.mr.valueChanged.connect(lambda v: self._emit("margin_r", v))
        self.mv.valueChanged.connect(lambda v: self._emit("margin_v", v))
        self.start.timeChanged.connect(lambda v: self.timeEdited.emit("start", v))
        self.end.timeChanged.connect(lambda v: self.timeEdited.emit("end", v))
        self.dur.timeChanged.connect(lambda v: self.timeEdited.emit("dur", v))
        self.setEnabled(False)

    def _build(self):
        r1 = QHBoxLayout()
        for w in (self.comment, self.layer, QLabel("Baş"), self.start, QLabel("Bit"), self.end, QLabel("Süre"), self.dur,
                  self.style, self.actor):
            r1.addWidget(w)
        r1.addStretch(1)
        r2 = QHBoxLayout()
        for w in (self.ml, self.mr, self.mv, self.effect):
            r2.addWidget(w)
        r2.addSpacing(12)
        for label, tip, fn in (("B", "Kalın {\\b1}", lambda: self._wrap("b")), ("I", "İtalik {\\i1}", lambda: self._wrap("i")),
                               ("U", "Altı çizili {\\u1}", lambda: self._wrap("u")), ("S", "Üstü çizili {\\s1}", lambda: self._wrap("s")),
                               ("\\N", "Satır sonu ekle", lambda: self.text.insertPlainText("\\N")),
                               ("{}", "Etiket bloğu", lambda: self._insert("{}", 1))):
            b = QToolButton(); b.setText(label); b.setToolTip(tip); b.clicked.connect(fn); r2.addWidget(b)
        for an in (7, 8, 9, 4, 5, 6, 1, 2, 3):
            pass
        r2.addStretch(1)
        r2.addWidget(self.info)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addLayout(r1); lay.addLayout(r2); lay.addWidget(self.text, 1)

    def _insert(self, s, back=0):
        c = self.text.textCursor()
        c.insertText(s)
        if back:
            c.movePosition(QTextCursor.MoveOperation.Left, QTextCursor.MoveMode.MoveAnchor, back)
            self.text.setTextCursor(c)
        self.text.setFocus()

    def _wrap(self, tag):
        c = self.text.textCursor()
        sel = c.selectedText()
        if sel:
            c.insertText("{\\%s1}%s{\\%s0}" % (tag, sel, tag))
        else:
            c.insertText("{\\%s1}" % tag)
        self.text.setFocus()

    def set_styles(self, names):
        cur = self.style.currentText()
        self.style.blockSignals(True)
        self.style.clear(); self.style.addItems(names)
        if cur in names:
            self.style.setCurrentText(cur)
        self.style.blockSignals(False)

    def load(self, e, frame_ms=41.7):
        self._loading = True
        self.setEnabled(e is not None)
        if e is not None:
            for t in (self.start, self.end, self.dur):
                t.frame_ms = frame_ms
            self.comment.setChecked(e.comment); self.layer.setValue(e.layer)
            self.start.set_ms(e.start_time); self.end.set_ms(e.end_time); self.dur.set_ms(e.end_time - e.start_time)
            if self.style.findText(e.style) < 0:
                self.style.addItem(e.style)
            self.style.setCurrentText(e.style)
            self.actor.setText(e.actor); self.effect.setText(e.effect)
            self.ml.setValue(e.margin_l); self.mr.setValue(e.margin_r); self.mv.setValue(e.margin_t)
            if self.text.toPlainText() != e.text:
                self.text.setPlainText(e.text)
            self._update_info(e)
        else:
            self.text.setPlainText("")
            self.info.setText("")
        self._loading = False

    def refresh_times(self, e):
        self._loading = True
        self.start.set_ms(e.start_time); self.end.set_ms(e.end_time); self.dur.set_ms(e.end_time - e.start_time)
        self._update_info(e)
        self._loading = False

    def _update_info(self, e):
        plain = plain_text(e.text)
        lines = plain.split("\n")
        c = event_cps(e)
        col = "#f7768e" if c > 20 else "#9ece6a"
        self.info.setText(f"<span style='color:{col}'>{c:.1f} cps</span> · {len(plain.replace(chr(10), ''))} krktr · "
                          f"{'/'.join(str(len(l)) for l in lines)}")

    def _text_changed(self):
        if self._loading:
            return
        self.textChanged.emit(self.text.toPlainText())

    def _emit(self, name, val):
        if not self._loading:
            self.fieldChanged.emit(name, val)
