"""Snapshot-based undo/redo (documents are small; snapshots are simple and robust)."""
from __future__ import annotations
from .ass import Document


class History:
    def __init__(self, limit: int = 200):
        self.undo: list[tuple[str, Document, list[int]]] = []
        self.redo: list[tuple[str, Document, list[int]]] = []
        self.limit = limit

    def push(self, label: str, doc: Document, sel: list[int]) -> None:
        """Record state BEFORE a change."""
        self.undo.append((label, doc.copy(), list(sel)))
        if len(self.undo) > self.limit:
            self.undo.pop(0)
        self.redo.clear()

    def undo_step(self, current: Document, sel: list[int]):
        if not self.undo:
            return None
        label, doc, s = self.undo.pop()
        self.redo.append((label, current.copy(), list(sel)))
        return label, doc, s

    def redo_step(self, current: Document, sel: list[int]):
        if not self.redo:
            return None
        label, doc, s = self.redo.pop()
        self.undo.append((label, current.copy(), list(sel)))
        return label, doc, s
