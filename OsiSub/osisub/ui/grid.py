"""Altyazı ızgarası (Aegisub'daki satır listesi).

Model tabanlı QTableView: yalnız görünen satırlar çizilir, 10 binlerce
satırda da akıcıdır. Sütunlar: #, Katman, Başlangıç, Bitiş, CPS, CPL,
Stil, Aktör, Efekt, Metin. CPL hücresi Osi kurallarına göre renklenir.
"""

from __future__ import annotations

from PySide6.QtCore import QAbstractTableModel, QItemSelection, QItemSelectionModel, QModelIndex, Qt, Signal
from PySide6.QtGui import QAction, QColor, QFont, QKeySequence
from PySide6.QtWidgets import QAbstractItemView, QHeaderView, QMenu, QTableView

from ..core.asstime import format_time
from ..core.textutil import STATUS_ERROR, STATUS_WARN, collapse_tags, cpl_info, cps
from . import theme

COLS = ["#", "K", "Başlangıç", "Bitiş", "CPS", "CPL", "Stil", "Aktör", "Efekt", "Referans", "Metin"]
C_NUM, C_LAYER, C_START, C_END, C_CPS, C_CPL, C_STYLE, C_ACTOR, C_EFFECT, C_REF, C_TEXT = range(11)

CPS_WARN, CPS_ERR = 17, 25


class SubsModel(QAbstractTableModel):
    def __init__(self, ctx):
        super().__init__()
        self.ctx = ctx
        self._cpl_cache: dict = {}
        ctx.document_reset.connect(self.reset)
        ctx.rows_changed.connect(self.rows_changed)
        ctx.active_changed.connect(self._active_changed)
        self._last_active = ctx.active
        self.marker = ctx.config["grid_tag_marker"] or "☀"

    def reset(self):
        self.beginResetModel()
        self._cpl_cache.clear()
        self.endResetModel()

    def rows_changed(self, rows):
        for r in rows:
            self.dataChanged.emit(self.index(r, 0), self.index(r, len(COLS) - 1))

    def _active_changed(self, row):
        for r in (self._last_active, row):
            if 0 <= r < self.rowCount():
                self.dataChanged.emit(self.index(r, 0), self.index(r, len(COLS) - 1),
                                      [Qt.BackgroundRole])
        self._last_active = row

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.ctx.doc.events)

    def columnCount(self, parent=QModelIndex()):
        return len(COLS)

    def headerData(self, section, orientation, role=Qt.DisplayRole):
        if orientation == Qt.Horizontal and role == Qt.DisplayRole:
            ref = getattr(self.ctx, "reference", None)
            if section == C_REF and ref is not None and ref.offset:
                return f"Referans ({ref.offset / 1000:+.2f} sn)"
            return COLS[section]
        if orientation == Qt.Horizontal and role == Qt.ToolTipRole:
            return {
                C_LAYER: "Katman (Layer)",
                C_CPS: "Saniye başına karakter (boşluk/noktalama hariç)",
                C_CPL: "Satır başına karakter: 43+ uyarı, \\N ile bölünmüşte 42'yi aşan parça hata",
            }.get(section)
        return None

    def _cpl(self, ev):
        key = ev.text
        hit = self._cpl_cache.get(key)
        if hit is None:
            hit = cpl_info(key)
            if len(self._cpl_cache) > 20000:
                self._cpl_cache.clear()
            self._cpl_cache[key] = hit
        return hit

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid():
            return None
        row, col = index.row(), index.column()
        evs = self.ctx.doc.events
        if row >= len(evs):
            return None
        ev = evs[row]
        c = theme.colors()
        if role == Qt.DisplayRole:
            if col == C_NUM:
                return str(row + 1)
            if col == C_LAYER:
                return str(ev.layer)
            if col == C_START:
                return format_time(ev.start)
            if col == C_END:
                return format_time(ev.end)
            if col == C_CPS:
                return "" if ev.comment else str(int(cps(ev.text, ev.duration)))
            if col == C_CPL:
                return self._cpl(ev)["label"]
            if col == C_STYLE:
                return ev.style
            if col == C_ACTOR:
                return ev.actor
            if col == C_EFFECT:
                return ev.effect
            if col == C_TEXT:
                return collapse_tags(ev.text, self.marker)
            if col == C_REF:
                ref = getattr(self.ctx, "reference", None)
                return collapse_tags(ref.text_for(ev), self.marker) if ref is not None and ref.loaded else ""
        elif role == Qt.ToolTipRole and col == C_TEXT:
            return ev.text if len(ev.text) < 2000 else ev.text[:2000] + "…"
        elif role == Qt.ToolTipRole and col == C_REF:
            ref = getattr(self.ctx, "reference", None)
            return ref.text_for(ev) if ref is not None and ref.loaded else None
        elif role == Qt.TextAlignmentRole:
            if col in (C_NUM, C_LAYER, C_CPS, C_CPL, C_START, C_END):
                return int(Qt.AlignCenter)
            return int(Qt.AlignVCenter | Qt.AlignLeft)
        elif role == Qt.BackgroundRole:
            if col == C_CPL and not ev.comment:
                st = self._cpl(ev)["status"]
                if st == STATUS_ERROR:
                    return QColor(c["err_bg"])
                if st == STATUS_WARN:
                    return QColor(c["warn_bg"])
            if col == C_CPS and not ev.comment:
                v = cps(ev.text, ev.duration)
                if v >= CPS_ERR:
                    return QColor(c["err_bg"])
                if v >= CPS_WARN:
                    return QColor(c["warn_bg"])
            if row == self.ctx.active:
                return QColor(c["active_bg"])
            if ev.comment:
                return QColor(c["comment_bg"])
        elif role == Qt.ForegroundRole and col == C_REF:
            return QColor(c["muted"])
        elif role == Qt.ForegroundRole:
            if col == C_CPL and not ev.comment:
                st = self._cpl(ev)["status"]
                if st == STATUS_ERROR:
                    return QColor(c["err_fg"])
                if st == STATUS_WARN:
                    return QColor(c["warn_fg"])
            if ev.comment:
                return QColor(c["comment_fg"])
        elif role == Qt.FontRole and ev.comment and col == C_TEXT:
            f = QFont()
            f.setItalic(True)
            return f
        return None


class SubsGrid(QTableView):
    """Izgara görünümü. Seçim, Context ile çift yönlü eşlenir."""

    request_edit_focus = Signal()

    def __init__(self, ctx, actions: dict | None = None):
        super().__init__()
        self.ctx = ctx
        self.actions_map = actions or {}
        self.model_ = SubsModel(ctx)
        self.setModel(self.model_)
        self._syncing = False
        self.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.setAlternatingRowColors(False)
        self.setShowGrid(False)
        self.setWordWrap(False)
        self.setTextElideMode(Qt.ElideRight)
        self.verticalHeader().setVisible(False)
        self.verticalHeader().setSectionResizeMode(QHeaderView.Fixed)
        fs = int(ctx.config["grid_font_size"] or 10)
        f = self.font()
        f.setPointSize(fs)
        self.setFont(f)
        self.verticalHeader().setDefaultSectionSize(int(fs * 2.3))
        h = self.horizontalHeader()
        h.setStretchLastSection(True)
        h.setHighlightSections(False)
        widths = {C_NUM: 46, C_LAYER: 28, C_START: 86, C_END: 86, C_CPS: 40, C_CPL: 62,
                  C_STYLE: 110, C_ACTOR: 90, C_EFFECT: 70, C_REF: 380}
        for col, w in widths.items():
            self.setColumnWidth(col, w)
        self.setColumnHidden(C_REF, True)        # referans yüklenince açılır
        self.selectionModel().selectionChanged.connect(self._on_sel)
        self.selectionModel().currentRowChanged.connect(self._on_current)
        ctx.document_reset.connect(self.sync_from_ctx)
        ctx.selection_changed.connect(self.sync_from_ctx)
        ctx.active_changed.connect(self._ensure_active_visible)
        self.setContextMenuPolicy(Qt.CustomContextMenu)
        self.customContextMenuRequested.connect(self._menu)
        self.doubleClicked.connect(lambda *_: self.request_edit_focus.emit())

    # ------------------------------------------------------------ eşleme
    def _on_sel(self, *_):
        if self._syncing:
            return
        rows = {i.row() for i in self.selectionModel().selectedRows()}
        cur = self.selectionModel().currentIndex()
        active = cur.row() if cur.isValid() else None
        self.ctx.set_selection(rows, active, emit=False)
        self.ctx.selection_changed.emit()

    def _on_current(self, cur, _prev):
        if self._syncing or not cur.isValid():
            return
        self._from_grid = True
        try:
            self.ctx.set_active(cur.row())
        finally:
            self._from_grid = False

    def sync_from_ctx(self):
        if self._syncing:
            return
        self._syncing = True
        try:
            sm = self.selectionModel()
            sel = QItemSelection()
            last = self.model_.columnCount() - 1
            for r in sorted(self.ctx.selection):
                if r < self.model_.rowCount():
                    sel.select(self.model_.index(r, 0), self.model_.index(r, last))
            sm.select(sel, QItemSelectionModel.ClearAndSelect)
            if 0 <= self.ctx.active < self.model_.rowCount():
                sm.setCurrentIndex(self.model_.index(self.ctx.active, 0), QItemSelectionModel.NoUpdate)
        finally:
            self._syncing = False
        self._ensure_active_visible(self.ctx.active)

    def _ensure_active_visible(self, row):
        # Etkin satır ızgaranın kendisinden değiştiyse seçimi Qt yönetir
        # (Ctrl/Shift tıklamaları bozulmasın diye burada dokunulmaz).
        if getattr(self, "_from_grid", False):
            return
        if 0 <= row < self.model_.rowCount():
            self._syncing = True
            try:
                self.selectionModel().setCurrentIndex(self.model_.index(row, 0),
                                                      QItemSelectionModel.NoUpdate)
            finally:
                self._syncing = False
            self.scrollTo(self.model_.index(row, 0), QAbstractItemView.EnsureVisible)

    # ------------------------------------------------------------ menü
    def _menu(self, pos):
        m = QMenu(self)
        for key in ("insert_before", "insert_after", "duplicate", None, "join_concat",
                    "join_keep_first", "split_at_n", "break_lines", None, "copy_lines", "cut_lines", "paste_lines",
                    None, "toggle_comment", "delete_lines", None, "sort_time"):
            if key is None:
                m.addSeparator()
            elif key in self.actions_map:
                m.addAction(self.actions_map[key])
        m.exec(self.viewport().mapToGlobal(pos))

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key_Return, Qt.Key_Enter):
            self.request_edit_focus.emit()
            return
        super().keyPressEvent(e)

    def set_reference_visible(self, on: bool):
        self.setColumnHidden(C_REF, not on)
        self._fit_reference()
        self.model_.reset()

    def _fit_reference(self):
        """Referans ve Metin kalan genişliği yarı yarıya paylaşsın (yatay kaydırma olmasın)."""
        if self.isColumnHidden(C_REF):
            return
        fixed = sum(self.columnWidth(c) for c in range(C_REF) if not self.isColumnHidden(c))
        free = self.viewport().width() - fixed
        self.setColumnWidth(C_REF, max(160, free // 2))

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._fit_reference()
