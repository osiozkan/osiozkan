"""Uygulama bağlamı: belge, geri al yığını, seçim ve etkin satır.

Aegisub'daki agi::Context karşılığı. Bütün bileşenler (ızgara, düzenleme
kutusu, menüler, Automation) belgeyi yalnız buradaki yöntemlerle değiştirir;
böylece her değişiklik tek yerden geri al yığınına ve sinyallere gider.
"""

from __future__ import annotations

import os

from PySide6.QtCore import QObject, Signal

from ..config import Config
from ..core.assfile import AssDocument, Event, Style
from ..core.undo import UndoStack


class Context(QObject):
    document_reset = Signal()            # belge yüklendi / geri alındı / yapısal değişti
    rows_changed = Signal(list)          # yalnız bu satırların içeriği değişti
    styles_changed = Signal()
    active_changed = Signal(int)
    selection_changed = Signal()
    undo_changed = Signal()
    status = Signal(str)
    path_changed = Signal()

    def __init__(self, config: Config | None = None):
        super().__init__()
        self.config = config or Config()
        self.doc = AssDocument.new()
        self.undo = UndoStack(self.doc)
        self.undo.listeners.append(self.undo_changed.emit)
        self.active = 0
        self.selection: set[int] = {0}
        self.frame_map = None           # video bağlanınca core.framemap.FrameMap
        self.video_path: str | None = None
        self.audio_path: str | None = None
        self.video_dims = None          # (w, h, ar, artype) — aegisub.video_size()
        self.media = None               # ui.media_ctl.MediaController (ana pencere kurar)
        self.reference = None           # ui.reference.ReferenceController (çeviri modu)

    # ---------------------------------------------------------- dosya
    def new_document(self):
        self.doc = AssDocument.new()
        self.undo.reset(self.doc)
        self._reset_view()

    def open_document(self, path: str):
        from ..core.formats import load_any
        doc = load_any(path)          # .srt/.vtt: yeni belge (path yok, Farklı kaydet ister)
        self.doc = doc
        self.undo.reset(doc)
        self.config.add_recent(os.path.abspath(path))
        self.config.save()
        g = doc.garbage()
        try:
            act = int(g.get("Active Line", "0") or 0)
        except ValueError:
            act = 0
        self._reset_view(active=act)

    def save_document(self, path: str | None = None):
        self.doc.save(path)
        self.undo.mark_saved()
        self.config.add_recent(os.path.abspath(self.doc.path))
        self.config.save()
        self.path_changed.emit()

    def _reset_view(self, active: int = 0):
        n = len(self.doc.events)
        self.active = max(0, min(active, n - 1)) if n else -1
        self.selection = {self.active} if n else set()
        self.document_reset.emit()
        self.path_changed.emit()
        self.active_changed.emit(self.active)
        self.selection_changed.emit()

    @property
    def title(self) -> str:
        src = getattr(self.doc, "source_path", None)
        name = os.path.basename(self.doc.path) if self.doc.path else (
            f"{os.path.basename(src)} (içe aktarıldı)" if src else "Adsız")
        return ("● " if self.undo.modified else "") + name

    # ---------------------------------------------------------- seçim
    def set_active(self, row: int, select_only: bool = False):
        n = len(self.doc.events)
        if n == 0:
            row = -1
        else:
            row = max(0, min(row, n - 1))
        if select_only:
            self.selection = {row} if row >= 0 else set()
            self.selection_changed.emit()
        if row != self.active:
            self.active = row
            self.undo.break_coalesce()
            self.active_changed.emit(row)

    def set_selection(self, rows, active: int | None = None, emit: bool = True):
        n = len(self.doc.events)
        self.selection = {r for r in rows if 0 <= r < n}
        if active is not None and active != self.active and 0 <= active < n:
            self.active = active
            self.undo.break_coalesce()
            self.active_changed.emit(active)
        if emit:
            self.selection_changed.emit()

    def selected_rows(self) -> list[int]:
        rows = sorted(r for r in self.selection if 0 <= r < len(self.doc.events))
        if not rows and 0 <= self.active < len(self.doc.events):
            rows = [self.active]
        return rows

    def active_event(self) -> Event | None:
        if 0 <= self.active < len(self.doc.events):
            return self.doc.events[self.active]
        return None

    # ---------------------------------------------------------- düzenleme
    def update_events(self, changes: dict[int, dict], desc: str, coalesce: str | None = None):
        """{satır: {alan: değer}} uygula; tek geri al adımı."""
        evs = self.doc.events
        touched = []
        for row, kw in changes.items():
            if 0 <= row < len(evs):
                old = evs[row]
                new = old.replace(**kw)
                if new._key() != old._key():
                    evs[row] = new
                    touched.append(row)
        if touched:
            self.undo.commit(desc, coalesce=coalesce)
            self.rows_changed.emit(touched)
        return touched

    def set_field_for_selection(self, field: str, value, desc: str, coalesce: str | None = None):
        return self.update_events({r: {field: value} for r in self.selected_rows()}, desc, coalesce)

    def replace_all_events(self, events: list[Event], desc: str, selection=None, active=None):
        self.doc.ensure_section("events").items = list(events)
        self.undo.commit(desc)
        self.document_reset.emit()
        n = len(events)
        if active is None:
            active = min(self.active, n - 1)
        self.set_selection(selection if selection is not None else ({active} if active >= 0 else set()),
                           active)
        self.active_changed.emit(self.active)

    def insert_events(self, index: int, new: list[Event], desc: str, select: bool = True):
        evs = self.doc.events
        index = max(0, min(index, len(evs)))
        evs[index:index] = new
        self.undo.commit(desc)
        self.document_reset.emit()
        if select and new:
            rows = range(index, index + len(new))
            self.set_selection(rows, index)
            self.active_changed.emit(self.active)

    def delete_rows(self, rows: list[int], desc: str = "Satırları sil"):
        evs = self.doc.events
        rows = sorted(set(r for r in rows if 0 <= r < len(evs)), reverse=True)
        if not rows:
            return
        for r in rows:
            del evs[r]
        if not evs:
            evs.append(Event())       # Aegisub da hiç boş belge bırakmaz
        self.undo.commit(desc)
        self.document_reset.emit()
        act = min(rows[-1], len(evs) - 1)
        self.set_selection({act}, act)
        self.active_changed.emit(self.active)

    def set_styles(self, styles: list[Style], desc: str, renames: dict[str, str] | None = None):
        self.doc.ensure_section("styles").items = list(styles)
        if renames:
            evs = self.doc.events
            for i, ev in enumerate(evs):
                if ev.style in renames:
                    evs[i] = ev.replace(style=renames[ev.style])
        self.undo.commit(desc)
        self.styles_changed.emit()
        if renames:
            self.document_reset.emit()

    def commit_external(self, desc: str, selection=None, active=None):
        """Belge dışarıda (Automation, bul-değiştir...) değiştirildikten sonra çağrılır."""
        self.undo.commit(desc)
        n = len(self.doc.events)
        self.document_reset.emit()
        self.styles_changed.emit()
        if active is None or not (0 <= active < n):
            active = max(0, min(self.active, n - 1))
        sel = selection if selection else {active}
        self.selection = {r for r in sel if 0 <= r < n}
        self.active = active
        self.active_changed.emit(active)
        self.selection_changed.emit()

    # ---------------------------------------------------------- geri al
    def do_undo(self):
        st = self.undo.undo()
        if st:
            self._after_history(f"Geri alındı: {st.desc}")

    def do_redo(self):
        st = self.undo.redo()
        if st:
            self._after_history(f"Yinelendi: {st.desc}")

    def _after_history(self, msg):
        n = len(self.doc.events)
        self.active = max(0, min(self.active, n - 1))
        self.selection = {r for r in self.selection if r < n} or {self.active}
        self.document_reset.emit()
        self.styles_changed.emit()
        self.active_changed.emit(self.active)
        self.selection_changed.emit()
        self.status.emit(msg)
