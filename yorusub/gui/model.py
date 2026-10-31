from __future__ import annotations
import copy
from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt
from PySide6.QtGui import QBrush, QColor

from ..core import AssFile, ms_to_ass
from ..core.timing import cps

COLS = ["#", "Katman", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Aktör", "Metin"]


class EventModel(QAbstractTableModel):
    """AssFile.events için tablo modeli + anlık görüntü tabanlı geri al/yinele."""

    def __init__(self, doc: AssFile):
        super().__init__()
        self.doc = doc
        self.undo_stack: list[AssFile] = []
        self.redo_stack: list[AssFile] = []
        self.issues: dict[int, str] = {}
        self.dirty = False

    # --- geri al
    def checkpoint(self):
        self.undo_stack.append(copy.deepcopy(self.doc))
        if len(self.undo_stack) > 200:
            self.undo_stack.pop(0)
        self.redo_stack.clear()
        self.dirty = True

    def undo(self) -> bool:
        if not self.undo_stack:
            return False
        self.redo_stack.append(copy.deepcopy(self.doc))
        self.replace_doc(self.undo_stack.pop())
        return True

    def redo(self) -> bool:
        if not self.redo_stack:
            return False
        self.undo_stack.append(copy.deepcopy(self.doc))
        self.replace_doc(self.redo_stack.pop())
        return True

    def replace_doc(self, doc: AssFile):
        self.beginResetModel()
        self.doc = doc
        self.endResetModel()

    # --- Qt API
    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.doc.events)

    def columnCount(self, parent=QModelIndex()):
        return len(COLS)

    def headerData(self, s, o, role=Qt.DisplayRole):
        if role == Qt.DisplayRole and o == Qt.Horizontal:
            return COLS[s]

    def data(self, idx, role=Qt.DisplayRole):
        e = self.doc.events[idx.row()]
        c = idx.column()
        if role == Qt.DisplayRole:
            return [idx.row() + 1, e.layer, ms_to_ass(e.start), ms_to_ass(e.end), f"{e.duration / 1000:.2f}",
                    f"{cps(e):.1f}" if e.duration > 0 else "∞", e.style, e.actor, e.text.replace("\n", " ")][c]
        if role == Qt.ToolTipRole and idx.row() in self.issues:
            return self.issues[idx.row()]
        if role == Qt.ForegroundRole and e.comment:
            return QBrush(QColor("#7d8590"))
        if role == Qt.BackgroundRole and idx.row() in self.issues:
            return QBrush(QColor(255, 120, 80, 60))

    def set_line(self, row: int, **kw):
        e = self.doc.events[row]
        for k, v in kw.items():
            setattr(e, k, v)
        self.dataChanged.emit(self.index(row, 0), self.index(row, len(COLS) - 1))

    def refresh(self):
        self.beginResetModel(); self.endResetModel()
