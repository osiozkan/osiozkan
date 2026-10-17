"""Denetim (QC) bölmesi: kuralları seç, denetle, sorunları gözden geçir, işaretlileri düzelt."""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QComboBox, QHBoxLayout, QHeaderView, QLabel, QListWidget,
                               QListWidgetItem, QPushButton, QSplitter, QTreeWidget,
                               QTreeWidgetItem, QVBoxLayout, QWidget)

from ..core.qc import RULES, RULES_BY_ID, QcOptions, apply_fixes, run_qc
from . import theme


class QcPanel(QWidget):
    def __init__(self, ctx, spell=None):
        super().__init__()
        self.ctx = ctx
        self.spell = spell
        self.issues = []
        saved = set(ctx.config["qc_rules"] or [r.id for r in RULES if r.default])
        self.rules = QListWidget()
        for r in RULES:
            it = QListWidgetItem(r.name)
            it.setData(Qt.UserRole, r.id)
            it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
            it.setCheckState(Qt.Checked if r.id in saved else Qt.Unchecked)
            tip = {"text": "Otomatik düzeltilebilir", "time": "Zamanlama düzeltmesi önerir",
                   "suggest": "Düzeltme önerir (varsayılan işaretsiz)", "info": "Yalnız bildirir"}[r.kind]
            it.setToolTip(tip)
            self.rules.addItem(it)
        self.scope = QComboBox()
        self.scope.addItems(["Tüm satırlar", "Seçili satırlar"])
        b_run = QPushButton("Denetle")
        b_run.clicked.connect(self.run)
        b_fix = QPushButton("İşaretlileri düzelt")
        b_fix.clicked.connect(self.fix_checked)
        self.summary = QLabel("")
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Satır", "Kural", "Ayrıntı"])
        self.tree.setRootIsDecorated(False)
        self.tree.header().setSectionResizeMode(2, QHeaderView.Stretch)
        self.tree.itemDoubleClicked.connect(self._go)
        top = QHBoxLayout()
        top.addWidget(self.scope)
        top.addWidget(b_run)
        top.addWidget(b_fix)
        top.addStretch(1)
        top.addWidget(self.summary)
        # kurallar üstte (dar bölmede sorun listesi tam genişlik alsın)
        split = QSplitter(Qt.Vertical)
        split.addWidget(self.rules)
        split.addWidget(self.tree)
        split.setStretchFactor(0, 1)
        split.setStretchFactor(1, 3)
        self.rules.setMinimumHeight(60)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addLayout(top)
        lay.addWidget(split, 1)
        self.rules.itemChanged.connect(self._save_rules)

    def _save_rules(self, *_):
        self.ctx.config["qc_rules"] = sorted(self.enabled())
        self.ctx.config.save()

    def enabled(self) -> set[str]:
        return {self.rules.item(i).data(Qt.UserRole) for i in range(self.rules.count())
                if self.rules.item(i).checkState() == Qt.Checked}

    def options(self) -> QcOptions:
        cfg = self.ctx.config
        o = QcOptions(min_duration=int(cfg["qc_min_duration"] or 700),
                      max_duration=int(cfg["qc_max_duration"] or 7000),
                      cps_max=float(cfg["qc_cps_max"] or 25), ellipsis=cfg["qc_ellipsis"] or "...")
        o.spell = self.spell if (self.spell is not None and self.spell.ready) else None
        o.glossary = getattr(self.ctx, "glossary", None)
        ref = getattr(self.ctx, "reference", None)
        if ref is not None and ref.loaded:
            evs = self.ctx.doc.events
            o.ref_text = lambda i: ref.text_for(evs[i])
        return o

    def run(self):
        evs = self.ctx.doc.events
        rows = list(range(len(evs))) if self.scope.currentIndex() == 0 else self.ctx.selected_rows()
        en = self.enabled()
        if "spell" in en and (self.spell is None or not self.spell.ready):
            self.ctx.status.emit("Yazım kuralı atlandı: sözlük yüklü değil (Araçlar > Türkçe sözlüğü indir)")
        self.issues = run_qc(evs, rows, en, self.options())
        self._fill()
        return self.issues

    def _fill(self):
        c = theme.colors()
        self.tree.clear()
        counts = {}
        for k, iss in enumerate(self.issues):
            counts[iss.rule] = counts.get(iss.rule, 0) + 1
            rule = RULES_BY_ID[iss.rule]
            it = QTreeWidgetItem([str(iss.row + 1), rule.name, iss.detail])
            it.setData(0, Qt.UserRole, k)
            if iss.fixable:
                it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
                it.setCheckState(0, Qt.Checked if rule.kind == "text" else Qt.Unchecked)
            col = {"error": c["err_fg"], "warn": c["warn_fg"]}.get(iss.severity, c["muted"])
            it.setForeground(1, QColor(col))
            self.tree.addTopLevelItem(it)
        self.tree.resizeColumnToContents(0)
        self.tree.resizeColumnToContents(1)
        fixable = sum(1 for i in self.issues if i.fixable)
        self.summary.setText(f"{len(self.issues)} sorun · {fixable} düzeltilebilir")

    def checked_issues(self):
        out = []
        for i in range(self.tree.topLevelItemCount()):
            it = self.tree.topLevelItem(i)
            if it.flags() & Qt.ItemIsUserCheckable and it.checkState(0) == Qt.Checked:
                out.append(self.issues[it.data(0, Qt.UserRole)])
        return out

    def fix_checked(self):
        sel = self.checked_issues()
        if not sel:
            return 0
        changes = apply_fixes(self.ctx.doc.events, sel, self.options())
        n = len(self.ctx.update_events(changes, f"Denetim düzeltmeleri ({len(changes)} satır)")) if changes else 0
        self.ctx.status.emit(f"{n} satır düzeltildi")
        self.run()
        return n

    def _go(self, item, _col=0):
        iss = self.issues[item.data(0, Qt.UserRole)]
        self.ctx.set_active(iss.row, select_only=True)
