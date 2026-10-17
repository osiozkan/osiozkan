"""Komut paleti (Ctrl+Shift+P): tüm eylemler, Automation makroları ve bölmeler tek arama kutusunda.

Arama Türkçe karakterlere duyarsızdır ("kirma" → "Dengeli kır", "yazi tipi" → "Yazı tipi toplayıcı"),
harf dizisi sırayla eşleşir; sözcük başı ve ardışık eşleşmeler öne çıkar. ":123" satıra gider.
Boş aramada son kullanılan komutlar üstte gelir.
"""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass

from PySide6.QtCore import QRect, Qt
from PySide6.QtGui import QColor, QKeySequence
from PySide6.QtWidgets import (QDialog, QLabel, QLineEdit, QListWidget, QListWidgetItem, QStyle,
                               QStyledItemDelegate, QVBoxLayout)

from . import theme

SHORTCUT_ROLE = Qt.UserRole + 1


def key_text(seq: QKeySequence) -> str:
    """Kısayolun alışılmış yazımı (Ctrl+Shift+N). Qt'nin Türkçe çevirisi NativeText'te
    "Kontrol+Üst Karakter", Alt için "Seçenek" yazıyor — bu yüzden PortableText."""
    return seq.toString(QKeySequence.PortableText)


class _Delegate(QStyledItemDelegate):
    """Komut adı solda, kısayol sağda soluk renkte."""

    def paint(self, painter, option, index):
        sc = index.data(SHORTCUT_ROLE) or ""
        super().paint(painter, option, index)
        if not sc:
            return
        painter.save()
        sel = option.state & QStyle.State_Selected
        painter.setPen(QColor(theme.colors()["hl_text"] if sel else theme.colors()["muted"]))
        r = QRect(option.rect)
        r.setRight(r.right() - 8)
        painter.drawText(r, Qt.AlignRight | Qt.AlignVCenter, sc)
        painter.restore()

_FOLD = str.maketrans({"ı": "i", "İ": "i", "ş": "s", "Ş": "s", "ğ": "g", "Ğ": "g", "ü": "u", "Ü": "u",
                       "ö": "o", "Ö": "o", "ç": "c", "Ç": "c", "â": "a", "î": "i", "û": "u"})


def fold(s: str) -> str:
    s = s.translate(_FOLD).casefold()
    s = unicodedata.normalize("NFKD", s)
    return "".join(ch for ch in s if not unicodedata.combining(ch))


def score(query: str, text: str) -> float | None:
    """Bulanık eşleşme puanı (büyük = iyi) ya da None. query ve text fold() edilmiş olmalı."""
    if not query:
        return 0.0
    if query in text:
        i = text.index(query)
        bonus = 30 if (i == 0 or not text[i - 1].isalnum()) else 15
        return 100 + bonus - i * 0.5 - len(text) * 0.05
    # sözcükleri ayrı ayrı (her sözcük metinde geçmeli)
    words = query.split()
    if len(words) > 1 and all(w in text for w in words):
        return 80 - len(text) * 0.05
    # harf dizisi
    pos = -1
    total = 0.0
    run = 0
    for ch in query.replace(" ", ""):
        j = text.find(ch, pos + 1)
        if j < 0:
            return None
        if j == pos + 1:
            run += 1
            total += 2 + run
        else:
            run = 0
            total += 1
        if j == 0 or not text[j - 1].isalnum():
            total += 4
        pos = j
    return total - len(text) * 0.05


@dataclass
class Command:
    key: str
    label: str
    shortcut: str
    hint: str
    run: object
    group: str = ""

    @property
    def haystack(self) -> str:
        return fold(f"{self.group} {self.label}")


class CommandPalette(QDialog):
    MAX = 60

    def __init__(self, win):
        super().__init__(win, Qt.Popup | Qt.FramelessWindowHint)
        self.win = win
        self.setObjectName("commandPalette")
        self.edit = QLineEdit()
        self.edit.setPlaceholderText("Komut ara…  (:123 satıra git)")
        self.list = QListWidget()
        self.list.setUniformItemSizes(True)
        self.list.setItemDelegate(_Delegate(self.list))
        self.hint = QLabel("")
        self.hint.setWordWrap(True)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(8, 8, 8, 8)
        lay.addWidget(self.edit)
        lay.addWidget(self.list, 1)
        lay.addWidget(self.hint)
        self.commands = self.collect()
        self.edit.textChanged.connect(self.refresh)
        self.list.itemActivated.connect(lambda it: self.run_item(it))
        self.list.currentItemChanged.connect(self._show_hint)
        self.edit.installEventFilter(self)
        w = min(720, max(480, win.width() // 2))
        self.resize(w, 440)
        geo = win.geometry()
        self.move(geo.x() + (geo.width() - w) // 2, geo.y() + 80)
        self.refresh()

    # ---------------------------------------------------------- komutlar
    def collect(self) -> list[Command]:
        win = self.win
        out = []
        for key, a in win.acts.items():
            label = a.text().replace("&", "")
            if not label:
                continue
            sc = key_text(a.shortcut())
            out.append(Command(key, label, sc, a.toolTip() if a.toolTip() != a.text() else "", a.trigger))
        for macro in getattr(win, "scripts", None) and win.scripts.macros() or []:
            out.append(Command("macro:" + macro.name, macro.menu_path[-1], "", macro.description,
                               lambda m=macro: win.run_macro(m), "Automation › " + " › ".join(macro.menu_path[:-1])))
        for d in win.findChildren(type(win.dock_video)):
            title = d.windowTitle()
            out.append(Command("dock:" + title, f"Bölmeyi göster/gizle: {title}", "", "",
                               lambda dd=d: (dd.setVisible(not dd.isVisible()), dd.raise_()), "Görünüm"))
        for name in (win.ctx.config["layouts"] or {}):
            out.append(Command("layout:" + name, f"Çalışma düzeni: {name}", "", "",
                               lambda n=name: win.apply_layout(n), "Görünüm"))
        for name in getattr(win, "LAYOUT_PRESETS", {}):
            out.append(Command("preset:" + name, f"Çalışma düzeni: {name}", "", "",
                               lambda n=name: win.apply_layout(n), "Görünüm"))
        return out

    def refresh(self, *_):
        q = self.edit.text().strip()
        self.list.clear()
        m = re.fullmatch(r":\s*(\d+)", q)
        if m:
            n = int(m.group(1))
            it = QListWidgetItem(f"Satıra git: {n}")
            it.setData(Qt.UserRole, ("line", n))
            self.list.addItem(it)
            self.list.setCurrentRow(0)
            return
        fq = fold(q)
        recent = list(self.win.ctx.config["palette_recent"] or [])
        scored = []
        for c in self.commands:
            s = score(fq, c.haystack)
            if s is None and c.hint:
                s2 = score(fq, fold(c.hint))
                s = s2 - 40 if s2 is not None else None
            if s is None:
                continue
            if c.key in recent:
                s += (len(recent) - recent.index(c.key)) * (3 if q else 100)
            scored.append((s, c))
        scored.sort(key=lambda x: -x[0])
        for _s, c in scored[:self.MAX]:
            text = c.label + (f"   —   {c.group}" if c.group else "")
            it = QListWidgetItem(text)
            it.setData(Qt.UserRole, ("cmd", c))
            it.setData(SHORTCUT_ROLE, c.shortcut)
            self.list.addItem(it)
        if self.list.count():
            self.list.setCurrentRow(0)

    def _show_hint(self, cur, _prev=None):
        if cur is None:
            self.hint.setText("")
            return
        kind, val = cur.data(Qt.UserRole)
        self.hint.setText(val.hint if kind == "cmd" else "")

    def eventFilter(self, obj, e):
        from PySide6.QtCore import QEvent
        if obj is self.edit and e.type() == QEvent.KeyPress:
            k = e.key()
            if k in (Qt.Key_Down, Qt.Key_Up, Qt.Key_PageDown, Qt.Key_PageUp):
                self.list.keyPressEvent(e)
                return True
            if k in (Qt.Key_Return, Qt.Key_Enter):
                self.run_item(self.list.currentItem())
                return True
            if k == Qt.Key_Escape:
                self.reject()
                return True
        return super().eventFilter(obj, e)

    def run_item(self, item):
        if item is None:
            return None
        kind, val = item.data(Qt.UserRole)
        self.accept()
        if kind == "line":
            self.win.ctx.set_active(val - 1, select_only=True)
            return "line"
        rec = [k for k in (self.win.ctx.config["palette_recent"] or []) if k != val.key]
        self.win.ctx.config["palette_recent"] = [val.key] + rec[:19]
        self.win.ctx.config.save()
        val.run()
        return val.key
