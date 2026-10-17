"""Konuşmayı yazıya dök (Whisper) penceresi.

Çıktı iki biçimde kullanılabilir:
  * Referans olarak yükle (varsayılan): ızgarada Referans sütununda özgün söz görünür.
  * Yeni satırlar olarak ekle: zamanlanmış satırlar (metinli) belgeye girer; mevcut
    satırların arasına zamana göre yerleşir, tek geri al adımı.
"""

from __future__ import annotations

import threading
import time

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QApplication, QCheckBox, QComboBox, QDialog, QDialogButtonBox,
                               QFormLayout, QLabel, QPlainTextEdit, QProgressBar, QVBoxLayout)

from ..core import asr
from ..core.assfile import Event


class AsrDialog(QDialog):
    def __init__(self, win):
        super().__init__(win)
        self.win = win
        self.ctx = win.ctx
        self.setWindowTitle("Konuşmayı yazıya dök (Whisper)")
        self.resize(560, 420)
        cfg = self.ctx.config
        self.model = QComboBox()
        self.model.addItems(asr.MODELS)
        self.model.setCurrentText(cfg["asr_model"] or "small")
        self.model.setToolTip("tiny/base hızlı ama kaba; small iyi denge; medium/large-v3 en doğru (yavaş, büyük)")
        self.lang = QComboBox()
        for code, name in asr.LANGS.items():
            self.lang.addItem(name, code)
        i = self.lang.findData(cfg["asr_lang"] or "ja")
        self.lang.setCurrentIndex(max(0, i))
        self.scope = QComboBox()
        self.scope.addItems(["Tüm ses", "Seçili satırların kapsadığı aralık"])
        self.output = QComboBox()
        self.output.addItems(["Referans olarak yükle", "Yeni satırlar olarak ekle"])
        self.snap = QCheckBox("Sınırları konuşma bölümlerine (VAD) yasla")
        self.snap.setChecked(True)
        form = QFormLayout()
        form.addRow("Model", self.model)
        form.addRow("Dil", self.lang)
        form.addRow("Aralık", self.scope)
        form.addRow("Çıktı", self.output)
        form.addRow("", self.snap)
        self.bar = QProgressBar()
        self.bar.setRange(0, 1000)
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        self.log.setMaximumHeight(140)
        self.info = QLabel("")
        ok, why = asr.available()
        if not ok:
            self.info.setText(why)
        self.bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        self.bb.button(QDialogButtonBox.Ok).setText("Başlat")
        self.bb.button(QDialogButtonBox.Ok).setEnabled(ok and self.ctx.media.audio is not None)
        self.bb.accepted.connect(self.start)
        self.bb.rejected.connect(self._cancel)
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(self.bar)
        lay.addWidget(self.log)
        lay.addWidget(self.info)
        lay.addWidget(self.bb)
        self.running = False
        self.cancelled = False
        self.result = None
        self.error = None

    def _range(self):
        if self.scope.currentIndex() == 1:
            rows = self.ctx.selected_rows()
            evs = self.ctx.doc.events
            if rows:
                return min(evs[r].start for r in rows) - 500, max(evs[r].end for r in rows) + 500
        return 0, None

    def _cancel(self):
        if self.running:
            self.cancelled = True
            self.info.setText("İptal ediliyor…")
        else:
            self.reject()

    def start(self):
        """Arka planda çalıştırır; bu pencere açıkken olay döngüsünü döndürür."""
        ad = self.ctx.media.audio
        if ad is None:
            return None
        cfg = self.ctx.config
        cfg["asr_model"] = self.model.currentText()
        cfg["asr_lang"] = self.lang.currentData()
        cfg.save()
        a, b = self._range()
        a = max(0, a)
        speech = self.ctx.media.speech if self.snap.isChecked() else None
        self.running = True
        self.bb.button(QDialogButtonBox.Ok).setEnabled(False)
        state = {"p": 0.0, "msg": ""}

        def prog(p, msg):
            state["p"], state["msg"] = p, msg

        def work():
            try:
                self.result = asr.transcribe(ad.pcm, ad.rate, self.lang.currentData(), self.model.currentText(),
                                             start_ms=a, end_ms=b, progress=prog,
                                             cancel=lambda: self.cancelled, speech_segs=speech)
            except Exception as e:  # noqa: BLE001
                self.error = str(e)

        th = threading.Thread(target=work, name="osisub-asr", daemon=True)
        th.start()
        last = ""
        while th.is_alive():
            QApplication.processEvents()
            th.join(0.05)
            self.bar.setValue(int(state["p"] * 1000))
            if state["msg"] and state["msg"] != last:
                last = state["msg"]
                self.log.appendPlainText(last)
        self.running = False
        if self.error:
            self.info.setText(f"Hata: {self.error}")
            self.bb.button(QDialogButtonBox.Ok).setEnabled(True)
            return None
        self.apply_result(self.result)
        self.accept()
        return self.result

    def apply_result(self, lines):
        label = f"Whisper ({self.lang.currentData() or 'oto'}, {self.model.currentText()})"
        evs = [Event(start=s, end=e, text=t) for s, e, t in lines]
        if self.output.currentIndex() == 0:
            self.win.reference.load_events(evs, label)
            self.win.dock_ref.show()
            self.ctx.status.emit(f"{label}: {len(evs)} satır referans olarak yüklendi")
        else:
            insert_timed(self.ctx, evs, "Whisper satırları")
            self.ctx.status.emit(f"{label}: {len(evs)} satır eklendi")


def insert_timed(ctx, new_events, desc: str):
    """Satırları mevcut sırayı bozmadan zamana göre araya yerleştir (tek geri al)."""
    style = ctx.active_event().style if ctx.active_event() is not None else "Default"
    out = list(ctx.doc.events)
    added = []
    for ev in sorted(new_events, key=lambda e: e.start):
        if ev.style == "Default" and style != "Default":
            ev = ev.replace(style=style)
        pos = len(out)
        for i, e in enumerate(out):
            if not e.comment and e.start > ev.start:
                pos = i
                break
        out.insert(pos, ev)
        added.append(ev)
    ids = {id(e) for e in added}
    rows = {i for i, e in enumerate(out) if id(e) in ids}
    first = min(rows) if rows else 0
    ctx.replace_all_events(out, desc, selection=rows or {first}, active=first)
    return rows
