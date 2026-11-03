from __future__ import annotations
from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt
from PySide6.QtGui import QColor, QBrush

from ..core.ass import Document
from ..core.tags import strip_tags, char_count
from ..core.timeutil import ass_time

COLS = ["#", "Katman", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Aktör", "Metin"]


class SubsModel(QAbstractTableModel):
    def __init__(self, doc: Document):
        super().__init__()
        self.doc = doc
        self.issues: dict[int, str] = {}
        self.dark = True
        self.show_tags = True

    def set_doc(self, doc: Document):
        self.beginResetModel(); self.doc = doc; self.endResetModel()

    def refresh(self):
        self.beginResetModel(); self.endResetModel()

    def refresh_rows(self, a: int, b: int):
        if self.doc.events:
            self.dataChanged.emit(self.index(a, 0), self.index(min(b, len(self.doc.events) - 1), len(COLS) - 1))

    def rowCount(self, parent=QModelIndex()): return 0 if parent.isValid() else len(self.doc.events)
    def columnCount(self, parent=QModelIndex()): return len(COLS)

    def headerData(self, s, o, role=Qt.DisplayRole):
        if o == Qt.Horizontal and role == Qt.DisplayRole:
            return COLS[s]
        return None

    def data(self, idx, role=Qt.DisplayRole):
        e = self.doc.events[idx.row()]
        c = idx.column()
        if role == Qt.DisplayRole:
            if c == 0: return idx.row() + 1
            if c == 1: return e.layer
            if c == 2: return ass_time(e.start)
            if c == 3: return ass_time(e.end)
            if c == 4: return f"{e.duration / 1000:.2f}"
            if c == 5:
                d = max(1, e.duration) / 1000
                return f"{char_count(e.text) / d:.1f}"
            if c == 6: return e.style
            if c == 7: return e.actor
            if c == 8:
                t = e.text if self.show_tags else strip_tags(e.text)
                return t.replace("\\N", " ⏎ ").replace("\\n", " ⏎ ")
        elif role == Qt.ForegroundRole:
            if e.comment:
                return QBrush(QColor("#7f8c8d"))
            if c == 5:
                d = max(1, e.duration) / 1000
                if char_count(e.text) / d > 20:
                    return QBrush(QColor("#ff6b6b"))
        elif role == Qt.BackgroundRole:
            if idx.row() in self.issues:
                return QBrush(QColor(120, 30, 30, 90) if self.dark else QColor(255, 200, 200))
            if e.comment:
                return QBrush(QColor(80, 80, 80, 60))
        elif role == Qt.ToolTipRole and idx.row() in self.issues:
            return self.issues[idx.row()]
        elif role == Qt.TextAlignmentRole and c in (0, 1, 2, 3, 4, 5):
            return int(Qt.AlignRight | Qt.AlignVCenter)
        return None
