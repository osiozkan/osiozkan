from __future__ import annotations
import os
import sys

from PySide6.QtCore import QItemSelectionModel, QSettings, Qt, QTimer
from PySide6.QtGui import QAction, QFont, QFontMetricsF, QKeySequence
from PySide6.QtWidgets import (QAbstractItemView, QApplication, QComboBox, QDialog, QDialogButtonBox, QDockWidget,
                               QFileDialog, QFormLayout, QHBoxLayout, QInputDialog, QLabel, QLineEdit, QListWidget,
                               QMainWindow, QMessageBox, QPlainTextEdit, QPushButton, QSpinBox, QSplitter,
                               QTableView, QVBoxLayout, QWidget, QCheckBox, QDoubleSpinBox)

from .. import __version__
from ..core import AssFile, Line, ass_to_ms, ms_to_ass, qc, search, timing
from ..core.formats import load_any, save_any
from ..core.ass import Style
from ..lua import Cancelled, LuaRuntime, LuaScriptError
from .luadialog import show_dialog
from .model import EventModel
from .preview import Preview

DARK = """
QWidget{background:#14181d;color:#d7dde3;font-size:13px}
QTableView{background:#10141a;alternate-background-color:#141a21;gridline-color:#222a33;selection-background-color:#2f5d9e}
QHeaderView::section{background:#1b222b;color:#9fb0c0;border:0;padding:4px}
QLineEdit,QPlainTextEdit,QSpinBox,QDoubleSpinBox,QComboBox{background:#0e1216;border:1px solid #2a343f;padding:3px;border-radius:3px}
QPushButton{background:#25303b;border:1px solid #33414f;padding:4px 10px;border-radius:3px}
QPushButton:hover{background:#2f3d4b}
QMenuBar,QMenu{background:#1b222b}QMenu::item:selected{background:#2f5d9e}
QDockWidget::title{background:#1b222b;padding:4px}
"""


class FindDialog(QDialog):
    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("Bul / Değiştir")
        f = QFormLayout(self)
        self.find = QLineEdit(); self.repl = QLineEdit()
        self.regex = QCheckBox("Düzenli ifade"); self.case = QCheckBox("Büyük/küçük harf duyarlı")
        self.field = QComboBox(); self.field.addItems(["text", "actor", "effect", "style"])
        for lab, w in (("Bul", self.find), ("Değiştir", self.repl), ("Alan", self.field)):
            f.addRow(lab, w)
        f.addRow(self.regex); f.addRow(self.case)
        bb = QDialogButtonBox(); self.b_find = bb.addButton("Hepsini seç", QDialogButtonBox.ActionRole)
        self.b_rep = bb.addButton("Hepsini değiştir", QDialogButtonBox.ActionRole)
        f.addRow(bb)


class StyleDialog(QDialog):
    def __init__(self, parent, doc: AssFile):
        super().__init__(parent)
        self.setWindowTitle("Stil Yöneticisi")
        self.doc = doc
        lay = QHBoxLayout(self)
        self.lst = QListWidget(); lay.addWidget(self.lst)
        right = QFormLayout(); lay.addLayout(right)
        self.f = {
            "name": QLineEdit(), "fontname": QLineEdit(), "fontsize": QDoubleSpinBox(), "primary": QLineEdit(),
            "secondary": QLineEdit(), "outline_c": QLineEdit(), "back": QLineEdit(), "outline": QDoubleSpinBox(),
            "shadow": QDoubleSpinBox(), "align": QSpinBox(), "margin_l": QSpinBox(), "margin_r": QSpinBox(),
            "margin_t": QSpinBox()}
        self.f["fontsize"].setRange(1, 999); self.f["align"].setRange(1, 9)
        for k in ("margin_l", "margin_r", "margin_t"):
            self.f[k].setRange(0, 9999)
        for k in ("outline", "shadow"):
            self.f[k].setRange(0, 99)
        labels = {"name": "Ad", "fontname": "Yazı tipi", "fontsize": "Boyut", "primary": "Ana renk",
                  "secondary": "İkincil", "outline_c": "Kenarlık rengi", "back": "Gölge rengi", "outline": "Kenarlık",
                  "shadow": "Gölge", "align": "Hizalama (numpad)", "margin_l": "Sol", "margin_r": "Sağ", "margin_t": "Dikey"}
        for k, w in self.f.items():
            right.addRow(labels[k], w)
        self.b_new = QPushButton("Yeni"); self.b_del = QPushButton("Sil"); self.b_ok = QPushButton("Kapat")
        row = QHBoxLayout(); [row.addWidget(b) for b in (self.b_new, self.b_del, self.b_ok)]; right.addRow(row)
        self.cur = -1
        self.reload()
        self.lst.currentRowChanged.connect(self.pick)
        self.b_new.clicked.connect(self.new); self.b_del.clicked.connect(self.delete); self.b_ok.clicked.connect(self.accept)
        for w in self.f.values():
            (w.textEdited if isinstance(w, QLineEdit) else w.valueChanged).connect(self.commit)

    def reload(self, row=0):
        self.lst.blockSignals(True); self.lst.clear()
        self.lst.addItems([s.name for s in self.doc.styles]); self.lst.blockSignals(False)
        self.lst.setCurrentRow(min(row, len(self.doc.styles) - 1)); self.pick(self.lst.currentRow())

    def pick(self, r):
        self.cur = r
        if r < 0:
            return
        s = self.doc.styles[r]
        for k, w in self.f.items():
            w.blockSignals(True)
            v = getattr(s, k)
            w.setText(str(v)) if isinstance(w, QLineEdit) else w.setValue(v)
            w.blockSignals(False)

    def commit(self, *_):
        if self.cur < 0:
            return
        s = self.doc.styles[self.cur]
        for k, w in self.f.items():
            v = w.text() if isinstance(w, QLineEdit) else w.value()
            setattr(s, k, v if not isinstance(w, QSpinBox) else int(v))
        self.lst.item(self.cur).setText(s.name)

    def new(self):
        name, ok = QInputDialog.getText(self, "Yeni stil", "Ad:")
        if ok and name:
            base = self.doc.styles[self.cur] if self.cur >= 0 else Style()
            from dataclasses import replace
            self.doc.styles.append(replace(base, name=name)); self.reload(len(self.doc.styles) - 1)

    def delete(self):
        if len(self.doc.styles) > 1 and self.cur >= 0:
            del self.doc.styles[self.cur]; self.reload()


class MainWindow(QMainWindow):
    def __init__(self, path: str | None = None):
        super().__init__()
        self.settings = QSettings("YoruSub", "YoruSub")
        self.path: str | None = None
        self.model = EventModel(AssFile.new())
        self.runtime: LuaRuntime | None = None
        self.fps = 23.976
        self.setAcceptDrops(True)
        self.resize(1280, 800)

        self.table = QTableView(); self.table.setModel(self.model)
        self.table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.ExtendedSelection)
        self.table.setAlternatingRowColors(True); self.table.verticalHeader().hide()
        self.table.horizontalHeader().setStretchLastSection(True)
        for i, w in enumerate((45, 50, 90, 90, 55, 50, 100, 90)):
            self.table.setColumnWidth(i, w)
        self.table.selectionModel().currentRowChanged.connect(lambda cur, _: self.load_editor(cur.row()))
        self.table.setEditTriggers(QAbstractItemView.NoEditTriggers)

        self.preview = Preview()
        ed = QWidget(); el = QVBoxLayout(ed)
        top = QHBoxLayout()
        self.e_start = QLineEdit(); self.e_end = QLineEdit(); self.e_style = QComboBox(); self.e_actor = QLineEdit()
        self.e_layer = QSpinBox(); self.e_comment = QCheckBox("Yorum"); self.e_info = QLabel()
        for lab, w in (("Başlangıç", self.e_start), ("Bitiş", self.e_end), ("Stil", self.e_style), ("Aktör", self.e_actor), ("Katman", self.e_layer)):
            top.addWidget(QLabel(lab)); top.addWidget(w)
        top.addWidget(self.e_comment)
        for w in (self.e_start, self.e_end):
            w.setMinimumWidth(95)
        self.e_actor.setMinimumWidth(80)
        self.e_text = QPlainTextEdit(); self.e_text.setFont(QFont("monospace", 12)); self.e_text.setMaximumHeight(110)
        el.addLayout(top); el.addWidget(self.e_text); el.addWidget(self.e_info)
        self.e_info.setStyleSheet("color:#8da0b3")
        right = QSplitter(Qt.Vertical); right.addWidget(self.preview); right.addWidget(ed); right.setSizes([300, 220])
        split = QSplitter(Qt.Horizontal); split.addWidget(self.table); split.addWidget(right); split.setSizes([640, 640])
        self.setCentralWidget(split)

        self.log = QPlainTextEdit(); self.log.setReadOnly(True)
        dock = QDockWidget("Günlük / Otomasyon", self); dock.setWidget(self.log); self.addDockWidget(Qt.BottomDockWidgetArea, dock)
        self.qc_list = QListWidget(); self.qc_list.itemActivated.connect(self.goto_issue)
        qd = QDockWidget("Kalite Kontrol", self); qd.setWidget(self.qc_list); self.addDockWidget(Qt.RightDockWidgetArea, qd)

        for w, sig in ((self.e_start, "editingFinished"), (self.e_end, "editingFinished"), (self.e_actor, "editingFinished")):
            getattr(w, sig).connect(self.commit_editor)
        self.e_style.activated.connect(lambda _: self.commit_editor())
        self.e_layer.editingFinished.connect(self.commit_editor)
        self.e_comment.clicked.connect(lambda _: self.commit_editor())
        self.e_text.textChanged.connect(self.text_changed)
        self._loading = False

        self.build_menus()
        self.refresh_styles()
        self.autosave = QTimer(self); self.autosave.timeout.connect(self.do_autosave); self.autosave.start(60_000)
        self.update_title()
        if path:
            self.open_file(path)

    # -------------------------------------------------------------- menüler
    def act(self, menu, text, fn, key=None):
        a = QAction(text, self); a.triggered.connect(lambda _=False: fn())
        if key:
            a.setShortcut(QKeySequence(key))
        menu.addAction(a); return a

    def build_menus(self):
        mb = self.menuBar()
        m = mb.addMenu("&Dosya")
        self.act(m, "Yeni", self.new_file, "Ctrl+N"); self.act(m, "Aç…", self.open_dialog, "Ctrl+O")
        self.act(m, "Kaydet", self.save, "Ctrl+S"); self.act(m, "Farklı kaydet…", self.save_as, "Ctrl+Shift+S")
        self.act(m, "Çık", self.close, "Ctrl+Q")
        m = mb.addMenu("&Düzenle")
        self.act(m, "Geri al", lambda: self.model.undo() and self.after_change(), "Ctrl+Z")
        self.act(m, "Yinele", lambda: self.model.redo() and self.after_change(), "Ctrl+Y")
        m.addSeparator()
        self.act(m, "Satır ekle", self.insert_line, "Ctrl+Return"); self.act(m, "Satırı çoğalt", self.duplicate, "Ctrl+D")
        self.act(m, "Satırı sil", self.delete_lines, "Ctrl+Delete"); self.act(m, "Seçilileri birleştir", self.join_lines, "Ctrl+J")
        self.act(m, "İmleçten böl", self.split_line, "Ctrl+B")
        self.act(m, "Bul / Değiştir…", self.find_replace, "Ctrl+F")
        m = mb.addMenu("&Zamanlama")
        self.act(m, "Kaydır…", self.shift_dialog, "Ctrl+T")
        self.act(m, "Çakışmaları düzelt", self.fix_overlaps); self.act(m, "Asgari süreyi zorla (500 ms)", self.min_dur)
        self.act(m, "Kareye oturt", self.snap_frames); self.act(m, "FPS ayarla…", self.set_fps)
        m = mb.addMenu("&Araçlar")
        self.act(m, "Stil yöneticisi…", self.styles_dialog, "Ctrl+Shift+T"); self.act(m, "Kalite kontrol", self.run_qc, "F7")
        self.act(m, "Betik bilgisi (Script Info)…", self.script_info)
        self.auto_menu = mb.addMenu("&Otomasyon")
        self.rebuild_automation()

    def rebuild_automation(self):
        self.auto_menu.clear()
        self.act(self.auto_menu, "Lua betiği yükle…", self.load_lua)
        self.act(self.auto_menu, "Betikleri yeniden yükle", self.reload_scripts, "Ctrl+Shift+R")
        self.auto_menu.addSeparator()
        if self.runtime:
            for mi in self.runtime.macros:
                self.act(self.auto_menu, mi.name, lambda mi=mi: self.run_macro(mi))

    # -------------------------------------------------------------- dosya
    def update_title(self):
        self.setWindowTitle(f"{'*' if self.model.dirty else ''}{os.path.basename(self.path) if self.path else 'Adsız'} — YoruSub {__version__}")

    def new_file(self):
        if not self.confirm_discard():
            return
        self.path = None; self.model.replace_doc(AssFile.new()); self.model.undo_stack.clear(); self.model.dirty = False
        self.refresh_styles(); self.update_title()

    def open_dialog(self):
        p, _ = QFileDialog.getOpenFileName(self, "Aç", "", "Altyazı (*.ass *.ssa *.srt *.vtt);;Hepsi (*)")
        if p:
            self.open_file(p)

    def open_file(self, p):
        if not self.confirm_discard():
            return
        try:
            doc = load_any(p)
        except Exception as e:
            QMessageBox.critical(self, "Hata", str(e)); return
        self.path = p; self.model.replace_doc(doc); self.model.undo_stack.clear(); self.model.redo_stack.clear()
        self.model.dirty = False
        self.refresh_styles(); self.update_title()
        if doc.events:
            self.table.selectRow(0)

    def save(self):
        if not self.path or self.path.lower().endswith((".srt", ".vtt")) and False:
            return self.save_as()
        save_any(self.model.doc, self.path); self.model.dirty = False; self.update_title()

    def save_as(self):
        p, _ = QFileDialog.getSaveFileName(self, "Kaydet", self.path or "", "ASS (*.ass);;SRT (*.srt);;WebVTT (*.vtt)")
        if p:
            self.path = p; self.save()

    def do_autosave(self):
        if self.model.dirty:
            try:
                base = self.path or os.path.join(os.path.expanduser("~"), "adsiz.ass")
                self.model.doc.save(base + ".autosave.ass")
            except OSError:
                pass

    def confirm_discard(self):
        if not self.model.dirty:
            return True
        r = QMessageBox.question(self, "Kaydedilmemiş değişiklikler", "Değişiklikler kaydedilsin mi?",
                                 QMessageBox.Save | QMessageBox.Discard | QMessageBox.Cancel)
        if r == QMessageBox.Save:
            self.save(); return not self.model.dirty
        return r == QMessageBox.Discard

    def closeEvent(self, e):
        e.accept() if self.confirm_discard() else e.ignore()

    def dragEnterEvent(self, e):
        e.acceptProposedAction() if e.mimeData().hasUrls() else None

    def dropEvent(self, e):
        for u in e.mimeData().urls():
            p = u.toLocalFile()
            if p.lower().endswith(".lua"):
                self.load_lua(p)
            else:
                self.open_file(p)

    # -------------------------------------------------------------- düzenleyici
    def sel_rows(self) -> list[int]:
        return sorted({i.row() for i in self.table.selectionModel().selectedRows()})

    def refresh_styles(self):
        self.e_style.clear(); self.e_style.addItems([s.name for s in self.model.doc.styles])

    def load_editor(self, row: int):
        if row < 0 or row >= len(self.model.doc.events):
            return
        e = self.model.doc.events[row]
        self._loading = True
        self.e_start.setText(ms_to_ass(e.start)); self.e_end.setText(ms_to_ass(e.end))
        self.e_style.setCurrentText(e.style); self.e_actor.setText(e.actor); self.e_layer.setValue(e.layer)
        self.e_comment.setChecked(e.comment); self.e_text.setPlainText(e.text.replace("\\N", "\\N\n"))
        self._loading = False
        self.preview.show_line(self.model.doc, e); self.update_info()

    def text_changed(self):
        if self._loading:
            return
        self.commit_editor()

    def current_row(self):
        i = self.table.currentIndex()
        return i.row() if i.isValid() else -1

    def commit_editor(self):
        if self._loading:
            return
        r = self.current_row()
        if r < 0:
            return
        try:
            st, en = ass_to_ms(self.e_start.text()), ass_to_ms(self.e_end.text())
        except ValueError:
            return
        txt = self.e_text.toPlainText().replace("\\N\n", "\\N").replace("\n", "\\N")
        e = self.model.doc.events[r]
        new = dict(start=st, end=en, style=self.e_style.currentText(), actor=self.e_actor.text(),
                   layer=self.e_layer.value(), comment=self.e_comment.isChecked(), text=txt)
        if all(getattr(e, k) == v for k, v in new.items()):
            return
        # aynı satırdaki ardışık yazım tek geri-al adımı olsun
        if getattr(self, "_last_commit_row", None) != r or getattr(self, "_last_commit_field", None) != ("text" if new["text"] != e.text else "meta"):
            self.model.checkpoint()
        self._last_commit_row = r; self._last_commit_field = "text" if new["text"] != e.text else "meta"
        self.model.set_line(r, **new)
        self.preview.show_line(self.model.doc, self.model.doc.events[r]); self.update_info(); self.update_title()

    def update_info(self):
        r = self.current_row()
        if r < 0:
            return
        e = self.model.doc.events[r]
        rows = e.plain.split("\n")
        self.e_info.setText(f"Süre {e.duration / 1000:.2f} s · {timing.cps(e):.1f} cps · "
                            f"karakter {len(e.plain.replace(chr(10), ''))} · satır uzunlukları {[len(x) for x in rows]}")

    def after_change(self):
        self.model.refresh(); self.refresh_styles(); self.update_title()

    # -------------------------------------------------------------- satır işlemleri
    def insert_line(self):
        r = self.current_row(); self.model.checkpoint()
        base = self.model.doc.events[r] if r >= 0 else None
        n = Line(text="", start=base.end if base else 0, end=(base.end + 2000) if base else 2000,
                 style=base.style if base else self.model.doc.styles[0].name)
        self.model.doc.events.insert(r + 1, n); self.after_change(); self.table.selectRow(r + 1)

    def duplicate(self):
        rows = self.sel_rows()
        if not rows:
            return
        self.model.checkpoint()
        for k, r in enumerate(rows):
            self.model.doc.events.insert(rows[-1] + 1 + k, self.model.doc.events[r].clone())
        self.after_change()

    def delete_lines(self):
        rows = self.sel_rows()
        if not rows:
            return
        self.model.checkpoint()
        for r in reversed(rows):
            del self.model.doc.events[r]
        self.after_change()

    def join_lines(self):
        rows = self.sel_rows()
        if len(rows) < 2:
            return
        self.model.checkpoint(); evs = self.model.doc.events
        first = evs[rows[0]]
        first.text = "\\N".join(evs[r].text for r in rows); first.end = max(evs[r].end for r in rows)
        for r in reversed(rows[1:]):
            del evs[r]
        self.after_change()

    def split_line(self):
        r = self.current_row()
        if r < 0:
            return
        pos = self.e_text.textCursor().position()
        raw = self.e_text.toPlainText()
        a, b = raw[:pos].replace("\n", "").strip(), raw[pos:].replace("\n", "").strip()
        e = self.model.doc.events[r]
        self.model.checkpoint()
        total = max(1, len(a) + len(b)); mid = e.start + int(e.duration * len(a) / total)
        n = e.clone(); e.text, e.end = a, mid; n.text, n.start = b, mid
        self.model.doc.events.insert(r + 1, n); self.after_change(); self.table.selectRow(r)

    def find_replace(self):
        d = FindDialog(self)

        def opts():
            return dict(regex=d.regex.isChecked(), case=d.case.isChecked(), field=d.field.currentText())

        def do_find():
            try:
                idx = search.find(self.model.doc, d.find.text(), **opts())
            except Exception as ex:
                QMessageBox.warning(d, "Hata", str(ex)); return
            self.table.clearSelection()
            for i in idx:
                self.table.selectRow(i) if i == idx[0] else self.table.selectionModel().select(
                    self.model.index(i, 0), QItemSelectionModel.Select | QItemSelectionModel.Rows)
            d.setWindowTitle(f"Bul / Değiştir — {len(idx)} eşleşme")

        def do_rep():
            self.model.checkpoint()
            try:
                n = search.replace(self.model.doc, d.find.text(), d.repl.text(), **opts())
            except Exception as ex:
                QMessageBox.warning(d, "Hata", str(ex)); self.model.undo_stack.pop(); return
            self.after_change(); d.setWindowTitle(f"Bul / Değiştir — {n} değişiklik")
        d.b_find.clicked.connect(do_find); d.b_rep.clicked.connect(do_rep)
        d.show()
        self._find = d

    # -------------------------------------------------------------- zamanlama
    def target_events(self):
        rows = self.sel_rows()
        return [self.model.doc.events[r] for r in rows] if rows else self.model.doc.events

    def shift_dialog(self):
        v, ok = QInputDialog.getInt(self, "Kaydır", "Milisaniye (+/−):", 0, -3_600_000, 3_600_000)
        if ok and v:
            self.model.checkpoint(); timing.shift(self.target_events(), v); self.after_change()

    def fix_overlaps(self):
        self.model.checkpoint(); n = timing.fix_overlaps(self.model.doc.events); self.after_change(); self.log.appendPlainText(f"{n} çakışma düzeltildi")

    def min_dur(self):
        self.model.checkpoint(); n = timing.enforce_min_duration(self.target_events(), 500); self.after_change()
        self.log.appendPlainText(f"{n} satır uzatıldı")

    def snap_frames(self):
        self.model.checkpoint(); timing.snap_to_frames(self.target_events(), self.fps); self.after_change()

    def set_fps(self):
        v, ok = QInputDialog.getDouble(self, "FPS", "Kare hızı:", self.fps, 1, 240, 3)
        if ok:
            self.fps = v

    # -------------------------------------------------------------- araçlar
    def styles_dialog(self):
        self.model.checkpoint()
        StyleDialog(self, self.model.doc).exec(); self.after_change()

    def script_info(self):
        d = self.model.doc
        w, ok = QInputDialog.getText(self, "Çözünürlük", "PlayResX x PlayResY:", text="%dx%d" % d.res)
        if ok and "x" in w:
            try:
                x, y = (int(t) for t in w.lower().split("x"))
            except ValueError:
                return
            self.model.checkpoint(); d.set_info("PlayResX", x); d.set_info("PlayResY", y); self.update_title()

    def run_qc(self):
        issues = qc.check(self.model.doc)
        self.model.issues = {}
        self.qc_list.clear()
        for i in issues:
            self.model.issues[i.index] = self.model.issues.get(i.index, "") + i.message + "; "
            it = self.qc_list.addItem(f"#{i.index + 1} [{i.kind}] {i.message}")
        self.model.refresh()
        self.log.appendPlainText(f"Kalite kontrol: {len(issues)} sorun")

    def goto_issue(self, item):
        try:
            n = int(item.text().split()[0][1:]) - 1
            self.table.selectRow(n); self.table.scrollTo(self.model.index(n, 0))
        except ValueError:
            pass

    # -------------------------------------------------------------- Lua
    def make_runtime(self):
        def extents(st, text):
            f = QFont(st.get("fontname", "Arial")); f.setPixelSize(int(st.get("fontsize", 40)))
            f.setBold(bool(st.get("bold"))); f.setItalic(bool(st.get("italic")))
            fm = QFontMetricsF(f)
            sx = float(st.get("scale_x", 100)) / 100
            return (fm.horizontalAdvance(text) * sx + float(st.get("spacing", 0)) * len(text), fm.height(), fm.descent(), fm.leading())
        w, h = self.model.doc.res
        return LuaRuntime(log=lambda s: self.log.appendPlainText(s), dialog=lambda items, btns: show_dialog(self, items, btns),
                          text_extents=extents, fps=self.fps, video_size=(w, h))

    def load_lua(self, path=None):
        if not path:
            path, _ = QFileDialog.getOpenFileName(self, "Lua betiği", "", "Lua (*.lua)")
        if not path:
            return
        self.runtime = self.runtime or self.make_runtime()
        self.scripts = getattr(self, "scripts", [])
        if path not in self.scripts:
            self.scripts.append(path)
        try:
            ms = self.runtime.load_script(path)
            self.log.appendPlainText(f"{os.path.basename(path)}: {len(ms)} makro yüklendi")
        except LuaScriptError as e:
            self.log.appendPlainText(f"HATA: {e}")
        self.rebuild_automation()

    def reload_scripts(self):
        paths = list(getattr(self, "scripts", []))
        self.runtime = None; self.scripts = []
        for p in paths:
            self.load_lua(p)

    def run_macro(self, mi):
        rows = self.sel_rows()
        self.runtime.fps = self.fps
        try:
            out, sel, _ = self.runtime.run(mi, self.model.doc, rows, self.current_row() if self.current_row() >= 0 else None)
        except Cancelled:
            self.log.appendPlainText("İptal edildi"); return
        except LuaScriptError as e:
            self.log.appendPlainText(f"HATA ({mi.name}): {e}"); return
        self.model.checkpoint(); self.model.replace_doc(out); self.refresh_styles(); self.update_title()
        if sel:
            self.table.clearSelection()
            for i in sel:
                if 0 <= i < len(out.events):
                    self.table.selectionModel().select(self.model.index(i, 0), QItemSelectionModel.Select | QItemSelectionModel.Rows)
        self.log.appendPlainText(f"'{mi.name}' tamamlandı")


def run(path=None) -> int:
    app = QApplication.instance() or QApplication(sys.argv)
    app.setStyleSheet(DARK)
    w = MainWindow(path)
    w.show()
    return app.exec()
