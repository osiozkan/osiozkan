"""Yan bölmeler: CPL denetimi, video/ses yer tutucuları, harici araçlar."""

from __future__ import annotations

import bisect
import os
import shlex

from PySide6.QtCore import QProcess, Qt, QTimer
from PySide6.QtGui import QColor, QTextCursor
from PySide6.QtWidgets import (QCheckBox, QDialog, QHBoxLayout, QLabel, QMessageBox,
                               QPlainTextEdit, QPushButton, QTreeWidget, QTreeWidgetItem,
                               QVBoxLayout, QWidget)

from ..core.asstime import format_time
from ..core.textutil import STATUS_ERROR, STATUS_OK, STATUS_WARN, cpl_info
from . import theme


class CplPanel(QWidget):
    """42 karakter sınırını aşan satırların canlı listesi.
    Çift tıkla satıra git; düzenleme kutusunda Shift+Enter ile \\N ekle —
    düzelen satır listeden kendiliğinden düşer."""

    def __init__(self, ctx):
        super().__init__()
        self.ctx = ctx
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["#", "CPL", "Durum", "Metin"])
        self.tree.setRootIsDecorated(False)
        self.tree.setColumnWidth(0, 48)
        self.tree.setColumnWidth(1, 70)
        self.tree.setColumnWidth(2, 60)
        self.tree.itemActivated.connect(self._go)
        self.tree.itemClicked.connect(self._go)
        self.only_err = QCheckBox("Yalnız hatalar")
        self.skip_comments = QCheckBox("Yorumları atla")
        self.skip_comments.setChecked(True)
        self.summary = QLabel("")
        h = QHBoxLayout()
        h.addWidget(self.only_err)
        h.addWidget(self.skip_comments)
        h.addStretch(1)
        h.addWidget(self.summary)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(4, 4, 4, 4)
        lay.addLayout(h)
        lay.addWidget(self.tree)
        self._timer = QTimer(self)
        self._timer.setSingleShot(True)
        self._timer.setInterval(250)
        self._timer.timeout.connect(self.refresh)
        # yazarken yalnız değişen satırlar güncellenir (büyük dosyada tüm listeyi yeniden kurmamak için)
        self._pending: set[int] = set()
        self._inc_timer = QTimer(self)
        self._inc_timer.setSingleShot(True)
        self._inc_timer.setInterval(150)
        self._inc_timer.timeout.connect(self._apply_pending)
        self._rows: list[int] = []            # listede görünen satırlar (sıralı)
        self._status: dict[int, str] = {}      # uyarı/hata durumundaki tüm satırlar (özet için)
        ctx.document_reset.connect(self._timer.start)
        ctx.rows_changed.connect(self._rows_changed)
        self.only_err.toggled.connect(self.refresh)
        self.skip_comments.toggled.connect(self.refresh)
        self.refresh()

    def _rows_changed(self, rows):
        if self._timer.isActive() or len(rows) > 50:
            self._timer.start()
            return
        self._pending.update(rows)
        self._inc_timer.start()

    def _row_state(self, i, ev):
        if ev.comment and self.skip_comments.isChecked():
            return None, None
        info = cpl_info(ev.text)
        st = info["status"]
        return (None, None) if st == STATUS_OK else (st, info)

    def _item(self, i, ev, st, info):
        c = theme.colors()
        it = QTreeWidgetItem([str(i + 1), info["label"], "hata" if st == STATUS_ERROR else "uyarı", ev.text])
        it.setData(0, Qt.UserRole, i)
        col = QColor(c["err_fg"] if st == STATUS_ERROR else c["warn_fg"])
        it.setForeground(1, col)
        it.setForeground(2, col)
        return it

    def _listed(self, st) -> bool:
        return st is not None and not (st == STATUS_WARN and self.only_err.isChecked())

    def _summary(self):
        ne = sum(1 for v in self._status.values() if v == STATUS_ERROR)
        self.summary.setText(f"{ne} hata · {len(self._status) - ne} uyarı")

    def _apply_pending(self):
        evs = self.ctx.doc.events
        for i in sorted(self._pending):
            if i >= len(evs):
                continue
            st, info = self._row_state(i, evs[i])
            if st is None:
                self._status.pop(i, None)
            else:
                self._status[i] = st
            k = bisect.bisect_left(self._rows, i)
            present = k < len(self._rows) and self._rows[k] == i
            if self._listed(st):
                it = self._item(i, evs[i], st, info)
                if present:
                    self.tree.takeTopLevelItem(k)
                else:
                    self._rows.insert(k, i)
                self.tree.insertTopLevelItem(k, it)
            elif present:
                self.tree.takeTopLevelItem(k)
                del self._rows[k]
        self._pending.clear()
        self._summary()

    def refresh(self):
        self._pending.clear()
        self._inc_timer.stop()
        self.tree.clear()
        self._rows = []
        self._status = {}
        items = []
        for i, ev in enumerate(self.ctx.doc.events):
            st, info = self._row_state(i, ev)
            if st is None:
                continue
            self._status[i] = st
            if self._listed(st):
                items.append(self._item(i, ev, st, info))
                self._rows.append(i)
        self.tree.addTopLevelItems(items)
        self._summary()

    def _go(self, item, _col=0):
        row = item.data(0, Qt.UserRole)
        if row is not None:
            self.ctx.set_active(int(row), select_only=True)


class Placeholder(QWidget):
    def __init__(self, title: str, text: str):
        super().__init__()
        lb = QLabel(f"<b>{title}</b><br><br>{text}")
        lb.setAlignment(Qt.AlignCenter)
        lb.setWordWrap(True)
        lb.setStyleSheet(f"color: {theme.colors()['muted']};")
        lay = QVBoxLayout(self)
        lay.addWidget(lb)


# ---------------------------------------------------------------------------
# Harici araçlar (assmotion, hairtrack, bsync ... komut satırı programları)
# ---------------------------------------------------------------------------

def expand_tool_args(args: str, ctx) -> list[str]:
    """Yer tutucular: {ass} {video} {audio} {sel} {active} {start_ms} {end_ms}
    {start_frame} {end_frame} {dir}"""
    ev = ctx.active_event()
    fm = ctx.frame_map
    vals = {
        "ass": ctx.doc.path or "",
        "dir": os.path.dirname(ctx.doc.path) if ctx.doc.path else "",
        "video": ctx.video_path or "",
        "audio": ctx.audio_path or "",
        "sel": ",".join(str(r + 1) for r in ctx.selected_rows()),
        "active": str(ctx.active + 1),
        "start_ms": str(ev.start) if ev else "0",
        "end_ms": str(ev.end) if ev else "0",
        "start_frame": str(fm.frame_from_ms(ev.start)) if (fm and ev) else "",
        "end_frame": str(fm.frame_from_ms(ev.end) - 1) if (fm and ev) else "",
    }
    out = []
    for tok in shlex.split(args, posix=(os.name != "nt")):
        tok = tok.strip('"') if os.name == "nt" else tok
        for k, v in vals.items():
            tok = tok.replace("{" + k + "}", v)
        out.append(tok)
    return out


class ToolRunDialog(QDialog):
    """Harici aracı çalıştırır, çıktısını gösterir; .ass dosyası değiştiyse yeniden yükler."""

    def __init__(self, win, tool: dict):
        super().__init__(win)
        self.win = win
        self.tool = tool
        self.setWindowTitle(tool.get("name", "Araç"))
        self.resize(720, 420)
        self.out = QPlainTextEdit()
        self.out.setReadOnly(True)
        self.btn = QPushButton("Durdur")
        self.btn.clicked.connect(self._btn)
        lay = QVBoxLayout(self)
        lay.addWidget(self.out, 1)
        h = QHBoxLayout()
        h.addStretch(1)
        h.addWidget(self.btn)
        lay.addLayout(h)
        self.proc = QProcess(self)
        self.proc.setProcessChannelMode(QProcess.MergedChannels)
        self.proc.readyReadStandardOutput.connect(self._read)
        self.proc.finished.connect(self._done)
        self.mtime = None

    def start(self):
        ctx = self.win.ctx
        path = ctx.doc.path
        if path:
            self.mtime = os.path.getmtime(path) if os.path.exists(path) else None
        prog = self.tool.get("program", "")
        args = expand_tool_args(self.tool.get("args", ""), ctx)
        wd = self.tool.get("workdir") or (os.path.dirname(path) if path else os.getcwd())
        self.out.appendPlainText(f"$ {prog} {' '.join(args)}\n")
        self.proc.setWorkingDirectory(wd)
        self.proc.start(prog, args)
        if not self.proc.waitForStarted(5000):
            self.out.appendPlainText(f"Başlatılamadı: {self.proc.errorString()}")
            self.btn.setText("Kapat")

    def _read(self):
        data = bytes(self.proc.readAllStandardOutput())
        try:
            txt = data.decode("utf-8")
        except UnicodeDecodeError:
            txt = data.decode("cp1254", errors="replace")
        self.out.moveCursor(QTextCursor.MoveOperation.End)
        self.out.insertPlainText(txt)

    def _done(self, code, _status):
        self.out.appendPlainText(f"\n[çıkış kodu {code}]")
        self.btn.setText("Kapat")
        ctx = self.win.ctx
        p = ctx.doc.path
        if p and self.tool.get("reload", True) and os.path.exists(p) and \
                self.mtime is not None and os.path.getmtime(p) != self.mtime:
            ans = QMessageBox.question(self, "Dosya değişti",
                                       "Araç .ass dosyasını değiştirdi. Yeniden yüklensin mi?\n"
                                       "(Geri alma geçmişi sıfırlanır.)")
            if ans == QMessageBox.Yes:
                self.win.open_path(p, force=True)

    def _btn(self):
        if self.proc.state() != QProcess.NotRunning:
            self.proc.kill()
        else:
            self.accept()
