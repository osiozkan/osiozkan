"""Fonts collector, attachments, statistics, shortcut editor, export filters."""
from __future__ import annotations

import os
import re
from collections import Counter

from PySide6.QtCore import Qt
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import (QAbstractItemView, QDialog, QFileDialog, QHBoxLayout, QKeySequenceEdit, QLabel,
                               QListWidget, QListWidgetItem, QMessageBox, QPlainTextEdit, QPushButton, QTableWidget,
                               QTableWidgetItem, QVBoxLayout, QCheckBox)

from ..core import fonts as F
from ..core.tags import plain_text
from ..core.timecode import ms_to_ass
from .dialogs import _buttons


class FontsDialog(QDialog):
    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Yazı tipleri")
        self.resize(720, 460)
        self.doc = main.project.doc
        self.use = F.used_fonts(self.doc)
        self.index = F.font_index()
        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(["Yazı tipi", "Kullanım", "Durum", "Dosya"])
        self.table.horizontalHeader().setStretchLastSection(True)
        self.table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self.table.setColumnWidth(0, 230)
        self.sum = QLabel("")
        b_copy = QPushButton("Klasöre topla…"); b_copy.clicked.connect(self.collect)
        b_embed = QPushButton("Betiğe göm (seçili/tümü)…"); b_embed.clicked.connect(self.embed)
        row = QHBoxLayout(); row.addWidget(b_copy); row.addWidget(b_embed); row.addStretch(1)
        lay = QVBoxLayout(self); lay.addWidget(self.table); lay.addWidget(self.sum); lay.addLayout(row)
        self._fill()

    def _fill(self):
        items = sorted(self.use.items(), key=lambda kv: -kv[1])
        self.table.setRowCount(len(items))
        miss = 0
        for r, (u, n) in enumerate(items):
            files = F.find_font_files(u, self.index)
            ok = bool(files)
            miss += not ok
            vals = [u.label(), str(n), "✔ kurulu" if ok else "✖ EKSİK", ", ".join(os.path.basename(f) for f in files)]
            for c, v in enumerate(vals):
                it = QTableWidgetItem(v)
                if not ok:
                    it.setForeground(Qt.GlobalColor.red)
                self.table.setItem(r, c, it)
        self.items = items
        self.sum.setText(f"{len(items)} yazı tipi kullanılıyor, {miss} eksik")

    def _rows(self):
        sel = sorted({i.row() for i in self.table.selectedIndexes()})
        return sel or list(range(len(self.items)))

    def collect(self):
        d = QFileDialog.getExistingDirectory(self, "Hedef klasör")
        if d:
            copied, missing = F.collect(self.doc, d, self.index)
            QMessageBox.information(self, "Toplandı", f"{len(copied)} dosya kopyalandı.\nEksik: "
                                    + (", ".join(m.label() for m in missing) or "yok"))

    def embed(self):
        atts = F.list_attachments(self.doc)
        names = {a.name for a in atts}
        n = 0
        for r in self._rows():
            for f in F.find_font_files(self.items[r][0], self.index):
                nm = os.path.basename(f)
                if nm in names:
                    continue
                try:
                    with open(f, "rb") as fh:
                        atts.append(F.Attachment(nm, fh.read()))
                    names.add(nm); n += 1
                except OSError:
                    pass
        with self.main.project.edit("Yazı tipi göm"):
            F.set_attachments(self.doc, atts)
        QMessageBox.information(self, "Gömüldü", f"{n} yazı tipi betiğe gömüldü (toplam ek: {len(atts)}).")


class AttachmentsDialog(QDialog):
    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Ekler (gömülü yazı tipleri/grafikler)")
        self.resize(480, 360)
        self.atts = F.list_attachments(main.project.doc)
        self.list = QListWidget()
        b_add = QPushButton("Ekle…"); b_add.clicked.connect(self.add)
        b_ext = QPushButton("Çıkar…"); b_ext.clicked.connect(self.extract)
        b_del = QPushButton("Sil"); b_del.clicked.connect(self.remove)
        row = QHBoxLayout()
        for b in (b_add, b_ext, b_del):
            row.addWidget(b)
        lay = QVBoxLayout(self); lay.addWidget(self.list); lay.addLayout(row); lay.addWidget(_buttons(self, "Uygula"))
        self._fill()

    def _fill(self):
        self.list.clear()
        for a in self.atts:
            self.list.addItem(f"{'🖼' if a.graphic else '🔤'} {a.name}  ({len(a.data) / 1024:.1f} KB)")

    def add(self):
        files, _ = QFileDialog.getOpenFileNames(self, "Dosya ekle")
        for f in files:
            with open(f, "rb") as fh:
                self.atts.append(F.Attachment(os.path.basename(f), fh.read(), graphic=not f.lower().endswith((".ttf", ".otf", ".ttc"))))
        self._fill()

    def extract(self):
        r = self.list.currentRow()
        if r >= 0:
            p, _ = QFileDialog.getSaveFileName(self, "Çıkar", self.atts[r].name)
            if p:
                with open(p, "wb") as fh:
                    fh.write(self.atts[r].data)

    def remove(self):
        r = self.list.currentRow()
        if r >= 0:
            del self.atts[r]; self._fill()

    def accept(self):
        with self.main.project.edit("Ekleri düzenle"):
            F.set_attachments(self.main.project.doc, self.atts)
        super().accept()


class StatsDialog(QDialog):
    def __init__(self, project, parent=None):
        super().__init__(parent)
        self.setWindowTitle("İstatistikler")
        self.resize(520, 480)
        ev = [e for e in project.events]
        dia = [e for e in ev if not e.comment]
        tot = sum(max(0, e.end_time - e.start_time) for e in dia)
        chars = sum(len(plain_text(e.text).replace("\n", "")) for e in dia)
        words = sum(len(plain_text(e.text).split()) for e in dia)
        cps_list = [len(plain_text(e.text).replace("\n", "")) * 1000 / max(1, e.end_time - e.start_time) for e in dia]
        lines = []
        lines.append(f"Satır: {len(ev)} (diyalog {len(dia)}, yorum {len(ev) - len(dia)})")
        if dia:
            lines.append(f"Süre aralığı: {ms_to_ass(min(e.start_time for e in dia))} – {ms_to_ass(max(e.end_time for e in dia))}")
            lines.append(f"Toplam görüntülenme: {ms_to_ass(tot)}  (ort. {tot / len(dia) / 1000:.2f} sn/satır)")
            lines.append(f"Karakter: {chars}  Sözcük: {words}")
            lines.append(f"CPS: ort. {sum(cps_list) / len(cps_list):.1f}, en yüksek {max(cps_list):.1f}; "
                         f">20: {sum(c > 20 for c in cps_list)}")
            longest = max(dia, key=lambda e: max((len(l) for l in plain_text(e.text).split('\n')), default=0))
            lines.append(f"En uzun satır: {max((len(l) for l in plain_text(longest.text).split(chr(10))), default=0)} krktr")
        lines.append("\nStil kullanımı:")
        for k, v in Counter(e.style for e in ev).most_common():
            lines.append(f"  {k}: {v}")
        acts = Counter(e.actor for e in ev if e.actor)
        if acts:
            lines.append("\nAktörler:")
            for k, v in acts.most_common(30):
                lines.append(f"  {k}: {v}")
        t = QPlainTextEdit("\n".join(lines)); t.setReadOnly(True)
        lay = QVBoxLayout(self); lay.addWidget(t)


class ShortcutsDialog(QDialog):
    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Kısayol tuşları")
        self.resize(560, 560)
        self.acts = main.shortcut_actions()
        self.table = QTableWidget(len(self.acts), 2)
        self.table.setHorizontalHeaderLabels(["Komut", "Kısayol"])
        self.table.horizontalHeader().setStretchLastSection(True)
        self.table.setColumnWidth(0, 300)
        self.edits = {}
        for r, (key, a) in enumerate(self.acts.items()):
            it = QTableWidgetItem(a.text().replace("&", ""))
            it.setFlags(Qt.ItemFlag.ItemIsEnabled)
            self.table.setItem(r, 0, it)
            ke = QKeySequenceEdit(a.shortcut())
            self.table.setCellWidget(r, 1, ke)
            self.edits[key] = ke
        reset = QPushButton("Varsayılanlara dön"); reset.clicked.connect(self.reset)
        lay = QVBoxLayout(self); lay.addWidget(self.table); lay.addWidget(reset); lay.addWidget(_buttons(self))

    def reset(self):
        for key, ke in self.edits.items():
            ke.setKeySequence(QKeySequence(self.main.default_shortcuts.get(key, "")))

    def accept(self):
        seen = {}
        for key, ke in self.edits.items():
            s = ke.keySequence().toString()
            if s and s in seen:
                QMessageBox.warning(self, "Kısayol", f"'{s}' hem '{seen[s]}' hem '{key}' için atanmış.")
                return
            seen[s] = key
        for key, ke in self.edits.items():
            self.main.set_shortcut(key, ke.keySequence().toString())
        super().accept()


class ExportFiltersDialog(QDialog):
    """Choose Auto4 export filters (ordered by priority) to run on a copy of the script before saving."""

    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Filtreli dışa aktar")
        self.resize(520, 380)
        self.filters = sorted([f for s in main.scripts for f in s.filters], key=lambda f: -f.priority)
        self.list = QListWidget()
        for f in self.filters:
            it = QListWidgetItem(f"{f.name} — {f.description}")
            it.setFlags(it.flags() | Qt.ItemFlag.ItemIsUserCheckable)
            it.setCheckState(Qt.CheckState.Unchecked)
            self.list.addItem(it)
        if not self.filters:
            self.list.addItem("(yüklü dışa aktarma filtresi yok)")
        lay = QVBoxLayout(self)
        lay.addWidget(QLabel("Filtreler öncelik sırasıyla uygulanır. Özgün betik değişmez."))
        lay.addWidget(self.list); lay.addWidget(_buttons(self, "Dışa aktar…"))

    def chosen(self):
        return [f for f, i in zip(self.filters, range(self.list.count()))
                if self.list.item(i).checkState() == Qt.CheckState.Checked]
