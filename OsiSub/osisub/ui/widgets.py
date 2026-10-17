"""Küçük ortak bileşenler: zaman kutusu, ASS sözdizimi vurgulayıcı, metin kutusu."""

from __future__ import annotations

import re

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QColor, QFont, QKeySequence, QSyntaxHighlighter, QTextCharFormat
from PySide6.QtWidgets import QLineEdit, QPlainTextEdit

from ..core.asstime import format_time, parse_time
from . import theme


class TimeEdit(QLineEdit):
    """H:MM:SS.cc kutusu. Aegisub gibi üzerine yazarak düzenlenir:
    rakam yazınca imleçteki rakamın yerine geçer, ':' ve '.' atlanır.
    Yukarı/Aşağı ok 10 ms, PageUp/PageDown 1 sn değiştirir. Enter onaylar."""

    committed = Signal(int)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._ms = 0
        self.setText(format_time(0))
        f = QFont("Consolas")
        f.setStyleHint(QFont.Monospace)
        self.setFont(f)
        self.setFixedWidth(self.fontMetrics().horizontalAdvance("0:00:00.000") + 16)
        self.setAlignment(Qt.AlignCenter)
        self.editingFinished.connect(self._finish)

    def value(self) -> int:
        return self._ms

    def setValue(self, ms: int):
        self._ms = int(ms)
        pos = self.cursorPosition()
        self.setText(format_time(self._ms))
        self.setCursorPosition(min(pos, len(self.text())))

    def _finish(self):
        ms = parse_time(self.text())
        self.setText(format_time(ms))
        if ms != self._ms:
            self._ms = ms
            self.committed.emit(ms)

    def keyPressEvent(self, e):
        k = e.key()
        t = e.text()
        if t and t.isdigit() and not (e.modifiers() & (Qt.ControlModifier | Qt.AltModifier)):
            s = list(self.text())
            pos = self.cursorPosition()
            while pos < len(s) and not s[pos].isdigit():
                pos += 1
            if pos >= len(s):
                return
            s[pos] = t
            self.setText("".join(s))
            self.setCursorPosition(pos + 1)
            return
        if k in (Qt.Key_Return, Qt.Key_Enter):
            self._finish()
            return
        if k in (Qt.Key_Up, Qt.Key_Down, Qt.Key_PageUp, Qt.Key_PageDown):
            step = {Qt.Key_Up: 10, Qt.Key_Down: -10, Qt.Key_PageUp: 1000, Qt.Key_PageDown: -1000}[k]
            ms = max(0, parse_time(self.text()) + step)
            self.setValue(ms)
            self._ms = ms
            self.committed.emit(ms)
            return
        if k == Qt.Key_Backspace:
            self.setCursorPosition(max(0, self.cursorPosition() - 1))
            return
        if k == Qt.Key_Delete:
            return
        if e.matches(QKeySequence.Paste):
            from PySide6.QtWidgets import QApplication
            txt = QApplication.clipboard().text().strip()
            if re.match(r"^\d+:\d{1,2}:\d{1,2}[.,]\d{1,3}$", txt):
                ms = parse_time(txt.replace(",", "."))
                self.setValue(ms)
                self._ms = ms
                self.committed.emit(ms)
            return
        if t and t.isprintable() and not (e.modifiers() & Qt.ControlModifier):
            return   # rakam dışı karakterleri yut
        super().keyPressEvent(e)


class AssHighlighter(QSyntaxHighlighter):
    """{...} blokları, etiketler, \\N \\n \\h ve çizim kipi renklendirmesi."""

    _BLOCK = re.compile(r"\{[^}]*\}")
    _TAG = re.compile(r"\\[a-zA-Z0-9]+")
    _BREAK = re.compile(r"\\[Nnh]")
    spell = None                     # SpellService (yazım denetimi; hazır değilse atlanır)

    def highlightBlock(self, text):
        c = theme.colors()
        brace = QTextCharFormat()
        brace.setForeground(QColor(c["tag_brace"]))
        tag = QTextCharFormat()
        tag.setForeground(QColor(c["tag"]))
        brk = QTextCharFormat()
        brk.setForeground(QColor(c["linebreak"]))
        brk.setFontWeight(QFont.Bold)
        draw = QTextCharFormat()
        draw.setForeground(QColor(c["drawing"]))
        drawing = False
        pos = 0
        for m in self._BLOCK.finditer(text):
            if drawing and m.start() > pos:
                self.setFormat(pos, m.start() - pos, draw)
            self.setFormat(m.start(), m.end() - m.start(), brace)
            inner = m.group(0)
            for t in self._TAG.finditer(inner):
                self.setFormat(m.start() + t.start(), t.end() - t.start(), tag)
            for p in re.finditer(r"\\p(\d+)", inner):
                drawing = int(p.group(1)) > 0
            pos = m.end()
        if drawing and pos < len(text):
            self.setFormat(pos, len(text) - pos, draw)
        for m in self._BREAK.finditer(text):
            inside = any(b.start() < m.start() < b.end() for b in self._BLOCK.finditer(text))
            if not inside:
                self.setFormat(m.start(), 2, brk)
        sp = self.spell
        if sp is not None and sp.ready:
            from ..core.qc import words_in
            for w, a, b in words_in(text):
                if not sp.check(w):
                    fmt = self.format(a)
                    fmt.setUnderlineStyle(QTextCharFormat.SpellCheckUnderline)
                    fmt.setUnderlineColor(QColor("#ff5252"))
                    self.setFormat(a, b - a, fmt)


class SubTextEdit(QPlainTextEdit):
    """Altyazı metni kutusu.
    Enter: onayla ve sonraki satıra geç · Ctrl+Enter: onayla, kal
    Shift+Enter: imlece \\N ekle (Aegisub davranışı)."""

    commit_next = Signal()
    commit_stay = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setTabChangesFocus(True)
        self.setLineWrapMode(QPlainTextEdit.WidgetWidth)
        self.highlighter = AssHighlighter(self.document())
        self.extra_actions = []          # sağ tık menüsünün sonuna eklenecek eylemler (ana pencere doldurur)

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key_Return, Qt.Key_Enter):
            mods = e.modifiers()
            if mods & Qt.ShiftModifier:
                self.insertPlainText("\\N")
                return
            if mods & Qt.ControlModifier:
                self.commit_stay.emit()
                return
            self.commit_next.emit()
            return
        super().keyPressEvent(e)

    def word_at(self, pos: int):
        """İmleç konumundaki görünen kelime: (kelime, başlangıç, bitiş) ya da None."""
        from ..core.qc import words_in
        for w, a, b in words_in(self.toPlainText()):
            if a <= pos <= b:
                return w, a, b
        return None

    def contextMenuEvent(self, e):
        menu = self.createStandardContextMenu()
        sp = self.highlighter.spell
        cur = self.cursorForPosition(e.pos())
        hit = self.word_at(cur.position())
        if sp is not None and sp.ready and hit and not sp.check(hit[0]):
            word, a, b = hit
            first = menu.actions()[0] if menu.actions() else None
            sugg = sp.suggest(word)
            acts = []
            for s in sugg:
                act = menu.addAction(s)
                act.triggered.connect(lambda _=False, rep=s, aa=a, bb=b: self._replace(aa, bb, rep))
                acts.append(act)
            if not sugg:
                none = menu.addAction("(öneri yok)")
                none.setEnabled(False)
                acts.append(none)
            add = menu.addAction(f"“{word}” sözlüğe ekle")
            add.triggered.connect(lambda _=False, w=word: (sp.add_word(w), self.highlighter.rehighlight()))
            ign = menu.addAction("Bu oturumda yoksay")
            ign.triggered.connect(lambda _=False, w=word: (sp.ignore(w), self.highlighter.rehighlight()))
            acts += [add, ign]
            if first is not None:
                for act in acts:
                    menu.removeAction(act)
                menu.insertActions(first, acts)
                menu.insertSeparator(first)
        if self.extra_actions:
            menu.addSeparator()
            for act in self.extra_actions:
                menu.addAction(act)
        menu.exec(e.globalPos())

    def _replace(self, a, b, rep):
        c = self.textCursor()
        c.setPosition(a)
        c.setPosition(b, c.MoveMode.KeepAnchor)
        c.insertText(rep)

    def insertFromMimeData(self, source):
        # Yapıştırılan satır sonlarını \N'ye çevir (altyazı tek satırdır)
        txt = source.text().replace("\r\n", "\n").replace("\r", "\n")
        self.insertPlainText(txt.replace("\n", "\\N"))
