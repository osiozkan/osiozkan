from __future__ import annotations
from PySide6.QtCore import QAbstractTableModel, Qt, QModelIndex
from PySide6.QtGui import QColor, QBrush
from ..core.ass import Document
from ..core.timeutil import format_ass_time
from ..core.tags import strip_tags

COLUMNS = ["#", "Katman", "Başla", "Bitir", "Süre", "CPS", "Stil", "Aktör", "Efekt", "Metin"]


class EventModel(QAbstractTableModel):
    def __init__(self, doc: Document, parent=None):
        super().__init__(parent)
        self.doc = doc
        self.overlaps: set[int] = set()
        self.max_cps = 20.0
        self.recompute()

    def set_document(self, doc: Document) -> None:
        self.beginResetModel()
        self.doc = doc
        self.recompute()
        self.endResetModel()

    def refresh(self) -> None:
        self.beginResetModel()
        self.recompute()
        self.endResetModel()

    def refresh_row(self, row: int) -> None:
        self.recompute()
        self.dataChanged.emit(self.index(row, 0), self.index(row, len(COLUMNS) - 1))

    def recompute(self) -> None:
        order = sorted((i for i, e in enumerate(self.doc.events) if not e.comment),
                       key=lambda i: (self.doc.events[i].start, self.doc.events[i].end))
        self.overlaps = set()
        for a, b in zip(order, order[1:]):
            ea, eb = self.doc.events[a], self.doc.events[b]
            if ea.end > eb.start and ea.style == eb.style and ea.layer == eb.layer and not (ea.effect or eb.effect):
                self.overlaps.update((a, b))

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.doc.events)

    def columnCount(self, parent=QModelIndex()):
        return len(COLUMNS)

    def headerData(self, section, orientation, role=Qt.DisplayRole):
        if role == Qt.DisplayRole and orientation == Qt.Horizontal:
            return COLUMNS[section]
        return None

    def cps(self, row: int) -> float:
        e = self.doc.events[row]
        if e.duration <= 0:
            return 0.0
        return len(strip_tags(e.text).replace("\n", "")) / (e.duration / 1000)

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid():
            return None
        r, c = index.row(), index.column()
        e = self.doc.events[r]
        if role == Qt.DisplayRole:
            if c == 0:
                return str(r + 1)
            if c == 1:
                return str(e.layer)
            if c == 2:
                return format_ass_time(e.start)
            if c == 3:
                return format_ass_time(e.end)
            if c == 4:
                return f"{e.duration / 1000:.2f}"
            if c == 5:
                return f"{self.cps(r):.1f}"
            if c == 6:
                return e.style
            if c == 7:
                return e.actor
            if c == 8:
                return e.effect
            if c == 9:
                return e.text.replace("\\N", " ⏎ ")
        elif role == Qt.ToolTipRole and c == 9:
            return e.text
        elif role == Qt.BackgroundRole:
            if e.comment:
                return QBrush(QColor(70, 90, 70, 90))
            if r in self.overlaps:
                return QBrush(QColor(200, 60, 60, 90))
            if c == 5 and self.cps(r) > self.max_cps:
                return QBrush(QColor(220, 150, 40, 110))
        elif role == Qt.TextAlignmentRole and c in (0, 1, 2, 3, 4, 5):
            return int(Qt.AlignRight | Qt.AlignVCenter)
        return None
