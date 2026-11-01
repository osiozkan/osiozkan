"""Karaoke tool, select-lines dialog, reference/translation panel."""
from __future__ import annotations

import re

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QFont, QKeySequence, QShortcut
from PySide6.QtWidgets import (QAbstractItemView, QCheckBox, QComboBox, QDialog, QDialogButtonBox, QFileDialog,
                               QFormLayout, QHBoxLayout, QLabel, QLineEdit, QMessageBox, QPlainTextEdit, QPushButton,
                               QSpinBox, QTableWidget, QTableWidgetItem, QVBoxLayout, QWidget)

from ..core import karaoke as K
from ..core.formats import load_file
from ..core.tags import plain_text, parse_karaoke
from ..core.timecode import ms_to_ass
from .dialogs import _buttons


class KaraokeDialog(QDialog):
    """Syllable editor for the active line: split, distribute, tap-time, edit durations."""

    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Karaoke zamanlayıcı")
        self.resize(760, 520)
        self.row = main.active_row()
        self.ev = main.active_event()
        self.taps: list[int] = []
        self.tapping = False
        if self.ev is None:
            QTimer.singleShot(0, self.reject)
            return
        syl = parse_karaoke(self.ev.text)
        self.lead = ""
        self.drops_tags = False
        if syl and any(s.duration_cs for s in syl):
            self.syls = [(s.text, s.duration_cs) for s in syl]
            self.lead = ""
        else:
            self.drops_tags = bool(re.search(r"\{[^}]*\}", self.ev.text))
            self.syls = [(t, 0) for t in K.split_text(plain_text_keep(self.ev.text), "char")]
            self.syls = list(zip([t for t, _ in self.syls], K.distribute([t for t, _ in self.syls], self._total())))
        self.info = QLabel("")
        self.table = QTableWidget(0, 2)
        self.table.setHorizontalHeaderLabels(["Hece", "Süre (cs)"])
        self.table.horizontalHeader().setStretchLastSection(True)
        self.table.setFont(QFont("monospace", 11))
        self.table.itemChanged.connect(self._item_changed)
        self.mode = QComboBox(); self.mode.addItems(["Karakter", "Sözcük", "Romaji hece"])
        self.ktype = QComboBox(); self.ktype.addItems(["k", "kf", "ko", "K"])
        b_split = QPushButton("Yeniden böl"); b_split.clicked.connect(self._resplit)
        b_eq = QPushButton("Süreleri eşitle"); b_eq.clicked.connect(self._equalise)
        b_join = QPushButton("Seçili hece + sonraki"); b_join.clicked.connect(self._join)
        b_cut = QPushButton("Seçili heceyi ikiye böl"); b_cut.clicked.connect(self._cut)
        self.tap_btn = QPushButton("● Dokunarak zamanla (başlat)"); self.tap_btn.clicked.connect(self._tap_toggle)
        self.tap_hint = QLabel("Başlatınca video satır başından oynar; her hece bittiğinde Ç tuşu veya 'Tık' düğmesi.")
        self.tap_hint.setWordWrap(True)
        tap = QPushButton("Tık"); tap.clicked.connect(self._tap)
        QShortcut(QKeySequence("C"), self, activated=self._tap)
        row1 = QHBoxLayout()
        for w in (QLabel("Bölme"), self.mode, b_split, b_eq, b_join, b_cut, QLabel("Tür"), self.ktype):
            row1.addWidget(w)
        row2 = QHBoxLayout(); row2.addWidget(self.tap_btn); row2.addWidget(tap)
        self.preview = QPlainTextEdit(); self.preview.setReadOnly(True); self.preview.setMaximumHeight(70)
        self.preview.setFont(QFont("monospace", 10))
        self.strip_b = QPushButton("Karaoke etiketlerini kaldır"); self.strip_b.clicked.connect(self._strip)
        lay = QVBoxLayout(self)
        lay.addWidget(self.info); lay.addLayout(row1); lay.addWidget(self.table, 1); lay.addLayout(row2)
        lay.addWidget(self.tap_hint); lay.addWidget(self.preview); lay.addWidget(self.strip_b); lay.addWidget(_buttons(self, "Uygula"))
        self._fill()

    def _total(self):
        return max(1, (self.ev.end_time - self.ev.start_time) // 10)

    def _fill(self):
        self.table.blockSignals(True)
        self.table.setRowCount(len(self.syls))
        for i, (t, d) in enumerate(self.syls):
            self.table.setItem(i, 0, QTableWidgetItem(t))
            self.table.setItem(i, 1, QTableWidgetItem(str(d)))
        self.table.blockSignals(False)
        self._update()

    def _item_changed(self, it):
        try:
            self.syls[it.row()] = (self.table.item(it.row(), 0).text(), max(0, int(self.table.item(it.row(), 1).text())))
        except ValueError:
            pass
        self._update()

    def _update(self):
        tot = sum(d for _, d in self.syls)
        want = self._total()
        self.info.setText(f"{len(self.syls)} hece — toplam {tot} cs / satır {want} cs "
                          f"({'tamam' if tot == want else 'fark ' + str(tot - want) + ' cs'})")
        self.preview.setPlainText(self.result())

    def _plain(self):
        return "".join(t for t, _ in self.syls)

    def _resplit(self):
        parts = K.split_text(self._plain(), ["char", "word", "romaji"][self.mode.currentIndex()])
        self.syls = list(zip(parts, K.distribute(parts, self._total())))
        self._fill()

    def _equalise(self):
        d = K.distribute([t for t, _ in self.syls], self._total())
        self.syls = [(t, x) for (t, _), x in zip(self.syls, d)]
        self._fill()

    def _join(self):
        r = self.table.currentRow()
        if 0 <= r < len(self.syls) - 1:
            a, b = self.syls[r], self.syls[r + 1]
            self.syls[r:r + 2] = [(a[0] + b[0], a[1] + b[1])]
            self._fill()

    def _cut(self):
        r = self.table.currentRow()
        if 0 <= r < len(self.syls):
            t, d = self.syls[r]
            if len(t) > 1:
                h = len(t) // 2
                self.syls[r:r + 1] = [(t[:h], d // 2), (t[h:], d - d // 2)]
                self._fill()

    def _strip(self):
        self.syls = [(self._plain(), self._total())]
        self._fill()

    def _tap_toggle(self):
        v = self.main.video
        if not self.tapping:
            self.taps = []
            self.tapping = True
            self.tap_btn.setText("■ Durdur / bitir")
            v.play_range(self.ev.start_time, self.ev.end_time + 500)
        else:
            self._finish_tap()

    def _tap(self):
        if not self.tapping:
            return
        self.taps.append(self.main.video.position())
        if len(self.taps) >= len(self.syls):
            self._finish_tap()

    def _finish_tap(self):
        self.tapping = False
        self.tap_btn.setText("● Dokunarak zamanla (başlat)")
        v = self.main.video
        if v.player:
            v.player.pause()
        n = len(self.taps)
        if n == 0:
            return
        durs = K.tap_durations(self.taps, self.ev.start_time, self.ev.end_time)
        # syllables without a tap share what remains of the line
        rest = max(0, self._total() - sum(durs))
        tail = K.distribute([t for t, _ in self.syls[n:]], rest) if len(self.syls) > n else []
        full = durs + tail
        self.syls = [(t, d) for (t, _), d in zip(self.syls, full)]
        self._fill()

    def result(self) -> str:
        return K.build(self.syls, self.ktype.currentText(), self.lead)

    def accept(self):
        txt = self.result()
        if self.drops_tags and QMessageBox.question(
                self, "Karaoke", "Satırdaki mevcut {…} etiketleri bu işlemle silinecek (karaoke etiketleri yeniden "
                "oluşturulur). Devam edilsin mi?") != QMessageBox.StandardButton.Yes:
            return
        if self.ev is not None:
            with self.main.project.edit("Karaoke", structural=False):
                self.ev.text = txt
            self.main.project.rowsChanged.emit(self.row, self.row)
            self.main._load_active()
        super().accept()


def plain_text_keep(text: str) -> str:
    """Plain text without tags but with \\N kept literal for syllable editing."""
    return re.sub(r"\{[^}]*\}", "", text)


class SelectDialog(QDialog):
    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Satır seç")
        self.pat = QLineEdit()
        self.field = QComboBox(); self.field.addItems(["Metin", "Stil", "Aktör", "Efekt", "Metin (etiketsiz)"])
        self.regex = QCheckBox("Düzenli ifade"); self.case = QCheckBox("Büyük/küçük harf duyarlı")
        self.neg = QCheckBox("Eşleşmeyenleri seç")
        self.comments = QComboBox(); self.comments.addItems(["Hepsi", "Yalnız yorumlar", "Yorumlar hariç"])
        self.mode = QComboBox(); self.mode.addItems(["Yeni seçim", "Seçime ekle", "Seçimden çıkar", "Kesişim"])
        self.status = QLabel("")
        f = QFormLayout(); f.addRow("Desen", self.pat); f.addRow("Alan", self.field)
        for w in (self.regex, self.case, self.neg):
            f.addRow(w)
        f.addRow("Satırlar", self.comments); f.addRow("Mod", self.mode)
        b = QPushButton("Seç"); b.clicked.connect(self.run)
        self.pat.returnPressed.connect(self.run)
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addWidget(b); lay.addWidget(self.status)

    def run(self):
        ev = self.main.project.events
        pat = self.pat.text()
        if not self.regex.isChecked():
            pat = re.escape(pat)
        try:
            rx = re.compile(pat, 0 if self.case.isChecked() else re.I)
        except re.error as e:
            self.status.setText(f"Regex hatası: {e}")
            return
        fi = self.field.currentIndex()
        hits = set()
        for i, e in enumerate(ev):
            if self.comments.currentIndex() == 1 and not e.comment:
                continue
            if self.comments.currentIndex() == 2 and e.comment:
                continue
            val = (e.text, e.style, e.actor, e.effect, plain_text(e.text))[fi]
            m = bool(rx.search(val))
            if m != self.neg.isChecked():
                hits.add(i)
        cur = set(self.main.selected_rows())
        mode = self.mode.currentIndex()
        res = hits if mode == 0 else (cur | hits) if mode == 1 else (cur - hits) if mode == 2 else (cur & hits)
        self.main.set_selection(sorted(res))
        self.status.setText(f"{len(res)} satır seçili")


class ReferencePanel(QWidget):
    """Shows reference/original subtitle text that overlaps the active line (translation workflow)."""

    def __init__(self, main):
        super().__init__()
        self.main = main
        self.ref = None
        self.by_index = QCheckBox("Sıra numarasıyla eşle")
        self.by_index.toggled.connect(lambda _: self.refresh())
        b = QPushButton("Referans yükle…"); b.clicked.connect(self.load)
        c = QPushButton("Metni satıra kopyala"); c.clicked.connect(self.copy_to_line)
        self.label = QLabel("Referans yok")
        self.text = QPlainTextEdit(); self.text.setReadOnly(True); self.text.setFont(QFont("sans", 12))
        row = QHBoxLayout(); row.addWidget(b); row.addWidget(c); row.addWidget(self.by_index)
        lay = QVBoxLayout(self); lay.addLayout(row); lay.addWidget(self.label); lay.addWidget(self.text)

    def load(self):
        p, _ = QFileDialog.getOpenFileName(self, "Referans altyazı", "", "Altyazılar (*.ass *.ssa *.srt *.vtt *.sub);;Tümü (*)")
        if p:
            try:
                self.ref = sorted((e for e in load_file(p)[0].events if not e.comment), key=lambda e: e.start_time)
            except Exception as e:
                QMessageBox.warning(self, "Referans", str(e))
                return
            self.label.setText(f"{p} — {len(self.ref)} satır")
            self.refresh()

    def matches(self):
        e = self.main.active_event()
        if e is None or not self.ref:
            return []
        if self.by_index.isChecked():
            r = self.main.active_row()
            return [self.ref[r]] if 0 <= r < len(self.ref) else []
        mid = (e.start_time + e.end_time) // 2
        hits = [x for x in self.ref if x.start_time < e.end_time and x.end_time > e.start_time]
        hits.sort(key=lambda x: -min(x.end_time, e.end_time) + max(x.start_time, e.start_time))
        return hits

    def refresh(self):
        m = self.matches()
        self.text.setPlainText("\n— — —\n".join(plain_text(x.text) for x in m))

    def copy_to_line(self):
        m = self.matches()
        e = self.main.active_event()
        if m and e is not None:
            with self.main.project.edit("Referans kopyala", structural=False):
                e.text = m[0].text
            self.main.project.rowsChanged.emit(self.main.active_row(), self.main.active_row())
            self.main._load_active()
