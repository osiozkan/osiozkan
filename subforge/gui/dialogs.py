"""Editor dialogs: styles, find/replace, timing tools, properties, QC."""
from __future__ import annotations

import numpy as np
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QFont, QImage, QPixmap
from PySide6.QtWidgets import (QAbstractItemView, QCheckBox, QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox,
                               QFontComboBox, QFormLayout, QGridLayout, QGroupBox, QHBoxLayout, QInputDialog, QLabel,
                               QLineEdit, QListWidget, QListWidgetItem, QMessageBox, QPushButton, QRadioButton,
                               QSpinBox, QTableWidget, QTableWidgetItem, QVBoxLayout, QWidget, QButtonGroup,
                               QPlainTextEdit, QTabWidget)

from ..core import checks, tools
from ..core.ass import ASSFile, Event, Style
from ..core.timecode import ms_to_ass, ass_to_ms
from .. import render
from .widgets import ColorButton, TimeEdit


def _buttons(dlg, ok="Tamam", cancel="İptal"):
    bb = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
    bb.button(QDialogButtonBox.StandardButton.Ok).setText(ok)
    bb.button(QDialogButtonBox.StandardButton.Cancel).setText(cancel)
    bb.accepted.connect(dlg.accept)
    bb.rejected.connect(dlg.reject)
    return bb


# ============================================================================ styles

class StyleEditor(QWidget):
    changed = Signal()

    def __init__(self):
        super().__init__()
        self.style: Style | None = None
        self._loading = False
        f = QFormLayout()
        self.name = QLineEdit(); self.font = QFontComboBox(); self.size = QDoubleSpinBox()
        self.size.setRange(1, 999); self.size.setDecimals(1)
        self.bold = QCheckBox("B"); self.italic = QCheckBox("I"); self.underline = QCheckBox("U"); self.strike = QCheckBox("S")
        fr = QHBoxLayout()
        for w in (self.font, self.size, self.bold, self.italic, self.underline, self.strike):
            fr.addWidget(w)
        self.c1, self.c2, self.c3, self.c4 = (ColorButton() for _ in range(4))
        cr = QHBoxLayout()
        for lbl, w in (("Ana", self.c1), ("İkincil", self.c2), ("Kenar", self.c3), ("Gölge", self.c4)):
            cr.addWidget(QLabel(lbl)); cr.addWidget(w)
        self.outline = QDoubleSpinBox(); self.shadow = QDoubleSpinBox()
        for w in (self.outline, self.shadow):
            w.setRange(0, 99); w.setDecimals(2); w.setSingleStep(0.5)
        self.border = QComboBox(); self.border.addItems(["Kenar+gölge", "Opak kutu"])
        br = QHBoxLayout(); br.addWidget(QLabel("Kenar")); br.addWidget(self.outline); br.addWidget(QLabel("Gölge"))
        br.addWidget(self.shadow); br.addWidget(self.border)
        self.sx = QDoubleSpinBox(); self.sy = QDoubleSpinBox(); self.spacing = QDoubleSpinBox(); self.angle = QDoubleSpinBox()
        for w in (self.sx, self.sy):
            w.setRange(1, 1000); w.setValue(100)
        self.spacing.setRange(-100, 100); self.angle.setRange(-360, 360)
        sr = QHBoxLayout()
        for lbl, w in (("Ölçek X", self.sx), ("Y", self.sy), ("Aralık", self.spacing), ("Açı", self.angle)):
            sr.addWidget(QLabel(lbl)); sr.addWidget(w)
        self.ml, self.mr, self.mv = QSpinBox(), QSpinBox(), QSpinBox()
        for w in (self.ml, self.mr, self.mv):
            w.setRange(0, 9999)
        mr = QHBoxLayout()
        for lbl, w in (("Sol", self.ml), ("Sağ", self.mr), ("Dikey", self.mv)):
            mr.addWidget(QLabel(lbl)); mr.addWidget(w)
        self.align_group = QButtonGroup(self)
        grid = QGridLayout()
        for n in range(1, 10):
            rb = QRadioButton(str(n)); self.align_group.addButton(rb, n)
            grid.addWidget(rb, (9 - n) // 3, (n - 1) % 3)
        self.encoding = QSpinBox(); self.encoding.setRange(0, 255)
        f.addRow("Ad", self.name); f.addRow("Yazı tipi", fr); f.addRow("Renkler", cr)
        f.addRow("Çerçeve", br); f.addRow("Ölçek", sr); f.addRow("Kenar boşluğu", mr)
        gw = QWidget(); gw.setLayout(grid)
        f.addRow("Hizalama", gw); f.addRow("Kodlama", self.encoding)
        self.setLayout(f)
        for w in (self.name,):
            w.textEdited.connect(self._apply)
        for w in (self.size, self.outline, self.shadow, self.sx, self.sy, self.spacing, self.angle, self.ml, self.mr, self.mv, self.encoding):
            w.valueChanged.connect(self._apply)
        for w in (self.bold, self.italic, self.underline, self.strike):
            w.toggled.connect(self._apply)
        self.font.currentFontChanged.connect(self._apply)
        self.border.currentIndexChanged.connect(self._apply)
        self.align_group.idClicked.connect(self._apply)
        for c in (self.c1, self.c2, self.c3, self.c4):
            c.colorChanged.connect(self._apply)

    def load(self, s: Style | None):
        self._loading = True
        self.style = s
        self.setEnabled(s is not None)
        if s:
            self.name.setText(s.name); self.font.setCurrentFont(QFont(s.fontname)); self.size.setValue(s.fontsize)
            self.bold.setChecked(s.bold); self.italic.setChecked(s.italic); self.underline.setChecked(s.underline)
            self.strike.setChecked(s.strikeout)
            for c, v in ((self.c1, s.color1), (self.c2, s.color2), (self.c3, s.color3), (self.c4, s.color4)):
                c.set_ass(v)
            self.outline.setValue(s.outline); self.shadow.setValue(s.shadow)
            self.border.setCurrentIndex(1 if s.borderstyle == 3 else 0)
            self.sx.setValue(s.scale_x); self.sy.setValue(s.scale_y); self.spacing.setValue(s.spacing); self.angle.setValue(s.angle)
            self.ml.setValue(s.margin_l); self.mr.setValue(s.margin_r); self.mv.setValue(s.margin_t)
            b = self.align_group.button(s.align)
            if b: b.setChecked(True)
            self.encoding.setValue(s.encoding)
        self._loading = False

    def _apply(self, *_):
        s = self.style
        if self._loading or s is None:
            return
        s.name = self.name.text() or s.name
        s.fontname = self.font.currentFont().family(); s.fontsize = self.size.value()
        s.bold, s.italic, s.underline, s.strikeout = (self.bold.isChecked(), self.italic.isChecked(),
                                                      self.underline.isChecked(), self.strike.isChecked())
        s.color1, s.color2, s.color3, s.color4 = self.c1.ass(), self.c2.ass(), self.c3.ass(), self.c4.ass()
        s.outline, s.shadow = self.outline.value(), self.shadow.value()
        s.borderstyle = 3 if self.border.currentIndex() == 1 else 1
        s.scale_x, s.scale_y, s.spacing, s.angle = self.sx.value(), self.sy.value(), self.spacing.value(), self.angle.value()
        s.margin_l, s.margin_r = self.ml.value(), self.mr.value()
        s.margin_t = s.margin_b = self.mv.value()
        s.align = self.align_group.checkedId() if self.align_group.checkedId() > 0 else s.align
        s.encoding = self.encoding.value()
        self.changed.emit()


class StylesDialog(QDialog):
    def __init__(self, project, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Stil Yöneticisi")
        self.resize(900, 560)
        self.project = project
        self.items: list[tuple[Style | None, Style]] = [(s, s.copy()) for s in project.doc.styles]  # (orig, working)
        self.removed: list[Style] = []
        self.list = QListWidget()
        self.editor = StyleEditor()
        self.preview = QLabel(); self.preview.setMinimumHeight(130); self.preview.setStyleSheet("background:#000")
        self.preview.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.sample = QLineEdit("Hızlı kahverengi tilki / The quick brown fox")
        b_new, b_copy, b_del = QPushButton("Yeni"), QPushButton("Kopyala"), QPushButton("Sil")
        b_new.clicked.connect(self._new); b_copy.clicked.connect(self._copy); b_del.clicked.connect(self._del)
        left = QVBoxLayout(); left.addWidget(self.list)
        lb = QHBoxLayout()
        for b in (b_new, b_copy, b_del):
            lb.addWidget(b)
        left.addLayout(lb)
        right = QVBoxLayout(); right.addWidget(self.editor); right.addWidget(QLabel("Önizleme metni")); right.addWidget(self.sample)
        right.addWidget(self.preview)
        row = QHBoxLayout(); row.addLayout(left, 1); row.addLayout(right, 3)
        lay = QVBoxLayout(self); lay.addLayout(row); lay.addWidget(_buttons(self))
        self.list.currentRowChanged.connect(self._select)
        self.editor.changed.connect(self._edited)
        self.sample.textChanged.connect(self._preview)
        self._fill()

    def _fill(self, sel=0):
        self.list.blockSignals(True)
        self.list.clear()
        for _o, w in self.items:
            self.list.addItem(w.name)
        self.list.blockSignals(False)
        if self.items:
            self.list.setCurrentRow(min(sel, len(self.items) - 1))
        else:
            self.editor.load(None)

    def _select(self, r):
        self.editor.load(self.items[r][1] if 0 <= r < len(self.items) else None)
        self._preview()

    def _edited(self):
        r = self.list.currentRow()
        if r >= 0:
            self.list.item(r).setText(self.items[r][1].name)
        self._preview()

    def _new(self):
        s = Style(name=self._unique("Style"))
        self.items.append((None, s)); self._fill(len(self.items) - 1)

    def _copy(self):
        r = self.list.currentRow()
        if r >= 0:
            s = self.items[r][1].copy(); s.name = self._unique(s.name + " copy")
            self.items.append((None, s)); self._fill(len(self.items) - 1)

    def _del(self):
        r = self.list.currentRow()
        if r >= 0 and len(self.items) > 1:
            o, _w = self.items.pop(r)
            if o is not None:
                self.removed.append(o)
            self._fill(r)

    def _unique(self, base):
        names = {w.name for _o, w in self.items}
        n, cand = 1, base
        while cand in names:
            n += 1; cand = f"{base} {n}"
        return cand

    def _preview(self):
        r = self.list.currentRow()
        if r < 0 or not render.available():
            return
        s = self.items[r][1]
        d = ASSFile.new()
        d.lines = [l for l in d.lines if not isinstance(l, Style)]
        d.set_info("PlayResX", self.project.doc.play_res[0]); d.set_info("PlayResY", self.project.doc.play_res[1])
        d.lines.append(s.copy())
        d.lines.append(Event(start_time=0, end_time=10000, style=s.name, text=self.sample.text()))
        rr = render.make_renderer()
        rr.set_document(d.to_string())
        W = 640
        H = int(W * d.play_res[1] / d.play_res[0])
        arr = rr.render(1000, W, H, d.play_res)
        bg = np.zeros((H, W, 3), dtype=np.float32) + [24, 28, 40]
        if arr is not None:
            a = arr[..., 3:4].astype(np.float32) / 255
            bg = bg * (1 - a) + arr[..., :3] * a
        img = np.ascontiguousarray(bg.astype(np.uint8))
        q = QImage(img.data, W, H, W * 3, QImage.Format.Format_RGB888).copy()
        self.preview.setPixmap(QPixmap.fromImage(q).scaledToHeight(130, Qt.TransformationMode.SmoothTransformation))
        rr.close()

    def apply(self):
        names = [w.name for _o, w in self.items]
        if len(set(names)) != len(names):
            QMessageBox.warning(self, "Stil", "Stil adları benzersiz olmalı.")
            return False
        with self.project.edit("Stilleri düzenle"):
            lines = self.project.doc.lines
            renames = {}
            for o, w in self.items:
                if o is not None:
                    if o.name != w.name:
                        renames[o.name] = w.name
                    i = next(i for i, l in enumerate(lines) if l is o)
                    lines[i] = w
            for o in self.removed:
                lines[:] = [l for l in lines if l is not o]
            last = max([i for i, l in enumerate(lines) if isinstance(l, Style)], default=-1)
            for o, w in self.items:
                if o is None:
                    last += 1
                    lines.insert(last, w)
            for l in lines:
                if isinstance(l, Event) and l.style in renames:
                    l.style = renames[l.style]
        self.project.stylesChanged.emit()
        return True

    def accept(self):
        if self.apply():
            super().accept()


# ============================================================================ find / replace

class FindReplaceDialog(QDialog):
    """Non-modal Find & Replace. `main` provides: selected_rows(), goto_row(row), project."""

    def __init__(self, main):
        super().__init__(main)
        self.main = main
        self.setWindowTitle("Bul / Değiştir")
        self.find = QLineEdit(); self.repl = QLineEdit()
        self.regex = QCheckBox("Düzenli ifade (regex)"); self.case = QCheckBox("Büyük/küçük harf"); self.whole = QCheckBox("Tam sözcük")
        self.field = QComboBox(); self.field.addItems(["Metin", "Aktör", "Efekt", "Stil"])
        self.scope = QComboBox(); self.scope.addItems(["Tüm satırlar", "Seçili satırlar"])
        self.skip_comments = QCheckBox("Yorum satırlarını atla")
        self.status = QLabel("")
        f = QFormLayout(); f.addRow("Bul", self.find); f.addRow("Değiştir", self.repl)
        f.addRow("Alan", self.field); f.addRow("Kapsam", self.scope)
        for w in (self.regex, self.case, self.whole, self.skip_comments):
            f.addRow(w)
        b1, b2, b3 = QPushButton("Sonrakini bul"), QPushButton("Değiştir"), QPushButton("Tümünü değiştir")
        b1.clicked.connect(self.find_next); b2.clicked.connect(self.replace_one); b3.clicked.connect(self.replace_all)
        row = QHBoxLayout()
        for b in (b1, b2, b3):
            row.addWidget(b)
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addLayout(row); lay.addWidget(self.status)
        self.find.returnPressed.connect(self.find_next)

    def _fieldname(self):
        return ["text", "actor", "effect", "style"][self.field.currentIndex()]

    def _rx(self):
        import re
        pat = self.find.text()
        if not self.regex.isChecked():
            pat = re.escape(pat)
        if self.whole.isChecked():
            pat = r"\b" + pat + r"\b"
        try:
            return re.compile(pat, 0 if self.case.isChecked() else re.I)
        except re.error as e:
            self.status.setText(f"Regex hatası: {e}")
            return None

    def _rows(self):
        evs = self.main.project.events
        if self.scope.currentIndex() == 1:
            return [r for r in self.main.selected_rows()]
        return list(range(len(evs)))

    def find_next(self):
        rx = self._rx()
        if not rx or not self.find.text():
            return
        evs = self.main.project.events
        cur = self.main.active_row()
        rows = self._rows()
        order = [r for r in rows if r > cur] + [r for r in rows if r <= cur]
        fld = self._fieldname()
        for r in order:
            if self.skip_comments.isChecked() and evs[r].comment:
                continue
            if rx.search(getattr(evs[r], fld)):
                self.main.goto_row(r)
                self.status.setText(f"Satır {r + 1}")
                return
        self.status.setText("Bulunamadı")

    def replace_one(self):
        rx = self._rx()
        if not rx:
            return
        r = self.main.active_row()
        evs = self.main.project.events
        if r < 0 or r >= len(evs):
            return
        fld = self._fieldname()
        if rx.search(getattr(evs[r], fld)):
            with self.main.project.edit("Değiştir"):
                setattr(evs[r], fld, rx.sub(self._r(), getattr(evs[r], fld), count=1))
        self.find_next()

    def _r(self):
        return self.repl.text() if self.regex.isChecked() else self.repl.text().replace("\\", "\\\\")

    def replace_all(self):
        rx = self._rx()
        if not rx or not self.find.text():
            return
        evs = self.main.project.events
        fld = self._fieldname()
        n = 0
        with self.main.project.edit("Tümünü değiştir"):
            for r in self._rows():
                e = evs[r]
                if self.skip_comments.isChecked() and e.comment:
                    continue
                new, k = rx.subn(self._r(), getattr(e, fld))
                if k:
                    setattr(e, fld, new)
                    n += k
        self.status.setText(f"{n} değişiklik")


# ============================================================================ timing dialogs

class ShiftDialog(QDialog):
    def __init__(self, parent, fps, n_sel):
        super().__init__(parent)
        self.setWindowTitle("Zamanları Kaydır")
        self.fps = fps or 23.976
        self.time = TimeEdit(); self.time.set_ms(0); self.time.setReadOnly(False)
        self.frames = QSpinBox(); self.frames.setRange(-10_000_000, 10_000_000)
        self.by_frames = QRadioButton("Kare"); self.by_time = QRadioButton("Süre (H:MM:SS.cc)")
        self.by_time.setChecked(True)
        self.back = QCheckBox("Geri (negatif)")
        self.which = QComboBox(); self.which.addItems(["Başlangıç+Bitiş", "Yalnız başlangıç", "Yalnız bitiş"])
        self.scope = QComboBox(); self.scope.addItems(["Tüm satırlar", f"Seçili satırlar ({n_sel})", "Seçilenden sona"])
        if n_sel:
            self.scope.setCurrentIndex(1)
        f = QFormLayout(); f.addRow(self.by_time, self.time); f.addRow(self.by_frames, self.frames)
        f.addRow(self.back); f.addRow("Uygula", self.which); f.addRow("Kapsam", self.scope)
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addWidget(_buttons(self))

    def values(self):
        ms = self.time.ms() if self.by_time.isChecked() else int(round(self.frames.value() * 1000 / self.fps))
        if self.back.isChecked():
            ms = -abs(ms)
        return ms, ["both", "start", "end"][self.which.currentIndex()], self.scope.currentIndex()


class TimingDialog(QDialog):
    """Aegisub-style timing post-processor + Subtitle Edit gap tools."""

    def __init__(self, parent, n_sel, has_kf):
        super().__init__(parent)
        self.setWindowTitle("Zamanlama Son İşlemcisi")
        self.lead_in = QSpinBox(); self.lead_out = QSpinBox()
        for w in (self.lead_in, self.lead_out):
            w.setRange(0, 5000); w.setSuffix(" ms")
        self.lead_in.setValue(120); self.lead_out.setValue(200)
        self.en_lead = QCheckBox("Lead-in / lead-out ekle"); self.en_lead.setChecked(True)
        self.en_bridge = QCheckBox("Kısa boşlukları kapat (≤)")
        self.bridge = QSpinBox(); self.bridge.setRange(0, 5000); self.bridge.setValue(250); self.bridge.setSuffix(" ms")
        self.min_gap = QSpinBox(); self.min_gap.setRange(0, 1000); self.min_gap.setValue(0); self.min_gap.setSuffix(" ms")
        self.en_kf = QCheckBox("Anahtar karelere yapıştır"); self.en_kf.setEnabled(has_kf); self.en_kf.setChecked(has_kf)
        self.kf_before = QSpinBox(); self.kf_after = QSpinBox()
        for w in (self.kf_before, self.kf_after):
            w.setRange(0, 5000); w.setValue(350); w.setSuffix(" ms")
        self.en_over = QCheckBox("Çakışmaları düzelt"); self.en_over.setChecked(True)
        self.en_dur = QCheckBox("Süre sınırları (min/max)")
        self.min_dur = QSpinBox(); self.min_dur.setRange(0, 20000); self.min_dur.setValue(700); self.min_dur.setSuffix(" ms")
        self.max_dur = QSpinBox(); self.max_dur.setRange(0, 60000); self.max_dur.setValue(8000); self.max_dur.setSuffix(" ms")
        self.scope = QComboBox(); self.scope.addItems(["Tüm satırlar", f"Seçili satırlar ({n_sel})"])
        if n_sel > 1:
            self.scope.setCurrentIndex(1)
        f = QFormLayout()
        f.addRow(self.en_lead); f.addRow("Lead-in", self.lead_in); f.addRow("Lead-out", self.lead_out)
        f.addRow(self.en_bridge); f.addRow("Maks. boşluk", self.bridge); f.addRow("Satırlar arası min. boşluk", self.min_gap)
        f.addRow(self.en_kf); f.addRow("Anahtar kareden önce", self.kf_before); f.addRow("Anahtar kareden sonra", self.kf_after)
        f.addRow(self.en_over); f.addRow(self.en_dur); f.addRow("Min. süre", self.min_dur); f.addRow("Maks. süre", self.max_dur)
        f.addRow("Kapsam", self.scope)
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addWidget(_buttons(self, "Uygula"))

    def run(self, events, keyframes) -> dict:
        r = {}
        if self.en_lead.isChecked():
            r["lead"] = tools.lead_in_out(events, self.lead_in.value(), self.lead_out.value())
        if self.en_kf.isChecked() and keyframes:
            r["snap"] = tools.snap_to_keyframes(events, keyframes, self.kf_before.value(), self.kf_after.value(),
                                                self.kf_before.value(), self.kf_after.value())
        if self.en_dur.isChecked():
            r["duration"] = tools.enforce_duration(events, self.min_dur.value(), self.max_dur.value(), None)
        if self.en_bridge.isChecked():
            r["bridge"] = tools.bridge_gaps(events, self.bridge.value(), self.min_gap.value())
        if self.en_over.isChecked():
            r["overlap"] = tools.fix_overlaps(events, self.min_gap.value())
        return r


class SyncDialog(QDialog):
    def __init__(self, parent, first_start, last_start):
        super().__init__(parent)
        self.setWindowTitle("İki Noktalı Senkron (kayma/FPS düzeltme)")
        self.s1, self.d1, self.s2, self.d2 = (TimeEdit() for _ in range(4))
        self.s1.set_ms(first_start); self.d1.set_ms(first_start); self.s2.set_ms(last_start); self.d2.set_ms(last_start)
        f = QFormLayout()
        f.addRow("Nokta 1 — mevcut", self.s1); f.addRow("Nokta 1 — olması gereken", self.d1)
        f.addRow("Nokta 2 — mevcut", self.s2); f.addRow("Nokta 2 — olması gereken", self.d2)
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addWidget(_buttons(self))

    def values(self):
        return self.s1.ms(), self.d1.ms(), self.s2.ms(), self.d2.ms()


class FpsDialog(QDialog):
    PRESETS = ["23.976", "24", "25", "29.97", "30", "50", "59.94", "60"]

    def __init__(self, parent, title="FPS Değiştir", two=True, cur=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.src = QComboBox(); self.src.setEditable(True); self.src.addItems(self.PRESETS)
        self.dst = QComboBox(); self.dst.setEditable(True); self.dst.addItems(self.PRESETS)
        if cur:
            self.src.setCurrentText(f"{cur:g}")
        f = QFormLayout()
        f.addRow("Kaynak FPS" if two else "FPS", self.src)
        if two:
            f.addRow("Hedef FPS", self.dst)
        self.two = two
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addWidget(_buttons(self))

    def values(self):
        return float(self.src.currentText().replace(",", ".")), float(self.dst.currentText().replace(",", "."))


class PropertiesDialog(QDialog):
    def __init__(self, project, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Betik Özellikleri")
        self.p = project
        d = project.doc
        self.title = QLineEdit(d.get_info("Title"))
        self.author = QLineEdit(d.get_info("Original Script"))
        self.rx = QSpinBox(); self.ry = QSpinBox()
        for w in (self.rx, self.ry):
            w.setRange(1, 20000)
        self.rx.setValue(d.play_res[0]); self.ry.setValue(d.play_res[1])
        self.wrap = QComboBox(); self.wrap.addItems(["0: Akıllı (üst geniş)", "1: Satır sonu", "2: Kaydırma yok", "3: Akıllı (alt geniş)"])
        self.wrap.setCurrentIndex(int(d.get_info("WrapStyle", "0") or 0) % 4)
        self.scaled = QCheckBox("Kenar ve gölgeyi ölçekle"); self.scaled.setChecked(d.get_info("ScaledBorderAndShadow", "yes").lower() == "yes")
        self.matrix = QComboBox(); self.matrix.setEditable(True)
        self.matrix.addItems(["None", "TV.601", "PC.601", "TV.709", "PC.709", "TV.2020", "PC.2020"])
        self.matrix.setCurrentText(d.get_info("YCbCr Matrix", "TV.709"))
        vb = QPushButton("Videodan al")
        vb.setEnabled(bool(project.video_size))
        vb.clicked.connect(lambda: (self.rx.setValue(project.video_size[0]), self.ry.setValue(project.video_size[1])))
        self.resample = QCheckBox("Çözünürlük değişirse stilleri/etiketleri ölçekle"); self.resample.setChecked(True)
        f = QFormLayout()
        f.addRow("Başlık", self.title); f.addRow("Yazar", self.author)
        row = QHBoxLayout(); row.addWidget(self.rx); row.addWidget(QLabel("×")); row.addWidget(self.ry); row.addWidget(vb)
        f.addRow("Çözünürlük", row); f.addRow(self.resample)
        f.addRow("Satır kaydırma", self.wrap); f.addRow(self.scaled); f.addRow("YCbCr matrisi", self.matrix)
        lay = QVBoxLayout(self); lay.addLayout(f); lay.addWidget(_buttons(self))

    def apply(self):
        p = self.p
        old = p.doc.play_res
        new = (self.rx.value(), self.ry.value())
        with p.edit("Betik özellikleri"):
            d = p.doc
            d.set_info("Title", self.title.text())
            if self.author.text():
                d.set_info("Original Script", self.author.text())
            d.set_info("WrapStyle", self.wrap.currentIndex())
            d.set_info("ScaledBorderAndShadow", "yes" if self.scaled.isChecked() else "no")
            d.set_info("YCbCr Matrix", self.matrix.currentText())
            if new != old:
                if self.resample.isChecked():
                    tools.resample(d, new[0], new[1])
                else:
                    d.set_info("PlayResX", new[0]); d.set_info("PlayResY", new[1])


# ============================================================================ QC

class QCDialog(QDialog):
    jumpTo = Signal(int)

    def __init__(self, project, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Kalite Kontrol")
        self.resize(780, 520)
        self.p = project
        self.cps = QDoubleSpinBox(); self.cps.setRange(5, 60); self.cps.setValue(20)
        self.chars = QSpinBox(); self.chars.setRange(10, 200); self.chars.setValue(42)
        self.lines = QSpinBox(); self.lines.setRange(1, 10); self.lines.setValue(2)
        self.mind = QSpinBox(); self.mind.setRange(0, 5000); self.mind.setValue(700)
        self.maxd = QSpinBox(); self.maxd.setRange(500, 60000); self.maxd.setValue(8000)
        self.gap = QSpinBox(); self.gap.setRange(0, 1000); self.gap.setValue(0)
        row = QHBoxLayout()
        for lbl, w in (("Maks CPS", self.cps), ("Satır uzunluğu", self.chars), ("Maks satır", self.lines),
                       ("Min süre", self.mind), ("Maks süre", self.maxd), ("Min boşluk", self.gap)):
            row.addWidget(QLabel(lbl)); row.addWidget(w)
        run = QPushButton("Denetle"); run.clicked.connect(self.run)
        self.table = QTableWidget(0, 4)
        self.table.setHorizontalHeaderLabels(["Satır", "Önem", "Tür", "Açıklama"])
        self.table.horizontalHeader().setStretchLastSection(True)
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self.table.cellDoubleClicked.connect(lambda r, c: self.jumpTo.emit(int(self.table.item(r, 0).data(Qt.ItemDataRole.UserRole))))
        self.summary = QLabel("")
        lay = QVBoxLayout(self); lay.addLayout(row); lay.addWidget(run); lay.addWidget(self.table); lay.addWidget(self.summary)
        self.issues = []

    def settings(self):
        return checks.QCSettings(max_cps=self.cps.value(), max_chars_per_line=self.chars.value(), max_lines=self.lines.value(),
                                 min_duration=self.mind.value(), max_duration=self.maxd.value(), min_gap=self.gap.value())

    def run(self):
        self.issues = checks.check(self.p.doc, self.settings())
        self.table.setRowCount(len(self.issues))
        for r, i in enumerate(self.issues):
            it = QTableWidgetItem(str(i.index + 1)); it.setData(Qt.ItemDataRole.UserRole, i.index)
            self.table.setItem(r, 0, it)
            self.table.setItem(r, 1, QTableWidgetItem({"error": "Hata", "warning": "Uyarı", "info": "Bilgi"}[i.severity]))
            self.table.setItem(r, 2, QTableWidgetItem(i.code))
            self.table.setItem(r, 3, QTableWidgetItem(i.message))
        e = sum(i.severity == "error" for i in self.issues)
        w = sum(i.severity == "warning" for i in self.issues)
        self.summary.setText(f"{len(self.issues)} sorun — {e} hata, {w} uyarı")
        return self.issues
