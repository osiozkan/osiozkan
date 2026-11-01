"""Automation manager dialog."""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QDialog, QHBoxLayout, QLabel, QPlainTextEdit, QPushButton, QTreeWidget, QTreeWidgetItem,
                               QVBoxLayout)


class ManagerDialog(QDialog):
    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Otomasyon yöneticisi")
        self.resize(820, 480)
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Betik / makro", "Açıklama", "Yazar", "Sürüm"])
        self.tree.setColumnWidth(0, 300)
        self.detail = QPlainTextEdit(); self.detail.setReadOnly(True); self.detail.setMaximumHeight(110)
        run = QPushButton("Çalıştır"); run.clicked.connect(self._run)
        rel = QPushButton("Yeniden yükle"); rel.clicked.connect(self._reload)
        add = QPushButton("Klasör ekle…"); add.clicked.connect(lambda: (main.add_script_dir(), self._fill()))
        row = QHBoxLayout()
        for b in (run, rel, add):
            row.addWidget(b)
        row.addStretch(1)
        lay = QVBoxLayout(self)
        lay.addWidget(QLabel("Taranan klasörler:\n  " + "\n  ".join(main.script_dirs())))
        lay.addWidget(self.tree); lay.addWidget(self.detail); lay.addLayout(row)
        self.tree.currentItemChanged.connect(self._sel)
        self.tree.itemDoubleClicked.connect(lambda *_: self._run())
        self._fill()

    def _fill(self):
        self.tree.clear()
        for s in self.main.scripts:
            top = QTreeWidgetItem([s.name, s.description, s.author, s.version])
            top.setData(0, Qt.ItemDataRole.UserRole, ("script", s))
            if s.error:
                top.setText(1, "HATA: " + s.error.splitlines()[0][:100])
            self.tree.addTopLevelItem(top)
            for m in s.macros:
                it = QTreeWidgetItem([m.name, m.description]); it.setData(0, Qt.ItemDataRole.UserRole, ("macro", m))
                top.addChild(it)
            for f in s.filters:
                it = QTreeWidgetItem([f"[filtre] {f.name}", f.description]); top.addChild(it)
            top.setExpanded(True)

    def _sel(self, cur, _prev):
        if not cur:
            return
        d = cur.data(0, Qt.ItemDataRole.UserRole)
        if d and d[0] == "script":
            s = d[1]
            self.detail.setPlainText(f"{s.path}\n\n" + (s.error or "Yüklendi."))

    def _run(self):
        it = self.tree.currentItem()
        d = it.data(0, Qt.ItemDataRole.UserRole) if it else None
        if d and d[0] == "macro":
            self.accept()
            self.main.run_macro(d[1])

    def _reload(self):
        self.main.reload_scripts()
        self._fill()
