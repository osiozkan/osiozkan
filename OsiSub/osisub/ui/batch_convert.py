"""Toplu dönüştürme (Subtitle Edit "Batch convert"): çok sayıda altyazıyı başka biçime çevirir.

Girdi: OsiSub'ın açabildiği her biçim (ASS/SSA, SRT, WebVTT, TTML/DFXP, SBV, MicroDVD).
Çıktı: core.formats.EXPORTERS. Kaynak dosyanın üstüne asla yazılmaz.
"""

from __future__ import annotations

import os

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QComboBox, QDialog, QDialogButtonBox, QDoubleSpinBox, QFileDialog, QFormLayout,
                               QHBoxLayout, QLabel, QLineEdit, QListWidget, QPlainTextEdit, QPushButton,
                               QVBoxLayout)

from ..core.formats import DEFAULT_FPS, EXPORTERS, IMPORT_EXT, IMPORT_FILTER, export_bytes, load_any


def output_path(src: str, key: str, out_dir: str | None) -> str:
    ext = EXPORTERS[key][1]
    base = os.path.splitext(os.path.basename(src))[0]
    d = out_dir or os.path.dirname(src)
    p = os.path.join(d, base + ext)
    if os.path.abspath(p) == os.path.abspath(src):
        p = os.path.join(d, base + "_donusum" + ext)
    return p


def convert_files(paths, key: str, out_dir: str | None = None, fps: float | None = None,
                  ttml_time: str = "clock", lang: str = "tr", log=print) -> tuple[int, int]:
    ok = bad = 0
    for src in paths:
        try:
            doc = load_any(src, fps)
            data = export_bytes(doc, key, fps=fps, lang=lang, ttml_time=ttml_time)
            dst = output_path(src, key, out_dir)
            if out_dir:
                os.makedirs(out_dir, exist_ok=True)
            with open(dst, "wb") as f:
                f.write(data)
            n = sum(1 for e in doc.events if not e.comment)
            log(f"TAMAM  {os.path.basename(src)} → {os.path.basename(dst)}  ({n} satır)")
            ok += 1
        except Exception as e:  # noqa: BLE001
            log(f"HATA   {os.path.basename(src)}: {e}")
            bad += 1
    return ok, bad


class BatchConvertDialog(QDialog):
    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx
        self.setWindowTitle("Toplu dönüştür")
        self.resize(720, 560)
        cfg = dict(ctx.config["batch_convert"] or {})
        self.files = QListWidget()
        self.files.setSelectionMode(QListWidget.ExtendedSelection)
        self.files.setAcceptDrops(True)
        b_add = QPushButton("Dosya ekle…")
        b_add.clicked.connect(self.add_dialog)
        b_rm = QPushButton("Seçilenleri çıkar")
        b_rm.clicked.connect(lambda: [self.files.takeItem(self.files.row(it)) for it in self.files.selectedItems()])
        b_clear = QPushButton("Temizle")
        b_clear.clicked.connect(self.files.clear)
        fh = QHBoxLayout()
        for b in (b_add, b_rm, b_clear):
            fh.addWidget(b)
        fh.addStretch(1)
        self.fmt = QComboBox()
        for k, (label, ext) in EXPORTERS.items():
            self.fmt.addItem(f"{label} ({ext})", k)
        self.fmt.setCurrentIndex(max(0, self.fmt.findData(cfg.get("format", "srt"))))
        self.out_dir = QLineEdit(cfg.get("out_dir", ""))
        self.out_dir.setPlaceholderText("boş = kaynak dosyanın klasörü")
        b_dir = QPushButton("…")
        b_dir.clicked.connect(self._pick_dir)
        dh = QHBoxLayout()
        dh.addWidget(self.out_dir, 1)
        dh.addWidget(b_dir)
        self.fps = QDoubleSpinBox()
        self.fps.setRange(1, 300)
        self.fps.setDecimals(3)
        self.fps.setValue(float(cfg.get("fps", round(DEFAULT_FPS, 3))))
        self.fps.setToolTip("Yalnız MicroDVD (.sub) okuma/yazma için")
        self.ttml_time = QComboBox()
        self.ttml_time.addItem("Saat (00:00:01.000)", "clock")
        self.ttml_time.addItem("Tik (10 MHz, Netflix)", "ticks")
        self.ttml_time.setCurrentIndex(max(0, self.ttml_time.findData(cfg.get("ttml_time", "clock"))))
        form = QFormLayout()
        form.addRow("Çıktı biçimi", self.fmt)
        form.addRow("Çıktı klasörü", dh)
        form.addRow("MicroDVD kare hızı", self.fps)
        form.addRow("TTML zaman biçimi", self.ttml_time)
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        bb = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Close)
        bb.button(QDialogButtonBox.Ok).setText("Dönüştür")
        bb.accepted.connect(self.run)
        bb.rejected.connect(self.reject)
        lay = QVBoxLayout(self)
        lay.addWidget(QLabel("Dosyalar (sürükle-bırak da olur)"))
        lay.addWidget(self.files, 1)
        lay.addLayout(fh)
        lay.addLayout(form)
        lay.addWidget(self.log, 1)
        lay.addWidget(bb)

    def add_paths(self, paths):
        have = {self.files.item(i).data(Qt.UserRole) for i in range(self.files.count())}
        for p in paths:
            if p.lower().endswith(IMPORT_EXT) and p not in have:
                have.add(p)
                self.files.addItem(os.path.basename(p))
                it = self.files.item(self.files.count() - 1)
                it.setData(Qt.UserRole, p)
                it.setToolTip(p)

    def add_dialog(self):
        paths, _ = QFileDialog.getOpenFileNames(self, "Altyazılar", "", IMPORT_FILTER)
        self.add_paths(paths)

    def _pick_dir(self):
        d = QFileDialog.getExistingDirectory(self, "Çıktı klasörü", self.out_dir.text())
        if d:
            self.out_dir.setText(d)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dropEvent(self, e):
        self.add_paths([u.toLocalFile() for u in e.mimeData().urls()])

    def paths(self):
        return [self.files.item(i).data(Qt.UserRole) for i in range(self.files.count())]

    def run(self):
        paths = self.paths()
        if not paths:
            self.log.appendPlainText("Dosya yok.")
            return (0, 0)
        cfg = {"format": self.fmt.currentData(), "out_dir": self.out_dir.text().strip(), "fps": self.fps.value(),
               "ttml_time": self.ttml_time.currentData()}
        self.ctx.config["batch_convert"] = cfg
        self.ctx.config.save()
        res = convert_files(paths, cfg["format"], cfg["out_dir"] or None, self.fps.value(), cfg["ttml_time"],
                            log=self.log.appendPlainText)
        self.log.appendPlainText(f"— {res[0]} dosya dönüştürüldü, {res[1]} hata")
        return res
