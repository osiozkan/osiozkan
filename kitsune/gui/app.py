"""Kitsune main window (PySide6)."""
from __future__ import annotations
import copy, sys, re
from pathlib import Path
from PySide6.QtCore import Qt, QAbstractTableModel, QModelIndex, QSettings
from PySide6.QtGui import QAction, QKeySequence, QColor, QFontMetricsF, QFont
from PySide6.QtWidgets import (QApplication, QMainWindow, QTableView, QWidget, QVBoxLayout, QHBoxLayout, QFileDialog,
    QPlainTextEdit, QLineEdit, QComboBox, QSpinBox, QLabel, QSplitter, QDockWidget, QListWidget, QMessageBox,
    QAbstractItemView, QInputDialog, QCheckBox, QDialog, QFormLayout, QDialogButtonBox, QDoubleSpinBox,
    QPushButton, QTextEdit)
from ..core.ass import Script, Event, Style
from ..core import srt as srtmod
from ..core.checks import lint, fix_basic
from ..core.tags import cps, strip_tags
from ..core.timecode import ms_to_ass, ass_to_ms
from ..lua.runtime import AutomationHost

COLS = ["#", "C", "Katman", "Başlangıç", "Bitiş", "Süre", "CPS", "Stil", "Aktör", "Metin"]

class Model(QAbstractTableModel):
    def __init__(self, win): super().__init__(); self.win = win
    @property
    def evs(self): return self.win.events()
    def rowCount(self, p=QModelIndex()): return 0 if p.isValid() else len(self.evs)
    def columnCount(self, p=QModelIndex()): return len(COLS)
    def headerData(self, s, o, r=Qt.DisplayRole):
        return COLS[s] if o == Qt.Horizontal and r == Qt.DisplayRole else None
    def data(self, ix, role=Qt.DisplayRole):
        e = self.evs[ix.row()]; c = ix.column()
        if role == Qt.DisplayRole:
            return [ix.row() + 1, "#" if e.comment else "", e.layer, ms_to_ass(e.start), ms_to_ass(e.end),
                    f"{(e.end - e.start) / 1000:.2f}", f"{cps(e.text, e.end - e.start):.0f}", e.style, e.actor,
                    e.text.replace("\\N", " ⏎ ")][c]
        if role == Qt.BackgroundRole:
            if e.comment: return QColor(70, 90, 70, 90)
            if c == 6 and cps(e.text, e.end - e.start) > 20: return QColor(200, 60, 60, 120)
        if role == Qt.TextAlignmentRole and c in (0, 2, 5, 6): return Qt.AlignRight | Qt.AlignVCenter
    def refresh(self): self.beginResetModel(); self.endResetModel()
    def row_changed(self, r): self.dataChanged.emit(self.index(r, 0), self.index(r, len(COLS) - 1))

class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.host = AutomationHost(Script())
        self.host.dialog_handler = self.lua_dialog
        self.host.text_measure = self.measure
        self.undo, self.redo = [], []
        self.dirty = False
        self.model = Model(self); self._loading = False
        self._build(); self._menus(); self._title()
        self.autoload_macros()

    # ---- helpers
    @property
    def script(self): return self.host.script
    def events(self): return self.script.events
    def _title(self):
        self.setWindowTitle(f"Kitsune — {Path(self.script.filename).name if self.script.filename else 'Adsız'}{' *' if self.dirty else ''}")

    def measure(self, st, text):
        f = QFont(st["fontname"]); f.setPixelSize(int(float(st["fontsize"]))); f.setBold(bool(st["bold"])); f.setItalic(bool(st["italic"]))
        m = QFontMetricsF(f)
        w = m.horizontalAdvance(text) * float(st["scale_x"]) / 100 + float(st["spacing"]) * len(text)
        return w, m.height() * float(st["scale_y"]) / 100, m.descent(), m.leading()

    def push_undo(self):
        self.undo.append(copy.deepcopy(self.script.entries)); self.undo = self.undo[-200:]; self.redo.clear()
        self.dirty = True; self._title()

    def do_undo(self, redo=False):
        src, dst = (self.redo, self.undo) if redo else (self.undo, self.redo)
        if not src: return
        dst.append(copy.deepcopy(self.script.entries)); self.script.entries = src.pop()
        self.reload_all()

    def reload_all(self):
        self.model.refresh(); self.refresh_styles(); self.dirty = True; self._title()

    def refresh_styles(self):
        self.styleBox.blockSigns = True
        self.styleBox.clear(); self.styleBox.addItems([s.name for s in self.script.styles])

    # ---- UI
    def _build(self):
        self.table = QTableView(); self.table.setModel(self.model)
        self.table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.table.verticalHeader().setVisible(False); self.table.verticalHeader().setDefaultSectionSize(22)
        self.table.horizontalHeader().setStretchLastSection(True)
        for i, w in enumerate([50, 24, 56, 90, 90, 56, 44, 110, 90]): self.table.setColumnWidth(i, w)
        self.table.selectionModel().selectionChanged.connect(self.on_select)
        self.table.doubleClicked.connect(lambda *_: self.textEdit.setFocus())
        # edit panel
        self.textEdit = QPlainTextEdit(); self.textEdit.setPlaceholderText("Altyazı metni (satır sonu = \\N)")
        self.textEdit.setMaximumHeight(110)
        self.startEdit, self.endEdit = QLineEdit(), QLineEdit()
        self.layerSpin = QSpinBox(); self.layerSpin.setRange(0, 9999)
        self.styleBox = QComboBox(); self.actorEdit = QLineEdit(); self.commentChk = QCheckBox("Yorum")
        self.info = QLabel("")
        row = QHBoxLayout()
        for lab, w in (("Katman", self.layerSpin), ("Başl.", self.startEdit), ("Bitiş", self.endEdit),
                       ("Stil", self.styleBox), ("Aktör", self.actorEdit)):
            row.addWidget(QLabel(lab)); row.addWidget(w)
        row.addWidget(self.commentChk); row.addWidget(self.info); row.addStretch()
        edit = QWidget(); lay = QVBoxLayout(edit); lay.addLayout(row); lay.addWidget(self.textEdit)
        sp = QSplitter(Qt.Vertical); sp.addWidget(self.table); sp.addWidget(edit); sp.setStretchFactor(0, 1)
        self.setCentralWidget(sp)
        for w in (self.startEdit, self.endEdit, self.actorEdit): w.editingFinished.connect(self.commit_fields)
        self.layerSpin.valueChanged.connect(lambda *_: self.commit_fields())
        self.styleBox.currentTextChanged.connect(lambda *_: self.commit_fields())
        self.commentChk.toggled.connect(lambda *_: self.commit_fields())
        self.textEdit.textChanged.connect(self.commit_text)
        # docks: QC + log
        self.qc = QListWidget(); self.qc.itemDoubleClicked.connect(self.goto_issue)
        d = QDockWidget("Kalite Kontrol", self); d.setWidget(self.qc); self.addDockWidget(Qt.RightDockWidgetArea, d)
        self.logBox = QTextEdit(); self.logBox.setReadOnly(True)
        d2 = QDockWidget("Otomasyon Günlüğü", self); d2.setWidget(self.logBox); self.addDockWidget(Qt.BottomDockWidgetArea, d2)
        self.resize(1280, 760)

    def act(self, menu, text, fn, key=None):
        a = QAction(text, self); a.triggered.connect(lambda *_: fn())
        if key: a.setShortcut(QKeySequence(key))
        menu.addAction(a); return a

    def _menus(self):
        mb = self.menuBar(); f = mb.addMenu("&Dosya")
        self.act(f, "Yeni", self.new, "Ctrl+N"); self.act(f, "Aç…", self.open, "Ctrl+O")
        self.act(f, "Kaydet", self.save, "Ctrl+S"); self.act(f, "Farklı kaydet…", lambda: self.save(True), "Ctrl+Shift+S")
        self.act(f, "SRT/VTT olarak dışa aktar…", self.export_srt); f.addSeparator(); self.act(f, "Çık", self.close, "Ctrl+Q")
        e = mb.addMenu("&Düzen")
        self.act(e, "Geri al", self.do_undo, "Ctrl+Z"); self.act(e, "Yinele", lambda: self.do_undo(True), "Ctrl+Y")
        e.addSeparator()
        self.act(e, "Satır ekle (sonra)", lambda: self.insert_line(1), "Ctrl+Return")
        self.act(e, "Satır ekle (önce)", lambda: self.insert_line(0))
        self.act(e, "Seçili satırları sil", self.delete_lines, "Ctrl+Delete")
        self.act(e, "Çoğalt", self.duplicate, "Ctrl+D"); self.act(e, "Seçilileri birleştir", self.join, "Ctrl+J")
        self.act(e, "Bul/Değiştir…", self.find_replace, "Ctrl+H")
        self.act(e, "Zamanı kaydır…", self.shift_times, "Ctrl+T")
        t = mb.addMenu("&Araçlar")
        self.act(t, "Kalite kontrol çalıştır", self.run_qc, "F7"); self.act(t, "Güvenli otomatik düzelt", self.autofix)
        self.act(t, "Stil yöneticisi…", self.style_manager)
        self.act(t, "Betik özellikleri…", self.script_props)
        self.autoMenu = mb.addMenu("&Otomasyon")

    def rebuild_auto_menu(self):
        self.autoMenu.clear()
        self.act(self.autoMenu, "Lua betiği yükle…", self.load_lua)
        self.act(self.autoMenu, "Betikleri yeniden yükle", self.reload_macros); self.autoMenu.addSeparator()
        sub = {}
        for m in self.host.macros():
            parts = m.name.split("/"); menu = self.autoMenu
            for p in parts[:-1]:
                if p not in sub: sub[p] = menu.addMenu(p)
                menu = sub[p]
            a = QAction(parts[-1], self); a.setToolTip(m.description); a.setStatusTip(m.description)
            a.triggered.connect(lambda _=False, mm=m: self.run_macro(mm)); menu.addAction(a)
        for s in self.host.scripts:
            if s.error: self.logBox.append(f"[YÜKLEME HATASI] {Path(s.path).name}: {s.error}")

    # ---- file ops
    def autoload_macros(self):
        self.macro_dirs = [Path(__file__).resolve().parents[2] / "automation", Path.home() / ".kitsune" / "automation"]
        self.reload_macros()

    def reload_macros(self):
        self.host.scripts.clear()
        for d in self.macro_dirs:
            if d.is_dir(): self.host.load_dir(d)
        self.rebuild_auto_menu()

    def load_lua(self):
        p, _ = QFileDialog.getOpenFileName(self, "Lua betiği", "", "Lua (*.lua)")
        if p: self.host.load_script(p); self.rebuild_auto_menu()

    def confirm_discard(self):
        if not self.dirty: return True
        r = QMessageBox.question(self, "Kitsune", "Kaydedilmemiş değişiklikler var. Devam edilsin mi?")
        return r == QMessageBox.Yes

    def set_script(self, sc):
        self.host.script = sc; self.undo.clear(); self.redo.clear(); self.dirty = False
        self.model.refresh(); self.refresh_styles(); self._title()
        if self.events(): self.table.selectRow(0)

    def new(self):
        if self.confirm_discard(): self.set_script(Script())

    def open(self, path=None):
        if not self.confirm_discard(): return
        path = path or QFileDialog.getOpenFileName(self, "Aç", "", "Altyazı (*.ass *.ssa *.srt *.vtt);;Hepsi (*)")[0]
        if path:
            try: self.set_script(srtmod.load_srt(path) if path.lower().endswith((".srt", ".vtt")) else Script.load(path))
            except Exception as ex: QMessageBox.critical(self, "Hata", str(ex))

    def save(self, as_=False):
        p = self.script.filename
        if as_ or not p or p.lower().endswith((".srt", ".vtt")):
            p, _ = QFileDialog.getSaveFileName(self, "Kaydet", p or "", "ASS (*.ass)")
            if not p: return
        self.script.save(p); self.dirty = False; self._title()

    def export_srt(self):
        p, _ = QFileDialog.getSaveFileName(self, "Dışa aktar", "", "SubRip (*.srt);;WebVTT (*.vtt)")
        if p: Path(p).write_text(srtmod.dump_srt(self.script, p.lower().endswith(".vtt")), encoding="utf-8")

    def closeEvent(self, ev):
        ev.accept() if self.confirm_discard() else ev.ignore()

    # ---- selection / editing
    def sel_rows(self): return sorted({i.row() for i in self.table.selectionModel().selectedRows()})
    def cur(self):
        r = self.sel_rows(); return (r[0], self.events()[r[0]]) if r else (None, None)

    def on_select(self, *_):
        r, e = self.cur()
        self._loading = True
        try:
            for w in (self.startEdit, self.endEdit, self.actorEdit, self.textEdit, self.layerSpin, self.styleBox, self.commentChk):
                w.setEnabled(e is not None)
            if e is None: return
            self.startEdit.setText(ms_to_ass(e.start)); self.endEdit.setText(ms_to_ass(e.end))
            self.actorEdit.setText(e.actor); self.layerSpin.setValue(e.layer); self.commentChk.setChecked(e.comment)
            self.styleBox.setCurrentText(e.style); self.textEdit.setPlainText(e.text.replace("\\N", "\n"))
            self.update_info(e)
        finally: self._loading = False

    def update_info(self, e):
        self.info.setText(f"CPS {cps(e.text, e.end - e.start):.1f} · {len(strip_tags(e.text))} kar.")

    def commit_text(self):
        if self._loading: return
        r, e = self.cur()
        if e is None: return
        new = self.textEdit.toPlainText().replace("\n", "\\N")
        if new != e.text:
            self.push_undo(); e.text = new; self.model.row_changed(r); self.update_info(e)

    def commit_fields(self):
        if self._loading: return
        r, e = self.cur()
        if e is None: return
        try: s, en = ass_to_ms(self.startEdit.text()), ass_to_ms(self.endEdit.text())
        except ValueError: return
        vals = (s, en, self.actorEdit.text(), self.layerSpin.value(), self.styleBox.currentText(), self.commentChk.isChecked())
        if vals != (e.start, e.end, e.actor, e.layer, e.style, e.comment) and self.styleBox.currentText():
            self.push_undo(); e.start, e.end, e.actor, e.layer, e.style, e.comment = vals
            self.model.row_changed(r); self.update_info(e)

    def _entry_index(self, row):
        n = -1
        for i, x in enumerate(self.script.entries):
            if x.kind == "event":
                n += 1
                if n == row: return i
        return len(self.script.entries)

    def insert_line(self, after):
        r, e = self.cur(); self.push_undo()
        st = self.script.styles[0].name if self.script.styles else "Default"
        new = Event(style=e.style if e else st, start=e.end if e and after else (e.start if e else 0),
                    end=(e.end + 2000 if e and after else (e.start if e else 2000)))
        row = (r + after) if r is not None else len(self.events())
        self.script.entries.insert(self._entry_index(row), new)
        self.model.refresh(); self.table.selectRow(row); self.textEdit.setFocus()

    def delete_lines(self):
        rows = self.sel_rows()
        if not rows: return
        self.push_undo()
        idx = sorted((self._entry_index(r) for r in rows), reverse=True)
        for i in idx: del self.script.entries[i]
        self.model.refresh()
        if self.events(): self.table.selectRow(min(rows[0], len(self.events()) - 1))

    def duplicate(self):
        r, e = self.cur()
        if e is None: return
        self.push_undo(); self.script.entries.insert(self._entry_index(r + 1), copy.deepcopy(e))
        self.model.refresh(); self.table.selectRow(r + 1)

    def join(self):
        rows = self.sel_rows()
        if len(rows) < 2: return
        self.push_undo(); evs = [self.events()[r] for r in rows]
        evs[0].text = "\\N".join(x.text for x in evs); evs[0].end = max(x.end for x in evs)
        for i in sorted((self._entry_index(r) for r in rows[1:]), reverse=True): del self.script.entries[i]
        self.model.refresh(); self.table.selectRow(rows[0])

    def shift_times(self):
        ms, ok = QInputDialog.getInt(self, "Zamanı kaydır", "Milisaniye (negatif = geri):", 0, -36000000, 36000000)
        if not ok or ms == 0: return
        rows = self.sel_rows() or range(len(self.events())); self.push_undo()
        for r in rows:
            e = self.events()[r]; e.start = max(0, e.start + ms); e.end = max(0, e.end + ms)
        self.model.refresh()

    def find_replace(self):
        d = QDialog(self); d.setWindowTitle("Bul / Değiştir"); f = QFormLayout(d)
        a, b = QLineEdit(), QLineEdit(); rx = QCheckBox("Düzenli ifade"); cs = QCheckBox("Büyük/küçük harf duyarlı")
        f.addRow("Bul", a); f.addRow("Değiştir", b); f.addRow(rx); f.addRow(cs)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel); f.addRow(bb)
        bb.accepted.connect(d.accept); bb.rejected.connect(d.reject)
        if d.exec() != QDialog.Accepted or not a.text(): return
        pat = a.text() if rx.isChecked() else re.escape(a.text())
        try: cre = re.compile(pat, 0 if cs.isChecked() else re.I)
        except re.error as ex: QMessageBox.warning(self, "Hata", str(ex)); return
        self.push_undo(); n = 0
        for e in self.events():
            t, k = cre.subn(b.text(), e.text)
            if k: e.text = t; n += k
        self.model.refresh(); self.statusBar().showMessage(f"{n} değişiklik", 5000)

    # ---- QC
    def run_qc(self):
        self.qc.clear()
        for i in lint(self.script):
            self.qc.addItem(str(i)); self.qc.item(self.qc.count() - 1).setData(Qt.UserRole, i.index)
        self.statusBar().showMessage(f"{self.qc.count()} sorun", 5000)

    def goto_issue(self, it):
        r = it.data(Qt.UserRole)
        if r: self.table.selectRow(r - 1); self.table.scrollTo(self.model.index(r - 1, 0))

    def autofix(self):
        self.push_undo(); n = fix_basic(self.script); self.model.refresh(); self.statusBar().showMessage(f"{n} satır düzeltildi", 5000)

    # ---- styles / props
    def style_manager(self):
        d = QDialog(self); d.setWindowTitle("Stil yöneticisi"); lay = QHBoxLayout(d)
        lst = QListWidget(); lst.addItems([s.name for s in self.script.styles]); lay.addWidget(lst)
        form = QFormLayout(); lay.addLayout(form)
        fn, fs, ol, sh, al = QLineEdit(), QDoubleSpinBox(), QDoubleSpinBox(), QDoubleSpinBox(), QSpinBox()
        fs.setRange(1, 999); ol.setRange(0, 99); sh.setRange(0, 99); al.setRange(1, 9)
        c1, c3 = QLineEdit(), QLineEdit()
        for l, w in (("Yazı tipi", fn), ("Boyut", fs), ("Ana renk", c1), ("Kenar rengi", c3), ("Kenar", ol), ("Gölge", sh), ("Hizalama", al)): form.addRow(l, w)
        def show():
            i = lst.currentRow()
            if i < 0: return
            s = self.script.styles[i]; fn.setText(s.fontname); fs.setValue(float(s.fontsize)); c1.setText(s.color1)
            c3.setText(s.color3); ol.setValue(float(s.outline)); sh.setValue(float(s.shadow)); al.setValue(s.align)
        def apply():
            i = lst.currentRow()
            if i < 0: return
            self.push_undo(); s = self.script.styles[i]
            s.fontname, s.fontsize, s.color1, s.color3, s.outline, s.shadow, s.align = fn.text(), fs.value(), c1.text(), c3.text(), ol.value(), sh.value(), al.value()
        def add():
            n, ok = QInputDialog.getText(d, "Yeni stil", "Ad:")
            if ok and n:
                self.push_undo(); s = copy.deepcopy(self.script.styles[0]) if self.script.styles else Style(); s.name = n
                self.script.entries.append(s); lst.addItem(n); self.refresh_styles()
        lst.currentRowChanged.connect(lambda *_: show())
        ap, ad = QPushButton("Uygula"), QPushButton("Yeni stil"); ap.clicked.connect(apply); ad.clicked.connect(add)
        form.addRow(ap); form.addRow(ad); lst.setCurrentRow(0); d.exec(); self.refresh_styles(); self.on_select()

    def script_props(self):
        d = QDialog(self); d.setWindowTitle("Betik özellikleri"); f = QFormLayout(d)
        t, rx, ry = QLineEdit(self.script.get_info("Title", "")), QSpinBox(), QSpinBox()
        rx.setRange(1, 20000); ry.setRange(1, 20000); rx.setValue(self.script.res[0]); ry.setValue(self.script.res[1])
        f.addRow("Başlık", t); f.addRow("PlayResX", rx); f.addRow("PlayResY", ry)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel); f.addRow(bb)
        bb.accepted.connect(d.accept); bb.rejected.connect(d.reject)
        if d.exec() == QDialog.Accepted:
            self.push_undo(); self.script.set_info("Title", t.text()); self.script.set_info("PlayResX", rx.value()); self.script.set_info("PlayResY", ry.value())

    # ---- automation
    def run_macro(self, m):
        rows = self.sel_rows()
        first_ev = next((i for i, x in enumerate(self.script.entries) if x.kind == "event"), len(self.script.entries))
        # entry-index (1-based) selection as Aegisub does
        sel = [self._entry_index(r) + 1 for r in rows]
        self.push_undo()
        try: out_sel, _ = self.host.run(m, sel, sel[0] if sel else None)
        except Exception as ex:
            self.logBox.append(f"[HATA] {m.name}: {ex}"); self.undo.pop() if self.undo else None
            self.script.entries[:] = self.script.entries; self.model.refresh(); return
        finally:
            for ln in self.host.logs: self.logBox.append(ln)
            self.host.logs.clear()
        self.model.refresh(); self.refresh_styles()
        evrows = []; n = -1
        for i, x in enumerate(self.script.entries, 1):
            if x.kind == "event":
                n += 1
                if i in out_sel: evrows.append(n)
        self.table.clearSelection()
        for r in evrows: self.table.selectRow(r) if r == evrows[0] else self.table.selectionModel().select(self.model.index(r, 0), self.table.selectionModel().Select | self.table.selectionModel().Rows)

    def lua_dialog(self, items, buttons, ids):
        d = QDialog(self); d.setWindowTitle("Otomasyon"); form = QFormLayout(d); widgets = {}
        for it in items:
            c = it.get("class"); name = it.get("name")
            if c == "label": form.addRow(QLabel(str(it.get("label", "")))); continue
            if c in ("edit", "textbox", "color", "coloralpha", "alpha"):
                w = QLineEdit(str(it.get("value", it.get("text", ""))))
            elif c == "intedit":
                w = QSpinBox(); w.setRange(int(it.get("min", -2**31 + 1)), int(it.get("max", 2**31 - 1))); w.setValue(int(it.get("value", 0)))
            elif c == "floatedit":
                w = QDoubleSpinBox(); w.setRange(float(it.get("min", -1e9)), float(it.get("max", 1e9))); w.setDecimals(4); w.setValue(float(it.get("value", 0)))
            elif c == "checkbox":
                w = QCheckBox(str(it.get("label", ""))); w.setChecked(bool(it.get("value", False)))
            elif c == "dropdown":
                w = QComboBox(); its = it.get("items"); vals = [its[i] for i in range(1, len(list(its.keys())) + 1)] if its else []
                w.addItems([str(v) for v in vals]); w.setCurrentText(str(it.get("value", "")))
            else: w = QLineEdit(str(it.get("value", "")))
            if name: widgets[name] = (c, w)
            form.addRow(str(it.get("label", "")) if c != "checkbox" else "", w)
        row = QHBoxLayout(); result = {"b": None}
        for b in buttons:
            btn = QPushButton(str(b)); btn.clicked.connect(lambda _=False, bb=b: (result.__setitem__("b", bb), d.accept())); row.addWidget(btn)
        form.addRow(row); d.exec()
        if result["b"] is None: return False, {}
        out = {}
        for n, (c, w) in widgets.items():
            out[n] = (w.text() if isinstance(w, QLineEdit) else w.isChecked() if isinstance(w, QCheckBox)
                      else w.currentText() if isinstance(w, QComboBox) else w.value())
        return result["b"], out

def run_gui(argv=None):
    app = QApplication.instance() or QApplication(argv or sys.argv)
    app.setApplicationName("Kitsune"); w = MainWindow(); w.show()
    if len(sys.argv) > 2: w.open(sys.argv[2])
    return app.exec()
