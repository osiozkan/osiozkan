"""Konuşmaya göre senkron penceresi (bsync).

Hesapla -> parça tablosu (satır sayısı, zaman, önerilen kayma, güven, not). İşaretli parçalar
Uygula ile tek geri al adımında kaydırılır. Güveni düşük parçalar kırmızı ve varsayılan olarak işaretsiz.
"""

from __future__ import annotations

import threading

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QDialogButtonBox, QFormLayout,
                               QHBoxLayout, QHeaderView, QLabel, QLineEdit, QProgressBar, QPushButton,
                               QSpinBox, QTableWidget, QTableWidgetItem, QVBoxLayout)

from ..core import sync as S
from ..core.asstime import format_time

LOW_CONF = 0.03


class SyncDialog(QDialog):
    def __init__(self, ctx, media, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.media = media
        self.setWindowTitle("Konuşmaya göre senkron (bsync)")
        self.resize(820, 560)
        cfg = dict(ctx.config["sync"] or {})
        self.scope = QComboBox()
        self.scope.addItems(["Tüm satırlar", "Seçili satırlar"])
        self.fine = QSpinBox()
        self.fine.setRange(500, 30000)
        self.fine.setSingleStep(500)
        self.fine.setSuffix(" ms")
        self.fine.setValue(int(cfg.get("fine", 8000)))
        self.coarse = QSpinBox()
        self.coarse.setRange(0, 600000)
        self.coarse.setSingleStep(5000)
        self.coarse.setSuffix(" ms")
        self.coarse.setValue(int(cfg.get("coarse", 60000)))
        self.gap = QSpinBox()
        self.gap.setRange(1000, 60000)
        self.gap.setSingleStep(500)
        self.gap.setSuffix(" ms")
        self.gap.setValue(int(cfg.get("gap", 4000)))
        self.gap.setToolTip("Kayma yalnız altyazıda bu kadar boşluk olan yerlerde (sahne aralarında) değişebilir")
        self.oped = QLineEdit(cfg.get("oped", S.SyncOptions.oped_regex))
        self.oped.setToolTip("Stil/aktör/efekt adı buna uyan satırların aralığındaki ses bastırılır ve "
                             "bu satırlar çapa olmaz")
        self.song = QSpinBox()
        self.song.setRange(0, 600000)
        self.song.setSingleStep(5000)
        self.song.setSuffix(" ms")
        self.song.setValue(int(cfg.get("song", 45000)))
        self.song.setToolTip("Bundan uzun kesintisiz ses şarkı sayılıp bastırılır (0 = kapalı)")
        form = QFormLayout()
        form.addRow("Kapsam", self.scope)
        form.addRow("İnce arama (± parça başına)", self.fine)
        form.addRow("Kaba arama (± blok başına)", self.coarse)
        form.addRow("Sahne arası (en az boşluk)", self.gap)
        form.addRow("OP/ED stil deseni", self.oped)
        form.addRow("Şarkı sayılacak kesintisiz ses", self.song)
        self.b_calc = QPushButton("Hesapla")
        self.b_calc.clicked.connect(self.calculate)
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.table = QTableWidget(0, 6)
        self.table.setHorizontalHeaderLabels(["", "Satırlar", "Başlangıç", "Kayma", "Güven", "Not"])
        self.table.horizontalHeader().setSectionResizeMode(5, QHeaderView.Stretch)
        self.table.verticalHeader().setVisible(False)
        self.table.cellDoubleClicked.connect(self._goto)
        self.info = QLabel("")
        self.info.setWordWrap(True)
        self.bb = QDialogButtonBox(QDialogButtonBox.Apply | QDialogButtonBox.Close)
        self.bb.button(QDialogButtonBox.Apply).setText("Uygula")
        self.bb.button(QDialogButtonBox.Apply).setEnabled(False)
        self.bb.button(QDialogButtonBox.Apply).clicked.connect(self.apply)
        self.bb.rejected.connect(self.reject)
        top = QHBoxLayout()
        top.addLayout(form, 1)
        right = QVBoxLayout()
        right.addWidget(self.b_calc)
        right.addStretch(1)
        top.addLayout(right)
        lay = QVBoxLayout(self)
        lay.addLayout(top)
        lay.addWidget(self.bar)
        lay.addWidget(self.table, 1)
        lay.addWidget(self.info)
        lay.addWidget(self.bb)
        self.segs: list[S.Segment] = []
        self.removed = []
        self.error = None
        self.busy = False
        self._snap = None
        if self.ctx.selected_rows() and len(self.ctx.selected_rows()) > 1:
            self.scope.setCurrentIndex(int(cfg.get("scope", 0)) % 2)

    def options(self) -> S.SyncOptions:
        rows = self.ctx.selected_rows() if self.scope.currentIndex() == 1 else None
        return S.SyncOptions(coarse_range_ms=self.coarse.value(), fine_range_ms=self.fine.value(),
                             scene_gap_ms=self.gap.value(), oped_regex=self.oped.text() or "$^",
                             oped_max_ms=self.song.value(), rows=rows)

    def _save_cfg(self):
        self.ctx.config["sync"] = {"fine": self.fine.value(), "coarse": self.coarse.value(),
                                   "gap": self.gap.value(), "oped": self.oped.text(), "song": self.song.value(),
                                   "scope": self.scope.currentIndex()}
        self.ctx.config.save()

    def calculate(self):
        ad = self.media.audio
        if ad is None:
            self.info.setText("Önce ses açın (Ses > Ses aç ya da videodan)")
            return False
        try:
            import re
            re.compile(self.oped.text() or "$^")
        except re.error as e:
            self.info.setText(f"OP/ED deseni hatalı: {e}")
            return False
        self._save_cfg()
        opts = self.options()
        events = list(self.ctx.doc.events)
        self._snap = events
        pcm, rate, total = ad.pcm, ad.rate, ad.duration_ms
        self.busy = True
        self.error = None
        self.b_calc.setEnabled(False)
        self.bb.button(QDialogButtonBox.Apply).setEnabled(False)
        state = {"p": 0.0}

        def work():
            try:
                from ..media.vad import speech_activity
                state["p"] = 0.05
                act = speech_activity(pcm, rate)
                state["p"] = 0.4
                self.segs, self.removed = S.plan(events, act, total, opts,
                                                 progress=lambda p: state.__setitem__("p", 0.4 + 0.6 * p))
            except Exception as e:  # noqa: BLE001
                self.error = str(e)

        th = threading.Thread(target=work, name="osisub-sync", daemon=True)
        th.start()

        def poll():
            self.bar.setValue(int(state["p"] * 1000))
            if th.is_alive():
                QTimer.singleShot(50, poll)
                return
            self.busy = False
            self.b_calc.setEnabled(True)
            self.bar.setValue(1000)
            if self.error:
                self.info.setText(f"Hata: {self.error}")
                return
            self._fill()
        QTimer.singleShot(50, poll)
        return True

    def wait(self, timeout=60.0):
        import time
        t = time.time()
        while self.busy and time.time() - t < timeout:
            QApplication.processEvents()
            time.sleep(0.01)
        QApplication.processEvents()
        return not self.busy

    def _fill(self):
        self.table.setRowCount(0)
        moved = 0
        for s in self.segs:
            r = self.table.rowCount()
            self.table.insertRow(r)
            cb = QCheckBox()
            low = bool(s.anchors) and s.confidence < LOW_CONF
            cb.setChecked(s.shift != 0 and not low)
            self.table.setCellWidget(r, 0, cb)
            vals = [f"{len(s.rows)} ({len(s.anchors)} çapa)", format_time(s.start),
                    f"{s.shift:+d} ms", f"{s.confidence:.2f}" if s.anchors else "—",
                    "; ".join(s.notes + (["güven düşük — kontrol edin"] if low else []))]
            for c, v in enumerate(vals, start=1):
                it = QTableWidgetItem(v)
                it.setFlags(it.flags() & ~Qt.ItemIsEditable)
                if low:
                    it.setForeground(QColor("#e05050"))
                self.table.setItem(r, c, it)
            moved += s.shift != 0
        self.table.resizeColumnsToContents()
        shifts = sorted({s.shift for s in self.segs})
        msg = f"{len(self.segs)} parça; {moved} parça kayıyor"
        if shifts:
            msg += f" (en az {shifts[0]:+d} ms, en çok {shifts[-1]:+d} ms)"
        if self.removed:
            msg += "; bastırılan ses: " + ", ".join(f"{format_time(a)}–{format_time(b)} ({why})"
                                                    for a, b, why in self.removed[:4])
        self.info.setText(msg + ". Satıra gitmek için çift tıkla.")
        self.bb.button(QDialogButtonBox.Apply).setEnabled(bool(self.segs))

    def _goto(self, r, _c):
        if 0 <= r < len(self.segs):
            row = self.segs[r].rows[0]
            self.ctx.set_active(row, select_only=False)

    def chosen(self) -> list[S.Segment]:
        out = []
        for r, s in enumerate(self.segs):
            cb = self.table.cellWidget(r, 0)
            if cb is not None and cb.isChecked():
                out.append(s)
        return out

    def apply(self):
        evs = self.ctx.doc.events
        if self._snap is None or len(evs) != len(self._snap) or any(a is not b for a, b in zip(evs, self._snap)):
            self.info.setText("Hesaplamadan sonra satırlar değişti; yeniden Hesapla")
            return None
        segs = self.chosen()
        ad = self.media.audio
        ch, warns = S.changes(evs, segs, ad.duration_ms if ad else 10 ** 9)
        if not ch:
            self.info.setText("Kaydırılacak satır yok")
            return None
        self.ctx.update_events(ch, "Konuşmaya göre senkron")
        self._snap = list(self.ctx.doc.events)
        self.info.setText(f"{len(ch)} satır kaydırıldı ({len(segs)} parça)" + ("; " + "; ".join(warns) if warns else ""))
        self.ctx.status.emit(self.info.text())
        return ch
