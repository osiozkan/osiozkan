"""Çeviri modu: referans altyazı, Referans sütunu, aktarma ve inceleme bölmesi.

Referans: başka bir altyazı (.ass/.srt/.vtt) salt okunur olarak yüklenir. Izgarada
her satırın yanında zamanca en çok çakışan referans satırı görünür; düzenleme
kutusunda da etkin satırın referansı durur. "Referanstan aktar" TR satırlarını
EN satırlarına yazar (core/transfer.py) ve tüm sorunları inceleme listesine döker.
Elle düzeltme: referans listesinden bir satır seçip etkin satıra yaz / \\N ile ekle /
yeni satır olarak ekle.
"""

from __future__ import annotations

import os

from dataclasses import dataclass

from PySide6.QtCore import QObject, Qt, Signal
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox,
                               QFileDialog, QFormLayout, QHBoxLayout, QHeaderView, QLabel,
                               QListWidget, QListWidgetItem, QMessageBox, QPushButton, QTabWidget,
                               QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget)

from ..core.asstime import format_time
from ..core.formats import load_any
from ..core.textutil import strip_tags
from ..core.transfer import TransferOptions, best_match, plan_transfer
from . import theme

REF_FILTER = "Altyazı (*.ass *.ssa *.srt *.vtt);;Tüm dosyalar (*)"


@dataclass(slots=True)
class _Span:
    start: int
    end: int


class ReferenceController(QObject):
    """Salt okunur referans altyazı. `offset` (ms): referans zamanlarına eklenen kayma —
    referans belgeden 2,35 sn ERKENSE offset = +2350 olur ve eşleme o kaymayla yapılır
    (Referans sütunu, düzenleme kutusu, aktarma, yeni satır)."""
    changed = Signal()

    def __init__(self, ctx):
        super().__init__()
        self.ctx = ctx
        self.path: str | None = None
        self.events = []
        self.offset = 0
        self._starts: list[int] = []
        self._maxdur = 0
        self._cache: dict = {}
        ctx.rows_changed.connect(lambda *_: self._cache.clear())
        ctx.document_reset.connect(self._cache.clear)

    @property
    def loaded(self) -> bool:
        return self.path is not None

    def load(self, path: str):
        doc = load_any(path)
        evs = [e for e in doc.events if not e.comment]
        evs.sort(key=lambda e: (e.start, e.end))
        self.events = evs
        self._starts = [e.start for e in evs]
        self._maxdur = max((e.end - e.start for e in evs), default=0)
        self.path = path
        self.offset = 0
        self._cache.clear()
        self.changed.emit()

    def load_events(self, events, label: str):
        """Bellekteki satırları referans yap (ör. Whisper dökümü)."""
        evs = sorted((e for e in events if not e.comment), key=lambda e: (e.start, e.end))
        self.events = evs
        self._starts = [e.start for e in evs]
        self._maxdur = max((e.end - e.start for e in evs), default=0)
        self.path = label
        self.offset = 0
        self._cache.clear()
        self.changed.emit()

    def close(self):
        self.path = None
        self.events = []
        self._starts = []
        self.offset = 0
        self._cache.clear()
        self.changed.emit()

    def set_offset(self, ms: int):
        ms = int(round(ms))
        if ms != self.offset:
            self.offset = ms
            self._cache.clear()
            self.changed.emit()

    def shifted_events(self) -> list:
        """Kayma uygulanmış referans satırları (aktarma / yeni satır için)."""
        if not self.offset:
            return list(self.events)
        return [e.replace(start=max(0, e.start + self.offset), end=max(0, e.end + self.offset))
                for e in self.events]

    def find_offset(self, max_shift_ms: int = 15000) -> tuple[int, float]:
        """Referansın belgeye göre kaymasını bul (aligncompare'ın kayma araması; typeset hariç)."""
        from ..core.compare import CompareOptions, collect, find_shift
        o = CompareOptions()
        src, _ = collect(self.events, "src", o)
        cand, _ = collect(self.ctx.doc.events, "cand", o)
        return find_shift(src, cand, max_shift_ms)

    def match(self, ev) -> int | None:
        key = (ev.start, ev.end)
        if key in self._cache:
            return self._cache[key]
        q = _Span(ev.start - self.offset, ev.end - self.offset) if self.offset else ev
        j, r = best_match(q, self.events, self._starts, self._maxdur)
        j = j if r > 0 else None
        if len(self._cache) > 50000:
            self._cache.clear()
        self._cache[key] = j
        return j

    def text_for(self, ev) -> str:
        if not self.loaded:
            return ""
        j = self.match(ev)
        return self.events[j].text if j is not None else ""


class TransferDialog(QDialog):
    def __init__(self, ctx, ref: ReferenceController, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.ref = ref
        self.setWindowTitle("Referanstan aktar (TR → bu dosya)")
        used = {}
        for e in ctx.doc.events:
            if not e.comment:
                used[e.style] = used.get(e.style, 0) + 1
        self.styles = QListWidget()
        self.styles.setMaximumHeight(140)
        for name, n in sorted(used.items(), key=lambda kv: -kv[1]):
            it = QListWidgetItem(f"{name}  ({n} satır)")
            it.setData(Qt.UserRole, name)
            it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
            looks_sign = any(k in name.lower() for k in ("sign", "tabela", "song", "op", "ed", "kara", "title"))
            it.setCheckState(Qt.Unchecked if looks_sign and len(used) > 1 else Qt.Checked)
            self.styles.addItem(it)
        self.min_ov = QDoubleSpinBox()
        self.min_ov.setRange(0.05, 1.0)
        self.min_ov.setSingleStep(0.05)
        self.min_ov.setValue(0.30)
        self.keep_tags = QCheckBox("EN satırın baştaki etiketlerini koru ({\\an8} vb.)")
        self.keep_tags.setChecked(True)
        self.keep_orig = QCheckBox("EN metnini sona {EN: …} yorumu olarak ekle")
        self.ref_timing = QCheckBox("Zamanları referanstan al (varsayılan: EN zamanları korunur)")
        self.orphans = QCheckBox("Ev sahibi olmayan TR satırlarını yeni satır olarak ekle")
        self.orphans.setChecked(True)
        self.summary = QLabel("")
        self.summary.setWordWrap(True)
        form = QFormLayout()
        form.addRow("TR yazılabilecek EN stilleri", self.styles)
        form.addRow("En az çakışma oranı", self.min_ov)
        for w in (self.keep_tags, self.keep_orig, self.ref_timing, self.orphans):
            form.addRow("", w)
        b_prev = QPushButton("Önizle")
        b_prev.clicked.connect(self.preview)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.button(QDialogButtonBox.Ok).setText("Aktar")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addWidget(QLabel(f"Referans: {os.path.basename(ref.path or '')} · {len(ref.events)} satır" +
                             (f" · kayma {ref.offset / 1000:+.2f} sn" if ref.offset else "")))
        lay.addLayout(form)
        h = QHBoxLayout()
        h.addWidget(b_prev)
        h.addWidget(self.summary, 1)
        lay.addLayout(h)
        lay.addWidget(bb)
        self.plan = None

    def options(self) -> TransferOptions:
        styles = {self.styles.item(i).data(Qt.UserRole) for i in range(self.styles.count())
                  if self.styles.item(i).checkState() == Qt.Checked}
        return TransferOptions(min_overlap=self.min_ov.value(), host_styles=styles,
                               keep_leading_tags=self.keep_tags.isChecked(),
                               keep_original=self.keep_orig.isChecked(),
                               use_ref_timing=self.ref_timing.isChecked(),
                               insert_orphans=self.orphans.isChecked())

    def preview(self):
        self.plan = plan_transfer(self.ctx.doc.events, self.ref.shifted_events(), self.options())
        s = self.plan.stats
        self.summary.setText(
            f"{s['filled']}/{s['hosts']} EN satırı doldurulacak · {s['no_ref']} karşılıksız EN (korunur) · "
            f"{s['orphans']} ev sahipsiz TR · {s['multi']} birleştirme · {s['ambiguous']} belirsiz · "
            f"{s['weak']} zayıf")
        return self.plan

    def apply(self):
        plan = self.preview()
        self.ctx.replace_all_events(plan.events, "Referanstan aktar", selection={0}, active=0)
        self.accept()


class ReferencePanel(QWidget):
    """Referans satırları + aktarma raporu."""

    def __init__(self, ctx, ref: ReferenceController, win):
        super().__init__()
        self.ctx = ctx
        self.ref = ref
        self.win = win
        self.issues = []
        b_open = QPushButton("Referans aç…")
        b_close = QPushButton("Kapat")
        b_tr = QPushButton("Referanstan aktar…")
        b_open.clicked.connect(win.open_reference)
        b_close.clicked.connect(ref.close)
        b_tr.clicked.connect(win.transfer_from_reference)
        self.info = QLabel("Referans yok")
        self.offset = QDoubleSpinBox()
        self.offset.setRange(-3600, 3600)
        self.offset.setDecimals(2)
        self.offset.setSingleStep(0.1)
        self.offset.setSuffix(" sn")
        self.offset.setToolTip("Referans kayması: referans zamanlarına eklenir (referans belgeden erkense artı)")
        self.offset.valueChanged.connect(lambda v: self.ref.set_offset(round(v * 1000)))
        b_find = QPushButton("Kaymayı bul")
        b_find.setToolTip("Referans ile belge arasındaki küresel kaymayı bul (±15 sn)")
        b_find.clicked.connect(self.find_offset)
        top = QHBoxLayout()
        for b in (b_open, b_close, b_tr):
            top.addWidget(b)
        top.addWidget(self.info, 1)
        top.addWidget(QLabel("Kayma"))
        top.addWidget(self.offset)
        top.addWidget(b_find)

        self.list = QTreeWidget()
        self.list.setHeaderLabels(["#", "Başlangıç", "Bitiş", "Durum", "Referans metni"])
        self.list.setRootIsDecorated(False)
        self.list.header().setSectionResizeMode(4, QHeaderView.Stretch)
        self.list.itemDoubleClicked.connect(self._go_ref)
        b_write = QPushButton("Etkin satıra yaz")
        b_append = QPushButton("Etkin satıra ekle (\\N)")
        b_new = QPushButton("Yeni satır olarak ekle")
        b_write.clicked.connect(lambda: self._apply_ref("write"))
        b_append.clicked.connect(lambda: self._apply_ref("append"))
        b_new.clicked.connect(lambda: self._apply_ref("new"))
        rb = QHBoxLayout()
        for b in (b_write, b_append, b_new):
            rb.addWidget(b)
        rb.addStretch(1)
        w1 = QWidget()
        l1 = QVBoxLayout(w1)
        l1.setContentsMargins(0, 0, 0, 0)
        l1.addWidget(self.list)
        l1.addLayout(rb)

        self.report = QTreeWidget()
        self.report.setHeaderLabels(["Satır", "Sorun", "Ayrıntı"])
        self.report.setRootIsDecorated(False)
        self.report.header().setSectionResizeMode(2, QHeaderView.Stretch)
        self.report.itemDoubleClicked.connect(self._go_issue)
        self.tabs = QTabWidget()
        self.tabs.addTab(w1, "Referans satırları")
        self.tabs.addTab(self.report, "Aktarma raporu")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addLayout(top)
        lay.addWidget(self.tabs, 1)
        ref.changed.connect(self.refresh)
        ctx.document_reset.connect(self._mark_status)
        ctx.rows_changed.connect(lambda *_: self._mark_status())
        self.refresh()

    def find_offset(self):
        if not self.ref.loaded:
            return None
        ms, score = self.ref.find_offset()
        if score < 0.35:
            self.ctx.status.emit(f"Kayma güvenilir bulunamadı (ilinti %{int(score * 100)}); elle girin")
            return None
        self.offset.setValue(ms / 1000)
        self.ctx.status.emit(f"Referans kayması: {ms / 1000:+.2f} sn (ilinti %{int(score * 100)})")
        return ms

    def refresh(self):
        self.list.clear()
        self.offset.blockSignals(True)
        self.offset.setValue(self.ref.offset / 1000)
        self.offset.blockSignals(False)
        if not self.ref.loaded:
            self.info.setText("Referans yok — .ass/.srt/.vtt aç")
            return
        self.info.setText(f"{os.path.basename(self.ref.path)} · {len(self.ref.events)} satır")
        items = []
        for j, e in enumerate(self.ref.events):
            it = QTreeWidgetItem([str(j + 1), format_time(e.start), format_time(e.end), "",
                                  strip_tags(e.text).replace("\\N", " ⏎ ")])
            it.setData(0, Qt.UserRole, j)
            items.append(it)
        self.list.addTopLevelItems(items)
        for i in range(4):
            self.list.resizeColumnToContents(i)
        self._mark_status()

    def _mark_status(self):
        """Her referans satırının belgedeki bir satıra eşlenip eşlenmediği."""
        if not self.ref.loaded or self.list.topLevelItemCount() == 0:
            return
        used = set()
        for ev in self.ctx.doc.events:
            if not ev.comment:
                j = self.ref.match(ev)
                if j is not None:
                    used.add(j)
        c = theme.colors()
        for k in range(self.list.topLevelItemCount()):
            it = self.list.topLevelItem(k)
            j = it.data(0, Qt.UserRole)
            if j in used:
                it.setText(3, "eşli")
                it.setForeground(3, QColor(c["ok_fg"]))
            else:
                it.setText(3, "EŞSİZ")
                it.setForeground(3, QColor(c["warn_fg"]))

    def show_issues(self, plan):
        self.issues = plan.issues
        self.report.clear()
        c = theme.colors()
        for iss in plan.issues:
            it = QTreeWidgetItem([str(iss.row + 1) if iss.row is not None else "—", iss.label, iss.detail])
            it.setData(0, Qt.UserRole, iss.row)
            it.setData(1, Qt.UserRole, iss.ref)
            col = c["err_fg"] if iss.kind in ("ambiguous", "orphan_skipped") else c["warn_fg"]
            it.setForeground(1, QColor(col))
            self.report.addTopLevelItem(it)
        self.report.resizeColumnToContents(0)
        self.report.resizeColumnToContents(1)
        self.tabs.setCurrentIndex(1)

    def _go_issue(self, item, _col=0):
        row = item.data(0, Qt.UserRole)
        if row is not None:
            self.ctx.set_active(int(row), select_only=True)
        refj = item.data(1, Qt.UserRole)
        if refj is not None:
            self._select_ref(int(refj))

    def _select_ref(self, j):
        for k in range(self.list.topLevelItemCount()):
            it = self.list.topLevelItem(k)
            if it.data(0, Qt.UserRole) == j:
                self.list.setCurrentItem(it)
                self.list.scrollToItem(it)
                return

    def _go_ref(self, item, _col=0):
        j = item.data(0, Qt.UserRole)
        rf = self.ref.shifted_events()[j] if self.ref.offset else self.ref.events[j]
        best, br = None, 0.0
        for i, ev in enumerate(self.ctx.doc.events):
            if ev.comment:
                continue
            ov = min(ev.end, rf.end) - max(ev.start, rf.start)
            if ov > br:
                best, br = i, ov
        if best is not None:
            self.ctx.set_active(best, select_only=True)
        elif self.win.media.index is not None:
            self.win.video.seek_ms(rf.start)

    def selected_ref(self):
        it = self.list.currentItem()
        return None if it is None else int(it.data(0, Qt.UserRole))

    def _apply_ref(self, mode):
        j = self.selected_ref()
        ev = self.ctx.active_event()
        if j is None or ev is None:
            self.ctx.status.emit("Önce referans listesinden bir satır seçin")
            return
        rf = self.ref.shifted_events()[j] if self.ref.offset else self.ref.events[j]
        row = self.ctx.active
        if mode == "write":
            self.ctx.update_events({row: {"text": rf.text}}, "Referans: satıra yaz")
        elif mode == "append":
            self.ctx.update_events({row: {"text": ev.text + "\\N" + rf.text}}, "Referans: satıra ekle")
        else:
            new = ev.fresh().replace(start=rf.start, end=rf.end, text=rf.text, comment=False, effect="")
            evs = self.ctx.doc.events
            pos = len(evs)
            for i, e in enumerate(evs):
                if not e.comment and e.start > rf.start:
                    pos = i
                    break
            self.ctx.insert_events(pos, [new], "Referans: yeni satır")
