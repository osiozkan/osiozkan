"""Snapshot based undo/redo for Document."""
from __future__ import annotations
from .ass import Document, Event, Style
from dataclasses import replace


def snapshot(doc: Document):
    return (list(doc.info), [replace(s) for s in doc.styles], [e.copy() for e in doc.events])


def restore(doc: Document, snap) -> None:
    doc.info = list(snap[0])
    doc.styles = [replace(s) for s in snap[1]]
    doc.events = [e.copy() for e in snap[2]]


class History:
    def __init__(self, doc: Document, limit: int = 200):
        self.doc = doc
        self.limit = limit
        self.undo_stack: list[tuple[str, tuple]] = []
        self.redo_stack: list[tuple[str, tuple]] = []
        self.current = snapshot(doc)
        self.saved_marker = self.current

    def commit(self, label: str) -> None:
        """Call after a change has been applied to doc."""
        self.undo_stack.append((label, self.current))
        if len(self.undo_stack) > self.limit:
            self.undo_stack.pop(0)
        self.current = snapshot(self.doc)
        self.redo_stack.clear()

    def undo(self):
        if not self.undo_stack:
            return None
        label, snap = self.undo_stack.pop()
        self.redo_stack.append((label, self.current))
        self.current = snap
        restore(self.doc, snap)
        return label

    def redo(self):
        if not self.redo_stack:
            return None
        label, snap = self.redo_stack.pop()
        self.undo_stack.append((label, self.current))
        self.current = snap
        restore(self.doc, snap)
        return label

    def mark_saved(self):
        self.saved_marker = self.current

    @property
    def dirty(self) -> bool:
        return self.current is not self.saved_marker
