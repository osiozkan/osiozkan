"""Event grid model (virtualised: handles tens of thousands of lines)."""
from __future__ import annotations

from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt
from PySide6.QtGui import QBrush, QColor, QFont

from ..core.ass import Event
from ..core.checks import cps
from ..core.tags import plain_text
from ..core.timecode import ms_to_ass

COLS = ["#", "L", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Aktör", "Efekt", "Metin"]


class EventModel(QAbstractTableModel):
    def __init__(self, project):
        super().__init__()
        self.project = project
        self.events: list[Event] = []
        self.issues: dict[int, str] = {}   # row -> severity
        self.max_cps = 20.0
        self.dark = True
        self.show_plain = False
        self._refresh()
        project.reset.connect(self.rebuild)
        project.rowsChanged.connect(self._rows_changed)

    def _refresh(self):
        self.events = self.project.doc.events

    def rebuild(self):
        self.beginResetModel()
        self._refresh()
        self.endResetModel()

    def _rows_changed(self, a, b):
        self.dataChanged.emit(self.index(a, 0), self.index(b, len(COLS) - 1))

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.events)

    def columnCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(COLS)

    def headerData(self, s, o, role=Qt.ItemDataRole.DisplayRole):
        if o == Qt.Orientation.Horizontal and role == Qt.ItemDataRole.DisplayRole:
            return COLS[s]
        return None

    def data(self, idx, role=Qt.ItemDataRole.DisplayRole):
        e = self.events[idx.row()]
        c = idx.column()
        if role == Qt.ItemDataRole.DisplayRole:
            if c == 0: return idx.row() + 1
            if c == 1: return e.layer
            if c == 2: return ms_to_ass(e.start_time)
            if c == 3: return ms_to_ass(e.end_time)
            if c == 4: return f"{(e.end_time - e.start_time) / 1000:.2f}"
            if c == 5:
                return "" if e.comment else f"{cps(e):.0f}"
            if c == 6: return e.style
            if c == 7: return e.actor
            if c == 8: return e.effect
            if c == 9:
                return plain_text(e.text).replace("\n", " ⏎ ") if self.show_plain else e.text
        elif role == Qt.ItemDataRole.ForegroundRole:
            if e.comment:
                return QBrush(QColor("#6b7394" if self.dark else "#8a8a8a"))
            if c == 5 and cps(e) > self.max_cps:
                return QBrush(QColor("#f7768e" if self.dark else "#c1121f"))
        elif role == Qt.ItemDataRole.BackgroundRole:
            sev = self.issues.get(idx.row())
            if sev == "error":
                return QBrush(QColor(247, 118, 142, 55))
            if sev == "warning":
                return QBrush(QColor(224, 175, 104, 40))
        elif role == Qt.ItemDataRole.TextAlignmentRole:
            if c in (0, 1, 2, 3, 4, 5):
                return int(Qt.AlignmentFlag.AlignCenter)
        elif role == Qt.ItemDataRole.FontRole:
            if e.comment:
                f = QFont(); f.setItalic(True); return f
        elif role == Qt.ItemDataRole.ToolTipRole and c == 9:
            return plain_text(e.text)
        return None
