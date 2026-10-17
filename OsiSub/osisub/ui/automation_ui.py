"""Automation'ın arayüz tarafı.

* GuiHost     : Lua'dan gelen çağrıları (diyalog, ilerleme, pano...) Qt'ye taşır.
                Makro ayrı bir iş parçacığında çalışır; arayüz gerektiren her
                çağrı ana iş parçacığına ENGELLEYİCİ olarak aktarılır.
* LuaDialog   : aegisub.dialog.display tablosunu Qt formuna çevirir.
* ProgressDlg : ilerleme çubuğu + görev metni + debug.out günlüğü + iptal.
* run_macro   : çalışma kopyası üzerinde makroyu koşturur, başarıda belgeye işler.
* ScriptManagerDialog : yüklü betikler, hatalar, yeniden yükleme.
"""

from __future__ import annotations

import os
import re
import threading
import traceback

from PySide6.QtCore import QCoreApplication, QObject, Qt, QThread, QTimer, Signal
from PySide6.QtGui import QColor, QTextCursor
from PySide6.QtWidgets import (QApplication, QCheckBox, QColorDialog, QComboBox, QDialog,
                               QDialogButtonBox, QDoubleSpinBox, QFileDialog, QGridLayout,
                               QHBoxLayout, QHeaderView, QLabel, QLineEdit, QMessageBox,
                               QPlainTextEdit, QProgressBar, QPushButton, QSpinBox,
                               QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget)

from ..automation.host import Host
from ..automation.runtime import LUA_FLAVOR, Macro, ScriptManager


# ---------------------------------------------------------------------------
# Ana iş parçacığına çağrı aktarma
# ---------------------------------------------------------------------------

class _Invoker(QObject):
    sig = Signal(object)

    def __init__(self):
        super().__init__()
        self.sig.connect(self._run, Qt.BlockingQueuedConnection)

    def _run(self, box):
        try:
            box["result"] = box["fn"]()
        except BaseException as e:  # noqa: BLE001
            box["exc"] = e


_invoker: _Invoker | None = None


def call_in_gui(fn):
    """fn'i ana iş parçacığında çalıştırıp sonucunu döndürür."""
    app = QCoreApplication.instance()
    if app is None or QThread.currentThread() is app.thread():
        return fn()
    box = {"fn": fn}
    _invoker.sig.emit(box)
    if "exc" in box:
        raise box["exc"]
    return box.get("result")


def init_invoker():
    global _invoker
    if _invoker is None:
        _invoker = _Invoker()


# ---------------------------------------------------------------------------
# Renk dönüşümleri (Aegisub dialog değerleri)
# ---------------------------------------------------------------------------

def parse_any_color(s: str) -> tuple[int, int, int, int]:
    """'#RRGGBB[AA]', '&HBBGGRR&', '&HAABBGGRR' -> (r, g, b, a)"""
    s = (s or "").strip()
    m = re.match(r"^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})?$", s)
    if m:
        return int(m[1], 16), int(m[2], 16), int(m[3], 16), int(m[4] or "00", 16)
    m = re.match(r"^&H([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})&?$", s)
    if m:
        return int(m[4], 16), int(m[3], 16), int(m[2], 16), int(m[1], 16)
    m = re.match(r"^&H([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})&?$", s)
    if m:
        return int(m[3], 16), int(m[2], 16), int(m[1], 16), 0
    m = re.match(r"^&H([0-9a-fA-F]{2})&?$", s)
    if m:
        return 0, 0, 0, int(m[1], 16)
    return 0, 0, 0, 0


class ColorButton(QPushButton):
    def __init__(self, value: str, with_alpha: bool = False, alpha_only: bool = False):
        super().__init__()
        self.with_alpha = with_alpha
        self.alpha_only = alpha_only
        self.rgba = parse_any_color(value)
        self.clicked.connect(self._pick)
        self._refresh()

    def _refresh(self):
        r, g, b, a = self.rgba
        self.setText(self.value())
        fg = "#000" if (r * 299 + g * 587 + b * 114) / 1000 > 128 else "#fff"
        self.setStyleSheet(f"background: rgb({r},{g},{b}); color: {fg};")

    def _pick(self):
        r, g, b, a = self.rgba
        opts = QColorDialog.ShowAlphaChannel if (self.with_alpha or self.alpha_only) else QColorDialog.ColorDialogOption(0)
        # ASS'te alfa ters: 00 = opak; Qt'de 255 = opak
        c = QColorDialog.getColor(QColor(r, g, b, 255 - a), self, "Renk seç", opts)
        if c.isValid():
            self.rgba = (c.red(), c.green(), c.blue(), 255 - c.alpha())
            self._refresh()

    def value(self) -> str:
        r, g, b, a = self.rgba
        if self.alpha_only:
            return f"&H{a:02X}&"
        if self.with_alpha:
            return f"#{r:02X}{g:02X}{b:02X}{a:02X}"
        return f"#{r:02X}{g:02X}{b:02X}"


# ---------------------------------------------------------------------------
# aegisub.dialog.display
# ---------------------------------------------------------------------------

class LuaDialog(QDialog):
    def __init__(self, controls: list[dict], buttons, button_ids, title: str, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title or "Automation")
        self.pressed = False
        self.widgets: dict[str, tuple[str, QWidget]] = {}
        grid = QGridLayout()
        grid.setHorizontalSpacing(8)
        grid.setVerticalSpacing(6)
        for c in controls:
            cls = str(c.get("class", "")).lower()
            x, y = int(c.get("x", 0) or 0), int(c.get("y", 0) or 0)
            w, h = int(c.get("width", 1) or 1), int(c.get("height", 1) or 1)
            wdg = self._make(cls, c)
            if wdg is None:
                continue
            if c.get("hint"):
                wdg.setToolTip(str(c["hint"]))
            grid.addWidget(wdg, y, x, max(1, h), max(1, w))
            name = c.get("name")
            if name and cls != "label":
                self.widgets[str(name)] = (cls, wdg)
        lay = QVBoxLayout(self)
        lay.addLayout(grid)
        bb = QHBoxLayout()
        bb.addStretch(1)
        self.button_ids = button_ids or {}
        cancel_label = self.button_ids.get("cancel")
        ok_label = self.button_ids.get("ok")
        if not buttons:
            buttons = None
        self.custom = buttons is not None
        labels = buttons if buttons else ["Tamam", "İptal"]
        for i, label in enumerate(labels):
            b = QPushButton(str(label))
            if (ok_label and label == ok_label) or (not ok_label and i == 0):
                b.setDefault(True)
            b.clicked.connect(lambda _=False, lb=label, idx=i: self._press(lb, idx))
            bb.addWidget(b)
        self._cancel_label = cancel_label
        lay.addLayout(bb)

    def _press(self, label, idx):
        if not self.custom:
            self.pressed = idx == 0
        elif self._cancel_label is not None and label == self._cancel_label:
            self.pressed = False
        else:
            self.pressed = label
        self.accept()

    def reject(self):
        self.pressed = False
        super().reject()

    def _make(self, cls, c):
        if cls == "label":
            lb = QLabel(str(c.get("label", "")))
            lb.setTextFormat(Qt.PlainText)
            return lb
        if cls == "edit":
            w = QLineEdit(str(c.get("text", c.get("value", "")) or ""))
            return w
        if cls == "textbox":
            w = QPlainTextEdit(str(c.get("text", c.get("value", "")) or ""))
            w.setMinimumHeight(60)
            return w
        if cls == "intedit":
            w = QSpinBox()
            lo, hi = c.get("min"), c.get("max")
            w.setRange(int(lo) if lo is not None else -2_000_000_000, int(hi) if hi is not None else 2_000_000_000)
            w.setValue(int(c.get("value", 0) or 0))
            return w
        if cls == "floatedit":
            w = QDoubleSpinBox()
            w.setDecimals(4)
            lo, hi = c.get("min"), c.get("max")
            w.setRange(float(lo) if lo is not None else -1e12, float(hi) if hi is not None else 1e12)
            if c.get("step"):
                w.setSingleStep(float(c["step"]))
            w.setValue(float(c.get("value", 0) or 0))
            return w
        if cls == "checkbox":
            w = QCheckBox(str(c.get("label", "")))
            w.setChecked(bool(c.get("value", False)))
            return w
        if cls == "dropdown":
            w = QComboBox()
            items = c.get("items") or []
            if isinstance(items, dict):
                items = [items[k] for k in sorted(items)]
            w.addItems([str(i) for i in items])
            v = str(c.get("value", "") or "")
            i = w.findText(v)
            if i >= 0:
                w.setCurrentIndex(i)
            return w
        if cls == "color":
            return ColorButton(str(c.get("value", "#FFFFFF") or "#FFFFFF"))
        if cls == "coloralpha":
            return ColorButton(str(c.get("value", "#FFFFFF00") or "#FFFFFF00"), with_alpha=True)
        if cls == "alpha":
            return ColorButton(str(c.get("value", "&H00&") or "&H00&"), alpha_only=True)
        lb = QLabel(f"[desteklenmeyen denetim: {cls}]")
        return lb

    def values(self) -> dict:
        out = {}
        for name, (cls, w) in self.widgets.items():
            if cls == "edit":
                out[name] = w.text()
            elif cls == "textbox":
                out[name] = w.toPlainText()
            elif cls == "intedit":
                out[name] = int(w.value())
            elif cls == "floatedit":
                out[name] = float(w.value())
            elif cls == "checkbox":
                out[name] = bool(w.isChecked())
            elif cls == "dropdown":
                out[name] = w.currentText()
            elif cls in ("color", "coloralpha", "alpha"):
                out[name] = w.value()
        return out


# ---------------------------------------------------------------------------
# İlerleme penceresi
# ---------------------------------------------------------------------------

class ProgressDlg(QDialog):
    def __init__(self, title: str, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setModal(True)
        self.resize(560, 300)
        self.task = QLabel("Çalışıyor…")
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        self.log.setVisible(False)
        self.btn = QPushButton("İptal")
        self.cancel_count = 0
        lay = QVBoxLayout(self)
        lay.addWidget(self.task)
        lay.addWidget(self.bar)
        lay.addWidget(self.log, 1)
        h = QHBoxLayout()
        h.addStretch(1)
        h.addWidget(self.btn)
        lay.addLayout(h)
        self.finished_run = False
        self.btn.clicked.connect(self._btn)

    def _btn(self):
        if self.finished_run:
            self.accept()
            return
        self.cancel_count += 1
        self.task.setText("İptal isteniyor…" if self.cancel_count == 1 else "Zorla durduruluyor…")

    def closeEvent(self, e):
        if not self.finished_run:
            self._btn()
            e.ignore()
            return
        super().closeEvent(e)

    def append_log(self, msg: str):
        self.log.setVisible(True)
        self.log.moveCursor(QTextCursor.MoveOperation.End)
        self.log.insertPlainText(msg if msg.endswith("\n") else msg + "\n")
        self.log.ensureCursorVisible()

    def done_running(self, keep_open: bool):
        self.finished_run = True
        self.btn.setText("Kapat")
        self.bar.setValue(1000)
        if not keep_open:
            self.hide()


# ---------------------------------------------------------------------------
# GuiHost
# ---------------------------------------------------------------------------

class GuiHost(Host, QObject):
    progress_sig = Signal(float)
    task_sig = Signal(str)
    title_sig = Signal(str)
    log_sig = Signal(int, str)

    def __init__(self, ctx, main_window):
        Host.__init__(self)
        QObject.__init__(self)
        self.ctx = ctx
        self.win = main_window
        self.progress: ProgressDlg | None = None
        self.macro_title = ""
        self.progress_sig.connect(self._on_progress)
        self.task_sig.connect(self._on_task)
        self.title_sig.connect(self._on_title)
        self.log_sig.connect(self._on_log)
        self.logged_visible = 0

    # --- bağlamdan beslenenler
    @property
    def frame_map(self):
        return self.ctx.frame_map

    @frame_map.setter
    def frame_map(self, v):
        pass

    @property
    def video_path(self):
        return self.ctx.video_path

    @video_path.setter
    def video_path(self, v):
        pass

    @property
    def audio_path(self):
        return self.ctx.audio_path

    @audio_path.setter
    def audio_path(self, v):
        pass

    # --- ilerleme (kuyruklu, engellemez)
    def progress_set(self, pct):
        self.progress_sig.emit(float(pct))

    def progress_task(self, msg):
        self.task_sig.emit(msg)

    def progress_title(self, msg):
        self.title_sig.emit(msg)

    def log(self, level, msg):
        super().log(level, msg)
        if level <= int(self.ctx.config["log_level"]):
            self.logged_visible += 1
        self.log_sig.emit(level, msg)

    def is_cancelled(self):
        return bool(self.progress and self.progress.cancel_count > 0)

    def _on_progress(self, pct):
        if self.progress:
            self.progress.bar.setValue(int(max(0.0, min(100.0, pct)) * 10))

    def _on_task(self, msg):
        if self.progress:
            self.progress.task.setText(msg)

    def _on_title(self, msg):
        if self.progress:
            self.progress.setWindowTitle(msg)

    def _on_log(self, level, msg):
        if self.progress and level <= int(self.ctx.config["log_level"]):
            self.progress.append_log(msg)

    # --- diyaloglar (engelleyici)
    def dialog_display(self, controls, buttons, button_ids):
        if self.dialog_answers:          # testler / makro kaydı için hazır cevap
            return Host.dialog_display(self, controls, buttons, button_ids)

        def show():
            dlg = LuaDialog(controls, buttons, button_ids, self.macro_title,
                            self.progress if self.progress and self.progress.isVisible() else self.win)
            dlg.exec()
            vals = dlg.values()
            return dlg.pressed, vals
        return call_in_gui(show)

    @staticmethod
    def _wild(w):
        # Aegisub: "Açıklama (*.ass)|*.ass|Tümü|*.*" -> Qt süzgeci
        if not w:
            return ""
        parts = w.split("|")
        out = []
        for i in range(0, len(parts) - 1, 2):
            pats = parts[i + 1].replace(";", " ")
            out.append(f"{parts[i]} ({pats})" if "(" not in parts[i] else parts[i])
        return ";;".join(out)

    def dialog_open(self, title, default_file, default_dir, wildcards, multiple, must_exist):
        def show():
            start = os.path.join(default_dir, default_file) if default_dir else default_file
            if multiple:
                files, _ = QFileDialog.getOpenFileNames(self.win, title, start, self._wild(wildcards))
                return files or None
            f, _ = QFileDialog.getOpenFileName(self.win, title, start, self._wild(wildcards))
            return f or None
        return call_in_gui(show)

    def dialog_save(self, title, default_file, default_dir, wildcards, no_prompt):
        def show():
            start = os.path.join(default_dir, default_file) if default_dir else default_file
            opts = QFileDialog.DontConfirmOverwrite if no_prompt else QFileDialog.Option(0)
            f, _ = QFileDialog.getSaveFileName(self.win, title, start, self._wild(wildcards), options=opts)
            return f or None
        return call_in_gui(show)

    def clipboard_get(self):
        return call_in_gui(lambda: QApplication.clipboard().text())

    def clipboard_set(self, s):
        call_in_gui(lambda: QApplication.clipboard().setText(s))
        return True

    def video_size(self):
        return self.ctx.video_dims

    def audio_selection(self):
        m = self.ctx.media
        ev = self.ctx.active_event()
        if m is None or m.audio is None or ev is None:
            return None
        return ev.start, ev.end

    def get_frame(self, frame, with_subs):
        m = self.ctx.media
        if m is None or m.index is None:
            return None
        video = getattr(self.win, "video", None)
        if with_subs and video is not None and video.mpv is not None:
            def grab():
                cur = m.frame
                video.seek_frame(frame)
                import time
                for _ in range(100):           # mpv aramayı bitirsin
                    QApplication.processEvents()
                    tp = video.mpv.time_pos
                    if tp is not None and m.index.frame_at_seconds(tp) == frame:
                        break
                    time.sleep(0.01)
                img = video.grab_frame(True)
                video.seek_frame(cur)
                return img
            return call_in_gui(grab)
        try:
            return m.grabber.get(frame) if m.grabber is not None else None
        except Exception:  # noqa: BLE001
            return None

    def gui_get_cursor(self):
        return call_in_gui(lambda: self.win.editbox.cursor_pos())

    def gui_set_cursor(self, pos):
        call_in_gui(lambda: self.win.editbox.set_cursor_pos(pos))

    def gui_is_modified(self):
        return self.ctx.undo.modified


# ---------------------------------------------------------------------------
# Makro çalıştırma
# ---------------------------------------------------------------------------

def run_macro_gui(win, macro: Macro):
    """Makroyu çalışma kopyasında, ayrı iş parçacığında koşturur.

    Ana iş parçacığı kendi olay döngüsünü döndürmeye devam eder; böylece
    betikten gelen diyalog/ilerleme çağrıları anında karşılanır ve arayüz
    donmaz. Pencere kipli olduğu için kullanıcı bu sırada belgeyi değiştiremez."""
    from PySide6.QtCore import QEventLoop
    ctx = win.ctx
    host: GuiHost = win.gui_host
    script = macro.script
    script.host = host
    host.macro_title = macro.name.split("/")[-1]
    host.logged_visible = 0
    work = ctx.doc.clone()
    sel = ctx.selected_rows()
    active = ctx.active
    dlg = ProgressDlg(host.macro_title, win)
    host.progress = dlg
    result = {}

    def worker():
        try:
            result["r"] = script.run_macro(macro, work, sel, active)
        except BaseException:  # noqa: BLE001
            result["exc"] = traceback.format_exc()

    th = threading.Thread(target=worker, name="osisub-macro", daemon=True)
    th.start()
    elapsed = 0.0
    while th.is_alive():
        QApplication.processEvents(QEventLoop.AllEvents, 20)
        th.join(0.005)
        elapsed += 0.025
        if elapsed > 0.35 and not dlg.isVisible():
            dlg.show()          # kısa makrolarda pencere hiç görünmez
        if dlg.cancel_count >= 2:
            script.hard_cancel = True
    QApplication.processEvents()

    r = result.get("r")
    keep = False
    if "exc" in result:
        dlg.task.setText("Beklenmeyen hata")
        dlg.append_log(result["exc"])
        keep = True
    elif r.status == "error":
        dlg.task.setText("Betik hatası — belge değişmedi")
        dlg.append_log(r.error)
        keep = True
    elif r.status == "cancel":
        dlg.task.setText("İptal edildi — belge değişmedi")
        ctx.status.emit(f"{host.macro_title}: iptal edildi")
        keep = host.logged_visible > 0
    else:
        if r.changed:
            ctx.doc.adopt(work)
            ctx.commit_external(host.macro_title, selection=r.selection, active=r.active)
        elif r.selection is not None:
            ctx.set_selection(r.selection, r.active)
        ctx.status.emit(f"{host.macro_title}: tamam ({r.seconds * 1000:.0f} ms)")
        dlg.task.setText("Bitti")
        keep = host.logged_visible > 0
    dlg.done_running(False)
    host.progress = None
    if keep:
        dlg.finished_run = True
        dlg.btn.setText("Kapat")
        if getattr(win, "headless", False):
            win.last_macro_log = dlg.log.toPlainText()
        else:
            dlg.exec()
    return r


def validate_macro_gui(win, macro: Macro) -> tuple[bool, str | None]:
    ctx = win.ctx
    try:
        macro.script.host = win.gui_host
        return macro.script.validate(macro, ctx.doc, ctx.selected_rows(), ctx.active)
    except Exception as e:  # noqa: BLE001
        return False, str(e)


# ---------------------------------------------------------------------------
# Betik yöneticisi
# ---------------------------------------------------------------------------

class ScriptManagerDialog(QDialog):
    def __init__(self, win, manager: ScriptManager):
        super().__init__(win)
        self.win = win
        self.mgr = manager
        self.setWindowTitle("Automation betikleri")
        self.resize(900, 480)
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Betik", "Durum", "Sürüm", "Yazar", "Makrolar", "Dosya"])
        self.tree.header().setSectionResizeMode(QHeaderView.Interactive)
        self.tree.setRootIsDecorated(False)
        self.tree.setAlternatingRowColors(True)
        self.detail = QPlainTextEdit()
        self.detail.setReadOnly(True)
        self.detail.setMaximumHeight(140)
        self.tree.currentItemChanged.connect(self._show)
        b_add = QPushButton("Betik ekle…")
        b_reload = QPushButton("Seçiliyi yeniden yükle")
        b_reload_all = QPushButton("Tümünü yeniden yükle")
        b_dirs = QPushButton("Klasörler")
        b_close = QPushButton("Kapat")
        b_add.clicked.connect(self._add)
        b_reload.clicked.connect(self._reload)
        b_reload_all.clicked.connect(self._reload_all)
        b_dirs.clicked.connect(self._dirs)
        b_close.clicked.connect(self.accept)
        h = QHBoxLayout()
        for b in (b_add, b_reload, b_reload_all, b_dirs):
            h.addWidget(b)
        h.addStretch(1)
        h.addWidget(b_close)
        info = QLabel(f"Lua motoru: {LUA_FLAVOR}")
        lay = QVBoxLayout(self)
        lay.addWidget(info)
        lay.addWidget(self.tree, 1)
        lay.addWidget(self.detail)
        lay.addLayout(h)
        self._fill()

    def _fill(self):
        self.tree.clear()
        for sc in self.mgr.scripts:
            it = QTreeWidgetItem([sc.name, "HATA" if sc.error else "Yüklü", sc.version, sc.author,
                                  str(len(sc.macros)), sc.path])
            if sc.error:
                it.setForeground(1, QColor("#ff8080"))
            it.setData(0, Qt.UserRole, sc)
            self.tree.addTopLevelItem(it)
        for i in range(5):
            self.tree.resizeColumnToContents(i)

    def _show(self, cur, _prev):
        if not cur:
            return
        sc = cur.data(0, Qt.UserRole)
        lines = [sc.description or ""]
        if sc.error:
            lines.append("Hata:\n" + sc.error)
        for m in sc.macros:
            lines.append(f"• {m.name} — {m.description}")
        for f in sc.filters:
            lines.append(f"◦ süzgeç: {f.name} — {f.description}")
        self.detail.setPlainText("\n".join(lines))

    def _add(self):
        files, _ = QFileDialog.getOpenFileNames(self, "Betik ekle", "", "Lua betikleri (*.lua)")
        for f in files:
            self.mgr.add(f)
        self._fill()
        self.win.rebuild_automation_menu()

    def _reload(self):
        cur = self.tree.currentItem()
        if cur:
            cur.data(0, Qt.UserRole).reload()
            self._fill()
            self.win.rebuild_automation_menu()

    def _reload_all(self):
        self.win.load_scripts()
        self._fill()

    def _dirs(self):
        txt = "Otomatik yükleme klasörleri:\n" + "\n".join(self.mgr.autoload_dirs) + \
              "\n\nInclude klasörleri:\n" + "\n".join(self.mgr.include_dirs)
        QMessageBox.information(self, "Klasörler", txt)
