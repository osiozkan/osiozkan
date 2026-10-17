"""Aegisub araçlarının pencereleri: Satır seç, Yazı tipi toplayıcı, Çözünürlüğü yeniden örnekle, Üstüne yapıştır."""

from __future__ import annotations

import os

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor
from PySide6.QtWidgets import (QApplication, QButtonGroup, QCheckBox, QComboBox, QDialog, QDialogButtonBox,
                               QFileDialog, QFormLayout, QGridLayout, QHBoxLayout, QHeaderView, QLabel, QLineEdit,
                               QPushButton, QRadioButton, QSpinBox, QTreeWidget, QTreeWidgetItem, QVBoxLayout,
                               QWidget)

from ..core import aegisub_tools as A
from ..core import fonts as F
from . import theme


# ---------------------------------------------------------------------------
# Satır seç
# ---------------------------------------------------------------------------

class SelectLinesDialog(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Satır seç")
        cfg = dict(ctx.config["select_lines"] or {})
        self.pattern = QLineEdit(cfg.get("pattern", ""))
        self.pattern.setPlaceholderText("aranan metin")
        self.field = QComboBox()
        for k, label in A.SELECT_FIELDS.items():
            self.field.addItem(label, k)
        self.field.setCurrentIndex(max(0, self.field.findData(cfg.get("field", "text"))))
        self.mode = QComboBox()
        for k, label in A.SELECT_MODES.items():
            self.mode.addItem(label, k)
        self.mode.setCurrentIndex(max(0, self.mode.findData(cfg.get("mode", "contains"))))
        self.case = QCheckBox("Büyük/küçük harf duyarlı")
        self.case.setChecked(cfg.get("case", False))
        self.invert = QCheckBox("Eşleşmeyenleri seç")
        self.invert.setChecked(cfg.get("invert", False))
        self.dialogue = QCheckBox("Diyalog satırları")
        self.dialogue.setChecked(cfg.get("dialogue", True))
        self.comments = QCheckBox("Yorum satırları")
        self.comments.setChecked(cfg.get("comments", False))
        self.actions = QButtonGroup(self)
        ag = QGridLayout()
        cur = cfg.get("action", "set")
        for i, (k, label) in enumerate(A.SELECT_ACTIONS.items()):
            rb = QRadioButton(label)
            rb.setProperty("key", k)
            rb.setChecked(k == cur)
            self.actions.addButton(rb)
            ag.addWidget(rb, i // 2, i % 2)
        aw = QWidget()
        aw.setLayout(ag)
        form = QFormLayout()
        form.addRow("Bul", self.pattern)
        form.addRow("Alan", self.field)
        form.addRow("Eşleşme", self.mode)
        for w in (self.case, self.invert, self.dialogue, self.comments):
            form.addRow("", w)
        form.addRow("Seçim", aw)
        self.info = QLabel("")
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.button(QDialogButtonBox.Ok).setText("Seç")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(self.info)
        lay.addWidget(bb)

    def options(self) -> A.SelectOptions:
        b = self.actions.checkedButton()
        return A.SelectOptions(pattern=self.pattern.text(), field=self.field.currentData(),
                               mode=self.mode.currentData(), case=self.case.isChecked(),
                               invert=self.invert.isChecked(), dialogue=self.dialogue.isChecked(),
                               comments=self.comments.isChecked(), action=b.property("key") if b else "set")

    def apply(self):
        o = self.options()
        try:
            rows = A.select_rows(self.ctx.doc.events, set(self.ctx.selection), o)
        except Exception as e:  # noqa: BLE001  (regex hatası)
            self.info.setText(f"Hata: {e}")
            return None
        self.ctx.config["select_lines"] = {"pattern": o.pattern, "field": o.field, "mode": o.mode, "case": o.case,
                                           "invert": o.invert, "dialogue": o.dialogue, "comments": o.comments,
                                           "action": o.action}
        self.ctx.config.save()
        first = min(rows) if rows else None
        self.ctx.set_selection(rows, first)
        self.ctx.status.emit(f"{len(rows)} satır seçildi")
        self.accept()
        return rows


# ---------------------------------------------------------------------------
# Yazı tipi toplayıcı
# ---------------------------------------------------------------------------

def _style_label(bold, italic):
    return {(False, False): "Normal", (True, False): "Kalın", (False, True): "İtalik",
            (True, True): "Kalın İtalik"}[(bold, italic)]


class FontsDialog(QDialog):
    """Belgede kullanılan yazı tiplerini bulur; eksik dosya/karakter uyarır; klasöre ya da zip'e toplar."""

    def __init__(self, ctx, parent=None, extra_dirs=None):
        super().__init__(parent)
        self.ctx = ctx
        self.extra_dirs = list(extra_dirs or [])
        self.setWindowTitle("Yazı tipi toplayıcı")
        self.resize(900, 520)
        self.only_bad = QCheckBox("Yalnız sorunlular")
        self.only_bad.toggled.connect(self._fill)
        b_scan = QPushButton("Yeniden tara")
        b_scan.clicked.connect(lambda: self.scan(force=True))
        top = QHBoxLayout()
        self.summary = QLabel("")
        self.summary.setWordWrap(True)
        top.addWidget(self.summary, 1)
        top.addWidget(self.only_bad)
        top.addWidget(b_scan)
        self.tree = QTreeWidget()
        self.tree.setHeaderLabels(["Yazı tipi", "Biçem", "Satır", "Durum", "Dosya"])
        self.tree.setRootIsDecorated(False)
        hd = self.tree.header()
        hd.setSectionResizeMode(3, QHeaderView.Stretch)
        self.tree.setColumnWidth(0, 200)
        self.tree.setColumnWidth(1, 90)
        self.tree.setColumnWidth(2, 50)
        self.tree.setColumnWidth(4, 220)
        self.tree.itemDoubleClicked.connect(self._select_rows)
        b_copy = QPushButton("Klasöre kopyala…")
        b_copy.clicked.connect(lambda: self.copy_to())
        b_zip = QPushButton("Zip olarak kaydet…")
        b_zip.clicked.connect(lambda: self.save_zip())
        bb = QDialogButtonBox(QDialogButtonBox.Close)
        bb.rejected.connect(self.reject)
        bh = QHBoxLayout()
        bh.addWidget(QLabel("Çift tık: o yazı tipini kullanan satırları seç"))
        bh.addStretch(1)
        bh.addWidget(b_copy)
        bh.addWidget(b_zip)
        bh.addWidget(bb)
        lay = QVBoxLayout(self)
        lay.addLayout(top)
        lay.addWidget(self.tree, 1)
        lay.addLayout(bh)
        self.reports: list[F.FontReport] = []
        self.scan()

    def _index(self, force=False):
        from ..media.cache import cache_root
        deep, shallow = F.doc_font_dirs(self.ctx.doc)
        cache = os.path.join(cache_root(), "fonts.json")
        if force and os.path.exists(cache):
            os.remove(cache)
        return F.FontIndex(F.system_font_dirs() + deep + self.extra_dirs, cache, shallow=shallow)

    def scan(self, force=False):
        QApplication.setOverrideCursor(Qt.WaitCursor)
        try:
            self.index = self._index(force)
            self.reports = F.collect(self.ctx.doc, self.index)
        finally:
            QApplication.restoreOverrideCursor()
        self._fill()
        return self.reports

    def _fill(self, *_):
        c = theme.colors()
        self.tree.clear()
        missing_file = sum(1 for r in self.reports if r.face is None)
        missing_chars = sum(1 for r in self.reports if r.missing)
        for r in self.reports:
            bad = r.face is None or bool(r.missing) or bool(r.warnings)
            if self.only_bad.isChecked() and not bad:
                continue
            if r.face is None:
                status = "BULUNAMADI"
            elif r.missing:
                status = "Eksik karakter: " + " ".join(r.missing[:30]) + (" …" if len(r.missing) > 30 else "")
            else:
                status = "tamam"
            if r.warnings:
                status += " · " + ", ".join(r.warnings)
            it = QTreeWidgetItem([r.use.name, _style_label(r.use.bold, r.use.italic), str(len(r.use.rows)), status,
                                  os.path.basename(r.face.path) if r.face else ""])
            it.setToolTip(4, r.face.path if r.face else "")
            it.setToolTip(3, status)
            it.setData(0, Qt.UserRole, sorted(r.use.rows))
            if r.face is None or r.missing:
                it.setForeground(3, QColor(c["err_fg"]))
            elif r.warnings:
                it.setForeground(3, QColor(c["warn_fg"]))
            else:
                it.setForeground(3, QColor(c["ok_fg"]))
            self.tree.addTopLevelItem(it)
        files = len({r.face.path for r in self.reports if r.face})
        self.summary.setText(f"{len(self.reports)} yazı tipi kullanımı · {files} dosya · {missing_file} bulunamadı · "
                             f"{missing_chars} yazı tipinde eksik karakter (ör. Türkçe ğ ş ı İ)")

    def _select_rows(self, item, _col=0):
        rows = item.data(0, Qt.UserRole) or []
        if rows:
            self.ctx.set_selection(rows, rows[0])

    def copy_to(self, dest: str | None = None):
        if dest is None:
            base = os.path.dirname(self.ctx.doc.path) if self.ctx.doc.path else ""
            dest = QFileDialog.getExistingDirectory(self, "Yazı tiplerini kopyala", base)
            if not dest:
                return []
        done = F.copy_fonts(self.reports, dest)
        self.ctx.status.emit(f"{len(done)} yazı tipi dosyası kopyalandı: {dest}")
        return done

    def save_zip(self, path: str | None = None):
        if path is None:
            base = os.path.splitext(self.ctx.doc.path or "yazitipleri")[0]
            path, _ = QFileDialog.getSaveFileName(self, "Zip kaydet", base + "_fonts.zip", "Zip (*.zip)")
            if not path:
                return []
        done = F.zip_fonts(self.reports, path)
        self.ctx.status.emit(f"{len(done)} yazı tipi zip'e yazıldı: {path}")
        return done

    def font_paths(self) -> list[str]:
        return sorted({r.face.path for r in self.reports if r.face is not None})


# ---------------------------------------------------------------------------
# Çözünürlüğü yeniden örnekle
# ---------------------------------------------------------------------------

class ResampleDialog(QDialog):
    PRESETS = [("640×360", 640, 360), ("640×480", 640, 480), ("1280×720", 1280, 720), ("1920×1080", 1920, 1080),
               ("2560×1440", 2560, 1440), ("3840×2160", 3840, 2160)]

    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Çözünürlüğü yeniden örnekle")
        sw, sh = ctx.doc.play_res()
        self.src = QLabel(f"{sw} × {sh}")
        self.w, self.h = QSpinBox(), QSpinBox()
        for sb in (self.w, self.h):
            sb.setRange(16, 20000)
        self.w.setValue(1920 if sw != 1920 else 1280)
        self.h.setValue(1080 if sh != 1080 else 720)
        self.preset = QComboBox()
        self.preset.addItem("— hazır —", None)
        for label, w, h in self.PRESETS:
            self.preset.addItem(label, (w, h))
        dims = getattr(ctx, "video_dims", None)
        if dims:
            self.preset.addItem(f"Video ({dims[0]}×{dims[1]})", (dims[0], dims[1]))
        self.preset.activated.connect(self._preset)
        self.margins = QCheckBox("Kenar boşluklarını ölçekle")
        self.margins.setChecked(True)
        self.sizes = QCheckBox("Yazı boyu, kenarlık, gölge, bulanıklığı ölçekle")
        self.sizes.setChecked(True)
        self.drawings = QCheckBox("Vektör çizimleri ölçekle")
        self.drawings.setChecked(True)
        wh = QHBoxLayout()
        wh.addWidget(self.w)
        wh.addWidget(QLabel("×"))
        wh.addWidget(self.h)
        wh.addWidget(self.preset)
        form = QFormLayout()
        form.addRow("Şimdiki (PlayRes)", self.src)
        form.addRow("Hedef", wh)
        for cb in (self.margins, self.sizes, self.drawings):
            form.addRow("", cb)
        self.note = QLabel("Konumlar (\\pos, \\move, \\org, \\clip) ve çizimler x/y oranıyla; yazı boyu ve kenarlık "
                           "dikey oranla ölçeklenir. En/boy oranı değişirse metin yatayda esnemez.")
        self.note.setWordWrap(True)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.button(QDialogButtonBox.Ok).setText("Yeniden örnekle")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addLayout(form)
        lay.addWidget(self.note)
        lay.addWidget(bb)

    def _preset(self, i):
        v = self.preset.itemData(i)
        if v:
            self.w.setValue(v[0])
            self.h.setValue(v[1])

    def apply(self):
        o = A.ResampleOptions(dst_w=self.w.value(), dst_h=self.h.value(), margins=self.margins.isChecked(),
                              sizes=self.sizes.isChecked(), drawings=self.drawings.isChecked())
        doc = self.ctx.doc
        styles, events, rx, ry = A.resample_doc(doc, o)
        doc.ensure_section("styles").items = styles
        doc.ensure_section("events").items = events
        doc.set_info("PlayResX", str(o.dst_w))
        doc.set_info("PlayResY", str(o.dst_h))
        for k, v in (("LayoutResX", o.dst_w), ("LayoutResY", o.dst_h)):
            if doc.get_info(k) is not None:
                doc.set_info(k, str(v))
        self.ctx.commit_external(f"Yeniden örnekle ({o.dst_w}×{o.dst_h})", selection=self.ctx.selection,
                                 active=self.ctx.active)
        self.ctx.status.emit(f"Yeniden örneklendi: ×{rx:.3f} / ×{ry:.3f}")
        self.accept()
        return rx, ry


# ---------------------------------------------------------------------------
# Üstüne yapıştır
# ---------------------------------------------------------------------------

class PasteOverDialog(QDialog):
    def __init__(self, ctx, text: str, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Üstüne yapıştır")
        fmt = ctx.doc.ensure_section("events").format_fields
        self.pasted = A.parse_clipboard_lines(text, fmt)
        lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
        self.full = bool(lines) and all(ln.lower().startswith(("dialogue:", "comment:")) for ln in lines)
        rows = ctx.selected_rows()
        self.info = QLabel(f"Panoda {len(self.pasted)} satır · seçili {len(rows)} satır — sırayla eşlenir"
                           + ("" if self.full else " (düz metin: yalnız Metin yazılabilir)"))
        self.info.setWordWrap(True)
        self.cbs = {}
        grid = QGridLayout()
        saved = set((ctx.config["paste_over"] or ["text"]))
        for i, (k, label) in enumerate(A.PASTE_FIELDS.items()):
            cb = QCheckBox(label)
            cb.setChecked(k in saved if self.full else k == "text")
            cb.setEnabled(self.full or k == "text")
            self.cbs[k] = cb
            grid.addWidget(cb, i // 3, i % 3)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        bb.button(QDialogButtonBox.Ok).setText("Yapıştır")
        bb.accepted.connect(self.apply)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addWidget(self.info)
        lay.addLayout(grid)
        lay.addWidget(bb)

    def fields(self) -> set[str]:
        return {k for k, cb in self.cbs.items() if cb.isChecked() and cb.isEnabled()}

    def apply(self):
        f = self.fields()
        if self.full:
            self.ctx.config["paste_over"] = sorted(f)
            self.ctx.config.save()
        ch = A.paste_over(self.ctx.doc.events, self.ctx.selected_rows(), self.pasted, f)
        n = len(self.ctx.update_events(ch, "Üstüne yapıştır")) if ch else 0
        self.ctx.status.emit(f"{n} satırın üstüne yapıştırıldı")
        self.accept()
        return n
