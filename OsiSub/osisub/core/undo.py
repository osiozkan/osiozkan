"""Anlık görüntü tabanlı geri al / yinele.

Satır nesneleri değişmez olduğu için bir görüntü yalnızca liste
referanslarının kopyasıdır; 10 bin satırlık dosyada bile ucuzdur.
Aynı `coalesce` anahtarıyla art arda gelen kayıtlar (ör. düzenleme
kutusunda yazarken) tek adımda birleşir.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class _State:
    snap: Any
    desc: str
    coalesce: str | None = None
    extra: dict = field(default_factory=dict)   # seçim/aktif satır vb.


class UndoStack:
    def __init__(self, doc, limit: int = 300):
        self.doc = doc
        self.limit = limit
        self.states: list[_State] = [_State(doc.snapshot(), "Açılış")]
        self.index = 0
        self.saved_index = 0
        self.listeners: list = []

    def reset(self, doc):
        self.doc = doc
        self.states = [_State(doc.snapshot(), "Açılış")]
        self.index = 0
        self.saved_index = 0
        self._notify()

    def commit(self, desc: str, coalesce: str | None = None, **extra):
        snap = self.doc.snapshot()
        top = self.states[self.index]
        if coalesce is not None and top.coalesce == coalesce and self.index > 0 \
                and self.index != self.saved_index:
            top.snap = snap
            top.extra.update(extra)
        else:
            del self.states[self.index + 1:]
            self.states.append(_State(snap, desc, coalesce, dict(extra)))
            self.index += 1
            if len(self.states) > self.limit:
                drop = len(self.states) - self.limit
                del self.states[:drop]
                self.index -= drop
                self.saved_index -= drop
        self._notify()

    def break_coalesce(self):
        self.states[self.index].coalesce = None

    @property
    def can_undo(self) -> bool:
        return self.index > 0

    @property
    def can_redo(self) -> bool:
        return self.index < len(self.states) - 1

    def undo_desc(self) -> str:
        return self.states[self.index].desc if self.can_undo else ""

    def redo_desc(self) -> str:
        return self.states[self.index + 1].desc if self.can_redo else ""

    def undo(self) -> _State | None:
        if not self.can_undo:
            return None
        undone = self.states[self.index]
        self.index -= 1
        self.doc.restore(self.states[self.index].snap)
        self._notify()
        return undone

    def redo(self) -> _State | None:
        if not self.can_redo:
            return None
        self.index += 1
        st = self.states[self.index]
        self.doc.restore(st.snap)
        self._notify()
        return st

    def mark_saved(self):
        self.saved_index = self.index
        self.break_coalesce()
        self._notify()

    @property
    def modified(self) -> bool:
        return self.index != self.saved_index

    def _notify(self):
        for cb in list(self.listeners):
            cb()
