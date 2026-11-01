"""Editing session state: document + undo/redo + media bookkeeping, with Qt signals."""
from __future__ import annotations

import dataclasses
import os
import time
from contextlib import contextmanager

from PySide6.QtCore import QObject, Signal

from .core.ass import ASSFile, Event, Style
from .core.formats import load_file, export_text


def _clone(line):
    return dataclasses.replace(line)


class Project(QObject):
    reset = Signal()                 # whole document replaced / big change -> views rebuild
    rowsChanged = Signal(int, int)   # event rows [first,last] content changed
    dirtyChanged = Signal(bool)
    mediaChanged = Signal()
    undoChanged = Signal()
    stylesChanged = Signal()

    MAX_UNDO = 200

    def __init__(self):
        super().__init__()
        self.doc = ASSFile.new()
        self.path: str | None = None
        self.format = "ass"
        self.encoding = "utf-8"
        self.dirty = False
        self.version = 0  # bumps on every change (render cache key)
        self.fps: float | None = None
        self.keyframes: list[int] = []   # ms
        self.video_path = ""
        self.audio_path = ""
        self.video_size: tuple[int, int] | None = None
        self.keyframes_path = ""
        self._undo: list[tuple[str, list]] = []
        self._redo: list[tuple[str, list]] = []
        self._last_merge = (None, 0.0)

    # ------------------------------------------------------------ io
    def new(self):
        self.doc = ASSFile.new()
        self.path = None
        self.format = "ass"
        self._undo.clear(); self._redo.clear()
        self._set_dirty(False)
        self.version += 1
        self.reset.emit(); self.undoChanged.emit()

    def open(self, path: str, encoding: str | None = None):
        doc, fmt, enc = load_file(path, encoding)
        self.doc = doc
        self.path = path if fmt == "ass" else None
        self.format = fmt
        self.encoding = enc
        self._undo.clear(); self._redo.clear()
        self._set_dirty(fmt != "ass")
        self.version += 1
        self.reset.emit(); self.undoChanged.emit()
        return fmt, enc, doc.warnings

    def save(self, path: str | None = None, fmt: str | None = None):
        path = path or self.path
        if not path:
            raise ValueError("no path")
        fmt = (fmt or os.path.splitext(path)[1].lstrip(".") or "ass").lower()
        data = export_text(self.doc, fmt)
        enc = "utf-8-sig" if fmt in ("ass", "ssa") else "utf-8"
        tmp = path + ".tmp"
        with open(tmp, "w", encoding=enc, newline="") as fh:
            fh.write(data)
        os.replace(tmp, path)
        if fmt in ("ass", "ssa"):
            self.path = path
            self._set_dirty(False)

    def _set_dirty(self, v: bool):
        if v != self.dirty:
            self.dirty = v
            self.dirtyChanged.emit(v)

    # ------------------------------------------------------------ undo
    def _snapshot(self):
        return [_clone(l) for l in self.doc.lines]

    @contextmanager
    def edit(self, label: str = "Edit", merge_key=None, structural: bool = True):
        """Wrap a mutation. structural=True emits reset(); False leaves views to emit rowsChanged."""
        now = time.monotonic()
        merge = merge_key is not None and merge_key == self._last_merge[0] and now - self._last_merge[1] < 1.2 \
            and self._undo
        if not merge:
            self._undo.append((label, self._snapshot()))
            if len(self._undo) > self.MAX_UNDO:
                self._undo.pop(0)
        self._redo.clear()
        self._last_merge = (merge_key, now)
        try:
            yield self.doc
        finally:
            self.version += 1
            self._set_dirty(True)
            if structural:
                self.reset.emit()
            self.undoChanged.emit()

    def can_undo(self):
        return bool(self._undo)

    def can_redo(self):
        return bool(self._redo)

    def undo_label(self):
        return self._undo[-1][0] if self._undo else ""

    def redo_label(self):
        return self._redo[-1][0] if self._redo else ""

    def undo(self):
        if not self._undo:
            return
        label, snap = self._undo.pop()
        self._redo.append((label, self._snapshot()))
        self.doc.lines = snap
        self._after_history()

    def redo(self):
        if not self._redo:
            return
        label, snap = self._redo.pop()
        self._undo.append((label, self._snapshot()))
        self.doc.lines = snap
        self._after_history()

    def _after_history(self):
        self._last_merge = (None, 0.0)
        self.version += 1
        self._set_dirty(True)
        self.reset.emit()
        self.undoChanged.emit()

    # ------------------------------------------------------------ helpers
    @property
    def events(self) -> list[Event]:
        return self.doc.events

    @property
    def title(self) -> str:
        return os.path.basename(self.path) if self.path else "Untitled"

    @property
    def frame_ms(self) -> float:
        return 1000.0 / self.fps if self.fps else 1000.0 / 23.976

    def ass_text_for_render(self) -> str:
        return self.doc.to_string()


def _commit_external(self, label: str, snapshot: list):
    """Register a mutation that already happened (e.g. a Lua macro) as one undo step."""
    self._undo.append((label, snapshot))
    if len(self._undo) > self.MAX_UNDO:
        self._undo.pop(0)
    self._redo.clear()
    self._last_merge = (None, 0.0)
    self.version += 1
    self._set_dirty(True)
    self.reset.emit()
    self.undoChanged.emit()


Project.commit_external = _commit_external
Project.snapshot = Project._snapshot
