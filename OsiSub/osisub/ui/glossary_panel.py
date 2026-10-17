"""Terim sözlüğü bölmesi: düzenlenebilir tablo, aç/kaydet, seçili metinden terim ekle."""

from __future__ import annotations

import os

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QFileDialog, QHBoxLayout, QHeaderView, QLabel, QPushButton,
                               QTableWidget, QTableWidgetItem, QVBoxLayout, QWidget)

from ..core.glossary import Glossary, Term, default_paths

COLS = ["Kaynak (EN/JA)", "Türkçe", "Yasak varyantlar ( | ile)", "Not"]


class GlossaryPanel(QWidget):
    def __init__(self, ctx):
        super().__init__()
        self.ctx = ctx
        self.glossary = Glossary()
        ctx.glossary = self.glossary
        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(COLS)
        self.table.horizontalHeader().setSectionResizeMode(QHeaderView.Stretch)
        self.table.verticalHeader().setVisible(False)
        self.table.itemChanged.connect(self._changed)
        self.path_label = QLabel("")
        self.path_label.setTextInteractionFlags(Qt.TextSelectableByMouse)
        b_add = QPushButton("Yeni terim")
        b_del = QPushButton("Sil")
        b_open = QPushButton("Aç…")
        b_save = QPushButton("Kaydet")
        b_save_as = QPushButton("Farklı kaydet…")
        b_add.clicked.connect(lambda: self.add_term())
        b_del.clicked.connect(self.delete_selected)
        b_open.clicked.connect(self.open_dialog)
        b_save.clicked.connect(self.save)
        b_save_as.clicked.connect(self.save_as)
        top = QHBoxLayout()
        for b in (b_add, b_del, b_open, b_save, b_save_as):
            top.addWidget(b)
        top.addStretch(1)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addLayout(top)
        lay.addWidget(self.table, 1)
        lay.addWidget(self.path_label)
        self.dirty = False
        self._loading = False
        ctx.path_changed.connect(self.auto_load)
        self.auto_load()

    # ------------------------------------------------------------ yükleme
    def auto_load(self):
        """Belgenin klasöründeki terimler.tsv, yoksa kullanıcı klasöründeki genel sözlük."""
        if self.dirty:
            return
        for p in default_paths(self.ctx.doc.path):
            if os.path.exists(p):
                if p != self.glossary.path:
                    self.load(p)
                return
        if self.glossary.path and not os.path.exists(self.glossary.path):
            self.set_glossary(Glossary())

    def load(self, path: str):
        self.set_glossary(Glossary.load(path))
        self.ctx.status.emit(f"Terim sözlüğü: {os.path.basename(path)} · {len(self.glossary.terms)} terim")

    def set_glossary(self, g: Glossary):
        self.glossary = g
        self.ctx.glossary = g
        self._loading = True
        self.table.setRowCount(0)
        for t in g.terms:
            self._append_row(t)
        self._loading = False
        self.dirty = False
        self._update_label()

    def _append_row(self, t: Term):
        r = self.table.rowCount()
        self.table.insertRow(r)
        for c, v in enumerate((t.source, t.target, "|".join(t.forbidden), t.note)):
            self.table.setItem(r, c, QTableWidgetItem(v))

    def _update_label(self):
        p = self.glossary.path or "(kaydedilmedi)"
        self.path_label.setText(f"{p}{' — değişti' if self.dirty else ''} · {len(self.glossary.terms)} terim")

    # ------------------------------------------------------------ düzenleme
    def _changed(self, *_):
        if self._loading:
            return
        self._sync()
        self.dirty = True
        self._update_label()

    def _sync(self):
        terms = []
        for r in range(self.table.rowCount()):
            vals = [(self.table.item(r, c).text() if self.table.item(r, c) else "").strip() for c in range(4)]
            if vals[0] or vals[1]:
                terms.append(Term(vals[0], vals[1], [x.strip() for x in vals[2].split("|") if x.strip()], vals[3]))
        self.glossary.terms = terms

    def add_term(self, source: str = "", target: str = ""):
        self._loading = True
        self._append_row(Term(source, target))
        self._loading = False
        self._sync()
        self.dirty = True
        self._update_label()
        r = self.table.rowCount() - 1
        self.table.setCurrentCell(r, 0 if not source else 1)

    def delete_selected(self):
        rows = sorted({i.row() for i in self.table.selectedIndexes()}, reverse=True)
        for r in rows:
            self.table.removeRow(r)
        self._sync()
        self.dirty = True
        self._update_label()

    # ------------------------------------------------------------ dosya
    def open_dialog(self):
        start = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
        p, _ = QFileDialog.getOpenFileName(self, "Terim sözlüğü aç", start, "Terim sözlüğü (*.tsv *.txt);;Tümü (*)")
        if p:
            self.load(p)

    def save(self):
        if not self.glossary.path:
            return self.save_as()
        self._sync()
        self.glossary.save()
        self.dirty = False
        self._update_label()
        return True

    def save_as(self, path: str | None = None):
        if not path:
            start = default_paths(self.ctx.doc.path)[0]
            path, _ = QFileDialog.getSaveFileName(self, "Terim sözlüğünü kaydet", start, "TSV (*.tsv)")
            if not path:
                return False
        self._sync()
        self.glossary.save(path)
        self.dirty = False
        self._update_label()
        return True
