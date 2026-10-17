"""Çoklu değiştir penceresi: kural grupları, kural tablosu, önizleme, tek geri al ile uygulama.

Kurallar config.json > multi_replace altında saklanır; JSON olarak dışa/içe aktarılır,
Subtitle Edit'in çoklu değiştir XML dosyası da içe aktarılabilir.
"""

from __future__ import annotations

import os

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QCheckBox, QComboBox, QDialog, QDialogButtonBox, QFileDialog, QHBoxLayout,
                               QHeaderView, QInputDialog, QLabel, QListWidget, QListWidgetItem, QMessageBox,
                               QPushButton, QSplitter, QTableWidget, QTableWidgetItem, QTreeWidget,
                               QTreeWidgetItem, QVBoxLayout, QWidget)

from ..core import multireplace as M

C_ON, C_FIND, C_REPL, C_MODE, C_CASE, C_NOTE = range(6)


class MultiReplaceDialog(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Çoklu değiştir")
        self.resize(980, 660)
        saved = ctx.config["multi_replace"]
        self.groups = M.from_data(saved) if saved else M.default_groups()
        self._loading = False

        # ---- gruplar
        self.glist = QListWidget()
        self.glist.currentRowChanged.connect(self._group_selected)
        self.glist.itemChanged.connect(self._group_item_changed)
        gb = QHBoxLayout()
        for label, fn in (("Yeni", self.new_group), ("Ad", self.rename_group), ("Sil", self.delete_group)):
            b = QPushButton(label)
            b.clicked.connect(fn)
            gb.addWidget(b)
        gw = QWidget()
        gl = QVBoxLayout(gw)
        gl.setContentsMargins(0, 0, 0, 0)
        gl.addWidget(QLabel("Gruplar (sırayla uygulanır)"))
        gl.addWidget(self.glist, 1)
        gl.addLayout(gb)

        # ---- kurallar
        self.table = QTableWidget(0, 6)
        self.table.setHorizontalHeaderLabels(["Açık", "Bul", "Değiştir", "Tür", "Aa", "Not"])
        self.table.horizontalHeaderItem(C_CASE).setToolTip("Büyük/küçük harf duyarlı")
        hd = self.table.horizontalHeader()
        hd.setSectionResizeMode(C_FIND, QHeaderView.Stretch)
        hd.setSectionResizeMode(C_REPL, QHeaderView.Stretch)
        self.table.setColumnWidth(C_ON, 44)
        self.table.setColumnWidth(C_MODE, 120)
        self.table.setColumnWidth(C_CASE, 36)
        self.table.setColumnWidth(C_NOTE, 140)
        self.table.verticalHeader().setVisible(False)
        self.table.itemChanged.connect(self._rule_item_changed)
        rb = QHBoxLayout()
        for label, fn in (("Kural ekle", self.add_rule), ("Sil", self.delete_rules), ("Yukarı", lambda: self.move_rule(-1)),
                          ("Aşağı", lambda: self.move_rule(1))):
            b = QPushButton(label)
            b.clicked.connect(fn)
            rb.addWidget(b)
        rb.addStretch(1)
        rw = QWidget()
        rl = QVBoxLayout(rw)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.addWidget(QLabel("Kurallar — düz/kelime kurallarında harfe duyarsız eşleşme Türkçe doğrudur, "
                            "baş harfin büyüklüğü korunur"))
        rl.addWidget(self.table, 1)
        rl.addLayout(rb)
        top = QSplitter(Qt.Horizontal)
        top.addWidget(gw)
        top.addWidget(rw)
        top.setStretchFactor(0, 1)
        top.setStretchFactor(1, 4)

        # ---- önizleme
        self.scope = QComboBox()
        self.scope.addItems(["Tüm satırlar", "Seçili satırlar"])
        self.scope.setCurrentIndex(1 if len(ctx.selected_rows()) > 1 else 0)
        b_prev = QPushButton("Önizle")
        b_prev.clicked.connect(self.preview)
        self.summary = QLabel("")
        ph = QHBoxLayout()
        ph.addWidget(self.scope)
        ph.addWidget(b_prev)
        ph.addWidget(self.summary, 1)
        self.prev = QTreeWidget()
        self.prev.setHeaderLabels(["#", "Önce", "Sonra", "Kurallar"])
        self.prev.setRootIsDecorated(False)
        self.prev.setUniformRowHeights(True)
        ph_hd = self.prev.header()
        ph_hd.setSectionResizeMode(1, QHeaderView.Stretch)
        ph_hd.setSectionResizeMode(2, QHeaderView.Stretch)
        self.prev.setColumnWidth(0, 50)
        self.prev.setColumnWidth(3, 180)
        self.prev.itemDoubleClicked.connect(self._go)
        bottom = QWidget()
        bl = QVBoxLayout(bottom)
        bl.setContentsMargins(0, 0, 0, 0)
        bl.addLayout(ph)
        bl.addWidget(self.prev, 1)
        split = QSplitter(Qt.Vertical)
        split.addWidget(top)
        split.addWidget(bottom)
        split.setStretchFactor(0, 3)
        split.setStretchFactor(1, 2)

        b_imp = QPushButton("İçe aktar…")
        b_imp.setToolTip("OsiSub JSON ya da Subtitle Edit çoklu değiştir XML dosyası")
        b_imp.clicked.connect(lambda: self.import_file())
        b_exp = QPushButton("Dışa aktar…")
        b_exp.clicked.connect(lambda: self.export_file())
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Close)
        bb.button(QDialogButtonBox.Ok).setText("Uygula")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.close_save)
        fh = QHBoxLayout()
        fh.addWidget(b_imp)
        fh.addWidget(b_exp)
        fh.addStretch(1)
        fh.addWidget(bb)
        lay = QVBoxLayout(self)
        lay.addWidget(split, 1)
        lay.addLayout(fh)
        self._fill_groups(0)

    # ---------------------------------------------------------- gruplar
    def _fill_groups(self, select=0):
        self._loading = True
        self.glist.clear()
        for g in self.groups:
            it = QListWidgetItem(f"{g.name}  ({len(g.rules)})")
            it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
            it.setCheckState(Qt.Checked if g.enabled else Qt.Unchecked)
            self.glist.addItem(it)
        self._loading = False
        if self.groups:
            self.glist.setCurrentRow(max(0, min(select, len(self.groups) - 1)))
        else:
            self._fill_rules()

    def cur_group(self) -> M.Group | None:
        i = self.glist.currentRow()
        return self.groups[i] if 0 <= i < len(self.groups) else None

    def _group_selected(self, _row):
        self._fill_rules()

    def _group_item_changed(self, item):
        if self._loading:
            return
        i = self.glist.row(item)
        if 0 <= i < len(self.groups):
            self.groups[i].enabled = item.checkState() == Qt.Checked

    def new_group(self, name: str | None = None):
        if name is None:
            name, ok = QInputDialog.getText(self, "Yeni grup", "Ad:", text="Yeni grup")
            if not ok or not name.strip():
                return
        self.groups.append(M.Group(name.strip()))
        self._fill_groups(len(self.groups) - 1)

    def rename_group(self):
        g = self.cur_group()
        if g is None:
            return
        name, ok = QInputDialog.getText(self, "Grup adı", "Ad:", text=g.name)
        if ok and name.strip():
            g.name = name.strip()
            self._fill_groups(self.glist.currentRow())

    def delete_group(self):
        i = self.glist.currentRow()
        if 0 <= i < len(self.groups):
            if QMessageBox.question(self, "Grup sil", f"'{self.groups[i].name}' silinsin mi?") == QMessageBox.Yes:
                del self.groups[i]
                self._fill_groups(i)

    # ---------------------------------------------------------- kurallar
    def _fill_rules(self):
        self._loading = True
        g = self.cur_group()
        self.table.setRowCount(0)
        for r in (g.rules if g else []):
            self._append_row(r)
        self._loading = False

    def _append_row(self, r: M.Rule):
        row = self.table.rowCount()
        self.table.insertRow(row)
        on = QTableWidgetItem()
        on.setFlags(Qt.ItemIsUserCheckable | Qt.ItemIsEnabled | Qt.ItemIsSelectable)
        on.setCheckState(Qt.Checked if r.enabled else Qt.Unchecked)
        self.table.setItem(row, C_ON, on)
        self.table.setItem(row, C_FIND, QTableWidgetItem(r.find))
        self.table.setItem(row, C_REPL, QTableWidgetItem(r.replace))
        mode = QComboBox()
        for k, label in M.MODES.items():
            mode.addItem(label, k)
        mode.setCurrentIndex(max(0, mode.findData(r.mode)))
        mode.currentIndexChanged.connect(lambda _i: self._sync_rules())
        self.table.setCellWidget(row, C_MODE, mode)
        case = QTableWidgetItem()
        case.setFlags(Qt.ItemIsUserCheckable | Qt.ItemIsEnabled | Qt.ItemIsSelectable)
        case.setCheckState(Qt.Checked if r.case else Qt.Unchecked)
        self.table.setItem(row, C_CASE, case)
        self.table.setItem(row, C_NOTE, QTableWidgetItem(r.note))

    def _rule_item_changed(self, _item):
        if not self._loading:
            self._sync_rules()

    def _sync_rules(self):
        """Tablodaki satırları etkin grubun kural listesine yazar."""
        g = self.cur_group()
        if g is None:
            return
        rules = []
        for row in range(self.table.rowCount()):
            def t(c):
                it = self.table.item(row, c)
                return it.text() if it is not None else ""
            mode = self.table.cellWidget(row, C_MODE)
            on = self.table.item(row, C_ON)
            case = self.table.item(row, C_CASE)
            rules.append(M.Rule(find=t(C_FIND), replace=t(C_REPL), mode=mode.currentData() if mode else "word",
                                case=case is not None and case.checkState() == Qt.Checked,
                                enabled=on is None or on.checkState() == Qt.Checked, note=t(C_NOTE)))
        g.rules = rules
        i = self.groups.index(g)
        it = self.glist.item(i)
        if it is not None:
            self._loading = True
            it.setText(f"{g.name}  ({len(rules)})")
            self._loading = False

    def add_rule(self, find: str = "", replace: str = "", mode: str = "word"):
        if self.cur_group() is None:
            self.new_group("Kurallarım")
        self._loading = True
        self._append_row(M.Rule(find, replace, mode))
        self._loading = False
        self._sync_rules()
        row = self.table.rowCount() - 1
        self.table.setCurrentCell(row, C_FIND)
        if not find:
            self.table.editItem(self.table.item(row, C_FIND))

    def delete_rules(self):
        rows = sorted({i.row() for i in self.table.selectedIndexes()}, reverse=True)
        for r in rows:
            self.table.removeRow(r)
        self._sync_rules()

    def move_rule(self, d):
        g = self.cur_group()
        row = self.table.currentRow()
        j = row + d
        if g is None or not (0 <= row < len(g.rules)) or not (0 <= j < len(g.rules)):
            return
        g.rules[row], g.rules[j] = g.rules[j], g.rules[row]
        self._fill_rules()
        self.table.setCurrentCell(j, C_FIND)

    # ---------------------------------------------------------- önizleme / uygulama
    def _rows(self):
        n = len(self.ctx.doc.events)
        return list(range(n)) if self.scope.currentIndex() == 0 else self.ctx.selected_rows()

    def compute(self):
        self._sync_rules()
        return M.apply_events(self.ctx.doc.events, self._rows(), self.groups)

    def preview(self):
        changes, why, errors = self.compute()
        self.prev.clear()
        evs = self.ctx.doc.events
        items = []
        for r, ch in sorted(changes.items())[:3000]:
            it = QTreeWidgetItem([str(r + 1), evs[r].text.replace("\\N", " ⏎ "), ch["text"].replace("\\N", " ⏎ "),
                                  ", ".join(why.get(r, []))])
            it.setData(0, Qt.UserRole, r)
            items.append(it)
        self.prev.addTopLevelItems(items)
        msg = f"{len(changes)} satır değişecek"
        if errors:
            msg += " · HATALI KURAL: " + "; ".join(errors[:3])
        self.summary.setText(msg)
        return changes, errors

    def _go(self, item, _col=0):
        r = item.data(0, Qt.UserRole)
        if r is not None:
            self.ctx.set_active(int(r), select_only=True)

    def save(self):
        self._sync_rules()
        self.ctx.config["multi_replace"] = M.to_data(self.groups)
        self.ctx.config.save()

    def apply(self):
        changes, _why, errors = self.compute()
        self.save()
        if errors:
            self.summary.setText("HATALI KURAL: " + "; ".join(errors[:3]))
            return 0
        n = len(self.ctx.update_events(changes, f"Çoklu değiştir ({len(changes)} satır)")) if changes else 0
        self.ctx.status.emit(f"Çoklu değiştir: {n} satır değişti")
        self.accept()
        return n

    def close_save(self):
        self.save()
        self.reject()

    def import_file(self, path: str | None = None):
        if path is None:
            path, _ = QFileDialog.getOpenFileName(self, "Kural dosyası", "", "Kurallar (*.json *.xml *.template);;"
                                                  "Tüm dosyalar (*)")
            if not path:
                return 0
        try:
            new = M.load_file(path)
        except Exception as e:  # noqa: BLE001
            QMessageBox.warning(self, "İçe aktarma", f"{os.path.basename(path)} okunamadı:\n{e}")
            return 0
        self.groups.extend(new)
        self._fill_groups(len(self.groups) - 1)
        self.summary.setText(f"{len(new)} grup, {sum(len(g.rules) for g in new)} kural eklendi")
        return len(new)

    def export_file(self, path: str | None = None):
        self._sync_rules()
        if path is None:
            path, _ = QFileDialog.getSaveFileName(self, "Kuralları dışa aktar", "coklu_degistir.json", "JSON (*.json)")
            if not path:
                return None
        M.save_json(path, self.groups)
        return path
